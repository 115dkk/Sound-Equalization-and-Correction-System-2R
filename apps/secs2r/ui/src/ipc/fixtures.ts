// Fixtures typed by the generated bindings, for the mock service, the
// gallery and the tests. Values are plausible, not measured: the sample room
// is the 7.1.4 project the service creates with `project_new { sample }`.
import type { AppSettings } from "./bindings/AppSettings";
import type { CalibrationSettings } from "./bindings/CalibrationSettings";
import type { ConfigResult } from "./bindings/ConfigResult";
import type { ConfigSpeaker } from "./bindings/ConfigSpeaker";
import type { DspResult } from "./bindings/DspResult";
import type { LayoutResult } from "./bindings/LayoutResult";
import type { ModeSpecResult } from "./bindings/ModeSpecResult";
import type { Mode } from "./bindings/Mode";
import type { RewInfoResult } from "./bindings/RewInfoResult";
import type { SubEntry } from "./bindings/SubEntry";
import type { TargetResult } from "./bindings/TargetResult";
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

import { arrangementPoints, ARRANGEMENT_COUNTS } from "../ui/maps/geometry";

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
  tutorial: false,
  migration_warnings: [],
};

export const emptyProject: ProjectInfo = {
  path: "",
  name: "",
  preset: "2.0",
  sub_count: 0,
  positions: [],
  dirty: false,
  tutorial: false,
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
  repetitions: 1,
  sample_rate: 48000,
  sweep_lengths: [65536, 131072, 262144, 524288, 1048576, 2097152, 4194304],
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
  stale_reason: null,
  mode: "mixed",
  ultra_ms: 100,
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
  notes: [{ code: "rc_limit", params: { hz: "310", auto: "true", schroeder_hz: "310" } }],
  chain: signalChain,
};

export const mimoSummary: MimoSummary = {
  stale: false,
  kind: "iir",
  fsiso: 150,
  support_count: 24,
  fir_taps_total: 0,
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
  folder: "C:/Users/demo/Documents/SECS_export/2026-10-14 거실 7.1.4/camilladsp",
  stale: false,
};

export const presetRoles: Record<string, readonly string[]> = {
  "2.0": ["L", "R"], "2.1": ["L", "R"], "2.2": ["L", "R"],
  "5.1": ["L", "R", "C", "Ls", "Rs"], "7.1.4": SAMPLE_MAINS,
  "9.1.6": [...SAMPLE_MAINS, "Lw", "Rw", "Tml", "Tmr"], custom: [],
};
/** Presentation group id → upstream group name (`ConfigGroup.key`). */
export const GROUP_KEYS: Readonly<Record<string, string>> = { front: "프론트", center: "센터", sub: "서브", surround: "서라운드", back: "백", wide: "와이드", top: "탑", height: "하이트", other: "기타" };
export const groupOf = (role: string): string => {
  if (role.startsWith("SW")) return "sub";
  if (["L", "R"].includes(role)) return "front";
  if (role === "C") return "center";
  if (["Ls", "Rs"].includes(role)) return "surround";
  if (["Lb", "Rb"].includes(role)) return "back";
  if (["Lw", "Rw"].includes(role)) return "wide";
  return role.startsWith("T") ? "top" : role.startsWith("H") ? "height" : "other";
};
export const speaker = (role: string, out: number): ConfigSpeaker => ({
  role, size: role.startsWith("SW") ? "sub" : "small",
  bands: [{ band: role.startsWith("SW") ? "SUB" : "FULL", out, pol: "+" }],
});
export function configOf(preset: string, speakers: ConfigSpeaker[], device = ""): ConfigResult {
  const outputs = speakers.flatMap((s) => s.bands.map((b) => b.out));
  return {
    preset, speakers, device,
    // As the service sends them: `id` is the presentation group, `key` the upstream (Korean) name the XO map uses.
    groups: Object.entries(GROUP_KEYS).map(([id, key]) => ({
      id, key, speaker_indices: speakers.flatMap((s, i) => groupOf(s.role) === id ? [i] : []),
    })).filter((g) => g.speaker_indices.length > 0),
    summary: {
      speaker_count: speakers.length, outputs: [...new Set(outputs)].sort((a, b) => a - b),
      max_channel: Math.max(0, ...outputs), sub_count: speakers.filter((s) => s.role.startsWith("SW")).length,
      duplicate_outputs: [...new Set(outputs.filter((out, i) => outputs.indexOf(out) !== i))],
    },
  };
}
export function configPreset(preset: string, subs: number): ConfigResult {
  const mains = presetRoles[preset] ?? presetRoles["2.0"]!;
  const roles = [...mains.slice(0, 3), ...Array.from({ length: subs }, (_, i) => `SW${i + 1}`), ...mains.slice(3)];
  return configOf(preset, roles.map((role, i) => speaker(role, i + 1)));
}

export const dsp: DspResult = {
  profile: "camilla", name: "PC · CamillaDSP",
  fields: { fir_taps: 65536, fir_budget: 2097152, iir_bands: 64, allpass_bands: 64, bm_path_delays: 64 },
  presets: [
    { key: "camilla", name: "PC · CamillaDSP", fields: { fir_taps: 65536, fir_budget: 2097152, iir_bands: 64, allpass_bands: 64, bm_path_delays: 64 } },
    { key: "custom", name: "Custom", fields: { fir_taps: 2048, fir_budget: 32768, iir_bands: 16, allpass_bands: 16, bm_path_delays: 16 } },
    { key: "minidsp", name: "miniDSP Flex / 2x4 HD", fields: { fir_taps: 2048, fir_budget: 4096, iir_bands: 10, allpass_bands: 0, bm_path_delays: 0 } },
    { key: "qsys", name: "Q-SYS Core", fields: { fir_taps: 16384, fir_budget: 65536, iir_bands: 24, allpass_bands: 16, bm_path_delays: 16 } },
    { key: "tesira", name: "Biamp Tesira", fields: { fir_taps: 2048, fir_budget: 51200, iir_bands: 16, allpass_bands: 16, bm_path_delays: 16 } },
  ],
  window_ms: 65536 / 48, resolution_hz: 48000 / 65536,
};
export const layout: LayoutResult = {
  count: 9, start_delay: "끄기", timing: { reference: "acoustic", role: "R", output: 2 },
  arrangements: ARRANGEMENT_COUNTS.map((count) => ({ count, points: arrangementPoints(count).map((p, i) => ({ position: i + 1, x_cm: p.x, y_cm: p.y, height: p.h })) })),
};
export const target: TargetResult = {
  preset: "harman", points: [[25, 6], [60, 5], [120, 1], [1000, 0], [16000, -3]],
  presets: [
    ["flat", [[25, 0], [1000, 0], [16000, 0]]],
    ["harman", [[25, 6], [60, 5], [120, 1], [1000, 0], [16000, -3]]],
    ["cinema", [[25, 4], [80, 3], [200, 0], [1000, -1], [16000, -5]]],
  ],
};
export const calibrationSettings: CalibrationSettings = {
  mode: "mixed", rc_lo: 20, rc_hi: null, room_volume_m3: 62.6, room_dims_m: [5.2, 4.3, 2.8],
  mimo_on: false, fsiso: 150, mimo_low: null, mimo_max_supports: 0, mimo_fir: false,
  xo: { front: 80, center: 100, surround: 80, back: 80, top: 100 }, xo_rec: {}, ultra_ms: 100,
  subs_manual: {}, sub_main_delay_ms: 0, bundle_layouts: {}, infra_boost_max_db: 24,
  xo_peq_on: true, boost_max_db: 10, xo_peq_lo: 0, xo_peq_hi: 0,
};
export const subEntry: SubEntry = {
  mode: "auto", bundle: "A", gain_db: 0, delay_ms: 0, polarity: 1, allpass: [], mimo_support: true,
  hp_hz: 0, lp_hz: 0, peq: [], hp_type: "LR24", lp_type: "LR24", dba_row: "front",
};
export const appSettings: AppSettings = { language: "ko", theme: "system", expert_display: false, recent_projects: [], last_window_size: null };
export const rewInfo: RewInfoResult = {
  executable_path: "C:/Program Files/REW/roomeqwizard.exe", ...rewStatus, version: "5.40", blocking: false,
  input_calibration_file: "UMIK-1.txt", input_devices: [{ id: "mic", name: "UMIK-1", channels: 2 }],
  output_devices: [{ id: "hdmi", name: "AVR (HDMI)", channels: 16 }], input_channel_names: ["Input 1", "Input 2"],
  sample_rate: 48000, sweep: { start_hz: 10, end_hz: 20000, length: "256k" }, level_dbfs: -12,
};
export function modeSpec(mode: Mode, ultra: number): ModeSpecResult {
  const max = mode === "zero" ? 0 : mode === "mixed" ? 10 : mode === "linear" ? 30 : Math.min(200, Math.max(30, ultra));
  return { rc_filter: mode === "zero" ? "iir" : "iir_fir", fir_taps: mode === "zero" ? 0 : mode === "ultra" ? 8192 : 2048,
    peq_bands: mode === "zero" ? 32 : null, mimo_filter: "iir", lift_ms: Math.max(0, max - 2), phase_frac: max > 10 ? 24 : 6, max_latency_ms: max };
}
// Synthetic IPC values, not a numerical-engine implementation.
export const curveGrid = Array.from({ length: 280 }, (_, i) => 5 * (20000 / 5) ** (i / 279));
export const curveValues = (seed: number, scale = 1): number[] => curveGrid.map((_, i) => scale * (4 * Math.sin(i / 13 + seed) + 2 * Math.cos(i / 7 + seed)));
