// The service's recorded answers (ADR 0014, ADR 0052), written by
// crates/secs-service/tests/recorded.rs (and the app crate's updater test
// for `update_transitions.json`). Each file lists `{ params, data }` per
// request. `recorded()` returns the no-input answers (`RECORDED`) as they
// are; the keyed ones are read through `recordedCalibration` (by the
// settings' `recordKey`), `recordedRefusal` (by the edit) and the export
// and updater tables, and the mock adds only its own state to them (whether
// a result is still current, the folder written to). recorded.test.ts
// replays every file against the mock. Service output: never edit these by
// hand, rerun `SECS_RECORD=1 cargo test -p secs-service --test recorded`.
import type { CalibrationSettings } from "./bindings/CalibrationSettings";
import type { ErrorCode } from "./bindings/ErrorCode";
import type { ExportKind } from "./bindings/ExportKind";
import type { ExportResult } from "./bindings/ExportResult";
import type { IpcError } from "./bindings/IpcError";
import type { Request } from "./bindings/Request";
import type { Response } from "./bindings/Response";
import type { ResultLinesResult } from "./bindings/ResultLinesResult";
import type { ResultsSummary } from "./bindings/ResultsSummary";
import type { SettingEdit } from "./bindings/SettingEdit";
import type { UpdatePhase } from "./bindings/UpdatePhase";
import type { UpdatePolicy } from "./bindings/UpdatePolicy";
import calibrationSettingsEdit from "./recorded/calibration_settings_edit.json";
import calibrationSettingsGet from "./recorded/calibration_settings_get.json";
import configApplyPreset from "./recorded/config_apply_preset.json";
import exportRecords from "./recorded/export.json";
import modeSpec from "./recorded/mode_spec.json";
import resultLines from "./recorded/result_lines.json";
import results from "./recorded/results.json";
import targetApplyPreset from "./recorded/target_apply_preset.json";
import updateTransitions from "./recorded/update_transitions.json";

type Method = Request["method"];
type ParamsOf<M extends Method> = Extract<Request, { method: M }> extends { params: infer P } ? P : null;
type RecordedEntry<M extends Method> = { params: ParamsOf<M>; data: Extract<Response, { method: M }>["data"] };
type RecordedTable = { [M in Method]?: readonly RecordedEntry<M>[] };

/** The no-input answers, recorded on a freshly created 2.0 project and keyed by the request's own params. */
export const RECORDED = {
  mode_spec: modeSpec as RecordedEntry<"mode_spec">[],
  config_apply_preset: configApplyPreset as RecordedEntry<"config_apply_preset">[],
  target_apply_preset: targetApplyPreset as RecordedEntry<"target_apply_preset">[],
  calibration_settings_get: calibrationSettingsGet as RecordedEntry<"calibration_settings_get">[],
} satisfies RecordedTable;

/** A copy of the recorded data of the first entry whose params match, if any. */
export function recorded<M extends keyof typeof RECORDED>(
  method: M,
  match: (params: (typeof RECORDED)[M][number]["params"]) => boolean = () => true,
): (typeof RECORDED)[M][number]["data"] | undefined {
  const entries: readonly { params: (typeof RECORDED)[M][number]["params"]; data: (typeof RECORDED)[M][number]["data"] }[] = RECORDED[method];
  const entry = entries.find((e) => match(e.params));
  return entry ? structuredClone(entry.data) : undefined;
}

type Recorded<P, D> = readonly { params: P; data: D }[];

/**
 * `calibration_settings_edit` on the sample room (7.1.4, three subs): one
 * accepted edit of every kind and one outside its limits, keyed by the edit.
 * The data is the whole envelope, a refusal included.
 */
export const RECORDED_EDITS = calibrationSettingsEdit as unknown as Recorded<
  { edit: SettingEdit },
  { ok: true; data: Extract<Response, { method: "calibration_settings_edit" }> } | { ok: false; error: IpcError }
>;

/**
 * `results` and `result_lines` after a calibration of the sample room at
 * nine positions, keyed by the `engine_key` of the settings it ran on: the
 * defaults, then one edit each (ADR 0052).
 */
export const RECORDED_RESULTS = results as unknown as Recorded<{ engine_key: string }, ResultsSummary>;
export const RECORDED_RESULT_LINES = resultLines as unknown as Recorded<{ engine_key: string }, ResultLinesResult>;

/**
 * One `export` of every kind from that calibration into an explicit folder:
 * `folder` is that folder's name and `files` are relative to it.
 */
export const RECORDED_EXPORTS = exportRecords as unknown as Recorded<
  { kind: ExportKind; design_name: string | null; extra_rates: number[] },
  ExportResult
>;

/** The updater's next phase (by kind) for each request in each policy and phase, or the refusal. */
export const RECORDED_UPDATE_TRANSITIONS = updateTransitions as unknown as Recorded<
  { method: "app_settings_set" | "update_check" | "update_download" | "update_restart" | "update_notice_dismiss"; policy: UpdatePolicy; phase: UpdatePhase["kind"] },
  { policy: UpdatePolicy; phase: UpdatePhase["kind"] } | { error: { code: ErrorCode; detail: string } }
>;

const byKey = ([a]: [string, unknown], [b]: [string, unknown]) => (a < b ? -1 : a > b ? 1 : 0);
/** A value with its object keys sorted, so that two equal values print the same. */
export const ordered = (value: unknown): unknown => Array.isArray(value) ? value.map(ordered)
  : value !== null && typeof value === "object" ? Object.fromEntries(Object.entries(value).sort(byKey).map(([key, item]) => [key, ordered(item)]))
  : value;
export const same = (a: unknown, b: unknown) => JSON.stringify(ordered(a)) === JSON.stringify(ordered(b));

/**
 * The text of `CalibrationSettings::engine_key` (secs-types), the key the
 * calibration records are filed under. No record has manual sub settings,
 * so any manual entry gives a key no record has.
 */
export function recordKey(s: CalibrationSettings): string {
  const bits = new DataView(new ArrayBuffer(8));
  const number = (value: number) => {
    if (value === 0) return "0";
    bits.setFloat64(0, value);
    return bits.getBigUint64(0).toString(16).padStart(16, "0");
  };
  const optional = (value: number | null) => (value === null ? "-" : number(value));
  const xo = Object.entries(s.xo).sort(byKey).map(([group, hz]) => `${group}=${number(hz)};`).join("");
  const manual = Object.keys(s.subs_manual).length > 0 ? "manual" : "|-";
  let key = `${s.mode.charAt(0).toUpperCase()}${s.mode.slice(1)}|${number(s.rc_lo)}|${optional(s.rc_hi)}|${optional(s.room_volume_m3)}|${xo}|`
    + `${s.mode === "ultra" ? number(s.ultra_ms) : "-"}|${manual}`
    + `|${number(s.infra_boost_max_db)}|${s.xo_peq_on}|${number(s.xo_peq_lo)}|${number(s.xo_peq_hi)}|${number(s.boost_max_db)}`;
  if (s.rc_hi_basis !== "strict") key += `|rc_hi_basis=${s.rc_hi_basis}`;
  if (s.rc_hi_transition !== 1) key += `|rc_hi_transition=${s.rc_hi_transition}`;
  if (s.rc_bands.length > 0) {
    const bands = [...s.rc_bands].sort((a, b) => a.lo_hz - b.lo_hz || a.hi_hz - b.hi_hz);
    key += `|rc_bands=${bands.map((band) => `${number(band.lo_hz)}-${number(band.hi_hz)}`).join(",")}`;
  }
  if (s.sample_rate_policy === "resample") key += "|sample_rate_policy=resample";
  for (const flag of ["low_protect", "lf_align", "mode_decay", "hf_phase", "dip_limit"] as const) if (s[flag]) key += `|${flag}`;
  if (s.input_hp_hz !== null) key += `|input_hp=${number(s.input_hp_hz)}`;
  if (s.peq_analytic_derivatives) key += "|peq_analytic_derivatives";
  return key;
}

/**
 * Copies of the recorded summary and lines of a calibration on settings with
 * this key. A combination that was not recorded gets the first record, the
 * defaults' (ADR 0052).
 */
export function recordedCalibration(key: string): { results: ResultsSummary; lines: ResultLinesResult } {
  const index = Math.max(0, RECORDED_RESULTS.findIndex((entry) => entry.params.engine_key === key));
  return { results: structuredClone(RECORDED_RESULTS[index]!.data), lines: structuredClone(RECORDED_RESULT_LINES[index]!.data) };
}

/**
 * A copy of the service's recorded refusal of this edit, or with `ofKind` of
 * the refused edit of the same kind (a value outside its limits refuses with
 * the same params whatever the value).
 */
export function recordedRefusal(edit: SettingEdit, ofKind = false): IpcError | undefined {
  const refusals = RECORDED_EDITS.flatMap((entry) => entry.params.edit.kind === edit.kind && !entry.data.ok ? [{ edit: entry.params.edit, error: entry.data.error }] : []);
  const refusal = refusals.find((entry) => same(entry.edit, edit)) ?? (ofKind ? refusals[0] : undefined);
  return refusal && structuredClone(refusal.error);
}
