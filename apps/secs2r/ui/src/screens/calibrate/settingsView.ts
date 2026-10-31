// What the calibration settings screen reads from the settings: the limits
// of every field, a sub's entry with defaults, its layout group and mode,
// whether its value fields apply, the infra sub's state and the room volume
// from three dimensions (upstream src/secs/calibration.py and
// wizard/step_calibration.py). Edits go to the service
// (`calibration_settings_edit`), which applies upstream's rules.
import type { BandType } from "../../ipc/bindings/BandType";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { RcBand } from "../../ipc/bindings/RcBand";
import type { RcHiBasis } from "../../ipc/bindings/RcHiBasis";
import type { SubEntry } from "../../ipc/bindings/SubEntry";
import { CALIBRATION_LIMITS } from "../../ipc/bindings/rules";
import type { SettingEdit, SubLayout, SubModeChoice } from "./types";

/** Limits of every value the service clamps on an edit (generated from secs_types::rules). */
export const LIMITS = CALIBRATION_LIMITS;

/**
 * Limits of a sub's entry, as the project adapter normalizes it
 * (secs-project calibration.rs, upstream `sub_entry`); not generated yet.
 */
export const SUB_LIMITS = {
  gain: [-60, 20],
  delay: [0, 500],
  apHz: [10, 500],
  apQ: [0.1, 20],
  filterHz: [10, 300],
  peqGain: [-30, 12],
} as const;

export const BAND_TYPES: readonly BandType[] = ["LR24", "LR12", "LR48", "BW12", "BW24"];
const SUB_BUNDLES = ["A", "B"] as const;
export const INFRA_DEFAULT_HZ = 25;
export const MIMO_MIN_POSITIONS = 3;
export const ALLPASS_Q_DEFAULT = 0.71;

/**
 * Whether 보정 계산 computes MIMO in the same job (`calibrate { mimo }`,
 * P141): MIMO on and enough measured positions. The service refuses the
 * request otherwise, so the screen never asks for it then.
 */
export const mimoIncluded = (settings: Pick<CalibrationSettings, "mimo_on">, positions: number) => settings.mimo_on && positions >= MIMO_MIN_POSITIONS;

/* ── RC upper limit (ADR 0021) ──────────────────────────────────────────── */

/** The upper limit as the range card offers it: an automatic basis, or 직접 입력 (`rc_hi` set). */
export type UpperChoice = RcHiBasis | "manual";
export const UPPER_CHOICES: readonly UpperChoice[] = ["strict", "lenient", "agreement", "manual"];
/** The Schroeder bases' transition factors; the service rejects any other. */
export const TRANSITION_FACTORS = [1, 2, 4] as const;
/** Measured positions the agreement basis needs (secs-service `check_rc_agreement`). */
export const AGREEMENT_MIN_POSITIONS = 3;

export function upperChoice(settings: CalibrationSettings): UpperChoice {
  return settings.rc_hi === null ? settings.rc_hi_basis : "manual";
}

/** Whether the room volume and the transition factor apply: the two Schroeder bases. */
export const usesSchroeder = (choice: UpperChoice) => choice === "strict" || choice === "lenient";

/**
 * The edits that choose `choice`, in the order to send them. 직접 입력 starts
 * from the limit in force (`shownHi`), or from the first 50 Hz step above the
 * lower edge when that limit is not above it (the service refuses an upper
 * edge at or below the lower one, ADR 0024); leaving it stores the basis
 * first, so the card never shows the old basis in between, then clears
 * `rc_hi`. The current choice needs none.
 */
export function upperEdits(settings: CalibrationSettings, choice: UpperChoice, shownHi: number): SettingEdit[] {
  if (choice === upperChoice(settings)) return [];
  if (choice === "manual") return [{ kind: "rc_hi", hz: Math.max(shownHi, LIMITS.rc_hi[0], (Math.floor(settings.rc_lo / 50) + 1) * 50) }];
  if (settings.rc_hi === null) return [{ kind: "rc_hi_basis", basis: choice }];
  return [
    { kind: "rc_hi_basis", basis: choice },
    { kind: "rc_hi", hz: null },
  ];
}

/* ── Extra correction bands (ADR 0024) ──────────────────────────────────── */

/** Extra bands the range card holds (`rc_bands_max`). */
export const MAX_EXTRA_BANDS = CALIBRATION_LIMITS.rc_bands_max[1];

/** Round spans a new extra band starts at, the highest first. */
const BAND_LADDER: readonly (readonly [number, number])[] = [
  [15000, 20000], [10000, 15000], [5000, 10000], [2000, 5000], [1000, 2000], [500, 1000], [200, 500], [100, 200], [50, 100], [20, 50],
];

/**
 * Where a new extra band starts: the first span of the ladder that keeps a
 * gap to the primary band (`rc_lo` to the manual upper limit, or `primaryHi`,
 * the limit in force) and to every band already there. Each span is a valid
 * band, so adding one never draws a rejection; when none is clear it starts
 * at 15–20 kHz, which the engine merges with what covers it.
 */
export function nextBand(settings: CalibrationSettings, primaryHi: number): RcBand {
  const taken: (readonly [number, number])[] = [[settings.rc_lo, settings.rc_hi ?? primaryHi], ...settings.rc_bands.map((band) => [band.lo_hz, band.hi_hz] as const)];
  const clear = ([lo, hi]: readonly [number, number]) => taken.every(([a, b]) => hi < a || lo > b);
  const [lo_hz, hi_hz] = BAND_LADDER.find(clear) ?? BAND_LADDER[0]!;
  return { lo_hz, hi_hz };
}

/** The extra bands with one edge of band `index` changed: the whole list the `rc_bands` edit sends. */
export function withBandEdge(bands: readonly RcBand[], index: number, edge: "lower" | "upper", hz: number): RcBand[] {
  return bands.map((band, i) => (i !== index ? { ...band } : edge === "lower" ? { ...band, lo_hz: hz } : { ...band, hi_hz: hz }));
}

/** The edits whose refusal the range card shows at the input it concerns. */
export type RangeEdit = Extract<SettingEdit, { kind: "rc_lo" | "rc_hi" | "rc_bands" }>;
export const isRangeEdit = (edit: SettingEdit): edit is RangeEdit => edit.kind === "rc_lo" || edit.kind === "rc_hi" || edit.kind === "rc_bands";

/** The reasons the service gives when it refuses a range edit (`invalid_request` params, ADR 0024). */
const RANGE_REASONS: ReadonlySet<string> = new Set(["rc_lo_range", "rc_lo_above_hi", "rc_hi_range", "rc_hi_below_lo", "rc_band_range", "rc_band_count"]);

/** A range edit the service refused, with its reason; the range card shows it in place of a global notice. */
export type RangeRejection = { edit: RangeEdit; error: IpcError };

/** The refusal of `edit` as the range card shows it, or null when it is not one (another edit, field or reason). */
export function rangeRejection(edit: SettingEdit, error: IpcError): RangeRejection | null {
  if (!isRangeEdit(edit) || error.code !== "invalid_request") return null;
  if (error.params.field !== edit.kind || !RANGE_REASONS.has(error.params.reason ?? "")) return null;
  return { edit, error };
}

/**
 * The input a refusal concerns: the primary lower or upper edge (the basis
 * choice while the upper edge is automatic, as when 직접 입력 is chosen), an
 * edge of an extra band's row, or the add action. A band the service names
 * by its 1-based `index` is the row at that place, since the card sends the
 * stored list with one edge changed; an edge outside 20–20000 Hz is the one
 * refused, otherwise the edge that differs from the stored band.
 */
export type RejectionPlace = { at: "lower" | "upper" | "basis" | "add" } | { at: "band"; index: number; edge: "lower" | "upper" };

export function rejectionPlace({ edit, error }: RangeRejection, settings: CalibrationSettings): RejectionPlace {
  if (edit.kind === "rc_lo") return { at: "lower" };
  if (edit.kind === "rc_hi") return { at: upperChoice(settings) === "manual" ? "upper" : "basis" };
  const index = Number(error.params.index) - 1;
  if (error.params.reason !== "rc_band_range" || !Number.isInteger(index) || index < 0 || index >= settings.rc_bands.length) return { at: "add" };
  const lo = Number(error.params.lo_hz);
  const hi = Number(error.params.hi_hz);
  const inside = (hz: number) => hz >= LIMITS.rc_band[0] && hz <= LIMITS.rc_band[1];
  const edge = !inside(lo) ? "lower" : !inside(hi) ? "upper" : settings.rc_bands[index]!.lo_hz === lo ? "upper" : "lower";
  return { at: "band", index, edge };
}


const clamp = (value: number, [lo, hi]: readonly [number, number]) => Math.min(hi, Math.max(lo, value));
const finite = (value: number, fallback: number) => (Number.isFinite(value) ? value : fallback);

const DEFAULT_SUB: SubEntry = {
  mode: "auto",
  bundle: "A",
  gain_db: 0,
  delay_ms: 0,
  polarity: 1,
  allpass: [],
  mimo_support: true,
  hp_hz: 0,
  lp_hz: 0,
  peq: [],
  hp_type: "LR24",
  lp_type: "LR24",
  dba_row: "front",
};

const isBandType = (value: string): value is BandType => (BAND_TYPES as readonly string[]).includes(value);

/** One sub's settings within the limits (upstream `sub_entry`). */
export function normalizeSub(value: Partial<SubEntry>): SubEntry {
  const e = { ...DEFAULT_SUB, ...value };
  const corner = (hz: number) => {
    const v = clamp(finite(hz, 0), [0, SUB_LIMITS.filterHz[1]]);
    return v <= 0 ? 0 : Math.max(v, SUB_LIMITS.filterHz[0]);
  };
  return {
    mode: e.mode === "fixed" || e.mode === "bundle" ? e.mode : "auto",
    bundle: (SUB_BUNDLES as readonly string[]).includes(e.bundle) ? e.bundle : "A",
    gain_db: clamp(finite(e.gain_db, 0), SUB_LIMITS.gain),
    delay_ms: clamp(finite(e.delay_ms, 0), SUB_LIMITS.delay),
    polarity: e.polarity < 0 ? -1 : 1,
    allpass: e.allpass
      .filter(([f, q]) => Number.isFinite(f) && Number.isFinite(q) && f > 0)
      .slice(0, 2)
      .map(([f, q]) => [clamp(f, SUB_LIMITS.apHz), clamp(q, SUB_LIMITS.apQ)] as [number, number]),
    mimo_support: e.mimo_support,
    hp_hz: corner(e.hp_hz),
    lp_hz: corner(e.lp_hz),
    peq: e.peq
      .filter(([f, q, g]) => Number.isFinite(f) && Number.isFinite(q) && Number.isFinite(g) && f > 0)
      .slice(0, 3)
      .map(([f, q, g]) => [clamp(f, SUB_LIMITS.apHz), clamp(q, SUB_LIMITS.apQ), clamp(g, SUB_LIMITS.peqGain)] as [number, number, number]),
    hp_type: isBandType(e.hp_type) ? e.hp_type : "LR24",
    lp_type: isBandType(e.lp_type) ? e.lp_type : "LR24",
    dba_row: e.dba_row === "rear" ? "rear" : "front",
  };
}

/** A sub's settings, or the defaults (자동, MIMO support, no filters). */
export function subOf(settings: CalibrationSettings, role: string): SubEntry {
  const entry = settings.subs_manual[role];
  return entry ? normalizeSub(entry) : { ...DEFAULT_SUB };
}

/** The layout of a group: 상대값 고정 (`values`), DBA or 멀티서브. */
export function layoutOf(settings: CalibrationSettings, bundle: string): SubLayout {
  const kind = settings.bundle_layouts[bundle]?.kind;
  return kind === "dba" || kind === "multi" ? kind : "values";
}

/** The mode a sub shows in the 방식 list. */
export function subModeChoice(entry: SubEntry): SubModeChoice {
  if (entry.mode === "bundle") return entry.bundle === "B" ? "bundle:B" : "bundle:A";
  return entry.mode === "fixed" ? "fixed" : "auto";
}

/** Whether the value fields (gain, delay, polarity, all-pass) apply: 고정, or a group with 상대값 고정. */
export function subValuesApply(settings: CalibrationSettings, entry: SubEntry): boolean {
  if (entry.mode === "fixed") return true;
  return entry.mode === "bundle" && layoutOf(settings, entry.bundle) === "values";
}

/**
 * The infra crossover read back from the filters (upstream `infra_state`):
 * the one sub with only a low-pass while every other sub has a high-pass.
 */
export function infraState(settings: CalibrationSettings, subs: readonly string[]): { role: string | null; hz: number; type: BandType } {
  const lows = subs.filter((r) => {
    const e = subOf(settings, r);
    return e.lp_hz > 0 && e.hp_hz <= 0;
  });
  if (lows.length > 0 && subs.filter((r) => !lows.includes(r)).every((r) => subOf(settings, r).hp_hz > 0)) {
    const e = subOf(settings, lows[0]!);
    return { role: lows[0]!, hz: e.lp_hz, type: isBandType(e.lp_type) ? e.lp_type : "LR24" };
  }
  return { role: null, hz: 0, type: "LR24" };
}

/** Room volume from width × depth × height, when all three are valid (upstream `room_volume`). */
export function roomVolume(dims: readonly number[] | null): number | null {
  if (!dims || dims.length !== 3 || !dims.every((d) => Number.isFinite(d) && d >= LIMITS.dim_valid[0] && d <= LIMITS.dim_valid[1])) return null;
  return dims[0]! * dims[1]! * dims[2]!;
}
