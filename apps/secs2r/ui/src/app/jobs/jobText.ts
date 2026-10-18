// Codes from the service turned into catalog keys (ADR 0003: codes, not
// sentences). Unknown codes get a generic line and keep the code for 자세히.
import { isMessageKey, type MessageKey, type MessageParams } from "../../i18n";
import type { Translate } from "../../i18n/react";
import type { ErrorCode } from "../../ipc/bindings/ErrorCode";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { JobKind } from "../../ipc/bindings/JobKind";
import type { JobStatus } from "../../ipc/bindings/JobStatus";
import { roleCode } from "../../ui/maps/geometry";

export const JOB_NAME: Record<JobKind, MessageKey> = {
  level_check: "jobs.kind.level_check",
  measure: "jobs.kind.measure",
  sample_load: "jobs.kind.sample_load",
  mdat_import: "jobs.kind.mdat_import",
  calibrate: "jobs.kind.calibrate",
  recommend_xo: "jobs.kind.recommend_xo",
  mimo: "jobs.kind.mimo",
  export: "jobs.kind.export",
  level_monitor: "jobs.kind.level_monitor",
  status_watch: "jobs.kind.status_watch",
};

export const JOB_STATUS: Record<JobStatus, MessageKey> = {
  running: "jobs.status.running",
  cancel_requested: "jobs.status.cancel_requested",
  succeeded: "jobs.status.succeeded",
  failed: "jobs.status.failed",
  cancelled: "jobs.status.cancelled",
};

export const ERROR_TEXT: Record<ErrorCode, { cause: MessageKey; next: MessageKey }> = {
  invalid_request: { cause: "error.invalid_request", next: "error.invalid_request.next" },
  internal: { cause: "error.internal", next: "error.internal.next" },
  not_implemented: { cause: "error.not_implemented", next: "error.not_implemented.next" },
  project_not_open: { cause: "error.project_not_open", next: "error.project_not_open.next" },
  project_io: { cause: "error.project_io", next: "error.project_io.next" },
  job_busy: { cause: "error.job_busy", next: "error.job_busy.next" },
  job_not_found: { cause: "error.job_not_found", next: "error.job_not_found.next" },
  cancelled: { cause: "error.cancelled", next: "error.cancelled.next" },
  engine_input: { cause: "error.engine_input", next: "error.engine_input.next" },
  export_failed: { cause: "error.export_failed", next: "error.export_failed.next" },
  export_scope: { cause: "error.export_scope", next: "error.export_scope.next" },
  stale_inputs: { cause: "error.stale_inputs", next: "error.stale_inputs.next" },
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

/** Cause and next step for an IPC error, in the current language. */
export function errorText(t: Translate, error: IpcError): { cause: string; next: string } {
  const keys = ERROR_TEXT[error.code] ?? ERROR_TEXT.internal;
  return { cause: t(keys.cause, error.params), next: t(keys.next, error.params) };
}

/** What a progress stage is doing. Measurement stages are role ids. */
export function stageText(t: Translate, kind: JobKind, stage: string | null): string | null {
  if (!stage) return null;
  const key = `jobs.stage.${stage}`;
  if (isMessageKey(key)) return t(key);
  if (kind === "measure") return t("jobs.stage.measure_role", { code: roleCode(stage) });
  if (kind === "level_check") return t("jobs.stage.level_role", { code: roleCode(stage) });
  return null;
}

/** A pushed Log notice as a sentence. */
export function logText(t: Translate, code: string, params: Record<string, string>): { text: string; known: boolean } {
  const key = `jobs.log.${code}`;
  const values: MessageParams = { ...params };
  if (typeof params.role === "string") values.code = roleCode(params.role);
  if (isMessageKey(key)) return { text: t(key, values), known: true };
  return { text: t("jobs.log.unknown"), known: false };
}

/** Log codes that are warnings (the rest are notes). */
export const WARNING_LOGS = new Set(["measurement_low_snr", "mdat_duplicate_roles", "mdat_nothing_selected"]);
