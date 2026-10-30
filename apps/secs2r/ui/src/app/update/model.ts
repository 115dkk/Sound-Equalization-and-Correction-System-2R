// What the update parts say about the updater's phase (ADR 0020), shared by
// the frame's badge and the settings section: the state as a noun phrase,
// its glyph, and when the badge is drawn at all.
import type { Locale, MessageKey } from "../../i18n";
import type { Translate } from "../../i18n/react";
import type { UpdatePhase } from "../../ipc/bindings/UpdatePhase";

/**
 * A filled dot in a state colour (dimmed for off), or a spinner while the
 * updater works. `info` is the in-progress dot for a line that already
 * draws its progress as a bar (the badge).
 */
export type UpdateTone = "off" | "busy" | "info" | "success" | "warning" | "danger";

const TONE: Record<UpdatePhase["kind"], UpdateTone> = {
  off: "off",
  idle: "off",
  checking: "busy",
  up_to_date: "success",
  // A new version the user has to act on: download it (알림만) or replace
  // the portable copy by hand.
  available: "warning",
  portable: "warning",
  downloading: "busy",
  ready: "success",
  installing: "busy",
  failed: "danger",
};

export const phaseTone = (phase: UpdatePhase): UpdateTone => TONE[phase.kind];

/** The phase as a noun phrase ("1.2.0 받는 중", "다음 실행 때 1.2.0 적용"). */
export function phaseText(t: Translate, phase: UpdatePhase): string {
  switch (phase.kind) {
    case "off":
      return t("update.phase.off");
    case "idle":
      return t("update.phase.idle");
    case "checking":
      return t("update.phase.checking");
    case "up_to_date":
      return t("update.phase.up_to_date");
    case "available":
    case "portable":
      return t("update.phase.available", { version: phase.version });
    case "downloading":
      return t("update.phase.downloading", { version: phase.version });
    case "ready":
      return t(phase.install === "on_exit" ? "update.phase.ready.on_exit" : "update.phase.ready.on_request", { version: phase.version });
    case "installing":
      return t("update.phase.installing", { version: phase.version });
    case "failed":
      return t("update.phase.failed");
  }
}

/** The version a phase is about, for `data-version`. */
export function phaseVersion(phase: UpdatePhase): string | undefined {
  return "version" in phase ? phase.version : undefined;
}

/**
 * The frame draws the badge only while something is on its way or waits
 * for the user: a new version not downloaded yet (알림만), a download, a
 * downloaded version, a new version for a portable copy, and a failure
 * that answers a check the user started. The install that follows 지금 다시
 * 시작 stays on it too, so the press shows its effect until the window
 * closes.
 */
export function badgeShown(phase: UpdatePhase, userAction: boolean): boolean {
  switch (phase.kind) {
    case "available":
    case "downloading":
    case "ready":
    case "portable":
      return true;
    case "failed":
    case "installing":
      return userAction;
    default:
      return false;
  }
}

/** The restart button's label: 지금 다시 시작, or 지금 설치 where installing asks for the password (deb). */
export function restartLabel(phase: UpdatePhase): MessageKey {
  return phase.kind === "ready" && phase.install === "on_request" ? "update.install_now" : "update.restart";
}

const LOCALE_TAG: Record<Locale, string> = { ko: "ko-KR", en: "en-US" };

/** When the last check finished (RFC 3339, UTC) in local time, to the minute. Text that is not a time is shown as it came. */
export function checkedText(iso: string, locale: Locale): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  return new Intl.DateTimeFormat(LOCALE_TAG[locale], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).format(at);
}
