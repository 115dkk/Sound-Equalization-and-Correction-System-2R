// Calibration settings edits, as pure functions. `calibration_settings_set`
// stores a whole settings object, so the rules upstream applied while the
// user edited live here: `CalibrationSettings.set_sub` (joining a group,
// sharing filters inside a layout group), `set_layout`, `sub_entry`'s
// limits, `StepCalibration.set_infra`, `_on_volume`, `_on_dims` and
// `_on_sub_band_type` (src/secs/calibration.py, wizard/step_calibration.py).
// P33 sends `applySettingEdit(view.settings, edit)`; the service normalizes
// what it stores again.
import type { BandType } from "../../ipc/bindings/BandType";
import type { BundleLayoutEntry } from "../../ipc/bindings/BundleLayoutEntry";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import type { SubEntry } from "../../ipc/bindings/SubEntry";
import type { SettingEdit, SubLayout, SubModeChoice } from "./types";

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

type Draft = {
  subs: Record<string, SubEntry>;
  layouts: Record<string, BundleLayoutEntry>;
};

const draftOf = (settings: CalibrationSettings): Draft => ({
  subs: Object.fromEntries(Object.entries(settings.subs_manual).map(([role, e]) => [role, normalizeSub(e)])),
  layouts: Object.fromEntries(Object.entries(settings.bundle_layouts).map(([b, e]) => [b, { ...e }])),
});

const draftLayout = (draft: Draft, bundle: string): SubLayout => {
  const kind = draft.layouts[bundle]?.kind;
  return kind === "dba" || kind === "multi" ? kind : "values";
};

const draftMembers = (draft: Draft, bundle: string) =>
  Object.entries(draft.subs)
    .filter(([, e]) => e.mode === "bundle" && e.bundle === bundle)
    .map(([role]) => role);

/** Inside a layout group (DBA, 멀티서브) every member has the same fixed filters (upstream `_share_filters`). */
function shareFilters(draft: Draft, role: string) {
  const e = draft.subs[role];
  if (!e || e.mode !== "bundle" || draftLayout(draft, e.bundle) === "values") return;
  for (const other of draftMembers(draft, e.bundle)) {
    if (other === role) continue;
    const target = draft.subs[other]!;
    draft.subs[other] = normalizeSub({
      ...target,
      hp_hz: e.hp_hz,
      hp_type: e.hp_type,
      lp_hz: e.lp_hz,
      lp_type: e.lp_type,
      peq: e.peq.map((band) => [...band] as [number, number, number]),
    });
  }
}

/** Upstream `CalibrationSettings.set_sub`. */
function setSub(draft: Draft, role: string, values: Partial<SubEntry>) {
  const before = draft.subs[role] ?? { ...DEFAULT_SUB };
  const joining = values.mode === "bundle" && (before.mode !== "bundle" || before.bundle !== (values.bundle ?? before.bundle));
  draft.subs[role] = normalizeSub({ ...before, ...values });
  const e = draft.subs[role]!;
  if (joining) {
    const others = draftMembers(draft, e.bundle).filter((r) => r !== role);
    if (others.length === 0 && !(e.bundle in draft.layouts)) {
      draft.layouts[e.bundle] = { kind: "values", rear_trim_ms: 0 };
    } else if (others.length > 0) {
      shareFilters(draft, others[0]!);
      return;
    }
  }
  if (FILTER_KEYS.some((key) => key in values)) shareFilters(draft, role);
}

/** Upstream `CalibrationSettings.set_layout`. */
function setLayout(draft: Draft, bundle: string, kind?: SubLayout, rearTrim?: number) {
  const current = draft.layouts[bundle] ?? { kind: "values", rear_trim_ms: 0 };
  const next: BundleLayoutEntry = {
    kind: kind ?? (current.kind === "dba" || current.kind === "multi" ? current.kind : "values"),
    rear_trim_ms: clamp(finite(rearTrim ?? current.rear_trim_ms, 0), LIMITS.rearTrim),
  };
  draft.layouts[bundle] = next;
  const members = draftMembers(draft, bundle);
  if (next.kind !== "values" && members.length > 0) shareFilters(draft, members[0]!);
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

/** Upstream `StepCalibration.set_infra`: a LP on the infra sub (or its layout group), the matching HP on the others. */
function setInfra(settings: CalibrationSettings, draft: Draft, role: string | null, hz: number, type: BandType, subs: readonly string[]) {
  const before = infraState(settings, subs);
  if (!role) {
    if (before.role) {
      for (const r of subs) {
        if (r === before.role) setSub(draft, r, { lp_hz: 0 });
        else if ((draft.subs[r] ?? DEFAULT_SUB).hp_hz === before.hz) setSub(draft, r, { hp_hz: 0 });
      }
    }
    return;
  }
  const at = clamp(finite(hz, INFRA_DEFAULT_HZ), LIMITS.infraHz);
  const e = draft.subs[role] ?? { ...DEFAULT_SUB };
  const group = new Set<string>([role]);
  if (e.mode === "bundle" && draftLayout(draft, e.bundle) !== "values") for (const m of draftMembers(draft, e.bundle)) group.add(m);
  for (const r of subs) {
    if (group.has(r)) {
      setSub(draft, r, { lp_hz: at, lp_type: type, hp_hz: 0 });
    } else {
      // A LP below or near the infra crossover blocks the band above it: an earlier infra LP.
      const lp = (draft.subs[r] ?? DEFAULT_SUB).lp_hz;
      const clear = r === before.role || (lp > 0 && lp < 2 * Math.max(at, before.hz || at));
      setSub(draft, r, { hp_hz: at, hp_type: type, ...(clear ? { lp_hz: 0 } : {}) });
    }
  }
}

/** Room volume from width × depth × height, when all three are valid (upstream `room_volume`). */
export function roomVolume(dims: readonly number[] | null): number | null {
  if (!dims || dims.length !== 3 || !dims.every((d) => Number.isFinite(d) && d >= LIMITS.dimValid[0] && d <= LIMITS.dimValid[1])) return null;
  return dims[0]! * dims[1]! * dims[2]!;
}

/** The settings after one edit. Pure: `settings` is not changed. */
export function applySettingEdit(settings: CalibrationSettings, edit: SettingEdit): CalibrationSettings {
  const s: CalibrationSettings = { ...settings, xo: { ...settings.xo }, xo_rec: { ...settings.xo_rec } };
  const withDraft = (change: (draft: Draft) => void): CalibrationSettings => {
    const draft = draftOf(settings);
    change(draft);
    return { ...s, subs_manual: draft.subs, bundle_layouts: draft.layouts };
  };
  switch (edit.kind) {
    case "mode":
      return { ...s, mode: edit.mode };
    case "ultra_ms":
      return { ...s, ultra_ms: clamp(edit.ms, LIMITS.ultraMs) };
    case "rc_lo":
      return { ...s, rc_lo: clamp(edit.hz, LIMITS.rcLo) };
    case "rc_hi":
      return { ...s, rc_hi: edit.hz === null ? null : clamp(edit.hz, LIMITS.rcHi) };
    case "room_volume": {
      const volume = edit.m3 > 0 ? clamp(edit.m3, LIMITS.volume) : null;
      const made = roomVolume(s.room_dims_m);
      const keepDims = made !== null && volume !== null && Math.abs(made - volume) <= 0.5;
      return { ...s, room_volume_m3: volume, room_dims_m: keepDims ? s.room_dims_m : null };
    }
    case "room_dims": {
      const volume = roomVolume(edit.dims);
      if (volume === null) return { ...s, room_dims_m: null };
      return { ...s, room_dims_m: [...edit.dims], room_volume_m3: Math.min(Math.round(volume * 10) / 10, LIMITS.volume[1]) };
    }
    case "boost_max":
      return { ...s, boost_max_db: clamp(edit.db, LIMITS.boost) };
    case "xo":
      return { ...s, xo: { ...s.xo, [edit.group]: Math.round(clamp(edit.hz, LIMITS.xo)) } };
    case "xo_peq":
      return {
        ...s,
        xo_peq_on: edit.on ?? s.xo_peq_on,
        xo_peq_lo: edit.lo === undefined ? s.xo_peq_lo : clamp(edit.lo, LIMITS.xoPeq),
        xo_peq_hi: edit.hi === undefined ? s.xo_peq_hi : clamp(edit.hi, LIMITS.xoPeq),
      };
    case "infra":
      return withDraft((draft) => setInfra(settings, draft, edit.role, edit.hz, edit.type, edit.subs));
    case "infra_boost":
      return { ...s, infra_boost_max_db: clamp(edit.db, LIMITS.infraBoost) };
    case "sub_mode":
      return withDraft((draft) => {
        const [kind, bundle] = edit.mode.split(":") as ["auto" | "fixed" | "bundle", string | undefined];
        setSub(draft, edit.role, bundle ? { mode: kind, bundle } : { mode: kind });
      });
    case "sub_layout":
      return withDraft((draft) => setLayout(draft, edit.bundle, edit.layout));
    case "rear_trim":
      return withDraft((draft) => setLayout(draft, edit.bundle, undefined, edit.ms));
    case "sub":
      return withDraft((draft) => setSub(draft, edit.role, edit.patch));
    case "sub_band_type":
      return withDraft((draft) => {
        const hzKey = edit.band === "hp" ? "hp_hz" : "lp_hz";
        if (edit.type === "off") {
          setSub(draft, edit.role, { [hzKey]: 0 });
          return;
        }
        const current = (draft.subs[edit.role] ?? DEFAULT_SUB)[hzKey];
        const hz = current > 0 ? current : SUB_FILTER_DEFAULT_HZ[edit.band];
        setSub(draft, edit.role, edit.band === "hp" ? { hp_hz: hz, hp_type: edit.type } : { lp_hz: hz, lp_type: edit.type });
      });
    case "main_delay":
      return { ...s, sub_main_delay_ms: clamp(edit.ms, LIMITS.mainDelay) };
    case "mimo_on":
      return { ...s, mimo_on: edit.on };
    case "fsiso":
      return { ...s, fsiso: clamp(edit.hz, LIMITS.fsiso) };
    case "mimo_low":
      return { ...s, mimo_low: edit.hz === null ? null : clamp(edit.hz, LIMITS.mimoLow) };
    case "mimo_max":
      return { ...s, mimo_max_supports: Math.round(clamp(edit.count, LIMITS.mimoMax)) };
    case "mimo_fir":
      return { ...s, mimo_fir: edit.on };
  }
}

/**
 * Subs whose fixed LP sits below twice their HP: almost no pass band
 * (upstream `sub_filter_warnings`, shown before a run; service gap: no
 * method returns it for the current settings).
 */
export function subFilterWarnings(settings: CalibrationSettings, subs: readonly string[]): { role: string; hp_hz: number; lp_hz: number }[] {
  return subs
    .map((role) => ({ role, e: subOf(settings, role) }))
    .filter(({ e }) => e.hp_hz > 0 && e.lp_hz > 0 && e.lp_hz < 2 * e.hp_hz)
    .map(({ role, e }) => ({ role, hp_hz: e.hp_hz, lp_hz: e.lp_hz }));
}
