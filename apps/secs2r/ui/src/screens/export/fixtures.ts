// Export views for every state the specimens and tests draw: the sample
// 7.1.4 room (src/ipc/fixtures.ts) with two subs, nine positions measured,
// and the same room without subs for the filter formats' scope.
import type { FrameModel } from "../../app/model";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { MimoSummary } from "../../ipc/bindings/MimoSummary";
import type { Notice } from "../../ipc/bindings/Notice";
import { SAMPLE_MAINS, SAMPLE_SUBS, calibrationSummary, cpuComputeReport, noResults } from "../../ipc/fixtures";
import { exportStepState, type ExportView } from "./types";

const BASE = "C:/Users/demo/Documents/SECS/SECS_export/2026-10-14 거실 7.1.4";

const exportMimo: MimoSummary = {
  compute: cpuComputeReport,
  stale: false,
  kind: "iir",
  fsiso: 150,
  support_count: SAMPLE_MAINS.length * SAMPLE_SUBS.length,
  // IIR supports carry no FIR taps.
  fir_taps_total: 0,
  notes: [],
  input_count: SAMPLE_MAINS.length,
};

const exportRc: Notice[] = [
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

/** The sample result without its subs: RC alone, so every filter format can carry it. */
const rcOnlySummary = { ...calibrationSummary, sub_roles: [] };

/** No subs, MIMO off: all three filter formats are on. */
export const exportRcOnly: ExportView = {
  ...exportReady,
  results: { calibration: rcOnlySummary, mimo: null },
  settings: { mode: "mixed", ultra_ms: 200, mimo_on: false },
};

/** No subs, the mains supporting their neighbours: MIMO goes in, so the text and Equalizer APO cannot carry it. */
export const exportMimoNoSubs: ExportView = {
  ...exportRcOnly,
  results: {
    calibration: rcOnlySummary,
    mimo: {
      ...exportMimo,
      support_count: SAMPLE_MAINS.length * 2,
      input_count: SAMPLE_MAINS.length,
    },
  },
  settings: { mode: "mixed", ultra_ms: 200, mimo_on: true },
};

const EQAPO = "D:/Audio/거실 보정/SECS_eqapo_RC_BA_mixed_20261014-142233";

/** Subs, no MIMO: the Equalizer APO configuration was saved (the files as the service lists them: config.txt first). */
export const exportEqapoDone: ExportView = {
  ...exportReady,
  results: { calibration: calibrationSummary, mimo: null },
  settings: { mode: "mixed", ultra_ms: 200, mimo_on: false },
  folder: { folder: "D:/Audio/거실 보정", stored: true },
  runs: {
    equalizer_apo: {
      status: "done",
      folder: EQAPO,
      files: [
        `${EQAPO}/config.txt`,
        `${EQAPO}/secs_mixed/secs_mixed.txt`,
        `${EQAPO}/EqAPO 적용 방법.txt`,
        ...SAMPLE_MAINS.map((role) => `${EQAPO}/secs_mixed/secs_mixed_${role}_fir.wav`),
      ],
    },
  },
};

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
  // The room the result came from: the sample's two subs, or none for the RC-only results.
  const subs = result ? result.sub_roles.length : SAMPLE_SUBS.length;
  return {
    current: "export",
    project: {
      preset: subs > 0 ? "7.1.4" : "7.0.4",
      subCount: subs,
      outputs: SAMPLE_MAINS.length + subs,
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
