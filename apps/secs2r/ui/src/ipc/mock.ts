// Stateful service double: every command returns an envelope, and jobs push
// events through the same bus as the real transport. The mock owns the
// contract only (ADR 0014, ADR 0052): envelopes, the job scheduler and its
// events, the exclusive job groups, failure injection, undo, the open
// project's state and the dialog queue. Rules are not copied here: limits,
// field lists and tables come from the generated `bindings/rules`, and the
// answers that compute something (mode specs, presets, new-project settings,
// refused edits, calibration results and their lines, exports, updater
// transitions) are the service's recorded answers, returned as recorded.
// Edits that move state (sub entries, bundles, infra, the XO map) stay here.
import type { Invoke } from "./client";
import { makeClient } from "./client";
import { isMonitorJob } from "../state/jobRetention";
import { createEventBus, type JobEventBus } from "./events";
import * as fx from "./fixtures";
import type { ApplyStatus } from "./bindings/ApplyStatus";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { HostManaged } from "./bindings/HostManaged";
import type { HostStatus } from "./bindings/HostStatus";
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
import type { CalibrationSettings } from "./bindings/CalibrationSettings";
import type { SettingEdit } from "./bindings/SettingEdit";
import type { PositionRecord } from "./bindings/PositionRecord";
import type { LevelsResult } from "./bindings/LevelsResult";
import type { ExportFolderResult } from "./bindings/ExportFolderResult";
import type { ResultCurvesResult } from "./bindings/ResultCurvesResult";
import type { ResultLinesResult } from "./bindings/ResultLinesResult";
import type { Notice } from "./bindings/Notice";
import type { MicCalibrationStatus } from "./bindings/MicCalibrationStatus";
import { umikFile } from "../screens/connect/fixtures";
import type { ExportParams } from "./bindings/ExportParams";
import type { ExportResult } from "./bindings/ExportResult";
import type { CurveReference } from "./bindings/CurveReference";
import type { MeasurementCurve } from "./bindings/MeasurementCurve";
import type { UpdatePhase } from "./bindings/UpdatePhase";
import type { UpdatePolicy } from "./bindings/UpdatePolicy";
import {
  AGREEMENT_MIN_POSITIONS, CALIBRATION_LIMITS as LIMITS, ENGINE_KEY_FIELDS, JOB_GROUPS, MANUAL_KEY_FIELDS, MIMO_KEY_FIELDS,
  OUTPUT_RATES, RECENT_PROJECTS_MAX, REPETITIONS, SCALAR_EDITS, TRANSITION_FACTORS,
} from "./bindings/rules";
import { isSubRole, roleKind } from "../ui/maps/geometry";
import { RECORDED_EXPORTS, RECORDED_UPDATE_TRANSITIONS, ordered, recordKey, recorded, recordedCalibration, recordedRefusal, same } from "./recorded";
import { CAMILLA_EXE_FILE, addressProblem, designNameFor, designNameProblem, isFilterKind } from "../screens/export/types";

export type Method = Request["method"];
type DialogMethod = Extract<Method, `dialog_${string}`>;
type Stage = { stage: string | null; total: number; ticks?: number };
type Log = { atTick: number; code: string; params: Record<string, string> };
type SimJob = {
  id: JobId; kind: JobKind; stages: Stage[]; stageIndex: number; done: number;
  ticks: number; logs: Log[]; cancelRequested: boolean; complete: () => JobResult;
  /** The job fails with this when it ends, as the service fails one its input does not allow. */
  error?: IpcError | undefined;
  /**
   * `calibrate` with `mimo` (P141): when its correction stages end, the
   * correction is stored (or the job fails there with `error`), and the MIMO
   * stages that follow fail with `next()` when one is asked for.
   */
  phase?: { at: number; commit: () => void; next: () => IpcError | undefined } | undefined;
};

export type MockService = {
  invoke: Invoke;
  request: ReturnType<typeof makeClient>;
  events: JobEventBus;
  calls: Request[];
  failNext: (method: Method, code: ErrorCode, detail?: string, params?: Record<string, string>) => void;
  failAlways: (method: Method, code: ErrorCode, detail?: string) => void;
  clearFailures: () => void;
  failJobs: (kind: JobKind, code: ErrorCode, detail?: string, params?: Record<string, string>) => void;
  /**
   * The level monitor fails on its next tick while a burst plays, once: the
   * native monitor opens its output only to play (#120). A monitor started
   * again afterwards runs normally.
   */
  failLevelPlay: (code: ErrorCode, detail?: string, params?: Record<string, string>) => void;
  /** A null answer simulates cancelling the next matching host dialog. */
  queueDialog: (method: DialogMethod, path: string | null) => void;
  tick: () => void;
  /** Whether a finite job (or a cancellation) is still running; live monitors do not count. */
  busy: () => boolean;
  /** Drains finite jobs (and cancellation), leaving live monitors running. */
  drain: () => void;
  setResults: (results: ResultsSummary) => void;
  /** What `apply_status` answers from now on: a running CamillaDSP, an Equalizer APO install (specimens and tests flip it). */
  setApplyStatus: (status: ApplyStatus) => void;
  /** Script the updater's next state; requests never contact a release server. */
  setUpdateStatus: (status: UpdateStatus) => void;
  dispose: () => void;
};
type MockOptions = {
  computeStatus?: import("./bindings/ComputeStatus").ComputeStatus; tickMs?: number; project?: ProjectInfo | null; results?: ResultsSummary;
  /** `apply_status` until `setApplyStatus`; CamillaDSP unreachable and no Equalizer APO by default. */
  applyStatus?: ApplyStatus;
  updateStatus?: UpdateStatus;
};
export const ipcError = (code: ErrorCode, detail = "", params: Record<string, string> = {}): IpcError => ({
  code, params, retryable: code === "job_busy" || code === "rew_unreachable" || code === "device_unavailable", detail,
});
class MockError extends Error {
  constructor(readonly error: IpcError) { super(error.code); }
}
const clone = <T,>(value: T): T => structuredClone(value);
const clamp = (value: number, [lo, hi]: readonly [number, number]) => Math.min(hi, Math.max(lo, value));
/** The exclusive group of each job kind, from the service's jobs.rs table (audit #155 TD-124). */
const GROUP_OF = new Map<JobKind, string>(JOB_GROUPS.flatMap(([group, kinds]) => kinds.map((kind) => [kind, group] as const)));
const jobGroup = (kind: JobKind) => GROUP_OF.get(kind) ?? kind;
/** secs-types valid_rc_bands also normalizes settings supplied directly to a job. */
function validRcBands(bands: CalibrationSettings["rc_bands"]) {
  return bands.filter(({ lo_hz, hi_hz }) => Number.isFinite(lo_hz) && Number.isFinite(hi_hz)
    && lo_hz >= LIMITS.rc_band[0] && hi_hz <= LIMITS.rc_band[1] && lo_hz < hi_hz)
    .sort((a, b) => a.lo_hz - b.lo_hz || a.hi_hz - b.hi_hz).slice(0, LIMITS.rc_bands_max[1]);
}
const projection = (settings: CalibrationSettings, fields: readonly (keyof CalibrationSettings)[]) => JSON.stringify(fields.map((field) => ordered(settings[field])));
/** The fields the correction reads (secs-types `engine_key` and `manual_key`, generated lists). */
const correctionKey = (settings: CalibrationSettings) => projection(settings, [...ENGINE_KEY_FIELDS, ...MANUAL_KEY_FIELDS]);
/** The fields only MIMO reads (secs-types `mimo_key`, generated list). */
const mimoKey = (settings: CalibrationSettings) => projection(settings, MIMO_KEY_FIELDS);
const SCALAR = new Map<string, (typeof SCALAR_EDITS)[number]>(SCALAR_EDITS.map((rule) => [rule.kind, rule]));
/**
 * The number a scalar edit sets (`SCALAR_EDITS`, ADR 0052): clamped to its
 * limits, or refused outside them with the service's recorded refusal. Null
 * turns the setting off and passes through.
 */
function scalar(edit: SettingEdit): number | null {
  const rule = SCALAR.get(edit.kind);
  const [value = null] = Object.entries(edit).flatMap(([key, item]) => key !== "kind" && (typeof item === "number" || item === null) ? [item as number | null] : []);
  if (!rule || value === null) return value;
  const limits = LIMITS[rule.limit];
  if (rule.policy === "clamp") return clamp(value, limits);
  if (!Number.isFinite(value) || value < limits[0] || value > limits[1]) {
    throw new MockError(recordedRefusal(edit, true) ?? ipcError("invalid_request", `${rule.field} is outside its range`, { field: rule.field }));
  }
  return value;
}

/** Where the service writes what it applies to CamillaDSP (`app_data_path/applied/camilladsp`). */
const APPLIED_CAMILLA = "C:/Users/demo/AppData/Roaming/SECS2R/applied/camilladsp";
const basename = (path: string) => path.split(/[\\/]/).at(-1) ?? path;

/**
 * Zero dB of the measurement graph (secs-service `measurement_reference`,
 * U17): the first main in configuration order, at position 1 when it was
 * measured there and otherwise at its lowest measured position, over
 * 200–2000 Hz; with no main measured, the first sub at its lowest measured
 * position over 40–120 Hz. `order` is the configuration's roles.
 */
function measurementReference(order: readonly string[], measured: readonly { role: string; position: number }[]): CurveReference | null {
  const lowest = (role: string) => {
    const positions = measured.filter((curve) => curve.role === role).map((curve) => curve.position);
    return positions.length ? Math.min(...positions) : null;
  };
  for (const sub of [false, true]) {
    for (const role of order) {
      if (isSubRole(role) !== sub) continue;
      const low = lowest(role);
      if (low === null) continue;
      const position = !sub && measured.some((curve) => curve.role === role && curve.position === 1) ? 1 : low;
      return { role, position, band: sub ? "sub" : "mid" };
    }
  }
  return null;
}

/**
 * The curves shifted by one value, the reference curve's mean over its band,
 * and that reference (secs-service `measurement_curves`). Every curve keeps
 * its level against the others; the reference is chosen from all of them,
 * before any request filter.
 */
export function shiftToReference<C extends { role: string; position: number; values: number[] }>(
  grid: ArrayLike<number>,
  curves: readonly C[],
  order: readonly string[],
): { curves: C[]; reference: CurveReference | null } {
  const reference = measurementReference(order, curves);
  const source = reference && curves.find((curve) => curve.role === reference.role && curve.position === reference.position);
  if (!reference || !source) return { curves: [...curves], reference };
  const [lo, hi] = reference.band === "mid" ? [200, 2000] : [40, 120];
  const band = source.values.filter((_, i) => (grid[i] ?? 0) >= lo && (grid[i] ?? 0) <= hi);
  const mean = band.reduce((sum, value) => sum + value, 0) / Math.max(1, band.length);
  return { curves: curves.map((curve) => ({ ...curve, values: curve.values.map((value) => value - mean) })), reference };
}

/**
 * A measured response at the microphone in dB SPL: about 78 dB with synthetic
 * ripple, mains falling below about 45 Hz, subs low-passed near 95 Hz.
 * Deterministic per seed.
 */
const mockMeasurement = (role: string, seed: number): number[] =>
  fx.curveValues(seed).map((value, i) => {
    const hz = fx.curveGrid[i]!;
    const shape = isSubRole(role) ? -24 * Math.log2(Math.max(1, hz / 95)) : -12 * Math.log2(Math.max(1, 45 / hz));
    return 78 + value + shape;
  });

/** The curve kind the service names (secs-service `measurement_curves`). */
const measurementKind = (role: string): MeasurementCurve["kind"] => (roleKind(role) === "height" ? "top" : roleKind(role));

/** The service fails a calibration on the agreement basis below the positions it needs (secs-service `check_rc_agreement`). */
const rcAgreementError = (settings: CalibrationSettings, positions: number): IpcError | undefined =>
  settings.rc_hi === null && settings.rc_hi_basis === "agreement" && positions < AGREEMENT_MIN_POSITIONS
    ? ipcError("engine_input", `RC agreement requires at least ${AGREEMENT_MIN_POSITIONS} measured positions`, { reason: "rc_agreement_positions", measured: String(positions), required: String(AGREEMENT_MIN_POSITIONS) })
    : undefined;

const updateBase: UpdateStatus = { current: fx.bootstrapInfo.version, policy: "auto", phase: { kind: "up_to_date" }, checked_at: "2026-10-30T09:00:00Z", notice: null };
const updateVersion = "1.2.0";
const updateUrl = `https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R/releases/tag/v${updateVersion}`;
const updateNotice = { from: "1.1.0", to: updateVersion, notes: "Update notes", skipped: false, release_url: updateUrl };
/** Fixed updater specimens. Use setUpdateStatus to advance checking/downloading in tests. */
export const updateStates = {
  upToDate: updateBase,
  downloading: { ...updateBase, phase: { kind: "downloading", version: updateVersion, received: 40, total: 100 } },
  readyOnExit: { ...updateBase, phase: { kind: "ready", version: updateVersion, install: "on_exit" } },
  readyOnRequest: { ...updateBase, phase: { kind: "ready", version: updateVersion, install: "on_request" } },
  available: { ...updateBase, policy: "notify", phase: { kind: "available", version: updateVersion } },
  portable: { ...updateBase, phase: { kind: "portable", version: updateVersion, url: updateUrl } },
  failed: { ...updateBase, phase: { kind: "failed", reason: "Could not check for updates" } },
  notice: { ...updateBase, current: updateVersion, notice: updateNotice },
  skippedNotice: { ...updateBase, current: updateVersion, notice: { ...updateNotice, from: "1.0.0", skipped: true } },
} satisfies Record<string, UpdateStatus>;

type UpdateMethod = (typeof RECORDED_UPDATE_TRANSITIONS)[number]["params"]["method"];
/**
 * The updater's next phase after this request in this policy, as the app
 * recorded it (`update_transitions.json`), or its refusal. The record names
 * the next phase's kind; the version it carries comes from the phase it
 * leaves. The update requests were recorded in one policy per phase, which
 * does not change their answer.
 */
function nextPhase(method: UpdateMethod, policy: UpdatePolicy, from: UpdatePhase): UpdatePhase {
  const entries = RECORDED_UPDATE_TRANSITIONS.filter((entry) => entry.params.method === method && entry.params.phase === from.kind);
  const entry = entries.find((e) => e.params.policy === policy) ?? entries[0];
  if (!entry) return from;
  if ("error" in entry.data) throw new MockError(ipcError(entry.data.error.code, entry.data.error.detail));
  const kind = entry.data.phase;
  if (kind === from.kind) return from;
  const version = "version" in from ? from.version : updateVersion;
  switch (kind) {
    case "downloading": return { kind, version, received: 0, total: null };
    case "available": case "installing": return { kind, version };
    case "ready": return { kind, version, install: "on_exit" };
    case "portable": return { kind, version, url: updateUrl };
    case "failed": return { kind, reason: "Could not check for updates" };
    default: return { kind };
  }
}

export function createMockService(options: MockOptions = {}): MockService {
  const tickMs = options.tickMs ?? 450;
  const events = createEventBus();
  const calls: Request[] = [];
  const failures = new Map<Method, { error: IpcError; once: boolean }>();
  const jobFailures = new Map<JobKind, IpcError>();
  let playFailure: IpcError | null = null;
  const dialogs = new Map<DialogMethod, (string | null)[]>();
  const jobs = new Map<JobId, SimJob>();
  let project = clone(options.project === undefined ? fx.projectInfo : options.project);
  let results = clone(options.results ?? fx.noResults);
  let config = fx.configPreset(project?.preset ?? "2.0", project?.sub_count ?? 0);
  let dsp = clone(fx.dsp);
  let layout = clone(fx.layout);
  let settings = clone(fx.calibrationSettings);
  let target = clone(fx.target);
  let appSettings = clone(fx.appSettings);
  let applyStatus = clone(options.applyStatus ?? fx.applyStatus);
  let updateStatus: UpdateStatus = clone(options.updateStatus ?? updateStates.upToDate);
  appSettings.updates = updateStatus.policy;
  const availableCompute = clone(options.computeStatus ?? fx.computeStatus);
  const computeStatus = () => ({ ...availableCompute, preference: appSettings.compute,
    ...(appSettings.compute === "cpu" ? { selected: "cpu" as const, reason: "user_cpu" as const } :
      appSettings.compute === "gpu" && availableCompute.gpu_available ? { selected: "cuda" as const, reason: "user_gpu" as const } : {}) });
  let measureConfig = clone(fx.measureConfig);
  // REW's own state outlives projects; a driver switch or a chosen device changes it.
  const rew = clone(fx.rewInfo);
  let levels: LevelsResult = { levels: {}, spl: {} };
  let exportFolder: ExportFolderResult = { folder: "C:/Users/demo/Documents/SECS_export", stored: false };
  let records: PositionRecord[] = (project?.positions ?? []).map((position) => ({ position, roles: config.speakers.map((s) => s.role), done: true, sources: Object.fromEntries(config.speakers.map((s) => [s.role, "measured"])) }));
  let micCalFile: MicCalibrationStatus["file"] = null;
  let micCalAttached = new Set<string>();
  const micCalKey = (position: number, role: string) => `${position}:${role}`;
  const micCalStatus = (): MicCalibrationStatus => {
    needProject();
    let attached = 0; let attachable = 0;
    for (const record of records) for (const role of record.roles) {
      if (micCalAttached.has(micCalKey(record.position, role))) attached++;
      else if (record.sources[role] === "measured") attachable++;
    }
    return { file: micCalFile, attached, attachable };
  };
  const editMicCal = () => {
    needProject();
    if ([...jobs.values()].some((job) => job.kind === "measure")) throw new MockError(ipcError("job_busy", "a measurement is running"));
  };
  let playing: { role: string; master: number; ticks: number } | null = null;
  let subsOnly = false;
  /** What the live correction ran on (`correctionKey`), for the MIMO run that follows it; null for none or a cached one. */
  let calibratedKey: string | null = results.calibration && !results.calibration.cached ? correctionKey(settings) : null;
  /** The record key of the stored correction's settings, which its lines are looked up by. */
  let resultKey = recordKey(settings);
  let nextJob = 1;
  let nextUndo = 1;
  let timer: ReturnType<typeof setInterval> | null = null;
  const needProject = (): ProjectInfo => {
    if (!project) throw new MockError(ipcError("project_not_open", "no project is open"));
    return project;
  };
  const invalid: (detail: string) => never = (detail) => { throw new MockError(ipcError("invalid_request", detail)); };
  /** The service's status over its pipe; one that is not running cannot be reached (`secs_sys::host::pipe_call`). */
  const hostReached = (): HostStatus => {
    if (applyStatus.host !== "running" || !applyStatus.host_status) throw new MockError(ipcError("apply_failed", "connect secs2r-host pipe: The system cannot find the file specified. (os error 2)"));
    return applyStatus.host_status;
  };
  /** `HostRequest::SetManaged`: the service takes the target over; CamillaDSP starts at once, the line is kept. */
  const manage = (update: HostManaged) => {
    const host = hostReached();
    applyStatus = {
      ...applyStatus,
      host_status: {
        ...host,
        managed: { ...host.managed, ...update },
        ...(update.camilla ? { camilla: { state: { kind: "running" as const }, pid: 4812, restarts_in_window: 0 } } : {}),
        ...(update.eqapo ? { eqapo_include: "present" as const } : {}),
      },
    };
  };
  let inputRevision = 0;
  /** A stored MIMO result falls behind, and so does its place in an export. */
  const mimoBehind = () => {
    if (results.mimo) results = { ...results, mimo: { ...results.mimo, stale: true }, mimo_export: "stale" };
  };
  const dirty = (reason = "settings", recommendationChanged = true) => {
    inputRevision++;
    if (recommendationChanged && Object.keys(settings.xo_rec).length > 0) results = { ...results, xo_rec_stale: true };
    project = { ...needProject(), dirty: true };
    results = { ...results, calibration: results.calibration ? { ...results.calibration, stale: true, stale_reason: reason } : null };
    mimoBehind();
  };
  const syncMeasure = () => {
    measureConfig.roles = config.speakers.map((s) => s.role);
    measureConfig.output_channels = config.speakers.map((s) => s.bands[0]?.out ?? 1);
    for (const record of records) record.done = measureConfig.roles.length > 0 && measureConfig.roles.every((role) => record.roles.includes(role));
  };
  syncMeasure();
  const state = () => clone({ project: needProject(), results, config, dsp, layout, settings, target, measureConfig, levels, exportFolder, records, micCalFile, micCalAttached, subsOnly, calibratedKey, resultKey });
  type State = ReturnType<typeof state>;
  const undos = new Map<string, State>();
  const savedProjects = new Map<string, State>();
  const savedTargets = new Map<string, typeof target>();
  const restore = (s: State) => {
    ({ project, results, config, dsp, layout, settings, target, measureConfig, levels, exportFolder, records, micCalFile, micCalAttached, subsOnly, calibratedKey, resultKey } = clone(s));
  };
  const remember = () => {
    const token = `undo-${nextUndo++}`;
    undos.set(token, state());
    return token;
  };
  const clearMeasurements = (confirm: boolean) => {
    needProject();
    if (!records.length) return null;
    if (!confirm) throw new MockError(ipcError("confirm_required", "this edit clears measurements"));
    const token = remember();
    records = []; micCalAttached.clear();
    project = { ...needProject(), positions: [] };
    dirty("measurements");
    return token;
  };
  const updateConfig = (next: typeof config) => {
    config = next;
    project = { ...needProject(), preset: config.preset, sub_count: config.summary.sub_count };
    syncMeasure();
    dirty("config");
    return config;
  };
  const recordPosition = (position: number, roles = measureConfig.roles, source?: string, calibrated = false) => {
    const old = records.find((r) => r.position === position);
    const sources = { ...old?.sources };
    for (const role of roles) {
      sources[role] = source ? basename(source) : "measured";
      const key = micCalKey(position, role);
      if (calibrated) micCalAttached.add(key);
      else micCalAttached.delete(key);
    }
    const merged = [...new Set([...(old?.roles ?? []), ...roles])];
    const record = { position, roles: merged, sources, done: measureConfig.roles.every((role) => merged.includes(role)) };
    records = [...records.filter((r) => r.position !== position), record].sort((a, b) => a.position - b.position);
    project = { ...needProject(), positions: records.filter((r) => r.roles.length).map((r) => r.position) };
    dirty("measurements");
  };
  /** A created project starts from a new project's recorded settings, an opened one from the sample room's. */
  const reset = (info: ProjectInfo, created = false) => {
    project = clone(info);
    config = fx.configPreset(info.preset, info.sub_count);
    dsp = clone(fx.dsp); layout = clone(fx.layout); target = clone(fx.target);
    settings = clone(created ? fx.newProjectSettings : fx.calibrationSettings);
    measureConfig = clone(fx.measureConfig); levels = { levels: {}, spl: {} };
    results = clone(fx.noResults); exportFolder = { folder: "C:/Users/demo/Documents/SECS_export", stored: false };
    records = info.positions.map((position) => ({ position, roles: config.speakers.map((s) => s.role), sources: Object.fromEntries(config.speakers.map((s) => [s.role, "measured"])), done: true }));
    micCalFile = null; micCalAttached.clear();
    undos.clear(); subsOnly = false; calibratedKey = null; resultKey = recordKey(settings); syncMeasure();
  };
  const dialog = (method: DialogMethod, fallback: string): string | null => {
    const queue = dialogs.get(method);
    return queue?.length ? queue.shift()! : fallback;
  };
  const methods = () => clone(fx.measureMethods);

  const plan = (kind: JobKind): { stages: Stage[]; logs: Log[] } => {
    const mains = config.speakers.filter((s) => !s.role.startsWith("SW")).length;
    switch (kind) {
      case "calibrate": return { stages: [{ stage: "engine.design", total: mains + 1 }], logs: [] };
      case "recommend_xo": return { stages: [{ stage: "engine.xo_recommendation", total: 4 }], logs: [] };
      case "mimo": return { stages: [{ stage: "engine.mimo", total: 2 }, { stage: "engine.mimo_iir", total: Math.max(1, mains) }], logs: [] };
      case "export": case "apply": return { stages: [{ stage: "design", total: 1 }], logs: [] };
      case "sample_load": return { stages: [{ stage: null, total: 0, ticks: 3 }], logs: [] };
      case "mdat_import": return { stages: [{ stage: "mdat_import", total: 1 }], logs: [] };
      case "level_monitor": case "status_watch": return { stages: [], logs: [] };
      case "measure": return {
        stages: measureConfig.roles.map((r) => ({ stage: r, total: measureConfig.roles.length })),
        logs: measureConfig.roles.includes("SW2") ? [{ atTick: 4, code: "measurement_low_snr", params: { role: "SW2", snr_db: "17.2", threshold_db: "20" } }] : [],
      };
    }
  };
  const snapshot = (job: SimJob): JobSnapshot => {
    const stage = job.stages[job.stageIndex];
    return { job: job.id, kind: job.kind, status: job.cancelRequested ? "cancel_requested" : "running", cancellable: true,
      progress: stage && stage.total > 0 ? { stage: stage.stage ?? "", done: job.done, total: stage.total } : null, error: null };
  };
  const finish = (job: SimJob, ended: "succeeded" | "failed" | "cancelled", reason: IpcError | null): JobSnapshot => {
    jobs.delete(job.id);
    let status = ended;
    let error = reason;
    let result: JobResult | null = null;
    // A completion may still refuse (관리 hands over to a service that is not there): the job fails with that error.
    if (status === "succeeded") {
      try { result = clone(job.complete()); }
      catch (thrown) {
        if (!(thrown instanceof MockError)) throw thrown;
        status = "failed";
        error = thrown.error;
      }
    }
    const done: JobSnapshot = { ...snapshot(job), status, error, cancellable: false };
    events.emit({ event: "finished", job: job.id, status, result, error });
    if (jobs.size === 0 && timer) { clearInterval(timer); timer = null; }
    return done;
  };
  const step = (job: SimJob) => {
    job.ticks++;
    if (job.cancelRequested) { finish(job, "cancelled", ipcError("cancelled", "cancelled by the user")); return; }
    if (isMonitorJob(job.kind)) {
      const failure = jobFailures.get(job.kind);
      if (failure) { finish(job, "failed", failure); return; }
      if (job.kind === "level_monitor" && playing && playFailure) {
        // No level_play_end: the burst never played.
        const error = playFailure; playFailure = null; playing = null;
        finish(job, "failed", error);
        return;
      }
      if (job.kind === "status_watch") events.emit({ event: "measure_status", job: job.id, status: methods() });
      else {
        events.emit({ event: "meter", job: job.id, rms_dbfs: playing ? playing.master - 10 : -62, peak_dbfs: playing ? playing.master : -49, spl_db: playing ? 78.5 : 32, clipping: false });
        if (playing && --playing.ticks <= 0) {
          levels.levels[playing.role] = playing.master - 10; levels.spl[playing.role] = 78.5;
          project = { ...needProject(), dirty: true };
          events.emit({ event: "log", job: job.id, code: "level_play_end", params: { role: playing.role, outcome: "done" } });
          playing = null;
        }
      }
      return;
    }
    for (const log of job.logs) if (log.atTick === job.ticks) events.emit({ event: "log", job: job.id, code: log.code, params: log.params });
    const stage = job.stages[job.stageIndex];
    if (stage) {
      if (stage.total === 0) { if (job.ticks >= (stage.ticks ?? 1)) job.stageIndex++; }
      else {
        job.done++;
        events.emit({ event: "progress", job: job.id, stage: stage.stage ?? "", done: job.done, total: stage.total });
        if (job.kind === "measure") job.stageIndex++;
        else if (job.done >= stage.total) { job.stageIndex++; job.done = 0; }
      }
    }
    if (job.phase && job.stageIndex >= job.phase.at) {
      const phase = job.phase;
      job.phase = undefined;
      const failure = job.error ?? jobFailures.get(job.kind);
      if (failure) { finish(job, "failed", failure); return; }
      // Stored before MIMO runs: a MIMO failure or a cancel from here on keeps it.
      phase.commit();
      job.error = phase.next();
    }
    if (job.stageIndex >= job.stages.length) {
      const failure = job.error ?? jobFailures.get(job.kind);
      finish(job, failure ? "failed" : "succeeded", failure ?? null);
    }
  };
  const tick = () => { for (const job of [...jobs.values()]) step(job); };
  const busy = () => [...jobs.values()].some((job) => !isMonitorJob(job.kind) || job.cancelRequested);
  /** `then`: stages that follow the kind's own, reached through `phase` (the combined calibrate). */
  const start = (kind: JobKind, complete: () => JobResult, error?: IpcError, then?: { stages: Stage[]; phase: Omit<NonNullable<SimJob["phase"]>, "at"> }) => {
    if ([...jobs.values()].some((job) => jobGroup(job.kind) === jobGroup(kind))) throw new MockError(ipcError("job_busy", `a ${kind} job is running`));
    const id = nextJob++;
    const own = plan(kind);
    const stages = then ? [...own.stages, ...then.stages] : own.stages;
    const phase = then ? { ...then.phase, at: own.stages.length } : undefined;
    jobs.set(id, { id, kind, stages, logs: own.logs, stageIndex: 0, done: 0, ticks: 0, cancelRequested: false, complete, error, phase });
    events.emit({ event: "started", job: id, kind });
    if (tickMs > 0 && !timer) timer = setInterval(tick, tickMs);
    return { job: id, kind };
  };
  const stopMonitor = (kind: JobKind) => {
    const job = [...jobs.values()].find((j) => j.kind === kind);
    if (kind === "level_monitor" && playing) {
      if (job) events.emit({ event: "log", job: job.id, code: "level_play_end", params: { role: playing.role, outcome: "stopped" } });
      playing = null;
    }
    return job ? finish(job, "succeeded", null) : null;
  };

  const editSettings = (edit: SettingEdit): CalibrationSettings => {
    const sub = (role: string) => settings.subs_manual[role] ??= clone(fx.subEntry);
    const share = (role: string) => {
      const entry = sub(role);
      if (entry.mode !== "bundle" || (settings.bundle_layouts[entry.bundle]?.kind ?? "values") === "values") return;
      for (const other of Object.values(settings.subs_manual)) {
        if (other.mode === "bundle" && other.bundle === entry.bundle) Object.assign(other, clone({ hp_hz: entry.hp_hz, lp_hz: entry.lp_hz, hp_type: entry.hp_type, lp_type: entry.lp_type, peq: entry.peq }));
      }
    };
    // An edit the service was recorded refusing, a number outside a refusing
    // limit (SCALAR_EDITS), or a factor outside TRANSITION_FACTORS answers the
    // recorded refusal, and nothing is stored.
    const refused = recordedRefusal(edit)
      ?? (edit.kind === "rc_hi_transition" && !(TRANSITION_FACTORS as readonly number[]).includes(edit.factor) ? recordedRefusal(edit, true) : undefined);
    if (refused) throw new MockError(refused);
    const value = scalar(edit);
    const before = { correction: correctionKey(settings), mimo: mimoKey(settings) };
    switch (edit.kind) {
      case "mode": settings.mode = edit.mode; break;
      case "sample_rate_policy": settings.sample_rate_policy = edit.policy; break;
      case "low_protect": settings.low_protect = edit.on; break;
      // Taken in every mode, Zero included: the engine leaves lf_align out there, and the screen says why (ADR 0025).
      case "lf_align": settings.lf_align = edit.on; break;
      case "mode_decay": settings.mode_decay = edit.on; break;
      case "hf_phase": settings.hf_phase = edit.on; break;
      case "dip_limit": settings.dip_limit = edit.on; break;
      // The scalar edits of one field each: the field and the number from SCALAR_EDITS.
      case "input_hp": case "ultra_ms": case "rc_lo": case "rc_hi": case "boost_max": case "infra_boost":
      case "main_delay": case "fsiso": case "mimo_low": case "mimo_early":
        Object.assign(settings, { [SCALAR.get(edit.kind)!.field]: value });
        break;
      case "mimo_max": settings.mimo_max_supports = Math.round(value ?? 0); break;
      case "rc_bands": settings.rc_bands = edit.bands.map((band) => ({ ...band })).sort((a, b) => a.lo_hz - b.lo_hz); break;
      case "rc_hi_basis": settings.rc_hi_basis = edit.basis; break;
      case "rc_hi_transition": settings.rc_hi_transition = edit.factor; break;
      case "room_volume": settings.room_volume_m3 = edit.m3 > LIMITS.volume[0] ? value : null; settings.room_dims_m = null; break;
      case "room_dims": {
        const dims = edit.dims.map((n) => clamp(n, LIMITS.dim)) as [number, number, number];
        settings.room_dims_m = dims.every((n) => n >= LIMITS.dim_valid[0] && n <= LIMITS.dim_valid[1]) ? dims : null;
        if (settings.room_dims_m) settings.room_volume_m3 = Math.min(LIMITS.volume[1], Math.round(dims[0] * dims[1] * dims[2] * 10) / 10);
        break;
      }
      case "xo_rec_include": settings.xo_rec_include = edit.on; break;
      case "xo": settings.xo[edit.group] = Math.round(value ?? edit.hz); settings.xo_rec_include = false; break;
      case "xo_peq":
        if (edit.on !== undefined) settings.xo_peq_on = edit.on;
        if (edit.lo !== undefined) settings.xo_peq_lo = clamp(edit.lo, LIMITS.xo_peq);
        if (edit.hi !== undefined) settings.xo_peq_hi = clamp(edit.hi, LIMITS.xo_peq);
        break;
      case "infra":
        for (const role of edit.subs) {
          const e = sub(role);
          e.hp_hz = edit.role && role !== edit.role ? clamp(edit.hz, LIMITS.infra_hz) : 0;
          e.lp_hz = role === edit.role ? clamp(edit.hz, LIMITS.infra_hz) : 0;
          e.hp_type = edit.type; e.lp_type = edit.type;
        }
        break;
      case "sub_mode": {
        const e = sub(edit.role);
        e.mode = edit.mode.startsWith("bundle:") ? "bundle" : edit.mode;
        if (e.mode === "bundle") {
          e.bundle = edit.mode.slice(-1);
          settings.bundle_layouts[e.bundle] ??= { kind: "values", rear_trim_ms: 0 };
          const first = Object.entries(settings.subs_manual).find(([r, s]) => r !== edit.role && s.mode === "bundle" && s.bundle === e.bundle);
          if (first) share(first[0]);
        }
        break;
      }
      case "sub_layout": case "rear_trim": {
        const entry = settings.bundle_layouts[edit.bundle] ??= { kind: "values", rear_trim_ms: 0 };
        if (edit.kind === "sub_layout") entry.kind = edit.layout;
        else entry.rear_trim_ms = value ?? 0;
        const first = Object.entries(settings.subs_manual).find(([, s]) => s.mode === "bundle" && s.bundle === edit.bundle);
        if (first) share(first[0]);
        break;
      }
      case "sub": Object.assign(sub(edit.role), clone(edit.patch)); share(edit.role); break;
      case "sub_band_type": {
        const e = sub(edit.role);
        const hz = edit.band === "hp" ? "hp_hz" : "lp_hz";
        const type = edit.band === "hp" ? "hp_type" : "lp_type";
        e[hz] = edit.type === "off" ? 0 : e[hz] || (edit.band === "hp" ? 20 : 80);
        if (edit.type !== "off") e[type] = edit.type;
        share(edit.role); break;
      }
      case "mimo_on": settings.mimo_on = edit.on; break;
      case "mimo_fir": settings.mimo_fir = edit.on; break;
      case "mimo_seat_guard": settings.mimo_seat_guard = edit.on; break;
      case "analytic_derivatives":
        if (edit.peq !== undefined) settings.peq_analytic_derivatives = edit.peq;
        if (edit.mimo_iir !== undefined) settings.mimo_iir_analytic_derivatives = edit.mimo_iir;
        break;
    }
    // Which result falls behind is read from the generated key fields: a correction field makes the
    // correction (and MIMO on it) stale; a field only MIMO reads leaves the correction current and only
    // MIMO behind, which is what 'MIMO만 계산' is for; a field in neither key (MIMO on or off, the
    // recommendation's inclusion) changes the project only.
    if (correctionKey(settings) !== before.correction) dirty("settings", edit.kind !== "xo");
    else {
      project = { ...needProject(), dirty: true };
      if (mimoKey(settings) !== before.mimo) mimoBehind();
    }
    return settings;
  };
  const warnings = (): Notice[] => Object.entries(settings.subs_manual).flatMap(([role, sub]) => sub.hp_hz > 0 && sub.lp_hz > 0 && sub.lp_hz < 2 * sub.hp_hz
    ? [{ code: "sub_narrow_band", params: { role, hp_hz: String(sub.hp_hz), lp_hz: String(sub.lp_hz) } }] : []);
  /** A MIMO result on the current correction, from these settings (a `mimo` run, or the MIMO part of `calibrate`). */
  const commitMimo = (s: CalibrationSettings) => {
    const inputs = config.speakers.filter((speaker) => !speaker.role.startsWith("SW")).length;
    const supports = config.speakers.filter((speaker) => speaker.role.startsWith("SW")).length;
    const supportCount = inputs * supports;
    const selected = computeStatus();
    const gpu = s.mimo_fir && selected.selected === "cuda";
    const compute: import("./bindings/ComputeReport").ComputeReport = {
      requested: appSettings.compute, backend: gpu ? "cuda" : "cpu", device: gpu ? selected.device?.name ?? null : null,
      stage: gpu ? "ideal_fir_sweep" : null, accelerated_systems: gpu ? 11 : 0, cpu_systems: gpu ? 0 : 11, fallbacks: 0, reason: s.mimo_fir ? selected.reason : "iir_reference",
    };
    results = { ...results, mimo: { ...clone(fx.mimoSummary), compute, fsiso: s.fsiso, kind: s.mimo_fir ? "fir" : "iir",
      support_count: supportCount, fir_taps_total: s.mimo_fir ? 3 * supportCount : 0, input_count: supports > 0 ? inputs : 0,
      scorecard: results.calibration?.scorecard.map((score) => ({ role: score.role, after_mimo: supports > 0
        ? { ...score.after, in1_20: fx.scoreFigures(score.role).within[2], gd_20: fx.scoreFigures(score.role).gd[2] }
        : { ...score.after } })) ?? [],
      late_energy: results.calibration?.late_energy.map(({ role, position, after_db }) => ({ role, position,
        after_mimo_db: supports > 0 ? fx.lateAt(role, position)[2] : after_db })) ?? [] },
      mimo_export: "included" };
  };
  // Mock-only design values: compact wire summaries carry roles, not DSP filters.
  const resultSubs = () => results.calibration?.sub_roles.map((role, i) => ({
    ...clone(fx.signalChain.subs[i % fx.signalChain.subs.length]!), role,
  })) ?? [];
  /**
   * The service's recorded export of this kind (ADR 0052): its folder name,
   * files, output rate and notes, written below the explicit folder or the
   * export folder. A design name or extra rates that were not recorded get
   * the kind's record. Whether the result was current is this session's.
   */
  const exported = (params: ExportParams, stale: boolean): ExportResult => {
    const kind = RECORDED_EXPORTS.filter((entry) => entry.params.kind === params.kind);
    const entry = kind.find((e) => e.params.design_name === (params.design_name ?? null) && same(e.params.extra_rates, params.extra_rates ?? [])) ?? kind[0]!;
    const folder = params.folder || `${exportFolder.folder.replace(/[\\/]+$/, "")}/${entry.data.folder}`;
    return { ...clone(entry.data), folder, files: entry.data.files.map((file) => `${folder}/${file}`), stale };
  };
  /**
   * The recorded lines of the stored correction's settings (ADR 0052), with
   * this session's state: whether the correction is still current, which run
   * the BA lines come from, and the notes the stored summary carries.
   */
  const resultLines = (): ResultLinesResult => {
    const calibration = results.calibration;
    if (!calibration) return { stale: false, channels: [], total_delay_ms: null, notes: [], notices: [], rc: [], ba: null };
    const { lines } = recordedCalibration(resultKey);
    return { ...lines, stale: calibration.stale, notes: calibration.notes, notices: calibration.notices,
      ba: lines.ba && { ...lines.ba, stale: calibration.stale, subs_only: subsOnly } };
  };
  const resultCurves = (params: Extract<Request, { method: "result_curves" }>["params"]): ResultCurvesResult => {
    const role = params.channel ?? config.speakers.find((s) => !s.role.startsWith("SW"))?.role ?? "L";
    const hasMeasurement = records.some((r) => r.roles.includes(role));
    const curves: ResultCurvesResult["curves"] = hasMeasurement ? [{ kind: "before", role: null, values: fx.curveValues(1) }] : [];
    if (results.calibration) curves.push({ kind: "after", role: null, values: fx.curveValues(1, 0.2) });
    if (params.tab !== "gd") curves.push({ kind: "target", role: null, values: fx.curveGrid.map(() => 0) });
    if (params.tab === "spl" && results.mimo) curves.push({ kind: "mimo", role: null, values: fx.curveValues(1, 0.1) });
    if (params.tab === "xo" && hasMeasurement) {
      curves.push({ kind: "main", role, values: fx.curveValues(2) }, { kind: "sub", role: null, values: fx.curveValues(3) }, { kind: "sum", role: null, values: fx.curveValues(2, 0.4) });
    }
    if (params.tab === "ba") {
      curves.splice(0, curves.length);
      for (const s of config.speakers.filter((s) => s.role.startsWith("SW"))) {
        const seed = 2 + Number(s.role.slice(2)) / 4;
        if (records.some((r) => r.roles.includes(s.role))) curves.push({ kind: "before", role: s.role, values: fx.curveValues(seed) });
        if (results.calibration) curves.push({ kind: "after", role: s.role, values: fx.curveValues(seed, 0.3) });
      }
      if (curves.length) curves.push({ kind: "sum_before", role: null, values: fx.curveValues(1) }, { kind: "sum_after", role: null, values: fx.curveValues(1, 0.2) });
    }
    return { tab: params.tab, grid: [...fx.curveGrid], curves,
      axes: { frequency_hz: [5, params.tab === "ba" ? 300 : params.tab === "xo" ? 500 : 20000], value: params.tab === "gd" ? [-10, 90] : [-30, 15], unit: params.tab === "gd" ? "ms" : "db" },
      stale: results.calibration?.stale ?? false, subs_only: subsOnly, rc_band_hz: [settings.rc_lo, settings.rc_hi],
      // The extra bands (ADR 0024) where the graph shows the RC band.
      rc_bands_hz: params.tab === "spl" || params.tab === "gd" ? settings.rc_bands.map((band) => [band.lo_hz, band.hi_hz] as [number, number]) : [],
      xo_hz: params.tab === "xo" ? params.xo_hz ?? settings.xo[params.group ?? fx.GROUP_KEYS[fx.groupOf(role)]!] ?? 80 : null };
  };

  const answer = (req: Request): Response => {
    switch (req.method) {
      case "bootstrap": return { method: req.method, data: { ...fx.bootstrapInfo, log_dir: "C:/Users/demo/AppData/Roaming/SECS2R/logs" } };
      case "project_info": return { method: req.method, data: needProject() };
      case "project_new": {
        const p = req.params;
        const sample = p.sample || p.tutorial;
        reset({ ...fx.emptyProject, path: p.path, name: basename(p.path).replace(/\.secs$/i, ""), preset: sample ? "7.1.4" : p.preset, sub_count: sample ? 3 : p.sub_count, tutorial: p.tutorial, dirty: false }, true);
        if (sample) layout = { ...layout, count: 1 };
        measureConfig = { ...measureConfig, method: "none" };
        return { method: req.method, data: needProject() };
      }
      case "project_open": {
        const saved = savedProjects.get(req.params.path);
        if (saved) {
          restore(saved);
          if (results.calibration) results.calibration.cached = true;
          calibratedKey = null;
        } else reset({ ...fx.projectInfo, path: req.params.path, name: basename(req.params.path).replace(/\.secs$/i, ""), dirty: false });
        return { method: req.method, data: needProject() };
      }
      case "project_save": {
        const current = needProject();
        if (current.tutorial) throw new MockError(ipcError("tutorial_blocked"));
        const path = req.params.path ?? current.path;
        project = { ...current, path, name: basename(path).replace(/\.secs$/i, ""), dirty: false };
        if (!exportFolder.stored) exportFolder = { folder: `${path.replace(/[^\\/\\\\]+$/, "")}SECS_export/2026-10-24 ${project.name}`, stored: false };
        savedProjects.set(path, state());
        return { method: req.method, data: project };
      }
      case "results": return { method: req.method, data: results };
      case "jobs": return { method: req.method, data: [...jobs.values()].map(snapshot) };
      case "job_cancel": {
        const job = jobs.get(req.params.job);
        if (!job) throw new MockError(ipcError("job_not_found", `job ${req.params.job} is not running`));
        job.cancelRequested = true;
        return { method: req.method, data: snapshot(job) };
      }
      case "measure_methods": return { method: req.method, data: methods() };
      case "measure_config_get": needProject(); return { method: req.method, data: measureConfig };
      case "measure_configure": {
        needProject();
        const before = JSON.stringify(config);
        const p = req.params;
        if (!["none", "mdat", ...fx.measureMethods.methods].includes(p.method)) invalid("unknown measurement method");
        if (p.repetitions !== null && !(REPETITIONS as readonly number[]).includes(p.repetitions)) invalid("invalid repetitions");
        measureConfig = { ...measureConfig, method: p.method, input_device: p.input_device ?? measureConfig.input_device,
          output_device: p.output_device ?? measureConfig.output_device, input_channel: p.input_channel ?? measureConfig.input_channel,
          sweep_length: p.sweep_length ?? measureConfig.sweep_length, repetitions: p.repetitions ?? measureConfig.repetitions,
          sweep_level_dbfs: p.sweep_level_dbfs ?? measureConfig.sweep_level_dbfs, timing: p.timing ?? measureConfig.timing,
          loopback_input_channel: p.loopback_input_channel ?? measureConfig.loopback_input_channel,
          timing_ref_output: p.timing_ref_output ?? measureConfig.timing_ref_output, share_mode: p.share_mode ?? measureConfig.share_mode };
        if (p.output_device) config.device = p.output_device;
        if (p.output_channels.length) {
          config = fx.configOf(config.preset, config.speakers.map((s, i) => ({ ...s, bands: s.bands.map((b, j) => j === 0 ? { ...b, out: p.output_channels[i] ?? b.out } : b) })), config.device);
          syncMeasure();
        }
        layout.timing.reference = measureConfig.timing;
        layout.timing.output = measureConfig.timing_ref_output ?? layout.timing.output;
        // REW mode opens a listed device in REW, as the service does.
        if (p.method === "rew") {
          if (p.input_device && rew.input_devices?.some((d) => d.name === p.input_device)) rew.input_device = p.input_device;
          if (p.output_device && rew.output_devices?.some((d) => d.name === p.output_device)) rew.output_device = p.output_device;
        }
        // Service config_key includes device and wiring, not capture options.
        if (JSON.stringify(config) !== before) dirty("config");
        else project = { ...needProject(), dirty: true };
        return { method: req.method, data: measureConfig };
      }
      case "mic_calibration_get": return { method: req.method, data: micCalStatus() };
      case "mic_calibration_set":
        editMicCal();
        micCalFile = { ...clone(umikFile), name: basename(req.params.path), path: req.params.path };
        project = { ...needProject(), dirty: true };
        return { method: req.method, data: micCalStatus() };
      case "mic_calibration_clear":
        editMicCal(); micCalFile = null;
        project = { ...needProject(), dirty: true };
        return { method: req.method, data: micCalStatus() };
      case "mic_calibration_attach": {
        editMicCal();
        if (!micCalFile) throw new MockError(ipcError("measurement_input", "no microphone calibration file is selected", { reason: "mic_calibration", issue: "no_file", file: "" }));
        const changed = micCalStatus().attachable > 0;
        for (const record of records) for (const role of record.roles) {
          if (record.sources[role] === "measured") micCalAttached.add(micCalKey(record.position, role));
        }
        if (changed) results = {
          ...results,
          calibration: results.calibration ? { ...results.calibration, stale: true, stale_reason: "measurements" } : null,
          mimo: results.mimo ? { ...results.mimo, stale: true } : null,
        };
        return { method: req.method, data: micCalStatus() };
      }
      case "measurement_records": {
        needProject();
        const positions = [...new Set([...Array.from({ length: layout.count }, (_, i) => i + 1), ...records.map((r) => r.position)])].sort((a, b) => a - b);
        return { method: req.method, data: { records: positions.map((position) => records.find((r) => r.position === position) ?? { position, roles: [], done: false, sources: {} }) } };
      }
      case "measurement_curves": {
        needProject();
        // One reference for every curve, chosen before the request's filter (U17).
        const measured = records.flatMap((r) => r.roles.map((role, i) => ({ position: r.position, role, kind: measurementKind(role), values: mockMeasurement(role, i + r.position) })));
        const { curves, reference } = shiftToReference(fx.curveGrid, measured, config.speakers.map((s) => s.role));
        const { positions, roles } = req.params;
        return { method: req.method, data: { grid: [...fx.curveGrid], axes: { frequency_hz: [5, 22000], value: [-35, 20], unit: "db" }, reference,
          curves: curves.filter((c) => (!positions || positions.includes(c.position)) && (!roles || roles.includes(c.role))) } };
      }
      case "levels_get": return { method: req.method, data: levels };
      case "levels_fill_defaults":
        for (const role of measureConfig.roles) levels.levels[role] ??= -12;
        project = { ...needProject(), dirty: true };
        return { method: req.method, data: levels };
      case "level_stop": {
        const active = playing; playing = null;
        if (active) {
          const monitorJob = [...jobs.values()].find((job) => job.kind === "level_monitor");
          if (monitorJob) events.emit({ event: "log", job: monitorJob.id, code: "level_play_end", params: { role: active.role, outcome: "stopped" } });
        }
        return { method: req.method, data: null };
      }
      case "level_play": {
        needProject();
        if (playing) {
          const monitorJob = [...jobs.values()].find((job) => job.kind === "level_monitor");
          if (monitorJob) events.emit({ event: "log", job: monitorJob.id, code: "level_play_end", params: { role: playing.role, outcome: "stopped" } });
        }
        playing = { role: req.params.role, master: req.params.master_dbfs, ticks: 5 };
        return { method: req.method, data: null };
      }
      case "rew_launch": return { method: req.method, data: fx.rewStatus };
      case "rew_info": return { method: req.method, data: clone(rew) };
      case "rew_driver_set":
        if (!req.params.driver.trim()) invalid("driver must not be empty");
        rew.driver = req.params.driver;
        return { method: req.method, data: clone(rew) };
      case "position_clear": {
        needProject(); const token = remember();
        for (const role of records.find((r) => r.position === req.params.position)?.roles ?? []) micCalAttached.delete(micCalKey(req.params.position, role));
        records = records.filter((r) => r.position !== req.params.position);
        project = { ...needProject(), positions: records.map((r) => r.position) }; dirty("measurements");
        return { method: req.method, data: { token, project: needProject() } };
      }
      case "undo": {
        const saved = undos.get(req.params.token);
        if (!saved) invalid("unknown or consumed undo token");
        restore(saved); undos.delete(req.params.token); project = { ...needProject(), dirty: true };
        return { method: req.method, data: needProject() };
      }
      case "sample_load": {
        needProject(); const count = req.params.positions ?? 1;
        return { method: req.method, data: start("sample_load", () => {
          for (let pos = 1; pos <= count; pos++) recordPosition(pos);
          return { kind: "measurement", data: needProject() };
        }) };
      }
      case "calibrate": {
        needProject();
        if (req.params.settings) settings = clone(req.params.settings);
        const captured = clone(settings);
        captured.rc_bands = validRcBands(captured.rc_bands);
        const revision = inputRevision;
        const positions = needProject().positions.length;
        const withMimo = !!req.params.mimo && !req.params.subs_only;
        // The service's recorded summary of a calibration on these settings (ADR 0052); only whether
        // the inputs changed while it ran is this session's.
        const commit = () => {
          const key = recordKey(captured);
          const summary = recordedCalibration(key).results.calibration!;
          if (revision !== inputRevision) { summary.stale = true; summary.stale_reason = results.calibration?.stale_reason ?? "settings"; }
          if (!req.params.subs_only) {
            results = { ...results, calibration: summary };
            mimoBehind();
            calibratedKey = correctionKey(captured);
            resultKey = key;
          }
          subsOnly = req.params.subs_only;
        };
        const error = rcAgreementError(captured, positions);
        if (!withMimo) return { method: req.method, data: start("calibrate", () => { commit(); return { kind: "calibration" }; }, error) };
        // MIMO follows in the same job on the stored correction; it fails as a separate MIMO run would, with `stage: "mimo"`.
        const next = () => {
          const failure = jobFailures.get("mimo");
          return failure ? { ...failure, params: { ...failure.params, stage: "mimo" } } : undefined;
        };
        return { method: req.method, data: start("calibrate", () => { commitMimo(captured); return { kind: "calibration" }; }, error, { stages: plan("mimo").stages, phase: { commit, next } }) };
      }
      case "recommend_xo":
        needProject();
        return { method: req.method, data: start("recommend_xo", () => {
          const best: [string, number][] = config.groups.filter((g) => g.id !== "sub").map((g) => [g.key, g.id === "center" || g.id === "top" ? 100 : 80]);
          settings.xo_rec = Object.fromEntries(best); settings.xo = { ...settings.xo, ...settings.xo_rec };
          dirty("settings", false);
          settings.xo_rec_key = `mock-recommendation-${inputRevision}`;
          results = { ...results, xo_rec_stale: false };
          return { kind: "xo", data: { best, candidates: [60, 80, 100, 120] } };
        }) };
      case "mimo": {
        needProject();
        // start_mimo compares raw engine keys; only calibration snapshots normalize bands.
        const captured = clone(req.params.settings ?? settings);
        if (!results.calibration || results.calibration.cached || calibratedKey === null) throw new MockError(ipcError("engine_input", "calibration must run before MIMO"));
        if (results.calibration.stale) throw new MockError(ipcError("stale_inputs", "calibration result is stale"));
        if (correctionKey(captured) !== calibratedKey) throw new MockError(ipcError("stale_inputs", "MIMO settings do not match the calibration result"));
        return { method: req.method, data: start("mimo", () => {
          commitMimo(captured);
          return { kind: "mimo" };
        }) };
      }
      case "export": {
        const project = needProject();
        if (project.tutorial) throw new MockError(ipcError("tutorial_blocked"));
        const kind = req.params.kind;
        const nameProblem = req.params.design_name === "" ? "length" : designNameProblem(req.params.design_name ?? "");
        if (nameProblem) throw new MockError(ipcError("invalid_request", "invalid design name", { reason: `design_name_${nameProblem}` }));
        const designName = designNameFor({ designName: req.params.design_name ?? "", tutorial: false });
        if (designName !== null && !isFilterKind(kind)) throw new MockError(ipcError("invalid_request", "design_name only applies to filter exports", { reason: "design_name" }));
        const devices = req.params.camilla_devices;
        if (devices && kind !== "camilla_dsp") throw new MockError(ipcError("invalid_request", "camilla_devices only applies to CamillaDSP", { reason: "camilla_devices" }));
        if (devices) {
          if ([devices.capture, devices.playback].some((device) => device.type === "asio" && device.device.trim() === "")) {
            throw new MockError(ipcError("invalid_request", "invalid CamillaDSP device options", { reason: "camilla_asio_device" }));
          }
          if (devices.capture.type === "wasapi" && devices.capture.loopback && devices.capture.exclusive) {
            throw new MockError(ipcError("invalid_request", "invalid CamillaDSP device options", { reason: "camilla_wasapi_loopback_exclusive" }));
          }
        }
        if (req.params.mdat_scope !== undefined && kind !== "before_after_mdat") {
          throw new MockError(ipcError("invalid_request", "mdat_scope is only valid for REW mdat", { reason: "mdat_scope" }));
        }
        const extraRates = req.params.extra_rates ?? [];
        if (extraRates.length > 0 && kind !== "convolution") {
          throw new MockError(ipcError("invalid_request", "extra_rates only applies to convolution export", { reason: "extra_rates" }));
        }
        for (const rate of extraRates) {
          if (!(OUTPUT_RATES as readonly number[]).includes(rate)) throw new MockError(ipcError("invalid_request", "unsupported extra output sample rate", { rate: String(rate) }));
        }
        const scope = req.params.mdat_scope ?? "all";
        if (kind === "before_after_mdat" && scope === "measurements") {
          if (project.positions.length === 0) throw new MockError(ipcError("engine_input", "no measurements to export", { reason: "no_measurements" }));
          return { method: req.method, data: start("export", () => ({ kind: "export", data: exported(req.params, false) })) };
        }
        if (!results.calibration) throw new MockError(ipcError("engine_input", "no calibration result to export"));
        if ((kind === "staged_mdat" || kind === "before_after_mdat") && results.calibration.cached && results.calibration.stale) {
          throw new MockError(ipcError("export_scope", `${kind} requires a current calibration result`, { reason: "stale_cache" }));
        }
        const calibration = results.calibration;
        if (kind === "filter_text" || kind === "equalizer_apo") {
          const reason = kind === "filter_text" ? calibration.sub_roles.length > 0 ? "subs" : results.mimo_export === "included" ? "mimo" : null : null;
          if (reason) throw new MockError(ipcError("export_scope", `${kind} cannot export a result with ${reason}`, { reason }));
        }
        return { method: req.method, data: start("export", () => ({ kind: "export", data: exported(req.params, calibration.stale) })) };
      }
      case "measure_position": {
        needProject(); const position = req.params.position; const roles = [...measureConfig.roles];
        if (!Number.isInteger(position) || position < 1 || position > layout.count) invalid("position is outside the arrangement");
        const calibrated = measureConfig.method === "native" && micCalFile !== null;
        return { method: req.method, data: start("measure", () => { recordPosition(position, roles, undefined, calibrated); return { kind: "measurement", data: needProject() }; }) };
      }
      case "mdat_scan":
        return { method: req.method, data: {
          measurements: measureConfig.roles.map((role, index) => ({ index, title: `${role} P1`, role, position: 1, position_tagged: true, looks_like_sub: role.startsWith("SW") })),
          suggestions: measureConfig.roles.map((role, index) => ({ measurement: `${role} P1`, role, index, position: 1 })), problems: [],
        } };
      case "mdat_import":
        needProject();
        return { method: req.method, data: start("mdat_import", () => {
          for (const assignment of req.params.assignments) recordPosition(assignment.position ?? req.params.position, [assignment.role], req.params.path);
          return { kind: "mdat_import", data: needProject() };
        }) };
      case "config_get": needProject(); return { method: req.method, data: config };
      case "config_apply_preset": {
        const next = fx.presetConfig(req.params.preset) ?? invalid("unknown configuration preset");
        const token = clearMeasurements(req.params.confirm);
        next.device = config.device;
        return { method: req.method, data: { config: updateConfig(next), undo_token: token } };
      }
      case "config_set_sub_count": {
        if (!Number.isInteger(req.params.count) || req.params.count < 0 || req.params.count > 8) invalid("sub count must be 0–8");
        const token = req.params.count === config.summary.sub_count ? null : clearMeasurements(req.params.confirm);
        const speakers = config.speakers.filter((s) => !s.role.startsWith("SW"));
        const oldSubs = config.speakers.filter((s) => s.role.startsWith("SW"));
        const subs = Array.from({ length: req.params.count }, (_, i) => oldSubs[i] ?? fx.speaker(`SW${i + 1}`, config.summary.max_channel + i + 1));
        const at = fx.subsAt(speakers.map((s) => s.role));
        return { method: req.method, data: { config: updateConfig(fx.configOf(config.preset, [...speakers.slice(0, at), ...subs, ...speakers.slice(at)], config.device)), undo_token: token } };
      }
      case "config_set_speaker": {
        const p = req.params; const speakers = clone(config.speakers); const s = speakers[p.index];
        if (!s) invalid("speaker index is outside the configuration");
        if (p.role !== null) { s.role = p.role; if (s.role.startsWith("SW")) { s.size = "sub"; s.bands = [{ band: "SUB", out: s.bands[0]?.out ?? 1, pol: "+" }]; } }
        if (p.size !== null) s.size = p.size;
        const band = s.bands[p.band_index ?? 0];
        if (band) { if (p.out !== null) band.out = p.out; if (p.pol !== null) band.pol = p.pol; }
        return { method: req.method, data: updateConfig(fx.configOf("custom", speakers, config.device)) };
      }
      case "config_add_speaker": {
        const role = [...fx.SAMPLE_ROLES, "Lw", "Rw", "Tml", "Tmr"].find((r) => !config.speakers.some((s) => s.role === r)) ?? "L";
        return { method: req.method, data: updateConfig(fx.configOf("custom", [...config.speakers, fx.speaker(role, config.summary.max_channel + 1)], config.device)) };
      }
      case "config_remove_speaker": return { method: req.method, data: updateConfig(fx.configOf("custom", config.speakers.filter((_, i) => i !== req.params.index), config.device)) };
      case "config_renumber": {
        let out = 0;
        return { method: req.method, data: updateConfig(fx.configOf("custom", config.speakers.map((s) => ({ ...s, bands: s.bands.map((b) => ({ ...b, out: ++out })) })), config.device)) };
      }
      case "config_validate": {
        const notices: Notice[] = [];
        const outputs = new Map<number, string>();
        for (const speaker of config.speakers) for (const band of speaker.bands) {
          if (band.out < 1) continue;
          const firstRole = outputs.get(band.out);
          if (firstRole !== undefined) notices.push({ code: "out_duplicate", params: { out: String(band.out), first_role: firstRole, role: speaker.role } });
          else outputs.set(band.out, speaker.role);
        }
        for (const [i, s] of config.speakers.entries()) if (config.speakers.findIndex((other) => other.role === s.role) !== i) notices.push({ code: "role_duplicate", params: { role: s.role } });
        return { method: req.method, data: notices };
      }
      case "patch_sheet_csv": return { method: req.method, data: { path: req.params.path } };
      case "dsp_get": return { method: req.method, data: dsp };
      case "dsp_set": {
        const preset = dsp.presets.find((p) => p.key === req.params.preset);
        if (req.params.preset !== null && !preset) invalid("unknown DSP preset");
        if (preset) dsp = { ...dsp, profile: preset.key, name: preset.name, fields: clone(preset.fields) };
        const fields = req.params.fields;
        // State only: the fields as sent, and the profile kept while only the output rate changes
        // (ADR 0017: the rate stays out of the design, so the result stays current).
        const rateOnly = !!fields && (Object.keys(fields) as (keyof typeof fields)[]).every((key) => key === "output_rate" || key === "output_rate_editable" || fields[key] === dsp.fields[key]);
        if (fields) {
          const profile = rateOnly ? dsp.profile : "custom";
          dsp = { ...dsp, profile, fields: { ...clone(fields), output_rate_editable: profile === "camilla" || profile === "custom" } };
        }
        if (rateOnly) project = { ...needProject(), dirty: true }; else dirty();
        return { method: req.method, data: dsp };
      }
      case "layout_get": return { method: req.method, data: layout };
      case "layout_set": {
        const p = req.params;
        if (p.count !== null && !layout.arrangements.some((a) => a.count === p.count)) invalid("unknown arrangement");
        const token = p.count !== null && p.count !== layout.count ? clearMeasurements(p.confirm) : null;
        layout = { ...layout, count: p.count ?? layout.count, start_delay: p.start_delay ?? layout.start_delay, timing: p.timing ?? layout.timing };
        measureConfig.timing = layout.timing.reference; measureConfig.timing_ref_output = layout.timing.output;
        dirty(); return { method: req.method, data: { layout, undo_token: token } };
      }
      case "position_estimate": {
        const delay = Number.parseInt(layout.start_delay, 10) || 0;
        const per = measureConfig.roles.length * ((measureConfig.timing === "acoustic" ? 2 : 1) * measureConfig.sweep_length / measureConfig.sample_rate * measureConfig.repetitions + 0.7) + delay;
        return { method: req.method, data: { positions: layout.count, per_position_seconds: per, total_seconds: per * layout.count } };
      }
      case "target_get": return { method: req.method, data: target };
      case "target_set_points": target = { ...target, points: clone(req.params.points), preset: null }; dirty("target"); return { method: req.method, data: target };
      case "target_apply_preset":
        target = recorded("target_apply_preset", (params) => params.preset === req.params.preset) ?? invalid("unknown target preset");
        dirty("target"); return { method: req.method, data: target };
      case "target_save": savedTargets.set(req.params.path, clone(target)); return { method: req.method, data: { path: req.params.path } };
      case "target_load": target = clone(savedTargets.get(req.params.path) ?? fx.target); dirty("target"); return { method: req.method, data: target };
      case "calibration_settings_get": return { method: req.method, data: settings };
      case "calibration_settings_set": settings = clone(req.params.settings); dirty(); return { method: req.method, data: settings };
      case "calibration_settings_edit": needProject(); return { method: req.method, data: editSettings(req.params.edit) };
      case "sub_warnings": return { method: req.method, data: warnings() };
      case "mode_spec": return { method: req.method, data: fx.modeSpec(req.params.mode, req.params.mimo_fir ?? false) };
      case "open_gpu_help": return { method: req.method, data: null };
      case "compute_status": return { method: req.method, data: computeStatus() };
      case "local_api_status":
      case "local_api_token_regenerate": return { method: req.method, data: {
        ...appSettings.local_api, running: appSettings.local_api.enabled, error: null,
        url: `http://127.0.0.1:${appSettings.local_api.port}`,
        token: "mock-local-api-token", token_path: "C:/Users/demo/AppData/Roaming/SECS2R/local-api-token",
      } };
      case "update_status": return { method: req.method, data: updateStatus };
      // The updater's phases follow the app's recorded transitions (update_transitions.json).
      case "update_check":
      case "update_download":
        updateStatus = { ...updateStatus, phase: nextPhase(req.method, updateStatus.policy, updateStatus.phase) };
        return { method: req.method, data: updateStatus };
      case "update_restart":
        updateStatus = { ...updateStatus, phase: nextPhase(req.method, updateStatus.policy, updateStatus.phase) };
        return { method: req.method, data: null };
      case "update_notice_dismiss":
        updateStatus = { ...updateStatus, notice: null, phase: nextPhase(req.method, updateStatus.policy, updateStatus.phase) };
        return { method: req.method, data: updateStatus };
      case "update_open_release":
        if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z]+(\.[0-9A-Za-z]+)*)?$/.test(req.params.version)) return invalid("Not a release version");
        return { method: req.method, data: null };
      case "app_settings_get": return { method: req.method, data: appSettings };
      case "app_settings_set": {
        if (req.params.settings.compute === "gpu" && !availableCompute.gpu_available) throw new MockError(ipcError("compute_unavailable"));
        if (req.params.settings.compute !== appSettings.compute) mimoBehind();
        if (req.params.settings.updates !== appSettings.updates) {
          const policy = req.params.settings.updates;
          updateStatus = { ...updateStatus, policy, phase: nextPhase("app_settings_set", policy, updateStatus.phase) };
        }
        appSettings = clone(req.params.settings);
        appSettings.recent_projects = appSettings.recent_projects.slice(0, RECENT_PROJECTS_MAX);
        return { method: req.method, data: appSettings };
      }
      // 적용 (ADR 0018, crates/secs-service/src/apply.rs): the order of the service's checks.
      case "apply_status": return { method: req.method, data: clone({ ...applyStatus, current_design_id: results.calibration ? fx.MOCK_DESIGN_ID : null }) };
      case "apply_settings_set": {
        const address = req.params.camilla.address;
        if (address !== address.trim() || addressProblem(address)) throw new MockError(ipcError("invalid_request", "CamillaDSP address must be host:port", { field: "camilla.address" }));
        appSettings = { ...appSettings, apply: clone(req.params) };
        return { method: req.method, data: clone(req.params) };
      }
      case "apply_camilla": {
        const camilla = appSettings.apply.camilla;
        // 관리 needs camilladsp.exe before anything else (apply.rs `start_apply_camilla`).
        if (camilla.level === "manage" && !camilla.exe_path?.trim()) throw new MockError(ipcError("invalid_request", "managed CamillaDSP needs an executable path", { field: "apply.camilla.exe_path" }));
        if (camilla.level === "files") throw new MockError(ipcError("unsupported", "the target is configured for file export only", { target: "camilladsp" }));
        if (needProject().tutorial) throw new MockError(ipcError("tutorial_blocked", "tutorial sessions cannot export"));
        const devices = req.params.camilla_devices;
        if (devices && [devices.capture, devices.playback].some((device) => device.type === "asio" && device.device.trim() === "")) {
          throw new MockError(ipcError("invalid_request", "invalid CamillaDSP device options", { reason: "camilla_asio_device" }));
        }
        if (!results.calibration) throw new MockError(ipcError("engine_input", "no calibration result to export"));
        const stale = results.calibration.stale;
        const exePath = camilla.level === "manage" ? camilla.exe_path ?? null : null;
        const port = Number(camilla.address.slice(camilla.address.lastIndexOf(":") + 1));
        return { method: req.method, data: start("apply", () => {
          // 관리 hands the design to the service after 적용, or instead of it when no CamillaDSP answers
          // (`handed_over`: the service starts one with the design); without a running service the job fails.
          const handedOver = exePath !== null && !applyStatus.camilla.reachable;
          if (exePath !== null) manage({ camilla: { ...fx.managedCamilla, exe_path: exePath, port } });
          // Mock only: a CamillaDSP that answers now runs this design.
          if (applyStatus.camilla.reachable) applyStatus = { ...applyStatus, camilla: { ...applyStatus.camilla, design: { kind: "this", design_id: fx.MOCK_DESIGN_ID } } };
          return { kind: "apply", data: { outcome: { kind: handedOver ? "handed_over" : "applied" }, design_id: fx.MOCK_DESIGN_ID, folder: APPLIED_CAMILLA, stale } };
        }) };
      }
      case "apply_eqapo": {
        if (appSettings.apply.eqapo.level === "files") throw new MockError(ipcError("unsupported", "the target is configured for file export only", { target: "equalizer_apo" }));
        const install = applyStatus.eqapo;
        if (!install.installed || install.config_path === null) throw new MockError(ipcError("unsupported", "Equalizer APO is not installed", { target: "equalizer_apo" }));
        if (needProject().tutorial) throw new MockError(ipcError("tutorial_blocked", "tutorial sessions cannot export"));
        if (!results.calibration) throw new MockError(ipcError("engine_input", "no calibration result to export"));
        const stale = results.calibration.stale;
        const folder = `${install.config_path}/SECS2R`;
        const managed = appSettings.apply.eqapo.level === "manage";
        const configPath = install.config_path;
        return { method: req.method, data: start("apply", () => {
          if (managed) manage({ eqapo: { config_path: configPath, design_id: fx.MOCK_DESIGN_ID } });
          applyStatus = { ...applyStatus, eqapo: { ...applyStatus.eqapo, include: "present", applied_design_id: fx.MOCK_DESIGN_ID } };
          return { kind: "apply", data: { outcome: { kind: "applied" }, design_id: fx.MOCK_DESIGN_ID, folder, stale } };
        }) };
      }
      case "revert_eqapo":
        if (!applyStatus.eqapo.installed) throw new MockError(ipcError("unsupported", "Equalizer APO is not installed", { target: "equalizer_apo" }));
        // Only the two marked lines go; the SECS2R folder and its secs-design.json stay.
        applyStatus = { ...applyStatus, eqapo: { ...applyStatus.eqapo, include: "absent" } };
        return { method: req.method, data: null };
      // 관리 (ADR 0018). Installing registers the service and it starts at once (automatic start); Windows's approval is not modelled.
      case "host_install":
        applyStatus = { ...applyStatus, host: "running", host_status: clone(applyStatus.host_status ?? fx.hostIdle) };
        return { method: req.method, data: null };
      case "host_uninstall": {
        const { host_status: _hostStatus, ...withoutHostStatus } = applyStatus;
        applyStatus = { ...withoutHostStatus, host: "not_installed" };
        return { method: req.method, data: null };
      }
      case "host_stop_managing": {
        // StopManaging forgets both targets and ends CamillaDSP; config.txt keeps its line.
        const host = hostReached();
        applyStatus = { ...applyStatus, host_status: { ...host, managed: {}, camilla: clone(fx.hostIdle.camilla), eqapo_include: "absent" } };
        return { method: req.method, data: null };
      }
      case "result_curves": return { method: req.method, data: resultCurves(req.params) };
      case "result_lines": return { method: req.method, data: resultLines() };
      case "sub_take_values":
        for (const s of resultSubs()) settings.subs_manual[s.role] = { ...clone(fx.subEntry), ...clone(s), mode: "fixed" };
        dirty(); return { method: req.method, data: settings };
      case "dialog_open_project": return { method: req.method, data: dialog(req.method, fx.projectInfo.path) };
      case "dialog_save_project": return { method: req.method, data: dialog(req.method, `C:/Users/demo/Documents/SECS/${req.params.suggested || "project.secs"}`) };
      case "dialog_pick_mdat": return { method: req.method, data: dialog(req.method, "C:/Users/demo/Documents/SECS/measurements.mdat") };
      case "dialog_pick_folder": return { method: req.method, data: dialog(req.method, req.params.start || exportFolder.folder) };
      case "dialog_save_file": return { method: req.method, data: dialog(req.method, `C:/Users/demo/Documents/SECS/${req.params.suggested || "target.json"}`) };
      case "dialog_open_file": {
        const fallback = req.params.kind === CAMILLA_EXE_FILE ? fx.CAMILLA_EXE : req.params.kind === "mic_calibration" ? "C:/Users/demo/Documents/UMIK-1/7000001_90deg.txt" : "C:/Users/demo/Documents/SECS/target.json";
        return { method: req.method, data: dialog(req.method, fallback) };
      }
      case "open_folder": return { method: req.method, data: null };
      case "level_monitor_start": return { method: req.method, data: start("level_monitor", () => ({ kind: "stopped" })) };
      case "level_monitor_stop": return { method: req.method, data: stopMonitor("level_monitor") };
      case "status_watch_start": return { method: req.method, data: start("status_watch", () => ({ kind: "stopped" })) };
      case "status_watch_stop": return { method: req.method, data: stopMonitor("status_watch") };
      case "export_folder_get": return { method: req.method, data: exportFolder };
      case "export_folder_set": exportFolder = { folder: req.params.folder, stored: true }; project = { ...needProject(), dirty: true }; return { method: req.method, data: exportFolder };
      default: { const exhaustive: never = req; return exhaustive; }
    }
  };
  const invoke: Invoke = async (command, args) => {
    if (command !== "secs_call") return { ok: false, error: ipcError("invalid_request", `unknown command ${command}`) };
    const req = clone(args.request as Request);
    calls.push(req);
    const failure = failures.get(req.method);
    if (failure) { if (failure.once) failures.delete(req.method); return { ok: false, error: clone(failure.error) }; }
    try { return { ok: true, data: clone(answer(req)) }; }
    catch (error) { return { ok: false, error: error instanceof MockError ? error.error : ipcError("internal", String(error)) }; }
  };
  return {
    invoke, request: makeClient(invoke), events, calls,
    failNext: (method, code, detail = "", params = {}) => failures.set(method, { error: ipcError(code, detail, params), once: true }),
    failAlways: (method, code, detail = "") => failures.set(method, { error: ipcError(code, detail), once: false }),
    clearFailures: () => failures.clear(), failJobs: (kind, code, detail = "", params = {}) => jobFailures.set(kind, ipcError(code, detail, params)),
    failLevelPlay: (code, detail = "", params = {}) => { playFailure = ipcError(code, detail, params); },
    queueDialog: (method, path) => { const queue = dialogs.get(method) ?? []; queue.push(path); dialogs.set(method, queue); },
    tick, busy,
    drain: () => { for (let guard = 0; busy() && guard < 1000; guard++) tick(); },
    setApplyStatus: (next) => { applyStatus = clone(next); },
    setUpdateStatus: (next) => { updateStatus = clone(next); appSettings.updates = next.policy; },
    setResults: (next) => {
      results = clone(next);
      calibratedKey = results.calibration && !results.calibration.cached ? correctionKey(settings) : null;
      resultKey = recordKey(settings);
    },
    dispose: () => { if (timer) clearInterval(timer); timer = null; jobs.clear(); playing = null; },
  };
}
