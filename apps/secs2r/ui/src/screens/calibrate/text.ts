// Words and numbers of the 보정 screen: mode names, group names, notice
// codes as sentences, sub modes and filter specs. The service sends codes
// and numbers (ADR 0003); these helpers turn them into the current
// language and never compute an engine value.
import { isMessageKey, type MessageKey, type MessageParams } from "../../i18n";
import type { Translate } from "../../i18n/react";
import type { ConfigResult } from "../../ipc/bindings/ConfigResult";
import type { Mode } from "../../ipc/bindings/Mode";
import type { ModeSpecResult } from "../../ipc/bindings/ModeSpecResult";
import type { Notice } from "../../ipc/bindings/Notice";
import { roleCode } from "../../ui/maps/geometry";
import type { CalibrateJob } from "./types";

export type FormatNumber = (value: number, digits?: number) => string;

export const MODES: readonly Mode[] = ["zero", "mixed", "linear", "ultra"];

export const MODE_NAME: Record<Mode, MessageKey> = {
  zero: "calibrate.mode.zero",
  mixed: "calibrate.mode.mixed",
  linear: "calibrate.mode.linear",
  ultra: "calibrate.mode.ultra",
};

export const MODE_EFFECT: Record<Mode, MessageKey> = {
  zero: "calibrate.mode.zero.effect",
  mixed: "calibrate.mode.mixed.effect",
  linear: "calibrate.mode.linear.effect",
  ultra: "calibrate.mode.ultra.effect",
};

export const RUNNING_TEXT: Record<CalibrateJob, MessageKey> = {
  calibrate: "calibrate.running.calibrate",
  calibrate_subs: "calibrate.running.calibrate_subs",
  recommend_xo: "calibrate.running.recommend_xo",
  mimo: "calibrate.running.mimo",
};

export const FAILED_TEXT: Record<CalibrateJob, MessageKey> = {
  calibrate: "calibrate.failed.calibrate",
  calibrate_subs: "calibrate.failed.calibrate_subs",
  recommend_xo: "calibrate.failed.recommend_xo",
  mimo: "calibrate.failed.mimo",
};

/** "낮은 지연" or, for 초고지연, "초고지연 120 ms". */
export function modeName(t: Translate, number: FormatNumber, mode: Mode, ultraMs?: number): string {
  if (mode === "ultra" && ultraMs !== undefined) return t("calibrate.mode.ultra_ms", { ms: number(ultraMs) });
  return t(MODE_NAME[mode]);
}

/** The mode's spec line from `mode_spec` (upstream `cal.mode_spec`). */
export function modeSpecText(t: Translate, number: FormatNumber, spec: ModeSpecResult): string {
  const mimo = spec.mimo_filter.toUpperCase();
  if (spec.fir_taps <= 0) return t("calibrate.mode.spec.iir", { mimo, ms: number(spec.max_latency_ms) });
  return t("calibrate.mode.spec.fir", { taps: number(spec.fir_taps), mimo, ms: number(spec.max_latency_ms) });
}

/* ── Speaker groups ─────────────────────────────────────────────────────── */

/** Configuration group keys (the model's Korean names, stored as is in .secs) to catalog keys. */
const GROUP_KEY: Readonly<Record<string, MessageKey>> = {
  프론트: "calibrate.group.front",
  센터: "calibrate.group.center",
  서브: "calibrate.group.sub",
  서라운드: "calibrate.group.surround",
  백: "calibrate.group.back",
  와이드: "calibrate.group.wide",
  탑: "calibrate.group.top",
  하이트: "calibrate.group.height",
  기타: "calibrate.group.other",
};

export const SUB_GROUP = "서브";

export function groupName(t: Translate, group: string): string {
  const key = GROUP_KEY[group];
  return key ? t(key) : group;
}

/** Sub roles in configuration order. */
export function subRoles(config: ConfigResult): string[] {
  return config.speakers.filter((s, i) => s.size === "sub" || config.groups.some((g) => g.key === SUB_GROUP && g.speaker_indices.includes(i))).map((s) => s.role);
}

/** Groups that get a crossover: every main group, when the configuration has subs (upstream `xo_group_names`). */
export function xoGroups(config: ConfigResult): string[] {
  if (subRoles(config).length === 0) return [];
  return config.groups.filter((g) => g.key !== SUB_GROUP).map((g) => g.key);
}

/** The group of a role (null when unknown). */
export function groupOf(config: ConfigResult, role: string): string | null {
  const index = config.speakers.findIndex((s) => s.role === role);
  return config.groups.find((g) => g.speaker_indices.includes(index))?.key ?? null;
}

/* ── Notices ────────────────────────────────────────────────────────────── */

const num = (value: string | undefined) => (value === undefined ? Number.NaN : Number(value));

function list(value: string | undefined): unknown[] {
  try {
    const parsed: unknown = JSON.parse(value ?? "[]");
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

/**
 * A service notice as a sentence in the current language, or null when the
 * screen has no sentence for it (the expert view shows those raw).
 */
export function noticeText(t: Translate, number: FormatNumber, notice: Notice): string | null {
  const p = notice.params;
  const f = (key: string, digits = 0) => {
    const value = num(p[key]);
    return Number.isFinite(value) ? number(value, digits) : (p[key] ?? "");
  };
  const signed = (key: string, digits = 1) => {
    const value = num(p[key]);
    if (!Number.isFinite(value)) return p[key] ?? "";
    return `${value > 0 ? "+" : ""}${number(value, digits)}`;
  };
  const code = (key = "role") => roleCode(p[key] ?? "");
  const params: MessageParams = {};
  let key: string = `notice.${notice.code}`;
  switch (notice.code) {
    case "rc_limit":
      params.hz = f("hz");
      if (p.auto === "true" && p.schroeder_hz !== undefined) {
        key = "notice.rc_limit.schroeder";
        params.schroeder_hz = f("schroeder_hz");
      } else key = p.auto === "true" ? "notice.rc_limit.auto" : "notice.rc_limit.manual";
      break;
    case "rc_room_volume":
      params.volume_m3 = f("volume_m3");
      break;
    case "rc_auto_cap":
      params.hz = f("hz");
      break;
    case "sub_narrow_band":
      Object.assign(params, { code: code(), hp_hz: f("hp_hz"), lp_hz: f("lp_hz") });
      break;
    case "sub_high_gain":
      Object.assign(params, { code: code(), gain_db: signed("gain_db") });
      break;
    case "lift_capped":
      Object.assign(params, { from_ms: f("from_ms", 1), taps: f("taps"), lift_ms: f("lift_ms", 1) });
      break;
    case "latency":
      Object.assign(params, { latency_ms: f("latency_ms", 1), lift_ms: f("lift_ms", 1), main_extra_ms: f("main_extra_ms", 1) });
      break;
    case "long_mode_boost": {
      const modes = list(p.modes)
        .map((m) => (Array.isArray(m) && typeof m[0] === "number" ? `${number(m[0])} Hz` : null))
        .filter(Boolean);
      Object.assign(params, { cap_db: f("cap_db", 1), modes: modes.join(", "), count: modes.length });
      break;
    }
    case "gd_cuts":
      Object.assign(params, { code: code(), count: list(p.cuts).length });
      break;
    case "two_way": {
      const roles = list(p.roles).map((r) => roleCode(String(r)));
      Object.assign(params, { roles: roles.join("·"), gain_db: signed("gain_db"), xo_hz: f("xo_hz") });
      break;
    }
    case "bus_gain_anchored":
      params.value = signed("value");
      break;
    case "layout_fallback":
    case "layout":
      params.value = p.value ?? "";
      break;
  }
  return isMessageKey(key) ? t(key, params) : null;
}

/* ── Subs ───────────────────────────────────────────────────────────────── */

/** 'LR24' → 'LR 24' (dB/oct), as upstream `band_type_text`. */
export const bandTypeText = (type: string) => `${type.slice(0, 2)} ${type.slice(2)}`;

/** A result row's mode code (`auto`, `fixed`, `bundle:A`, `bundle:A:dba-rear` …) as words. */
export function subModeText(t: Translate, mode: string): string {
  if (mode === "fixed") return t("calibrate.ba.mode.fixed");
  if (mode.startsWith("bundle:")) {
    const [, bundle = "A", place] = mode.split(":");
    if (place === "multi") return t("calibrate.ba.mode.bundle_multi", { bundle });
    if (place === "dba-front") return t("calibrate.ba.mode.bundle_dba_front", { bundle });
    if (place === "dba-rear") return t("calibrate.ba.mode.bundle_dba_rear", { bundle });
    return t("calibrate.ba.mode.bundle", { bundle });
  }
  return t("calibrate.ba.mode.auto");
}

/** "HP 25 Hz LR 24 · LP 80 Hz LR 24 · PEQ 38 Hz Q 5.00 −4.0 dB" for one sub's fixed filters; "" without any. */
export function filterSpec(
  t: Translate,
  number: FormatNumber,
  high: [number, string],
  low: [number, string],
  peq: readonly (readonly [number, number, number])[],
): string {
  const parts: string[] = [];
  if (high[0] > 0) parts.push(t("calibrate.spec.hp", { hz: number(high[0]), type: bandTypeText(high[1]) }));
  if (low[0] > 0) parts.push(t("calibrate.spec.lp", { hz: number(low[0]), type: bandTypeText(low[1]) }));
  for (const [hz, q, gain] of peq) parts.push(t("calibrate.spec.peq", { hz: number(hz), q: number(q, 2), gain: signedNumber(number, gain, 1) }));
  return parts.join(" · ");
}

/** "+1.5" / "−3.0": a sign on every value but zero. */
export function signedNumber(number: FormatNumber, value: number, digits: number): string {
  const text = number(value, digits);
  return value > 0 && text !== number(0, digits) ? `+${text}` : text;
}
