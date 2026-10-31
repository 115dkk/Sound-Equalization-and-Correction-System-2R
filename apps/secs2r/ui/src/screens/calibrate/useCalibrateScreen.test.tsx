// @vitest-environment happy-dom
import { act, cleanup, fireEvent, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { gpuComputeStatus, resultsSummary } from "../../ipc/fixtures";
import { createMockService } from "../../ipc/mock";
import { SessionProvider } from "../../state/react";
import { createSession } from "../../state/session";
import { installFakeCanvas } from "../../test/canvas";
import { renderWith } from "../../test/render";
import { CalibrateScreen } from "./CalibrateScreen";
import { useCalibrateScreen } from "./useCalibrateScreen";

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

describe("useCalibrateScreen", () => {
  it("reads the DSP's taps, budget and output rate, counting 자동 as 48 kHz", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    expect(result.current.view).toMatchObject({ firTaps: 65536, firBudget: 2097152, outputRate: 48000 });
    await perform(() => session.writers.dsp.edit({ preset: "minidsp" }));
    expect(result.current.view).toMatchObject({ firTaps: 2048, firBudget: 4096, outputRate: 96000 });
    await perform(() => session.writers.dsp.edit({ preset: "camilla" }));
    await perform(() => session.writers.dsp.edit({ output_rate: "auto" }));
    expect(result.current.view?.outputRate).toBe(48000);
  });

  it("edits the center crossover after selecting C on SPL and opening XO", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    await perform(() => result.current.actions.selectChannel("C", null));
    await perform(() => result.current.actions.selectTab("xo"));
    expect(session.host.getSnapshot().xoGroup).toBe("센터");
    expect(result.current.view?.xoGroup).toBe("센터");
    expect(mock.calls).toContainEqual({ method: "result_curves", params: { tab: "xo", channel: "C", group: "센터", xo_hz: null, corrected: null } });
    const group = result.current.view!.xoGroup!;
    await perform(() => result.current.actions.previewXo(group, 95));
    await perform(() => result.current.actions.changeSetting({ kind: "xo", group, hz: 95 }));
    expect(mock.calls).toContainEqual({ method: "calibration_settings_edit", params: { edit: { kind: "xo", group: "센터", hz: 95 } } });
    expect(result.current.view?.settings.xo["센터"]).toBe(95);
    expect(result.current.view?.xoRecTurnedOffByEdit).toBe(true);
    await perform(() => result.current.actions.changeSetting({ kind: "xo_rec_include", on: true }));
    expect(result.current.view?.xoRecTurnedOffByEdit).toBe(false);
    await perform(() => result.current.actions.selectChannel("L", null));
    expect(result.current.view?.xoGroup).toBe("프론트");
    await perform(() => result.current.actions.selectTab("spl"));
    await perform(() => result.current.actions.selectChannel("C", null));
    await perform(() => result.current.actions.selectTab("xo"));
    expect(result.current.view?.xoGroup).toBe("센터");
  });
  it("removes the automatic-off hint after undo and an explicit off choice", async () => {
    installFakeCanvas();
    function Live() {
      const { view, actions } = useCalibrateScreen();
      return view ? <CalibrateScreen view={view} actions={actions} /> : null;
    }
    renderWith(<Live />, { session });
    let token = "";
    await perform(async () => { token = (await session.call({ method: "position_clear", params: { position: 9 } })).token; });
    await perform(() => session.call({ method: "calibration_settings_edit", params: { edit: { kind: "xo", group: "프론트", hz: 95 } } }));
    expect(document.getElementById("calibrate.xo_rec_include-hint")?.textContent).toBe("XO를 직접 바꿔 꺼짐");
    await perform(() => session.call({ method: "undo", params: { token } }));
    expect(document.getElementById("calibrate.xo_rec_include-hint")).toBeNull();
    await perform(() => session.call({ method: "calibration_settings_edit", params: { edit: { kind: "xo_rec_include", on: false } } }));
    expect(document.querySelector('[data-ui="calibrate.xo_rec_include"]')?.getAttribute("aria-checked")).toBe("false");
    expect(document.getElementById("calibrate.xo_rec_include-hint")).toBeNull();
  });

  it("resolves the default preview channel on XO entry and leaves an empty preview without a group", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    await perform(() => result.current.actions.selectTab("xo"));
    expect(result.current.view?.xoGroup).toBe("프론트");
    await perform(() => result.current.actions.selectXoGroup("센터"));
    expect(result.current.view?.xoGroup).toBe("센터");
    await perform(() => session.call({ method: "project_new", params: { path: "", preset: "2.1", sub_count: 1, sample: false, tutorial: false } }));
    await perform(() => result.current.actions.selectTab("xo"));
    expect(result.current.view?.xoGroup).toBeNull();
  });

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
    expect(mock.calls).toContainEqual({ method: "recommend_xo" });
    expect(result.current.view?.running).toBe("recommend_xo");
    await perform(mock.drain);
    expect(mock.calls).toContainEqual({ method: "calibrate", params: { settings: null, subs_only: false, mimo: false } });
    expect(result.current.view?.running).toBe("calibrate");
    expect(result.current.view?.runningMimo).toBe(false);
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

  it("computes MIMO with 보정 계산 only when it is on with three positions, and puts a MIMO failure in the MIMO card (P142)", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    const a = result.current.actions;
    const calibrateCall = (mimo: boolean) => ({ method: "calibrate", params: { settings: null, subs_only: false, mimo } });
    // Isolate the MIMO choice from the optional recommendation job.
    await perform(() => a.changeSetting({ kind: "xo_rec_include", on: false }));
    // On with three positions: one job computes both.
    await perform(() => a.changeSetting({ kind: "mimo_on", on: true }));
    await perform(a.calibrate);
    expect(mock.calls).toContainEqual(calibrateCall(true));
    expect(result.current.view).toMatchObject({ running: "calibrate", runningMimo: true });
    await perform(mock.drain);
    expect(result.current.view?.running).toBeNull();
    expect(result.current.view?.results.calibration?.stale).toBe(false);
    expect(result.current.view?.results.mimo?.stale).toBe(false);
    // MIMO fails after the correction: the correction is the new one, and the failure is MIMO's, without the stage.
    mock.failJobs("mimo", "engine_input", "MIMO support design produced nonfinite filters");
    await perform(() => a.changeSetting({ kind: "boost_max", db: 6 }));
    expect(result.current.view?.results.calibration?.stale).toBe(true);
    await perform(a.calibrate);
    await perform(mock.drain);
    expect(result.current.view?.failure).toEqual({ job: "mimo", error: expect.objectContaining({ code: "engine_input", params: {} }) });
    expect(session.host.getSnapshot().failure?.error.params.stage).toBe("mimo");
    expect(result.current.view?.results.calibration?.stale).toBe(false);
    // Off, and below three positions with MIMO on: the correction alone.
    mock.calls.length = 0;
    await perform(() => a.changeSetting({ kind: "mimo_on", on: false }));
    await perform(a.calibrate);
    expect(mock.calls).toContainEqual(calibrateCall(false));
    expect(result.current.view?.runningMimo).toBe(false);
    await perform(mock.drain);
    await perform(() => a.changeSetting({ kind: "mimo_on", on: true }));
    await perform(() => session.call({ method: "position_clear", params: { position: 3 } }));
    mock.calls.length = 0;
    await perform(a.calibrate);
    expect(mock.calls).toContainEqual(calibrateCall(false));
    expect(result.current.view?.runningMimo).toBe(false);
    await perform(mock.drain);
    expect(result.current.view?.failure).toBeNull();
  });

  it("stores the RC upper limit's basis and factor, and reads their evidence after a run (ADR 0021)", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    const a = result.current.actions;
    await perform(() => a.changeSetting({ kind: "xo_rec_include", on: false }));
    await perform(() => a.changeSetting({ kind: "rc_hi_basis", basis: "lenient" }));
    await perform(() => a.changeSetting({ kind: "rc_hi_transition", factor: 2 }));
    expect(result.current.view?.settings).toMatchObject({ rc_hi: null, rc_hi_basis: "lenient", rc_hi_transition: 2 });
    await perform(a.calibrate);
    await perform(mock.drain);
    expect(result.current.view?.lines.rc[0]).toEqual({
      code: "rc_limit",
      params: { hz: "400", auto: "true", basis: "lenient", factor: "2", schroeder_hz: "154", band_hz: "200", band_t60_s: "0.5" },
    });
    // Agreement with three measured positions: its evidence, and no room volume line.
    await perform(() => a.changeSetting({ kind: "rc_hi_basis", basis: "agreement" }));
    await perform(a.calibrate);
    await perform(mock.drain);
    expect(result.current.view?.lines.rc).toEqual([{ code: "rc_limit", params: { hz: "287", auto: "true", basis: "agreement", factor: "2", schroeder_hz: "154", positions: "3" } }]);
    // With two, the run fails the way the service fails it.
    await perform(() => session.call({ method: "position_clear", params: { position: 3 } }));
    expect(result.current.view?.positions).toEqual([1, 2]);
    await perform(a.calibrate);
    await perform(mock.drain);
    expect(result.current.view?.failure).toMatchObject({ job: "calibrate", error: { code: "engine_input", params: { reason: "rc_agreement_positions", measured: "2", required: "3" } } });
  });

  it("turns 극저역 보호 on from the range card's switch, then shows each channel's roll-off and the bus warning after runs (ADR 0022)", async () => {
    installFakeCanvas();
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "xo_rec_include", on: false } } });
    function Live() {
      const { view, actions } = useCalibrateScreen();
      return view ? <CalibrateScreen view={view} actions={actions} /> : null;
    }
    renderWith(<Live />, { session });
    const at = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
    const range = at("calibrate.settings.range")!;
    fireEvent.click(range.querySelector<HTMLElement>(".ui-advanced__toggle")!);
    const toggle = at("calibrate.range.low_protect")!;
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    await perform(() => fireEvent.click(toggle));
    expect(mock.calls).toContainEqual({ method: "calibration_settings_edit", params: { edit: { kind: "low_protect", on: true } } });
    expect(at("calibrate.range.low_protect")!.getAttribute("aria-checked")).toBe("true");
    await perform(() => fireEvent.click(at("calibrate.compute")!));
    await perform(mock.drain);
    const lines = () => [...at("calibrate.verdict.notes")!.querySelectorAll("li")].map((li) => li.textContent ?? "");
    expect(lines()).toContain("극저역 보호(SW1): 22 Hz 아래 부스트 안 함, 16 Hz 하이패스");
    expect(lines()).toContain("극저역 보호(L): 28 Hz 아래 부스트 안 함");
    expect(at("calibrate.verdict.low_protect")).toBeNull();
    // Below the 20 Hz band with the default boost limit the sub bus keeps its fit: a warning, not a note.
    // An edit refuses 15 Hz (ADR 0024); a project saved by SECS 2 keeps it, as the settings set here.
    const stored = await mock.request({ method: "calibration_settings_get" });
    await perform(() => session.call({ method: "calibration_settings_set", params: { settings: { ...stored, rc_lo: 15 } } }));
    await perform(() => fireEvent.click(at("calibrate.compute")!));
    await perform(mock.drain);
    expect(at("calibrate.verdict.low_protect")!.textContent).toContain("극저역 보호(SW1 + SW2): 22 Hz 아래 부스트를 막지 못해 원래 필터를 그대로 썼습니다.");
    expect(lines().some((line) => line.includes("SW1 + SW2"))).toBe(false);
  });

  it("keeps a refused edge or band in the view instead of a global notice, and drops it with the next range edit (ADR 0024)", async () => {
    const { result } = renderHook(useCalibrateScreen, { wrapper });
    const a = result.current.actions;
    expect(result.current.view?.rangeRejection).toBeNull();
    let accepted: boolean | undefined;
    await perform(async () => { accepted = await a.changeSetting({ kind: "rc_bands", bands: [{ lo_hz: 15000, hi_hz: 12000 }] }); });
    expect(accepted).toBe(false);
    expect(result.current.view?.rangeRejection).toMatchObject({
      edit: { kind: "rc_bands" },
      error: { code: "invalid_request", params: { field: "rc_bands", reason: "rc_band_range", index: "1", lo_hz: "15000", hi_hz: "12000" } },
    });
    expect(session.errors.getSnapshot()).toBeNull();
    expect(result.current.view?.settings.rc_bands).toEqual([]);
    // A valid list is stored sorted and clears the refusal.
    await perform(async () => { accepted = await a.changeSetting({ kind: "rc_bands", bands: [{ lo_hz: 15000, hi_hz: 20000 }, { lo_hz: 2000, hi_hz: 4000 }] }); });
    expect(accepted).toBe(true);
    expect(result.current.view?.rangeRejection).toBeNull();
    expect(result.current.view?.settings.rc_bands).toEqual([{ lo_hz: 2000, hi_hz: 4000 }, { lo_hz: 15000, hi_hz: 20000 }]);
    // The primary lower edge below 20 Hz: its reason with the limits.
    await perform(() => a.changeSetting({ kind: "rc_lo", hz: 15 }));
    expect(result.current.view?.rangeRejection?.error.params).toEqual({ field: "rc_lo", reason: "rc_lo_range", min: "20", max: "200" });
    expect(result.current.view?.settings.rc_lo).toBe(20);
    expect(session.errors.getSnapshot()).toBeNull();
    // Another edit's refusal is still a global notice and leaves the card's refusal alone.
    await perform(() => a.changeSetting({ kind: "rc_hi_transition", factor: 3 }));
    expect(session.errors.getSnapshot()).not.toBeNull();
    expect(result.current.view?.rangeRejection?.error.params.reason).toBe("rc_lo_range");
    // Choosing another basis replaces it.
    await perform(() => a.changeSetting({ kind: "rc_hi_basis", basis: "lenient" }));
    expect(result.current.view?.rangeRejection).toBeNull();
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
    expect(result.current.view).toMatchObject({ tab: "xo", channel: "R", xoGroup: "프론트" });
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
