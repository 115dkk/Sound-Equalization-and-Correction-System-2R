// Stateful service double: every command returns an envelope, and jobs push
// events through the same bus as the real transport. Values are synthetic;
// configuration edits model the service contract, not the numerical engine.
// The mock owns the contract only (ADR 0014): rule values come from the
// generated `bindings/rules`, and no-input responses (mode specs, presets,
// new-project settings) are the service's recorded answers, returned as is.
import type { Invoke } from "./client";
import { makeClient } from "./client";
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
import type { ExportKind } from "./bindings/ExportKind";
import type { CurveReference } from "./bindings/CurveReference";
import type { MeasurementCurve } from "./bindings/MeasurementCurve";
import { CALIBRATION_LIMITS as LIMITS, RECENT_PROJECTS_MAX } from "./bindings/rules";
import { isSubRole, roleKind } from "../ui/maps/geometry";
import { recorded } from "./recorded";
import { CAMILLA_EXE_FILE, addressProblem, designNameFor, designNameProblem, filterModeName, isFilterKind, mdatFileName, MIMO_MIN_POSITIONS } from "../screens/export/types";
import { DESIGN_RATE, OUTPUT_RATES, countedRate, sameRate } from "../screens/system/outputRate";

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
  /** The default playback device's rate that 자동 reads when exporting (48000 by default); null when it cannot be read. */
  playbackRate?: number | null;
};
export const ipcError = (code: ErrorCode, detail = "", params: Record<string, string> = {}): IpcError => ({
  code, params, retryable: code === "job_busy" || code === "rew_unreachable" || code === "device_unavailable", detail,
});
class MockError extends Error {
  constructor(readonly error: IpcError) { super(error.code); }
}
const clone = <T,>(value: T): T => structuredClone(value);
const clamp = (value: number, [lo, hi]: readonly [number, number]) => Math.min(hi, Math.max(lo, value));
const monitor = (kind: JobKind) => kind === "level_monitor" || kind === "status_watch";
/** `JobGroup::Export` (jobs.rs): an apply job is an export job too, one at a time. */
const jobGroup = (kind: JobKind): JobKind => kind === "apply" ? "export" : kind;
/** Where the service writes what it applies to CamillaDSP (`app_data_path/applied/camilladsp`). */
const APPLIED_CAMILLA = "C:/Users/demo/AppData/Roaming/SECS2R/applied/camilladsp";
const basename = (path: string) => path.split(/[\\/]/).at(-1) ?? path;
/** The export kind in a timestamped folder name (export_folder.rs). */
const EXPORT_FOLDER_TAG: Record<ExportKind, string> = {
  camilla_dsp: "camilla", before_after_mdat: "mdat", staged_mdat: "mdat", filter_text: "text", equalizer_apo: "eqapo", convolution: "convolution",
};
/**
 * The RC upper limit a calibration notes (secs-engine `Note::RcLimit`, ADR
 * 0021), with synthetic evidence: ADR 0021's examples, a strict Schroeder
 * frequency of 154 Hz, the lenient 200 Hz band at T60 0.50 s and agreement
 * at 287 Hz. A Schroeder basis needs a room volume (500 Hz without one, no
 * factor); the factor multiplies within 100 Hz and 500 Hz × factor.
 */
function rcLimitNote(settings: CalibrationSettings, positions: number): Notice {
  const params: Record<string, string> = { auto: String(settings.rc_hi === null), basis: settings.rc_hi_basis, factor: String(settings.rc_hi_transition) };
  if (settings.rc_hi !== null) return { code: "rc_limit", params: { ...params, hz: String(settings.rc_hi) } };
  const volume = (settings.room_volume_m3 ?? 0) > 0;
  if (volume) params.schroeder_hz = "154";
  const factor = settings.rc_hi_transition;
  let hz = 500;
  if (settings.rc_hi_basis === "agreement") {
    hz = 287;
    params.positions = String(positions);
  } else if (volume) {
    const lenient = settings.rc_hi_basis === "lenient";
    if (lenient) Object.assign(params, { band_hz: "200", band_t60_s: "0.5" });
    hz = clamp((lenient ? 200 : 154) * factor, [100, 500 * factor]);
  }
  return { code: "rc_limit", params: { ...params, hz: String(hz) } };
}

/**
 * `result_lines().rc` after a calibration (secs-service curves.rs): its
 * rc_limit note, then how the room volume set an automatic Schroeder limit.
 * The agreement basis uses no volume.
 */
function rcLines(limit: Notice | undefined, volume: number | null): Notice[] {
  if (!limit) return [];
  if (limit.params.auto !== "true" || limit.params.basis === "agreement") return [limit];
  return [limit, volume ? { code: "rc_room_volume", params: { volume_m3: String(volume) } } : { code: "rc_no_room_volume", params: {} }];
}

/** The RC lines the mock's calibration leaves for these settings at this many measured positions (gallery views). */
export const mockRcLines = (settings: CalibrationSettings, positions: number): Notice[] => rcLines(rcLimitNote(settings, positions), settings.room_volume_m3);

/**
 * The correction band set a calibration with extra bands notes (secs-engine
 * `Note::RcBands`, ADR 0024): the primary band up to its resolved upper
 * limit and the extra bands, sorted, with overlapping or touching bands
 * merged, as `"lo-hi,lo-hi"`. Without extra bands there is no note.
 */
export function mockRcBandsNotes(settings: CalibrationSettings, positions: number): Notice[] {
  if (settings.rc_bands.length === 0) return [];
  const hi = Number(rcLimitNote(settings, positions).params.hz);
  const all = [[settings.rc_lo, hi], ...settings.rc_bands.map((band) => [band.lo_hz, band.hi_hz])].sort((a, b) => a[0]! - b[0]!);
  const merged: number[][] = [];
  for (const [lo, top] of all) {
    const last = merged.at(-1);
    if (last && lo! <= last[1]!) last[1] = Math.max(last[1]!, top!);
    else merged.push([lo!, top!]);
  }
  return [{ code: "rc_bands", params: { bands: merged.map(([lo, top]) => `${lo}-${top}`).join(",") } }];
}

/**
 * Why the service refuses an edit of the RC edges or the extra bands
 * (secs-service `calibration_settings_edit`, ADR 0024): an edge outside its
 * limits, the lower edge at or above a manual upper one (or the reverse), more
 * than four extra bands, or a band outside 20–20000 Hz or not rising (its
 * 1-based place in the list sent). Nothing is clamped.
 */
export function rcEditRefusal(settings: CalibrationSettings, edit: SettingEdit): IpcError | undefined {
  const refuse = (field: string, reason: string, params: Record<string, string>) =>
    ipcError("invalid_request", `${field}: ${reason}`, { field, reason, ...params });
  const outside = (hz: number, [lo, hi]: readonly [number, number]) => !Number.isFinite(hz) || hz < lo || hz > hi;
  switch (edit.kind) {
    case "rc_lo":
      if (outside(edit.hz, LIMITS.rc_lo)) return refuse("rc_lo", "rc_lo_range", { min: String(LIMITS.rc_lo[0]), max: String(LIMITS.rc_lo[1]) });
      if (settings.rc_hi !== null && edit.hz >= settings.rc_hi) return refuse("rc_lo", "rc_lo_above_hi", { hi_hz: String(settings.rc_hi) });
      return undefined;
    case "rc_hi":
      if (edit.hz === null) return undefined;
      if (outside(edit.hz, LIMITS.rc_hi)) return refuse("rc_hi", "rc_hi_range", { min: String(LIMITS.rc_hi[0]), max: String(LIMITS.rc_hi[1]) });
      if (edit.hz <= settings.rc_lo) return refuse("rc_hi", "rc_hi_below_lo", { lo_hz: String(settings.rc_lo) });
      return undefined;
    case "rc_bands": {
      if (edit.bands.length > LIMITS.rc_bands_max[1]) return refuse("rc_bands", "rc_band_count", { max: String(LIMITS.rc_bands_max[1]) });
      const index = edit.bands.findIndex((band) => outside(band.lo_hz, LIMITS.rc_band) || outside(band.hi_hz, LIMITS.rc_band) || band.lo_hz >= band.hi_hz);
      const band = edit.bands[index];
      return band ? refuse("rc_bands", "rc_band_range", { index: String(index + 1), lo_hz: String(band.lo_hz), hi_hz: String(band.hi_hz) }) : undefined;
    }
    default:
      return undefined;
  }
}

/**
 * Zero dB of the measurement graph (secs-service `measurement_reference`,
 * U17): the first main in configuration order, at position 1 when it was
 * measured there and otherwise at its lowest measured position, over
 * 200–2000 Hz; with no main measured, the first sub at its lowest measured
 * position over 40–120 Hz. `order` is the configuration's roles.
 */
export function measurementReference(order: readonly string[], measured: readonly { role: string; position: number }[]): CurveReference | null {
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

/**
 * The 극저역 보호 notes a calibration with `low_protect` leaves (ADR 0022,
 * secs-service `scalar_notice`), with synthetic roll-offs: about 30 Hz for
 * the mains, about 22 Hz for the subs. Each sub gets its own high-pass, or
 * keeps the one set by the user. With the RC lower limit below the 20 Hz
 * measurement band and more than 6 dB of boost allowed, the sub bus PEQ
 * cannot hold its boost below the roll-off and keeps its fit (`limited`
 * false); raising the lower limit or lowering the boost limit clears it.
 */
export function mockLowProtectNotes(settings: CalibrationSettings, roles: readonly string[]): Notice[] {
  if (!settings.low_protect) return [];
  const subs = roles.filter(isSubRole);
  const notes: Notice[] = roles.map((role, i) => {
    if (!isSubRole(role)) return { code: "low_protect", params: { role, rolloff_hz: String(28 + ((i * 3) % 7)), limited: "true" } };
    const rolloff = 22 + 2 * subs.indexOf(role);
    const user = settings.subs_manual[role]?.hp_hz ?? 0;
    return { code: "low_protect", params: { role, rolloff_hz: String(rolloff), hp_hz: String(user > 0 ? user : rolloff / Math.SQRT2), limited: "true" } };
  });
  if (subs.length > 0 && settings.rc_lo < 20 && settings.boost_max_db > 6) {
    notes.push({ code: "low_protect", params: { role: subs.join(" + "), rolloff_hz: "22", limited: "false" } });
  }
  return notes;
}

/** Settings edits that change only the MIMO result (secs-types `mimo_key`). */
const MIMO_ONLY_EDITS: ReadonlySet<SettingEdit["kind"]> = new Set(["fsiso", "mimo_low", "mimo_max", "mimo_fir"]);

/** The service fails a calibration on the agreement basis below three measured positions (secs-service `check_rc_agreement`). */
const rcAgreementError = (settings: CalibrationSettings, positions: number): IpcError | undefined =>
  settings.rc_hi === null && settings.rc_hi_basis === "agreement" && positions < 3
    ? ipcError("engine_input", "RC agreement requires at least 3 measured positions", { reason: "rc_agreement_positions", measured: String(positions), required: "3" })
    : undefined;

/** The folder stamp `20261015-094107` (clock.rs `stamp`) in local time; Date.now follows the tour's clock. */
const stamp = () => {
  const date = new Date(Date.now());
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}-${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}`;
};

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
  let calibrationPositions: number | null = results.calibration && !results.calibration.cached ? project?.positions.length ?? 0 : null;
  // ResultBook::mimo: live runs keep their measured positions; cached runs use the project.
  const mimoExport = (): ResultsSummary["mimo_export"] => {
    if (!settings.mimo_on) return "off";
    if ((calibrationPositions ?? project?.positions.length ?? 0) < MIMO_MIN_POSITIONS) return "too_few_positions";
    if (!results.mimo) return "none";
    if (!results.calibration || results.calibration.stale || results.mimo.stale) return "stale";
    return results.mimo.support_count === 0 ? "no_supports" : "included";
  };
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
  const dirty = (reason = "settings", recommendationChanged = true) => {
    inputRevision++;
    if (recommendationChanged && Object.keys(settings.xo_rec).length > 0) results = { ...results, xo_rec_stale: true };
    project = { ...needProject(), dirty: true };
    results = {
      ...results,
      calibration: results.calibration ? { ...results.calibration, stale: true, stale_reason: reason } : null,
      mimo: results.mimo ? { ...results.mimo, stale: true } : null,
    };
  };
  const syncMeasure = () => {
    measureConfig.roles = config.speakers.map((s) => s.role);
    measureConfig.output_channels = config.speakers.map((s) => s.bands[0]?.out ?? 1);
    for (const record of records) record.done = measureConfig.roles.length > 0 && measureConfig.roles.every((role) => record.roles.includes(role));
  };
  syncMeasure();
  const state = () => clone({ project: needProject(), results, config, dsp, layout, settings, target, measureConfig, levels, exportFolder, records, micCalFile, micCalAttached, subsOnly, calibrationPositions });
  type State = ReturnType<typeof state>;
  const undos = new Map<string, State>();
  const savedProjects = new Map<string, State>();
  const savedTargets = new Map<string, typeof target>();
  const restore = (s: State) => {
    ({ project, results, config, dsp, layout, settings, target, measureConfig, levels, exportFolder, records, micCalFile, micCalAttached, subsOnly, calibrationPositions } = clone(s));
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
    dirty();
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
    undos.clear(); subsOnly = false; calibrationPositions = null; syncMeasure();
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
    if (monitor(job.kind)) {
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
    const refusal = rcEditRefusal(settings, edit);
    if (refusal) throw new MockError(refusal);
    switch (edit.kind) {
      case "mode": settings.mode = edit.mode; break;
      case "sample_rate_policy": settings.sample_rate_policy = edit.policy; break;
      case "low_protect": settings.low_protect = edit.on; break;
      // Taken in every mode, Zero included: the engine leaves lf_align out there, and the screen says why (ADR 0025).
      case "lf_align": settings.lf_align = edit.on; break;
      case "mode_decay": settings.mode_decay = edit.on; break;
      case "ultra_ms": settings.ultra_ms = clamp(edit.ms, LIMITS.ultra_ms); break;
      case "rc_lo": settings.rc_lo = edit.hz; break;
      case "rc_hi": settings.rc_hi = edit.hz; break;
      case "rc_bands": settings.rc_bands = edit.bands.map((band) => ({ ...band })).sort((a, b) => a.lo_hz - b.lo_hz); break;
      case "rc_hi_basis": settings.rc_hi_basis = edit.basis; break;
      case "rc_hi_transition":
        if (edit.factor !== 1 && edit.factor !== 2 && edit.factor !== 4) throw new MockError(ipcError("invalid_request", "RC transition factor must be 1, 2 or 4", { field: "rc_hi_transition" }));
        settings.rc_hi_transition = edit.factor; break;
      case "room_volume": settings.room_volume_m3 = edit.m3 > LIMITS.volume[0] ? Math.min(edit.m3, LIMITS.volume[1]) : null; settings.room_dims_m = null; break;
      case "room_dims": {
        const dims = edit.dims.map((n) => clamp(n, LIMITS.dim)) as [number, number, number];
        settings.room_dims_m = dims.every((n) => n >= LIMITS.dim_valid[0] && n <= LIMITS.dim_valid[1]) ? dims : null;
        if (settings.room_dims_m) settings.room_volume_m3 = Math.min(LIMITS.volume[1], Math.round(dims[0] * dims[1] * dims[2] * 10) / 10);
        break;
      }
      case "boost_max": settings.boost_max_db = clamp(edit.db, LIMITS.boost); break;
      case "xo_rec_include": settings.xo_rec_include = edit.on; break;
      case "xo": settings.xo[edit.group] = Math.round(clamp(edit.hz, LIMITS.xo)); settings.xo_rec_include = false; break;
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
      case "infra_boost": settings.infra_boost_max_db = clamp(edit.db, LIMITS.infra_boost); break;
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
        else entry.rear_trim_ms = clamp(edit.ms, LIMITS.rear_trim);
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
      case "main_delay": settings.sub_main_delay_ms = clamp(edit.ms, LIMITS.main_delay); break;
      case "mimo_on": settings.mimo_on = edit.on; break;
      case "fsiso": settings.fsiso = clamp(edit.hz, LIMITS.fsiso); break;
      case "mimo_low": settings.mimo_low = edit.hz === null ? null : clamp(edit.hz, LIMITS.mimo_low); break;
      case "mimo_max": settings.mimo_max_supports = Math.round(clamp(edit.count, LIMITS.mimo_max)); break;
      case "mimo_fir": settings.mimo_fir = edit.on; break;
      case "analytic_derivatives":
        if (edit.peq !== undefined) settings.peq_analytic_derivatives = edit.peq;
        if (edit.mimo_iir !== undefined) settings.mimo_iir_analytic_derivatives = edit.mimo_iir;
        break;
    }
    // MIMO's own settings are not in the correction's key (secs-types `engine_key`, `mimo_key`): they
    // leave the correction current and only MIMO behind, which is what 'MIMO만 계산' is for. Turning
    // MIMO on or off is in neither key.
    if (edit.kind === "mimo_on" || edit.kind === "xo_rec_include") project = { ...needProject(), dirty: true };
    else if (MIMO_ONLY_EDITS.has(edit.kind)) {
      project = { ...needProject(), dirty: true };
      results = { ...results, mimo: results.mimo ? { ...results.mimo, stale: true } : null };
    } else dirty("settings", edit.kind !== "xo");
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
        after_mimo_db: supports > 0 ? fx.lateAt(role, position)[2] : after_db })) ?? [] } };
  };
  /**
   * Why `calibrate { mimo: true }` cannot start, in the service's order
   * (start_calibration: the explicit GPU, then check_mimo_preconditions).
   */
  const mimoRefusal = (s: CalibrationSettings, positions: number): IpcError | undefined => {
    const status = computeStatus();
    if (status.preference === "gpu" && !status.gpu_available) return ipcError("compute_unavailable", "no supported GPU is available", { reason: status.reason });
    if (!s.mimo_on) return ipcError("engine_input", "invalid input: MIMO is disabled");
    if (positions < MIMO_MIN_POSITIONS) return ipcError("engine_input", `invalid input: MIMO needs ${MIMO_MIN_POSITIONS} measured positions, the project has ${positions}`);
    return undefined;
  };
  // Mock-only design values: compact wire summaries carry roles, not DSP filters.
  const resultSubs = () => results.calibration?.sub_roles.map((role, i) => ({
    ...clone(fx.signalChain.subs[i % fx.signalChain.subs.length]!), role,
  })) ?? [];
  const resultLines = (): ResultLinesResult => ({
    stale: results.calibration?.stale ?? false,
    channels: results.calibration?.scorecard.map((s) => ({ role: s.role, within_1db_pct: [s.before.in1_20, s.after.in1_20], gd_spread_ms: [s.before.gd_20, s.after.gd_20], peq_count: 2,
      fir_taps: fx.modeSpec(results.calibration!.mode).fir_taps, xo_hz: settings.xo[fx.GROUP_KEYS[fx.groupOf(s.role)]!] ?? null, mimo_support_count: results.mimo ? 2 : null })) ?? [],
    total_delay_ms: results.calibration?.latency_ms ?? null, notes: results.calibration?.notes ?? [],
    rc: rcLines(results.calibration?.notes.find((note) => note.code === "rc_limit"), settings.room_volume_m3),
    ba: results.calibration && results.calibration.sub_roles.length >= 2 ? {
      stale: results.calibration.stale, subs_only: subsOnly, band_hz: [20, 200], flatness_db: [6.2, 1.8], seat_spread_db: [4.5, 2.1], positions: records.length,
      rows: resultSubs().map((s) => ({ role: s.role, mode: s.mode, gain_db: s.gain_db, delay_ms: s.delay_ms, polarity: s.polarity,
        allpass: s.allpass, high_pass: [s.hp_hz, s.hp_type], low_pass: [s.lp_hz, s.lp_type], peq: s.peq })),
      layouts: Object.entries(settings.bundle_layouts).map(([bundle, entry]) => ({ bundle, kind: entry.kind,
        members: Object.entries(settings.subs_manual).filter(([, s]) => s.mode === "bundle" && s.bundle === bundle).map(([role]) => role),
        front: [], rear: [], rear_ms: entry.rear_trim_ms, auto_ms: null, trim_ms: entry.rear_trim_ms, peq: [],
      })), warnings: warnings(),
    } : null,
  });
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
          calibrationPositions = null;
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
      case "results": return { method: req.method, data: { ...results, mimo_export: mimoExport() } };
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
        const p = req.params;
        if (!["none", "mdat", ...fx.measureMethods.methods].includes(p.method)) invalid("unknown measurement method");
        if (p.repetitions !== null && ![1, 2, 4, 8].includes(p.repetitions)) invalid("invalid repetitions");
        measureConfig = { ...measureConfig, method: p.method, input_device: p.input_device ?? measureConfig.input_device,
          output_device: p.output_device ?? measureConfig.output_device, input_channel: p.input_channel ?? measureConfig.input_channel,
          sweep_length: p.sweep_length ?? measureConfig.sweep_length, repetitions: p.repetitions ?? measureConfig.repetitions,
          sweep_level_dbfs: p.sweep_level_dbfs ?? measureConfig.sweep_level_dbfs, timing: p.timing ?? measureConfig.timing,
          loopback_input_channel: p.loopback_input_channel ?? measureConfig.loopback_input_channel,
          timing_ref_output: p.timing_ref_output ?? measureConfig.timing_ref_output, share_mode: p.share_mode ?? measureConfig.share_mode };
        if (p.output_device !== null) config.device = p.output_device;
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
        project = { ...needProject(), dirty: true };
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
        const revision = inputRevision;
        const positions = needProject().positions.length;
        const withMimo = !!req.params.mimo && !req.params.subs_only;
        // start_calibration refuses a MIMO it cannot run before the job starts.
        const refusal = withMimo ? mimoRefusal(captured, positions) : undefined;
        if (refusal) throw new MockError(refusal);
        const commit = () => {
          const roles = config.speakers.filter((s) => !s.role.startsWith("SW")).map((s) => s.role);
          const summary = clone(fx.calibrationSummary);
          if (revision !== inputRevision) { summary.stale = true; summary.stale_reason = results.calibration?.stale_reason ?? "settings"; }
          summary.mode = captured.mode; summary.ultra_ms = captured.ultra_ms;
          const spec = fx.modeSpec(captured.mode);
          summary.latency_ms = spec.max_latency_ms;
          summary.verdict.added_delay_ms = spec.max_latency_ms;
          summary.notes = [rcLimitNote(captured, positions), ...mockRcBandsNotes(captured, positions), ...mockLowProtectNotes(captured, config.speakers.map((s) => s.role))];
          summary.scorecard = roles.map((role) => ({ ...clone(fx.scorecard.find((score) => score.role === role) ?? fx.scorecard[0]!), role }));
          summary.late_energy = fx.lateEnergy(roles, needProject().positions);
          summary.main_roles = roles;
          summary.sub_roles = config.speakers.filter((s) => s.role.startsWith("SW")).map((s) => s.role);
          if (!req.params.subs_only) {
            results = { ...results, calibration: summary, mimo: results.mimo ? { ...results.mimo, stale: true } : null };
            calibrationPositions = positions;
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
      case "mimo":
        needProject();
        if (req.params.settings) settings = clone(req.params.settings);
        return { method: req.method, data: start("mimo", () => {
          commitMimo(settings);
          return { kind: "mimo" };
        }) };
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
        // export.rs resolve_output_rate: the REW mdat stays at 48 kHz; 자동 reads the default playback device
        // (`playbackRate`) and writes 48 kHz with an `output_rate_fallback` note when it cannot use that rate.
        const playback = options.playbackRate === undefined ? DESIGN_RATE : options.playbackRate;
        const rated = kind === "before_after_mdat" || kind === "staged_mdat" ? { output_rate: DESIGN_RATE, notes: [] as Notice[] }
          : dsp.fields.output_rate !== "auto" ? { output_rate: dsp.fields.output_rate.fixed, notes: [] as Notice[] }
          : playback !== null && (OUTPUT_RATES as readonly number[]).includes(playback) ? { output_rate: playback, notes: [] as Notice[] }
          : { output_rate: DESIGN_RATE, notes: [{ code: "output_rate_fallback", params: playback === null ? {} : { rate: String(playback) } }] as Notice[] };
        const outputRate = rated.output_rate;
        for (const [index, rate] of extraRates.entries()) {
          if (!(OUTPUT_RATES as readonly number[]).includes(rate)) throw new MockError(ipcError("invalid_request", "unsupported extra output sample rate", { rate: String(rate) }));
          if ((rate === outputRate && dsp.fields.output_rate !== "auto") || extraRates.indexOf(rate) !== index) {
            throw new MockError(ipcError("invalid_request", "extra output sample rates must be distinct from the main rate and each other", { rate: String(rate) }));
          }
        }
        const scope = req.params.mdat_scope ?? "all";
        if (kind === "before_after_mdat" && scope === "measurements") {
          if (project.positions.length === 0) throw new MockError(ipcError("engine_input", "no measurements to export", { reason: "no_measurements" }));
          const folder = req.params.folder || `${exportFolder.folder.replace(/[\\/]+$/, "")}/SECS_mdat_measurements_${stamp()}`;
          const files = [`${folder}/${mdatFileName(scope, settings)}`];
          return { method: req.method, data: start("export", () => ({ kind: "export", data: { folder, files, stale: false, ...rated } })) };
        }
        if (!results.calibration) throw new MockError(ipcError("engine_input", "no calibration result to export"));
        if ((kind === "staged_mdat" || kind === "before_after_mdat") && results.calibration.cached && results.calibration.stale) {
          throw new MockError(ipcError("export_scope", `${kind} requires a current calibration result`, { reason: "stale_cache" }));
        }
        const calibration = results.calibration;
        const hasMimo = mimoExport() === "included";
        if (kind === "filter_text" || kind === "equalizer_apo") {
          const reason = kind === "filter_text" ? calibration.sub_roles.length > 0 ? "subs" : hasMimo ? "mimo" : null : null;
          if (reason) throw new MockError(ipcError("export_scope", `${kind} cannot export a result with ${reason}`, { reason }));
        }
        // export_folder.rs: an explicit folder as is, else SECS_<kind>_<RC_BA[_MIMO]>_<the result's mode>_<stamp>
        // below the export base, with CamillaDSP in its own `camilladsp`.
        const timestamped = `${exportFolder.folder.replace(/[\\/]+$/, "")}/SECS_${EXPORT_FOLDER_TAG[kind]}_${hasMimo ? "RC_BA_MIMO" : "RC_BA"}_${calibration.mode}_${stamp()}`;
        const base = (req.params.folder || exportFolder.folder).replace(/[\\/]+$/, "");
        const folder = designName !== null ? `${base}/SECS_${kind}_${designName}` : req.params.folder || (kind === "camilla_dsp" ? `${timestamped}/camilladsp` : timestamped);
        if (kind === "convolution") {
          // The synthetic chain feeds each main to itself and every sub;
          // mock MIMO adds support to those same pairs. LFE feeds the subs.
          const pairs = calibration.main_roles.flatMap((input) => [input, ...calibration.sub_roles].map((output) => `${input}_to_${output}`));
          pairs.push(...calibration.sub_roles.map((output) => `LFE_to_${output}`));
          // secs-filters: the extra rates go into `<rate>Hz` folders beside the output rate's files.
          const files = ["", ...extraRates.filter((rate) => rate !== outputRate).map((rate) => `/${rate}Hz`)].flatMap((sub) => pairs.flatMap((pair) => [`${folder}${sub}/${pair}.wav`, `${folder}${sub}/${pair}.csv`]));
          files.push(`${folder}/컨볼루션 적용 방법.txt`, `${folder}/routing.json`, `${folder}/secs-design.json`);
          return { method: req.method, data: start("export", () => ({ kind: "export", data: { folder, files, stale: calibration.stale, ...rated } })) };
        }
        if (kind === "filter_text" || kind === "equalizer_apo") {
          const mode = filterModeName(settings);
          const prefix = `secs_${mode}`;
          const firFolder = kind === "filter_text" ? folder : `${folder}/${prefix}`;
          const files = kind === "filter_text"
            ? [`${folder}/SECS 필터 ${fx.modeLabel(settings.mode, settings.ultra_ms)}.txt`]
            : [`${folder}/config.txt`, `${folder}/${prefix}/${prefix}.txt`, `${folder}/EqAPO 적용 방법.txt`];
          if (settings.mode !== "zero") files.push(...calibration.main_roles.map((role) => `${firFolder}/${prefix}_${role}_fir.wav`));
          files.push(`${folder}/secs-design.json`);
          return { method: req.method, data: start("export", () => ({ kind: "export", data: { folder, files, stale: calibration.stale, ...rated } })) };
        }
        const prefix = `secs_${filterModeName(calibration)}`;
        const files = kind === "camilla_dsp" ? [
          `${folder}/configs/${prefix}.yml`, `${folder}/configs/${prefix}_bypass.yml`,
          ...(calibration.mode === "zero" ? [] : calibration.main_roles.map((role) => `${folder}/coeffs/${prefix}_${role}_fir.wav`).sort()),
          `${folder}/secs-design.json`,
        ] : [kind === "before_after_mdat" ? `${folder}/${mdatFileName(scope, calibration)}` : `${folder}/${kind}.mdat`];
        return { method: req.method, data: start("export", () => ({ kind: "export", data: { folder, files, stale: calibration.stale, ...rated } })) };
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
        // project_edit.rs dsp_set: only editable profiles accept a rate change; changing other fields makes the profile custom.
        if (fields) {
          if (fields.output_rate !== "auto" && !(OUTPUT_RATES as readonly number[]).includes(fields.output_rate.fixed)) {
            throw new MockError(ipcError("invalid_request", "unsupported output sample rate", { reason: "output_rate" }));
          }
          if (!sameRate(fields.output_rate, dsp.fields.output_rate) && !dsp.fields.output_rate_editable) {
            throw new MockError(ipcError("invalid_request", "this DSP preset has a fixed output sample rate", { reason: "output_rate" }));
          }
        }
        // The output rate stays out of the design (ADR 0017): changing it alone leaves the result current.
        const rateOnly = !!fields && (Object.keys(fields) as (keyof typeof fields)[]).every((key) => key === "output_rate" || key === "output_rate_editable" || fields[key] === dsp.fields[key]);
        if (fields) {
          const profile = rateOnly ? dsp.profile : "custom";
          dsp = { ...dsp, profile, fields: { ...clone(fields), output_rate_editable: profile === "camilla" || profile === "custom" } };
        }
        dsp.window_ms = (dsp.fields.fir_taps / countedRate(dsp.fields.output_rate)) * 1000; dsp.resolution_hz = 48000 / Math.max(1, dsp.fields.fir_taps);
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
      case "update_check":
        updateStatus = { ...updateStatus, phase: { kind: "checking" } };
        return { method: req.method, data: updateStatus };
      case "update_download": {
        const phase = updateStatus.phase;
        if (phase.kind !== "available") return invalid("No update is available to download");
        updateStatus = { ...updateStatus, phase: { kind: "downloading", version: phase.version, received: 0, total: null } };
        return { method: req.method, data: updateStatus };
      }
      case "update_restart": {
        const phase = updateStatus.phase;
        if (phase.kind !== "ready") return invalid("No update is ready to install");
        updateStatus = { ...updateStatus, phase: { kind: "installing", version: phase.version } };
        return { method: req.method, data: null };
      }
      case "update_notice_dismiss":
        updateStatus = { ...updateStatus, notice: null };
        return { method: req.method, data: updateStatus };
      case "update_open_release":
        if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z]+(\.[0-9A-Za-z]+)*)?$/.test(req.params.version)) return invalid("Not a release version");
        return { method: req.method, data: null };
      case "app_settings_get": return { method: req.method, data: appSettings };
      case "app_settings_set": {
        if (req.params.settings.compute === "gpu" && !availableCompute.gpu_available) throw new MockError(ipcError("compute_unavailable"));
        if (req.params.settings.compute !== appSettings.compute && results.mimo) results = { ...results, mimo: { ...results.mimo, stale: true } };
        if (req.params.settings.updates !== appSettings.updates) {
          const policy = req.params.settings.updates;
          const phase = updateStatus.phase;
          updateStatus = { ...updateStatus, policy, phase: phase.kind === "installing" ? phase
            : policy === "off" ? { kind: "off" }
            : policy === "auto" && phase.kind === "available" ? { kind: "downloading", version: phase.version, received: 0, total: null }
            : policy === "notify" && phase.kind === "downloading" ? { kind: "available", version: phase.version }
            : phase.kind === "off" ? { kind: "checking" } : phase };
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
    tick,
    drain: () => { for (let guard = 0; [...jobs.values()].some((j) => !monitor(j.kind) || j.cancelRequested) && guard < 1000; guard++) tick(); },
    setApplyStatus: (next) => { applyStatus = clone(next); },
    setUpdateStatus: (next) => { updateStatus = clone(next); appSettings.updates = next.policy; },
    setResults: (next) => {
      results = clone(next);
      calibrationPositions = results.calibration && !results.calibration.cached ? project?.positions.length ?? 0 : null;
    },
    dispose: () => { if (timer) clearInterval(timer); timer = null; jobs.clear(); playing = null; },
  };
}
