// Export views for every state the specimens and tests draw: the sample
// 7.1.4 room (src/ipc/fixtures.ts) with two subs, nine positions measured.
import type { FrameModel } from "../../app/model";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { MimoPath } from "../../ipc/bindings/MimoPath";
import type { MimoSummary } from "../../ipc/bindings/MimoSummary";
import type { Notice } from "../../ipc/bindings/Notice";
import { SAMPLE_MAINS, SAMPLE_SUBS, calibrationSummary, noResults } from "../../ipc/fixtures";
import { exportStepState, type ExportView } from "./types";

const BASE = "C:/Users/demo/Documents/SECS/SECS_export/2026-10-14 거실 7.1.4";

/** IIR support paths: each main input supported by both subs. */
const paths: MimoPath[] = SAMPLE_MAINS.flatMap((input, i) =>
  SAMPLE_SUBS.map((support, j) => ({
    input,
    support,
    filter: { Iir: { gain_db: -6 - j * 2, delay_ms: 4.5 + i * 0.3, polarity: j === 1 ? -1 : 1, peq: [[38 + j * 6, 4, -3]], allpass: [[52, 0.71]] } },
    low_hz: 20,
    delay_ms: 2.4 + i * 0.1,
  })),
);

export const exportMimo: MimoSummary = {
  stale: false,
  kind: "iir",
  fsiso: 150,
  support_count: paths.length,
  notes: [],
  paths: { fsiso: 150, paths },
};

export const exportRc: Notice[] = [
  { code: "rc_limit", params: { hz: "310.4", auto: "true", schroeder_hz: "310.4" } },
  { code: "rc_room_volume", params: { volume_m3: "62.5" } },
];

const exportError: IpcError = {
  code: "export_failed",
  params: {},
  retryable: true,
  detail: "failed to create C:/Users/demo/Documents/SECS/SECS_export/2026-10-14 거실 7.1.4/camilladsp/configs: Access is denied. (os error 5)",
};

/** A current result with MIMO, nothing exported yet. */
export const exportReady: ExportView = {
  results: { calibration: calibrationSummary, mimo: exportMimo },
  settings: { mode: "mixed", ultra_ms: 200, mimo_on: true },
  positions: 9,
  rc: exportRc,
  folder: { folder: BASE, stored: false },
  computing: false,
  exporting: null,
  runs: {},
  tutorial: false,
};

export const exportNoResult: ExportView = { ...exportReady, results: noResults, rc: [] };

export const exportComputingFirst: ExportView = { ...exportNoResult, computing: true };

export const exportStale: ExportView = {
  ...exportReady,
  results: { calibration: { ...calibrationSummary, stale: true }, mimo: { ...exportMimo, stale: true } },
};

export const exportExporting: ExportView = { ...exportReady, exporting: "before_after_mdat" };

export const exportDone: ExportView = {
  ...exportReady,
  folder: { folder: "D:/Audio/거실 보정", stored: true },
  runs: {
    camilla_dsp: {
      status: "done",
      folder: "D:/Audio/거실 보정/camilladsp",
      files: [
        "D:/Audio/거실 보정/camilladsp/configs/secs_mixed.yml",
        ...SAMPLE_MAINS.map((role) => `D:/Audio/거실 보정/camilladsp/coeffs/fir_${role}.wav`),
      ],
    },
    before_after_mdat: {
      status: "done",
      folder: "D:/Audio/거실 보정/SECS_mdat_RC_BA_MIMO_mixed_20261014-142233",
      files: ["D:/Audio/거실 보정/SECS_mdat_RC_BA_MIMO_mixed_20261014-142233/SECS 보정 전후 저지연 모드.mdat"],
    },
  },
};

export const exportFailed: ExportView = { ...exportReady, runs: { camilla_dsp: { status: "failed", error: exportError } } };

/** Fewer than three positions: MIMO is left out and the mdat folder says RC_BA. */
export const exportNoMimo: ExportView = { ...exportReady, positions: 1, results: { calibration: calibrationSummary, mimo: null }, settings: { mode: "linear", ultra_ms: 200, mimo_on: false } };

export const exportTutorial: ExportView = {
  ...exportNoMimo,
  settings: { mode: "mixed", ultra_ms: 200, mimo_on: false },
  folder: { folder: "C:/Users/demo/AppData/Local/Temp/secs2r-tutorial/SECS_export/2026-10-14 튜토리얼", stored: false },
  runs: { camilla_dsp: { status: "tutorial" } },
  tutorial: true,
};

/** The frame around 내보내기 for a view: earlier steps done, the rail state from the view. */
export function exportFrame(view: ExportView, fileName = "거실 7.1.4.secs"): FrameModel {
  const result = view.results.calibration;
  const calibration = view.computing ? "computing" : !result ? "none" : result.stale ? "stale" : "done";
  const total = view.positions >= 9 ? 9 : view.positions;
  return {
    current: "export",
    project: {
      preset: "7.1.4",
      subCount: 2,
      outputs: 13,
      positionsDone: view.positions,
      positionsTotal: total,
      calibration,
      path: view.tutorial ? { kind: "mock", state: "ok" } : { kind: "rew", state: "ok" },
      fileName,
      dirty: false,
    },
    steps: {
      system: { status: "done" },
      connect: { status: "done" },
      measure: { status: "done" },
      calibrate: calibration === "done" ? { status: "done" } : { status: "ready" },
      export: exportStepState(view),
    },
  };
}
