// Fixtures typed by the generated bindings, for the mock service, the
// gallery and the tests. Values are plausible, not measured: the sample room
// is the 7.1.4 project the service creates with `project_new { sample }`.
// Service rules are not copied here (ADR 0014): limits and other constants
// come from the generated `bindings/rules`, presets, mode specs and new-project
// settings from the responses the service recorded (`./recorded`).
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

import { ARRANGEMENT_COUNTS, CALIBRATION_LIMITS, CONFIG_GROUPS } from "./bindings/rules";
import { recorded } from "./recorded";
import { arrangementPoints } from "../ui/maps/geometry";

export const SAMPLE_ROLES = ["L", "R", "C", "SW1", "SW2", "Ls", "Rs", "Lb", "Rb", "Tfl", "Tfr", "Tbl", "Tbr"] as const;
export const SAMPLE_MAINS = SAMPLE_ROLES.filter((role) => !role.startsWith("SW"));
export const SAMPLE_SUBS = SAMPLE_ROLES.filter((role) => role.startsWith("SW"));

export const bootstrapInfo: BootstrapInfo = {
  app_name: "SECS 2R",
  version: __BUILD_INFO__.version,
  prerelease: true,
  upstream: { ...__BUILD_INFO__.upstream },
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
  cached: false,
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
  main_roles: signalChain.mains.map((main) => main.role),
  sub_roles: signalChain.subs.map((sub) => sub.role),
};

export const computeStatus: import("./bindings/ComputeStatus").ComputeStatus = {
  preference: "auto", selected: "cpu", reason: "no_device", gpu_available: false, device: null, devices: [],
};
export const gpuDevice: import("./bindings/GpuDeviceInfo").GpuDeviceInfo = {
  id: 0, name: "NVIDIA GeForce RTX 2080 SUPER", compute_major: 7, compute_minor: 5, multiprocessors: 48,
  memory_bytes: 8589934592, integrated: false, supported: true, recommended: true, reason: "recommended",
};
export const gpuComputeStatus: import("./bindings/ComputeStatus").ComputeStatus = {
  preference: "auto", selected: "cuda", reason: "recommended", gpu_available: true, device: gpuDevice, devices: [gpuDevice],
};
export const cpuComputeReport: import("./bindings/ComputeReport").ComputeReport = {
  requested: "auto", backend: "cpu", device: null, stage: null, accelerated_systems: 0, cpu_systems: 11, fallbacks: 0, reason: "no_device",
};

export const mimoSummary: MimoSummary = {
  compute: cpuComputeReport,
  stale: false,
  kind: "iir",
  fsiso: 150,
  support_count: 24,
  fir_taps_total: 0,
  notes: [],
  input_count: 0,
};

export const resultsSummary: ResultsSummary = { calibration: calibrationSummary, mimo: mimoSummary };
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

/** Presentation group id → upstream group name (`ConfigGroup.key`), in the service's order. */
export const GROUP_KEYS: Readonly<Record<string, string>> = Object.fromEntries(CONFIG_GROUPS.map(([key, id]) => [id, key]));
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
/** Where subwoofers go among a layout's mains: after L, R and C, as in upstream model.PRESETS. */
export function subsAt(roles: readonly string[]): number {
  const at = roles.findIndex((role) => role !== "L" && role !== "R" && role !== "C");
  return at < 0 ? roles.length : at;
}

/** A preset's configuration as `config_apply_preset` answers it on a new project (recorded). */
export function presetConfig(preset: string): ConfigResult | undefined {
  return recorded("config_apply_preset", (params) => params.preset === preset)?.config;
}

/** A preset's mains with `subs` subwoofers after L, R and C, outputs numbered in order (the sample room, new projects). */
export function configPreset(preset: string, subs: number): ConfigResult {
  const base = presetConfig(preset) ?? presetConfig("2.0")!;
  const mains = base.speakers.map((s) => s.role).filter((role) => !role.startsWith("SW"));
  const at = subsAt(mains);
  const roles = [...mains.slice(0, at), ...Array.from({ length: subs }, (_, i) => `SW${i + 1}`), ...mains.slice(at)];
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
/** The sample room's target: the harman preset as `target_apply_preset` answers it (recorded). */
export const target: TargetResult = recorded("target_apply_preset", (params) => params.preset === "harman")!;
/** A new project's calibration settings (recorded `calibration_settings_get`). */
export const newProjectSettings: CalibrationSettings = recorded("calibration_settings_get")!;
/** The sample room's settings: a new project's, with the room, crossovers and MIMO filter the sample sets. */
export const calibrationSettings: CalibrationSettings = {
  ...newProjectSettings,
  room_volume_m3: 62.6, room_dims_m: [5.2, 4.3, 2.8],
  xo: { 프론트: 80, 센터: 100, 서라운드: 80, 백: 80, 탑: 100 },
  mimo_fir: false,
};
export const subEntry: SubEntry = {
  mode: "auto", bundle: "A", gain_db: 0, delay_ms: 0, polarity: 1, allpass: [], mimo_support: true,
  hp_hz: 0, lp_hz: 0, peq: [], hp_type: "LR24", lp_type: "LR24", dba_row: "front",
};
export const appSettings: AppSettings = { compute: "auto", language: "ko", theme: "system", icon: "expert", skin: "default", recent_projects: [], last_window_size: null };
export const rewInfo: RewInfoResult = {
  executable_path: "C:/Program Files/REW/roomeqwizard.exe", ...rewStatus, version: "5.40", driver: "Java", blocking: false,
  input_device: "UMIK-1", output_device: "AVR (HDMI)", input: "Input 1",
  // Java: REW knows the channel count only of the device it has open (0 = unknown).
  input_calibration_file: "UMIK-1.txt", input_devices: [{ id: "mic", name: "UMIK-1", channels: 2 }, { id: "line", name: "Line In (Realtek)", channels: 0 }],
  output_devices: [{ id: "hdmi", name: "AVR (HDMI)", channels: 16 }, { id: "speakers", name: "Speakers (Realtek)", channels: 0 }], input_channel_names: ["Input 1", "Input 2"],
  sample_rate: 48000, sweep: { start_hz: 10, end_hz: 20000, length: "256k" }, level_dbfs: -12,
};
/** File label from secs_types::mode_label, not the localized screen label. */
export function modeLabel(mode: Mode, ultra: number): string {
  const [lo, hi] = CALIBRATION_LIMITS.ultra_ms;
  if (mode === "ultra") return `초고지연 모드 ${Number(Math.min(hi, Math.max(lo, ultra)).toPrecision(6))} ms`;
  return { zero: "최소 지연 모드", mixed: "저지연 모드", linear: "표준 모드" }[mode];
}

/**
 * A mode's spec as `mode_spec` answers it (recorded). The ultra mode is
 * recorded at a new project's budget; other budgets get that spec too, a
 * sample rather than the service's taps and latency for them.
 */
export function modeSpec(mode: Mode, mimoFir = false): ModeSpecResult {
  return (recorded("mode_spec", (params) => params.mode === mode && (params.mimo_fir ?? false) === mimoFir)
    ?? recorded("mode_spec", (params) => params.mode === mode))!;
}
// Synthetic IPC values, not a numerical-engine implementation.
export const curveGrid = Array.from({ length: 280 }, (_, i) => 5 * (20000 / 5) ** (i / 279));
export const curveValues = (seed: number, scale = 1): number[] => curveGrid.map((_, i) => scale * (4 * Math.sin(i / 13 + seed) + 2 * Math.cos(i / 7 + seed)));
