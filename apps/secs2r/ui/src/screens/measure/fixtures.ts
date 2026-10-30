// Views of 측정·가져오기 for the gallery and the tests: the sample 7.1.4 room
// (two subs, 13 roles, the 9-position single-chair arrangement) in every
// state the specimens draw, and a 2.1 and a subs-only project for the graph's
// 0 dB reference. Curves come from the gallery's synthetic room, shifted by
// one reference as the service sends them (U17).
import { logGrid, response } from "../../gallery/data";
import type { LayoutResult } from "../../ipc/bindings/LayoutResult";
import type { MdatScanMeasurement } from "../../ipc/bindings/MdatScanMeasurement";
import type { MeasurementCurvesResult } from "../../ipc/bindings/MeasurementCurvesResult";
import { SAMPLE_ROLES, measureConfig, projectInfo } from "../../ipc/fixtures";
import { shiftToReference } from "../../ipc/mock";
import { ARRANGEMENT_COUNTS, arrangementPoints, isSubRole, roleKind } from "../../ui/maps/geometry";
import type { MeasureView, PositionRecord, SweepOption } from "./types";

const ROLES: string[] = [...SAMPLE_ROLES];

const GRID = Array.from(logGrid(5, 22000, 280));

/**
 * The curves of these measured positions, each with its roles, shifted by the
 * reference the service would pick for a project whose roles are `order`.
 */
function curvesFor(measured: readonly { position: number; roles: readonly string[] }[], order: readonly string[] = ROLES): MeasurementCurvesResult {
  const raw = measured.flatMap(({ position, roles }) =>
    roles.map((role) => ({
      role,
      position,
      kind: isSubRole(role) ? "sub" : roleKind(role) === "top" ? "top" : "main",
      values: Array.from(response(GRID, role, position, isSubRole(role))),
    })),
  );
  const { curves, reference } = shiftToReference(GRID, raw, order);
  return {
    grid: GRID,
    curves: curves.map((curve) => ({ ...curve, values: curve.values.map((v) => Math.round(v * 100) / 100) })),
    reference,
    axes: { frequency_hz: [5, 22000], value: [-35, 20], unit: "db" },
  };
}

const curvesAt = (positions: readonly number[], roles: readonly string[] = ROLES): MeasurementCurvesResult =>
  curvesFor(positions.map((position) => ({ position, roles })));

const layout: LayoutResult = {
  count: 9,
  start_delay: "끄기",
  timing: { reference: "acoustic", role: "R", output: 1 },
  arrangements: ARRANGEMENT_COUNTS.map((count) => ({
    count,
    points: arrangementPoints(count).map((point, index) => ({ position: index + 1, x_cm: point.x, y_cm: point.y, height: point.h })),
  })),
};

/** Sweep lengths at 48 kHz (upstream SWEEP_SECONDS: 64k 1.4 s … 4M 87.4 s). */
const sweepOptions: SweepOption[] = [65536, 131072, 262144, 524288, 1048576, 2097152, 4194304].map((samples) => ({
  samples,
  seconds: Math.round((samples / 48000) * 10) / 10,
}));

const measured = (positions: readonly number[], sources: Record<string, string> = Object.fromEntries(ROLES.map((role) => [role, "measured"])), roles: readonly string[] = ROLES): PositionRecord[] =>
  positions.map((position) => ({ position, done: true, roles: [...roles], sources: { ...sources } }));

const fromFile = (file: string): Record<string, string> => Object.fromEntries(ROLES.map((role) => [role, file]));

const base: MeasureView = {
  project: { ...projectInfo, positions: [], dirty: false },
  layout,
  estimate: { positions: 9, per_position_seconds: 113.1, total_seconds: 1017.9 },
  config: { ...measureConfig, method: "rew", roles: [...ROLES] },
  path: "measure",
  records: [],
  curves: curvesAt([]),
  sweepOptions,
  repetitions: 1,
  job: null,
  warnings: [],
  selected: null,
  graphHidden: false,
  plotRange: { x: null, y: null },
  mdat: null,
};

/** Nothing measured yet; the project has never been saved. */
export const emptyView: MeasureView = {
  ...base,
  project: { ...base.project, path: "", name: "", dirty: true },
};

/** Three of nine positions measured. */
export const partialView: MeasureView = {
  ...base,
  project: { ...projectInfo, positions: [1, 2, 3] },
  records: measured([1, 2, 3]),
  curves: curvesAt([1, 2, 3]),
};

/** Measuring position 4: seven of thirteen channels recorded. */
export const measuringView: MeasureView = {
  ...partialView,
  records: [...measured([1, 2, 3]), { position: 4, done: false, roles: ROLES.slice(0, 7), sources: Object.fromEntries(ROLES.slice(0, 7).map((role) => [role, "measured"])) }],
  job: { job: 12, kind: "measure", status: "running", position: 4, progress: { stage: "Lb", done: 7, total: 13 } },
};

/** Position 4 with a 10 s start delay, counting down before the first sweep. */
export const countdownView: MeasureView = {
  ...partialView,
  layout: { ...layout, start_delay: "10 초" },
  estimate: { positions: 9, per_position_seconds: 123.1, total_seconds: 1107.9 },
  job: { job: 12, kind: "measure", status: "running", position: 4, progress: { stage: "countdown", done: 3, total: 10 } },
};

/** Every position measured. */
export const doneView: MeasureView = {
  ...base,
  project: { ...projectInfo, positions: [1, 2, 3, 4, 5, 6, 7, 8, 9], dirty: false },
  records: measured([1, 2, 3, 4, 5, 6, 7, 8, 9]),
  curves: curvesAt([1, 2, 3, 4, 5, 6, 7, 8, 9]),
};

/** Three positions measured, the graph hidden and the map drawn larger. */
export const graphHiddenView: MeasureView = { ...partialView, graphHidden: true };

/** Position 4 measured with two channels under the S/N threshold, picked again. */
export const lowSnrView: MeasureView = {
  ...partialView,
  project: { ...projectInfo, positions: [1, 2, 3, 4] },
  records: measured([1, 2, 3, 4]),
  curves: curvesAt([1, 2, 3, 4]),
  selected: 4,
  warnings: [
    {
      position: 4,
      notices: [
        { code: "measurement_low_snr", params: { role: "SW2", snr_db: "17.23", threshold_db: "20" } },
        { code: "measurement_low_snr", params: { role: "Tbr", snr_db: "18.61", threshold_db: "20" } },
      ],
    },
  ],
};

const scanRow = (index: number, title: string, role: string | null, position: number, sub: boolean | null): MdatScanMeasurement => ({
  index,
  title,
  role,
  position,
  // The titles name their position ("L P4").
  position_tagged: true,
  looks_like_sub: sub,
});

/** An mdat file for position 4, scanned: one channel twice, one unreadable measurement. */
export const mdatAssignView: MeasureView = {
  ...partialView,
  mdat: {
    path: "C:/Users/demo/Documents/REW/거실 위치 4.mdat",
    position: null,
    scan: {
      measurements: [
        scanRow(0, "L P4", "L", 4, false),
        scanRow(1, "R P4", "R", 4, false),
        scanRow(2, "C P4", "C", 4, false),
        scanRow(3, "SW1 P4", "SW1", 4, true),
        scanRow(4, "SW2 P4", "SW2", 4, true),
        scanRow(5, "Ls P4", "Ls", 4, false),
        scanRow(6, "Rs P4", "Rs", 4, false),
        scanRow(7, "Rs P4 (다시)", "Rs", 4, false),
        scanRow(8, "Lrs P4", "Lb", 4, false),
        scanRow(9, "Rrs P4", "Rb", 4, false),
        scanRow(10, "Ltf P4", "Tfl", 4, false),
        scanRow(11, "Rtf P4", "Tfr", 4, false),
        scanRow(12, "Ltr P4", "Tbl", 4, false),
        scanRow(13, "Rtr P4", "Tbr", 4, false),
        scanRow(14, "Noise floor", null, 1, null),
      ],
      suggestions: [],
      problems: [],
    },
  },
};

/** Three positions measured here and part of position 5 imported: the 파일 관리 dialog's data. */
export const filesView: MeasureView = {
  ...partialView,
  project: { ...projectInfo, positions: [1, 2, 3, 5] },
  records: [
    ...measured([1, 2, 3]),
    { position: 5, done: false, roles: ROLES.slice(0, 8), sources: Object.fromEntries(ROLES.slice(0, 8).map((role) => [role, "거실 위치 5.mdat"])) },
  ],
  curves: curvesFor([...[1, 2, 3].map((position) => ({ position, roles: ROLES })), { position: 5, roles: ROLES.slice(0, 8) }]),
};

/** The mdat path: 연결·점검 skipped, positions 1–6 imported from one file. */
export const importedView: MeasureView = {
  ...base,
  path: "import",
  project: { ...projectInfo, positions: [1, 2, 3, 4, 5, 6] },
  records: measured([1, 2, 3, 4, 5, 6], fromFile("거실 7.1.4 위치 1-6.mdat")),
  curves: curvesAt([1, 2, 3, 4, 5, 6]),
  selected: 7,
};

/** No measurement method chosen yet (blocked: no_path). */
export const noPathView: MeasureView = { ...emptyView, path: "none" };

/** The tutorial: the mock method on the tutorial's own project, nothing measured. */
export const tutorialView: MeasureView = {
  ...base,
  project: { ...base.project, path: "C:/Users/demo/AppData/Local/Temp/SECS tutorial/tutorial 20261014-1412.secs", name: "튜토리얼" },
  config: { ...measureConfig, method: "mock", roles: [...ROLES] },
};

/** A demo-data load running (another device job): blocked with device_busy. */
export const sampleLoadingView: MeasureView = {
  ...tutorialView,
  job: { job: 3, kind: "sample_load", status: "running", position: null, progress: null },
};

/** A project with one sub and two mains, three positions measured: 0 dB is L at position 1 over 200~2000 Hz. */
const ROLES_21 = ["L", "R", "SW1"];
export const referenceMidView: MeasureView = {
  ...base,
  project: { ...projectInfo, path: "C:/Users/demo/Documents/SECS/서재 2.1.secs", name: "서재 2.1", preset: "2.1", sub_count: 1, positions: [1, 2, 3] },
  config: { ...base.config, roles: ROLES_21 },
  records: measured([1, 2, 3], Object.fromEntries(ROLES_21.map((role) => [role, "measured"])), ROLES_21),
  curves: curvesFor([1, 2, 3].map((position) => ({ position, roles: ROLES_21 })), ROLES_21),
};

/** A project of two subs and no mains, three positions measured: 0 dB is SW1 at position 1 over 40~120 Hz. */
const SUB_ROLES = ["SW1", "SW2"];
export const referenceSubView: MeasureView = {
  ...base,
  project: { ...projectInfo, path: "C:/Users/demo/Documents/SECS/서브 정렬.secs", name: "서브 정렬", preset: "custom", sub_count: 2, positions: [1, 2, 3] },
  config: { ...base.config, roles: SUB_ROLES },
  records: measured([1, 2, 3], Object.fromEntries(SUB_ROLES.map((role) => [role, "measured"])), SUB_ROLES),
  curves: curvesFor([1, 2, 3].map((position) => ({ position, roles: SUB_ROLES })), SUB_ROLES),
};

export const MEASURE_FIXTURES = {
  empty: emptyView,
  partial: partialView,
  measuring: measuringView,
  countdown: countdownView,
  done: doneView,
  graphHidden: graphHiddenView,
  lowSnr: lowSnrView,
  mdatAssign: mdatAssignView,
  files: filesView,
  imported: importedView,
  noPath: noPathView,
  tutorial: tutorialView,
  sampleLoading: sampleLoadingView,
  referenceMid: referenceMidView,
  referenceSub: referenceSubView,
} satisfies Record<string, MeasureView>;
