// The 보정 step: what the screen draws (CalibrateView) and what the user can
// do (CalibrateActions). P33 builds the view from the service and implements
// the actions; the screen never calls the service itself
// (docs/packets/M5-screens-common.md, P31b).
import type { ComputePreference } from "../../ipc/bindings/ComputePreference";
import type { ComputeStatus } from "../../ipc/bindings/ComputeStatus";
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
import type { Notice } from "../../ipc/bindings/Notice";
import type { SettingEdit } from "../../ipc/bindings/SettingEdit";
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
export type StaleReason = "settings" | "target" | "measurements" | "config";

export type CalibrateView = {
  compute: { preference: ComputePreference; status: ComputeStatus | null; loading: boolean; saving: boolean; error: IpcError | null; locked: boolean };
  /** `results`: the latest calibration and MIMO summaries, with their stale flags. */
  results: ResultsSummary;
  /** `result_lines`: per-channel lines, RC notes and the BA lines. */
  lines: ResultLinesResult;
  /**
   * `result_curves { tab, channel, group: xoGroup, xo_hz: null, corrected: null }`:
   * the last curves received. While `curvesLoading` they may still belong to
   * the previous tab, channel or group; null before the first reply and after
   * a failed request.
   */
  curves: ResultCurvesResult | null;
  /**
   * The curves for the current tab, channel and group are being fetched
   * (selection change, or a refresh after a run or an edit). The graph keeps
   * `curves` dimmed meanwhile instead of showing them as the new selection.
   */
  curvesLoading: boolean;
  /** `calibration_settings_get`, and the reply of every `calibration_settings_set`. */
  settings: CalibrationSettings;
  /** `mode_spec { mode: settings.mode, ultra_ms: settings.ultra_ms, mimo_fir: settings.mimo_fir }`. */
  modeSpec: ModeSpecResult;
  /** `target_get` (and the reply of every target change). */
  target: TargetResult;
  targetEdit: "idle" | "saving" | "failed";
  xoEdit: "idle" | "saving" | "failed";
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
  /** `dsp_get().fields.fir_budget`: the FIR budget of the DSP being exported to (U2), in taps at its output rate; 0 when unknown. */
  firBudget: number;
  /** `dsp_get().fields.fir_taps`: the DSP's FIR taps per channel at its output rate; 0 when unknown. */
  firTaps: number;
  /**
   * `dsp_get().fields.output_rate` as the screens count it (ADR 0017): the
   * fixed rate, or 48000 for 자동 and while unknown. The design's 48 kHz
   * taps are converted to it before they are compared with the DSP.
   */
  outputRate: number;
  /** The last `recommend_xo` result (`JobResult` kind `xo`) of this session; null before one. */
  xoRecommendation: XoRecommendation | null;
  /** Which of this screen's jobs runs (from `jobs` and the job events); null when none. */
  running: CalibrateJob | null;
  /** A measurement job runs (the step is blocked with `measuring`). */
  measuring: boolean;
  /** The last of this screen's jobs that failed, until the next one starts. */
  failure: { job: CalibrateJob; error: IpcError } | null;
  /** `results.calibration.stale_reason`; null shows the general sentence. */
  staleReason: StaleReason | null;
  /** `results.calibration.mode` and `ultra_ms`; null without a calibration result. */
  resultMode: { mode: Mode; ultra_ms: number } | null;
  /** Selection P33 keeps across visits; it decides what `result_curves` asks for. */
  tab: ResultTab;
  /** Selected channel (role); null lets the service pick the first. */
  channel: string | null;
  /** Resolved XO group (configuration group key, `프론트` …); follows the selected channel on XO entry/channel changes, null without an eligible group. */
  xoGroup: string | null;
  /** sub_warnings: the subs' filter warnings for the current settings (upstream `sub_filter_warnings`, shown before a run). */
  subWarnings: Notice[];
};

/**
 * One change to the calibration settings, as the service takes it
 * (`calibration_settings_edit`, which applies upstream's rules: joining a
 * group, sharing filters, the infra sub, the room volume, the limits). Every
 * edit marks the result stale; nothing recomputes on its own.
 * `SubModeChoice` is a sub's BA mode as the screen offers it (자동, 고정,
 * 묶음 A, 묶음 B); `BandTypeChoice` adds "off" to the filter types.
 */
export type { BandTypeChoice } from "../../ipc/bindings/BandTypeChoice";
export type { SettingEdit } from "../../ipc/bindings/SettingEdit";
export type { SubLayout } from "../../ipc/bindings/SubLayout";
export type { SubModeChoice } from "../../ipc/bindings/SubModeChoice";

export type CalibrateActions = {
  /** Host: opens the measurement list to inspect a structured measurement error, or to measure again. */
  goToMeasurements: () => void;
  /** Host: opens the 내보내기 step once the result is current. */
  goToExport: () => void;
  changeCompute: (preference: ComputePreference) => void;
  refreshCompute: () => void;
  openGpuHelp: () => void;
  /** calibrate { settings: null, subs_only: false } */
  calibrate: () => void;
  /** calibrate { settings: null, subs_only: true } */
  calibrateSubs: () => void;
  /** recommend_xo */
  recommendXo: () => void;
  /** mimo { settings: null } */
  computeMimo: () => void;
  /** result_curves { tab, channel, group: xoGroup }: P33 keeps `tab` */
  selectTab: (tab: ResultTab) => void;
  /** result_curves { tab, channel: role, group }: keeps `channel` and resolves its XO group from the current configuration; the group argument is retained for view callers. */
  selectChannel: (role: string, group: string | null) => void;
  /** result_curves { tab, channel, group }: P33 keeps `xoGroup` */
  selectXoGroup: (group: string) => void;
  /** result_curves { tab: "xo", channel, group, xo_hz: hz } while the XO line is dragged; stores nothing */
  previewXo: (group: string, hz: number) => void;
  /** calibration_settings_edit { edit } */
  changeSetting: (edit: SettingEdit) => Promise<boolean>;
  /** sub_take_values */
  takeSubValues: () => void;
  /** target_set_points { points } */
  setTargetPoints: (points: [number, number][]) => Promise<boolean>;
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
