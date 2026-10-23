// Typed 시스템 views for every state the specimens and tests draw. Values
// are plausible for the sample 7.1.4 room (src/ipc/fixtures.ts): two subs,
// thirteen outputs, three positions measured.
import type { ConfigBand } from "../../ipc/bindings/ConfigBand";
import type { ConfigResult } from "../../ipc/bindings/ConfigResult";
import type { ConfigSpeaker } from "../../ipc/bindings/ConfigSpeaker";
import type { DspFields } from "../../ipc/bindings/DspFields";
import type { DspResult } from "../../ipc/bindings/DspResult";
import type { LayoutResult } from "../../ipc/bindings/LayoutResult";
import type { Notice } from "../../ipc/bindings/Notice";
import type { PositionEstimateResult } from "../../ipc/bindings/PositionEstimateResult";
import { SAMPLE_ROLES } from "../../ipc/fixtures";
import { ARRANGEMENT_COUNTS, arrangementPoints, roleKind } from "../../ui/maps/geometry";
import type { SystemView } from "./types";

const GROUP_ORDER = ["front", "center", "sub", "surround", "back", "wide", "top", "height", "other"] as const;
/** The upstream group names the service sends as `ConfigGroup.key`. */
const GROUP_KEYS: Record<(typeof GROUP_ORDER)[number], string> = { front: "프론트", center: "센터", sub: "서브", surround: "서라운드", back: "백", wide: "와이드", top: "탑", height: "하이트", other: "기타" };

function groupOf(role: string): (typeof GROUP_ORDER)[number] {
  if (role === "L" || role === "R") return "front";
  if (role === "C") return "center";
  if (/^SW[1-8]$/.test(role)) return "sub";
  if (role === "Ls" || role === "Rs") return "surround";
  if (role === "Lb" || role === "Rb") return "back";
  if (role === "Lw" || role === "Rw") return "wide";
  if (/^T[fmb][lr]$/.test(role)) return "top";
  if (/^H[fb][lr]$/.test(role)) return "height";
  return "other";
}

const band = (name: string, out: number, pol = "+"): ConfigBand => ({ band: name, out, pol });

/** One speaker on one output, the way `SpeakerConfig.from_preset` builds it. */
export function speaker(role: string, out: number, options: { pol?: string; size?: string; bands?: ConfigBand[] } = {}): ConfigSpeaker {
  const sub = roleKind(role) === "sub";
  return {
    role,
    size: options.size ?? (sub ? "sub" : "small"),
    bands: options.bands ?? [band(sub ? "SUB" : "FULL", out, options.pol ?? "+")],
  };
}

/** A ConfigResult with the groups and summary the service derives. */
export function configOf(preset: string, speakers: ConfigSpeaker[], device: string): ConfigResult {
  const groups = GROUP_ORDER.map((id) => ({
    id,
    key: GROUP_KEYS[id],
    speaker_indices: speakers.map((s, i) => ({ s, i })).filter(({ s }) => groupOf(s.role) === id).map(({ i }) => i),
  })).filter((group) => group.speaker_indices.length > 0);
  const outs = speakers.flatMap((s) => s.bands.map((b) => b.out));
  const seen = new Map<number, number>();
  for (const out of outs) seen.set(out, (seen.get(out) ?? 0) + 1);
  return {
    speakers,
    preset,
    device,
    groups,
    summary: {
      speaker_count: speakers.length,
      outputs: [...new Set(outs)].sort((a, b) => a - b),
      max_channel: Math.max(0, ...outs),
      duplicate_outputs: [...seen].filter(([, n]) => n > 1).map(([out]) => out).sort((a, b) => a - b),
      sub_count: speakers.filter((s) => roleKind(s.role) === "sub").length,
    },
  };
}

const fields = (fir_taps: number, fir_budget: number, iir_bands: number, allpass_bands: number, bm_path_delays: number): DspFields => ({
  fir_taps,
  fir_budget,
  iir_bands,
  allpass_bands,
  bm_path_delays,
});

/** DspProfile.presets() in the service's (BTreeMap) order. */
const DSP_PRESETS: DspResult["presets"] = [
  { key: "camilla", name: "PC · CamillaDSP", fields: fields(65536, 2097152, 64, 64, 64) },
  { key: "custom", name: "사용자 정의", fields: fields(2048, 32768, 16, 16, 16) },
  { key: "minidsp", name: "miniDSP Flex / 2x4 HD", fields: fields(2048, 4096, 10, 0, 0) },
  { key: "qsys", name: "Q-SYS Core 8 Flex / 110f", fields: fields(16384, 65536, 24, 16, 16) },
  { key: "tesira", name: "Biamp Tesira SERVER (DSP-2 ×3)", fields: fields(2048, 51200, 16, 16, 16) },
];

export function dspOf(profile: string, override?: DspFields): DspResult {
  const preset = DSP_PRESETS.find((p) => p.key === profile) ?? DSP_PRESETS[0]!;
  const current = override ?? preset.fields;
  return {
    profile,
    name: preset.name,
    fields: current,
    presets: DSP_PRESETS,
    window_ms: (current.fir_taps / 48000) * 1000,
    resolution_hz: 48000 / Math.max(1, current.fir_taps),
  };
}

export function layoutOf(count: number, start_delay = "끄기", reference = "acoustic"): LayoutResult {
  return {
    count,
    start_delay,
    timing: { reference, role: "R", output: 1 },
    arrangements: ARRANGEMENT_COUNTS.map((n) => ({
      count: n,
      points: arrangementPoints(n).map((p, i) => ({ position: i + 1, x_cm: p.x, y_cm: p.y, height: p.h })),
    })),
  };
}

/** Upstream position_seconds with an acoustic reference, 256k at 48 kHz: 2 × 5.46 s + 0.7 s per channel. */
export function estimateOf(channels: number, positions: number, delaySeconds = 0): PositionEstimateResult {
  const per = channels * (2 * (262144 / 48000) + 0.7) + delaySeconds;
  return { positions, per_position_seconds: per, total_seconds: per * positions };
}

const sampleSpeakers = SAMPLE_ROLES.map((role, index) => speaker(role, index + 1));

/** New project: 2.0, nothing measured, no output device chosen yet. */
export const newProjectView: SystemView = {
  outputDeviceState: "unselected",
  config: configOf("2.0", [speaker("L", 1), speaker("R", 2)], ""),
  notices: [],
  dsp: dspOf("camilla"),
  layout: layoutOf(9),
  rew: false,
  estimate: estimateOf(2, 9),
  outputChannels: null,
  outputNames: [],
  positionsDone: [],
};

/** The sample 7.1.4 room with two subs on a 16-output interface; three positions measured. */
export const sampleView: SystemView = {
  outputDeviceState: "selected",
  config: configOf("7.1.4", sampleSpeakers, "MOTU 16A"),
  notices: [],
  dsp: dspOf("camilla"),
  layout: layoutOf(9),
  rew: true,
  estimate: estimateOf(sampleSpeakers.length, 9),
  outputChannels: 16,
  outputNames: [],
  positionsDone: [1, 2, 3],
};

const invalidSpeakers: ConfigSpeaker[] = [
  speaker("L", 1),
  speaker("R", 2),
  speaker("C", 3),
  speaker("SW1", 4),
  speaker("SW2", 5),
  speaker("Ls", 6),
  speaker("Rs", 6),
  speaker("Lb", 7),
  speaker("Rb", 8, { pol: "-" }),
  speaker("Lw", 9),
  speaker("Lw", 10),
  speaker("Tfl", 11),
  speaker("Tfr", 12),
  speaker("Tbl", 14, { bands: [band("SUB", 14)] }),
  speaker("Hfl", 0, { bands: [] }),
  speaker("Hfr", 13, { bands: [band("LF", 13), band("MF", 15)] }),
];

const invalidNotices: Notice[] = [
  { code: "role_duplicate", params: { role: "Lw" } },
  { code: "out_duplicate", params: { out: "6", first_role: "Ls", role: "Rs" } },
  { code: "bands_empty", params: { role: "Hfl" } },
  { code: "band_combination", params: { role: "Hfr", bands: "LF/MF" } },
  { code: "main_with_sub_band", params: { role: "Tbl" } },
];

/** A custom config with problems: a duplicate output, outputs past the device's 12, a speaker without a band. */
export const invalidView: SystemView = {
  outputDeviceState: "selected",
  config: configOf("custom", invalidSpeakers, "Focusrite 18i20"),
  notices: invalidNotices,
  dsp: dspOf("custom", fields(4096, 40960, 16, 8, 8)),
  layout: layoutOf(13, "10 초", "loopback"),
  rew: false,
  estimate: estimateOf(16, 13, 10),
  outputChannels: 12,
  outputNames: [],
  positionsDone: [],
};
