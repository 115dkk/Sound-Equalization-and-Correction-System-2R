// Fixtures typed by the generated bindings, for the mock service, the
// gallery and the tests. Values are plausible, not measured: the sample room
// is the 7.1.4 project the service creates with `project_new { sample }`.
import type { BootstrapInfo } from "./bindings/BootstrapInfo";
import type { CalibrationSummary } from "./bindings/CalibrationSummary";
import type { ChannelScore } from "./bindings/ChannelScore";
import type { ExportResult } from "./bindings/ExportResult";
import type { LevelResult } from "./bindings/LevelResult";
import type { MeasureConfigResult } from "./bindings/MeasureConfigResult";
import type { MeasureMethodsResult } from "./bindings/MeasureMethodsResult";
import type { MetricsSummary } from "./bindings/MetricsSummary";
import type { MimoSummary } from "./bindings/MimoSummary";
import type { ProjectInfo } from "./bindings/ProjectInfo";
import type { ResultsSummary } from "./bindings/ResultsSummary";
import type { RewStatusResult } from "./bindings/RewStatusResult";
import type { SignalChain } from "./bindings/SignalChain";
import type { XoRecommendation } from "./bindings/XoRecommendation";

export const SAMPLE_ROLES = ["L", "R", "C", "SW1", "SW2", "Ls", "Rs", "Lb", "Rb", "Tfl", "Tfr", "Tbl", "Tbr"] as const;
export const SAMPLE_MAINS = SAMPLE_ROLES.filter((role) => !role.startsWith("SW"));
export const SAMPLE_SUBS = SAMPLE_ROLES.filter((role) => role.startsWith("SW"));

export const bootstrapInfo: BootstrapInfo = {
  app_name: "SECS 2R",
  version: "0.13.0",
  prerelease: true,
  upstream: {
    name: "Sound Equalization and Correction System 2",
    version: "Beta 1.2.0",
    commit: "60ac53ad3a3d9304da115e5dda3183a475d2d7dd",
    date: "2026-10-12",
  },
  platform: { os: "windows", arch: "x86_64" },
};

export const projectInfo: ProjectInfo = {
  path: "C:/Users/demo/Documents/SECS/거실 7.1.4.secs",
  name: "거실 7.1.4",
  preset: "7.1.4",
  sub_count: 2,
  positions: [1, 2, 3],
  dirty: true,
  migration_warnings: [],
};

export const emptyProject: ProjectInfo = {
  path: "",
  name: "",
  preset: "2.0",
  sub_count: 0,
  positions: [],
  dirty: false,
  migration_warnings: [],
};

export const rewStatus: RewStatusResult = { running: true, reachable: true, pro: true };

export const measureMethods: MeasureMethodsResult = {
  methods: ["native", "rew", "mock"],
  native_devices: [
    { id: "wasapi:{0.0.1.00000000}.{mic}", name: "UMIK-1", input_channels: 2, output_channels: 0 },
    { id: "wasapi:{0.0.0.00000000}.{out}", name: "MOTU M4", input_channels: 4, output_channels: 4 },
    { id: "wasapi:{0.0.0.00000000}.{hdmi}", name: "AVR (HDMI)", input_channels: 0, output_channels: 16 },
  ],
  native_error: null,
  rew: rewStatus,
  mock_positions: [1, 2, 3, 4, 5, 6, 7, 8, 9],
  mock_roles: [...SAMPLE_ROLES],
};

export const measureConfig: MeasureConfigResult = {
  method: "mock",
  sweep_level_dbfs: -12,
  input_device: "",
  output_device: "",
  input_channel: 1,
  roles: [...SAMPLE_ROLES],
  output_channels: SAMPLE_ROLES.map((_, index) => index + 1),
  sweep_length: 262144,
  timing: "acoustic",
  loopback_input_channel: null,
  timing_ref_output: 2,
  share_mode: "auto",
};

export const levelResult: LevelResult = { rms_dbfs: -22.4, peak_dbfs: -11.8, spl_db: 78.5, recommended_sweep_dbfs: -12 };

const metrics = (seed: number, after: boolean): MetricsSummary => {
  const j = (k: number) => ((seed * 37 + k * 11) % 17) / 17;
  return {
    level: 82 + j(1) * 4,
    in1_20: after ? 78 + j(2) * 14 : 31 + j(2) * 18,
    in2_20: after ? 92 + j(3) * 6 : 52 + j(3) * 20,
    in1_200: after ? 70 + j(4) * 12 : 38 + j(4) * 14,
    in2_200: after ? 90 + j(5) * 6 : 60 + j(5) * 12,
    in1_2000: after ? 64 + j(6) * 10 : 41 + j(6) * 10,
    in2_2000: after ? 86 + j(7) * 8 : 63 + j(7) * 10,
    turns: after ? 0.4 + j(8) * 0.4 : 1.6 + j(8) * 1.2,
    gd_20: after ? 18 + j(9) * 10 : 64 + j(9) * 40,
    gd_100: after ? 4 + j(10) * 4 : 12 + j(10) * 8,
    gd_500: after ? 1 + j(11) : 2 + j(11) * 2,
    gd24_abs: after ? 6 + j(12) * 6 : 22 + j(12) * 18,
    gd24_abs_hz: 40 + j(13) * 60,
    gd24_max: after ? 5 + j(14) * 4 : 18 + j(14) * 12,
    gd24_span: after ? 9 + j(15) * 6 : 30 + j(15) * 20,
    gd_30: after ? 12 + j(16) * 8 : 48 + j(16) * 30,
    pre_db: after ? -38 - j(1) * 6 : -60,
  };
};

export const scorecard: ChannelScore[] = SAMPLE_MAINS.map((role, index) => ({ role, before: metrics(index + 1, false), after: metrics(index + 1, true) }));

export const signalChain: SignalChain = {
  sample_rate: 48000,
  mains: SAMPLE_MAINS.map((role, index) => ({
    role,
    input_peq: [
      [42 + index * 3, 4.2, -5.5],
      [118 + index * 5, 6.1, -3.0],
    ],
    fir: [],
    gain_db: -1.5 - (index % 4) * 0.5,
    delay_ms: 0.4 + index * 0.21,
    xo_hz: 80,
    top_peq: [],
    align_delay_ms: 0,
  })),
  subs: SAMPLE_SUBS.map((role, index) => ({
    role,
    gain_db: -3 + index,
    delay_ms: 2.1 + index * 1.3,
    polarity: index === 1 ? -1 : 1,
    allpass: [],
    hp_hz: 0,
    hp_type: "LR24",
    lp_hz: 80,
    lp_type: "LR24",
    peq: [[38, 5, -4]],
    mode: "auto",
  })),
  sub_bus_peq: [[31, 4, -3.5]],
  main_extra_ms: 0,
  lift_ms: 7.5,
  latency_ms: 9.8,
};

export const calibrationSummary: CalibrationSummary = {
  stale: false,
  verdict: {
    within_1db_before_pct: 34.2,
    within_1db_after_pct: 81.6,
    low_band_time_spread_before_ms: 71.4,
    low_band_time_spread_after_ms: 22.9,
    added_delay_ms: 9.8,
    position_spread_db: 2.1,
  },
  scorecard,
  latency_ms: 9.8,
  warnings: [],
  notes: [{ code: "rc_upper_schroeder", params: { hz: "310" } }],
  chain: signalChain,
};

export const mimoSummary: MimoSummary = {
  stale: false,
  kind: "iir",
  fsiso: 150,
  support_count: 24,
  notes: [],
  paths: { fsiso: 150, paths: [] },
};

export const resultsSummary: ResultsSummary = { calibration: calibrationSummary, mimo: mimoSummary };
export const staleResults: ResultsSummary = {
  calibration: { ...calibrationSummary, stale: true },
  mimo: { ...mimoSummary, stale: true },
};
export const noResults: ResultsSummary = { calibration: null, mimo: null };

export const xoRecommendation: XoRecommendation = {
  best: [
    ["프론트", 80],
    ["센터", 100],
    ["서라운드", 80],
    ["백", 80],
    ["탑", 100],
  ],
  candidates: [60, 80, 100, 120],
};

export const exportResult: ExportResult = {
  files: [
    "C:/Users/demo/Documents/SECS_export/2026-10-14 거실 7.1.4/camilladsp/secs_camilladsp.yml",
    "C:/Users/demo/Documents/SECS_export/2026-10-14 거실 7.1.4/camilladsp/fir_L.wav",
  ],
};
