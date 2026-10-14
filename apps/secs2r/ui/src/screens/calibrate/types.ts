// The 보정 step: what the screen draws (CalibrateView) and what the user can
// do (CalibrateActions). P33 builds the view from the service and implements
// the actions; the screen never calls the service itself
// (docs/packets/M5-screens-common.md, P31b).
import type { StepState } from "../../app/model";
import type { BandType } from "../../ipc/bindings/BandType";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import type { ConfigResult } from "../../ipc/bindings/ConfigResult";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { Mode } from "../../ipc/bindings/Mode";
import type { ModeSpecResult } from "../../ipc/bindings/ModeSpecResult";
import type { ResultCurvesResult } from "../../ipc/bindings/ResultCurvesResult";
import type { ResultLinesResult } from "../../ipc/bindings/ResultLinesResult";
import type { ResultTab } from "../../ipc/bindings/ResultTab";
import type { ResultsSummary } from "../../ipc/bindings/ResultsSummary";
import type { SubEntry } from "../../ipc/bindings/SubEntry";
import type { TargetResult } from "../../ipc/bindings/TargetResult";
import type { XoRecommendation } from "../../ipc/bindings/XoRecommendation";

/**
 * This screen's long jobs. They never run side by side (upstream ran one at
 * a time): while one runs, the screen disables starting the others. Progress
 * and cancel live in the job tray (U8). `calibrate_subs` is the `calibrate`
 * job started with `subs_only: true`.
 */
export type CalibrateJob = "calibrate" | "calibrate_subs" | "recommend_xo" | "mimo";

/** What changed since the result was computed, when P33 knows it. */
export type StaleReason = "settings" | "target" | "measurements";

export type CalibrateView = {
  /** `results`: the latest calibration and MIMO summaries, with their stale flags. */
  results: ResultsSummary;
  /** `result_lines`: per-channel lines, RC notes and the BA lines. */
  lines: ResultLinesResult;
  /** `result_curves { tab, channel, group: xoGroup, xo_hz: null, corrected: null }` for the open tab; null while it loads. */
  curves: ResultCurvesResult | null;
  /** `calibration_settings_get`, and the reply of every `calibration_settings_set`. */
  settings: CalibrationSettings;
  /** `mode_spec { mode: settings.mode, ultra_ms: settings.ultra_ms }`. */
  modeSpec: ModeSpecResult;
  /** `target_get` (and the reply of every target change). */
  target: TargetResult;
  /** `config_get`: speakers, subs and groups (XO groups, BA subs). */
  config: ConfigResult;
  /** `project_info.positions`: measured positions. */
  positions: number[];
  /**
   * Roles measured at position 1, in configuration order: the channel list
   * before the first run (upstream previewed position 1). P33 reads them from
   * `measurement_curves { positions: [1] }` (service gap: no cheap list).
   */
  measuredRoles: string[];
  /** `dsp_get().fields.fir_budget`: the FIR budget of the DSP being exported to (U2); 0 when unknown. */
  firBudget: number;
  /** The last `recommend_xo` result (`JobResult` kind `xo`) of this session; null before one. */
  xoRecommendation: XoRecommendation | null;
  /** Which of this screen's jobs runs (from `jobs` and the job events); null when none. */
  running: CalibrateJob | null;
  /** A measurement job runs (the step is blocked with `measuring`). */
  measuring: boolean;
  /** The last of this screen's jobs that failed, until the next one starts. */
  failure: { job: CalibrateJob; error: IpcError } | null;
  /**
   * Why the result is stale, when P33 saw the change (service gap: the
   * summary's `stale` carries no reason); null shows the general sentence.
   */
  staleReason: StaleReason | null;
  /**
   * The mode the shown result was computed with: P33 keeps the settings it
   * started the run with (service gap: `CalibrationSummary` has no mode);
   * null when unknown (a result restored from the cache).
   */
  resultMode: { mode: Mode; ultra_ms: number } | null;
  /** Selection P33 keeps across visits; it decides what `result_curves` asks for. */
  tab: ResultTab;
  /** Selected channel (role); null lets the service pick the first. */
  channel: string | null;
  /** Selected XO group (configuration group key, `프론트` …); null before one exists. */
  xoGroup: string | null;
};

/** A sub's BA mode as the screen offers it: 자동, 고정, 묶음 A, 묶음 B. */
export type SubModeChoice = "auto" | "fixed" | "bundle:A" | "bundle:B";
export type SubLayout = "values" | "dba" | "multi";
export type BandTypeChoice = BandType | "off";

/**
 * One change to the calibration settings. Every one is stored with
 * `calibration_settings_set { settings: applySettingEdit(view.settings, edit) }`
 * (settingsEdit.ts ports upstream `CalibrationSettings.set_sub`,
 * `set_layout`, `StepCalibration.set_infra` and the room volume rules) and
 * marks the result stale; nothing recomputes on its own.
 */
export type SettingEdit =
  | { kind: "mode"; mode: Mode }
  | { kind: "ultra_ms"; ms: number }
  | { kind: "rc_lo"; hz: number }
  /** null = automatic upper limit (Schroeder, at most 500 Hz). */
  | { kind: "rc_hi"; hz: number | null }
  /** 0 = no room volume; clears dimensions that no longer match it. */
  | { kind: "room_volume"; m3: number }
  /** Width, depth, height in m; all three set the volume. */
  | { kind: "room_dims"; dims: [number, number, number] }
  | { kind: "boost_max"; db: number }
  | { kind: "xo"; group: string; hz: number }
  | { kind: "xo_peq"; on?: boolean; lo?: number; hi?: number }
  /** The infra sub (null: none) with its crossover; `subs` are the configured sub roles. */
  | { kind: "infra"; role: string | null; hz: number; type: BandType; subs: readonly string[] }
  | { kind: "infra_boost"; db: number }
  | { kind: "sub_mode"; role: string; mode: SubModeChoice }
  | { kind: "sub_layout"; bundle: string; layout: SubLayout }
  | { kind: "rear_trim"; bundle: string; ms: number }
  /** Values, filters, DBA row or MIMO support of one sub. */
  | { kind: "sub"; role: string; patch: Partial<Omit<SubEntry, "mode" | "bundle">> }
  /** Picking a type turns the filter on (HP 20 Hz, LP 80 Hz when empty); "off" turns it off. */
  | { kind: "sub_band_type"; role: string; band: "hp" | "lp"; type: BandTypeChoice }
  | { kind: "main_delay"; ms: number }
  | { kind: "mimo_on"; on: boolean }
  | { kind: "fsiso"; hz: number }
  /** null = automatic lower limit. */
  | { kind: "mimo_low"; hz: number | null }
  | { kind: "mimo_max"; count: number }
  | { kind: "mimo_fir"; on: boolean };

export type CalibrateActions = {
  /** calibrate { settings: null, subs_only: false } (a first run without an XO recommendation recommends first: the service's business) */
  calibrate: () => void;
  /** calibrate { settings: null, subs_only: true } */
  calibrateSubs: () => void;
  /** recommend_xo */
  recommendXo: () => void;
  /** mimo { settings: null } */
  computeMimo: () => void;
  /** result_curves { tab, channel, group: xoGroup }: P33 keeps `tab` */
  selectTab: (tab: ResultTab) => void;
  /** result_curves { tab, channel: role, group }: P33 keeps `channel`, and on the XO tab sets `xoGroup` to `group` (the channel's group) */
  selectChannel: (role: string, group: string | null) => void;
  /** result_curves { tab, channel, group }: P33 keeps `xoGroup` */
  selectXoGroup: (group: string) => void;
  /** result_curves { tab: "xo", channel, group, xo_hz: hz } while the XO line is dragged; stores nothing */
  previewXo: (group: string, hz: number) => void;
  /** calibration_settings_set { settings: applySettingEdit(view.settings, edit) } */
  changeSetting: (edit: SettingEdit) => void;
  /** sub_take_values */
  takeSubValues: () => void;
  /** target_set_points { points } */
  setTargetPoints: (points: [number, number][]) => void;
  /** target_apply_preset { preset } */
  applyTargetPreset: (preset: string) => void;
  /** dialog_save_file { kind: "target", suggested: "타겟.json" }, then target_save { path } */
  saveTarget: () => void;
  /** dialog_open_file { kind: "target" }, then target_load { path } */
  loadTarget: () => void;
};

/** What the rail shows for 보정: blocked without measurements or while measuring, done with a current result. */
export function calibrateStepState(view: Pick<CalibrateView, "measuring" | "positions" | "results">): StepState {
  if (view.measuring) return { status: "blocked", reason: "measuring" };
  if (view.positions.length === 0) return { status: "blocked", reason: "no_measurements" };
  const result = view.results.calibration;
  if (result && !result.stale) return { status: "done" };
  return { status: "ready" };
}
