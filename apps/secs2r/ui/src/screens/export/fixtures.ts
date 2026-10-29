// Export views for every state the specimens and tests draw: the sample
// 7.1.4 room (src/ipc/fixtures.ts) with two subs, nine positions measured,
// and the same room without subs for the filter formats' scope.
import type { FrameModel } from "../../app/model";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { MeasureDevice } from "../../ipc/bindings/MeasureDevice";
import type { MimoSummary } from "../../ipc/bindings/MimoSummary";
import type { Notice } from "../../ipc/bindings/Notice";
import { SAMPLE_MAINS, SAMPLE_SUBS, calibrationSummary, cpuComputeReport, noResults } from "../../ipc/fixtures";
import { FILE_DEVICES, exportStepState, type DesignNameProblem, type ExportView } from "./types";

const BASE = "C:/Users/demo/Documents/SECS/SECS_export/2026-10-14 거실 7.1.4";

/**
 * WASAPI endpoints of a Windows PC that replaces a DSP processor: a virtual
 * cable the player plays into, the processor's USB endpoints, and the
 * measurement mic.
 */
export const EXPORT_DEVICES: MeasureDevice[] = [
  { id: "wasapi:{0.0.0.00000000}.{tesira-out}", name: "Speakers (Tesira)", input_channels: 0, output_channels: 8 },
  { id: "wasapi:{0.0.0.00000000}.{cable-in}", name: "CABLE Input (VB-Audio Virtual Cable)", input_channels: 0, output_channels: 8 },
  { id: "wasapi:{0.0.1.00000000}.{cable-out}", name: "CABLE Output (VB-Audio Virtual Cable)", input_channels: 8, output_channels: 0 },
  { id: "wasapi:{0.0.1.00000000}.{tesira-in}", name: "Line (Tesira)", input_channels: 8, output_channels: 0 },
  { id: "wasapi:{0.0.1.00000000}.{mic}", name: "UMIK-1", input_channels: 2, output_channels: 0 },
];

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
  scorecard: [],
  late_energy: [],
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
  results: { calibration: calibrationSummary, mimo: exportMimo, mimo_export: "included" },
  settings: { mode: "mixed", ultra_ms: 200, mimo_on: true },
  positions: 9,
  mdatScope: "all",
  rc: exportRc,
  folder: { folder: BASE, stored: false },
  computing: false,
  recalibrating: false,
  exporting: null,
  runs: {},
  tutorial: false,
  designName: "",
  camillaDevices: FILE_DEVICES,
  nativeDevices: EXPORT_DEVICES,
  outputRate: { fixed: 48000 },
  extraRates: [],
};

export const exportNoResult: ExportView = { ...exportReady, results: { ...noResults, mimo_export: "none" }, rc: [] };

export const exportComputingFirst: ExportView = { ...exportNoResult, computing: true, recalibrating: true };

export const exportStale: ExportView = {
  ...exportReady,
  results: { calibration: { ...calibrationSummary, stale: true }, mimo: { ...exportMimo, stale: true }, mimo_export: "stale" },
};

/**
 * A reopened project whose result came from the project cache and no longer
 * matches the inputs: CamillaDSP still exports it, both mdat formats need a
 * new correction (`stale_cache`, #92 TD-09).
 */
export const exportStaleCache: ExportView = {
  ...exportStale,
  results: { ...exportStale.results, calibration: { ...calibrationSummary, stale: true, cached: true } },
};

/** The same, with the correction 재보정 started running. */
export const exportRecalibrating: ExportView = { ...exportStaleCache, computing: true, recalibrating: true };

export const exportExporting: ExportView = { ...exportReady, exporting: "before_after_mdat" };

export const exportDone: ExportView = {
  ...exportReady,
  folder: { folder: "D:/Audio/거실 보정", stored: true },
  runs: {
    camilla_dsp: {
      status: "done",
      rate: 48000,
      notes: [],
      folder: "D:/Audio/거실 보정/camilladsp",
      files: [
        "D:/Audio/거실 보정/camilladsp/configs/secs_mixed.yml",
        ...SAMPLE_MAINS.map((role) => `D:/Audio/거실 보정/camilladsp/coeffs/fir_${role}.wav`),
      ],
    },
    before_after_mdat: {
      status: "done",
      rate: 48000,
      notes: [],
      folder: "D:/Audio/거실 보정/SECS_mdat_RC_BA_MIMO_mixed_20261014-142233",
      files: ["D:/Audio/거실 보정/SECS_mdat_RC_BA_MIMO_mixed_20261014-142233/SECS 원 측정+보정 결과 저지연 모드.mdat"],
    },
  },
};

/** The REW mdat set to carry the original measurements alone. */
export const exportMeasurementsOnly: ExportView = { ...exportReady, mdatScope: "measurements" };

/** The REW mdat set to carry the correction results alone. */
export const exportResultsOnly: ExportView = { ...exportReady, mdatScope: "results" };

/** No result yet: the REW mdat can carry the measurements, and says so. */
export const exportMeasurementsDone: ExportView = {
  ...exportNoResult,
  runs: {
    before_after_mdat: {
      status: "done",
      rate: 48000,
      notes: [],
      folder: `${BASE}/SECS_mdat_measurements_20261014-141502`,
      files: [`${BASE}/SECS_mdat_measurements_20261014-141502/SECS 원 측정.mdat`],
    },
  },
};

/** No result and nothing measured: nothing to save. */
export const exportNothingMeasured: ExportView = { ...exportNoResult, positions: 0 };

/** The old cache blocks the results, not the original measurements. */
export const exportStaleCacheMeasurements: ExportView = { ...exportStaleCache, mdatScope: "measurements" };

export const exportFailed: ExportView = { ...exportReady, runs: { camilla_dsp: { status: "failed", error: exportError } } };

/** The sample result without its subs: RC alone, so every filter format can carry it. */
const rcOnlySummary = { ...calibrationSummary, sub_roles: [] };

/** No subs, MIMO off: all three filter formats are on. */
export const exportRcOnly: ExportView = {
  ...exportReady,
  results: { calibration: rcOnlySummary, mimo: null, mimo_export: "off" },
  settings: { mode: "mixed", ultra_ms: 200, mimo_on: false },
};

/** No subs, the mains supporting their neighbours: MIMO goes in, so the text and Equalizer APO cannot carry it. */
export const exportMimoNoSubs: ExportView = {
  ...exportRcOnly,
  results: {
    calibration: rcOnlySummary,
    mimo_export: "included",
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
  results: { calibration: calibrationSummary, mimo: null, mimo_export: "off" },
  settings: { mode: "mixed", ultra_ms: 200, mimo_on: false },
  folder: { folder: "D:/Audio/거실 보정", stored: true },
  runs: {
    equalizer_apo: {
      status: "done",
      rate: 48000,
      notes: [],
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

const CONVOLUTION = "D:/Audio/거실 보정/SECS_convolution_RC_BA_MIMO_mixed_20261014-142233";

/** The convolution IR saved: a WAV and a CSV per input→output pair (subs fed from LFE), the guide, routing.json and the design record. */
export const exportConvolutionDone: ExportView = {
  ...exportReady,
  folder: { folder: "D:/Audio/거실 보정", stored: true },
  runs: {
    convolution: {
      status: "done",
      rate: 48000,
      notes: [],
      folder: CONVOLUTION,
      files: [
        ...[...SAMPLE_MAINS.map((role) => `${role}_to_${role}`), ...SAMPLE_SUBS.map((role) => `LFE_to_${role}`)].flatMap((stem) => [`${CONVOLUTION}/${stem}.wav`, `${CONVOLUTION}/${stem}.csv`]),
        `${CONVOLUTION}/컨볼루션 적용 방법.txt`,
        `${CONVOLUTION}/routing.json`,
        `${CONVOLUTION}/secs-design.json`,
      ],
    },
  },
};

/** The convolution IR could not be written. */
export const exportConvolutionFailed: ExportView = {
  ...exportReady,
  runs: {
    convolution: {
      status: "failed",
      error: { code: "export_failed", params: {}, retryable: true, detail: `failed to write ${BASE}/SECS_convolution_RC_BA_MIMO_mixed_20261014-142233/Tfl_to_Tfl.wav: There is not enough space on the disk. (os error 112)` },
    },
  },
};

/** A DSP at 96 kHz (ADR 0017): every filter row says it writes 96 kHz; the REW mdat stays at 48 kHz and says nothing. */
export const exportRate96k: ExportView = { ...exportReady, outputRate: { fixed: 96000 } };

/** The same, the convolution IR also written at 44.1 and 192 kHz for a convolver that follows the playing rate. */
export const exportExtraRates: ExportView = { ...exportRate96k, extraRates: [44100, 192000] };

/**
 * 자동 on a PC whose default playback device runs at 32 kHz: the CamillaDSP
 * configuration went out at 48 kHz with the service's `output_rate_fallback`
 * note, and its row says why and where the rate is set.
 */
export const exportRateFallback: ExportView = {
  ...exportReady,
  outputRate: "auto",
  folder: { folder: "D:/Audio/거실 보정", stored: true },
  runs: {
    camilla_dsp: {
      status: "done",
      rate: 48000,
      notes: [{ code: "output_rate_fallback", params: { rate: "32000" } }],
      folder: "D:/Audio/거실 보정/camilladsp",
      files: [
        "D:/Audio/거실 보정/camilladsp/configs/secs_mixed.yml",
        "D:/Audio/거실 보정/camilladsp/configs/secs_mixed_bypass.yml",
        ...SAMPLE_MAINS.map((role) => `D:/Audio/거실 보정/camilladsp/coeffs/secs_mixed_${role}_fir.wav`),
        "D:/Audio/거실 보정/camilladsp/secs-design.json",
      ],
    },
  },
};

/** RC alone, every filter export on, with a design name: each goes to its own SECS_<kind>_거실 영화 folder. */
export const exportNamed: ExportView = { ...exportRcOnly, designName: "거실 영화" };

/** A design name for each rule the service checks. */
export const DESIGN_NAME_SAMPLES: Record<DesignNameProblem, string> = {
  length: "거실 7.1.4 영화 감상 ".repeat(5).trim(),
  control: "거실\t영화",
  character: "거실/영화",
  trailing: "거실 영화.",
  reserved: "COM1",
};

/** The CamillaDSP devices of a Dirac Live Processor replacement: the player's virtual cable in, the processor's USB endpoint out. */
export const exportDevicesWasapi: ExportView = {
  ...exportReady,
  camillaDevices: {
    capture: { type: "wasapi", device: "CABLE Output (VB-Audio Virtual Cable)", format: "F32", exclusive: false, loopback: false },
    playback: { type: "wasapi", device: "Speakers (Tesira)", format: "S24", exclusive: true },
  },
};

/** No device list (enumeration failed): the playback name typed as Windows shows it, the capture recording it by loopback. */
export const exportDevicesTyped: ExportView = {
  ...exportReady,
  nativeDevices: null,
  camillaDevices: {
    capture: { type: "wasapi", device: null, format: "F32", exclusive: false, loopback: true },
    playback: { type: "wasapi", device: "Speakers (Tesira)", format: "F32", exclusive: false },
  },
};

/** Playback through an ASIO driver; capture stays a file placeholder. */
export const exportDevicesAsio: ExportView = {
  ...exportReady,
  camillaDevices: { capture: { type: "file" }, playback: { type: "asio", device: "Tesira ASIO" } },
};

/** ASIO chosen on both sides, the capture driver not named yet: CamillaDSP stays off until it is. */
export const exportDevicesAsioUnnamed: ExportView = {
  ...exportReady,
  camillaDevices: { capture: { type: "asio", device: "" }, playback: { type: "asio", device: "Tesira ASIO" } },
};

/** Fewer than three positions: MIMO is left out and the mdat folder says RC_BA. */
export const exportNoMimo: ExportView = { ...exportReady, positions: 1, results: { calibration: calibrationSummary, mimo: null, mimo_export: "off" }, settings: { mode: "linear", ultra_ms: 200, mimo_on: false } };

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
