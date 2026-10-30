import { describe, expect, it } from "vitest";
import { translate, type MessageKey } from "../i18n";
import type { HostLogLine } from "../ipc/bindings/HostLogLine";
import type { HostStatus } from "../ipc/bindings/HostStatus";
import { hostOtherUser } from "./fixtures";
import { camillaManaged, logSentence } from "./model";

const number = (value: number, digits = 0) => value.toFixed(digits);
const line = (event: string, fields: Record<string, string> = {}): HostLogLine => ({ timestamp: "2026-10-29T12:00:00Z", event, fields });

const events: { event: string; fields: Record<string, string>; key: MessageKey; params: Record<string, string> }[] = [
  { event: "launch", fields: { pid: "4812", design_id: "raw-design" }, key: "host.log.launch", params: { pid: "4812" } },
  { event: "exit", fields: { code: "1", pid: "4812" }, key: "host.log.exit", params: { code: "1" } },
  { event: "restart", fields: { after_ms: "4000", reason: "port_busy" }, key: "host.log.restart", params: { seconds: "4" } },
  { event: "repair", fields: { path: "raw-path" }, key: "host.log.repair", params: {} },
  { event: "takeover", fields: { port: "1234" }, key: "host.log.takeover", params: { port: "1234" } },
  { event: "port_busy", fields: { port: "1234" }, key: "host.log.port_busy", params: { port: "1234" } },
  { event: "given_up", fields: { reason: "port_busy" }, key: "host.log.given_up", params: {} },
  { event: "other_user", fields: { session: "7" }, key: "host.log.other_user", params: {} },
  { event: "session_logon", fields: { session: "7" }, key: "host.log.session_logon", params: {} },
  { event: "session_logoff", fields: { session: "7" }, key: "host.log.session_logoff", params: {} },
  { event: "state_corrupt", fields: { detail: "raw-error" }, key: "host.log.state_corrupt", params: {} },
  { event: "service_error", fields: { detail: "raw-error" }, key: "host.log.service_error", params: {} },
];

describe("host log sentences", () => {
  it.each(events)("maps $event with only its sentence parameters in both catalogs", ({ event, fields, key, params }) => {
    const entry = line(event, fields);
    const original = structuredClone(entry);
    const sentence = logSentence(entry, number);
    expect(sentence).toEqual({ key, params, known: true });
    for (const locale of ["ko", "en"] as const) {
      expect(translate(locale, sentence.key, sentence.params)).not.toMatch(/\{\w+\}|raw-error|raw-path|raw-design|port_busy/);
    }
    expect(entry).toEqual(original);
  });

  it.each(["takeover", "port_busy", "launch", "exit", "restart"])("keeps %s without required fields unknown", (event) => {
    expect(logSentence(line(event), number)).toEqual({ key: "host.log.unknown", params: {}, known: false });
  });

  it("keeps missing raw details optional and never uses them as sentence parameters", () => {
    for (const event of ["given_up", "other_user"]) {
      const entry: HostLogLine = { timestamp: "2026-10-29T12:00:00Z", event };
      expect(logSentence(entry, number)).toMatchObject({ known: true, params: {} });
    }
  });

  it("keeps unknown events and invalid restart durations unknown", () => {
    for (const entry of [line("toString"), line("future_event"), line("restart", { after_ms: "not a number" }), line("restart", { after_ms: " " }), line("port_busy", { port: " " })]) {
      expect(logSentence(entry, number)).toEqual({ key: "host.log.unknown", params: {}, known: false });
    }
    expect(logSentence(line("restart", { after_ms: "1500" }), number).params).toEqual({ seconds: "1.5" });
  });
});

it("accepts other_user from the generated HostStatus without widening its process state", () => {
  const status: HostStatus = hostOtherUser;
  expect(camillaManaged(status)).toEqual({ kind: "other_user" });
});
