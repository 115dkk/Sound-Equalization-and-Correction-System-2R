// Views of the 보정 screen for the gallery and the tests: the sample 7.1.4
// room with two subs (src/ipc/fixtures.ts) in every state the specimens
// draw. Curves are synthetic but shaped like the service's: levels relative
// to the target, the RC band, LR4 crossovers, sub sums. Deterministic.
import { logGrid, response } from "../../gallery/data";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import type { CalibrationSummary } from "../../ipc/bindings/CalibrationSummary";
import type { ChannelResultLine } from "../../ipc/bindings/ChannelResultLine";
import type { ConfigResult } from "../../ipc/bindings/ConfigResult";
import type { MimoSummary } from "../../ipc/bindings/MimoSummary";
import type { ModeSpecResult } from "../../ipc/bindings/ModeSpecResult";
import type { Notice } from "../../ipc/bindings/Notice";
import type { ResultCurve } from "../../ipc/bindings/ResultCurve";
import type { ResultCurvesResult } from "../../ipc/bindings/ResultCurvesResult";
import type { ResultLinesResult } from "../../ipc/bindings/ResultLinesResult";
import type { SubResultLines } from "../../ipc/bindings/SubResultLines";
import type { TargetResult } from "../../ipc/bindings/TargetResult";
import * as fx from "../../ipc/fixtures";
import { ipcError, mockRcLines } from "../../ipc/mock";
import { targetAt } from "../../ui/plot/targetEdit";
import type { CalibrateView } from "./types";

/* ── Project ────────────────────────────────────────────────────────────── */

const GROUP_OF: Record<string, string> = { L: "프론트", R: "프론트", C: "센터", SW1: "서브", SW2: "서브", Ls: "서라운드", Rs: "서라운드", Lb: "백", Rb: "백", Tfl: "탑", Tfr: "탑", Tbl: "탑", Tbr: "탑" };
const GROUP_ORDER = ["프론트", "센터", "서브", "서라운드", "백", "와이드", "탑", "하이트", "기타"];

function config(roles: readonly string[], preset: string): ConfigResult {
  const speakers = roles.map((role, i) => ({ role, size: role.startsWith("SW") ? "sub" : roles.some((r) => r.startsWith("SW")) ? "small" : "large", bands: [{ band: role.startsWith("SW") ? "SUB" : "FULL", out: i + 1, pol: "+" }] }));
  const groups = GROUP_ORDER.map((key) => ({ id: key, key, speaker_indices: roles.map((r, i) => ((r.startsWith("SW") ? "서브" : GROUP_OF[r]) === key ? i : -1)).filter((i) => i >= 0) })).filter((g) => g.speaker_indices.length);
  const subs = roles.filter((r) => r.startsWith("SW")).length;
  return { speakers, preset, device: "MOTU M4", groups, summary: { speaker_count: roles.length, outputs: roles.map((_, i) => i + 1), max_channel: roles.length, duplicate_outputs: [], sub_count: subs } };
}

const sampleConfig = config(fx.SAMPLE_ROLES, "7.1.4");
const stereoConfig = config(["L", "R"], "2.0");

const HARMAN: [number, number][] = [
  [25, 6],
  [60, 5],
  [120, 1],
  [1000, 0],
  [16000, -3],
];
const sampleTarget: TargetResult = {
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
  sample_rate_policy: "reject",
  mode: "mixed",
  rc_lo: 20,
  rc_hi: null,
  rc_hi_basis: "strict",
  rc_hi_transition: 1,
  room_volume_m3: 62.6,
  room_dims_m: [5.2, 4.3, 2.8],
  mimo_on: false,
  fsiso: 150,
  mimo_low: null,
  mimo_max_supports: 0,
  mimo_fir: false,
  peq_analytic_derivatives: false,
  mimo_iir_analytic_derivatives: false,
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

const modeSpecs: Record<"zero" | "mixed" | "linear" | "ultra120", ModeSpecResult> = {
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

function splResult(role = "L", options: { mimo?: boolean } = {}): ResultCurvesResult {
  return curvesResult("spl", logGrid(5, 20000, 520), splCurves(role, options));
}

/** Before the first run: the position-1 measurement and the target; the auto upper limit is not known yet. */
function previewResult(role = "L"): ResultCurvesResult {
  const curves = splCurves(role).filter((c) => c.kind !== "after");
  return curvesResult("spl", logGrid(5, 20000, 520), curves, { rc_band_hz: [RC[0], null] });
}

function gdResult(role = "L"): ResultCurvesResult {
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

function xoResult(role = "L", hz = 80): ResultCurvesResult {
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

function baResult({ subsOnly = false }: { subsOnly?: boolean } = {}): ResultCurvesResult {
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

/* ── Scorecard figures ──────────────────────────────────────────────────── */

/**
 * Per main role: ±1 dB share (%), GD spread (ms) and late 20–150 Hz energy
 * (dB) at position 1, each before correction, after it and after MIMO. L, R
 * and C are the reference case: MIMO gives up a little flatness for shorter
 * ringing. The others are shaped like them.
 */
const FIGURES: Record<string, { within: [number, number, number]; gd: [number, number, number]; late: [number, number, number] }> = {
  L: { within: [38, 64, 54], gd: [88.4, 47.5, 41.1], late: [-23.9, -16.3, -23.1] },
  R: { within: [41, 65, 56], gd: [85.2, 47.3, 45.2], late: [-24.0, -17.0, -21.6] },
  C: { within: [36, 62, 55], gd: [90.6, 47.7, 45.1], late: [-17.0, -16.0, -18.7] },
  Ls: { within: [44, 71, 66], gd: [76.3, 39.8, 36.2], late: [-21.4, -18.2, -22.5] },
  Rs: { within: [42, 69, 63], gd: [79.1, 41.2, 37.9], late: [-20.8, -17.6, -21.9] },
  Lb: { within: [39, 67, 61], gd: [82.5, 43.6, 40.3], late: [-19.7, -16.9, -20.4] },
  Rb: { within: [40, 66, 60], gd: [81.0, 44.1, 41.0], late: [-20.2, -17.3, -20.8] },
  Tfl: { within: [47, 74, 70], gd: [68.4, 35.2, 33.6], late: [-22.6, -19.8, -22.1] },
  Tfr: { within: [46, 73, 69], gd: [70.2, 36.0, 34.1], late: [-22.1, -19.4, -21.7] },
  Tbl: { within: [43, 70, 67], gd: [72.9, 37.4, 35.8], late: [-21.0, -18.7, -20.9] },
  Tbr: { within: [45, 72, 68], gd: [71.5, 36.8, 35.0], late: [-21.3, -18.9, -21.2] },
};

const round1 = (value: number) => Math.round(value * 10) / 10;

/** The ChannelScore metrics with this role's ±1 dB share and GD spread (before, after); the rest as the IPC fixture has them. */
const scorecard: CalibrationSummary["scorecard"] = fx.scorecard.map((score) => {
  const figures = FIGURES[score.role];
  if (!figures) return score;
  return {
    ...score,
    before: { ...score.before, in1_20: figures.within[0], gd_20: figures.gd[0] },
    after: { ...score.after, in1_20: figures.within[1], gd_20: figures.gd[1] },
  };
});

/**
 * Away from position 1 the room rings differently and the correction and
 * MIMO help less: each figure a few dB apart, deterministic per role and
 * position.
 */
function lateAt(role: string, position: number): [number, number, number] {
  const [before, after, mimo] = FIGURES[role]!.late;
  if (position === 1) return [before, after, mimo];
  const k = fx.SAMPLE_MAINS.indexOf(role as (typeof fx.SAMPLE_MAINS)[number]);
  const shift = [0, 2.1, -1.4, 1.2][position - 1]! + (((k * 7 + position * 3) % 5) - 2) * 0.4;
  return [round1(before + shift), round1(after + shift * 0.6 + 0.9), round1(mimo + shift * 0.8 + 1.6)];
}

const lateEnergy = (positions: readonly number[]): CalibrationSummary["late_energy"] =>
  positions.flatMap((position) => fx.SAMPLE_MAINS.map((role) => {
    const [before_db, after_db] = lateAt(role, position);
    return { role, position, before_db, after_db };
  }));

/** The MIMO result with its scorecard and late energy at the three measured positions. */
const mimoSummary: MimoSummary = {
  ...fx.mimoSummary,
  input_count: fx.SAMPLE_MAINS.length,
  scorecard: scorecard.map((score) => ({
    role: score.role,
    after_mimo: { ...score.after, in1_20: FIGURES[score.role]!.within[2], gd_20: FIGURES[score.role]!.gd[2] },
  })),
  late_energy: [1, 2, 3].flatMap((position) => fx.SAMPLE_MAINS.map((role) => ({ role, position, after_mimo_db: lateAt(role, position)[2] }))),
};

/* ── Results and lines ──────────────────────────────────────────────────── */

const groupXo = (role: string) => XO[GROUP_OF[role] as keyof typeof XO] ?? null;

function channelLines(roles: readonly string[], mimo: boolean): ChannelResultLine[] {
  return roles.map((role, i) => {
    const score = scorecard.find((s) => s.role === role) ?? scorecard[i % scorecard.length]!;
    return {
      role,
      within_1db_pct: [round1(score.before.in1_20), round1(score.after.in1_20)],
      gd_spread_ms: [round1(score.before.gd_20), round1(score.after.gd_20)],
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

const baLines: SubResultLines = {
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
const baWarnLines: SubResultLines = {
  ...baLines,
  flatness_db: [7.4, 3.6],
  rows: [BA_ROWS[0]!, { ...BA_ROWS[1]!, mode: "fixed", gain_db: 13.4, delay_ms: 1.2, high_pass: [40, "LR24"], low_pass: [60, "LR24"] }],
  warnings: [
    { code: "sub_narrow_band", params: { role: "SW2", hp_hz: "40", lp_hz: "60" } },
    { code: "sub_high_gain", params: { role: "SW2", gain_db: "13.4" } },
  ],
};

const baDbaLines: SubResultLines = {
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

const summary = (stale = false, notes: readonly Notice[] = NOTES, positions: readonly number[] = [1, 2, 3]): CalibrationSummary => ({
  ...fx.calibrationSummary,
  stale,
  notes: [...notes],
  // The verdict sums up the channels below it (about their mean).
  verdict: { ...fx.calibrationSummary.verdict, within_1db_before_pct: 41.9, within_1db_after_pct: 68.6, low_band_time_spread_before_ms: 78.7, low_band_time_spread_after_ms: 41.5 },
  scorecard,
  late_energy: lateEnergy(positions),
});

function lines({ roles = fx.SAMPLE_MAINS, mimo = false, stale = false, ba = baLines as SubResultLines | null }: { roles?: readonly string[]; mimo?: boolean; stale?: boolean; ba?: SubResultLines | null } = {}): ResultLinesResult {
  return { stale, channels: channelLines(roles, mimo), total_delay_ms: 9.8, notes: summary().notes, rc: RC_NOTES, ba: ba ? { ...ba, stale } : null };
}

const noLines: ResultLinesResult = { stale: false, channels: [], total_delay_ms: null, notes: [], rc: [], ba: null };

/* ── Views ──────────────────────────────────────────────────────────────── */

const base: CalibrateView = {
  compute: { preference: "auto", status: fx.computeStatus, loading: false, saving: false, error: null, locked: false },
  results: { calibration: summary(), mimo: null, mimo_export: "off" },
  lines: lines(),
  curves: splResult(),
  settings: baseSettings,
  modeSpec: modeSpecs.mixed,
  target: sampleTarget,
  targetEdit: "idle", xoEdit: "idle",
  config: sampleConfig,
  positions: [1, 2, 3],
  measuredRoles: [...fx.SAMPLE_ROLES],
  subWarnings: [],
  curvesLoading: false,
  firBudget: 2097152,
  firTaps: 65536,
  outputRate: 48000,
  xoRecommendation: fx.xoRecommendation,
  running: null,
  runningMimo: false,
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

/** Pre-calculation branches: no measurements, singular/plural subs and each mode. */
export const planViews: Record<"unmeasured" | "oneSub" | "threeSubs" | "noSubs", CalibrateView> = {
  unmeasured: { ...emptyView, positions: [], measuredRoles: [], channel: null, curves: { ...previewResult(), curves: previewResult().curves.filter((c) => c.kind === "target") }, settings: { ...emptyView.settings, mode: "zero" }, modeSpec: modeSpecs.zero },
  oneSub: { ...emptyView, positions: emptyView.positions.slice(0, 1), config: config(["L", "R", "SW1"], "2.1"), measuredRoles: ["L", "R", "SW1"] },
  threeSubs: { ...emptyView, positions: emptyView.positions.slice(0, 1), config: config(["L", "R", "SW1", "SW2", "SW3"], "2.3"), measuredRoles: ["L", "R", "SW1", "SW2", "SW3"], settings: { ...emptyView.settings, mode: "linear" }, modeSpec: modeSpecs.linear },
  noSubs: { ...emptyView, config: stereoConfig, measuredRoles: ["L", "R"], settings: { ...emptyView.settings, mode: "ultra", ultra_ms: 120 }, modeSpec: modeSpecs.ultra120 },
};

export const computingView: CalibrateView = { ...emptyView, running: "calibrate" };

/* ── MIMO 함께 계산 (P142) ──────────────────────────────────────────────── */

/** Before the first run with MIMO on and three positions: 보정 계산 computes MIMO too. */
export const readyMimoView: CalibrateView = { ...emptyView, settings: { ...emptyView.settings, mimo_on: true } };

/** MIMO on but two measured positions: MIMO 함께 계산 is off and says why; 보정 계산 corrects only. */
export const readyMimoTwoView: CalibrateView = { ...readyMimoView, positions: [1, 2] };

/** The first run computing the correction and then MIMO in one job. */
export const computingMimoView: CalibrateView = { ...readyMimoView, running: "calibrate", runningMimo: true };

/**
 * The combined run failed in its MIMO part: the correction is current, MIMO
 * has no result, and the failure is the MIMO card's (the screen's view of a
 * `calibrate` error with `stage: "mimo"`).
 */
export const mimoFailedView: CalibrateView = {
  ...base,
  results: { ...base.results, mimo_export: "none" },
  settings: { ...baseSettings, mimo_on: true },
  failure: { job: "mimo", error: ipcError("engine_input", "invalid input: MIMO support design produced nonfinite filters") },
};

/** That failure as the job reports it (the tray's entry). */
export const mimoStageError = ipcError("engine_input", "invalid input: MIMO support design produced nonfinite filters", { stage: "mimo" });

export const splView: CalibrateView = base;

export const gdView: CalibrateView = { ...base, tab: "gd", curves: gdResult() };

export const xoView: CalibrateView = { ...base, tab: "xo", curves: xoResult() };

export const baView: CalibrateView = {
  ...base,
  tab: "ba",
  curves: baResult(),
  lines: lines({ ba: baWarnLines }),
  settings: { ...baseSettings, subs_manual: { SW2: { ...defaultSub(), mode: "fixed", gain_db: 13.4, delay_ms: 1.2, polarity: -1, hp_hz: 40, lp_hz: 60 } }, sub_main_delay_ms: 0.4 },
  results: { calibration: summary(false, [NOTES[0]!, ...SW2_FILTERS, ...NOTES.slice(1)]), mimo: null, mimo_export: "off" },
  // What sub_warnings answers for these settings: SW2's LP is below twice its HP.
  subWarnings: [{ code: "sub_narrow_band", params: { role: "SW2", hp_hz: "40", lp_hz: "60" } }],
};

export const staleView: CalibrateView = {
  ...base,
  results: { calibration: summary(true), mimo: null, mimo_export: "off" },
  lines: lines({ stale: true }),
  curves: { ...splResult(), stale: true },
  settings: { ...baseSettings, mode: "linear" },
  modeSpec: modeSpecs.linear,
  staleReason: "settings",
};

export const failedView: CalibrateView = {
  ...emptyView,
  failure: { job: "calibrate", error: ipcError("engine_input", "invalid input: position 2 SW2: empty or nonfinite IR", { reason: "invalid_measurement", position: "2", role: "SW2" }) },
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
  results: { calibration: summary(), mimo: mimoSummary, mimo_export: "included" },
  lines: lines({ mimo: true }),
  curves: splResult("L", { mimo: true }),
  settings: { ...baseSettings, mimo_on: true, mimo_max_supports: 4 },
};

/** MIMO computed on an earlier correction: the scorecard compares before and after only, and the graph draws no MIMO curve. */
export const mimoStaleView: CalibrateView = {
  ...mimoView,
  results: { calibration: summary(), mimo: { ...mimoSummary, stale: true }, mimo_export: "stale" },
  lines: lines(),
  curves: splResult(),
};

/** MIMO behind a stale correction: '다시 보정하기' computes both, and 'MIMO만 계산' is off (`mimoStaleView` has the correction current). */
export const mimoBehindCorrectionView: CalibrateView = {
  ...mimoStaleView,
  results: { calibration: summary(true), mimo: { ...mimoSummary, stale: true }, mimo_export: "stale" },
  lines: lines({ stale: true }),
  curves: { ...splResult(), stale: true },
  staleReason: "settings",
};

/** One measured position: the late bass there only, so no positions table (and no MIMO, which needs three). */
export const onePositionView: CalibrateView = {
  ...base,
  positions: [1],
  results: { calibration: summary(false, NOTES, [1]), mimo: null, mimo_export: "off" },
  lines: lines({ ba: { ...baLines, positions: 1 } }),
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

export const runningMimoView: CalibrateView = { ...mimoView, results: { calibration: summary(), mimo: { ...mimoSummary, stale: true }, mimo_export: "stale" }, running: "mimo" };

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

/* ── RC upper limit (ADR 0021) ──────────────────────────────────────────── */

const NINE_POSITIONS = [1, 2, 3, 4, 5, 6, 7, 8, 9];

/**
 * The range card on one upper-limit basis, its RC lines as the mock service's
 * calibration leaves them for these settings (`mockRcLines`). `ranOn` names
 * the settings of the run when they differ from the ones shown (an edit since).
 */
function rangeView(patch: Partial<CalibrationSettings>, positions: number[] = [1, 2, 3], ranOn: Partial<CalibrationSettings> = patch): CalibrateView {
  const settings = { ...baseSettings, ...patch };
  return { ...base, positions, settings, lines: { ...base.lines, rc: mockRcLines({ ...baseSettings, ...ranOn }, positions.length) } };
}

export const rangeViews = {
  strict: rangeView({}),
  strictTimes2: rangeView({ rc_hi_transition: 2 }),
  strictTimes4: rangeView({ rc_hi_transition: 4 }),
  lenient: rangeView({ rc_hi_basis: "lenient" }),
  lenientTimes2: rangeView({ rc_hi_basis: "lenient", rc_hi_transition: 2 }),
  agreement: rangeView({ rc_hi_basis: "agreement" }, NINE_POSITIONS),
  /** Two measured positions: agreement is off, with the reason. */
  agreementUnavailable: rangeView({}, [1, 2]),
  /** Agreement chosen earlier, a position cleared since: still chosen, with what to do. */
  agreementShort: rangeView({ rc_hi_basis: "agreement" }, [1, 2], {}),
  manual: rangeView({ rc_hi: 300, rc_hi_basis: "lenient" }),
  noVolume: rangeView({ room_volume_m3: null, room_dims_m: null }),
} satisfies Record<string, CalibrateView>;

/**
 * The RC line for every kind of evidence the service's rc_limit note carries
 * (secs-service `scalar_notice`), and a note from before the bases.
 */
export const rcLimitNotes: Notice[] = [
  { code: "rc_limit", params: { hz: "154", auto: "true", schroeder_hz: "154", basis: "strict", factor: "1" } },
  { code: "rc_limit", params: { hz: "308", auto: "true", schroeder_hz: "154", basis: "strict", factor: "2" } },
  { code: "rc_limit", params: { hz: "1241.68", auto: "true", schroeder_hz: "310.42", basis: "strict", factor: "4" } },
  { code: "rc_limit", params: { hz: "200", auto: "true", schroeder_hz: "154", basis: "lenient", factor: "1", band_hz: "200", band_t60_s: "0.5" } },
  { code: "rc_limit", params: { hz: "400", auto: "true", schroeder_hz: "154", basis: "lenient", factor: "2", band_hz: "200", band_t60_s: "0.5" } },
  { code: "rc_limit", params: { hz: "250", auto: "true", schroeder_hz: "250", basis: "lenient", factor: "1", band_hz: "160", band_t60_s: "0.31" } },
  { code: "rc_limit", params: { hz: "500", auto: "true", schroeder_hz: "250", basis: "lenient", factor: "2" } },
  { code: "rc_limit", params: { hz: "287", auto: "true", schroeder_hz: "154", basis: "agreement", factor: "1", positions: "9" } },
  { code: "rc_limit", params: { hz: "300", auto: "false", basis: "strict", factor: "1" } },
  { code: "rc_limit", params: { hz: "500", auto: "true", basis: "strict", factor: "2" } },
  { code: "rc_no_room_volume", params: {} },
  { code: "rc_limit", params: { hz: "310.42", auto: "true", schroeder_hz: "310.42" } },
];

function defaultSub() {
  return { mode: "auto", bundle: "A", gain_db: 0, delay_ms: 0, polarity: 1, allpass: [] as [number, number][], mimo_support: true, hp_hz: 0, lp_hz: 0, peq: [] as [number, number, number][], hp_type: "LR24", lp_type: "LR24", dba_row: "front" };
}

const firMimoSummary: MimoSummary = {
  ...mimoSummary, kind: "fir", support_count: 2, fir_taps_total: 16000,
  input_count: 1,
};

/**
 * Exporting to a miniDSP Flex, which runs at 96 kHz (ADR 0017). The mixed
 * mode's 2,048 taps become 4,096, past its 2,048 per channel; two FIR
 * supports of 3,072 taps at 48 kHz, inside its 4,096 budget at 48 kHz,
 * become 6,144 and go past it.
 */
export const minidspView: CalibrateView = {
  ...mimoView,
  settings: { ...mimoView.settings, mimo_fir: true },
  results: { ...mimoView.results, mimo: { ...fx.mimoSummary, kind: "fir", support_count: 2, fir_taps_total: 3072, input_count: 1 } },
  firBudget: 4096,
  firTaps: 2048,
  outputRate: 96000,
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
  results: { ...mimoView.results, mimo: { ...mimoSummary, compute: {
    ...fx.cpuComputeReport, requested: "gpu", reason: "iir_reference",
  } } },
};
