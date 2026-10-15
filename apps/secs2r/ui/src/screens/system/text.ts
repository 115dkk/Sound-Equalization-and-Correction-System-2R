// Catalog keys for the values 시스템 and 연결·점검 show: role names (the
// Dolby channel names upstream shows in both languages), speaker groups,
// output labels, validation notices and DSP device names.
import type { MessageKey, MessageParams } from "../../i18n";
import type { Translate } from "../../i18n/react";
import type { Notice } from "../../ipc/bindings/Notice";
import { ROLES, roleCode } from "../../ui/maps/geometry";

const ROLE_NAME: Record<(typeof ROLES)[number], MessageKey> = {
  L: "system.role.L",
  R: "system.role.R",
  C: "system.role.C",
  SW1: "system.role.SW1",
  SW2: "system.role.SW2",
  SW3: "system.role.SW3",
  SW4: "system.role.SW4",
  SW5: "system.role.SW5",
  SW6: "system.role.SW6",
  SW7: "system.role.SW7",
  SW8: "system.role.SW8",
  Ls: "system.role.Ls",
  Rs: "system.role.Rs",
  Lb: "system.role.Lb",
  Rb: "system.role.Rb",
  Lw: "system.role.Lw",
  Rw: "system.role.Rw",
  Tfl: "system.role.Tfl",
  Tfr: "system.role.Tfr",
  Tml: "system.role.Tml",
  Tmr: "system.role.Tmr",
  Tbl: "system.role.Tbl",
  Tbr: "system.role.Tbr",
  Hfl: "system.role.Hfl",
  Hfr: "system.role.Hfr",
  Hbl: "system.role.Hbl",
  Hbr: "system.role.Hbr",
};

export const isKnownRole = (role: string): role is (typeof ROLES)[number] => (ROLES as readonly string[]).includes(role);

/** The channel's display name ("Left Surround"); roles outside the table are 사용자 정의. */
export function roleName(t: Translate, role: string): string {
  return isKnownRole(role) ? t(ROLE_NAME[role]) : t("system.role.custom");
}

/** "Lrs · Left Rear Surround". */
export const roleLabel = (t: Translate, role: string): string => t("system.role.label", { code: roleCode(role), name: roleName(t, role) });

export const GROUP_NAME: Record<string, MessageKey> = {
  front: "system.group.front",
  center: "system.group.center",
  sub: "system.group.sub",
  surround: "system.group.surround",
  back: "system.group.back",
  wide: "system.group.wide",
  top: "system.group.top",
  height: "system.group.height",
  other: "system.group.other",
};

/** "출력 3", or "3 · Analog 3" when the device names its outputs. */
export function outputLabel(t: Translate, out: number, names: readonly string[]): string {
  const name = names[out - 1];
  return name ? t("system.output.named", { n: out, name }) : t("system.output.n", { n: out });
}

/** `config_validate` notices (secs-project `validation_rows`) as cause and next step. */
const NOTICE_TEXT: Record<string, { cause: MessageKey; next: MessageKey }> = {
  role_duplicate: { cause: "notice.role_duplicate", next: "notice.role_duplicate.next" },
  out_invalid: { cause: "notice.out_invalid", next: "notice.out_invalid.next" },
  out_duplicate: { cause: "notice.out_duplicate", next: "notice.out_duplicate.next" },
  band_unknown: { cause: "notice.band_unknown", next: "notice.band_unknown.next" },
  pol_invalid: { cause: "notice.pol_invalid", next: "notice.pol_invalid.next" },
  bands_empty: { cause: "notice.bands_empty", next: "notice.bands_empty.next" },
  sub_bands: { cause: "notice.sub_bands", next: "notice.sub_bands.next" },
  main_with_sub_band: { cause: "notice.main_with_sub_band", next: "notice.main_with_sub_band.next" },
  band_combination: { cause: "notice.band_combination", next: "notice.band_combination.next" },
};

/** A notice as words; role ids become the codes the screen shows. Unknown codes keep their code for the reader. */
export function noticeText(t: Translate, notice: Notice): { cause: string; next: string | null } {
  const params: MessageParams = { ...notice.params };
  for (const key of ["role", "first_role"]) {
    const value = notice.params[key];
    if (value !== undefined) params[key] = roleCode(value);
  }
  const keys = NOTICE_TEXT[notice.code];
  if (!keys) return { cause: t("notice.unknown", { code: notice.code }), next: null };
  return { cause: t(keys.cause, params), next: t(keys.next, params) };
}

/** DSP device names (the service's DspPreset keys). */
export const DSP_NAME: Record<string, MessageKey> = {
  camilla: "system.dsp.preset.camilla",
  tesira: "system.dsp.preset.tesira",
  qsys: "system.dsp.preset.qsys",
  minidsp: "system.dsp.preset.minidsp",
  custom: "system.dsp.preset.custom",
};

/** "약 2분 31초" / "약 23초". */
export function durationText(t: Translate, seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  const min = Math.floor(total / 60);
  const sec = total % 60;
  return min > 0 ? t("system.duration.min_sec", { min, sec }) : t("system.duration.sec", { sec });
}
