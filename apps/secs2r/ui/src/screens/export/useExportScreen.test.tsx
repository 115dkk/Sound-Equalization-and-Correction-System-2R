// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StrictMode, type ReactNode } from "react";
import { createMockService } from "../../ipc/mock";
import { SessionProvider } from "../../state/react";
import { createSession } from "../../state/session";
import { CAMILLA_EXE_FILE, EXPORT_KINDS, FILTER_KINDS, MDAT_SCOPES, canApply, canExport, exportBlock } from "./types";
import type { CamillaDevices } from "../../ipc/bindings/CamillaDevices";
import * as fx from "../../ipc/fixtures";
import { measureMethods } from "../../ipc/fixtures";
import { IpcFailure } from "../../ipc/client";
import { useExportScreen } from "./useExportScreen";

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

describe("useExportScreen", () => {
  it("waits for settings and does no I/O for tutorial exports", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => session.stores.project.set({ ...session.stores.project.getSnapshot()!, tutorial: true }));
    for (const kind of EXPORT_KINDS) {
      await perform(() => result.current.actions.exportFiles(kind));
      expect(result.current.view?.runs[kind]).toEqual({ status: "tutorial" });
    }
    expect(mock.calls).toEqual([]);
    await perform(() => session.stores.settings.set(null));
    expect(result.current.view).toBeNull();
  });

  it("exports all kinds through jobs and opens finished folders", async () => {
    await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain(); await session.stores.results.load();
    const { result } = renderHook(useExportScreen, { wrapper });
    for (const kind of EXPORT_KINDS) {
      if (kind === "filter_text") {
        // The sample result has subs: the service refuses the text before any
        // job starts (the screen keeps the button off; this is the backstop).
        mock.calls.length = 0;
        await perform(() => result.current.actions.exportFiles(kind));
        expect(session.errors.getSnapshot()?.error).toMatchObject({ code: "export_scope", params: { reason: "subs" } });
        expect(result.current.view?.runs[kind]).toBeUndefined();
        expect(result.current.view?.exporting).toBeNull();
        expect(mock.calls.some((call) => call.method === "open_folder")).toBe(false);
        continue;
      }
      await perform(() => result.current.actions.exportFiles(kind));
      expect(mock.calls).toContainEqual({ method: "export", params: { kind, folder: "", ...(kind === "before_after_mdat" ? { mdat_scope: "all" } : {}) } });
      expect(result.current.view?.exporting).toBe(kind);
      await perform(mock.drain);
      const run = result.current.view?.runs[kind];
      expect(run?.status).toBe("done");
      if (run?.status === "done") expect(mock.calls).toContainEqual({ method: "open_folder", params: { path: run.folder } });
      expect(result.current.view?.exporting).toBeNull();
    }
  });

  it("reads the latest scope and results when exporting and keeps the choice after calibration", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    const actions = result.current.actions;
    expect(result.current.view?.mdatScope).toBe("all");
    await perform(() => actions.setMdatScope("results"));
    expect(result.current.view?.mdatScope).toBe("results");
    expect(session.host.getSnapshot().mdatScope).toBe("results");
    await perform(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } }));
    await perform(mock.drain);
    expect(result.current.view?.mdatScope).toBe("results");
    for (const scope of MDAT_SCOPES) {
      await perform(() => actions.setMdatScope(scope));
      expect(result.current.view?.mdatScope).toBe(scope);
      mock.calls.length = 0;
      await perform(() => actions.exportFiles("before_after_mdat"));
      expect(mock.calls).toContainEqual({ method: "export", params: { kind: "before_after_mdat", folder: "", mdat_scope: scope } });
      await perform(mock.drain);
      expect(result.current.view?.runs.before_after_mdat?.status).toBe("done");
    }
    await perform(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } }));
    await perform(mock.drain);
    expect(result.current.view?.runs).toEqual({});
    expect(result.current.view?.mdatScope).toBe("results");
  });

  it.each(MDAT_SCOPES)("sends measurements without a result when %s was chosen", async (scope) => {
    const { result } = renderHook(useExportScreen, { wrapper });
    expect(result.current.view?.results.calibration).toBeNull();
    await perform(() => result.current.actions.setMdatScope(scope));
    await perform(() => result.current.actions.exportFiles("before_after_mdat"));
    expect(mock.calls).toContainEqual({ method: "export", params: { kind: "before_after_mdat", folder: "", mdat_scope: "measurements" } });
    await perform(mock.drain);
    expect(result.current.view?.runs.before_after_mdat?.status).toBe("done");
    expect(result.current.view?.mdatScope).toBe(scope);
  });

  it.each(EXPORT_KINDS.filter((kind) => kind !== "before_after_mdat"))("never sends mdat_scope for %s", async (kind) => {
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setMdatScope("measurements"));
    await perform(() => result.current.actions.exportFiles(kind));
    expect(mock.calls.filter((call) => call.method === "export")).toEqual([{ method: "export", params: { kind, folder: "" } }]);
  });

  it.each(EXPORT_KINDS)("sends the current trimmed design name only for filter kind %s", async (kind) => {
    const { result } = renderHook(useExportScreen, { wrapper });
    const actions = result.current.actions;
    await perform(() => actions.setDesignName("\u0085 거실 영화 \u0085"));
    expect(result.current.view?.designName).toBe("\u0085 거실 영화 \u0085");
    await perform(() => actions.exportFiles(kind));
    expect(mock.calls.filter((call) => call.method === "export")).toEqual([{ method: "export", params: {
      kind, folder: "",
      ...(FILTER_KINDS.some((filter) => filter === kind) ? { design_name: "거실 영화" } : {}),
      ...(kind === "before_after_mdat" ? { mdat_scope: "measurements" } : {}),
    } }]);
  });

  it.each(["", "  ", "a".repeat(65), "bad\nname", "bad/name", "bad.", "con.txt"])("omits an unset or invalid design name %j", async (name) => {
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setDesignName(name));
    for (const kind of FILTER_KINDS) await perform(() => result.current.actions.exportFiles(kind));
    expect(mock.calls.filter((call) => call.method === "export")).toEqual(FILTER_KINDS.map((kind) => ({ method: "export", params: { kind, folder: "" } })));
  });

  it.each(["capture", "playback"] as const)("sends devices only to CamillaDSP when %s is not file", async (side) => {
    const { result } = renderHook(useExportScreen, { wrapper });
    const actions = result.current.actions;
    const devices: CamillaDevices = { capture: { type: "file" }, playback: { type: "file" }, [side]: { type: "asio", device: "Tesira ASIO" } };
    await perform(() => actions.setCamillaDevices(devices));
    expect(result.current.view?.camillaDevices).toEqual(devices);
    for (const kind of EXPORT_KINDS) await perform(() => actions.exportFiles(kind));
    expect(mock.calls.filter((call) => call.method === "export")).toEqual(EXPORT_KINDS.map((kind) => ({ method: "export", params: {
      kind, folder: "", ...(kind === "camilla_dsp" ? { camilla_devices: devices } : {}),
      ...(kind === "before_after_mdat" ? { mdat_scope: "measurements" } : {}),
    } })));
  });

  it.each(["project_open", "project_new"] as const)("resets the name but preserves CamillaDSP devices after %s", async (method) => {
    const { result, unmount } = renderHook(useExportScreen, { wrapper });
    const devices: CamillaDevices = { capture: { type: "file" }, playback: { type: "wasapi", device: "Speakers (Tesira)", format: "S24", exclusive: true } };
    await perform(() => { result.current.actions.setDesignName("Cinema"); result.current.actions.setCamillaDevices(devices); });
    unmount();
    const reopened = renderHook(useExportScreen, { wrapper });
    expect(reopened.result.current.view).toMatchObject({ designName: "Cinema", camillaDevices: devices });
    await perform(() => session.call({ method: "project_save", params: { path: "/projects/cinema.secs" } }));
    expect(reopened.result.current.view?.designName).toBe("Cinema");
    await perform(() => session.call(method === "project_open"
      ? { method, params: { path: "/projects/cinema.secs" } }
      : { method, params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } }));
    expect(reopened.result.current.view).toMatchObject({ designName: "", camillaDevices: devices });
  });

  it("uses the session's device list without repeated reads and follows enumeration updates", async () => {
    const { result, rerender } = renderHook(useExportScreen, { wrapper });
    expect(result.current.view?.nativeDevices).toEqual(measureMethods.native_devices);
    rerender();
    expect(mock.calls.filter((call) => call.method === "measure_methods")).toEqual([]);
    await perform(() => mock.events.emit({ event: "measure_status", job: 0, status: { ...measureMethods, native_devices: [measureMethods.native_devices[0]!] } }));
    expect(result.current.view?.nativeDevices).toEqual([measureMethods.native_devices[0]]);
    await perform(() => session.stores.methods.set({ ...measureMethods, native_error: { code: "device_unavailable", params: {}, detail: "enumeration failed", retryable: true } }));
    expect(result.current.view?.nativeDevices).toBeNull();
    await perform(() => session.stores.methods.set({ ...measureMethods, native_devices: [] }));
    expect(result.current.view?.nativeDevices).toBeNull();
    await perform(() => {
      session.stores.methods.set(measureMethods);
      session.stores.bootstrap.set({ ...session.stores.bootstrap.getSnapshot()!, platform: { os: "linux", arch: "x86_64" } });
    });
    expect(result.current.view?.nativeDevices).toBeNull();
  });

  it.each([false, true])("reads unavailable methods once on mount (failed: %s)", async (failed) => {
    session.stores.methods.set(null);
    if (failed) mock.failNext("measure_methods", "device_unavailable");
    const { result, rerender } = renderHook(useExportScreen, { wrapper: ({ children }: { children: ReactNode }) => <StrictMode><SessionProvider value={session}>{children}</SessionProvider></StrictMode> });
    expect(result.current.view?.nativeDevices).toBeNull();
    await perform(() => {});
    expect(result.current.view?.nativeDevices).toEqual(failed ? null : measureMethods.native_devices);
    rerender();
    expect(mock.calls.filter((call) => call.method === "measure_methods")).toHaveLength(1);
  });

  it.each(["length", "control", "character", "trailing", "reserved"])("records a pre-job design-name %s rejection in the requested row", async (problem) => {
    const { result } = renderHook(useExportScreen, { wrapper });
    const reason = `design_name_${problem}`;
    mock.failNext("export", "invalid_request", "invalid design name", { reason });
    await perform(() => result.current.actions.exportFiles("convolution"));
    expect(result.current.view?.runs.convolution).toMatchObject({ status: "failed", error: { code: "invalid_request", params: { reason } } });
    expect(result.current.view?.exporting).toBeNull();
    expect(mock.calls.some((call) => call.method === "open_folder")).toBe(false);
  });

  it("does not put a late design-name rejection into a replacement project", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    let reject!: (error: unknown) => void;
    const original = mock.request;
    vi.spyOn(mock, "request").mockImplementation((request) => request.method === "export"
      ? new Promise((_, fail) => { reject = fail; }) : original(request));
    act(() => result.current.actions.exportFiles("convolution"));
    await act(async () => { await session.call({ method: "project_open", params: { path: "/projects/next.secs" } }); });
    await perform(() => reject(new IpcFailure({ code: "invalid_request", params: { reason: "design_name_reserved" }, detail: "invalid design name", retryable: false })));
    expect(result.current.view?.runs).toEqual({});
  });

  it("recalibrates a reopened stale cache and unblocks both mdat formats", async () => {
    await perform(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } }));
    await perform(mock.drain);
    await perform(() => session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mode", mode: "linear" } } }));
    await perform(() => session.call({ method: "project_save", params: { path: "/projects/stale.secs" } }));
    await perform(() => session.call({ method: "project_open", params: { path: "/projects/stale.secs" } }));
    const { result } = renderHook(useExportScreen, { wrapper });
    const mdatKinds = ["staged_mdat", "before_after_mdat"] as const;
    expect(result.current.view?.results.calibration).toMatchObject({ cached: true, stale: true, mode: "mixed" });
    for (const kind of mdatKinds) {
      expect(exportBlock(result.current.view!, kind)).toBe("stale_cache");
      await perform(() => result.current.actions.exportFiles(kind));
      expect(session.errors.getSnapshot()?.error).toMatchObject({ code: "export_scope", params: { reason: "stale_cache" } });
      expect(result.current.view?.exporting).toBeNull();
    }
    expect(result.current.view?.recalibrating).toBe(false);

    mock.calls.length = 0;
    await perform(result.current.actions.recalibrate);
    expect(mock.calls).toContainEqual({ method: "calibrate", params: { settings: null, subs_only: false } });
    expect(result.current.view?.recalibrating).toBe(true);
    expect(result.current.view?.computing).toBe(true);
    for (const kind of mdatKinds) expect(exportBlock(result.current.view!, kind)).toBe("stale_cache");
    await perform(mock.drain);
    expect(result.current.view?.recalibrating).toBe(false);
    expect(result.current.view?.computing).toBe(false);
    expect(result.current.view?.results.calibration).toMatchObject({ cached: false, stale: false, mode: "linear" });
    for (const kind of mdatKinds) {
      expect(exportBlock(result.current.view!, kind)).toBeNull();
      await perform(() => result.current.actions.exportFiles(kind));
      expect(result.current.view?.exporting).toBe(kind);
      await perform(mock.drain);
      expect(result.current.view?.runs[kind]?.status).toBe("done");
    }
  });

  it.each(["failed", "cancelled"] as const)("clears recalibrating after a %s job without unblocking the stale cache", async (status) => {
    await perform(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } }));
    await perform(mock.drain);
    await perform(() => session.call({ method: "project_save", params: { path: "/projects/cached.secs" } }));
    await perform(() => session.call({ method: "project_open", params: { path: "/projects/cached.secs" } }));
    await perform(() => session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mode", mode: "linear" } } }));
    const { result } = renderHook(useExportScreen, { wrapper });
    if (status === "failed") mock.failJobs("calibrate", "engine_input");
    await perform(result.current.actions.recalibrate);
    expect(result.current.view?.recalibrating).toBe(true);
    if (status === "cancelled") {
      const job = session.jobs.getSnapshot().jobs.find((job) => job.kind === "calibrate" && job.status === "running")!;
      await perform(() => session.call({ method: "job_cancel", params: { job: job.job } }));
      expect(result.current.view?.recalibrating).toBe(true);
    }
    await perform(mock.drain);
    expect(result.current.view?.recalibrating).toBe(false);
    expect(result.current.view?.results.calibration).toMatchObject({ cached: true, stale: true });
    expect(exportBlock(result.current.view!, "staged_mdat")).toBe("stale_cache");
    expect(exportBlock(result.current.view!, "before_after_mdat")).toBe("stale_cache");
  });

  it("reports failed exports without opening a folder", async () => {
    await session.call({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain(); await session.stores.results.load();
    mock.calls.length = 0; mock.failJobs("export", "export_failed");
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.exportFiles("camilla_dsp"));
    await perform(mock.drain);
    expect(result.current.view?.runs.camilla_dsp).toMatchObject({ status: "failed", error: { code: "export_failed" } });
    expect(mock.calls.some((call) => call.method === "open_folder")).toBe(false);
  });

  it("wires folder selection, cancellation, opening and navigation", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    const original = session.stores.folder.getSnapshot()!.folder;
    mock.queueDialog("dialog_pick_folder", null);
    await perform(result.current.actions.changeFolder);
    expect(mock.calls).toContainEqual({ method: "dialog_pick_folder", params: { start: original } });
    expect(mock.calls.some((call) => call.method === "export_folder_set")).toBe(false);
    mock.queueDialog("dialog_pick_folder", "/exports");
    await perform(result.current.actions.changeFolder);
    expect(mock.calls).toContainEqual({ method: "export_folder_set", params: { folder: "/exports" } });
    expect(result.current.view?.folder?.folder).toBe("/exports");
    await perform(() => result.current.actions.openFolder("/exports"));
    expect(mock.calls).toContainEqual({ method: "open_folder", params: { path: "/exports" } });
    await perform(result.current.actions.goToCalibrate);
    expect(session.host.getSnapshot().current).toBe("calibrate");
  });
});

describe("useExportScreen 적용", () => {
  const applySets = () => mock.calls.filter((call) => call.method === "apply_settings_set").map((call) => call.method === "apply_settings_set" ? call.params : null);
  const statusReads = () => mock.calls.filter((call) => call.method === "apply_status").length;
  async function withResult() {
    await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain(); await session.stores.results.load(); await session.settled();
    mock.calls.length = 0;
  }

  it("reads no status while both targets write files, and writes the whole ApplySettings when a level changes", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => {});
    expect(statusReads()).toBe(0);
    expect(result.current.view?.apply).toMatchObject({ settings: fx.appSettings.apply, status: null, windows: true, applying: null, runs: {} });
    await perform(() => result.current.actions.setApplyLevel("camilla", "apply"));
    expect(applySets()).toEqual([{ camilla: { level: "apply", address: "127.0.0.1:1234" }, eqapo: { level: "files" } }]);
    expect(result.current.view?.apply.settings.camilla.level).toBe("apply");
    expect(session.stores.appSettings.getSnapshot()?.apply.camilla.level).toBe("apply");
    // Turning to 적용 reads the status once.
    expect(statusReads()).toBe(1);
    expect(result.current.view?.apply.status).toEqual(fx.applyStatus);
    expect(result.current.view?.apply.checking).toBe(false);
    await perform(() => result.current.actions.setApplyLevel("eqapo", "apply"));
    expect(applySets().at(-1)).toEqual({ camilla: { level: "apply", address: "127.0.0.1:1234" }, eqapo: { level: "apply" } });
    await perform(() => result.current.actions.setApplyLevel("camilla", "files"));
    expect(applySets().at(-1)).toEqual({ camilla: { level: "files", address: "127.0.0.1:1234" }, eqapo: { level: "apply" } });
    // The quiet 다시 확인 reads it again.
    const before = statusReads();
    await perform(result.current.actions.refreshApplyStatus);
    expect(statusReads()).toBe(before + 1);
  });

  it("builds each level change from the latest stored settings, so a change elsewhere is kept", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    // Another client stores a new address after this screen read the settings.
    await mock.request({ method: "apply_settings_set", params: { camilla: { level: "files", address: "10.0.0.5:1234" }, eqapo: { level: "files" } } });
    mock.calls.length = 0;
    await perform(() => result.current.actions.setApplyLevel("eqapo", "apply"));
    expect(applySets()).toEqual([{ camilla: { level: "files", address: "10.0.0.5:1234" }, eqapo: { level: "apply" } }]);
  });

  it("keeps the typed address as a draft, stores it trimmed once it is valid, and never stores an invalid one", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setApplyLevel("camilla", "apply"));
    mock.calls.length = 0;
    await perform(() => result.current.actions.setCamillaAddress("192.168.0.40"));
    expect(result.current.view?.apply.addressDraft).toBe("192.168.0.40");
    await perform(result.current.actions.saveCamillaAddress);
    expect(applySets()).toEqual([]);
    await perform(() => result.current.actions.setCamillaAddress(" 192.168.0.40:1234 "));
    await perform(result.current.actions.saveCamillaAddress);
    expect(applySets()).toEqual([{ camilla: { level: "apply", address: "192.168.0.40:1234" }, eqapo: { level: "files" } }]);
    expect(result.current.view?.apply.addressDraft).toBeNull();
    expect(result.current.view?.apply.settings.camilla.address).toBe("192.168.0.40:1234");
    // A new address reads the status again.
    expect(mock.calls.filter((call) => call.method === "apply_status").length).toBeGreaterThan(0);
    // Typing the stored address back is no draft; saving it again sends nothing.
    await perform(() => result.current.actions.setCamillaAddress("192.168.0.40:1234"));
    expect(result.current.view?.apply.addressDraft).toBeNull();
    mock.calls.length = 0;
    await perform(result.current.actions.saveCamillaAddress);
    expect(applySets()).toEqual([]);
  });

  it("stores a pending address before applying, sends the row's devices, and runs one export-group job at a time", async () => {
    await withResult();
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setApplyLevel("camilla", "apply"));
    const devices: CamillaDevices = { capture: { type: "file" }, playback: { type: "wasapi", device: "Speakers (Tesira)", format: "S24", exclusive: true } };
    await perform(() => { result.current.actions.setCamillaDevices(devices); result.current.actions.setCamillaAddress("127.0.0.1:1300"); });
    mock.calls.length = 0;
    await perform(result.current.actions.applyCamilla);
    const methods = mock.calls.map((call) => call.method);
    expect(methods.indexOf("apply_settings_set")).toBeLessThan(methods.indexOf("apply_camilla"));
    expect(applySets()).toEqual([{ camilla: { level: "apply", address: "127.0.0.1:1300" }, eqapo: { level: "files" } }]);
    expect(mock.calls).toContainEqual({ method: "apply_camilla", params: { camilla_devices: devices } });
    // The job runs like an export: the row knows it, every export waits.
    expect(result.current.view?.apply.applying).toBe("camilla");
    expect(canExport(result.current.view!, "camilla_dsp")).toBe(false);
    expect(session.jobs.getSnapshot().jobs.find((job) => job.kind === "apply")?.status).toBe("running");
    mock.calls.length = 0;
    await perform(mock.drain);
    expect(result.current.view?.apply.applying).toBeNull();
    expect(result.current.view?.apply.runs.camilla).toEqual({ status: "applied", designId: fx.MOCK_DESIGN_ID, folder: "C:/Users/demo/AppData/Roaming/SECS2R/applied/camilladsp" });
    expect(mock.calls.some((call) => call.method === "apply_status")).toBe(true);
    expect(mock.calls.some((call) => call.method === "open_folder")).toBe(false);
    // File placeholders send no devices.
    await perform(() => result.current.actions.setCamillaDevices({ capture: { type: "file" }, playback: { type: "file" } }));
    mock.calls.length = 0;
    await perform(result.current.actions.applyCamilla);
    expect(mock.calls).toContainEqual({ method: "apply_camilla", params: {} });
    await perform(mock.drain);
  });

  it("records a refusal before any job in its row, and applies once Equalizer APO is there", async () => {
    await withResult();
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setApplyLevel("eqapo", "apply"));
    expect(result.current.view?.apply.status?.eqapo.installed).toBe(false);
    await perform(result.current.actions.applyEqapo);
    expect(result.current.view?.apply.runs.eqapo).toMatchObject({ status: "failed", error: { code: "unsupported", params: { target: "equalizer_apo" } } });
    mock.setApplyStatus({ ...fx.applyStatus, eqapo: fx.eqapoXt });
    await perform(result.current.actions.refreshApplyStatus);
    expect(result.current.view?.apply.status?.eqapo).toEqual(fx.eqapoXt);
    await perform(result.current.actions.applyEqapo);
    expect(result.current.view?.apply.applying).toBe("eqapo");
    await perform(mock.drain);
    expect(result.current.view?.apply.runs.eqapo).toMatchObject({ status: "applied", folder: `${fx.eqapoXt.config_path}/SECS2R` });
    expect(result.current.view?.apply.status?.eqapo).toMatchObject({ include: "present", applied_design_id: fx.MOCK_DESIGN_ID });
    // A failed job lands in the row with its folder.
    mock.failJobs("apply", "permission_denied", "Access is denied. (os error 5)", { path: "C:/Program Files/EqualizerAPO/config/SECS2R" });
    await perform(result.current.actions.applyEqapo);
    await perform(mock.drain);
    expect(result.current.view?.apply.runs.eqapo).toMatchObject({ status: "failed", error: { code: "permission_denied", params: { path: "C:/Program Files/EqualizerAPO/config/SECS2R" } } });
  });

  it("takes SECS 2R's line out of config.txt, says so in the row and reads the status again", async () => {
    await withResult();
    mock.setApplyStatus({ ...fx.applyStatus, eqapo: { ...fx.eqapoXt, include: "present", applied_design_id: fx.MOCK_DESIGN_ID } });
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setApplyLevel("eqapo", "apply"));
    mock.calls.length = 0;
    await perform(result.current.actions.revertEqapo);
    expect(mock.calls.map((call) => call.method)).toEqual(["revert_eqapo", "apply_status"]);
    expect(result.current.view?.apply.reverting).toBe(false);
    expect(result.current.view?.apply.runs.eqapo).toEqual({ status: "reverted" });
    expect(result.current.view?.apply.status?.eqapo).toMatchObject({ include: "absent", applied_design_id: fx.MOCK_DESIGN_ID });
    // A refusal stays in the row too.
    mock.setApplyStatus(fx.applyStatus);
    await perform(result.current.actions.revertEqapo);
    expect(result.current.view?.apply.runs.eqapo).toMatchObject({ status: "failed", error: { code: "unsupported" } });
  });

  it("clears the apply outcomes for a new result and a new project, and keeps the levels", async () => {
    await withResult();
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setApplyLevel("camilla", "apply"));
    await perform(result.current.actions.applyCamilla);
    await perform(mock.drain);
    expect(result.current.view?.apply.runs.camilla?.status).toBe("applied");
    await perform(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } }));
    await perform(mock.drain);
    expect(result.current.view?.apply.runs).toEqual({});
    session.host.set({ applyRuns: { camilla: { status: "reverted" } } });
    await perform(() => session.call({ method: "project_open", params: { path: "/projects/next.secs" } }));
    expect(result.current.view?.apply.runs).toEqual({});
    expect(result.current.view?.apply.settings.camilla.level).toBe("apply");
  });
});

describe("useExportScreen 관리", () => {
  const applySets = () => mock.calls.flatMap((call) => (call.method === "apply_settings_set" ? [call.params] : []));
  async function withResult() {
    await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain(); await session.stores.results.load(); await session.settled();
    mock.calls.length = 0;
  }

  it("writes 관리 into the whole ApplySettings, picks camilladsp.exe into it, and keeps it through other changes", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setApplyLevel("camilla", "manage"));
    expect(applySets()).toEqual([{ camilla: { level: "manage", address: "127.0.0.1:1234" }, eqapo: { level: "files" } }]);
    expect(result.current.view?.apply.settings.camilla.level).toBe("manage");
    // 관리 reads the status, which carries the service's state.
    expect(result.current.view?.apply.host).toMatchObject({ service: "not_installed", status: null, installing: false, stopping: false, failure: null });
    await perform(result.current.actions.pickCamillaExe);
    expect(mock.calls).toContainEqual({ method: "dialog_open_file", params: { kind: CAMILLA_EXE_FILE } });
    expect(applySets().at(-1)).toEqual({ camilla: { level: "manage", address: "127.0.0.1:1234", exe_path: fx.CAMILLA_EXE }, eqapo: { level: "files" } });
    expect(result.current.view?.apply.settings.camilla.exe_path).toBe(fx.CAMILLA_EXE);
    // Another level change and the address keep the executable.
    await perform(() => result.current.actions.setApplyLevel("eqapo", "manage"));
    await perform(() => result.current.actions.setCamillaAddress("127.0.0.1:1300"));
    await perform(result.current.actions.saveCamillaAddress);
    expect(applySets().at(-1)).toEqual({ camilla: { level: "manage", address: "127.0.0.1:1300", exe_path: fx.CAMILLA_EXE }, eqapo: { level: "manage" } });
    // A cancelled dialog changes nothing.
    const before = applySets().length;
    mock.queueDialog("dialog_open_file", null);
    await perform(result.current.actions.pickCamillaExe);
    expect(applySets()).toHaveLength(before);
  });

  it("keeps a typed executable as a draft, stores it trimmed, and stores none for an empty field", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setApplyLevel("camilla", "manage"));
    mock.calls.length = 0;
    await perform(() => result.current.actions.setCamillaExe("  D:/CamillaDSP/camilladsp.exe "));
    expect(result.current.view?.apply.exeDraft).toBe("  D:/CamillaDSP/camilladsp.exe ");
    expect(applySets()).toEqual([]);
    await perform(result.current.actions.saveCamillaExe);
    expect(applySets()).toEqual([{ camilla: { level: "manage", address: "127.0.0.1:1234", exe_path: "D:/CamillaDSP/camilladsp.exe" }, eqapo: { level: "files" } }]);
    expect(result.current.view?.apply.exeDraft).toBeNull();
    // Typing the stored path back is no draft; leaving the field unchanged stores nothing.
    await perform(() => result.current.actions.setCamillaExe("D:/CamillaDSP/camilladsp.exe"));
    expect(result.current.view?.apply.exeDraft).toBeNull();
    await perform(result.current.actions.saveCamillaExe);
    expect(applySets()).toHaveLength(1);
    // Emptied: the setting goes.
    await perform(() => result.current.actions.setCamillaExe(""));
    await perform(result.current.actions.saveCamillaExe);
    expect(applySets().at(-1)).toEqual({ camilla: { level: "manage", address: "127.0.0.1:1234" }, eqapo: { level: "files" } });
    expect(result.current.view?.apply.settings.camilla.exe_path).toBeUndefined();
  });

  it("installs the service from a row, then 관리 시작 stores a typed executable, applies and hands over, and 관리 끄기 lets go", async () => {
    await withResult();
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setApplyLevel("camilla", "manage"));
    await perform(() => result.current.actions.setCamillaExe(fx.CAMILLA_EXE));
    expect(applySets().at(-1)?.camilla.exe_path).toBeUndefined();
    mock.calls.length = 0;
    await perform(() => result.current.actions.installHost("camilla"));
    expect(mock.calls.map((call) => call.method)).toEqual(["host_install", "apply_status"]);
    expect(result.current.view?.apply.host).toMatchObject({ service: "running", status: fx.hostIdle, installing: false, failure: null });
    expect(canApply(result.current.view!, "camilla")).toBe(true);
    mock.calls.length = 0;
    await perform(result.current.actions.applyCamilla);
    const methods = mock.calls.map((call) => call.method);
    // The typed executable is stored before the apply starts.
    expect(methods.indexOf("apply_settings_set")).toBeLessThan(methods.indexOf("apply_camilla"));
    expect(applySets().at(-1)?.camilla.exe_path).toBe(fx.CAMILLA_EXE);
    expect(result.current.view?.apply.applying).toBe("camilla");
    await perform(mock.drain);
    // No CamillaDSP answered: the design went to the service, which starts CamillaDSP with it.
    expect(result.current.view?.apply.runs.camilla).toMatchObject({ status: "applied", handedOver: true });
    expect(result.current.view?.apply.host.status).toMatchObject({ managed: { camilla: { exe_path: fx.CAMILLA_EXE } }, camilla: { state: { kind: "running" }, pid: 4812 } });
    mock.calls.length = 0;
    await perform(() => result.current.actions.stopManaging("camilla"));
    expect(mock.calls.map((call) => call.method)).toEqual(["host_stop_managing", "apply_status"]);
    expect(result.current.view?.apply.host.status).toMatchObject({ managed: {}, camilla: { state: { kind: "stopped" } } });
    expect(result.current.view?.apply.host.stopping).toBe(false);
  });

  it("keeps a failed host action in the row it came from, without the global notice", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setApplyLevel("eqapo", "manage"));
    mock.failNext("host_install", "apply_failed", "elevated install: The operation was canceled by the user. (os error 1223)");
    await perform(() => result.current.actions.installHost("eqapo"));
    expect(result.current.view?.apply.host).toMatchObject({ service: "not_installed", installing: false, failure: { action: "install", target: "eqapo", error: { code: "apply_failed" } } });
    expect(session.errors.getSnapshot()).toBeNull();
    // The next try clears it.
    await perform(() => result.current.actions.installHost("eqapo"));
    expect(result.current.view?.apply.host).toMatchObject({ service: "running", failure: null });
    // 관리 끄기 that cannot reach the service says so in its row too.
    await perform(() => mock.request({ method: "host_uninstall" }));
    await perform(() => result.current.actions.stopManaging("eqapo"));
    expect(result.current.view?.apply.host.failure).toMatchObject({ action: "stop", target: "eqapo" });
    expect(session.errors.getSnapshot()).toBeNull();
  });
});
