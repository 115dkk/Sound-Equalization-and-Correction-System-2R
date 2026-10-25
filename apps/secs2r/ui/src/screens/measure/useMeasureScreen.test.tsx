// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { createMockService } from "../../ipc/mock";
import { SessionProvider } from "../../state/react";
import { createSession } from "../../state/session";
import { useMeasureScreen } from "./useMeasureScreen";
import { measureStepState } from "./types";
import type { JobKind } from "../../ipc/bindings/JobKind";

let mock: ReturnType<typeof createMockService>;
let session: ReturnType<typeof createSession>;
const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
async function perform(work: () => unknown) {
  await act(async () => { await work(); await session.settled(); });
}
beforeEach(async () => {
  mock = createMockService({ tickMs: 0 }); session = createSession(mock); await session.start();
  mock.calls.length = 0;
});
afterEach(async () => { cleanup(); await session.monitor(false); session.dispose(); mock.dispose(); vi.restoreAllMocks(); });

describe("useMeasureScreen", () => {
  it("ignores passive watches but blocks finite device checks and active monitor playback", async () => {
    await session.configure({ method: "mock" });
    const { result } = renderHook(useMeasureScreen, { wrapper });
    let job = 99;
    for (const kind of ["level_monitor", "status_watch", "calibrate", "recommend_xo", "mimo", "export", "level_check", "measure", "sample_load", "mdat_import"] as JobKind[]) {
      await perform(() => mock.events.emit({ event: "started", job, kind }));
      const view = result.current.view!;
      const expected = kind === "level_check" ? { status: "blocked", reason: "device_busy" } : { status: "ready" };
      expect(measureStepState(view)).toEqual(expected);
      // The state function also accepts passive snapshots independently of hook filtering.
      expect(measureStepState({ ...view, job: { job, kind, status: "cancel_requested", progress: null, position: null } })).toEqual(expected);
      await perform(() => mock.events.emit({ event: "finished", job, status: "cancelled", result: null, error: null }));
      job++;
    }
    await perform(() => session.host.set({ playing: "L" }));
    expect(measureStepState(result.current.view!)).toEqual({ status: "blocked", reason: "device_busy" });
    await perform(() => session.host.set({ playing: null, playingAll: true }));
    expect(measureStepState(result.current.view!).reason).toBe("device_busy");
    await perform(() => session.host.set({ playingAll: false }));
    expect(measureStepState(result.current.view!).status).toBe("ready");
  });

  it("has no manufactured settings, and keeps host-only choices", async () => {
    // Before the session has read the stored settings the hook invents none.
    session.stores.measurementSettings.set(null);
    const { result } = renderHook(useMeasureScreen, { wrapper });
    expect(result.current.view).toBeNull();
    await perform(() => session.configure({ method: "mdat" }));
    expect(result.current.view?.path).toBe("import");
    expect(result.current.view?.repetitions).toBe(session.stores.measurementSettings.getSnapshot()?.repetitions);
    mock.calls.length = 0;
    await perform(() => {
      result.current.actions.selectPosition(3);
      result.current.actions.setGraphHidden(true);
      result.current.actions.setPlotRange({ x: [20, 200], y: [-20, 10] });
      result.current.actions.closeMdat();
      result.current.actions.openCalibrate();
    });
    expect(result.current.view).toMatchObject({ selected: 3, graphHidden: true, plotRange: { x: [20, 200], y: [-20, 10] }, mdat: null });
    expect(session.host.getSnapshot().current).toBe("calibrate");
    expect(mock.calls).toEqual([]);
  });

  it("requires a successful save before measurement and clears selection only at finish", async () => {
    await session.configure({ method: "mock" });
    session.stores.project.set({ ...session.stores.project.getSnapshot()!, path: "" });
    const { result } = renderHook(useMeasureScreen, { wrapper });
    await perform(() => result.current.actions.selectPosition(2));
    mock.queueDialog("dialog_save_project", null);
    await perform(() => result.current.actions.measurePosition(2));
    expect(mock.calls.some((call) => call.method === "measure_position")).toBe(false);
    expect(result.current.view?.selected).toBe(2);
    mock.queueDialog("dialog_save_project", "/room.secs");
    await perform(() => result.current.actions.measurePosition(2));
    expect(mock.calls).toContainEqual({ method: "project_save", params: { path: "/room.secs" } });
    expect(mock.calls).toContainEqual({ method: "measure_position", params: { position: 2 } });
    expect(result.current.view?.job).toMatchObject({ kind: "measure", position: 2 });
    expect(result.current.view?.selected).toBe(2);
    const job = result.current.view!.job!.job;
    await perform(() => result.current.actions.cancelJob(job));
    expect(mock.calls).toContainEqual({ method: "job_cancel", params: { job } });
    await perform(mock.drain);
    expect(result.current.view?.selected).toBeNull();
    expect(result.current.view?.job).toBeNull();
  });

  it("wires settings, destructive clear and undo, and demo loading", async () => {
    await session.configure({ method: "mock" });
    const { result } = renderHook(useMeasureScreen, { wrapper });
    const a = result.current.actions;
    await perform(() => a.setStartDelay("5 초"));
    expect(mock.calls).toContainEqual({ method: "layout_set", params: { count: null, start_delay: "5 초", timing: null, confirm: false } });
    await perform(() => a.setSweepLength(65536));
    await perform(() => a.setRepetitions(4));
    expect(mock.calls).toContainEqual({ method: "measure_configure", params: expect.objectContaining({ method: "mock", sweep_length: 65536, repetitions: null, output_channels: [] }) });
    expect(mock.calls).toContainEqual({ method: "measure_configure", params: expect.objectContaining({ method: "mock", repetitions: 4, sweep_length: null }) });
    const token = await act(() => a.clearPosition(1));
    expect(token).toEqual(expect.any(String));
    expect(mock.calls).toContainEqual({ method: "position_clear", params: { position: 1 } });
    await perform(() => a.undo(token));
    expect(mock.calls).toContainEqual({ method: "undo", params: { token } });
    await perform(a.loadSample);
    expect(mock.calls).toContainEqual({ method: "sample_load", params: { positions: null } });
  });

  it("scans only picked files, closes only accepted imports and handles cancellation", async () => {
    await session.configure({ method: "mdat" });
    const { result } = renderHook(useMeasureScreen, { wrapper });
    const a = result.current.actions;
    mock.queueDialog("dialog_pick_mdat", null);
    await perform(() => a.pickMdat(null));
    expect(mock.calls.some((call) => call.method === "mdat_scan")).toBe(false);
    mock.queueDialog("dialog_pick_mdat", "/room.mdat");
    await perform(() => a.pickMdat(2));
    expect(mock.calls).toContainEqual({ method: "mdat_scan", params: { path: "/room.mdat" } });
    expect(result.current.view?.mdat).toMatchObject({ path: "/room.mdat", position: 2 });
    const params = { path: "/room.mdat", position: 2, assignments: result.current.view!.mdat!.scan.suggestions };
    mock.failNext("mdat_import", "invalid_request");
    await perform(() => a.importMdat(params));
    expect(result.current.view?.mdat).not.toBeNull();
    await perform(() => a.importMdat(params));
    expect(mock.calls).toContainEqual({ method: "mdat_import", params });
    expect(result.current.view?.mdat).toBeNull();
    await perform(mock.drain);
    mock.failNext("position_clear", "invalid_request");
    await act(async () => { await expect(a.clearPosition(2)).rejects.toThrow(); });
    expect(session.errors.getSnapshot()?.error.code).toBe("invalid_request");
  });
});
