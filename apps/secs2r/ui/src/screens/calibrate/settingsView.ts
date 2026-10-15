// What the calibration settings screen reads from the settings: the limits
// of every field, a sub's entry with defaults, its layout group and mode,
// whether its value fields apply, the infra sub's state and the room volume
// from three dimensions (upstream src/secs/calibration.py and
// wizard/step_calibration.py). Edits go to the service
// (`calibration_settings_edit`), which applies upstream's rules.
import type { BandType } from "../../ipc/bindings/BandType";
import type { BundleLayoutEntry } from "../../ipc/bindings/BundleLayoutEntry";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import type { SubEntry } from "../../ipc/bindings/SubEntry";
import type { SubLayout, SubModeChoice } from "./types";

/** Limits of every field (upstream calibration.py constants). */
export const LIMITS = {
  rcLo: [10, 200],
  rcHi: [100, 20000],
  volume: [0, 100000],
  dim: [0, 100],
  dimValid: [0.5, 100],
  boost: [0, 10],
  ultraMs: [30, 200],
  xo: [40, 200],
  xoPeq: [0, 1000],
  infraHz: [10, 300],
  infraBoost: [0, 24],
  gain: [-60, 20],
  delay: [0, 500],
  apHz: [10, 500],
  apQ: [0.1, 20],
  filterHz: [10, 300],
  peqGain: [-30, 12],
  mainDelay: [0, 100],
  rearTrim: [-20, 20],
  fsiso: [50, 150],
  mimoLow: [15, 150],
  mimoMax: [0, 12],
} as const;

export const BAND_TYPES: readonly BandType[] = ["LR24", "LR12", "LR48", "BW12", "BW24"];
export const SUB_BUNDLES = ["A", "B"] as const;
export const SUB_LAYOUTS: readonly SubLayout[] = ["values", "dba", "multi"];
export const SUB_FILTER_DEFAULT_HZ = { hp: 20, lp: 80 } as const;
export const INFRA_DEFAULT_HZ = 25;
export const MIMO_MIN_POSITIONS = 3;
export const DEFAULT_XO_HZ = 80;
export const ALLPASS_Q_DEFAULT = 0.71;

const FILTER_KEYS = ["hp_hz", "hp_type", "lp_hz", "lp_type", "peq"] as const;

const clamp = (value: number, [lo, hi]: readonly [number, number]) => Math.min(hi, Math.max(lo, value));
const finite = (value: number, fallback: number) => (Number.isFinite(value) ? value : fallback);

export const DEFAULT_SUB: SubEntry = {
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
    const v = clamp(finite(hz, 0), [0, LIMITS.filterHz[1]]);
    return v <= 0 ? 0 : Math.max(v, LIMITS.filterHz[0]);
  };
  return {
    mode: e.mode === "fixed" || e.mode === "bundle" ? e.mode : "auto",
    bundle: (SUB_BUNDLES as readonly string[]).includes(e.bundle) ? e.bundle : "A",
    gain_db: clamp(finite(e.gain_db, 0), LIMITS.gain),
    delay_ms: clamp(finite(e.delay_ms, 0), LIMITS.delay),
    polarity: e.polarity < 0 ? -1 : 1,
    allpass: e.allpass
      .filter(([f, q]) => Number.isFinite(f) && Number.isFinite(q) && f > 0)
      .slice(0, 2)
      .map(([f, q]) => [clamp(f, LIMITS.apHz), clamp(q, LIMITS.apQ)] as [number, number]),
    mimo_support: e.mimo_support,
    hp_hz: corner(e.hp_hz),
    lp_hz: corner(e.lp_hz),
    peq: e.peq
      .filter(([f, q, g]) => Number.isFinite(f) && Number.isFinite(q) && Number.isFinite(g) && f > 0)
      .slice(0, 3)
      .map(([f, q, g]) => [clamp(f, LIMITS.apHz), clamp(q, LIMITS.apQ), clamp(g, LIMITS.peqGain)] as [number, number, number]),
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

export function bundleMembers(settings: CalibrationSettings, bundle: string): string[] {
  return Object.entries(settings.subs_manual)
    .filter(([, e]) => e.mode === "bundle" && e.bundle === bundle)
    .map(([role]) => role);
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
  if (!dims || dims.length !== 3 || !dims.every((d) => Number.isFinite(d) && d >= LIMITS.dimValid[0] && d <= LIMITS.dimValid[1])) return null;
  return dims[0]! * dims[1]! * dims[2]!;
}
