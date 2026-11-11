// Codes from the service turned into catalog keys (ADR 0003: codes, not
// sentences). Unknown codes get a generic line and keep the code for 자세히.
import { isMessageKey, type MessageKey, type MessageParams } from "../../i18n";
import type { Translate } from "../../i18n/react";
import type { ErrorCode } from "../../ipc/bindings/ErrorCode";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { JobKind } from "../../ipc/bindings/JobKind";
import { roleCode } from "../../ui/maps/geometry";

export const JOB_NAME: Record<JobKind, MessageKey> = {
  measure: "jobs.kind.measure",
  sample_load: "jobs.kind.sample_load",
  mdat_import: "jobs.kind.mdat_import",
  calibrate: "jobs.kind.calibrate",
  recommend_xo: "jobs.kind.recommend_xo",
  mimo: "jobs.kind.mimo",
  export: "jobs.kind.export",
  apply: "jobs.kind.apply",
  level_monitor: "jobs.kind.level_monitor",
  status_watch: "jobs.kind.status_watch",
};

const ERROR_TEXT: Record<ErrorCode, { cause: MessageKey; next: MessageKey }> = {
  invalid_request: { cause: "error.invalid_request", next: "error.invalid_request.next" },
  internal: { cause: "error.internal", next: "error.internal.next" },
  not_implemented: { cause: "error.not_implemented", next: "error.not_implemented.next" },
  ui_only: { cause: "error.ui_only", next: "error.ui_only.next" },
  unsupported: { cause: "error.unsupported", next: "error.unsupported.next" },
  permission_denied: { cause: "error.permission_denied", next: "error.permission_denied.next" },
  apply_failed: { cause: "error.apply_failed", next: "error.apply_failed.next" },
  project_not_open: { cause: "error.project_not_open", next: "error.project_not_open.next" },
  project_io: { cause: "error.project_io", next: "error.project_io.next" },
  job_busy: { cause: "error.job_busy", next: "error.job_busy.next" },
  job_not_found: { cause: "error.job_not_found", next: "error.job_not_found.next" },
  cancelled: { cause: "error.cancelled", next: "error.cancelled.next" },
  engine_input: { cause: "error.engine_input", next: "error.engine_input.next" },
  export_failed: { cause: "error.export_failed", next: "error.export_failed.next" },
  export_scope: { cause: "error.export_scope", next: "error.export_scope.next" },
  stale_inputs: { cause: "error.stale_inputs", next: "error.stale_inputs.next" },
  compute_unavailable: { cause: "error.compute_unavailable", next: "error.compute_unavailable.next" },
  device_unavailable: { cause: "error.device_unavailable", next: "error.device_unavailable.next" },
  recording_failed: { cause: "error.recording_failed", next: "error.recording_failed.next" },
  share_mode_refused: { cause: "error.share_mode_refused", next: "error.share_mode_refused.next" },
  rew_unreachable: { cause: "error.rew_unreachable", next: "error.rew_unreachable.next" },
  rew_pro_required: { cause: "error.rew_pro_required", next: "error.rew_pro_required.next" },
  rew_failed: { cause: "error.rew_failed", next: "error.rew_failed.next" },
  measurement_input: { cause: "error.measurement_input", next: "error.measurement_input.next" },
  confirm_required: { cause: "error.confirm_required", next: "error.confirm_required.next" },
  tutorial_blocked: { cause: "error.tutorial_blocked", next: "error.tutorial_blocked.next" },
};

const ENGINE_INPUT_TEXT = {
  measurement_sample_rate: { cause: "error.engine_input.measurement_sample_rate", next: "error.engine_input.measurement_sample_rate.next", required: ["position", "role", "actual_hz", "expected_hz"] },
  unsupported_resampling_rate: { cause: "error.engine_input.unsupported_resampling_rate", next: "error.engine_input.unsupported_resampling_rate.next", required: ["position", "role", "actual_hz", "min_hz", "max_hz"] },
  invalid_measurement: { cause: "error.engine_input.invalid_measurement", next: "error.engine_input.invalid_measurement.next", required: ["position", "role"] },
  duplicate_measurement: { cause: "error.engine_input.duplicate_measurement", next: "error.engine_input.duplicate_measurement.next", required: ["position", "role"] },
  missing_reference_position: { cause: "error.engine_input.missing_reference_position", next: "error.engine_input.missing_reference_position.next", required: ["position"] },
  missing_main_measurement: { cause: "error.engine_input.missing_main_measurement", next: "error.engine_input.missing_main_measurement.next", required: ["position"] },
  // The RC upper limit's agreement basis below its positions (secs-service `check_rc_agreement`, ADR 0021).
  rc_agreement_positions: { cause: "error.engine_input.rc_agreement_positions", next: "error.engine_input.rc_agreement_positions.next", required: ["measured", "required"] },
} as const satisfies Record<string, { cause: MessageKey; next: MessageKey; required: readonly string[] }>;

/**
 * Error reasons with their own sentence (`params.reason`), keyed by code and
 * reason, and the params each sentence needs. A reason whose params did not
 * arrive reads as its code's sentence.
 */
const REASON_TEXT: Readonly<Record<string, { cause: MessageKey; next: MessageKey; required: readonly string[] }>> = {
  // Opening a project (secs-service `project_open_ipc`).
  "project_io.not_found": { cause: "error.project_io.not_found", next: "error.project_io.not_found.next", required: ["path"] },
  "project_io.io": { cause: "error.project_io.io", next: "error.project_io.io.next", required: ["path"] },
  "project_io.json": { cause: "error.project_io.json", next: "error.project_io.json.next", required: ["path"] },
  "project_io.invalid": { cause: "error.project_io.invalid", next: "error.project_io.invalid.next", required: ["path"] },
  "project_io.wav": { cause: "error.project_io.wav", next: "error.project_io.wav.next", required: ["path"] },
  "project_io.ir_path": { cause: "error.project_io.ir_path", next: "error.project_io.ir_path.next", required: ["path"] },
  "permission_denied.permission_denied": { cause: "error.permission_denied.permission_denied", next: "error.permission_denied.permission_denied.next", required: ["path"] },
  // The app settings file (secs-service project_edit.rs).
  "unsupported.newer_version": { cause: "error.unsupported.newer_version", next: "error.unsupported.newer_version.next", required: ["path"] },
  "unsupported.settings_unreadable": { cause: "error.unsupported.settings_unreadable", next: "error.unsupported.settings_unreadable.next", required: ["path"] },
  // Equalizer APO's configuration (secs-service apply.rs).
  "unsupported.eqapo_config_path": { cause: "error.unsupported.eqapo_config_path", next: "error.unsupported.eqapo_config_path.next", required: ["path"] },
  "apply_failed.config_encoding": { cause: "error.apply_failed.config_encoding", next: "error.apply_failed.config_encoding.next", required: ["path"] },
  // The export folder (`field: folder`).
  "invalid_request.relative_folder": { cause: "error.invalid_request.relative_folder", next: "error.invalid_request.relative_folder.next", required: [] },
  // A measurement's samples (secs-service `validate_samples`).
  "measurement_input.empty": { cause: "error.measurement_input.empty", next: "error.measurement_input.empty.next", required: ["row"] },
  "measurement_input.non_finite": { cause: "error.measurement_input.non_finite", next: "error.measurement_input.non_finite.next", required: ["row"] },
};

const isU32 = (value: string | undefined): boolean => typeof value === "string" && /^(0|[1-9]\d*)$/.test(value) && Number(value) <= 4_294_967_295;
// Both supported locales (ko-KR/en-US) group these unsigned integers with commas.
const integer = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });

/**
 * A generic cause can be replaced with the caller's named task failure. A
 * `calibrate` that failed in its MIMO part (`stage: "mimo"`, P141) kept the
 * correction: the cause says so first, and the next step after a cause that
 * names nothing is MIMO alone.
 */
export function errorText(t: Translate, error: IpcError): { cause: string; next: string; generic: boolean } {
  const text = codeText(t, error);
  if (error.params.stage !== "mimo") return text;
  return {
    cause: text.generic ? t("error.stage.mimo") : `${t("error.stage.mimo")} ${text.cause}`,
    next: text.generic ? t("error.stage.mimo.next") : text.next,
    generic: false,
  };
}

function codeText(t: Translate, error: IpcError): { cause: string; next: string; generic: boolean } {
  const reason = error.params.reason;
  if (error.code === "engine_input" && reason && Object.hasOwn(ENGINE_INPUT_TEXT, reason)) {
    const keys = ENGINE_INPUT_TEXT[reason as keyof typeof ENGINE_INPUT_TEXT];
    if (keys.required.every((key) => key === "role" ? typeof error.params.role === "string" && error.params.role.trim().length > 0 : isU32(error.params[key]))) {
      const params: Record<string, string> = { ...error.params, ...(error.params.role ? { role: roleCode(error.params.role.trim()) } : {}) };
      for (const key of keys.required) if (key !== "role") params[key] = integer.format(Number(error.params[key]));
      return { cause: t(keys.cause, params), next: t(keys.next, params), generic: false };
    }
  }
  const reasoned = reason ? REASON_TEXT[`${error.code}.${reason}`] : undefined;
  if (reasoned && reasoned.required.every((key) => typeof error.params[key] === "string" && error.params[key] !== "")) {
    return { cause: withSubscribe(t, error, t(reasoned.cause, error.params)), next: t(reasoned.next, error.params), generic: false };
  }
  const keys = ERROR_TEXT[error.code] ?? ERROR_TEXT.internal;
  return { cause: withSubscribe(t, error, t(keys.cause, error.params)), next: t(keys.next, error.params), generic: error.code === "engine_input" || !ERROR_TEXT[error.code] };
}

/** A REW failure that followed a lost progress subscription says so after its cause (`subscribe_error`). */
function withSubscribe(t: Translate, error: IpcError, cause: string): string {
  return error.params.subscribe_error ? `${cause} ${t("error.rew.subscribe_error")}` : cause;
}

/** The phases a measurement's progress names (secs-service measure/native.rs, rew.rs); any other measure stage is a role. */
export const MEASURE_PHASES: ReadonlySet<string> = new Set(["sweep", "running", "process", "done", "cancelled"]);

/** What a progress stage is doing. Measurement stages are role ids. */
export function stageText(t: Translate, kind: JobKind, stage: string | null): string | null {
  if (!stage) return null;
  const key = `jobs.stage.${stage}`;
  if (isMessageKey(key)) return t(key);
  if (kind === "measure") return t("jobs.stage.measure_role", { code: roleCode(stage) });
  return null;
}

/** The stages the service counts in bytes (secs-service `export.rs`: the files an export writes). */
const BYTE_STAGES: ReadonlySet<string> = new Set(["export.write"]);

const KIB = 1024;
const SIZE_UNITS = [
  { unit: "GB", bytes: KIB ** 3, digits: 2 },
  { unit: "MB", bytes: KIB ** 2, digits: 1 },
  { unit: "KB", bytes: KIB, digits: 0 },
] as const;

/** Bytes written out of the bytes to write, both in the unit the total calls for ('12.4 / 40.1 MB'), as a file manager sizes files. */
export function sizeProgressText(done: number, total: number): string {
  const scale = SIZE_UNITS.find(({ bytes }) => total >= bytes);
  if (!scale) return `${done} / ${total} B`;
  const size = (value: number) => (value / scale.bytes).toFixed(scale.digits);
  return `${size(done)} / ${size(total)} ${scale.unit}`;
}

/** How much of a stage is done, in the stage's own units: a count of items, or the bytes written as sizes. */
export function progressText(t: Translate, stage: string | null, done: number, total: number): string {
  return stage !== null && BYTE_STAGES.has(stage) ? sizeProgressText(done, total) : t("jobs.progress", { done, total });
}

/** Diagnostic settings belong in the app log, not the user-facing job history. */
export const isVisibleLog = (code: string): boolean => code !== "calibration_settings";

/** A pushed Log notice as a sentence. */
export function logText(t: Translate, code: string, params: Record<string, string>): { text: string; known: boolean } {
  const key = `jobs.log.${code}`;
  const values: MessageParams = { ...params };
  if (typeof params.role === "string") values.code = roleCode(params.role);
  // A level check burst that failed names its error code; the sentence is the code's, not the raw detail.
  if (code === "level_play_end" && params.outcome === "failed") {
    const failed = ERROR_TEXT[params.error_code as ErrorCode] ?? ERROR_TEXT.internal;
    return { text: t("jobs.log.level_play_end.failed", { code: values.code ?? "", cause: t(failed.cause) }), known: true };
  }
  if (isMessageKey(key)) return { text: t(key, values), known: true };
  return { text: t("jobs.log.unknown"), known: false };
}

/** Log codes that are warnings (the rest are notes). */
export const WARNING_LOGS = new Set([
  "measurement_low_snr", "mdat_duplicate_roles", "mdat_nothing_selected",
  "measurement_calibration_too_large", "measurement_calibration_unreadable", "measurement_subscribe_failed", "measurement_source_missing",
  "measurement.source.skipped", "measurement.source.budget",
]);
