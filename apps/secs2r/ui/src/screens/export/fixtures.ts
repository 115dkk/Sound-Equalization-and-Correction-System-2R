// Export views for every state the specimens and tests draw: the sample
// 7.1.4 room (src/ipc/fixtures.ts) with two subs, nine positions measured,
// and the same room without subs for the filter formats' scope.
import type { FrameModel } from "../../app/model";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { MeasureDevice } from "../../ipc/bindings/MeasureDevice";
import type { MimoSummary } from "../../ipc/bindings/MimoSummary";
import type { Notice } from "../../ipc/bindings/Notice";
import type { ApplyStatus } from "../../ipc/bindings/ApplyStatus";
import type { CamillaApplyStatus } from "../../ipc/bindings/CamillaApplyStatus";
import type { ServiceState } from "../../ipc/bindings/ServiceState";
import { HOST_NOW, hostEqapoRepaired, hostGivenUp, hostOtherUser, hostRestarting } from "../../host/fixtures";
import type { HostState } from "../../host/model";
import {
  CAMILLA_EXE,
  MOCK_DESIGN_ID,
  SAMPLE_MAINS,
  SAMPLE_SUBS,
  calibrationSummary,
  camillaUnreachable,
  cpuComputeReport,
  eqapoMissing,
  eqapoXt,
  hostIdle,
  hostManaging,
  noResults,
} from "../../ipc/fixtures";
import { DEFAULT_APPLY, FILE_DEVICES, exportStepState, type ApplyView, type DesignNameProblem, type ExportView, type HostView } from "./types";

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

/** The service as the screen sees it: `service` and its status, nothing in flight, drawn at the specimens' fixed time. */
const hostView = (service: ServiceState | null, status: HostState | null = null, patch: Partial<HostView> = {}): HostView => ({
  service,
  status: service === "running" ? status : null,
  installing: false,
  stopping: false,
  failure: null,
  now: HOST_NOW,
  ...patch,
});

/** 파일만 on both targets (the default), on Windows, nothing read or applied. */
export const applyIdle: ApplyView = {
  settings: DEFAULT_APPLY,
  addressDraft: null,
  exeDraft: null,
  host: hostView(null),
  status: null,
  checking: false,
  windows: true,
  applying: null,
  reverting: false,
  runs: {},
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
  apply: applyIdle,
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
      folder: "D:/Audio/거실 보정/camilladsp",
      files: [
        "D:/Audio/거실 보정/camilladsp/configs/secs_mixed.yml",
        ...SAMPLE_MAINS.map((role) => `D:/Audio/거실 보정/camilladsp/coeffs/fir_${role}.wav`),
      ],
    },
    before_after_mdat: {
      status: "done",
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

/* ── 적용 (ADR 0018) ────────────────────────────────────────────────────── */

/** CamillaDSP 4.1.3 at the default address, processing. */
const camillaRunning = (design: CamillaApplyStatus["design"], clipped = 0): CamillaApplyStatus => ({
  reachable: true,
  error: null,
  status: {
    version: "4.1.3",
    state: "Running",
    stop_reason: null,
    title: design.kind === "other" ? design.title : "SECS 2R · 거실 7.1.4 · 저지연 모드",
    description: design.kind === "this" ? `secs-design ${design.design_id}` : null,
    config_path: "C:/Users/demo/AppData/Roaming/SECS2R/applied/camilladsp/configs/secs_mixed.yml",
    capture_rate: 48000,
    clipped_samples: clipped,
    processing_load_pct: 18.4,
    playback_peak_db: [-14.2, -13.8],
  },
  design,
});

const status = (camilla: CamillaApplyStatus, eqapo = eqapoMissing, current_design_id: string | null = MOCK_DESIGN_ID, host: ServiceState = "not_installed"): ApplyStatus => ({ camilla, eqapo, host, current_design_id });

/** Another design's title as the service names it (`SECS 2R · <project> · <label>`). */
const OTHER_TITLE = "SECS 2R · 거실 영화 · 표준 모드";

/**
 * CamillaDSP set to 적용, with the Dirac Live Processor replacement's
 * devices so it has real endpoints to open; one apply view per state below.
 */
const camillaApply = (apply: Partial<ApplyView>): ExportView => ({
  ...exportReady,
  camillaDevices: exportDevicesWasapi.camillaDevices,
  apply: { ...applyIdle, settings: { ...DEFAULT_APPLY, camilla: { level: "apply", address: "127.0.0.1:1234" } }, status: status(camillaUnreachable), ...apply },
});

/** CamillaDSP 적용 with file placeholders still on both sides: the row says to choose devices. */
export const applyCamillaPlaceholders: ExportView = { ...camillaApply({}), camillaDevices: FILE_DEVICES };

/** CamillaDSP 적용, nothing answering at 127.0.0.1:1234 (the mock's default). */
export const applyCamillaUnreachable: ExportView = camillaApply({});

/** CamillaDSP 적용 while the status is first read. */
export const applyCamillaChecking: ExportView = camillaApply({ status: null, checking: true });

/** CamillaDSP runs this design. */
export const applyCamillaThis: ExportView = camillaApply({ status: status(camillaRunning({ kind: "this", design_id: MOCK_DESIGN_ID })) });

/** CamillaDSP runs another design and has clipped. */
export const applyCamillaOther: ExportView = camillaApply({ status: status(camillaRunning({ kind: "other", title: OTHER_TITLE }, 1204)) });

/** The header's 적용 running. */
export const applyCamillaApplying: ExportView = camillaApply({ status: status(camillaRunning({ kind: "other", title: OTHER_TITLE })), applying: "camilla" });

/** Applied: CamillaDSP now runs this design. */
export const applyCamillaApplied: ExportView = camillaApply({
  status: status(camillaRunning({ kind: "this", design_id: MOCK_DESIGN_ID })),
  runs: { camilla: { status: "applied", designId: MOCK_DESIGN_ID, folder: "C:/Users/demo/AppData/Roaming/SECS2R/applied/camilladsp" } },
});

/** CamillaDSP refused the new configuration on Reload and went back to the previous one. */
export const applyCamillaRolledBack: ExportView = camillaApply({
  status: status(camillaRunning({ kind: "other", title: OTHER_TITLE })),
  runs: {
    camilla: {
      status: "rolled_back",
      error: { code: "apply_failed", params: {}, retryable: false, detail: "Reload: Playback error: could not open device 'Speakers (Tesira)': the device is in use by another application" },
    },
  },
});

/** An address being typed that cannot be stored (no port): the field says why, the header's primary is off. */
export const applyCamillaBadAddress: ExportView = camillaApply({ addressDraft: "192.168.0.40" });

/** An address on another computer: the row says the configuration file must be readable there at the same path. */
export const applyCamillaRemote: ExportView = camillaApply({
  settings: { ...DEFAULT_APPLY, camilla: { level: "apply", address: "192.168.0.40:1234" } },
  status: status({
    ...camillaUnreachable,
    error: { ...camillaUnreachable.error!, detail: "connect 192.168.0.40:1234: A connection attempt failed because the connected party did not properly respond after a period of time. (os error 10060)" },
  }),
});

/** Equalizer APO set to 적용 on an XT install; one apply view per state below. CamillaDSP stays 파일만. */
const eqapoApply = (apply: Partial<ApplyView>): ExportView => ({
  ...exportReady,
  apply: { ...applyIdle, settings: { ...DEFAULT_APPLY, eqapo: { level: "apply" } }, status: status(camillaUnreachable, eqapoXt), ...apply },
});

/** Equalizer APO 적용, not installed: 적용 stays chosen, its button off with the reason. */
export const applyEqapoMissing: ExportView = eqapoApply({ status: status(camillaUnreachable, eqapoMissing) });

/** Equalizer APO XT installed, SECS 2R's line not in config.txt yet. */
export const applyEqapoAbsent: ExportView = eqapoApply({});

/** The line present with this design, known from apply_status without an apply in this session. */
export const applyEqapoPresent: ExportView = eqapoApply({
  status: status(camillaUnreachable, { ...eqapoXt, include: "present", applied_design_id: MOCK_DESIGN_ID }),
});

/** Another design according to apply_status, without a session apply; CamillaDSP runs this one. */
export const applyEqapoOther: ExportView = eqapoApply({
  status: status(camillaRunning({ kind: "this", design_id: MOCK_DESIGN_ID }), { ...eqapoXt, include: "present", applied_design_id: "0c1f9e7a" }),
});

/** The official edition, the line present, but apply_status has no current design id to compare. */
export const applyEqapoOfficialUnknown: ExportView = eqapoApply({
  status: status(camillaUnreachable, { ...eqapoXt, variant: "official", config_path: "C:/Program Files/EqualizerAPO/config", include: "present", applied_design_id: "0c1f9e7a" }, null),
});

/** One of the two marked lines edited by hand: 적용 repairs them. */
export const applyEqapoDamaged: ExportView = eqapoApply({ status: status(camillaUnreachable, { ...eqapoXt, include: "damaged", applied_design_id: "0c1f9e7a" }) });

/** The official edition's config folder under Program Files, not writable for this user. */
export const applyEqapoDenied: ExportView = eqapoApply({
  status: status(camillaUnreachable, { ...eqapoXt, variant: "official", config_path: "C:/Program Files/EqualizerAPO/config" }),
  runs: { eqapo: { status: "failed", error: { code: "permission_denied", params: { path: "C:/Program Files/EqualizerAPO/config/SECS2R" }, retryable: false, detail: "Access is denied. (os error 5)" } } },
});

/** 빼기 done: the line is gone, the filter files stay in SECS2R. */
export const applyEqapoReverted: ExportView = eqapoApply({ runs: { eqapo: { status: "reverted" } } });

/** Not Windows: Equalizer APO's 적용 is off with the reason. */
export const applyEqapoNotWindows: ExportView = { ...exportReady, nativeDevices: null, apply: { ...applyIdle, windows: false } };

/* ── 관리 (ADR 0018) ────────────────────────────────────────────────────── */

const thisDesign = camillaRunning({ kind: "this", design_id: MOCK_DESIGN_ID });

/**
 * CamillaDSP set to 관리 with camilladsp.exe chosen and the Dirac Live
 * Processor replacement's devices; the service, and what CamillaDSP itself
 * answers, per state below.
 */
const camillaManage = (host: HostView, apply: Partial<ApplyView> = {}, camilla: CamillaApplyStatus = thisDesign): ExportView => ({
  ...exportReady,
  camillaDevices: exportDevicesWasapi.camillaDevices,
  apply: {
    ...applyIdle,
    settings: { ...DEFAULT_APPLY, camilla: { level: "manage", address: "127.0.0.1:1234", exe_path: CAMILLA_EXE } },
    status: status(camilla, eqapoMissing, MOCK_DESIGN_ID, host.service ?? "not_installed"),
    host,
    ...apply,
  },
});

/** 관리 chosen, camilladsp.exe not yet: the field says so and the header's primary is off. The service runs, managing nothing. */
export const manageCamillaNoExe: ExportView = camillaManage(hostView("running", hostIdle), { settings: { ...DEFAULT_APPLY, camilla: { level: "manage", address: "127.0.0.1:1234" } } }, camillaUnreachable);

/** The service is not installed: why it is needed, and 서비스 설치 (Windows asks once). */
export const manageCamillaNotInstalled: ExportView = camillaManage(hostView("not_installed"), {}, camillaUnreachable);

/** 서비스 설치 pressed: Windows's administrator approval is open. */
export const manageCamillaInstalling: ExportView = camillaManage(hostView("not_installed", null, { installing: true }), {}, camillaUnreachable);

/** The approval was declined: the install failed in this row. */
export const manageCamillaInstallFailed: ExportView = camillaManage(
  hostView("not_installed", null, { failure: { action: "install", target: "camilla", error: { code: "apply_failed", params: {}, retryable: false, detail: "elevated install: The operation was canceled by the user. (os error 1223)" } } }),
  {},
  camillaUnreachable,
);

/** The service is registered but stopped. */
export const manageCamillaStopped: ExportView = camillaManage(hostView("stopped"), {}, camillaUnreachable);

/** The service manages both targets: CamillaDSP running this design (PID 4812). */
export const manageCamillaRunning: ExportView = camillaManage(hostView("running", hostManaging));

/** CamillaDSP stopped; the service starts it again in four seconds, its second restart within five minutes. */
export const manageCamillaRestarting: ExportView = camillaManage(hostView("running", hostRestarting), {}, camillaUnreachable);

/** CamillaDSP stopped five times within five minutes: the service no longer starts it. */
export const manageCamillaGivenUp: ExportView = camillaManage(hostView("running", hostGivenUp), {}, camillaUnreachable);

/** Another user is signed in at the console: the service waits for the user who started managing. */
export const manageCamillaOtherUser: ExportView = camillaManage(hostView("running", hostOtherUser), {}, camillaUnreachable);

/** 관리 started from this row: applied and handed to the service. */
export const manageCamillaApplied: ExportView = camillaManage(hostView("running", hostManaging), {
  runs: { camilla: { status: "applied", designId: MOCK_DESIGN_ID, folder: "C:/Users/demo/AppData/Roaming/SECS2R/applied/camilladsp" } },
});

/** 관리 started while no CamillaDSP ran: the design was handed to the service, which starts CamillaDSP with it. */
export const manageCamillaHandedOver: ExportView = camillaManage(hostView("running", hostManaging), {
  runs: { camilla: { status: "applied", designId: MOCK_DESIGN_ID, folder: "C:/Users/demo/AppData/Roaming/SECS2R/applied/camilladsp", handedOver: true } },
}, camillaUnreachable);

/** Equalizer APO set to 관리 on the XT install with this design; the service put SECS 2R's line back two minutes ago. */
export const manageEqapoRepaired: ExportView = {
  ...exportReady,
  apply: {
    ...applyIdle,
    settings: { ...DEFAULT_APPLY, eqapo: { level: "manage" } },
    status: status(camillaUnreachable, { ...eqapoXt, include: "present", applied_design_id: MOCK_DESIGN_ID }, MOCK_DESIGN_ID, "running"),
    host: hostView("running", hostEqapoRepaired),
  },
};

/** Not Windows: CamillaDSP set to 적용 (its websocket works anywhere), 관리 off with the reason on both rows. */
export const manageNotWindows: ExportView = {
  ...exportReady,
  nativeDevices: null,
  camillaDevices: exportDevicesWasapi.camillaDevices,
  apply: { ...applyIdle, windows: false, settings: { ...DEFAULT_APPLY, camilla: { level: "apply", address: "127.0.0.1:1234" } }, status: status(thisDesign) },
};
