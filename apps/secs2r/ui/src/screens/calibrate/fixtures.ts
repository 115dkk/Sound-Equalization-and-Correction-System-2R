// Views of the 보정 screen for the gallery and the tests: the sample 7.1.4
// room with two subs (src/ipc/fixtures.ts) in every state the specimens
// draw. Curves are synthetic but shaped like the service's: levels relative
// to the target, the RC band, LR4 crossovers, sub sums. Deterministic.
import { logGrid, response } from "../../gallery/data";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import type { CalibrationSummary } from "../../ipc/bindings/CalibrationSummary";
import type { ChannelResultLine } from "../../ipc/bindings/ChannelResultLine";
import type { ConfigResult } from "../../ipc/bindings/ConfigResult";
import type { ModeSpecResult } from "../../ipc/bindings/ModeSpecResult";
import type { Notice } from "../../ipc/bindings/Notice";
import type { ResultCurve } from "../../ipc/bindings/ResultCurve";
import type { ResultCurvesResult } from "../../ipc/bindings/ResultCurvesResult";
import type { ResultLinesResult } from "../../ipc/bindings/ResultLinesResult";
import type { SubResultLines } from "../../ipc/bindings/SubResultLines";
import type { TargetResult } from "../../ipc/bindings/TargetResult";
import * as fx from "../../ipc/fixtures";
import { ipcError } from "../../ipc/mock";
import { targetAt } from "../../ui/plot/targetEdit";
import type { CalibrateView } from "./types";

/* ── Project ────────────────────────────────────────────────────────────── */

const GROUP_OF: Record<string, string> = { L: "프론트", R: "프론트", C: "센터", SW1: "서브", SW2: "서브", Ls: "서라운드", Rs: "서라운드", Lb: "백", Rb: "백", Tfl: "탑", Tfr: "탑", Tbl: "탑", Tbr: "탑" };
const GROUP_ORDER = ["프론트", "센터", "서브", "서라운드", "백", "와이드", "탑", "하이트", "기타"];

function config(roles: readonly string[], preset: string): ConfigResult {
  const speakers = roles.map((role, i) => ({ role, size: role.startsWith("SW") ? "sub" : roles.some((r) => r.startsWith("SW")) ? "small" : "large", bands: [{ band: role.startsWith("SW") ? "SUB" : "FULL", out: i + 1, pol: "+" }] }));
  const groups = GROUP_ORDER.map((key) => ({ id: key, key, speaker_indices: roles.map((r, i) => (GROUP_OF[r] === key ? i : -1)).filter((i) => i >= 0) })).filter((g) => g.speaker_indices.length);
  const subs = roles.filter((r) => r.startsWith("SW")).length;
  return { speakers, preset, device: "MOTU M4", groups, summary: { speaker_count: roles.length, outputs: roles.map((_, i) => i + 1), max_channel: roles.length, duplicate_outputs: [], sub_count: subs } };
}

export const sampleConfig = config(fx.SAMPLE_ROLES, "7.1.4");
export const stereoConfig = config(["L", "R"], "2.0");

const HARMAN: [number, number][] = [
  [25, 6],
  [60, 5],
  [120, 1],
  [1000, 0],
  [16000, -3],
];
export const sampleTarget: TargetResult = {
  points: HARMAN,
  preset: "harman",
  presets: [
    [
      "flat",
      [
        [25, 0],
        [200, 0],
        [1000, 0],
        [16000, 0],
      ],
    ],
    ["harman", HARMAN],
    [
      "cinema",
      [
        [25, 4],
        [80, 3],
        [200, 0],
        [1000, -1],
        [16000, -5],
      ],
    ],
  ],
};
const targetPoints = HARMAN.map(([hz, db]) => ({ hz, db }));

const XO = { 프론트: 80, 센터: 100, 서라운드: 80, 백: 80, 탑: 100 };

export const baseSettings: CalibrationSettings = {
  mode: "mixed",
  rc_lo: 20,
  rc_hi: null,
  room_volume_m3: 62.6,
  room_dims_m: [5.2, 4.3, 2.8],
  mimo_on: false,
  fsiso: 150,
  mimo_low: null,
  mimo_max_supports: 0,
  mimo_fir: false,
  xo: XO,
  xo_rec: XO,
  ultra_ms: 200,
  subs_manual: {},
  sub_main_delay_ms: 0,
  bundle_layouts: {},
  infra_boost_max_db: 24,
  xo_peq_on: true,
  boost_max_db: 10,
  xo_peq_lo: 0,
  xo_peq_hi: 0,
};

export const modeSpecs: Record<"zero" | "mixed" | "linear" | "ultra120", ModeSpecResult> = {
  zero: { rc_filter: "iir", fir_taps: 0, peq_bands: 32, mimo_filter: "iir", lift_ms: 0, phase_frac: 6, max_latency_ms: 0 },
  mixed: { rc_filter: "iir_fir", fir_taps: 2048, peq_bands: null, mimo_filter: "iir", lift_ms: 8, phase_frac: 6, max_latency_ms: 10 },
  linear: { rc_filter: "iir_fir", fir_taps: 2048, peq_bands: null, mimo_filter: "iir", lift_ms: 28, phase_frac: 24, max_latency_ms: 30 },
  ultra120: { rc_filter: "iir_fir", fir_taps: 8192, peq_bands: null, mimo_filter: "iir", lift_ms: 118, phase_frac: 24, max_latency_ms: 120 },
};

/* ── Curves ─────────────────────────────────────────────────────────────── */

const RC: [number, number] = [20, 310];
const arr = (values: ArrayLike<number>) => Array.from(values, (v) => Math.round(v * 100) / 100);
const lr4 = (f: number, fc: number, high: boolean) => {
  const x = (f / fc) ** 4;
  return 20 * Math.log10(high ? x / (1 + x) : 1 / (1 + x));
};
/** 0 outside [lo, hi], 1 inside, a smooth step over a third of an octave at each edge. */
const inside = (f: number, lo: number, hi: number) => {
  const edge = (x: number) => Math.min(1, Math.max(0, x * 3 + 0.5));
  return Math.min(edge(Math.log2(f / lo)), edge(Math.log2(hi / f)));
};
/** The roll-offs gallery/data.ts gives a sub (low-pass near 95 Hz, high-pass near 18 Hz). */
const subShape = (f: number) => -24 * Math.log2(Math.max(1, f / 95)) - 18 * Math.log2(Math.max(1, 18 / f));
const sumDb = (...dbs: number[]) => 20 * Math.log10(dbs.reduce((s, db) => s + 10 ** (db / 20), 0));

function splCurves(role: string, { mimo = false, rc = RC }: { mimo?: boolean; rc?: [number, number] } = {}): ResultCurve[] {
  const grid = logGrid(5, 20000, 520);
  const dev = response(grid, role, 1, false);
  const before = new Float64Array(grid.length);
  const after = new Float64Array(grid.length);
  const withMimo = new Float64Array(grid.length);
  const target = new Float64Array(grid.length);
  grid.forEach((f, i) => {
    const tdb = targetAt(targetPoints, f);
    const d = dev[i]!;
    target[i] = tdb;
    before[i] = tdb + d;
    const w = inside(f, rc[0], rc[1]);
    after[i] = tdb + d * (1 - w) + w * d * 0.18 + (f > rc[1] ? -d * 0.25 : 0);
    const m = inside(f, 18, 150);
    withMimo[i] = after[i]! * (1 - m) + m * (tdb + d * 0.07);
  });
  const curves: ResultCurve[] = [
    { kind: "before", role: null, values: arr(before) },
    { kind: "after", role: null, values: arr(after) },
    { kind: "target", role: null, values: arr(target) },
  ];
  if (mimo) curves.push({ kind: "mimo", role: null, values: arr(withMimo) });
  return curves;
}

function curvesResult(tab: ResultCurvesResult["tab"], grid: Float64Array, curves: ResultCurve[], extra: Partial<ResultCurvesResult> = {}): ResultCurvesResult {
  const axes = {
    spl: { frequency_hz: [5, 20000] as [number, number], value: [-30, 15] as [number, number], unit: "db" },
    gd: { frequency_hz: [5, 5000] as [number, number], value: [-10, 90] as [number, number], unit: "ms" },
    xo: { frequency_hz: [5, 500] as [number, number], value: [-29, 13] as [number, number], unit: "db" },
    ba: { frequency_hz: [5, 300] as [number, number], value: [-31, 15] as [number, number], unit: "db" },
  }[tab];
  return { tab, grid: arr(grid), curves, axes, stale: false, subs_only: false, rc_band_hz: tab === "spl" || tab === "gd" ? [RC[0], RC[1]] : null, xo_hz: null, ...extra };
}

export function splResult(role = "L", options: { mimo?: boolean } = {}): ResultCurvesResult {
  return curvesResult("spl", logGrid(5, 20000, 520), splCurves(role, options));
}

/** Before the first run: the position-1 measurement and the target; the auto upper limit is not known yet. */
export function previewResult(role = "L"): ResultCurvesResult {
  const curves = splCurves(role).filter((c) => c.kind !== "after");
  return curvesResult("spl", logGrid(5, 20000, 520), curves, { rc_band_hz: [RC[0], null] });
}

export function gdResult(role = "L"): ResultCurvesResult {
  const grid = logGrid(5, 5000, 420);
  const seed = [...role].reduce((h, c) => h + c.charCodeAt(0), 0);
  const modes = [34, 52, 78, 115, 170].map((f, k) => ({ f: f * (0.95 + ((seed * (k + 3)) % 10) / 100), h: [42, 28, 20, 12, 7][k]! }));
  const gd = (f: number, scale: number) =>
    3 + Math.min(70, 55 * (18 / f) ** 1.6) * (scale > 0.5 ? 1 : 0.55) + modes.reduce((s, m) => s + scale * m.h * Math.exp(-(((Math.log10(f) - Math.log10(m.f)) / 0.035) ** 2)), 0) + 0.8 * Math.sin(Math.log10(f) * 19 + seed);
  const before = grid.map((f) => gd(f, 1));
  const after = grid.map((f) => gd(f, f < RC[1] ? 0.22 : 0.9));
  return curvesResult("gd", grid, [
    { kind: "before", role: null, values: arr(before) },
    { kind: "after", role: null, values: arr(after) },
  ]);
}

export function xoResult(role = "L", hz = 80): ResultCurvesResult {
  const grid = logGrid(5, 500, 320);
  const main = response(grid, role, 1, false);
  const sub = response(grid, "SW1", 1, true);
  const t = grid.map((f) => targetAt(targetPoints, f));
  const m = grid.map((f, i) => t[i]! + main[i]! * 0.25 + lr4(f, hz, true));
  const s = grid.map((f, i) => t[i]! + (sub[i]! - subShape(f)) * 0.15 + lr4(f, hz, false) - 18 * Math.log2(Math.max(1, 16 / f)));
  const sum = grid.map((_, i) => sumDb(m[i]!, s[i]!));
  return curvesResult(
    "xo",
    grid,
    [
      { kind: "target", role: null, values: arr(t) },
      { kind: "main", role: null, values: arr(m) },
      { kind: "sub", role: null, values: arr(s) },
      { kind: "sum", role: null, values: arr(sum) },
    ],
    { xo_hz: hz },
  );
}

export function baResult({ subsOnly = false }: { subsOnly?: boolean } = {}): ResultCurvesResult {
  const grid = logGrid(5, 300, 280);
  const t = grid.map((f) => targetAt(targetPoints, f));
  const shape = subShape;
  const subs = fx.SAMPLE_SUBS.map((role, k) => {
    const dev = response(grid, role, 1, true);
    const before = grid.map((f, i) => t[i]! - 6 + (dev[i]! - shape(f) - (k ? 1.5 : -1)) * 0.9 + shape(f));
    const after = grid.map((f, i) => t[i]! - 6 + (dev[i]! - shape(f)) * 0.3 + shape(f));
    return { role, before, after };
  });
  const sumBefore = grid.map((_, i) => sumDb(subs[0]!.before[i]! - 1.5, subs[1]!.before[i]! + 2.5 * Math.sin(i / 9)));
  const sumAfter = grid.map((_, i) => sumDb(subs[0]!.after[i]!, subs[1]!.after[i]!));
  const curves: ResultCurve[] = [{ kind: "target", role: null, values: arr(t) }];
  for (const s of subs) curves.push({ kind: "before", role: s.role, values: arr(s.before) }, { kind: "after", role: s.role, values: arr(s.after) });
  curves.push({ kind: "sum_before", role: null, values: arr(sumBefore) }, { kind: "sum_after", role: null, values: arr(sumAfter) });
  return curvesResult("ba", grid, curves, { subs_only: subsOnly });
}

/* ── Results and lines ──────────────────────────────────────────────────── */

const groupXo = (role: string) => XO[GROUP_OF[role] as keyof typeof XO] ?? null;

function channelLines(roles: readonly string[], mimo: boolean): ChannelResultLine[] {
  return roles.map((role, i) => {
    const score = fx.scorecard.find((s) => s.role === role) ?? fx.scorecard[i % fx.scorecard.length]!;
    return {
      role,
      within_1db_pct: [Math.round(score.before.in1_20 * 10) / 10, Math.round(score.after.in1_20 * 10) / 10],
      gd_spread_ms: [Math.round(score.before.gd_20 * 10) / 10, Math.round(score.after.gd_20 * 10) / 10],
      peq_count: 3 + ((i * 5) % 6),
      fir_taps: 2048,
      xo_hz: groupXo(role),
      mimo_support_count: mimo ? 1 + ((i * 3) % 4) : null,
    };
  });
}

const RC_NOTES = [
  { code: "rc_limit", params: { hz: "310.42", auto: "true", schroeder_hz: "310.42" } },
  { code: "rc_room_volume", params: { volume_m3: "62.6" } },
];

const BA_ROWS: SubResultLines["rows"] = [
  { role: "SW1", mode: "auto", gain_db: 1.5, delay_ms: 3.42, polarity: 1, allpass: [[62, 0.8]], high_pass: [0, "LR24"], low_pass: [0, "LR24"], peq: [] },
  { role: "SW2", mode: "auto", gain_db: -2, delay_ms: 0, polarity: -1, allpass: [], high_pass: [0, "LR24"], low_pass: [0, "LR24"], peq: [] },
];

export const baLines: SubResultLines = {
  stale: false,
  subs_only: false,
  band_hz: [20, 120],
  flatness_db: [7.4, 2.1],
  seat_spread_db: [5.8, 3.2],
  positions: 3,
  rows: BA_ROWS,
  layouts: [],
  warnings: [],
};

/** BA with a fixed SW2 whose filters leave almost no pass band, and the gain that tries to fill it. */
export const baWarnLines: SubResultLines = {
  ...baLines,
  flatness_db: [7.4, 3.6],
  rows: [BA_ROWS[0]!, { ...BA_ROWS[1]!, mode: "fixed", gain_db: 13.4, delay_ms: 1.2, high_pass: [40, "LR24"], low_pass: [60, "LR24"] }],
  warnings: [
    { code: "sub_narrow_band", params: { role: "SW2", hp_hz: "40", lp_hz: "60" } },
    { code: "sub_high_gain", params: { role: "SW2", gain_db: "13.4" } },
  ],
};

export const baDbaLines: SubResultLines = {
  ...baLines,
  subs_only: true,
  flatness_db: [7.4, 1.8],
  seat_spread_db: [5.8, 2.4],
  rows: [
    { ...BA_ROWS[0]!, mode: "bundle:A:dba-front", gain_db: 0, delay_ms: 0, allpass: [], peq: [[42, 4.5, -3.5]] },
    { ...BA_ROWS[1]!, mode: "bundle:A:dba-rear", gain_db: -1.2, delay_ms: 12.25, polarity: -1, peq: [[42, 4.5, -3.5]] },
  ],
  layouts: [{ bundle: "A", kind: "dba", members: ["SW1", "SW2"], front: ["SW1"], rear: ["SW2"], rear_ms: 12.25, auto_ms: 11.75, trim_ms: 0.5, peq: [[42, 4.5, -3.5]] }],
};

/** The calculation notes as secs-service sends them: one record per role, filter or room mode, scalar params. */
const NOTES: Notice[] = [
  { code: "lift_capped", params: { from_ms: "8", taps: "2048", lift_ms: "7.5" } },
  { code: "rc_limit", params: { hz: "310.42", auto: "true", schroeder_hz: "310.42" } },
  ...fx.SAMPLE_MAINS.map((role) => ({ code: "phase_strength", params: { role, strength: role.startsWith("T") ? "0.5" : "0.9" } })),
  // The engine's phase_band is the same fraction as the strength (rc.rs), named `hz`.
  ...fx.SAMPLE_MAINS.map((role) => ({ code: "phase_band", params: { role, enabled: "true", hz: role.startsWith("T") ? "0.5" : "0.9" } })),
  { code: "long_mode_boost", params: { cap_db: "4", hz: "34.2", t60_s: "0.61" } },
  { code: "long_mode_boost", params: { cap_db: "4", hz: "51.8", t60_s: "0.44" } },
  { code: "ba_cost", params: { value: "0.8417" } },
  { code: "ba_bus_pass", params: { first: "0.8417", second: "0.7952", kept_first: "false" } },
];

/** SW2's fixed filters (the BA view's fixed sub), as the engine notes them first. */
const SW2_FILTERS: Notice[] = [
  { code: "sub_filters", params: { role: "SW2", hp_hz: "40", hp_type: "LR24", lp_hz: "60", lp_type: "LR24", peq_count: "1" } },
  { code: "sub_filter_peq", params: { role: "SW2", index: "0", hz: "38", q: "5", gain_db: "-4" } },
];

const summary = (stale = false, notes: readonly Notice[] = NOTES): CalibrationSummary => ({
  ...fx.calibrationSummary,
  stale,
  notes: [...notes],
});

function lines({ roles = fx.SAMPLE_MAINS, mimo = false, stale = false, ba = baLines as SubResultLines | null }: { roles?: readonly string[]; mimo?: boolean; stale?: boolean; ba?: SubResultLines | null } = {}): ResultLinesResult {
  return { stale, channels: channelLines(roles, mimo), total_delay_ms: 9.8, notes: summary().notes, rc: RC_NOTES, ba: ba ? { ...ba, stale } : null };
}

const noLines: ResultLinesResult = { stale: false, channels: [], total_delay_ms: null, notes: [], rc: [], ba: null };

/* ── Views ──────────────────────────────────────────────────────────────── */

const base: CalibrateView = {
  compute: { preference: "auto", status: fx.computeStatus, loading: false, saving: false, error: null, locked: false },
  results: { calibration: summary(), mimo: null },
  lines: lines(),
  curves: splResult(),
  settings: baseSettings,
  modeSpec: modeSpecs.mixed,
  target: sampleTarget,
  config: sampleConfig,
  positions: [1, 2, 3],
  measuredRoles: [...fx.SAMPLE_ROLES],
  subWarnings: [],
  curvesLoading: false,
  firBudget: 2097152,
  xoRecommendation: fx.xoRecommendation,
  running: null,
  measuring: false,
  failure: null,
  staleReason: null,
  resultMode: { mode: "mixed", ultra_ms: 200 },
  tab: "spl",
  channel: "L",
  xoGroup: "프론트",
};

/** Before the first run: the plan, the position-1 preview and the target. */
export const emptyView: CalibrateView = {
  ...base,
  results: fx.noResults,
  lines: noLines,
  curves: previewResult(),
  settings: { ...baseSettings, xo: {}, xo_rec: {} },
  xoRecommendation: null,
  resultMode: null,
};

export const computingView: CalibrateView = { ...emptyView, running: "calibrate" };

export const splView: CalibrateView = base;

export const gdView: CalibrateView = { ...base, tab: "gd", curves: gdResult() };

export const xoView: CalibrateView = { ...base, tab: "xo", curves: xoResult() };

export const baView: CalibrateView = {
  ...base,
  tab: "ba",
  curves: baResult(),
  lines: lines({ ba: baWarnLines }),
  settings: { ...baseSettings, subs_manual: { SW2: { ...defaultSub(), mode: "fixed", gain_db: 13.4, delay_ms: 1.2, polarity: -1, hp_hz: 40, lp_hz: 60 } }, sub_main_delay_ms: 0.4 },
  results: { calibration: summary(false, [NOTES[0]!, ...SW2_FILTERS, ...NOTES.slice(1)]), mimo: null },
  // What sub_warnings answers for these settings: SW2's LP is below twice its HP.
  subWarnings: [{ code: "sub_narrow_band", params: { role: "SW2", hp_hz: "40", lp_hz: "60" } }],
};

export const staleView: CalibrateView = {
  ...base,
  results: { calibration: summary(true), mimo: null },
  lines: lines({ stale: true }),
  curves: { ...splResult(), stale: true },
  settings: { ...baseSettings, mode: "linear" },
  modeSpec: modeSpecs.linear,
  staleReason: "settings",
};

export const failedView: CalibrateView = {
  ...emptyView,
  failure: { job: "calibrate", error: ipcError("engine_input", "position 2 has no IR for SW2 (expected 13 roles, found 12)") },
};

export const subsOnlyView: CalibrateView = {
  ...base,
  tab: "ba",
  curves: baResult({ subsOnly: true }),
  lines: lines({ ba: baDbaLines }),
  settings: {
    ...baseSettings,
    subs_manual: { SW1: { ...defaultSub(), mode: "bundle", bundle: "A", dba_row: "front" }, SW2: { ...defaultSub(), mode: "bundle", bundle: "A", dba_row: "rear" } },
    bundle_layouts: { A: { kind: "dba", rear_trim_ms: 0.5 } },
  },
};

export const mimoView: CalibrateView = {
  ...base,
  results: { calibration: summary(), mimo: fx.mimoSummary },
  lines: lines({ mimo: true }),
  curves: splResult("L", { mimo: true }),
  settings: { ...baseSettings, mimo_on: true, mimo_max_supports: 4 },
};

export const noSubsView: CalibrateView = {
  ...base,
  config: stereoConfig,
  measuredRoles: ["L", "R"],
  lines: lines({ roles: ["L", "R"], ba: null }),
  settings: { ...baseSettings, xo: {}, xo_rec: {} },
  xoRecommendation: null,
  tab: "xo",
  curves: curvesResult("xo", logGrid(5, 500, 320), []),
  xoGroup: null,
};

export const ultraView: CalibrateView = {
  ...base,
  settings: { ...baseSettings, mode: "ultra", ultra_ms: 120 },
  modeSpec: modeSpecs.ultra120,
  resultMode: { mode: "ultra", ultra_ms: 120 },
};

export const runningMimoView: CalibrateView = { ...mimoView, results: { calibration: summary(), mimo: { ...fx.mimoSummary, stale: true } }, running: "mimo" };

/** R was chosen and its curves are on their way: L's curves are still the ones received. */
export const loadingView: CalibrateView = { ...base, channel: "R", curvesLoading: true };

/** Positions 2 and 3 measured, position 1 not: no channel to preview, only the target. */
export const noPosition1View: CalibrateView = {
  ...emptyView,
  positions: [2, 3],
  measuredRoles: [],
  channel: null,
  curves: { ...previewResult(), curves: previewResult().curves.filter((c) => c.kind === "target") },
};

/** The first tab's curves are on their way and none are here yet. */
export const loadingEmptyView: CalibrateView = { ...base, curves: null, curvesLoading: true };

function defaultSub() {
  return { mode: "auto", bundle: "A", gain_db: 0, delay_ms: 0, polarity: 1, allpass: [] as [number, number][], mimo_support: true, hp_hz: 0, lp_hz: 0, peq: [] as [number, number, number][], hp_type: "LR24", lp_type: "LR24", dba_row: "front" };
}

const firMimoSummary: import("../../ipc/bindings/MimoSummary").MimoSummary = {
  ...fx.mimoSummary, kind: "fir", support_count: 2, fir_taps_total: 16000,
  input_count: 1,
};
export const gpuView: CalibrateView = {
  ...mimoView, settings: { ...mimoView.settings, mimo_fir: true }, compute: { ...base.compute, status: fx.gpuComputeStatus },
  results: { ...mimoView.results, mimo: { ...firMimoSummary, compute: {
    ...fx.cpuComputeReport, backend: "cuda", device: fx.gpuDevice.name, stage: "ideal_fir_sweep", accelerated_systems: 11, cpu_systems: 0, reason: "recommended",
  } } },
};
export const cpuSelectedView: CalibrateView = { ...gpuView, compute: { ...gpuView.compute, preference: "cpu", status: {
  ...fx.gpuComputeStatus, preference: "cpu", selected: "cpu", reason: "user_cpu",
} } };
export const runtimeMissingView: CalibrateView = { ...mimoView, compute: { ...base.compute, status: { ...fx.computeStatus, reason: "runtime_unavailable" } } };
export const computeLoadingView: CalibrateView = { ...mimoView, compute: { ...base.compute, status: null, loading: true } };
export const computeErrorView: CalibrateView = { ...mimoView, compute: { ...base.compute, status: null, error: ipcError("internal", "device probe failed") } };
export const gpuFallbackView: CalibrateView = { ...gpuView, results: { ...gpuView.results, mimo: { ...firMimoSummary, compute: {
  ...fx.cpuComputeReport, reason: "device_failure", fallbacks: 1,
} } } };

export const gpuSelectedView: CalibrateView = { ...gpuView, compute: { ...gpuView.compute, preference: "gpu", status: {
  ...fx.gpuComputeStatus, preference: "gpu", reason: "user_gpu",
} } };

/** A saved explicit GPU choice survives moving to a host without its runtime. */
export const requestedGpuUnavailableView: CalibrateView = { ...runtimeMissingView, compute: {
  ...runtimeMissingView.compute, preference: "gpu", status: { ...fx.computeStatus, preference: "gpu", reason: "runtime_unavailable" },
} };

/** The saved GPU preference does not change the CPU reference used for IIR. */
export const gpuIirReferenceView: CalibrateView = {
  ...mimoView, compute: gpuSelectedView.compute,
  results: { ...mimoView.results, mimo: { ...fx.mimoSummary, compute: {
    ...fx.cpuComputeReport, requested: "gpu", reason: "iir_reference",
  } } },
};
