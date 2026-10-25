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
import { createResource as resource, createStore, type Failure } from "./store";
import { isActiveJob } from "./jobRetention";
import { createJobLedger, type FinishedJob, type JobRequestContext, type LedgerJob } from "./jobLedger";
import { effectsOf, projectEdits, type EffectContext } from "./effects";

type HostState = Pick<MeasureView, "selected" | "graphHidden" | "plotRange" | "mdat" | "warnings"> & {
  computeLoading: boolean; computeSaving: boolean; computeError: import("../ipc/bindings/IpcError").IpcError | null;
  current: StepId; path: ConnectPath | null;
  tab: CalibrateView["tab"]; channel: string | null; xoGroup: string | null; previewHz: number | null;
  targetEdit: CalibrateView["targetEdit"]; xoEdit: CalibrateView["xoEdit"];
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
  let pending = 0;
  const waiters = new Set<() => void>();
  async function track<T>(work: () => Promise<T>): Promise<T> {
    pending++;
    try { return await work(); }
    finally {
      if (--pending === 0) { for (const resolve of waiters) resolve(); waiters.clear(); }
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
    meter: null, monitor: "off", playing: null, playingAll: false, masterDbfs: -20,
    sweepResult: null, sweepRole: null, xoRecommendation: null, runs: {}, failure: null,
    rewLaunch: "idle", rewProblem: null, rewProblemDetail: null, readingRew: false, switchingRewDriver: false, opened: 0,
  });
  const host = { ...rawHost, set: (patch: Partial<HostState>) => rawHost.set({ ...rawHost.getSnapshot(), ...patch }) };
  const errors = createStore<Failure | null>(null);
  const ready = createStore(false);
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
    folder: read({ method: "export_folder_get" }),
    // The settings-failed screen retries this read: once it succeeds, the
    // notice its own earlier failure raised is withdrawn (a newer one stays).
    appSettings: createResource(async () => {
      const value = await ipc.request({ method: "app_settings_get" });
      if (appSettingsFailure !== null && errors.getSnapshot()?.id === appSettingsFailure) errors.set(null);
      appSettingsFailure = null;
      return value;
    }, (error) => { report(error); appSettingsFailure = errors.getSnapshot()?.id ?? null; }),
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
  const jobs = createJobLedger({ request: (request) => ipc.request(request), now: Date.now, finished: completeJob, started: (job) => {
    if (job.kind === "level_monitor" && monitorWanted) monitorJob = job.job;
    if (job.epoch === jobs.epoch && ["calibrate", "mimo", "recommend_xo"].includes(job.kind)) host.set({ failure: null });
  } });
  let pendingProjectEdits = 0;
  const call: RequestFn = (request) => track(async () => {
    let epoch = jobs.epoch;
    const projectEdit = request.method !== "project_save" && projectEdits.includes(request.method);
    if (projectEdit) pendingProjectEdits++;
    const methodBefore = stores.measurementSettings.getSnapshot()?.method;
    const pendingStart = jobs.begin(request);
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
      const effects = effectsOf({ request: request.method });
      const ctx: EffectContext = { host, stores, jobs, call, configure, run, source: { request, data, methodBefore },
        replaceProject: () => { jobs.replaceProject(); epoch = jobs.epoch; curveVersion++; curveState.set(null); },
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
        report(error);
      }
      throw error;
    } finally {
      if (projectEdit) pendingProjectEdits--;
      jobs.settle(pendingStart);
    }
  });
  const run = (work: () => Promise<unknown>) => { void track(work).catch(() => { /* call reports the typed cause once */ }); };
  async function configure(patch: Partial<MeasureConfigureParams>) {
    return call({ method: "measure_configure", params: {
      method: stores.measurementSettings.getSnapshot()?.method ?? "none", input_device: null, output_device: null, input_channel: null,
      output_channels: [], sweep_length: null, repetitions: null, sweep_level_dbfs: null, timing: null,
      loopback_input_channel: null, timing_ref_output: null, share_mode: null, ...patch,
    } });
  }
  function ensureSaved() { return track(async () => {
    if (stores.project.getSnapshot()?.path) return true;
    const path = await call({ method: "dialog_save_project", params: { suggested: "" } });
    if (!path) return false;
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
        host.set({ monitor: "starting" });
        try { await startMonitor(); }
        catch (error) { host.set({ monitor: "offline" }); throw error; }
      } else {
        await stopMonitor();
        host.set({ monitor: "off", meter: null, playing: null, playingAll: false });
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
    const ctx: EffectContext = { host, stores, jobs, call, configure, run, source: { job, event, request: job.request }, replaceProject: () => {} };
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
      if (!restartingMonitor) host.set({ monitor: event.status === "failed" ? "offline" : "off" });
    }
    if ((request?.epoch ?? job.epoch) !== jobs.epoch) return;
    const effects = effectsOf({ finished: job.kind, status: event.status });
    const ctx: EffectContext = { host, stores, jobs, call, configure, run, source: { job, event, request }, replaceProject: () => {} };
    // A failed/cancelled job may already have committed some measurements.
    run(async () => {
      for (const rule of effects.rules) rule.beforeReload?.(ctx);
      await reload(effects.reload);
      if (job.epoch !== jobs.epoch) return;
      for (const rule of effects.rules) if (rule.apply) await rule.apply(ctx);
    });
  }

  let off: (() => void) | null = null;
  let startPromise: Promise<void> | null = null;
  let lifecycle = 0;
  async function start() {
    lifecycle++;
    if (startPromise) return startPromise;
    off = ipc.events.subscribe(event);
    startPromise = track(async () => {
      await Promise.all([stores.bootstrap.load(), stores.appSettings.load(), stores.computeStatus.load(), stores.methods.load(),
        jobs.readSnapshots().catch(report),
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
    });
    return startPromise;
  }
  function dispose() {
    const ticket = ++lifecycle;
    // StrictMode's immediate setup/cleanup/setup must not stop the new watch.
    run(async () => {
      await new Promise<void>((resolve) => queueMicrotask(resolve));
      if (ticket !== lifecycle) return;
      await Promise.resolve(startPromise).catch(() => {});
      if (ticket !== lifecycle) return;
      off?.(); off = null;
      await monitor(false).catch(() => {});
      await ipc.request({ method: "status_watch_stop" }).catch(() => {});
      startPromise = null;
    });
  }
  return { ipc, stores, host, jobs, errors, ready, call, run, settled, configure, refreshCurves, ensureSaved, monitor, start, dispose,
    hasPendingProjectEdits: () => pendingProjectEdits > 0,
    navigate: (current: StepId) => host.set({ current }) };
}
export type Session = ReturnType<typeof createSession>;
