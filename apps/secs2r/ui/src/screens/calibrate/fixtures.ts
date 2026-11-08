// Views of the 보정 screen for the gallery and the tests, in every state the
// specimens draw. The results come from the service's recorded calibrations
// of the sample room (src/ipc/recorded, ADR 0052): the 7.1.4 mains and three
// subs at nine positions, a new project's settings, and one edit per
// recorded variant. Only the curves are drawn here, synthetic but shaped like
// the service's: levels relative to the target, the RC band, LR4 crossovers,
// sub sums. A few views carry what no record has yet (a fixed or bundled
// sub, a reversed main, an unheld sub bus, MIMO); each says so.
import { logGrid, response } from "../../gallery/data";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import type { CalibrationSummary } from "../../ipc/bindings/CalibrationSummary";
import type { ConfigResult } from "../../ipc/bindings/ConfigResult";
import type { MimoSummary } from "../../ipc/bindings/MimoSummary";
import type { ModeSpecResult } from "../../ipc/bindings/ModeSpecResult";
import type { Note } from "../../ipc/bindings/Note";
import type { Notice } from "../../ipc/bindings/Notice";
import type { RcBand } from "../../ipc/bindings/RcBand";
import type { ResultCurve } from "../../ipc/bindings/ResultCurve";
import type { ResultCurvesResult } from "../../ipc/bindings/ResultCurvesResult";
import type { ResultLinesResult } from "../../ipc/bindings/ResultLinesResult";
import type { SubResultLines } from "../../ipc/bindings/SubResultLines";
import type { TargetResult } from "../../ipc/bindings/TargetResult";
import * as fx from "../../ipc/fixtures";
import { ipcError } from "../../ipc/mock";
import { recordKey, recordedCalibration } from "../../ipc/recorded";
import { bandLayout } from "../../ui/plot/bands";
import { targetAt } from "../../ui/plot/targetEdit";
import type { RangeRejection } from "./settingsView";
import type { CalibrateView } from "./types";

/* ── The recorded calibration ───────────────────────────────────────────── */

/** The service's recorded calibration of the sample room on a new project's settings with this patch; the defaults' when not recorded. */
const recordedRun = (patch: Partial<CalibrationSettings>) => recordedCalibration(recordKey({ ...fx.newProjectSettings, ...patch }));
const RECORD = recordedRun({});
const RECORDED_SUMMARY = RECORD.results.calibration!;
const MAINS = RECORDED_SUMMARY.main_roles;
const SUBS = RECORDED_SUMMARY.sub_roles;
/** The sample room's roles in configuration order: the front three, the subs, then the other mains. */
const SAMPLE_ROLES = [...MAINS.slice(0, 3), ...SUBS, ...MAINS.slice(3)];
const NINE_POSITIONS = [...new Set(RECORDED_SUMMARY.late_energy.map((late) => late.position))];
/** The RC band the recorded calibration used (its rc_limit note), which the curves draw. */
const RC: [number, number] = [fx.newProjectSettings.rc_lo, RECORDED_SUMMARY.notes.find((note) => note.code === "rc_limit")!.hz];

/* ── Project ────────────────────────────────────────────────────────────── */

const GROUP_OF: Record<string, string> = { L: "프론트", R: "프론트", C: "센터", Ls: "서라운드", Rs: "서라운드", Lb: "백", Rb: "백", Tfl: "탑", Tfr: "탑", Tbl: "탑", Tbr: "탑" };
const GROUP_ORDER = ["프론트", "센터", "서브", "서라운드", "백", "와이드", "탑", "하이트", "기타"];

function config(roles: readonly string[], preset: string): ConfigResult {
  const speakers = roles.map((role, i) => ({ role, size: role.startsWith("SW") ? "sub" : roles.some((r) => r.startsWith("SW")) ? "small" : "large", bands: [{ band: role.startsWith("SW") ? "SUB" : "FULL", out: i + 1, pol: "+" }] }));
  const groups = GROUP_ORDER.map((key) => ({ id: key, key, speaker_indices: roles.map((r, i) => ((r.startsWith("SW") ? "서브" : GROUP_OF[r]) === key ? i : -1)).filter((i) => i >= 0) })).filter((g) => g.speaker_indices.length);
  const subs = roles.filter((r) => r.startsWith("SW")).length;
  return { speakers, preset, device: "MOTU M4", groups, summary: { speaker_count: roles.length, outputs: roles.map((_, i) => i + 1), max_channel: roles.length, duplicate_outputs: [], sub_count: subs } };
}

const sampleConfig = config(SAMPLE_ROLES, "7.1.4");
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

/**
 * The settings the recorded calibration ran on (a new project's: no room
 * volume), with the recommended crossovers stored and MIMO's IIR supports,
 * which neither changes the correction's record.
 */
export const baseSettings: CalibrationSettings = { ...fx.newProjectSettings, xo: XO, xo_rec: XO, mimo_fir: false };

const modeSpecs: Record<"zero" | "mixed" | "linear" | "ultra120", ModeSpecResult> = {
  zero: { rc_filter: "iir", fir_taps: 0, peq_bands: 32, mimo_filter: "iir", lift_ms: 0, phase_frac: 6, max_latency_ms: 0 },
  mixed: { rc_filter: "iir_fir", fir_taps: 2048, peq_bands: null, mimo_filter: "iir", lift_ms: 8, phase_frac: 6, max_latency_ms: 10 },
  linear: { rc_filter: "iir_fir", fir_taps: 2048, peq_bands: null, mimo_filter: "iir", lift_ms: 28, phase_frac: 24, max_latency_ms: 30 },
  ultra120: { rc_filter: "iir_fir", fir_taps: 8192, peq_bands: null, mimo_filter: "iir", lift_ms: 118, phase_frac: 24, max_latency_ms: 120 },
};

/* ── Curves ─────────────────────────────────────────────────────────────── */

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
  return { tab, grid: arr(grid), curves, axes, stale: false, subs_only: false, rc_band_hz: tab === "spl" || tab === "gd" ? [RC[0], RC[1]] : null, rc_bands_hz: [], xo_hz: null, ...extra };
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
  const subs = SUBS.map((role, k) => {
    const dev = response(grid, role, 1, true);
    const before = grid.map((f, i) => t[i]! - 6 + (dev[i]! - shape(f) - (k ? 1.5 : -1)) * 0.9 + shape(f));
    const after = grid.map((f, i) => t[i]! - 6 + (dev[i]! - shape(f)) * 0.3 + shape(f));
    return { role, before, after };
  });
  const sumBefore = grid.map((_, i) => sumDb(...subs.map((s, k) => s.before[i]! + [-1.5, 2.5 * Math.sin(i / 9), 1.2 * Math.cos(i / 7)][k % 3]!)));
  const sumAfter = grid.map((_, i) => sumDb(...subs.map((s) => s.after[i]!)));
  const curves: ResultCurve[] = [{ kind: "target", role: null, values: arr(t) }];
  for (const s of subs) curves.push({ kind: "before", role: s.role, values: arr(s.before) }, { kind: "after", role: s.role, values: arr(s.after) });
  curves.push({ kind: "sum_before", role: null, values: arr(sumBefore) }, { kind: "sum_after", role: null, values: arr(sumAfter) });
  return curvesResult("ba", grid, curves, { subs_only: subsOnly });
}

/* ── Results and lines ──────────────────────────────────────────────────── */

const round1 = (value: number) => Math.round(value * 10) / 10;

/**
 * MIMO on the recorded correction. No record has MIMO yet, so these figures
 * are drawn on the recorded ones: shorter ringing than the correction alone
 * at every recorded position, and a flatness that MIMO gains on some mains
 * and gives up on others (so the order after MIMO differs).
 */
const MIMO_SHARE = [4, -10, 6];
const mimoSummary: MimoSummary = {
  ...fx.mimoSummary,
  input_count: MAINS.length,
  scorecard: RECORDED_SUMMARY.scorecard.map((score, k) => ({
    role: score.role,
    after_mimo: { ...score.after, in1_20: Math.min(100, round1(score.after.in1_20 + MIMO_SHARE[k % MIMO_SHARE.length]!)), gd_20: round1(score.after.gd_20 * 0.85) },
  })),
  late_energy: RECORDED_SUMMARY.late_energy.map(({ role, position, after_db }) => ({ role, position, after_mimo_db: round1(after_db - 2.5) })),
};

/** The recorded BA lines: three subs, nine positions. */
const baLines: SubResultLines = RECORD.lines.ba!;

/** BA with a fixed SW2 whose filters leave almost no pass band, and the gain that tries to fill it (hand-made: no record has a fixed sub yet). */
const baWarnLines: SubResultLines = {
  ...baLines,
  rows: baLines.rows.map((row) => (row.role === "SW2" ? { ...row, mode: "fixed", gain_db: 13.4, delay_ms: 1.2, high_pass: [40, "LR24"], low_pass: [60, "LR24"] } : row)),
  warnings: [
    { code: "sub_narrow_band", params: { role: "SW2", hp_hz: "40", lp_hz: "60" } },
    { code: "sub_high_gain", params: { role: "SW2", gain_db: "13.4" } },
  ],
};

/** SW1 and SW2 as a D.B.A. bundle, calculated alone (hand-made: no record has a bundle yet). */
const baDbaLines: SubResultLines = {
  ...baLines,
  subs_only: true,
  rows: baLines.rows.map((row) => row.role === "SW1" ? { ...row, mode: "bundle:A:dba-front", gain_db: 0, delay_ms: 0, allpass: [], peq: [[42, 4.5, -3.5]] }
    : row.role === "SW2" ? { ...row, mode: "bundle:A:dba-rear", gain_db: -1.2, delay_ms: 12.25, polarity: -1, peq: [[42, 4.5, -3.5]] } : row),
  layouts: [{ bundle: "A", kind: "dba", members: ["SW1", "SW2"], front: ["SW1"], rear: ["SW2"], rear_ms: 12.25, auto_ms: 11.75, trim_ms: 0.5, peq: [[42, 4.5, -3.5]] }],
};

/** The recorded calculation notes. */
const NOTES: readonly Note[] = RECORDED_SUMMARY.notes;

/** SW2's fixed filters (the BA view's fixed sub), as one typed engine note (hand-made with that view). */
const SW2_FILTERS: Note[] = [{
  code: "sub_filters",
  filters: [{ role: "SW2", value: { hp_hz: 40, hp_type: "LR24", lp_hz: 60, lp_type: "LR24", peq: [{ hz: 38, q: 5, gain_db: -4 }] } }],
}];

/** The recorded summary, stale or not, with these notes and the late energy at these positions. */
const summary = (stale = false, notes: readonly Note[] = NOTES, positions: readonly number[] = NINE_POSITIONS): CalibrationSummary => ({
  ...structuredClone(RECORDED_SUMMARY),
  stale,
  notes: [...notes],
  late_energy: RECORDED_SUMMARY.late_energy.filter((late) => positions.includes(late.position)),
});

/** The recorded lines for these mains, stale or not, with these BA lines; MIMO's support counts are drawn (no record has MIMO). */
function lines({ roles = MAINS, mimo = false, stale = false, ba = baLines as SubResultLines | null }: { roles?: readonly string[]; mimo?: boolean; stale?: boolean; ba?: SubResultLines | null } = {}): ResultLinesResult {
  const recorded = structuredClone(RECORD.lines);
  const channels = recorded.channels.filter((channel) => roles.includes(channel.role)).map((channel, i) => (mimo ? { ...channel, mimo_support_count: 1 + ((i * 3) % 4) } : channel));
  return { ...recorded, stale, channels, ba: ba ? { ...ba, stale } : null };
}

const noLines: ResultLinesResult = { stale: false, channels: [], total_delay_ms: null, notes: [], notices: [], rc: [], ba: null };

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
  positions: NINE_POSITIONS,
  measuredRoles: [...SAMPLE_ROLES],
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
  rangeRejection: null,
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

/* ── XO 추천 포함 and the stale recommendation (ADR 0028) ─────────────── */

/**
 * The front crossover typed in by hand (90 Hz, the recommendation 80 Hz):
 * the edit turned 'XO 추천 포함' off, so both of its switches are off with
 * the reason under them, and the result waits for 다시 보정하기.
 */
export const xoRecManualOffView: CalibrateView = {
  ...base,
  results: { calibration: summary(true), mimo: null, mimo_export: "off" },
  lines: lines({ stale: true }),
  curves: { ...splResult(), stale: true },
  settings: { ...baseSettings, xo: { ...XO, 프론트: 90 }, xo_rec_include: false },
  staleReason: "settings",
  xoRecTurnedOffByEdit: true,
};

/** Before the first run with two positions after a manual XO edit: both switches give their reason, each under its own switch. */
export const readyReasonsView: CalibrateView = {
  ...readyMimoTwoView,
  settings: { ...readyMimoTwoView.settings, xo: { ...XO, 프론트: 90 }, xo_rec: XO, xo_rec_include: false },
  xoRecTurnedOffByEdit: true,
};

/**
 * Recommended with 'XO 추천 포함' off, then the boost limit changed: the
 * crossover card keeps the recommended values and says they were computed
 * for other inputs (`results.xo_rec_stale`); the result is stale too.
 */
export const xoRecStaleView: CalibrateView = {
  ...base,
  results: { calibration: summary(true), mimo: null, mimo_export: "off", xo_rec_stale: true },
  lines: lines({ stale: true }),
  curves: { ...splResult(), stale: true },
  settings: { ...baseSettings, boost_max_db: 8, xo_rec_include: false },
  staleReason: "settings",
};

/* ── RC upper limit (ADR 0021) ──────────────────────────────────────────── */

/**
 * The range card on one upper-limit basis, the result and its RC lines as
 * the service's recorded calibration with these settings left them. `ranOn`
 * names the settings of the run when they differ from the ones shown (an
 * edit since).
 */
function rangeView(patch: Partial<CalibrationSettings>, positions: number[] = NINE_POSITIONS, ranOn: Partial<CalibrationSettings> = patch): CalibrateView {
  const settings = { ...baseSettings, ...patch };
  const run = recordedRun(ranOn);
  return { ...base, positions, settings, results: { ...run.results, calibration: summary(false, run.results.calibration!.notes, positions) }, lines: { ...lines(), notes: run.lines.notes, rc: run.lines.rc } };
}

/** One view per recorded upper-limit case (a new project has no room volume, so the Schroeder bases stay at 500 Hz). */
export const rangeViews = {
  strict: rangeView({}),
  strictTimes2: rangeView({ rc_hi_transition: 2 }),
  strictTimes4: rangeView({ rc_hi_transition: 4 }),
  lenient: rangeView({ rc_hi_basis: "lenient" }),
  agreement: rangeView({ rc_hi_basis: "agreement" }),
  /** Two measured positions: agreement is off, with the reason. */
  agreementUnavailable: rangeView({}, [1, 2]),
  /** Agreement chosen earlier, a position cleared since: still chosen, with what to do. */
  agreementShort: rangeView({ rc_hi_basis: "agreement" }, [1, 2], {}),
  manual: rangeView({ rc_hi: 500 }),
} satisfies Record<string, CalibrateView>;

/* ── Extra correction bands (ADR 0024) ──────────────────────────────────── */

const TWO_BANDS: RcBand[] = [
  { lo_hz: 2000, hi_hz: 4000 },
  { lo_hz: 15000, hi_hz: 20000 },
];
const FOUR_BANDS: RcBand[] = [
  { lo_hz: 1000, hi_hz: 2000 },
  { lo_hz: 3000, hi_hz: 5000 },
  { lo_hz: 8000, hi_hz: 10000 },
  { lo_hz: 15000, hi_hz: 20000 },
];

/** The extra bands of the recorded calculation with extra bands. */
const RECORDED_BANDS: RcBand[] = [
  { lo_hz: 1000, hi_hz: 2000 },
  { lo_hz: 15000, hi_hz: 20000 },
];

/**
 * A `rc_bands` note's shape for these extra bands, the RC band of `hi`
 * merged in; cases for the text renderer, not service output.
 */
function rcBandsNote(bands: readonly RcBand[], hi = 310.42): Note[] {
  const merged = bandLayout({ lo: RC[0], hi, extra: bands.map((band) => [band.lo_hz, band.hi_hz] as const) }).bands;
  return [{ code: "rc_bands", bands: merged.map(([lo_hz, hi_hz]) => ({ lo_hz, hi_hz })) }];
}

/**
 * The range card with these extra bands: SPL shades outside their union
 * with the RC band (`rc_bands_hz`). The result is the recorded calculation
 * with them when it was recorded (its notes carry the merged band set), and
 * the defaults' otherwise.
 */
function extraBandsView(bands: RcBand[], patch: Partial<CalibrateView> = {}): CalibrateView {
  const run = recordedRun({ rc_bands: bands });
  return {
    ...base,
    settings: { ...baseSettings, rc_bands: bands },
    curves: { ...splResult(), rc_bands_hz: bands.map((band) => [band.lo_hz, band.hi_hz]) },
    results: { calibration: summary(false, run.results.calibration!.notes), mimo: null, mimo_export: "off" },
    lines: { ...lines(), notes: run.lines.notes },
    ...patch,
  };
}

/**
 * The second band's upper edge set to 12000 Hz, below its lower edge: the
 * service refuses the list (`rc_band_range`, 1-based index) and the stored
 * bands stay.
 */
export const refusedBandEdit: RangeRejection = {
  edit: { kind: "rc_bands", bands: [{ ...TWO_BANDS[0]! }, { lo_hz: 15000, hi_hz: 12000 }] },
  error: ipcError("invalid_request", "rc_bands: rc_band_range", { field: "rc_bands", reason: "rc_band_range", index: "2", lo_hz: "15000", hi_hz: "12000" }),
};

export const extraBandViews = {
  /** None (the default): only 보정 구간 추가 under the edges. */
  none: base,
  /** 15–20 kHz: one row, the graph shaded between the RC band and it. */
  one: extraBandsView([{ lo_hz: 15000, hi_hz: 20000 }]),
  /** 2–4 kHz and 15–20 kHz: two rows, the graph shaded outside the three bands. */
  two: extraBandsView(TWO_BANDS),
  /** The second band's upper edge refused: the reason at that input, the stored 20000 Hz in it. */
  refused: extraBandsView(TWO_BANDS, { rangeRejection: refusedBandEdit }),
  /** Four bands: no add action. */
  four: extraBandsView(FOUR_BANDS),
  /** 1–2 kHz and 15–20 kHz as the service calculated them: the merged band set in the notes. */
  calculated: extraBandsView(RECORDED_BANDS),
} satisfies Record<string, CalibrateView>;

/** `rc_bands` note shapes for one extra band, two bands, and a band that merges with the RC band (text renderer cases). */
export const rcBandsNotes: Note[][] = [
  rcBandsNote([{ lo_hz: 15000, hi_hz: 20000 }]),
  rcBandsNote(TWO_BANDS),
  rcBandsNote([{ lo_hz: 250, hi_hz: 800 }]),
];

/* ── 극성 경고와 딥 보정 제한 (ADR 0033, 0034) ─────────────────────────── */

/**
 * A current result whose engine notes name a reversed main and the dips the
 * limit filled only partly, as secs-engine records them (one record per
 * dip). Hand-made until the service's recorded answers carry these notes.
 */
const POLARITY_DIP_NOTES: Note[] = [
  { code: "polarity_reversed", role: "Rs", confidence: 0.94, compared: 3, opposite: 3 },
  { code: "dip_limit", role: "L", dips: [{ hz: 94.43, depth_db: 7.54, fill_db: 4.77 }, { hz: 136.78, depth_db: 9.2, fill_db: 5.6 }] },
];

/** The polarity warning with its flip button, and the dip-limit line among the calculation notes. */
export const polarityDipView: CalibrateView = (() => {
  const notes = [...NOTES, ...POLARITY_DIP_NOTES];
  return { ...base, settings: { ...baseSettings, dip_limit: true }, results: { calibration: summary(false, notes), mimo: null, mimo_export: "off" }, lines: { ...lines(), notes } };
})();

/* ── 극저역 보호 (ADR 0022) ─────────────────────────────────────────────── */

/** A current result calculated with these settings, its notes as the service's recorded calculation with them left them. */
function recordedView(patch: Partial<CalibrationSettings>, shown: Partial<CalibrationSettings> = patch): CalibrateView {
  const notes = recordedRun(patch).results.calibration!.notes;
  return { ...base, settings: { ...baseSettings, ...shown }, results: { calibration: summary(false, notes), mimo: null, mimo_export: "off" }, lines: { ...lines(), notes } };
}

/**
 * 극저역 보호 notes for the sample room with the RC lower limit at 15 Hz:
 * each channel's roll-off, a high-pass for each sub, and the sub bus that
 * cannot hold its boost below the roll-off. Hand-made: the recorded
 * calculation with 극저역 보호 notes none of these, and none was recorded
 * at 15 Hz.
 */
const UNHELD_NOTES: Note[] = [
  ...SAMPLE_ROLES.map((role, i): Note => SUBS.includes(role)
    ? { code: "low_protect", role, rolloff_hz: 22 + 2 * SUBS.indexOf(role), hp_hz: (22 + 2 * SUBS.indexOf(role)) / Math.SQRT2, limited: true, reduced: 0, cuts: 0 }
    : { code: "low_protect", role, rolloff_hz: 28 + ((i * 3) % 7), hp_hz: null, limited: true, reduced: 0, cuts: 0 }),
  { code: "low_protect", role: SUBS.join(" + "), rolloff_hz: 22, hp_hz: null, limited: false, reduced: 0, cuts: 0 },
];

export const lowProtectViews = {
  /** The switch off (the default): the results as before. */
  off: base,
  /** On: the recorded calculation with it, which notes no channel in the sample room. */
  on: recordedView({ low_protect: true }),
  /** On with the RC lower limit at 15 Hz: the sub bus cannot hold its boost below the roll-off, a warning with what to do. */
  unheld: (() => {
    const notes = [...NOTES, ...UNHELD_NOTES];
    return { ...base, settings: { ...baseSettings, low_protect: true, rc_lo: 15 }, results: { calibration: summary(false, notes), mimo: null, mimo_export: "off" }, lines: { ...lines(), notes } };
  })(),
} satisfies Record<string, CalibrateView>;

/**
 * 극저역 보호 note shapes for each case the text renderer handles: a sub
 * with its high-pass, a sub keeping the one the user set, a main, and a sub
 * bus that could not hold its boost (not service output).
 */
export const lowProtectNotes: Notice[] = [
  { code: "low_protect", params: { role: "SW1", rolloff_hz: "22", hp_hz: String(22 / Math.SQRT2), limited: "true" } },
  { code: "low_protect", params: { role: "SW2", rolloff_hz: "28", hp_hz: "25", limited: "true" } },
  { code: "low_protect", params: { role: "L", rolloff_hz: "30", limited: "true" } },
  { code: "low_protect", params: { role: "SW1 + SW2", rolloff_hz: "22", limited: "false" } },
];

/* ── 입력 저역 컷 (ADR 0031) ────────────────────────────────────────────── */

export const inputHpViews = {
  /** On at 25 Hz: its frequency field under the switch, and the recorded calculation's note. */
  on: recordedView({ input_hp_hz: 25 }),
  /** Both protections on, side by side; the notes are the input cut's record (none has both). */
  both: recordedView({ input_hp_hz: 25 }, { input_hp_hz: 25, low_protect: true }),
} satisfies Record<string, CalibrateView>;

/* ── 저음 도착 맞추기, 고역 위상 맞추기, 방 울림 줄이기 (ADR 0025, 0030) ─────── */

/**
 * What a calculation with the three switches notes for each main, in the
 * engine's order (secs-service `scalar_notice` and the ModeDecay expansion):
 * lf_align with its phase strength and the bass arrival left against the
 * treble (not in Zero mode, which does not change phase), then a mode_decay
 * record per cut mode, or mode_decay_none where no mode rings long enough
 * (the height speakers here), then hf_phase with the rotation unwound above
 * the band (not in Zero mode either). Mixed mode's short delay cuts the
 * alignment (`limited`). Hand-made: no record has these switches on yet.
 */
function lfNotes(mode: "zero" | "mixed" | "linear", { align, decay, phase }: { align: boolean; decay: boolean; phase: boolean }): Note[] {
  return MAINS.flatMap((role, i): Note[] => {
    const notes: Note[] = [];
    if (align && mode !== "zero") {
      const arrival = mode === "mixed" ? 2.4 + (i % 3) * 0.3 : (((i * 7) % 9) - 4) / 10;
      notes.push({ code: "lf_align", role, strength: mode === "mixed" ? 0.5 : role.startsWith("T") ? 0.75 : 1, arrival_ms: Math.round(arrival * 10) / 10, limited: mode === "mixed" });
    }
    if (decay) {
      const modes = role.startsWith("T") ? [] : [
        { hz: 42 + (i % 2), t60_s: 0.82, cut_db: -8.4 },
        ...(i < 3 ? [{ hz: 63, t60_s: 0.55, cut_db: -4.9 }] : []),
      ];
      notes.push({ code: "mode_decay", role, target_t60_s: 0.31, modes });
    }
    if (phase && mode !== "zero") notes.push({ code: "hf_phase", role, rotation_deg: ((i * 67) % 340) - 170, limited: false });
    return notes;
  });
}

/** A current result calculated in `mode` with these switches, its notes after the phase notes as the engine leaves them. */
function lfView(mode: "zero" | "mixed" | "linear", patch: Partial<CalibrationSettings>): CalibrateView {
  const settings = { ...baseSettings, mode, ...patch };
  // Zero mode has no FIR, so no phase correction notes.
  const own = mode === "zero" ? NOTES.filter((note) => !["lift_capped", "phase_strength", "phase_band"].includes(note.code)) : NOTES;
  const notes = [...own, ...lfNotes(mode, { align: settings.lf_align, decay: settings.mode_decay, phase: settings.hf_phase })];
  return {
    ...base,
    settings,
    modeSpec: modeSpecs[mode],
    resultMode: { mode, ultra_ms: settings.ultra_ms },
    results: { calibration: summary(false, notes), mimo: null, mimo_export: "off" },
    lines: { ...lines(), notes },
  };
}

export const lfViews = {
  /** All off (the default): the card between BA and MIMO, the results as before. */
  off: base,
  /** Standard (linear) mode with all three on: a line per main for each in the notes. */
  on: lfView("linear", { lf_align: true, mode_decay: true, hf_phase: true }),
  /** Mixed mode with 저음 도착 맞추기 on: it aligns only part of the way, said under the switch and in its notes. */
  mixed: lfView("mixed", { lf_align: true }),
  /** Zero mode with all stored on: 저음 도착 맞추기 and 고역 위상 맞추기 off with the reason, 방 울림 줄이기 still on. */
  zero: lfView("zero", { lf_align: true, mode_decay: true, hf_phase: true }),
} satisfies Record<string, CalibrateView>;

/**
 * Line shapes for each case the text renderer handles (not service output):
 * a main aligned in full, one held by the pull limit with the bass arriving
 * early, one the engine could not measure at strength 0, two cut modes of
 * one main, a main with no mode to cut, a rotation unwound in full, one the
 * delay limit kept part of, and a channel with no room above the band to
 * unwind it.
 */
export const lfNoteCases: Notice[] = [
  { code: "lf_align", params: { role: "L", strength: "1", arrival_ms: "0.4", limited: "false" } },
  { code: "lf_align", params: { role: "Tfl", strength: "0.75", arrival_ms: "-1.2", limited: "true" } },
  { code: "lf_align", params: { role: "C", strength: "0", limited: "true" } },
  { code: "mode_decay", params: { role: "L", hz: "42.4", t60_s: "0.82", target_s: "0.31", depth_db: "-8.4" } },
  { code: "mode_decay", params: { role: "L", hz: "63", t60_s: "0.55", target_s: "0.31", depth_db: "-4.9" } },
  { code: "mode_decay_none", params: { role: "Lb", target_s: "0.31" } },
  { code: "hf_phase", params: { role: "L", rotation_deg: "37.4", limited: "false" } },
  { code: "hf_phase", params: { role: "R", rotation_deg: "-114.5", limited: "true" } },
  { code: "hf_phase_none", params: { role: "C" } },
];

/**
 * RC line shapes for every kind of evidence an rc_limit note can carry
 * (secs-service `scalar_notice`), and a note from before the bases: cases
 * for the text renderer, not service output (the recorded ones are in
 * `rangeViews`).
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
