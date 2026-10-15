// A mock of the service behind `secs_call`, for the gallery and the tests:
// an `Invoke` for makeClient() that answers from typed fixtures, runs jobs
// on a timer (or on `tick()`), pushes their JobEvents on a bus, and fails on
// request. It keeps client.ts's contract: every reply is an envelope and the
// command never rejects.
import type { Invoke } from "./client";
import { makeClient } from "./client";
import { createEventBus, type JobEventBus } from "./events";
import * as fx from "./fixtures";
import type { ErrorCode } from "./bindings/ErrorCode";
import type { IpcError } from "./bindings/IpcError";
import type { JobId } from "./bindings/JobId";
import type { JobKind } from "./bindings/JobKind";
import type { JobResult } from "./bindings/JobResult";
import type { JobSnapshot } from "./bindings/JobSnapshot";
import type { ProjectInfo } from "./bindings/ProjectInfo";
import type { Request } from "./bindings/Request";
import type { Response } from "./bindings/Response";
import type { ResultsSummary } from "./bindings/ResultsSummary";

export type Method = Request["method"];

type Stage = { stage: string | null; total: number; ticks?: number };
type Log = { atTick: number; code: string; params: Record<string, string> };

type SimJob = {
  id: JobId;
  kind: JobKind;
  stages: Stage[];
  stageIndex: number;
  done: number;
  ticks: number;
  logs: Log[];
  cancelRequested: boolean;
};

export type MockService = {
  invoke: Invoke;
  request: ReturnType<typeof makeClient>;
  events: JobEventBus;
  /** Every request received, in order. */
  calls: Request[];
  /** The next call to `method` fails with `code`. */
  failNext: (method: Method, code: ErrorCode, detail?: string, params?: Record<string, string>) => void;
  /** Every call to `method` fails until `clearFailures()`. */
  failAlways: (method: Method, code: ErrorCode, detail?: string) => void;
  clearFailures: () => void;
  /** Jobs of `kind` end with this error instead of a result. */
  failJobs: (kind: JobKind, code: ErrorCode, detail?: string) => void;
  /** Advances every running job by one step (with `tickMs: 0`, the only way). */
  tick: () => void;
  /** Runs ticks until no job is running. */
  drain: () => void;
  setResults: (results: ResultsSummary) => void;
  dispose: () => void;
};

export type MockOptions = {
  /** Milliseconds per job step; 0 drives jobs only through tick(). */
  tickMs?: number;
  project?: ProjectInfo | null;
  results?: ResultsSummary;
};

export const ipcError = (code: ErrorCode, detail = "", params: Record<string, string> = {}): IpcError => ({
  code,
  params,
  retryable: code === "job_busy" || code === "rew_unreachable" || code === "device_unavailable",
  detail,
});

class MockError extends Error {
  constructor(readonly error: IpcError) {
    super(error.code);
  }
}

export function createMockService(options: MockOptions = {}): MockService {
  const tickMs = options.tickMs ?? 450;
  const events = createEventBus();
  const calls: Request[] = [];
  const failures = new Map<Method, { error: IpcError; once: boolean }>();
  const jobFailures = new Map<JobKind, IpcError>();
  const jobs = new Map<JobId, SimJob>();
  let project: ProjectInfo | null = options.project === undefined ? { ...fx.projectInfo } : options.project;
  let results: ResultsSummary = options.results ?? fx.noResults;
  let nextJob = 1;
  let timer: ReturnType<typeof setInterval> | null = null;
  let undoToken = 0;

  const mains = () => fx.SAMPLE_MAINS.length;

  const plan = (kind: JobKind, role?: string): { stages: Stage[]; logs: Log[] } => {
    switch (kind) {
      case "calibrate":
        return { stages: [{ stage: "engine.design", total: mains() + 1 }], logs: [] };
      case "recommend_xo":
        return { stages: [{ stage: "engine.xo_recommendation", total: 4 }], logs: [] };
      case "mimo":
        return { stages: [{ stage: "engine.mimo", total: 2 }, { stage: "engine.mimo_iir", total: mains() }], logs: [] };
      case "export":
        return { stages: [{ stage: "design", total: 1 }], logs: [] };
      case "sample_load":
        return { stages: [{ stage: null, total: 0, ticks: 3 }], logs: [] };
      case "mdat_import":
        return { stages: [{ stage: "mdat_import", total: 1 }], logs: [] };
      case "level_check":
        return { stages: [{ stage: role ?? "L", total: 1 }], logs: [] };
      case "level_monitor":
      case "status_watch":
        return { stages: [{ stage: null, total: 0, ticks: 1 }], logs: [] };
      case "measure":
        return {
          stages: fx.SAMPLE_ROLES.map((r) => ({ stage: r, total: fx.SAMPLE_ROLES.length })),
          logs: [{ atTick: 4, code: "measurement_low_snr", params: { role: "SW2", snr_db: "17.2", threshold_db: "20" } }],
        };
    }
  };

  const result = (kind: JobKind): JobResult => {
    switch (kind) {
      case "calibrate":
        results = { calibration: { ...fx.calibrationSummary, stale: false }, mimo: results.mimo ? { ...results.mimo, stale: true } : null };
        return { kind: "calibration", data: results };
      case "recommend_xo":
        return { kind: "xo", data: fx.xoRecommendation };
      case "mimo":
        results = { ...results, mimo: { ...fx.mimoSummary, stale: false } };
        return { kind: "mimo", data: fx.mimoSummary };
      case "export":
        return { kind: "export", data: fx.exportResult };
      case "level_check":
        return { kind: "level", data: fx.levelResult };
      case "measure":
      case "sample_load": {
        const base = project ?? fx.projectInfo;
        const next = base.positions.length + 1;
        project = { ...base, positions: kind === "measure" ? [...base.positions, next] : [1], dirty: true };
        return { kind: "measurement", data: project };
      }
      case "mdat_import":
        project = { ...(project ?? fx.projectInfo), dirty: true };
        return { kind: "mdat_import", data: project };
      case "level_monitor":
      case "status_watch":
        return { kind: "stopped" };
    }
  };

  const snapshot = (job: SimJob): JobSnapshot => {
    const stage = job.stages[job.stageIndex];
    return {
      job: job.id,
      kind: job.kind,
      status: job.cancelRequested ? "cancel_requested" : "running",
      cancellable: true,
      progress: stage && stage.total > 0 ? { stage: stage.stage ?? "", done: job.done, total: stage.total } : null,
      result: null,
      error: null,
    };
  };

  const finish = (job: SimJob, status: "succeeded" | "failed" | "cancelled", error: IpcError | null) => {
    jobs.delete(job.id);
    events.emit({ event: "finished", job: job.id, status, result: status === "succeeded" ? result(job.kind) : null, error });
    if (jobs.size === 0 && timer) {
      clearInterval(timer);
      timer = null;
    }
  };

  const step = (job: SimJob) => {
    job.ticks += 1;
    if (job.cancelRequested) {
      finish(job, "cancelled", ipcError("cancelled", "cancelled by the user"));
      return;
    }
    for (const log of job.logs) if (log.atTick === job.ticks) events.emit({ event: "log", job: job.id, code: log.code, params: log.params });
    const stage = job.stages[job.stageIndex];
    if (!stage) return;
    if (stage.total === 0) {
      if (job.ticks >= (stage.ticks ?? 1)) job.stageIndex += 1;
    } else {
      job.done += 1;
      events.emit({ event: "progress", job: job.id, stage: stage.stage ?? "", done: job.done, total: stage.total });
      const sameCount = job.kind === "measure";
      if (sameCount) job.stageIndex += 1;
      else if (job.done >= stage.total) {
        job.stageIndex += 1;
        job.done = 0;
      }
    }
    if (job.stageIndex >= job.stages.length) {
      const failure = jobFailures.get(job.kind);
      if (failure) finish(job, "failed", failure);
      else finish(job, "succeeded", null);
    }
  };

  const tick = () => {
    for (const job of [...jobs.values()]) step(job);
  };

  const start = (kind: JobKind, role?: string) => {
    if ([...jobs.values()].some((job) => job.kind === kind)) throw new MockError(ipcError("job_busy", `a ${kind} job is running`));
    const id = nextJob++;
    const { stages, logs } = plan(kind, role);
    jobs.set(id, { id, kind, stages, stageIndex: 0, done: 0, ticks: 0, logs, cancelRequested: false });
    events.emit({ event: "started", job: id, kind });
    if (tickMs > 0 && !timer) timer = setInterval(tick, tickMs);
    return { job: id, kind };
  };

  const needProject = (): ProjectInfo => {
    if (!project) throw new MockError(ipcError("project_not_open", "no project is open"));
    return project;
  };

  const answer = (req: Request): Response => {
    switch (req.method) {
      case "bootstrap":
        return { method: "bootstrap", data: fx.bootstrapInfo };
      case "project_info":
        return { method: "project_info", data: needProject() };
      case "project_new":
        project = { ...(req.params.sample ? fx.projectInfo : fx.emptyProject), path: req.params.path, preset: req.params.preset, sub_count: req.params.sub_count, positions: [], dirty: true };
        results = fx.noResults;
        return { method: "project_new", data: project };
      case "project_open":
        project = { ...fx.projectInfo, path: req.params.path, dirty: false };
        return { method: "project_open", data: project };
      case "project_save":
        project = { ...needProject(), path: req.params.path ?? needProject().path, dirty: false };
        return { method: "project_save", data: project };
      case "results":
        return { method: "results", data: results };
      case "jobs":
        return { method: "jobs", data: [...jobs.values()].map(snapshot) };
      case "job_cancel": {
        const job = jobs.get(req.params.job);
        if (!job) throw new MockError(ipcError("job_not_found", `job ${req.params.job} is not running`));
        job.cancelRequested = true;
        return { method: "job_cancel", data: snapshot(job) };
      }
      case "measure_methods":
        return { method: "measure_methods", data: fx.measureMethods };
      case "measure_configure":
        return { method: "measure_configure", data: { ...fx.measureConfig, method: req.params.method } };
      case "level_apply":
        return { method: "level_apply", data: fx.measureConfig };
      case "rew_launch":
        return { method: "rew_launch", data: fx.rewStatus };
      case "position_clear": {
        const current = needProject();
        project = { ...current, positions: current.positions.filter((p) => p !== req.params.position), dirty: true };
        undoToken += 1;
        return { method: "position_clear", data: { token: `undo-${undoToken}`, project } };
      }
      case "undo":
        project = { ...fx.projectInfo, dirty: true };
        return { method: "undo", data: project };
      case "sample_load":
        needProject();
        return { method: "sample_load", data: start("sample_load") };
      case "calibrate":
        needProject();
        return { method: "calibrate", data: start("calibrate") };
      case "recommend_xo":
        return { method: "recommend_xo", data: start("recommend_xo") };
      case "mimo":
        return { method: "mimo", data: start("mimo") };
      case "export":
        if (!results.calibration) throw new MockError(ipcError("engine_input", "no calibration result to export"));
        return { method: "export", data: start("export") };
      case "level_check":
        return { method: "level_check", data: start("level_check", req.params.role) };
      case "measure_position":
        needProject();
        return { method: "measure_position", data: start("measure") };
      case "mdat_import":
        return { method: "mdat_import", data: start("mdat_import") };
      // The step screens add fixtures for these as they reach them (P30–P33).
      case "mdat_scan":
      case "config_get":
      case "config_apply_preset":
      case "config_set_sub_count":
      case "config_set_speaker":
      case "config_add_speaker":
      case "config_remove_speaker":
      case "config_renumber":
      case "config_validate":
      case "patch_sheet_csv":
      case "dsp_get":
      case "dsp_set":
      case "layout_get":
      case "layout_set":
      case "position_estimate":
      case "target_get":
      case "target_set_points":
      case "target_apply_preset":
      case "target_save":
      case "target_load":
      case "calibration_settings_get":
      case "calibration_settings_set":
      case "calibration_settings_edit":
      case "sub_warnings":
      case "rew_info":
      case "mode_spec":
      case "app_settings_get":
      case "app_settings_set":
      case "measurement_curves":
      case "result_curves":
      case "result_lines":
      case "sub_take_values":
      case "dialog_open_project":
      case "dialog_save_project":
      case "dialog_pick_mdat":
      case "dialog_pick_folder":
      case "dialog_save_file":
      case "dialog_open_file":
      case "open_folder":
      case "level_monitor_start":
      case "level_monitor_stop":
      case "level_play":
      case "status_watch_start":
      case "status_watch_stop":
      case "export_folder_get":
      case "export_folder_set":
        throw new MockError(ipcError("not_implemented", `${req.method} has no fixture yet`));
    }
  };

  const invoke: Invoke = async (command, args) => {
    if (command !== "secs_call") return { ok: false, error: ipcError("invalid_request", `unknown command ${command}`) };
    const req = args.request as Request;
    calls.push(req);
    const failure = failures.get(req.method);
    if (failure) {
      if (failure.once) failures.delete(req.method);
      return { ok: false, error: failure.error };
    }
    try {
      return { ok: true, data: answer(req) };
    } catch (error) {
      if (error instanceof MockError) return { ok: false, error: error.error };
      return { ok: false, error: ipcError("internal", String(error)) };
    }
  };

  return {
    invoke,
    request: makeClient(invoke),
    events,
    calls,
    failNext: (method, code, detail = "", params = {}) => failures.set(method, { error: ipcError(code, detail, params), once: true }),
    failAlways: (method, code, detail = "") => failures.set(method, { error: ipcError(code, detail), once: false }),
    clearFailures: () => failures.clear(),
    failJobs: (kind, code, detail = "") => jobFailures.set(kind, ipcError(code, detail)),
    tick,
    drain: () => {
      for (let guard = 0; jobs.size > 0 && guard < 1000; guard++) tick();
    },
    setResults: (next) => {
      results = next;
    },
    dispose: () => {
      if (timer) clearInterval(timer);
      timer = null;
      jobs.clear();
    },
  };
}
