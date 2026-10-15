// 측정·가져오기: what the screen draws (MeasureView) and what the user can
// do on it (MeasureActions). The screen never calls the service; P33 builds
// the view from the IPC and implements the actions (docs/packets/P31a,
// M5-screens-common.md). Upstream: src/secs/wizard/step5_measure.py.
import type { BlockedReason, StepState } from "../../app/model";
import type { JobId } from "../../ipc/bindings/JobId";
import type { JobKind } from "../../ipc/bindings/JobKind";
import type { JobProgress } from "../../ipc/bindings/JobProgress";
import type { JobStatus } from "../../ipc/bindings/JobStatus";
import type { LayoutResult } from "../../ipc/bindings/LayoutResult";
import type { MdatImportParams } from "../../ipc/bindings/MdatImportParams";
import type { MdatScanResult } from "../../ipc/bindings/MdatScanResult";
import type { MeasureConfigResult } from "../../ipc/bindings/MeasureConfigResult";
import type { MeasurementCurvesResult } from "../../ipc/bindings/MeasurementCurvesResult";
import type { Notice } from "../../ipc/bindings/Notice";
import type { PositionEstimateResult } from "../../ipc/bindings/PositionEstimateResult";
import type { ProjectInfo } from "../../ipc/bindings/ProjectInfo";
import type { AxisRangeValue } from "../../ui/plot/AxisRange";

/**
 * How this project gets its measurements. `measure`: a measurement method is
 * chosen in 연결·점검 (`config.method`); `none`: no method chosen yet (the
 * service itself has no such value, it falls back to `native`; P33 derives
 * it from the 연결·점검 choice); `import`: the mdat import path, where
 * 연결·점검 is skipped and the primary action imports instead of measuring.
 */
export type MeasurePath = "measure" | "none" | "import";

/**
 * One measurement position as the list, the source note and 파일 관리 show it.
 * Service gap: no IPC method reports per-position records yet (roles
 * recorded, done, `source` per role, upstream `done_positions()` and
 * `mdat_sources()`); `ProjectInfo.positions` only lists positions with any IR.
 */
export type PositionRecord = {
  /** One-based position number. */
  position: number;
  /** Every role of the measurement plan is recorded here (upstream `done_positions`). */
  done: boolean;
  /** Roles recorded at this position, in plan order. */
  roles: string[];
  /** mdat file name per imported role (upstream measurement `source`); roles measured here are absent. */
  sources: { [role: string]: string };
};

/**
 * A sweep length the device can play, with its duration at the device's
 * sample rate (upstream `fill_sweep_combo`: "256k · 5.5 s"). Service gap:
 * `MeasureConfigResult` has the chosen length but neither the choices nor
 * the sample rate.
 */
export type SweepOption = { samples: number; seconds: number };

/**
 * The running job of the service's device group (measure, sample_load,
 * mdat_import, level_check, level_monitor): only one runs at a time, and
 * while it runs nothing on this screen may start another. P33 keeps it from
 * the `jobs` snapshot and the pushed `JobEvent`s.
 */
export type DeviceJob = {
  job: JobId;
  kind: JobKind;
  /** `running` or `cancel_requested`. */
  status: JobStatus;
  /** The position a `measure` job records (its `measure_position` params); null for other kinds. */
  position: number | null;
  /**
   * Last `progress` event. For `measure` the stage is the role just recorded
   * and done/total count the plan's roles. Before the first sweep the start
   * delay counts down as stage `countdown` with done = seconds passed and
   * total = the delay. Service gap: the measure job does not run the start
   * delay yet (upstream counts down in the screen before starting).
   */
  progress: JobProgress | null;
};

/** The low S/N notices (`measurement_low_snr`) of the last measurement of one position. */
export type PositionWarnings = { position: number; notices: Notice[] };

/** An mdat file picked and scanned, waiting in the assignment dialog. */
export type MdatPending = {
  /** `dialog_pick_mdat` result. */
  path: string;
  /** `mdat_scan { path }`. */
  scan: MdatScanResult;
  /** 파일 관리 "가져오기…" on one row: every measurement of the file goes to this position. */
  position: number | null;
};

/** Start delays as stored in the project (`extra.start_delay`, upstream `START_DELAYS`). */
export const START_DELAYS = ["끄기", "5 초", "10 초", "15 초"] as const;
export type StartDelay = (typeof START_DELAYS)[number];

/** Sweep repetitions REW and the native path accept (upstream `REPETITIONS`). */
export const REPETITIONS = [1, 2, 4, 8] as const;

export type MeasureView = {
  /** `project_info`. `path` is "" while the project has never been saved: the primary action asks for one first. */
  project: ProjectInfo;
  /** `layout_get`: the arrangement (`count`) with its geometry, the start delay and the timing reference. */
  layout: LayoutResult;
  /** `position_estimate`; null while unknown. */
  estimate: PositionEstimateResult | null;
  /** `measure_configure` / `level_apply` result: method, roles in measuring order, sweep length, timing. */
  config: MeasureConfigResult;
  /** See MeasurePath. */
  path: MeasurePath;
  /** Recorded positions (service gap, see PositionRecord). */
  records: PositionRecord[];
  /** `measurement_curves { positions: null, roles: null }`; null or no curves before anything is measured. */
  curves: MeasurementCurvesResult | null;
  /** Sweep length choices (service gap, see SweepOption). */
  sweepOptions: SweepOption[];
  /** Sweep repetitions 1/2/4/8 (`extra.devices.repetitions`). Service gap: no IPC reads or writes it yet. */
  repetitions: number;
  /** The running device-group job, or null. */
  job: DeviceJob | null;
  /** Low S/N notices per position, from the measure job's `log` events (P33 keeps the last measurement's). */
  warnings: PositionWarnings[];
  /** The position picked on the map or the list (null: the next one not measured). P33 keeps it across visits and clears it after a measurement, as upstream does. */
  selected: number | null;
  /** The graph is hidden and the map drawn larger. P33 keeps it across visits. */
  graphHidden: boolean;
  /** The measurement graph's axis range from its popover (null limits: the graph's own). P33 keeps it for the run, as upstream does. */
  plotRange: AxisRangeValue;
  /** An mdat file waiting in the assignment dialog; null when the dialog is closed. */
  mdat: MdatPending | null;
};

export type MeasureActions = {
  /**
   * `measure_position { position }` (job). When `view.project.path` is "",
   * P33 first asks for a file with `dialog_save_project` and saves there
   * (`project_save { path }`); a cancelled dialog starts nothing. Upstream
   * created `~/Documents/SECS/측정 YYYYMMDD-HHMM.secs` without asking.
   */
  measurePosition: (position: number) => void | Promise<void>;
  /** `job_cancel { job }`: stops the countdown at once, or a sweep after the current one. */
  cancelJob: (job: JobId) => void | Promise<void>;
  /** `position_clear { position }`, after the screen's confirmation (U5); resolves to `UndoResult.token`. */
  clearPosition: (position: number) => Promise<string>;
  /** `undo { token }`. */
  undo: (token: string) => Promise<void>;
  /** Host: sets `view.selected`. The screen does not call it while a device job runs. */
  selectPosition: (position: number) => void;
  /** Host: sets `view.graphHidden`. */
  setGraphHidden: (hidden: boolean) => void;
  /** Host: sets `view.plotRange` (the axis popover's 적용, or 자동 for a limit). */
  setPlotRange: (range: AxisRangeValue) => void;
  /** `layout_set { count: null, start_delay, timing: null, confirm: false }`. */
  setStartDelay: (delay: StartDelay) => void | Promise<void>;
  /** `measure_configure { method: view.config.method, sweep_length, output_channels: [], every other field null }`. */
  setSweepLength: (samples: number) => void | Promise<void>;
  /** Stores `extra.devices.repetitions`. Service gap: no IPC method yet. */
  setRepetitions: (count: number) => void | Promise<void>;
  /**
   * `dialog_pick_mdat`, then `mdat_scan { path }`; the result goes to
   * `view.mdat` with `position` (null from 가져오기; a position from a 파일
   * 관리 row). A cancelled file dialog changes nothing.
   */
  pickMdat: (position: number | null) => void | Promise<void>;
  /** `mdat_import` (job) with the dialog's assignments; P33 then clears `view.mdat`. */
  importMdat: (params: MdatImportParams) => void | Promise<void>;
  /** Host: clears `view.mdat` (the assignment dialog was closed without importing). */
  closeMdat: () => void;
  /** `sample_load { positions: null }` (job): the demo room's measured position 1. Tutorial only. */
  loadSample: () => void | Promise<void>;
  /** Host: opens 보정 (the frame's navigation), the primary action once every position is measured. */
  openCalibrate: () => void;
};

export const isRunning = (job: DeviceJob | null): job is DeviceJob => !!job && (job.status === "running" || job.status === "cancel_requested");

/** Positions measured with every role of the plan. */
export function donePositions(view: MeasureView): number[] {
  return view.records.filter((record) => record.done).map((record) => record.position).sort((a, b) => a - b);
}

/**
 * The position the primary action measures: the running measurement's, the
 * picked one, or the first one not measured (upstream `current_position`).
 * Null once every position is measured and none is picked.
 */
export function currentPosition(view: MeasureView): number | null {
  const count = view.layout.count;
  if (isRunning(view.job) && view.job.kind === "measure" && view.job.position) return view.job.position;
  if (view.selected !== null && view.selected >= 1 && view.selected <= count) return view.selected;
  const done = new Set(donePositions(view));
  for (let position = 1; position <= count; position++) if (!done.has(position)) return position;
  return null;
}

/**
 * The rail state of 측정·가져오기 (upstream `can_advance()` plus the path):
 * blocked while a measurement runs (`measuring`) or another device job does
 * (`device_busy`), done once every position is measured, blocked without a
 * measurement method unless on the import path (`no_path`), ready otherwise.
 */
/** Device jobs this screen starts: while one runs the step is still the
 * one being worked on (the primary action shows it busy), not blocked. */
export const OWN_JOBS: readonly JobKind[] = ["measure", "sample_load", "mdat_import"];

export function measureStepState(view: MeasureView): StepState {
  const blocked = (reason: BlockedReason): StepState => ({ status: "blocked", reason });
  // Blocked means another step must act first: here, a device job another
  // step started (a level check). This step's own jobs keep it ready.
  if (isRunning(view.job) && !OWN_JOBS.includes(view.job.kind)) return blocked("device_busy");
  const done = new Set(donePositions(view));
  let complete = view.layout.count > 0;
  for (let position = 1; position <= view.layout.count; position++) if (!done.has(position)) complete = false;
  if (complete) return { status: "done" };
  if (view.path === "none") return blocked("no_path");
  return { status: "ready" };
}
