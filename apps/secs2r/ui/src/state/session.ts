import type { StepId } from "../app/model";
import { IpcFailure } from "../ipc/client";
import type { Ipc, RequestFn } from "../ipc/context";
import type { Request } from "../ipc/bindings/Request";
import type { JobSnapshot } from "../ipc/bindings/JobSnapshot";
import type { JobEvent } from "../ipc/bindings/JobEvent";
import type { MeasureConfigureParams } from "../ipc/bindings/MeasureConfigureParams";
import type { ResultCurvesParams } from "../ipc/bindings/ResultCurvesParams";
import type { ExportKind } from "../ipc/bindings/ExportKind";
import type { LevelResult } from "../ipc/bindings/LevelResult";
import type { XoRecommendation } from "../ipc/bindings/XoRecommendation";
import type { ConnectPath, MeterReading, MonitorState, RewProblem } from "../screens/connect/types";
import type { CalibrateView } from "../screens/calibrate/types";
import type { ExportRun } from "../screens/export/types";
import type { MeasureView } from "../screens/measure/types";
import { createResource, createStore, type Failure } from "./store";
import { HOST_TRIGGERS, triggeredBy } from "./triggers";

export type HostState = Pick<MeasureView, "selected" | "graphHidden" | "plotRange" | "mdat" | "warnings"> & {
  current: StepId; path: ConnectPath | null;
  tab: CalibrateView["tab"]; channel: string | null; xoGroup: string | null; previewHz: number | null;
  jobs: JobSnapshot[]; jobRequests: Record<number, Request>;
  meter: MeterReading | null; monitor: MonitorState; playing: string | null; playingAll: boolean; masterDbfs: number;
  sweepResult: LevelResult | null; sweepRole: string | null;
  xoRecommendation: XoRecommendation | null; runs: Partial<Record<ExportKind, ExportRun>>;
  failure: CalibrateView["failure"]; rewLaunch: "idle" | "checking" | "launching";
  rewProblem: RewProblem | null; rewProblemDetail: string | null; readingRew: boolean;
};

export function createSession(ipc: Ipc) {
  const rawHost = createStore<HostState>({
    current: "system", path: null, selected: null, graphHidden: false, plotRange: { x: null, y: null },
    mdat: null, warnings: [], tab: "spl", channel: null, xoGroup: null, previewHz: null,
    jobs: [], jobRequests: {}, meter: null, monitor: "off", playing: null, playingAll: false, masterDbfs: -20,
    sweepResult: null, sweepRole: null, xoRecommendation: null, runs: {}, failure: null,
    rewLaunch: "idle", rewProblem: null, rewProblemDetail: null, readingRew: false,
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
  const stores = {
    bootstrap: read({ method: "bootstrap" }), project: read({ method: "project_info" }),
    config: read({ method: "config_get" }), notices: read({ method: "config_validate" }), dsp: read({ method: "dsp_get" }),
    layout: read({ method: "layout_get" }), estimate: read({ method: "position_estimate" }), methods: read({ method: "measure_methods" }),
    measurementSettings: createStore<import("../ipc/bindings/MeasureConfigResult").MeasureConfigResult | null>(null),
    records: read({ method: "measurement_records" }), curves: read({ method: "measurement_curves", params: { positions: null, roles: null } }),
    levels: read({ method: "levels_get" }), settings: read({ method: "calibration_settings_get" }),
    modeSpec: createResource(async (): Promise<import("../ipc/bindings/ModeSpecResult").ModeSpecResult> => {
      const settings = stores.settings.getSnapshot();
      if (!settings) throw new Error("calibration settings have not loaded");
      return ipc.request({ method: "mode_spec", params: { mode: settings.mode, ultra_ms: settings.ultra_ms } });
    }, report),
    target: read({ method: "target_get" }), results: read({ method: "results" }),
    resultCurves: createResource(() => ipc.request({ method: "result_curves", params: curveParams() }), report),
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
  const earlyFinishes = new Map<number, Extract<JobEvent, { event: "finished" }>>();
  const call: RequestFn = async (request) => {
    try {
      const data = await ipc.request(request);
      errors.set(null);
      if (request.method === "measure_configure" || request.method === "measure_config_get" || request.method === "level_apply") {
        stores.measurementSettings.set(data as import("../ipc/bindings/MeasureConfigResult").MeasureConfigResult);
        const method = stores.measurementSettings.getSnapshot()!.method;
        host.set({ path: method === "none" ? null : method as ConnectPath });
      }
      if (["measure_position", "sample_load", "mdat_import", "calibrate", "recommend_xo", "mimo", "export", "level_check"].includes(request.method)) {
        const started = data as import("../ipc/bindings/JobStarted").JobStarted;
        host.set({ jobRequests: { ...host.getSnapshot().jobRequests, [started.job]: request } });
        const early = earlyFinishes.get(started.job);
        if (early) { earlyFinishes.delete(started.job); event(early); }
      }
      if (request.method === "project_new" || request.method === "project_open") {
        stores.measurementSettings.set(null);
        host.set({ path: null, selected: null, mdat: null, warnings: [], runs: {}, channel: null, xoGroup: null, xoRecommendation: null, failure: null });
      }
      await reload(triggeredBy({ mutation: request.method }));
      if ((HOST_TRIGGERS.measurementSettings.mutations as readonly string[]).includes(request.method) && stores.measurementSettings.getSnapshot()) await call({ method: "measure_config_get" });
      if (request.method === "project_new" && (request as Extract<Request, { method: "project_new" }>).params.tutorial) await configure({ method: "mock" });
      // A new project stores "none"; a reopened one keeps its method and devices.
      else if (request.method === "project_new" || request.method === "project_open") await call({ method: "measure_config_get" });
      return data;
    } catch (error) { report(error); throw error; }
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
  let curveVersion = 0;
  async function refreshCurves(params: ResultCurvesParams) {
    const ticket = ++curveVersion;
    stores.resultCurves.invalidate();
    const value = await call({ method: "result_curves", params });
    if (ticket === curveVersion) stores.resultCurves.set(value);
  }
  async function waitForJob(id: number) {
    const existing = host.getSnapshot().jobs.find((job) => job.job === id);
    if (existing && !["running", "cancel_requested"].includes(existing.status)) {
      if (existing.error) throw new IpcFailure(existing.error);
      return;
    }
    await new Promise<void>((resolve, reject) => {
      const off = ipc.events.subscribe((event) => {
        if (event.event === "finished" && event.job === id) {
          off(); if (event.error) reject(new IpcFailure(event.error)); else resolve();
        }
      });
    });
  }
  let monitorQueue = Promise.resolve();
  let monitoring = false;
  function monitor(active: boolean) {
    monitorQueue = monitorQueue.catch(() => {}).then(async () => {
      if (active === monitoring) return;
      if (active) {
        host.set({ monitor: "starting" });
        try { await call({ method: "level_monitor_start", params: { role: null } }); monitoring = true; }
        catch (error) { host.set({ monitor: "offline" }); throw error; }
      } else {
        const stopped = await call({ method: "level_monitor_stop" });
        if (stopped && ["running", "cancel_requested"].includes(stopped.status)
            && !host.getSnapshot().jobs.some((job) => job.job === stopped.job && !["running", "cancel_requested"].includes(job.status))) {
          await new Promise<void>((resolve) => {
            const off = ipc.events.subscribe((event) => {
              if (event.event === "finished" && event.job === stopped.job) { off(); resolve(); }
            });
          });
        }
        monitoring = false;
        host.set({ monitor: "off", meter: null, playing: null, playingAll: false });
      }
    });
    return monitorQueue;
  }
  function event(event: JobEvent) {
    const h = host.getSnapshot();
    if (event.event === "measure_status") { stores.methods.set(event.status); return; }
    if (event.event === "meter") { host.set({ meter: event, monitor: "receiving" }); return; }
    if (event.event === "started") {
      host.set({ jobs: [...h.jobs.filter((j) => j.job !== event.job), { job: event.job, kind: event.kind, status: "running", cancellable: true, progress: null, result: null, error: null }],
        ...(["calibrate", "mimo", "recommend_xo"].includes(event.kind) ? { failure: null } : {}) });
      return;
    }
    const job = h.jobs.find((j) => j.job === event.job);
    if (!job) return;
    if (event.event === "progress") {
      host.set({ jobs: h.jobs.map((j) => j === job ? { ...j, progress: { stage: event.stage, done: event.done, total: event.total } } : j) }); return;
    }
    const request = h.jobRequests[event.job];
    if (event.event === "log") {
      if (event.code === "measurement_low_snr" && request?.method === "measure_position") {
        const position = request.params.position;
        const old = h.warnings.find((w) => w.position === position);
        host.set({ warnings: [...h.warnings.filter((w) => w.position !== position), { position, notices: [...(old?.notices ?? []), { code: event.code, params: event.params }] }] });
      }
      return;
    }
    if (!request && !["level_monitor", "status_watch"].includes(job.kind)) earlyFinishes.set(event.job, event);
    host.set({ jobs: h.jobs.map((j) => j === job ? { ...j, status: event.status, result: event.result, error: event.error } : j) });
    if (job.kind === "level_monitor") { monitoring = false; host.set({ monitor: event.status === "failed" ? "offline" : "off" }); }
    if (event.error && ["calibrate", "mimo", "recommend_xo"].includes(job.kind)) {
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
      } else if (event.error) host.set({ runs: { ...h.runs, [request.params.kind]: { status: "failed", error: event.error } } });
    }
    // A failed/cancelled job may already have committed some measurements.
    void reload(triggeredBy({ finished: job.kind }));
  }
  let off: (() => void) | null = null;
  let startPromise: Promise<void> | null = null;
  let lifecycle = 0;
  async function start() {
    lifecycle++;
    if (startPromise) return startPromise;
    off = ipc.events.subscribe(event);
    startPromise = (async () => {
      await Promise.all([stores.bootstrap.load(), stores.appSettings.load(), stores.methods.load(),
        ipc.request({ method: "jobs" }).then((jobs) => {
          const live = host.getSnapshot().jobs;
          host.set({ jobs: [...jobs.filter((job) => !live.some((seen) => seen.job === job.job)), ...live] });
        }).catch(report),
      ]);
      try { stores.project.set(await ipc.request({ method: "project_info" })); }
      catch (error) { report(error); }
      if (stores.project.getSnapshot()) {
        await reload(Object.keys(stores).filter((key) => !["bootstrap", "measurementSettings", "appSettings", "methods", "rew"].includes(key)) as (keyof typeof stores)[]);
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
  return { ipc, stores, host, errors, ready, call, run, configure, refreshCurves, ensureSaved, waitForJob, monitor, start, dispose,
    navigate: (current: StepId) => host.set({ current }) };
}
export type Session = ReturnType<typeof createSession>;
