// What the SECS 2R Host service does (ADR 0018, 관리), as the export screen
// and the host shell both draw it: the service's own state, what it keeps
// running or repairs, CamillaDSP's supervision state, the times it reports
// and its log lines in words. Pure: no React, no IPC.
import type { Locale, MessageKey, MessageParams } from "../i18n";
import type { Translate } from "../i18n/react";
import type { HostLogLine } from "../ipc/bindings/HostLogLine";
import type { HostStatus as HostState } from "../ipc/bindings/HostStatus";
import type { ServiceState } from "../ipc/bindings/ServiceState";

/** The generated service status, under the existing screen-facing name. */
export type { HostStatus as HostState } from "../ipc/bindings/HostStatus";

/**
 * The service as a screen names it: not read yet, or its state; a service
 * that runs but does not answer over its pipe reads as unreachable.
 */
export type ServiceView = "checking" | ServiceState;

export function serviceView(service: ServiceState | null, status: HostState | null): ServiceView {
  if (service === null) return "checking";
  return service === "running" && status === null ? "unreachable" : service;
}

export const SERVICE_TEXT: Record<ServiceView, MessageKey> = {
  checking: "host.service.checking",
  not_installed: "host.service.not_installed",
  stopped: "host.service.stopped",
  running: "host.service.running",
  unreachable: "host.service.unreachable",
};

/** What the service does with CamillaDSP: nothing, or the supervision state of the one it manages. */
export type CamillaManaged =
  | { kind: "unmanaged" }
  | { kind: "running"; pid: number | null }
  | { kind: "restarting"; until: string; restarts: number }
  | { kind: "given_up"; reason: string }
  | { kind: "no_session" }
  | { kind: "other_user" }
  | { kind: "stopped" };

export function camillaManaged(host: HostState): CamillaManaged {
  if (!host.managed.camilla) return { kind: "unmanaged" };
  const state = host.camilla.state;
  switch (state.kind) {
    case "running":
      return { kind: "running", pid: host.camilla.pid };
    case "backing_off":
      return { kind: "restarting", until: state.until, restarts: host.camilla.restarts_in_window };
    case "given_up":
      return { kind: "given_up", reason: state.reason };
    case "no_session":
      return { kind: "no_session" };
    case "other_user":
      return { kind: "other_user" };
    case "stopped":
      return { kind: "stopped" };
  }
}

/** A state's glyph: a filled dot (on, dimmed off, danger), a spinner, or the warning sign. */
export type Glyph = "on" | "off" | "busy" | "warning" | "danger";

export const CAMILLA_TEXT: Record<CamillaManaged["kind"], { text: MessageKey; glyph: Glyph }> = {
  unmanaged: { text: "host.unmanaged", glyph: "off" },
  running: { text: "host.camilla.running", glyph: "on" },
  restarting: { text: "host.camilla.restarting", glyph: "busy" },
  given_up: { text: "host.camilla.given_up", glyph: "danger" },
  no_session: { text: "host.camilla.no_session", glyph: "off" },
  other_user: { text: "host.camilla.other_user", glyph: "off" },
  stopped: { text: "host.camilla.stopped", glyph: "off" },
};

/**
 * Why CamillaDSP is not running now, as a sentence, or null when there is
 * nothing to say. `givenUpNext` is the next step where the reader is: the
 * export row's 관리 시작, or SECS 2R from the shell.
 */
export function camillaDetail(t: Translate, managed: CamillaManaged, time: (iso: string) => string, givenUpNext: MessageKey): string | null {
  switch (managed.kind) {
    case "restarting":
      return t("host.camilla.restarting.detail", { time: time(managed.until), count: managed.restarts });
    case "given_up":
      return `${t("host.camilla.given_up.detail")} ${t(givenUpNext)}`;
    case "no_session":
      return t("host.camilla.no_session.detail");
    case "other_user":
      return t("host.camilla.other_user.detail");
    default:
      return null;
  }
}

/** Which targets the service manages now. */
export function managedTargets(host: HostState | null): { camilla: boolean; eqapo: boolean } {
  return { camilla: !!host?.managed.camilla, eqapo: !!host?.managed.eqapo };
}

/** 관리 끄기's one sentence: what stops, for what the service manages (both targets go at once). */
export function stopBody(host: HostState | null): MessageKey {
  const managed = managedTargets(host);
  if (managed.camilla && managed.eqapo) return "host.stop.body.both";
  return managed.camilla ? "host.stop.body.camilla" : "host.stop.body.eqapo";
}

/** A design id as the screens show it: its first eight hex digits, like a short commit hash. */
export function shortId(id: string): string {
  return id.slice(0, 8);
}

const LOCALE_TAG: Record<Locale, string> = { ko: "ko-KR", en: "en-US" };

/**
 * A time the service reports (RFC 3339, UTC) in local time: the clock time
 * when it falls on `now`'s day, else the date and minute. Text that is not a
 * time is shown as it came.
 */
export function clockText(iso: string, now: number, locale: Locale): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  const today = new Date(now);
  const sameDay = at.getFullYear() === today.getFullYear() && at.getMonth() === today.getMonth() && at.getDate() === today.getDate();
  const format = sameDay
    ? new Intl.DateTimeFormat(LOCALE_TAG[locale], { hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23" })
    : new Intl.DateTimeFormat(LOCALE_TAG[locale], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
  return format.format(at);
}

/**
 * The service's log events (`secs2r-host` `log_line`) and the fields each
 * sentence needs. An event not listed here, or one missing a field, reads
 * as an unknown entry; its raw line stays behind 자세히.
 */
const LOG_TEXT: Record<string, { key: MessageKey; fields: readonly string[] }> = {
  launch: { key: "host.log.launch", fields: ["pid"] },
  exit: { key: "host.log.exit", fields: ["code"] },
  restart: { key: "host.log.restart", fields: ["after_ms"] },
  repair: { key: "host.log.repair", fields: [] },
  takeover: { key: "host.log.takeover", fields: ["port"] },
  port_busy: { key: "host.log.port_busy", fields: ["port"] },
  given_up: { key: "host.log.given_up", fields: [] },
  other_user: { key: "host.log.other_user", fields: [] },
  session_logon: { key: "host.log.session_logon", fields: [] },
  session_logoff: { key: "host.log.session_logoff", fields: [] },
  state_corrupt: { key: "host.log.state_corrupt", fields: [] },
  service_error: { key: "host.log.service_error", fields: [] },
};

/** One log line as a sentence: its catalog key and parameters, or the unknown entry. */
export function logSentence(line: HostLogLine, number: (value: number, digits?: number) => string): { key: MessageKey; params: MessageParams; known: boolean } {
  const text = Object.hasOwn(LOG_TEXT, line.event) ? LOG_TEXT[line.event] : undefined;
  const fields = line.fields ?? {};
  if (!text || !text.fields.every((field) => (fields[field] ?? "").trim() !== "")) return { key: "host.log.unknown", params: {}, known: false };
  if (line.event === "restart") {
    const seconds = Number(fields.after_ms) / 1000;
    if (!Number.isFinite(seconds)) return { key: "host.log.unknown", params: {}, known: false };
    return { key: text.key, params: { seconds: number(seconds, Number.isInteger(seconds) ? 0 : 1) }, known: true };
  }
  // Only sentence placeholders leave this mapping; reasons and other raw fields stay in the disclosure.
  return { key: text.key, params: Object.fromEntries(text.fields.map((field) => [field, fields[field]!])), known: true };
}

/** The latest `count` lines, newest first (the service sends them oldest first). */
export function newestFirst(lines: readonly HostLogLine[], count = 50): HostLogLine[] {
  return lines.slice(-count).reverse();
}
