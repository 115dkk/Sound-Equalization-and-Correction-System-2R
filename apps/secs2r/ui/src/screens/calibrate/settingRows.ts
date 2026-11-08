// The calibration settings the 보정 cards show as one simple control each
// (ADR 0055): a toggle, a number or a segmented choice that edits one field
// through one `SettingEdit` kind. A card lists the ids it shows, in its body
// or under 고급 (ADR 0038); `SettingRow` (controls.tsx) draws a row with the
// kit component its `control` names. The structured editors stay in
// Settings.tsx: the upper limit's basis, the room dimensions, the extra
// correction bands, the analytic scope, the crossover card, the sub editor
// and the infra sub.
import type { MessageKey } from "../../i18n";
import type { FormatNumber, Translate } from "../../i18n/react";
import type { CalibrationLimits } from "../../ipc/bindings/CalibrationLimits";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import { INPUT_HP_START } from "../../ipc/bindings/rules";
import { MIMO_MIN_POSITIONS, TRANSITION_FACTORS, infraState, subOf } from "./settingsView";
import { MODES, MODE_NAME, subRoles } from "./text";
import type { CalibrateView, SettingEdit } from "./types";

type Kind = SettingEdit["kind"];
/** The fields of edit `K` that carry a value (all but `kind`). */
type Param<K extends Kind> = Exclude<keyof Extract<SettingEdit, { kind: K }>, "kind">;
/**
 * The edit a row sends: its kind and the field of that edit the value goes
 * in (`boost_max` carries it in `db`). The generated `SCALAR_EDITS` names the
 * kind's settings field but not this one.
 */
type Sends = { [K in Kind]: { kind: K; param: Param<K> } }[Kind];

type When = (s: CalibrationSettings, view: CalibrateView) => boolean;

/** The line under a control; `ui` names its span when tests and tours point at the line. */
export type RowHint = string | { ui: string; text: string };

type Common = Sends & {
  /** The control's `data-ui` id, as the card wrote it by hand. */
  id: string;
  /** The settings field the control shows. */
  field: keyof CalibrationSettings;
  label: MessageKey;
  help?: MessageKey;
  /** Not shown where it has no meaning (ADR 0038); the card still decides which group holds it. */
  shownWhen?: When;
  /** Off, with the reason in `hint`. A disabled toggle shows off; the stored choice is kept. */
  disabledWhen?: When;
  hint?: (s: CalibrationSettings, view: CalibrateView, t: Translate) => RowHint | null;
};

export type ToggleRow = Common & {
  control: "toggle";
  /**
   * A switch over a number that may be null: null is its on state (MIMO 하한
   * 자동) or its off state (the input cut), and the other state sends `start`.
   */
  nullable?: { null: "on" | "off"; start: number };
};

export type NumberRow = Common & {
  control: "number";
  limits: keyof CalibrationLimits;
  step: number;
  unit?: string;
  digits?: number;
  showRange?: boolean;
  /** Width of the input box in characters. */
  size?: number;
};

/** One choice of a segmented row: the stored value and its name (a catalog key, or a number written out). */
export type Choice = { value: string | number; label: MessageKey | ((number: FormatNumber) => string) };

export type SegmentedRow = Common & { control: "segmented"; choices: readonly Choice[]; hideLabel?: boolean };

export type SettingRow = ToggleRow | NumberRow | SegmentedRow;

const zeroMode: When = (s) => s.mode === "zero";
const mimoShort: When = (_s, view) => view.positions.length < MIMO_MIN_POSITIONS;

export const SETTING_ROWS = [
  /* ── 입력 샘플레이트 ─────────────────────────────────────────────────── */
  {
    id: "calibrate.sample_rate_policy", control: "segmented", field: "sample_rate_policy", kind: "sample_rate_policy", param: "policy",
    label: "calibrate.sample_rate.title", help: "calibrate.sample_rate.help", hideLabel: true,
    choices: [
      { value: "reject", label: "calibrate.sample_rate.reject" },
      { value: "resample", label: "calibrate.sample_rate.resample" },
    ],
  },

  /* ── 모드 ─────────────────────────────────────────────────────────────── */
  {
    id: "calibrate.mode", control: "segmented", field: "mode", kind: "mode", param: "mode",
    label: "calibrate.mode.title", hideLabel: true,
    choices: MODES.map((mode) => ({ value: mode, label: MODE_NAME[mode] })),
  },
  {
    id: "calibrate.mode.ultra_ms", control: "number", field: "ultra_ms", kind: "ultra_ms", param: "ms",
    label: "calibrate.mode.ultra_ms.label", limits: "ultra_ms", step: 10, unit: "ms", showRange: true,
    shownWhen: (s) => s.mode === "ultra",
  },

  /* ── 보정 범위 ────────────────────────────────────────────────────────── */
  {
    id: "calibrate.range.lower", control: "number", field: "rc_lo", kind: "rc_lo", param: "hz",
    label: "calibrate.range.lower", limits: "rc_lo", step: 5, unit: "Hz", showRange: true,
  },
  {
    id: "calibrate.range.transition", control: "segmented", field: "rc_hi_transition", kind: "rc_hi_transition", param: "factor",
    label: "calibrate.range.transition", help: "calibrate.range.transition.help",
    choices: TRANSITION_FACTORS.map((factor) => ({ value: factor, label: (number: FormatNumber) => `×${number(factor)}` })),
  },
  {
    id: "calibrate.range.volume", control: "number", field: "room_volume_m3", kind: "room_volume", param: "m3",
    label: "calibrate.range.volume", help: "calibrate.range.volume.hint", limits: "volume", step: 10, digits: 1, unit: "m³",
  },
  // While the upper limit is automatic, the card passes the limit in force to show (`unset`).
  {
    id: "calibrate.range.upper", control: "number", field: "rc_hi", kind: "rc_hi", param: "hz",
    label: "calibrate.range.upper", limits: "rc_hi", step: 50, unit: "Hz", showRange: true,
  },
  // 극저역 보호: 자동 설정 (ADR 0022) and 수동 설정, the input cut (ADR 0031), its frequency shown while on.
  {
    id: "calibrate.range.low_protect", control: "toggle", field: "low_protect", kind: "low_protect", param: "on",
    label: "calibrate.range.low_protect", help: "calibrate.range.low_protect.help",
  },
  {
    id: "calibrate.range.input_hp", control: "toggle", field: "input_hp_hz", kind: "input_hp", param: "hz",
    label: "calibrate.range.input_hp", help: "calibrate.range.input_hp.help",
    nullable: { null: "off", start: INPUT_HP_START },
  },
  {
    id: "calibrate.range.input_hp.hz", control: "number", field: "input_hp_hz", kind: "input_hp", param: "hz",
    label: "calibrate.range.input_hp.hz", help: "calibrate.range.input_hp.hz.hint", limits: "input_hp", step: 1, unit: "Hz", showRange: true,
    shownWhen: (s) => s.input_hp_hz !== null,
  },
  {
    id: "calibrate.range.boost", control: "number", field: "boost_max_db", kind: "boost_max", param: "db",
    label: "calibrate.range.boost", help: "calibrate.range.boost.hint", limits: "boost", step: 1, digits: 1, unit: "dB",
  },
  // Deep narrow dips are filled only partly (ADR 0034): a limit beside the boost limit, not a protection.
  {
    id: "calibrate.range.dip_limit", control: "toggle", field: "dip_limit", kind: "dip_limit", param: "on",
    label: "calibrate.range.dip_limit", help: "calibrate.range.dip_limit.help",
  },

  /* ── 서브 정렬 (BA) ───────────────────────────────────────────────────── */
  {
    id: "calibrate.ba.main_delay", control: "number", field: "sub_main_delay_ms", kind: "main_delay", param: "ms",
    label: "calibrate.ba.main_delay", help: "calibrate.ba.main_delay.hint", limits: "main_delay", step: 0.1, digits: 2, unit: "ms",
    shownWhen: (s, view) => subRoles(view.config).some((role) => subOf(s, role).mode === "fixed"),
  },
  {
    id: "calibrate.ba.infra_boost", control: "number", field: "infra_boost_max_db", kind: "infra_boost", param: "db",
    label: "calibrate.ba.infra_boost", limits: "infra_boost", step: 1, digits: 1, unit: "dB", size: 5,
    shownWhen: (s, view) => infraState(s, subRoles(view.config)).role !== null,
  },

  /* ── 저음 도착과 울림 (ADR 0025) ─────────────────────────────────────── */
  // Zero mode does not change phase: the two phase alignments are off there with the reason.
  {
    id: "calibrate.lf_align", control: "toggle", field: "lf_align", kind: "lf_align", param: "on",
    label: "calibrate.lf_align", help: "calibrate.lf_align.help", disabledWhen: zeroMode,
    // In Mixed mode the short delay aligns the bass only part of the way, which the switch says while it is on.
    hint: (s, _view, t) =>
      s.mode === "zero" ? { ui: "calibrate.lf_align.unavailable", text: t("calibrate.lf_align.unavailable_zero", { mode: t(MODE_NAME.zero) }) }
      : s.mode === "mixed" && s.lf_align ? { ui: "calibrate.lf_align.partial", text: t("calibrate.lf_align.mixed", { mode: t(MODE_NAME.mixed) }) }
      : null,
  },
  {
    id: "calibrate.hf_phase", control: "toggle", field: "hf_phase", kind: "hf_phase", param: "on",
    label: "calibrate.hf_phase", help: "calibrate.hf_phase.help", disabledWhen: zeroMode,
    hint: (s, _view, t) => (s.mode === "zero" ? { ui: "calibrate.hf_phase.unavailable", text: t("calibrate.hf_phase.unavailable_zero", { mode: t(MODE_NAME.zero) }) } : null),
  },
  {
    id: "calibrate.mode_decay", control: "toggle", field: "mode_decay", kind: "mode_decay", param: "on",
    label: "calibrate.mode_decay", help: "calibrate.mode_decay.help",
  },

  /* ── MIMO ─────────────────────────────────────────────────────────────── */
  {
    id: "calibrate.mimo.on", control: "toggle", field: "mimo_on", kind: "mimo_on", param: "on",
    label: "calibrate.mimo.on", help: "calibrate.mimo.on.hint", disabledWhen: mimoShort,
    hint: (s, view, t) => (mimoShort(s, view) ? t("calibrate.mimo.unavailable", { count: view.positions.length, min: MIMO_MIN_POSITIONS }) : null),
  },
  {
    id: "calibrate.mimo.fsiso", control: "number", field: "fsiso", kind: "fsiso", param: "hz",
    label: "calibrate.mimo.fsiso", help: "calibrate.mimo.fsiso.hint", limits: "fsiso", step: 5, unit: "Hz",
  },
  // The guards judge the worst seat instead of the sum (ADR 0036): a MIMO-only setting, so it only stales MIMO.
  {
    id: "calibrate.mimo.seat_guard", control: "toggle", field: "mimo_seat_guard", kind: "mimo_seat_guard", param: "on",
    label: "calibrate.mimo.seat_guard", help: "calibrate.mimo.seat_guard.hint",
  },
  {
    id: "calibrate.mimo.low_auto", control: "toggle", field: "mimo_low", kind: "mimo_low", param: "hz",
    label: "calibrate.mimo.low_auto", help: "calibrate.mimo.low_auto.hint",
    nullable: { null: "on", start: 20 },
  },
  {
    id: "calibrate.mimo.low", control: "number", field: "mimo_low", kind: "mimo_low", param: "hz",
    label: "calibrate.mimo.low", limits: "mimo_low", step: 5, unit: "Hz", showRange: true,
    shownWhen: (s) => s.mimo_low !== null,
  },
  {
    id: "calibrate.mimo.max", control: "number", field: "mimo_max_supports", kind: "mimo_max", param: "count",
    label: "calibrate.mimo.max", help: "calibrate.mimo.max.hint", limits: "mimo_max", step: 1,
    hint: (_s, _view, t) => t("calibrate.mimo.max.zero"),
  },
  {
    id: "calibrate.mimo.fir", control: "toggle", field: "mimo_fir", kind: "mimo_fir", param: "on",
    label: "calibrate.mimo.fir", help: "calibrate.mimo.fir.hint",
  },
  // When the supports start to act after the onset (ADR 0037); 60 ms is the long-standing protection.
  {
    id: "calibrate.mimo.early", control: "number", field: "mimo_early_ms", kind: "mimo_early", param: "ms",
    label: "calibrate.mimo.early", help: "calibrate.mimo.early.hint", limits: "mimo_early_ms", step: 1, unit: "ms", showRange: true,
  },
] as const satisfies readonly SettingRow[];

export type SettingRowId = (typeof SETTING_ROWS)[number]["id"];

const BY_ID = new Map<string, SettingRow>(SETTING_ROWS.map((row) => [row.id, row]));

export function settingRow(id: SettingRowId): SettingRow {
  return BY_ID.get(id)!;
}

/** Whether row `id` has a meaning in `view`: the card leaves out a line of fields that holds only this row. */
export function rowShown(id: SettingRowId, view: CalibrateView): boolean {
  return settingRow(id).shownWhen?.(view.settings, view) ?? true;
}

/** The edit row `row` sends to store `value`. */
export function rowEdit(row: SettingRow, value: unknown): SettingEdit {
  return { kind: row.kind, [row.param]: value } as SettingEdit;
}
