import type { StepId } from "../app/model";
import { IpcFailure } from "../ipc/client";
import type { Ipc, RequestFn } from "../ipc/context";
import type { Request } from "../ipc/bindings/Request";
import type { JobEvent } from "../ipc/bindings/JobEvent";
import type { MeasureConfigureParams } from "../ipc/bindings/MeasureConfigureParams";
import type { ResultCurvesParams } from "../ipc/bindings/ResultCurvesParams";
import type { ResultCurvesResult } from "../ipc/bindings/ResultCurvesResult";
import type { ExportKind } from "../ipc/bindings/ExportKind";
import type { LevelResult } from "../ipc/bindings/LevelResult";
import type { XoRecommendation } from "../ipc/bindings/XoRecommendation";
import type { ConnectPath, MeterReading, MonitorState, RewProblem } from "../screens/connect/types";
import type { CalibrateView } from "../screens/calibrate/types";
import type { ExportRun } from "../screens/export/types";
import type { MeasureView } from "../screens/measure/types";
import { createResource, createStore, type Failure } from "./store";
import { isActiveJob } from "./jobRetention";
import { createJobTracker, type FinishedJob, type JobRequestContext, type SessionJob } from "./jobTracker";
import { HOST_TRIGGERS, TRIGGERS, triggeredBy } from "./triggers";

type HostState = Pick<MeasureView, "selected" | "graphHidden" | "plotRange" | "mdat" | "warnings"> & {
  computeLoading: boolean; computeSaving: boolean; computeError: import("../ipc/bindings/IpcError").IpcError | null;
  current: StepId; path: ConnectPath | null;
  tab: CalibrateView["tab"]; channel: string | null; xoGroup: string | null; previewHz: number | null;
  targetEdit: CalibrateView["targetEdit"]; xoEdit: CalibrateView["xoEdit"];
  jobs: SessionJob[]; jobRequests: Record<number, JobRequestContext>;
  meter: MeterReading | null; monitor: MonitorState; playing: string | null; playingAll: boolean; masterDbfs: number;
  sweepResult: LevelResult | null; sweepRole: string | null;
  xoRecommendation: XoRecommendation | null; runs: Partial<Record<ExportKind, ExportRun>>;
  failure: CalibrateView["failure"]; rewLaunch: "idle" | "checking" | "launching";
  rewProblem: RewProblem | null; rewProblemDetail: string | null; readingRew: boolean;
  switchingRewDriver: boolean;
  /** Projects opened or created in this window; screens reset what they hid for the previous one when it changes. */
  opened: number;
};

export function createSession(ipc: Ipc) {
  const rawHost = createStore<HostState>({
    computeLoading: true, computeSaving: false, computeError: null,
    current: "system", path: null, selected: null, graphHidden: false, plotRange: { x: null, y: null },
    mdat: null, warnings: [], tab: "spl", channel: null, xoGroup: null, previewHz: null, targetEdit: "idle", xoEdit: "idle",
    jobs: [], jobRequests: {}, meter: null, monitor: "off", playing: null, playingAll: false, masterDbfs: -20,
    sweepResult: null, sweepRole: null, xoRecommendation: null, runs: {}, failure: null,
    rewLaunch: "idle", rewProblem: null, rewProblemDetail: null, readingRew: false, switchingRewDriver: false, opened: 0,
  });
  const host = { ...rawHost, set: (patch: Partial<HostState>) => rawHost.set({ ...rawHost.getSnapshot(), ...patch }) };
  const errors = createStore<Failure | null>(null);
  const ready = createStore(false);
  let errorId = 0;
  const report = (error: unknown) => {
    const value = error instanceof IpcFailure ? error.error : { code: "internal" as const, params: {}, retryable: false, detail: String(error) };
    if (value.code !== "project_not_open") errors.set({ id: ++errorId, error: value });
  };
  const read = <M extends Request["method"]>(request: Extract<Request, { method: M }>) => createResource(() => ipc.request(request), report);
  const curveState = createStore<{ params: ResultCurvesParams; curves: ResultCurvesResult | null; pending: ResultCurvesParams | null } | null>(null);
  let curveVersion = 0;
  const resultCurves = { ...curveState, load: () => refreshCurves(curveParams()) };
  let computeVersion = 0;
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
    modeSpec: createResource(async (): Promise<import("../ipc/bindings/ModeSpecResult").ModeSpecResult> => {
      const settings = stores.settings.getSnapshot();
      if (!settings) throw new Error("calibration settings have not loaded");
      return ipc.request({ method: "mode_spec", params: { mode: settings.mode, ultra_ms: settings.ultra_ms, mimo_fir: settings.mimo_fir } });
    }, report),
    target: read({ method: "target_get" }), results: read({ method: "results" }),
    resultCurves,
    lines: read({ method: "result_lines" }), subWarnings: read({ method: "sub_warnings" }), rew: read({ method: "rew_info" }),
    folder: read({ method: "export_folder_get" }), appSettings: read({ method: "app_settings_get" }),
  };
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
  const tracker = createJobTracker(host.getSnapshot, (state) => host.set(state), completeJob, (job) => {
    if (job.kind === "level_monitor" && monitorWanted) monitorJob = job.job;
    if (job.epoch === tracker.epoch && ["calibrate", "mimo", "recommend_xo"].includes(job.kind)) host.set({ failure: null });
  });
  let pendingProjectEdits = 0;
  const call: RequestFn = async (request) => {
    let epoch = tracker.epoch;
    const projectEdit = request.method !== "project_save" && (TRIGGERS.project.mutations as readonly string[]).includes(request.method);
    if (projectEdit) pendingProjectEdits++;
    const methodBefore = stores.measurementSettings.getSnapshot()?.method;
    const pendingStart = tracker.begin(request);
    const edit = request.method === "target_set_points" ? "targetEdit" : request.method === "calibration_settings_edit" && (request as Extract<Request, { method: "calibration_settings_edit" }>).params.edit.kind === "xo" ? "xoEdit" : null;
    if (edit) host.set({ [edit]: "saving" });
    try {
      // A monitor that cannot be stopped must not keep the next project from opening.
      if (request.method === "project_new" || request.method === "project_open") await monitor(false).catch(() => {});
      const data = await ipc.request(request);
      if (pendingStart) {
        tracker.reply(pendingStart, data as import("../ipc/bindings/JobStarted").JobStarted);
      }
      if (epoch !== tracker.epoch) return data;
      errors.set(null);
      if (request.method === "measure_configure" || request.method === "measure_config_get" || request.method === "level_apply") {
        stores.measurementSettings.set(data as import("../ipc/bindings/MeasureConfigResult").MeasureConfigResult);
        const method = stores.measurementSettings.getSnapshot()!.method;
        host.set({ path: method === "none" ? null : method as ConnectPath });
      }
      if (request.method === "project_new" || request.method === "project_open") {
        tracker.replaceProject();
        epoch = tracker.epoch;
        curveVersion++;
        curveState.set(null);
        stores.measurementSettings.set(null);
        host.set({ path: null, selected: null, mdat: null, warnings: [], runs: {}, channel: null, xoGroup: null, xoRecommendation: null, failure: null, sweepResult: null, sweepRole: null, opened: host.getSnapshot().opened + 1 });
      }
      // The recommended sweep level was measured with one method, device and channel; another makes it stale.
      if (request.method === "measure_configure") {
        const params = (request as Extract<Request, { method: "measure_configure" }>).params;
        if (params.method !== methodBefore || params.input_device !== null || params.input_channel !== null) host.set({ sweepResult: null, sweepRole: null });
      }
      // Low-SNR warnings belong to one run of a position (upstream lists them per run): a new run, a clear or an import replaces them.
      if (request.method === "measure_position" || request.method === "position_clear" || request.method === "mdat_import") {
        const params = (request as Extract<Request, { method: "measure_position" | "position_clear" | "mdat_import" }>).params;
        const positions = new Set<number>([params.position, ...("assignments" in params ? params.assignments.flatMap((a) => (a.position === null ? [] : [a.position])) : [])]);
        host.set({ warnings: host.getSnapshot().warnings.filter((w) => !positions.has(w.position)) });
      }
      if (request.method === "sample_load") host.set({ warnings: [] });
      // rew_driver_set answers with REW's facts read after the switch; another driver also makes the recommended level stale.
      if (request.method === "rew_driver_set") {
        stores.rew.set(data as import("../ipc/bindings/RewInfoResult").RewInfoResult);
        host.set({ sweepResult: null, sweepRole: null });
      }
      await reload(triggeredBy({ mutation: request.method }));
      if (epoch !== tracker.epoch) return data;
      if ((HOST_TRIGGERS.measurementSettings.mutations as readonly string[]).includes(request.method) && stores.measurementSettings.getSnapshot()) await call({ method: "measure_config_get" });
      if (request.method === "project_new" && (request as Extract<Request, { method: "project_new" }>).params.tutorial) await configure({ method: "mock" });
      // A new project stores "none"; a reopened one keeps its method and devices.
      else if (request.method === "project_new" || request.method === "project_open") await call({ method: "measure_config_get" });
      if (edit && epoch === tracker.epoch) host.set({ [edit]: "idle" });
      return data;
    } catch (error) {
      if (epoch === tracker.epoch) {
        if (edit) host.set({ [edit]: "failed" });
        report(error);
      }
      throw error;
    } finally {
      if (projectEdit) pendingProjectEdits--;
      tracker.settle(pendingStart);
    }
  };
  const run = (work: () => Promise<unknown>) => { void work().catch(() => { /* call reports the typed cause once */ }); };
  async function configure(patch: Partial<MeasureConfigureParams>) {
    return call({ method: "measure_configure", params: {
      method: stores.measurementSettings.getSnapshot()?.method ?? "none", input_device: null, output_device: null, input_channel: null,
      output_channels: [], sweep_length: null, repetitions: null, sweep_level_dbfs: null, timing: null,
      loopback_input_channel: null, timing_ref_output: null, share_mode: null, ...patch,
    } });
  }
  async function ensureSaved() {
    if (stores.project.getSnapshot()?.path) return true;
    const path = await call({ method: "dialog_save_project", params: { suggested: "" } });
    if (!path) return false;
    await call({ method: "project_save", params: { path } }); return true;
  }
  async function refreshCurves(params: ResultCurvesParams) {
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
  }
  let monitorQueue = Promise.resolve();
  let monitoring = false;
  let monitorWanted = false;
  let monitorEpoch = 0;
  let monitorJob: number | null = null;
  let restartingMonitor = false;
  let meterTimer: ReturnType<typeof setTimeout> | undefined;
  // upstream wizard/mic_meter.py: POLL_S (0.25) * STALE_POLLS (4). Only REW
  // streams readings while idle (monitor.rs rew_loop forwards its 0.25 s
  // level callbacks); the native and mock monitors read only during a play,
  // so silence there is normal and never restarts them.
  const staleMeterMs = 1000;
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
    monitoring = !host.getSnapshot().jobs.some((job) => job.job === started.job && !["running", "cancel_requested"].includes(job.status));
    if (monitoring) armMeterTimer();
  }
  async function stopMonitor() {
    clearMeterTimer();
    if (monitoring) {
      const stopped = await call({ method: "level_monitor_stop" });
      if (stopped && ["running", "cancel_requested"].includes(stopped.status)
          && !host.getSnapshot().jobs.some((job) => job.job === stopped.job && !["running", "cancel_requested"].includes(job.status))) {
        const finished = await new Promise<boolean>((resolve) => {
          const finish = (received: boolean) => { off(); clearTimeout(timer); resolve(received); };
          const off = ipc.events.subscribe((event) => {
            if (event.event === "finished" && event.job === stopped.job) finish(true);
          });
          const timer = setTimeout(() => finish(false), 5000);
        });
        if (!finished) {
          const jobs = await call({ method: "jobs" });
          const current = jobs.find((job) => job.job === stopped.job);
          if (current && isActiveJob(current.status)) {
            throw new IpcFailure({ code: "job_busy", params: {}, retryable: true, detail: "Level monitor is still stopping." });
          }
          if (current) tracker.finish({ event: "finished", job: current.job, status: current.status, result: null, error: current.error });
          else host.set({ jobs: host.getSnapshot().jobs.filter((job) => job.job !== stopped.job) });
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
        host.set({ monitor: "starting" });
        try { await startMonitor(); }
        catch (error) { host.set({ monitor: "offline" }); throw error; }
      } else {
        await stopMonitor();
        host.set({ monitor: "off", meter: null, playing: null, playingAll: false });
      }
    });
    return monitorQueue;
  }
  function event(event: JobEvent) {
    const h = host.getSnapshot();
    if (event.event === "measure_status") { stores.methods.set(event.status); return; }
    if (event.event === "meter") {
      if (monitorWanted && event.job === monitorJob) {
        host.set({ meter: event, monitor: "receiving" });
        armMeterTimer();
      }
      return;
    }
    if (event.event === "started") { tracker.observeStart(event.job, event.kind); return; }
    if (event.event === "progress") { tracker.progress(event); return; }
    if (event.event === "finished") { tracker.finish(event); return; }
    const job = h.jobs.find((candidate) => candidate.job === event.job);
    if (!job || job.epoch !== tracker.epoch || !isActiveJob(job.status)) return;
    const request = h.jobRequests[event.job];
    if (event.event === "log") {
      if (event.code === "measurement_low_snr" && request?.method === "measure_position") {
        const position = request.params.position;
        const old = h.warnings.find((w) => w.position === position);
        host.set({ warnings: [...h.warnings.filter((w) => w.position !== position), { position, notices: [...(old?.notices ?? []), { code: event.code, params: event.params }] }] });
      }
      return;
    }
  }
  function completeJob(job: SessionJob, event: FinishedJob, request: JobRequestContext | undefined) {
    const h = host.getSnapshot();
    if (job.kind === "level_monitor" && event.job === monitorJob) {
      monitoring = false;
      clearMeterTimer();
      if (!restartingMonitor) host.set({ monitor: event.status === "failed" ? "offline" : "off" });
    }
    if ((request?.epoch ?? job.epoch) !== tracker.epoch) return;
    // A cancelled job also carries an error (code `cancelled`); only a failure is one.
    if (event.status === "failed" && event.error && ["calibrate", "mimo", "recommend_xo"].includes(job.kind)) {
      host.set({ failure: { job: request?.method === "calibrate" && request.params.subs_only ? "calibrate_subs" : job.kind as "calibrate" | "mimo" | "recommend_xo", error: event.error } });
    }
    if (job.kind === "measure") host.set({ selected: null });
    if (event.result?.kind === "level") host.set({ sweepResult: event.result.data });
    if (event.result?.kind === "xo") host.set({ xoRecommendation: event.result.data });
    if (event.status === "succeeded" && (job.kind === "calibrate" || job.kind === "mimo")) host.set({ runs: {} });
    if (request?.method === "export") {
      if (event.result?.kind === "export") {
        const result = event.result.data;
        host.set({ runs: { ...h.runs, [request.params.kind]: { status: "done", files: result.files, folder: result.folder } } });
        run(() => call({ method: "open_folder", params: { path: result.folder } }));
      } else if (event.status === "failed" && event.error) host.set({ runs: { ...h.runs, [request.params.kind]: { status: "failed", error: event.error } } });
    }
    // A failed/cancelled job may already have committed some measurements.
    run(async () => {
      await reload(triggeredBy({ finished: job.kind }));
      if (job.kind === "recommend_xo" && event.status === "succeeded" && job.epoch === tracker.epoch) {
        await call({ method: "calibrate", params: { settings: null, subs_only: false } });
      }
    });
  }
  let off: (() => void) | null = null;
  let startPromise: Promise<void> | null = null;
  let lifecycle = 0;
  async function start() {
    lifecycle++;
    if (startPromise) return startPromise;
    off = ipc.events.subscribe(event);
    startPromise = (async () => {
      const jobRead = tracker.beginSnapshots();
      await Promise.all([stores.bootstrap.load(), stores.appSettings.load(), stores.computeStatus.load(), stores.methods.load(),
        ipc.request({ method: "jobs" }).then((jobs) => tracker.snapshots(jobs, jobRead)).catch(report).finally(() => tracker.endSnapshots(jobRead)),
      ]);
      try { stores.project.set(await ipc.request({ method: "project_info" })); }
      catch (error) { report(error); }
      if (stores.project.getSnapshot()) {
        await reload(Object.keys(stores).filter((key) => !["bootstrap", "measurementSettings", "appSettings", "computeStatus", "methods", "rew"].includes(key)) as (keyof typeof stores)[]);
        // A window reloaded over a project the service still holds reads its
        // stored method, as opening it does; call() reports a failure.
        await call({ method: "measure_config_get" }).catch(() => {});
      }
      ready.set(true);
      run(() => call({ method: "status_watch_start" }));
    })();
    return startPromise;
  }
  function dispose() {
    const ticket = ++lifecycle;
    // StrictMode's immediate setup/cleanup/setup must not stop the new watch.
    queueMicrotask(() => {
      if (ticket !== lifecycle) return;
      const pending = startPromise;
      void Promise.resolve(pending).catch(() => {}).then(async () => {
        if (ticket !== lifecycle) return;
        off?.(); off = null;
        await monitor(false).catch(() => {});
        await ipc.request({ method: "status_watch_stop" }).catch(() => {});
        startPromise = null;
      });
    });
  }
  return { ipc, stores, host, errors, ready, call, run, configure, refreshCurves, ensureSaved, monitor, start, dispose,
    hasPendingProjectEdits: () => pendingProjectEdits > 0,
    navigate: (current: StepId) => host.set({ current }) };
}
export type Session = ReturnType<typeof createSession>;
