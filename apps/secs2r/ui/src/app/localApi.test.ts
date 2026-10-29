// The local API's pure checks: the port draft, the state line and the
// request reference.
import { describe, expect, it } from "vitest";
import type { LocalApiStatus } from "../ipc/bindings/LocalApiStatus";
import { translate } from "../i18n";
import { errorText } from "./jobs/jobText";
import { hostOf, localApiState, parsePort, portProblem, referenceText, type LocalApiView } from "./localApi";

const status = (patch: Partial<LocalApiStatus> = {}): LocalApiStatus => ({
  enabled: true,
  port: 4712,
  running: true,
  error: null,
  url: "http://127.0.0.1:4712",
  token: "0123456789abcdef".repeat(4),
  token_path: "C:\\sample\\local-api-token",
  ...patch,
});
const view = (patch: Partial<LocalApiView> = {}): LocalApiView => ({ status: status(), pending: null, error: null, ...patch });

it("gives ui_only a specific cause and next action in both locales", () => {
  for (const locale of ["ko", "en"] as const) {
    const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) => translate(locale, key, params);
    expect(errorText(t, { code: "ui_only", params: {}, retryable: false, detail: "raw internal detail" })).toEqual({
      cause: t("error.ui_only"), next: t("error.ui_only.next"), generic: false,
    });
  }
});

describe("portProblem", () => {
  it("accepts whole numbers from 1024 to 65535, spaces around them included", () => {
    for (const text of ["1024", "4712", "65535", " 8080 ", "04712"]) expect(portProblem(text), text).toBeNull();
    expect(parsePort(" 8080 ")).toBe(8080);
    expect(parsePort("04712")).toBe(4712);
  });

  it("names an empty draft", () => {
    expect(portProblem("")).toBe("empty");
    expect(portProblem("   ")).toBe("empty");
  });

  it("names anything but digits", () => {
    for (const text of ["abc", "47.12", "-1", "+4712", "4 712", "4712a", "1e4"]) expect(portProblem(text), text).toBe("not_number");
  });

  it("names ports outside 1024 to 65535", () => {
    for (const text of ["0", "80", "1023", "65536", "99999999999999999999"]) expect(portProblem(text), text).toBe("out_of_range");
    expect(parsePort("80")).toBeNull();
  });
});

describe("localApiState", () => {
  it("is unknown until the first status, and unreadable when the read failed", () => {
    expect(localApiState(view({ status: null }))).toEqual({ kind: "unknown" });
    expect(localApiState(view({ status: null, error: { code: "internal", params: {}, retryable: true, detail: "x" } }))).toEqual({ kind: "unreadable" });
  });

  it("is off, or listening on the address without its scheme", () => {
    expect(localApiState(view({ status: status({ enabled: false, running: false }) }))).toEqual({ kind: "off" });
    expect(localApiState(view())).toEqual({ kind: "listening", address: "127.0.0.1:4712" });
  });

  it("says which way the switch is going while its save is in flight", () => {
    expect(localApiState(view({ status: status({ enabled: false, running: false }), pending: "toggle" }))).toEqual({ kind: "turning_on" });
    expect(localApiState(view({ pending: "toggle" }))).toEqual({ kind: "turning_off" });
    expect(localApiState(view({ pending: "port" }))).toEqual({ kind: "listening", address: "127.0.0.1:4712" });
  });

  it("reads a taken port out of the listener's failure and keeps any other failure's text", () => {
    expect(localApiState(view({ status: status({ running: false, error: "local API port 5000 is busy" }) }))).toEqual({ kind: "port_busy", port: 5000 });
    const other = "local API listener on port 4712: access denied (os error 10013)";
    expect(localApiState(view({ status: status({ running: false, error: other }) }))).toEqual({ kind: "failed", detail: other });
    expect(localApiState(view({ status: status({ running: false }) }))).toEqual({ kind: "failed", detail: "" });
  });

  it("strips the scheme and a trailing slash", () => {
    expect(hostOf("http://127.0.0.1:4712/")).toBe("127.0.0.1:4712");
    expect(hostOf("127.0.0.1:4712")).toBe("127.0.0.1:4712");
  });
});

describe("referenceText", () => {
  it("lists the base address, the three entry points in one column and the header with a placeholder", () => {
    const text = referenceText("http://127.0.0.1:4712", { what: (key) => key.split(".").pop() ?? "", auth: "Authorization: Bearer <token>" });
    expect(text.split("\n")).toEqual([
      "http://127.0.0.1:4712",
      "POST /v1/call     call",
      "GET  /v1/status   status",
      "GET  /v1/events   events",
      "Authorization: Bearer <token>",
    ]);
  });
});
