// @vitest-environment happy-dom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { createMockService } from "../../ipc/mock";
import { createSession } from "../../state/session";
import { SessionProvider } from "../../state/react";
import { useProjectScreen } from "./useProjectScreen";

afterEach(cleanup);
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
    act(() => hook.result.current.actions.startTutorial()); act(() => hook.result.current.actions.discardAndContinue());
    await waitFor(() => expect(session.stores.project.getSnapshot()?.tutorial).toBe(true));
    expect(mock.calls.some((r) => r.method === "project_new" && r.params.sample && r.params.tutorial)).toBe(true);
  } finally { hook.unmount(); session.dispose(); mock.dispose(); }
});
