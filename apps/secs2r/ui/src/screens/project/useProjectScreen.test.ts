// @vitest-environment happy-dom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { createMockService } from "../../ipc/mock";
import { resultsSummary } from "../../ipc/fixtures";
import { createSession } from "../../state/session";
import { SessionProvider } from "../../state/react";
import { useProjectScreen } from "./useProjectScreen";

afterEach(cleanup);
it.each(["cancel", "unmount"] as const)("pauses automatic refresh for the unsaved dialog and resumes on %s", async (end) => {
  vi.useFakeTimers();
  const mock = createMockService({ tickMs: 0, results: resultsSummary });
  const session = createSession(mock);
  await session.start(); await session.settled();
  const close = vi.fn(async () => {});
  const hook = renderHook(() => useProjectScreen(close), { wrapper: ({ children }: { children: ReactNode }) => createElement(SessionProvider, { value: session, children }) });
  const starts = () => mock.calls.filter((request) => request.method === "recommend_xo" || request.method === "calibrate");
  try {
    await act(async () => {
      await session.writers.app.edit({ auto_refresh: true });
      await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "boost_max", db: 4 } } });
      await session.settled();
    });
    act(() => hook.result.current.requestClose());
    expect(hook.result.current.view.dialog).toMatchObject({ kind: "unsaved", reason: "close" });
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); await session.settled(); });
    expect(starts()).toEqual([]);
    expect(close).not.toHaveBeenCalled();
    if (end === "cancel") {
      act(() => hook.result.current.actions.cancel());
      expect(hook.result.current.view.dialog).toBeNull();
    } else hook.unmount();
    await act(async () => { await vi.advanceTimersByTimeAsync(0); await session.settled(); });
    expect(starts()).toEqual([{ method: "recommend_xo", params: { background: true } }]);
    expect(close).not.toHaveBeenCalled();
  } finally {
    hook.unmount(); session.dispose(); await session.settled(); mock.dispose(); vi.useRealTimers();
  }
});

it.each(["open", "close"] as const)("asks about unsaved changes on %s while an edit response is pending", async (action) => {
  const mock = createMockService({ tickMs: 0 });
  let release!: () => void;
  const delayed = new Promise<void>((resolve) => { release = resolve; });
  const session = createSession({ ...mock, request: async (request) => {
    const response = await mock.request(request);
    if (request.method === "target_set_points") await delayed;
    return response;
  } });
  await session.start();
  await session.call({ method: "project_save", params: { path: "C:/room.secs" } });
  const close = vi.fn(async () => {});
  const hook = renderHook(() => useProjectScreen(close), { wrapper: ({ children }: { children: ReactNode }) => createElement(SessionProvider, { value: session, children }) });
  const edit = session.call({ method: "target_set_points", params: { points: [[20, 3], [20000, -3]] } });
  try {
    expect(session.stores.project.getSnapshot()?.dirty).toBe(false);
    act(() => action === "open" ? hook.result.current.actions.openRecent("C:/other.secs") : hook.result.current.requestClose());
    expect(hook.result.current.view.dialog).toMatchObject({ kind: "unsaved", reason: action });
    expect(mock.calls.some((r) => r.method === "project_open")).toBe(false);
    expect(close).not.toHaveBeenCalled();
    await act(async () => { release(); await edit; });
  } finally { release(); await edit; hook.unmount(); session.dispose(); mock.dispose(); }
});
it.each(["open", "close"] as const)("waits for every queued edit before save then %s and stops on failure", async (action) => {
  for (const fails of [false, true]) {
    const mock = createMockService({ tickMs: 0 });
    let release!: () => void;
    const delayed = new Promise<void>((resolve) => { release = resolve; });
    let edits = 0;
    const session = createSession({ ...mock, request: async (request) => {
      if (request.method === "dsp_set" && ++edits === 1) {
        await delayed;
        if (fails) throw new Error("edit refused");
      }
      return mock.request(request);
    } });
    await session.start(); await session.settled();
    const close = vi.fn(async () => {});
    const hook = renderHook(() => useProjectScreen(close), { wrapper: ({ children }: { children: ReactNode }) => createElement(SessionProvider, { value: session, children }) });
    const first = session.writers.dsp.edit({ fir_taps: 4096 });
    const second = session.writers.dsp.edit({ fir_budget: 12345 });
    try {
      act(() => action === "open" ? hook.result.current.actions.openRecent("C:/other.secs") : hook.result.current.requestClose());
      act(() => hook.result.current.actions.saveAndContinue());
      await act(async () => { await Promise.resolve(); await Promise.resolve(); });
      expect(mock.calls.filter((r) => r.method === "project_save")).toHaveLength(0);
      await act(async () => { release(); await Promise.all([first, second]); await session.settled(); });
      await waitFor(() => expect(hook.result.current.view.busy).toBe(false));
      expect(mock.calls.filter((r) => r.method === "project_save")).toHaveLength(fails ? 0 : 1);
      expect(mock.calls.filter((r) => r.method === "project_open")).toHaveLength(!fails && action === "open" ? 1 : 0);
      expect(close).toHaveBeenCalledTimes(!fails && action === "close" ? 1 : 0);
      if (!fails) {
        const methods = mock.calls.map((r) => r.method);
        expect(methods.lastIndexOf("dsp_set")).toBeLessThan(methods.indexOf("project_save"));
      }
    } finally { release(); await Promise.all([first, second]); hook.unmount(); session.dispose(); await session.settled(); mock.dispose(); }
  }
});

it("keeps jobs on cancelled close and saves before an approved close", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  await session.start();
  const close = vi.fn(async () => {});
  const hook = renderHook(() => useProjectScreen(close), { wrapper: ({ children }: { children: ReactNode }) => createElement(SessionProvider, { value: session, children }) });
  try {
    await act(async () => { await session.call({ method: "calibrate", params: { settings: null, subs_only: false } }); });
    act(() => hook.result.current.requestClose());
    expect(hook.result.current.view.dialog?.kind).toBe("unsaved");
    act(() => hook.result.current.actions.cancel());
    expect(close).not.toHaveBeenCalled();
    expect(mock.calls.some((r) => r.method === "job_cancel" || r.method === "status_watch_stop")).toBe(false);
    act(() => hook.result.current.requestClose());
    act(() => hook.result.current.actions.saveAndContinue());
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    expect(mock.calls.some((r) => r.method === "project_save")).toBe(true);
  } finally { hook.unmount(); session.dispose(); mock.dispose(); }
});

it("does not close when save picker is cancelled and starts tutorial with both flags", async () => {
  const mock = createMockService({ project: null, tickMs: 0 });
  const session = createSession(mock); await session.start();
  await session.call({ method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
  // One edit, so closing asks first.
  await session.configure({ method: "mock" });
  const close = vi.fn(async () => {});
  const hook = renderHook(() => useProjectScreen(close), { wrapper: ({ children }: { children: ReactNode }) => createElement(SessionProvider, { value: session, children }) });
  try {
    mock.queueDialog("dialog_save_project", null);
    act(() => hook.result.current.requestClose()); act(() => hook.result.current.actions.saveAndContinue());
    await waitFor(() => expect(hook.result.current.view.busy).toBe(false));
    expect(close).not.toHaveBeenCalled();
    expect(hook.result.current.view.dialog?.kind).toBe("unsaved");
    act(() => hook.result.current.actions.cancel());
    mock.calls.length = 0;
    act(() => hook.result.current.offerTutorial());
    expect(hook.result.current.view.dialog?.kind).toBe("tutorial");
    act(() => hook.result.current.actions.startTutorial()); act(() => hook.result.current.actions.discardAndContinue());
    await act(async () => { await session.settled(); });
    await waitFor(() => expect(hook.result.current.view.dialog).toBeNull());
    expect(session.stores.project.getSnapshot()?.tutorial).toBe(true);
    expect(session.tutorialReady.getSnapshot()).toBe(true);
    expect(mock.calls.filter((r) => r.method === "project_new")).toEqual([
      { method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } },
    ]);
    expect(mock.calls.filter((r) => r.method === "measure_configure")).toEqual([
      { method: "measure_configure", params: expect.objectContaining({ method: "none" }) },
    ]);
    expect(session.stores.config.getSnapshot()?.speakers.find((speaker) => speaker.role === "SW3")?.bands[0]?.out).toBe(16);
  } finally { hook.unmount(); session.dispose(); mock.dispose(); }
});
