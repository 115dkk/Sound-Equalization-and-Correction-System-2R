import type { StepId } from "../app/model";
import { IpcFailure, type ResponseOf } from "../ipc/client";
import type { Ipc } from "../ipc/context";
import type { Request } from "../ipc/bindings/Request";
import type { JobEvent } from "../ipc/bindings/JobEvent";
import type { MeasureConfigureParams } from "../ipc/bindings/MeasureConfigureParams";
import type { ResultCurvesParams } from "../ipc/bindings/ResultCurvesParams";
import type { ResultCurvesResult } from "../ipc/bindings/ResultCurvesResult";
import type { CamillaDevices } from "../ipc/bindings/CamillaDevices";
import type { ExportKind } from "../ipc/bindings/ExportKind";
import type { MdatScope } from "../ipc/bindings/MdatScope";
import type { XoRecommendation } from "../ipc/bindings/XoRecommendation";
import type { ConnectPath, MeterReading, MicCalibrationView, MonitorState, RewProblem } from "../screens/connect/types";
import type { CalibrateView } from "../screens/calibrate/types";
import { rangeRejection } from "../screens/calibrate/settingsView";
import type { ApplyRun, ApplyTarget, ExportRun, HostFailure } from "../screens/export/types";
import type { MeasureView } from "../screens/measure/types";
import { createResource as resource, createStore, type Failure } from "./store";
import { isActiveJob } from "./jobRetention";
import { createJobLedger, type FinishedJob, type JobRequestContext, type LedgerJob } from "./jobLedger";
import { effectsOf, projectEdits, refreshInputs, refreshMeasurements, type EffectContext } from "./effects";
import { createAutoRefresh } from "./autoRefresh";
import { calibrationMimo, startCalibration } from "./calibration";
import { createWriter, type Patch, type Writer, type WriterDeps } from "./writers";
import type { AppSettings } from "../ipc/bindings/AppSettings";
import type { ApplySettings } from "../ipc/bindings/ApplySettings";
import type { DspFields } from "../ipc/bindings/DspFields";
import type { LayoutSetParams } from "../ipc/bindings/LayoutSetParams";
import { wrongOutput } from "../screens/tutorial/questHost";
import { coalesceExternalChanges, externalRefresh } from "./externalChanges";

/** A DSP edit: a preset to select, or fields over the latest ones (`preset: null`). */
type DspEdit = DspFields & { preset: string | null };
type LayoutEdit = Pick<LayoutSetParams, "start_delay" | "timing">;
/** UI completion policy stays in the job ledger, never in the IPC payload. */
type SessionCall = <M extends Request["method"]>(
  request: Extract<Request, { method: M }>,
  context?: Pick<JobRequestContext, "calibrateOnFailure">,
) => Promise<ResponseOf<M>>;

type HostState = Pick<MeasureView, "selected" | "graphHidden" | "plotRange" | "mdat" | "warnings"> & {
  computeLoading: boolean; computeSaving: boolean; computeError: import("../ipc/bindings/IpcError").IpcError | null;
  current: StepId; path: ConnectPath | null;
  tab: CalibrateView["tab"]; channel: string | null; xoGroup: string | null; previewHz: number | null;
  targetEdit: CalibrateView["targetEdit"]; xoEdit: CalibrateView["xoEdit"];
  meter: MeterReading | null; monitor: MonitorState; playing: string | null; playingAll: boolean; masterDbfs: number;
  /** Why the level monitor job failed (the level check shows it); null once the monitor starts again or is switched off. */
  monitorError: import("../ipc/bindings/IpcError").IpcError | null;
  xoRecTurnedOffByEdit: boolean;
  xoRecommendation: XoRecommendation | null; runs: Partial<Record<ExportKind, ExportRun>>; mdatScope: MdatScope;
  /** 내보내기: the design name field's text (`""` = no name) and the CamillaDSP devices (file placeholders by default). */
  designName: string; camillaDevices: CamillaDevices;
  /** 내보내기: the convolution IR's extra rates in Hz (none by default); like the devices, kept for the session across projects. */
  extraRates: number[];
  /** 내보내기의 적용 (ADR 0018): how each target's last apply ended, the CamillaDSP address field's unsaved text, an apply_status read and a revert_eqapo in flight. */
  applyRuns: Partial<Record<ApplyTarget, ApplyRun>>; applyAddressDraft: string | null; applyChecking: boolean; applyReverting: boolean;
  /** 관리: the CamillaDSP executable field's unsaved text, a host_install or host_stop_managing in flight, and the last of them that failed. */
  applyExeDraft: string | null; hostInstalling: boolean; hostStopping: boolean; hostFailure: HostFailure | null;
  failure: CalibrateView["failure"]; rangeRejection: CalibrateView["rangeRejection"]; rewLaunch: "idle" | "checking" | "launching";
  rewProblem: RewProblem | null; rewProblemDetail: string | null; readingRew: boolean;
  switchingRewDriver: boolean;
  micCalBusy: MicCalibrationView["busy"]; micCalError: MicCalibrationView["error"];
  /** Projects opened or created in this window; screens reset what they hid for the previous one when it changes. */
  opened: number;
};

export function createSession(ipc: Ipc) {
  let pending = 0;
  let autoRefresh: ReturnType<typeof createAutoRefresh> | null = null;
  let savePromptOpen = false;
  let saveDialogs = 0;
  const canContinueRefresh = (revision: number) => {
    if (!autoRefresh?.canContinue(revision)) return false;
    if (savePromptOpen || saveDialogs > 0 || jobs.getSnapshot().jobs.some((job) => job.kind !== "status_watch" && isActiveJob(job.status))) {
      autoRefresh.defer(); return false;
    }
    return true;
  };
  const waiters = new Set<() => void>();
  async function track<T>(work: () => Promise<T>): Promise<T> {
    pending++;
    try { return await work(); }
    finally {
      if (--pending === 0) { autoRefresh?.check(); for (const resolve of waiters) resolve(); waiters.clear(); }
    }
  }
  // Wait for session work already started, not service jobs or future timer/events.
  // A tracked task must never await this barrier (it would wait for itself).
  const settled = (): Promise<void> => pending === 0 ? Promise.resolve() : new Promise((resolve) => waiters.add(resolve));
  function createResource<T>(read: () => Promise<T>, failed: (error: unknown) => void) {
    const store = resource(read, failed);
    return { ...store, load: () => track(() => store.load()) };
  }
  const rawHost = createStore<HostState>({
    computeLoading: true, computeSaving: false, computeError: null,
    current: "system", path: null, selected: null, graphHidden: false, plotRange: { x: null, y: null },
    mdat: null, warnings: [], tab: "spl", channel: null, xoGroup: null, previewHz: null, targetEdit: "idle", xoEdit: "idle",
    meter: null, monitor: "off", playing: null, playingAll: false, masterDbfs: -20, monitorError: null,
    xoRecTurnedOffByEdit: false, xoRecommendation: null, runs: {}, mdatScope: "all", failure: null, rangeRejection: null,
    designName: "", camillaDevices: { capture: { type: "file" }, playback: { type: "file" } }, extraRates: [],
    applyRuns: {}, applyAddressDraft: null, applyChecking: false, applyReverting: false,
    applyExeDraft: null, hostInstalling: false, hostStopping: false, hostFailure: null,
    micCalBusy: null, micCalError: null,
    rewLaunch: "idle", rewProblem: null, rewProblemDetail: null, readingRew: false, switchingRewDriver: false, opened: 0,
  });
  const host = { ...rawHost, set: (patch: Partial<HostState>) => rawHost.set({ ...rawHost.getSnapshot(), ...patch }) };
  const errors = createStore<Failure | null>(null);
  const ready = createStore(false);
  const tutorialReady = createStore(false);
  let tutorialStart: Promise<void> | null = null;
  let errorId = 0;
  let appSettingsFailure: number | null = null;
  const report = (error: unknown) => {
    const value = error instanceof IpcFailure ? error.error : { code: "internal" as const, params: {}, retryable: false, detail: String(error) };
    if (value.code !== "project_not_open") errors.set({ id: ++errorId, error: value });
  };
  const read = <M extends Request["method"]>(request: Extract<Request, { method: M }>) => createResource(() => ipc.request(request), report);
  const curveState = createStore<{ params: ResultCurvesParams; curves: ResultCurvesResult | null; pending: ResultCurvesParams | null } | null>(null);
  let curveVersion = 0;
  const resultCurves = { ...curveState, load: () => refreshCurves(curveParams()) };
  let computeVersion = 0;
  let applyVersion = 0;
  const stores = {
    computeStatus: createResource(async () => {
      const ticket = ++computeVersion;
      host.set({ computeLoading: true, computeError: null });
      try { return await ipc.request({ method: "compute_status" }); }
      finally { if (ticket === computeVersion) host.set({ computeLoading: false }); }
    }, (error) => {
      const value = error instanceof IpcFailure ? error.error : { code: "internal" as const, params: {}, retryable: true, detail: String(error) };
      host.set({ computeError: value });
    }),
    bootstrap: read({ method: "bootstrap" }), project: read({ method: "project_info" }),
    config: read({ method: "config_get" }), notices: read({ method: "config_validate" }), dsp: read({ method: "dsp_get" }),
    layout: read({ method: "layout_get" }), estimate: read({ method: "position_estimate" }), methods: read({ method: "measure_methods" }),
    measurementSettings: createStore<import("../ipc/bindings/MeasureConfigResult").MeasureConfigResult | null>(null),
    records: read({ method: "measurement_records" }), curves: read({ method: "measurement_curves", params: { positions: null, roles: null } }),
    levels: read({ method: "levels_get" }), settings: read({ method: "calibration_settings_get" }),
    micCalibration: read({ method: "mic_calibration_get" }),
    modeSpec: createResource(async (): Promise<import("../ipc/bindings/ModeSpecResult").ModeSpecResult> => {
      const settings = stores.settings.getSnapshot();
      if (!settings) throw new Error("calibration settings have not loaded");
      return ipc.request({ method: "mode_spec", params: { mode: settings.mode, ultra_ms: settings.ultra_ms, mimo_fir: settings.mimo_fir } });
    }, report),
    target: read({ method: "target_get" }), results: read({ method: "results" }),
    resultCurves,
    lines: read({ method: "result_lines" }), subWarnings: read({ method: "sub_warnings" }), rew: read({ method: "rew_info" }),
    folder: read({ method: "export_folder_get" }),
    // Read only while a target is set to 적용 (the export screen asks), never at startup:
    // it probes CamillaDSP's websocket and Equalizer APO's install.
    applyStatus: createResource(async () => {
      const ticket = ++applyVersion;
      host.set({ applyChecking: true });
      try { return await ipc.request({ method: "apply_status" }); }
      finally { if (ticket === applyVersion) host.set({ applyChecking: false }); }
    }, report),
    // The settings-failed screen retries this read: once it succeeds, the
    // notice its own earlier failure raised is withdrawn (a newer one stays).
    appSettings: createResource(async () => {
      const value = await ipc.request({ method: "app_settings_get" });
      if (appSettingsFailure !== null && errors.getSnapshot()?.id === appSettingsFailure) errors.set(null);
      appSettingsFailure = null;
      return value;
    }, (error) => { report(error); appSettingsFailure = errors.getSnapshot()?.id ?? null; }),
  };
  function clearProjectResources() {
    for (const key of ["project", "config", "notices", "dsp", "layout", "estimate", "measurementSettings", "records", "curves", "levels", "settings", "micCalibration", "modeSpec", "target", "results", "lines", "subWarnings", "folder", "applyStatus"] as const) {
      stores[key].set(null);
    }
  }
  function curveParams(): ResultCurvesParams {
    const h = host.getSnapshot();
    return { tab: h.tab, channel: h.channel, group: h.xoGroup, xo_hz: h.previewHz, corrected: null };
  }
  async function reload(keys: (keyof typeof stores)[]) {
    // mode_spec depends on the settings read, not a potentially stale snapshot.
    await Promise.all(keys.filter((key) => key !== "modeSpec" && key !== "measurementSettings").map((key) => {
      const store = stores[key]; return "load" in store ? store.load() : Promise.resolve();
    }));
    if (keys.includes("modeSpec") && stores.settings.getSnapshot()) await stores.modeSpec.load();
  }
  const jobs = createJobLedger({ request: (request) => ipc.request(request), now: Date.now, finished: completeJob, started: (job) => {
    if (job.kind === "level_monitor" && monitorWanted) monitorJob = job.job;
    if (job.epoch === jobs.epoch && ["calibrate", "mimo", "recommend_xo"].includes(job.kind)) host.set({ failure: null });
  } });
  let pendingProjectEdits = 0;
  const projectWork = new Map<Promise<boolean>, number>();
  function trackProjectEdit<T>(work: Promise<T>, epoch: number): Promise<T> {
    const done = work.then((value) => value !== false, () => false);
    projectWork.set(done, epoch);
    void done.then(() => projectWork.delete(done));
    return work;
  }
  function projectGuard() {
    const epoch = jobs.epoch;
    return { alive: () => epoch === jobs.epoch };
  }
  async function projectEditsSettled(): Promise<boolean> {
    const epoch = jobs.epoch;
    let succeeded = true;
    while (epoch === jobs.epoch) {
      const edits = [...projectWork].filter(([, owner]) => owner === epoch).map(([work]) => work);
      if (!edits.length) return succeeded;
      const results = await Promise.all(edits);
      succeeded = results.every(Boolean) && succeeded;
    }
    return false;
  }
  const call: SessionCall = (request, context) => {
    const owner = jobs.epoch;
    const work = track(async () => {
      let epoch = jobs.epoch;
      const saveDialog = request.method === "dialog_save_project" || request.method === "dialog_save_file";
      if (saveDialog) saveDialogs++;
      const projectEdit = request.method !== "project_save" && projectEdits.includes(request.method);
      if (projectEdit) pendingProjectEdits++;
      const pendingStart = jobs.begin(request);
      if (pendingStart?.context.method === "recommend_xo") {
        pendingStart.context.mimo = calibrationMimo(stores);
        if (context?.calibrateOnFailure) pendingStart.context.calibrateOnFailure = true;
      }
      if (pendingStart?.context.background && autoRefresh) pendingStart.context.autoRefreshRevision = autoRefresh.revision;
      if (refreshInputs.includes(request.method)) autoRefresh?.changed();
      const xoWasIncluded = stores.settings.getSnapshot()?.xo_rec_include !== false;
      const edit = request.method === "target_set_points" ? "targetEdit" : request.method === "calibration_settings_edit" && (request as Extract<Request, { method: "calibration_settings_edit" }>).params.edit.kind === "xo" ? "xoEdit" : null;
      if (edit) host.set({ [edit]: "saving" });
      try {
        // A monitor that cannot be stopped must not keep the next project from opening.
        if (request.method === "project_new" || request.method === "project_open") await monitor(false).catch(() => {});
        const data = await ipc.request(request);
        if (pendingStart) {
          jobs.reply(pendingStart, data as import("../ipc/bindings/JobStarted").JobStarted);
        }
        if (epoch !== jobs.epoch) return data;
        errors.set(null);
        if (request.method === "calibration_settings_edit" || request.method === "calibration_settings_set") {
          const settings = data as import("../ipc/bindings/CalibrationSettings").CalibrationSettings;
          const includeEdit = request.method === "calibration_settings_edit" && (request as Extract<Request, { method: "calibration_settings_edit" }>).params.edit.kind === "xo_rec_include";
          if (includeEdit || settings.xo_rec_include !== false) host.set({ xoRecTurnedOffByEdit: false });
          else if (edit === "xoEdit" && xoWasIncluded) host.set({ xoRecTurnedOffByEdit: true });
        }
        // Undo restores saved settings; the previous manual-edit notice is no longer current.
        if (["undo", "redo"].includes(request.method)) host.set({ xoRecTurnedOffByEdit: false });
        const effects = effectsOf({ request: request.method });
        const ctx: EffectContext = { host, stores, jobs, call, configure, run, canContinueRefresh, source: { request, data },
          replaceProject: () => { jobs.replaceProject(); clearProjectResources(); autoRefresh?.changed(); epoch = jobs.epoch; curveVersion++; curveState.set(null); tutorialReady.set(false); },
        };
        for (const rule of effects.rules) rule.beforeReload?.(ctx);
        await reload(effects.reload);
        if (epoch !== jobs.epoch) return data;
        for (const rule of effects.rules) if (rule.apply) await rule.apply(ctx);
        if (edit && epoch === jobs.epoch) host.set({ [edit]: "idle" });
        return data;
      } catch (error) {
        if (epoch === jobs.epoch) {
          if (edit) host.set({ [edit]: "failed" });
          // The mic calibration field owns typed action failures, including its
          // file dialog. Do not publish a second copy as a global notice.
          const micCalAction = request.method === "mic_calibration_set" || request.method === "mic_calibration_clear" || request.method === "mic_calibration_attach"
            || (request.method === "dialog_open_file" && (request as Extract<Request, { method: "dialog_open_file" }>).params.kind === "mic_calibration");
          // The export screen's service block owns the host actions' typed failures the same way.
          const hostAction = request.method === "host_install" || request.method === "host_uninstall" || request.method === "host_stop_managing";
          // The range card shows a refused edge or extra band at its input (ADR 0024).
          const rangeEdit = request.method === "calibration_settings_edit" && error instanceof IpcFailure
            && rangeRejection((request as Extract<Request, { method: "calibration_settings_edit" }>).params.edit, error.error) !== null;
          if (!(micCalAction || hostAction || rangeEdit) || !(error instanceof IpcFailure)) report(error);
        }
        throw error;
      } finally {
        if (projectEdit) pendingProjectEdits--;
        if (saveDialog) saveDialogs--;
        jobs.settle(pendingStart);
      }
    });
    return request.method !== "project_save" && projectEdits.includes(request.method) ? trackProjectEdit(work, owner) : work;
  };
  const run = (work: () => Promise<unknown>) => { void track(work).catch(() => { /* call reports the typed cause once */ }); };
  autoRefresh = createAutoRefresh({
    enabled: () => stores.appSettings.getSnapshot()?.auto_refresh === true,
    stale: () => stores.results.getSnapshot()?.calibration?.stale === true,
    blocked: () => pending > 0 || pendingProjectEdits > 0 || savePromptOpen,
    jobs: () => jobs.getSnapshot().jobs,
    start: () => run(() => startCalibration({ stores, call }, true)),
    cancel: (job) => run(() => jobs.cancel(job)),
  });
  jobs.subscribe(() => { queueMicrotask(() => autoRefresh?.check()); });
  stores.appSettings.subscribe(() => autoRefresh?.check());
  stores.results.subscribe(() => autoRefresh?.check());
  // One ordered writer per settings bundle (A-80, TD-68; see writers.ts). A
  // project bundle's edit counts as a pending project edit from the moment it
  // is queued, and one queued for a project that has since been replaced is
  // dropped (it resolves false) instead of landing in the new project.
  function writer<T>(deps: WriterDeps<T>, projectBundle: boolean): Writer<T> {
    const ordered = createWriter(deps);
    return {
      edit(patch) {
        const epoch = jobs.epoch;
        if (projectBundle) pendingProjectEdits++;
        const work = track(async () => {
          try {
            return await ordered.edit((latest) => {
              if (projectBundle && epoch !== jobs.epoch) throw new Error("the project was replaced before this edit was sent");
              return typeof patch === "function" ? patch(latest) : patch;
            });
          } finally { if (projectBundle) pendingProjectEdits--; }
        });
        return projectBundle ? trackProjectEdit(work, epoch) : work;
      },
    };
  }
  // measure_configure and layout_set leave a field sent as null as stored, so
  // their edits start from all-null params and carry only what they change.
  const measurementParams = (): MeasureConfigureParams => ({
    method: stores.measurementSettings.getSnapshot()?.method ?? "none", input_device: null, output_device: null, input_channel: null,
    output_channels: [], sweep_length: null, repetitions: null, sweep_level_dbfs: null, timing: null,
    loopback_input_channel: null, timing_ref_output: null, share_mode: null,
  });
  const layoutParams = (): LayoutEdit => ({ start_delay: null, timing: null });
  const writers = {
    /** app_settings_set replaces the whole object, which the compute choice, display settings and recent projects share. */
    app: writer<AppSettings>({
      latest: async () => {
        if (!stores.appSettings.getSnapshot()) return null;
        // External clients can save between notifications. Read inside the
        // ordered writer, so queued UI edits also use the current settings.
        const latest = await call({ method: "app_settings_get" });
        stores.appSettings.set(latest);
        return latest;
      },
      write: async (settings) => {
        const accepted = await call({ method: "app_settings_set", params: { settings } });
        stores.appSettings.set(accepted);
        return accepted;
      },
    }, false),
    /** apply_settings_set replaces AppSettings.apply as a whole: levels and the CamillaDSP address, built from the latest stored value. */
    apply: writer<ApplySettings>({
      latest: async () => {
        if (!stores.appSettings.getSnapshot()) return null;
        const latest = await call({ method: "app_settings_get" });
        stores.appSettings.set(latest);
        return latest.apply;
      },
      write: async (settings) => {
        const accepted = await call({ method: "apply_settings_set", params: settings });
        const current = stores.appSettings.getSnapshot();
        if (current) stores.appSettings.set({ ...current, apply: accepted });
        return accepted;
      },
    }, false),
    /** dsp_set takes a preset or the complete fields: `{ preset }` selects one, a field edit applies to the latest fields. */
    dsp: writer<DspEdit>({
      latest: () => {
        const dsp = stores.dsp.getSnapshot();
        return dsp ? { ...dsp.fields, preset: null } : null;
      },
      write: async ({ preset, ...fields }) => {
        const epoch = jobs.epoch;
        const accepted = await call({ method: "dsp_set", params: preset === null ? { preset: null, fields } : { preset, fields: null } });
        if (epoch === jobs.epoch) stores.dsp.set(accepted);
        return { ...accepted.fields, preset: null };
      },
    }, true),
    /** Start delay and timing reference; an arrangement change answers an undo token and stays a direct call. */
    layout: writer<LayoutEdit>({
      latest: layoutParams,
      write: async ({ start_delay, timing }) => {
        await call({ method: "layout_set", params: { count: null, start_delay, timing, confirm: false } });
        return layoutParams();
      },
    }, true),
    /** The method is the one the service last answered when the edit is sent. */
    measurement: writer<MeasureConfigureParams>({
      latest: measurementParams,
      write: async (params) => {
        await call({ method: "measure_configure", params });
        return measurementParams();
      },
    }, true),
  };
  /** The measurement writer's edit, under the name its many callers use. */
  function configure(patch: Patch<MeasureConfigureParams>) { return writers.measurement.edit(patch); }
  function startTutorial({ quest }: { quest?: string } = {}): Promise<void> {
    if (tutorialStart) return tutorialStart;
    // A gallery/reopened tutorial already has its fixture data. Prepare it
    // once without replacing it; a ready tutorial starts over on restart.
    const prepareExisting = stores.project.getSnapshot()?.tutorial && !tutorialReady.getSnapshot();
    tutorialReady.set(false);
    tutorialStart = track(async () => {
      let epoch = jobs.epoch;
      await monitor(false);
      if (epoch !== jobs.epoch) return;
      if (!prepareExisting) {
        await call({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } });
        epoch++;
      }
      if (epoch !== jobs.epoch || !stores.project.getSnapshot()?.tutorial) return;
      const systemQuest = !quest || quest.startsWith("system.");
      // No device or REW is opened as a side effect of a tutorial card.
      const configured = await writers.measurement.edit({ method: systemQuest || quest === "connect.retry" ? "none" : "mock" });
      if (epoch !== jobs.epoch) return;
      // The request reported its failure; the tutorial is not ready and can be started again.
      if (!configured) throw new IpcFailure(errors.getSnapshot()?.error ?? { code: "internal", params: {}, retryable: true, detail: "tutorial measurement settings were not saved" });
      const config = stores.config.getSnapshot();
      const index = systemQuest && config ? wrongOutput(config) : null;
      if (index !== null) await call({ method: "config_set_speaker", params: { index, role: null, size: null, band_index: 0, out: 16, pol: null } });
      if (epoch === jobs.epoch) tutorialReady.set(true);
    }).finally(() => { tutorialStart = null; });
    return tutorialStart;
  }
  function ensureSaved() { return track(async () => {
    const guard = projectGuard();
    if (stores.project.getSnapshot()?.path) return true;
    const path = await call({ method: "dialog_save_project", params: { suggested: "" } });
    if (!path || !guard.alive()) return false;
    await call({ method: "project_save", params: { path } }); return true;
  }); }
  function refreshCurves(params: ResultCurvesParams) { return track(async () => {
    const ticket = ++curveVersion;
    const previous = curveState.getSnapshot();
    curveState.set({ params: previous?.params ?? params, curves: previous?.curves ?? null, pending: params });
    try {
      const curves = await ipc.request({ method: "result_curves", params });
      if (ticket === curveVersion) curveState.set({ params, curves, pending: null });
    } catch (error) {
      if (ticket === curveVersion) {
        curveState.set({ params, curves: null, pending: null });
        report(error);
      }
    }
  }); }
  let monitorQueue = Promise.resolve();
  let monitoring = false;
  let monitorWanted = false;
  let monitorEpoch = 0;
  let monitorJob: number | null = null;
  let restartingMonitor = false;
  let meterTimer: ReturnType<typeof setTimeout> | undefined;
  // upstream wizard/mic_meter.py: POLL_S (0.25) * STALE_POLLS (4). Only REW
  // restarts on silence (monitor.rs rew_loop forwards its 0.25 s level
  // callbacks). The native monitor streams 0.1 s readings while idle (P138)
  // but pauses them for the length of a play, and the mock reads only during
  // a play, so a gap there is normal and never restarts them.
  const staleMeterMs = 1000;
  /** A failed monitor finish that carries no error of its own. */
  const unknownMonitorFailure: import("../ipc/bindings/IpcError").IpcError = { code: "internal", params: {}, retryable: true, detail: "level monitor failed without an error" };
  function clearMeterTimer() { clearTimeout(meterTimer); meterTimer = undefined; }
  function armMeterTimer() {
    clearMeterTimer();
    if (!monitorWanted || stores.measurementSettings.getSnapshot()?.method !== "rew") return;
    const epoch = monitorEpoch;
    meterTimer = setTimeout(() => {
      meterTimer = undefined;
      if (!monitorWanted || epoch !== monitorEpoch) return;
      host.set({ monitor: "stalled" });
      restartingMonitor = true;
      monitorQueue = monitorQueue.catch(() => {}).then(async () => {
        if (!monitorWanted || epoch !== monitorEpoch) return;
        await stopMonitor();
        if (monitorWanted && epoch === monitorEpoch) await startMonitor();
      }).catch((error) => {
        if (monitorWanted && epoch === monitorEpoch) host.set({ monitor: "offline" });
        throw error;
      }).finally(() => { restartingMonitor = false; });
      run(() => monitorQueue);
    }, staleMeterMs);
  }
  async function startMonitor() {
    const started = await call({ method: "level_monitor_start", params: { role: null } });
    monitorJob = started.job;
    monitoring = !jobs.getSnapshot().jobs.some((job) => job.job === started.job && !["running", "cancel_requested"].includes(job.status));
    if (monitoring) armMeterTimer();
  }
  async function stopMonitor() {
    clearMeterTimer();
    if (monitoring) {
      const stopped = await call({ method: "level_monitor_stop" });
      if (stopped && ["running", "cancel_requested"].includes(stopped.status)
          && !jobs.getSnapshot().jobs.some((job) => job.job === stopped.job && !["running", "cancel_requested"].includes(job.status))) {
        const finished = await new Promise<boolean>((resolve) => {
          const finish = (received: boolean) => { off(); clearTimeout(timer); resolve(received); };
          const off = ipc.events.subscribe((event) => {
            if (event.event === "finished" && event.job === stopped.job) finish(true);
          });
          const timer = setTimeout(() => finish(false), 5000);
        });
        if (!finished) {
          await jobs.readSnapshots({ reconcile: stopped.job });
          const current = jobs.getSnapshot().jobs.find((job) => job.job === stopped.job);
          if (current && isActiveJob(current.status)) {
            throw new IpcFailure({ code: "job_busy", params: {}, retryable: true, detail: "Level monitor is still stopping." });
          }
        }
      }
    }
    monitoring = false;
    monitorJob = null;
  }
  function monitor(active: boolean) {
    if (active !== monitorWanted) { monitorEpoch++; monitorWanted = active; }
    if (!active) clearMeterTimer();
    const epoch = monitorEpoch;
    monitorQueue = monitorQueue.catch(() => {}).then(async () => {
      if (active && (!monitorWanted || epoch !== monitorEpoch)) return;
      if (active && monitoring) return;
      if (active) {
        // Starting again (a play pressed while offline) drops the last failure.
        host.set({ monitor: "starting", monitorError: null });
        try { await startMonitor(); }
        catch (error) { host.set({ monitor: "offline" }); throw error; }
      } else {
        await stopMonitor();
        host.set({ monitor: "off", meter: null, playing: null, playingAll: false, monitorError: null });
      }
    });
    return track(() => monitorQueue);
  }
  function event(event: JobEvent) {
    if (event.event === "measure_status") { stores.methods.set(event.status); return; }
    if (event.event === "meter") {
      if (monitorWanted && event.job === monitorJob) {
        host.set({ meter: event, monitor: "receiving" });
        armMeterTimer();
      }
      return;
    }
    jobs.event(event);
    if (event.event !== "log") return;
    const job = jobs.getSnapshot().jobs.find((candidate) => candidate.job === event.job);
    if (!job || job.epoch !== jobs.epoch || !isActiveJob(job.status)) return;
    const effects = effectsOf({ log: event.code });
    const ctx: EffectContext = { host, stores, jobs, call, configure, run, canContinueRefresh, source: { job, event, request: job.request }, replaceProject: () => {} };
    run(async () => {
      for (const rule of effects.rules) rule.beforeReload?.(ctx);
      await reload(effects.reload);
      if (job.epoch !== jobs.epoch) return;
      for (const rule of effects.rules) if (rule.apply) await rule.apply(ctx);
    });
  }
  function completeJob(job: LedgerJob, event: FinishedJob, request: JobRequestContext | null) {
    if (job.kind === "level_monitor" && event.job === monitorJob) {
      monitoring = false;
      clearMeterTimer();
      if (event.status === "failed") {
        // Nothing plays any more and the last reading is not current; the
        // job's error says why on the level check (#120). Not a global
        // notice: the monitor is hidden from the job tray and the panel owns it.
        host.set({ monitor: "offline", monitorError: event.error ?? unknownMonitorFailure, meter: null, playing: null, playingAll: false });
      } else if (!restartingMonitor) host.set({ monitor: "off" });
    }
    if ((request?.epoch ?? job.epoch) !== jobs.epoch) return;
    const effects = effectsOf({ finished: job.kind, status: event.status });
    const ctx: EffectContext = { host, stores, jobs, call, configure, run, canContinueRefresh, source: { job, event, request }, replaceProject: () => {} };
    // A failed/cancelled job may already have committed some measurements.
    run(async () => {
      for (const rule of effects.rules) rule.beforeReload?.(ctx);
      await reload(effects.reload);
      if (job.epoch !== jobs.epoch) return;
      if (refreshMeasurements.includes(job.kind)) autoRefresh?.changed();
      // External jobs refresh state, but do not repeat the UI's chained writes.
      if (request) for (const rule of effects.rules) if (rule.apply) await rule.apply(ctx);
    });
  }

  let externalBatch: ReturnType<typeof coalesceExternalChanges> | null = null;
  let offExternal: (() => void) | null = null;
  let externalPoll: ReturnType<typeof setTimeout> | undefined;
  const hasExternalJobs = () => jobs.getSnapshot().jobs.some((job) => job.epoch === jobs.epoch && !job.request && isActiveJob(job.status) && !["status_watch", "level_monitor"].includes(job.kind));
  async function refreshExternal(methods: readonly string[]) {
    clearTimeout(externalPoll);
    const plans = methods.map(externalRefresh);
    if (methods.some((method) => refreshInputs.includes(method as Request["method"]))) autoRefresh?.changed();
    if (plans.some((plan) => plan.replaceProject)) {
      jobs.replaceProject(); clearProjectResources(); autoRefresh?.changed(); curveVersion++; curveState.set(null); tutorialReady.set(false);
      for (const store of Object.values(stores)) if ("invalidate" in store) store.invalidate();
      stores.measurementSettings.set(null); stores.micCalibration.set(null);
      host.set({ micCalBusy: null, micCalError: null, path: null, selected: null, mdat: null, warnings: [], runs: {}, applyRuns: {}, mdatScope: "all", channel: null, xoGroup: null, xoRecommendation: null, xoRecTurnedOffByEdit: false, failure: null, rangeRejection: null, opened: host.getSnapshot().opened + 1 });
    }
    const keys = [...new Set(plans.flatMap((plan) => plan.resources))];
    await Promise.all([
      reload(keys),
      call({ method: "measure_config_get" }).catch(() => {}),
      jobs.readSnapshots({ external: true }).catch(report),
    ]);
    // The shell sends external mutations, but external jobs have no UI
    // Channel. Poll only while such jobs run, to recover their final results.
    if (externalBatch && hasExternalJobs()) {
      externalPoll = setTimeout(() => externalBatch?.push("jobs"), 1000);
    }
  }

  let off: (() => void) | null = null;
  let startPromise: Promise<void> | null = null;
  let lifecycle = 0;
  async function start() {
    lifecycle++;
    autoRefresh?.start();
    if (startPromise) return startPromise;
    off = ipc.events.subscribe(event);
    startPromise = track(async () => {
      if (ipc.externalChanges) {
        externalBatch = coalesceExternalChanges((methods) => track(() => refreshExternal(methods)), report);
        try { offExternal = await ipc.externalChanges.subscribe((method) => externalBatch?.push(method)); }
        catch (error) { report(error); }
      }
      await Promise.all([stores.bootstrap.load(), stores.appSettings.load(), stores.computeStatus.load(), stores.methods.load(),
        jobs.readSnapshots().catch(report),
      ]);
      try { stores.project.set(await ipc.request({ method: "project_info" })); }
      catch (error) { report(error); }
      if (stores.project.getSnapshot()) {
        await reload(Object.keys(stores).filter((key) => !["bootstrap", "measurementSettings", "appSettings", "computeStatus", "methods", "rew", "applyStatus"].includes(key)) as (keyof typeof stores)[]);
        // A window reloaded over a project the service still holds reads its
        // stored method, as opening it does; call() reports a failure.
        await call({ method: "measure_config_get" }).catch(() => {});
      }
      ready.set(true);
      if (hasExternalJobs()) externalBatch?.push("jobs");
      run(() => call({ method: "status_watch_start" }));
    });
    return startPromise;
  }
  function dispose() {
    autoRefresh?.dispose();
    const ticket = ++lifecycle;
    // StrictMode's immediate setup/cleanup/setup must not stop the new watch.
    run(async () => {
      await new Promise<void>((resolve) => queueMicrotask(resolve));
      if (ticket !== lifecycle) return;
      await Promise.resolve(startPromise).catch(() => {});
      if (ticket !== lifecycle) return;
      off?.(); off = null;
      offExternal?.(); offExternal = null;
      externalBatch?.dispose(); externalBatch = null;
      clearTimeout(externalPoll); externalPoll = undefined;
      await monitor(false).catch(() => {});
      await ipc.request({ method: "status_watch_stop" }).catch(() => {});
      startPromise = null;
    });
  }
  return { ipc, stores, host, jobs, errors, ready, tutorialReady, call, run, settled, writers, configure, startTutorial, refreshCurves, ensureSaved, monitor, start, dispose,
    canContinueRefresh, projectEditsSettled, projectGuard,
    setSavePromptOpen: (open: boolean) => { savePromptOpen = open; autoRefresh?.check(); },
    hasPendingProjectEdits: () => pendingProjectEdits > 0,
    navigate: (current: StepId) => host.set({ current }) };
}
export type Session = ReturnType<typeof createSession>;
