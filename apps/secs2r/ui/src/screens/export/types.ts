// 내보내기: what the screen draws and what it reports. Upstream
// src/secs/wizard/step8_mdat.py (the export step) and the hidden export card
// of step_calibration.py (staged mdat, U3).
import type { StepState } from "../../app/model";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import type { ExportBlock } from "../../ipc/bindings/ExportBlock";
import type { ExportFolderResult } from "../../ipc/bindings/ExportFolderResult";
import type { ExportKind } from "../../ipc/bindings/ExportKind";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { Notice } from "../../ipc/bindings/Notice";
import type { ResultsSummary } from "../../ipc/bindings/ResultsSummary";

/**
 * The order the export kinds appear in: the filters for the DSP (the
 * primary's CamillaDSP first, then the filter text and Equalizer APO), then
 * the responses for REW (the staged mdat under 고급).
 */
export const EXPORT_KINDS = ["camilla_dsp", "filter_text", "equalizer_apo", "before_after_mdat", "staged_mdat"] as const satisfies readonly ExportKind[];

/** Upstream `MIMO_MIN_POSITIONS`: MIMO needs three measured positions. */
export const MIMO_MIN_POSITIONS = 3;

/** How the last export of one kind ended, for the current result. */
export type ExportRun =
  /** `ExportResult.files`, and the folder they went to (the common parent of the files). */
  | { status: "done"; files: string[]; folder: string }
  /** The job's `IpcError` (export_failed, stale_inputs, cancelled, …). */
  | { status: "failed"; error: IpcError }
  /** In the tutorial nothing is written (upstream `trial`): P33 records this instead of calling `export`. */
  | { status: "tutorial" };

export type ExportView = {
  /** `results`: the latest correction and MIMO summaries. */
  results: ResultsSummary;
  /**
   * Current `calibration_settings_get` mode, ultra limit and MIMO switch.
   * The computed mode and ultra limit are in `results.calibration.mode`
   * and `results.calibration.ultra_ms`.
   */
  settings: Pick<CalibrationSettings, "mode" | "ultra_ms" | "mimo_on">;
  /** `project_info().positions.length`: MIMO needs three positions or more. */
  positions: number;
  /** `result_lines().rc`: how the RC upper limit was set (`rc_limit`, `rc_room_volume`, `rc_no_room_volume`, `rc_auto_cap`). */
  rc: Notice[];
  /** `export_folder_get`; null until it arrives. */
  folder: ExportFolderResult | null;
  /** A `calibrate` job runs (job events): the result is about to change. */
  computing: boolean;
  /** A calibration job is running or awaiting cancellation; the recovery action stays busy until it finishes. */
  recalibrating: boolean;
  /** The kind of the running `export` job, or null. Export jobs run one at a time. */
  exporting: ExportKind | null;
  /** The last export of each kind for the current result. P33 keeps it for the session and clears it when a new result arrives. */
  runs: Partial<Record<ExportKind, ExportRun>>;
  /** Tutorial mode (the demo project): exports write nothing. */
  tutorial: boolean;
};

export type ExportActions = {
  /**
   * `export { kind, folder: "" }` (job; the service picks the subfolder). In
   * the tutorial P33 records `{ status: "tutorial" }` instead. Upstream opened
   * the written folder after a successful export: P33 follows with
   * `open_folder { path: run.folder }`.
   */
  exportFiles: (kind: ExportKind) => void;
  /** `dialog_pick_folder { start: folder }`, then `export_folder_set { folder }` when one was picked. */
  changeFolder: () => void;
  /** `open_folder { path }`: the export folder, or the folder a run wrote. */
  openFolder: (path: string) => void;
  /** Host: the frame goes to 보정. */
  goToCalibrate: () => void;
  /** `calibrate { settings: null, subs_only: false }`: recompute with current settings without leaving export. */
  recalibrate: () => void;
};

/** Whether MIMO goes into the export, and if not, why (upstream `show_mimo`: on, computed, current, three positions or more). */
export type MimoInclusion =
  | { included: true; kind: string; supports: number; channels: number }
  | { included: false; reason: "off" | "positions" | "none" | "stale" };

export function mimoInclusion(view: ExportView): MimoInclusion {
  const mimo = view.results.mimo;
  if (!view.settings.mimo_on) return { included: false, reason: "off" };
  if (view.positions < MIMO_MIN_POSITIONS) return { included: false, reason: "positions" };
  if (!mimo) return { included: false, reason: "none" };
  if (mimo.stale) return { included: false, reason: "stale" };
  const channels = mimo.input_count;
  return { included: true, kind: mimo.kind, supports: mimo.support_count, channels };
}

/**
 * Why `kind` cannot carry the current result, or null when it can (and when
 * there is no result). Follows the service's `filter_export_block`
 * (crates/secs-types/src/ipc.rs, upstream `calibration.filter_export_block`):
 * the filter text carries RC only, so subs block it first and MIMO next;
 * CamillaDSP, Equalizer APO and the mdat exports carry everything
 * (upstream stops MIMO at Equalizer APO too; DEVIATIONS F12). MIMO counts
 * only when it goes into the export (`mimoInclusion`).
 */
export function exportBlock(view: ExportView, kind: ExportKind): ExportBlock | null {
  const result = view.results.calibration;
  if (!result) return null;
  if ((kind === "staged_mdat" || kind === "before_after_mdat") && result.cached && result.stale) return "stale_cache";
  const subs = result.sub_roles.length > 0;
  const mimo = mimoInclusion(view).included;
  if (kind === "filter_text" && subs) return "subs";
  if (kind === "filter_text" && mimo) return "mimo";
  return null;
}

/**
 * The mode in the filter files' names (upstream `_filter_plan`): the mode,
 * and for 초고지연 its limit in ms as Python's `:g` writes it (`ultra200`,
 * `ultra62.5`; six significant digits cover the 30–200 ms the limit takes).
 * Folder names keep the bare mode (`SECS_<kind>_<what>_ultra_…`).
 */
export function filterModeName(settings: Pick<CalibrationSettings, "mode" | "ultra_ms">): string {
  return settings.mode === "ultra" ? `ultra${Number(settings.ultra_ms.toPrecision(6))}` : settings.mode;
}

/**
 * Whether the export buttons can start an export now (upstream `ready()`:
 * a result and no correction running). A stale result exports as it is, as
 * upstream does; the screen warns about it.
 */
export function canExport(view: ExportView): boolean {
  return !!view.results.calibration && !view.computing && view.exporting === null;
}

/** The rail state of 내보내기: blocked while there is no result or one is being computed, done after an export of the result. */
export function exportStepState(view: ExportView): StepState {
  const result = view.results.calibration;
  if (view.computing) return { status: "blocked", reason: "computing" };
  if (!result) return { status: "blocked", reason: "no_result" };
  if (Object.values(view.runs).some((run) => run?.status === "done")) return { status: "done" };
  return { status: "ready" };
}
