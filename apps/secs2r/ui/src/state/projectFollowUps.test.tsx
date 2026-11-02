// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { createMockService } from "../ipc/mock";
import { useMeasureScreen } from "../screens/measure/useMeasureScreen";
import { useCalibrateScreen } from "../screens/calibrate/useCalibrateScreen";
import { useExportScreen } from "../screens/export/useExportScreen";
import { SessionProvider } from "./react";
import { createSession } from "./session";

afterEach(() => { cleanup(); vi.useRealTimers(); });

it.each(["dialog_pick_mdat", "dialog_open_file", "dialog_save_file", "dialog_pick_folder"] as const)("drops %s started after the replacement notification but before refresh", async (method) => {
  vi.useFakeTimers();
  const mock = createMockService({ tickMs: 0 });
  let emit!: (method: string) => void;
  const session = createSession({ ...mock, externalChanges: { subscribe: async (next) => { emit = next; return () => {}; } } });
  await session.start(); await session.settled();
  const hook = renderHook(() => ({ measure: useMeasureScreen(), calibrate: useCalibrateScreen(), exporter: useExportScreen() }), {
    wrapper: ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>,
  });
  const choose = () => {
    mock.queueDialog(method, "/chosen");
    if (method === "dialog_pick_mdat") hook.result.current.measure.actions.pickMdat(2);
    else if (method === "dialog_open_file") hook.result.current.calibrate.actions.loadTarget();
    else if (method === "dialog_save_file") hook.result.current.calibrate.actions.saveTarget();
    else hook.result.current.exporter.actions.changeFolder();
  };
  const followUps = () => mock.calls.filter((r) => ["mdat_scan", "target_load", "target_save", "export_folder_set"].includes(r.method));
  try {
    const before = session.stores.project.getSnapshot();
    await act(async () => {
      await mock.request({ method: "project_new", params: { path: "/B.secs", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
      emit("project_new");
    });
    expect(session.stores.project.getSnapshot()).toBe(before);
    const pendingGuard = session.projectGuard();
    mock.calls.length = 0;
    await act(async () => { choose(); await session.settled(); });
    expect(followUps()).toEqual([]);
    expect(session.host.getSnapshot().mdat).toBeNull();
    expect(pendingGuard.alive()).toBe(false);
    expect(await session.projectEditsSettled()).toBe(false);
    await act(async () => { await vi.advanceTimersByTimeAsync(25); await session.settled(); });
    expect(session.stores.project.getSnapshot()?.path).toBe("/B.secs");
    expect(pendingGuard.alive()).toBe(false);
    expect(session.projectGuard().alive()).toBe(true);
    mock.calls.length = 0;
    await act(async () => { choose(); await session.settled(); });
    expect(followUps()).toHaveLength(1);
  } finally { hook.unmount(); session.dispose(); await session.settled(); mock.dispose(); }
});

it.each((["mdat_scan", "dialog_pick_mdat", "dialog_open_file", "dialog_save_file", "dialog_pick_folder"] as const)
  .flatMap((method) => [0, 25].map((delay) => ({ method, delay }))))("drops $method follow-up $delay ms after an external project switch", async ({ method, delay }) => {
  vi.useFakeTimers();
  const mock = createMockService({ tickMs: 0 });
  let release!: () => void;
  let entered!: () => void;
  const delayed = new Promise<void>((resolve) => { release = resolve; });
  const reached = new Promise<void>((resolve) => { entered = resolve; });
  let emit!: (method: string) => void;
  const session = createSession({ ...mock, externalChanges: { subscribe: async (next) => { emit = next; return () => {}; } }, request: async (request) => {
    const response = await mock.request(request);
    if (request.method === method) { entered(); await delayed; }
    return response;
  } });
  await session.start(); await session.settled();
  const hook = renderHook(() => ({ measure: useMeasureScreen(), calibrate: useCalibrateScreen(), exporter: useExportScreen() }), {
    wrapper: ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>,
  });
  try {
    mock.queueDialog(method === "mdat_scan" ? "dialog_pick_mdat" : method, "/chosen");
    await act(async () => {
      if (method === "mdat_scan" || method === "dialog_pick_mdat") hook.result.current.measure.actions.pickMdat(2);
      else if (method === "dialog_open_file") hook.result.current.calibrate.actions.loadTarget();
      else if (method === "dialog_save_file") hook.result.current.calibrate.actions.saveTarget();
      else hook.result.current.exporter.actions.changeFolder();
      await reached;
    });
    await act(async () => {
      await mock.request({ method: "project_new", params: { path: "/B.secs", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
      emit("project_new");
      if (delay) await vi.advanceTimersByTimeAsync(delay);
    });
    if (delay) expect(session.stores.project.getSnapshot()?.path).toBe("/B.secs");
    mock.calls.length = 0;
    await act(async () => { release(); await session.settled(); });
    expect(session.host.getSnapshot().mdat).toBeNull();
    expect(mock.calls.filter((r) => ["mdat_scan", "target_load", "target_save", "export_folder_set"].includes(r.method))).toEqual([]);
  } finally { release(); hook.unmount(); session.dispose(); await session.settled(); mock.dispose(); }
});
