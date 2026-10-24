// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { gpuComputeStatus, resultsSummary } from "../../ipc/fixtures";
import { createMockService } from "../../ipc/mock";
import { SessionProvider } from "../../state/react";
import { createSession } from "../../state/session";
import { useCalibrateScreen } from "./useCalibrateScreen";

let mock: ReturnType<typeof createMockService>;
let session: ReturnType<typeof createSession>;
let pending: Promise<unknown>[];
const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
async function perform(work: () => unknown) {
  await act(async () => { await work(); while (pending.length) await Promise.all(pending.splice(0)); });
}
beforeEach(async () => {
  mock = createMockService({ tickMs: 0 }); session = createSession(mock); await session.start();
  pending = []; vi.spyOn(session, "run").mockImplementation((work) => { pending.push(work().catch(() => {})); });
  mock.calls.length = 0;
});
afterEach(async () => { cleanup(); await session.monitor(false); session.dispose(); mock.dispose(); vi.restoreAllMocks(); });

describe("useCalibrateScreen", () => {
  it("returns failed target and XO edits and exposes saving, failure and retry state", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    const points: [number, number][] = [[20, 4], [20000, -2]];
    mock.failNext("target_set_points", "project_io");
    let accepted: unknown;
    await act(async () => { accepted = await result.current.actions.setTargetPoints(points); });
    expect(accepted).toBe(false);
    expect(result.current.view).toMatchObject({ targetEdit: "failed", xoEdit: "idle" });
    const states: string[] = [];
    const off = session.host.subscribe(() => states.push(session.host.getSnapshot().targetEdit));
    await act(async () => { accepted = await result.current.actions.setTargetPoints(points); });
    off();
    expect(accepted).toBe(true);
    expect(states).toContain("saving");
    expect(result.current.view).toMatchObject({ targetEdit: "idle", target: { points } });
    mock.failNext("calibration_settings_edit", "project_io");
    await act(async () => { accepted = await result.current.actions.changeSetting({ kind: "xo", group: "프론트", hz: 90 }); });
    expect(accepted).toBe(false);
    expect(result.current.view?.xoEdit).toBe("failed");
    await act(async () => { accepted = await result.current.actions.changeSetting({ kind: "xo", group: "프론트", hz: 90 }); });
    expect(accepted).toBe(true);
    expect(result.current.view?.xoEdit).toBe("idle");
  });
  it("loads automatic GPU choice, persists manual CPU and refreshes stale provenance", async () => {
    session.dispose(); mock.dispose();
    mock = createMockService({ tickMs: 0, computeStatus: gpuComputeStatus, results: resultsSummary });
    session = createSession(mock); await session.start();
    vi.spyOn(session, "run").mockImplementation((work) => { pending.push(work().catch(() => {})); });
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    expect(result.current.view?.compute).toMatchObject({ preference: "auto", status: { selected: "cuda" }, loading: false });
    const prior = session.stores.appSettings.getSnapshot()!;
    await perform(() => result.current.actions.changeCompute("cpu"));
    expect(mock.calls).toContainEqual({ method: "app_settings_set", params: { settings: { ...prior, compute: "cpu" } } });
    expect(result.current.view?.compute).toMatchObject({ preference: "cpu", status: { selected: "cpu", reason: "user_cpu" }, saving: false });
    expect(result.current.view?.results.mimo?.stale).toBe(true);
    await perform(result.current.actions.refreshCompute);
    expect(result.current.view?.compute.preference).toBe("cpu");
    await perform(result.current.actions.openGpuHelp);
    expect(mock.calls).toContainEqual({ method: "open_gpu_help" });
  });

  it("retains a preference after save failure and blocks changes during jobs", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    mock.failNext("app_settings_set", "project_io");
    await perform(() => result.current.actions.changeCompute("cpu"));
    expect(result.current.view?.compute).toMatchObject({ preference: "auto", saving: false });
    expect(session.errors.getSnapshot()?.error.code).toBe("project_io");
    await perform(result.current.actions.computeMimo);
    mock.calls.length = 0;
    await perform(() => result.current.actions.changeCompute("cpu"));
    expect(mock.calls.some((request) => request.method === "app_settings_set")).toBe(false);
  });

  it("exposes a failed device probe and recovers on retry", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    mock.failNext("compute_status", "internal", "probe unavailable");
    await perform(result.current.actions.refreshCompute);
    expect(result.current.view?.compute).toMatchObject({ loading: false, error: { code: "internal" } });
    await perform(result.current.actions.refreshCompute);
    expect(result.current.view?.compute).toMatchObject({ loading: false, error: null, status: { selected: "cpu" } });
  });

  it("keeps the preview channel list empty without position-one measurements", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    await perform(() => session.call({ method: "project_new", params: { path: "", preset: "2.1", sub_count: 1, sample: false, tutorial: false } }));
    expect(result.current.view).toMatchObject({ positions: [], measuredRoles: [], channel: null });
    await perform(() => session.configure({ method: "mock" }));
    await perform(() => session.call({ method: "measure_position", params: { position: 2 } }));
    await perform(mock.drain);
    expect(result.current.view?.measuredRoles).toEqual([]);
    await perform(() => session.call({ method: "measure_position", params: { position: 1 } }));
    await perform(mock.drain);
    expect(result.current.view?.measuredRoles).toEqual(session.stores.config.getSnapshot()!.speakers.map((speaker) => speaker.role));
  });

  it("uses service settings, warnings and result metadata without manufacturing them", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    expect(result.current.view?.settings).toBe(session.stores.settings.getSnapshot());
    expect(result.current.view?.subWarnings).toEqual(session.stores.subWarnings.getSnapshot());
    await perform(() => session.stores.settings.set(null));
    expect(result.current.view).toBeNull();
  });

  it("wires all compute actions and shows failure and running state", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    const a = result.current.actions;
    await perform(a.calibrateSubs);
    expect(mock.calls).toContainEqual({ method: "calibrate", params: { settings: null, subs_only: true } });
    expect(result.current.view?.running).toBe("calibrate_subs");
    await perform(mock.drain);
    await perform(a.calibrate);
    expect(mock.calls).toContainEqual({ method: "calibrate", params: { settings: null, subs_only: false } });
    expect(result.current.view?.running).toBe("calibrate");
    await perform(mock.drain);
    expect(result.current.view?.running).toBeNull();
    expect(result.current.view?.resultMode).toEqual({ mode: session.stores.results.getSnapshot()!.calibration!.mode, ultra_ms: session.stores.results.getSnapshot()!.calibration!.ultra_ms });
    await perform(a.recommendXo);
    expect(mock.calls).toContainEqual({ method: "recommend_xo" });
    expect(result.current.view?.running).toBe("recommend_xo");
    await perform(mock.drain);
    mock.failJobs("mimo", "internal");
    await perform(a.computeMimo);
    expect(mock.calls).toContainEqual({ method: "mimo", params: { settings: null } });
    await perform(mock.drain);
    expect(result.current.view?.failure).toMatchObject({ job: "mimo", error: { code: "internal" } });
  });

  it("refreshes exact curve selections and keeps preview out of persisted settings", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    const a = result.current.actions;
    await perform(() => a.selectTab("gd"));
    expect(mock.calls).toContainEqual({ method: "result_curves", params: { tab: "gd", channel: null, group: null, xo_hz: null, corrected: null } });
    await perform(() => a.selectChannel("L", "front"));
    expect(session.host.getSnapshot().xoGroup).toBeNull();
    await perform(() => a.selectTab("xo"));
    await perform(() => a.selectChannel("R", "front"));
    expect(result.current.view).toMatchObject({ tab: "xo", channel: "R", xoGroup: "front" });
    await perform(() => a.selectXoGroup("surround"));
    expect(mock.calls).toContainEqual({ method: "result_curves", params: { tab: "xo", channel: "R", group: "surround", xo_hz: null, corrected: null } });
    await perform(() => a.previewXo("front", 90));
    expect(mock.calls).toContainEqual({ method: "result_curves", params: { tab: "xo", channel: "R", group: "front", xo_hz: 90, corrected: null } });
    expect(mock.calls.some((call) => call.method === "calibration_settings_edit")).toBe(false);
    await perform(() => a.changeSetting({ kind: "rc_hi", hz: 300 }));
    expect(session.host.getSnapshot().previewHz).toBeNull();
    expect(mock.calls).toContainEqual({ method: "calibration_settings_edit", params: { edit: { kind: "rc_hi", hz: 300 } } });
    expect(mock.calls.some((call) => call.method === "calibration_settings_set")).toBe(false);
    expect(mock.calls).toContainEqual({ method: "sub_warnings" });
    await perform(a.takeSubValues);
    expect(mock.calls).toContainEqual({ method: "sub_take_values" });
  });

  it("keeps previous curves while loading, ignores late reads, and clears a failed selection", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    const original = mock.request;
    const replies: { resolve: () => void; reject: () => void }[] = [];
    vi.spyOn(mock, "request").mockImplementation(async (request) => {
      const value = await original(request);
      if (request.method === "result_curves") await new Promise<void>((resolve, reject) => {
        replies.push({ resolve, reject: () => reject(new Error("curve transport failed")) });
      });
      return value;
    });
    const previous = result.current.view!.curves;
    await act(async () => { result.current.actions.selectChannel("L", "front"); });
    expect(result.current.view).toMatchObject({ curvesLoading: true, curves: previous });
    await act(async () => { result.current.actions.selectChannel("R", "front"); });
    await act(async () => { replies[1]!.resolve(); });
    expect(result.current.view?.curvesLoading).toBe(false);
    expect(session.stores.resultCurves.getSnapshot()?.params.channel).toBe("R");
    const latest = result.current.view!.curves;
    await act(async () => { replies[0]!.resolve(); });
    expect(result.current.view?.curves).toBe(latest);
    expect(session.stores.resultCurves.getSnapshot()?.params.channel).toBe("R");
    await act(async () => { result.current.actions.selectChannel("C", "center"); });
    expect(result.current.view?.curvesLoading).toBe(true);
    await act(async () => { replies[2]!.reject(); });
    expect(result.current.view).toMatchObject({ curves: null, curvesLoading: false });
    expect(session.errors.getSnapshot()?.error.detail).toContain("curve transport failed");
    await perform(() => {});
  });

  it("edits target points and preset, and honors both file dialog cancellations", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    const a = result.current.actions;
    const points: [number, number][] = [[20, 3], [20000, -2]];
    await perform(() => a.setTargetPoints(points));
    await perform(() => a.applyTargetPreset("flat"));
    expect(mock.calls).toContainEqual({ method: "target_set_points", params: { points } });
    expect(mock.calls).toContainEqual({ method: "target_apply_preset", params: { preset: "flat" } });
    mock.queueDialog("dialog_save_file", null); mock.queueDialog("dialog_open_file", null);
    await perform(a.saveTarget); await perform(a.loadTarget);
    expect(mock.calls.some((call) => call.method === "target_save" || call.method === "target_load")).toBe(false);
    expect(mock.calls).toContainEqual({ method: "dialog_save_file", params: { kind: "target", suggested: "타겟.json" } });
    expect(mock.calls).toContainEqual({ method: "dialog_open_file", params: { kind: "target" } });
    mock.queueDialog("dialog_save_file", "/target.json"); mock.queueDialog("dialog_open_file", "/target.json");
    await perform(a.saveTarget); await perform(a.loadTarget);
    expect(mock.calls).toContainEqual({ method: "target_save", params: { path: "/target.json" } });
    expect(mock.calls).toContainEqual({ method: "target_load", params: { path: "/target.json" } });
  });
});
