// @vitest-environment happy-dom
// The host shell's page (host.html, ADR 0018) on the mock HostShellApi:
// polling while visible, each service state, what is managed, the log, and
// the actions with their confirms.
import { StrictMode } from "react";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { translate } from "../i18n";
import type { Translate } from "../i18n/react";
import type { HostLogLine } from "../ipc/bindings/HostLogLine";
import * as fx from "../ipc/fixtures";
import { renderWith } from "../test/render";
import { HOST_NOW, LOG_GIVEN_UP, LOG_MANAGING, hostGivenUp, hostManagingRepaired } from "./fixtures";
import { HostShell, HostShellPage } from "./HostShell";
import { createMockHostApi } from "./mockApi";
import { clockText } from "./model";
import { SHELL_GIVEN_UP, SHELL_MANAGING, SHELL_NOT_INSTALLED } from "./specimens";
import type { HostShellActions, HostShellView } from "./useHostShell";

const t: Translate = (key, params) => translate("ko", key, params);
const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const button = (id: string) => ui(id) as HTMLButtonElement;
const noop: HostShellActions = { install: () => {}, uninstall: () => {}, stopManaging: () => {}, openApp: () => {} };
const page = (view: HostShellView, actions = noop) => renderWith(<HostShellPage view={view} actions={actions} />);

let visibility: DocumentVisibilityState = "visible";
Object.defineProperty(document, "visibilityState", { configurable: true, get: () => visibility });
const setVisibility = (state: DocumentVisibilityState) => {
  visibility = state;
  document.dispatchEvent(new Event("visibilitychange"));
};

afterEach(() => {
  vi.useRealTimers();
  visibility = "visible";
});

describe("host shell", () => {
  it.each(["ko", "en"] as const)("sets %s tray labels once at startup, even hidden and in StrictMode", async (locale) => {
    vi.useFakeTimers();
    visibility = "hidden";
    const api = createMockHostApi();
    renderWith(<StrictMode><HostShell api={api} /></StrictMode>, { locale });
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(api.calls).toEqual(["trayLabels"]);
    expect(api.trayLabelCalls).toEqual([{
      open: translate(locale, "host.tray.open"),
      open_app: translate(locale, "host.tray.open_app"),
      quit: translate(locale, "host.tray.quit"),
    }]);
    act(() => setVisibility("visible"));
    await act(() => vi.advanceTimersByTimeAsync(4000));
    expect(api.calls.filter((call) => call === "status")).toHaveLength(3);
    expect(api.calls.filter((call) => call === "trayLabels")).toHaveLength(1);
  });

  it("keeps polling if setting native tray labels fails", async () => {
    const api = createMockHostApi();
    api.failNext("trayLabels", { code: "internal", params: {}, retryable: false, detail: "tray unavailable" });
    renderWith(<HostShell api={api} intervalMs={60_000} />);
    await waitFor(() => expect(ui("host.service")?.textContent).toBe("설치 안 됨"));
    expect(api.calls).toEqual(["trayLabels", "status"]);
    expect(ui("host.failure")).toBeNull();
  });

  it.each(["ko", "en"] as const)("shows all added log events in %s, with reasons only in closed details", (locale) => {
    const log: HostLogLine[] = [
      { timestamp: "2026-10-29T12:00:00Z", event: "takeover", fields: { port: "1234" } },
      { timestamp: "2026-10-29T12:00:01Z", event: "port_busy", fields: { port: "1234" } },
      { timestamp: "2026-10-29T12:00:02Z", event: "given_up", fields: { reason: "port_busy: raw service reason" } },
      { timestamp: "2026-10-29T12:00:03Z", event: "other_user", fields: { session: "7" } },
    ];
    renderWith(<HostShellPage view={{ ...SHELL_MANAGING, log }} actions={noop} />, { locale });
    const rows = within(ui("host.log")!).getAllByRole("listitem");
    const expected = ["host.log.other_user", "host.log.given_up", "host.log.port_busy", "host.log.takeover"] as const;
    rows.forEach((row, index) => {
      expect(row.querySelector(".host-log__text")?.textContent).toBe(translate(locale, expected[index]!, { port: "1234" }));
      expect(row.querySelector("summary")?.textContent).not.toContain("raw service reason");
      expect(row.querySelector("details")?.open).toBe(false);
      expect(JSON.parse(row.querySelector("pre")!.textContent!)).toEqual(log[log.length - 1 - index]);
    });
  });

  it("draws every state in both languages with every placeholder filled", () => {
    for (const locale of ["ko", "en"] as const) {
      for (const view of [SHELL_NOT_INSTALLED, SHELL_MANAGING, SHELL_GIVEN_UP, { ...SHELL_MANAGING, host: { ...fx.hostIdle } }]) {
        const { unmount } = renderWith(<HostShellPage view={view} actions={noop} />, { locale });
        expect(document.body.textContent).not.toMatch(/\{\w+\}/);
        unmount();
      }
    }
  });

  it("reads the service at once and every 2 s while the window is visible, and not while it is hidden", async () => {
    vi.useFakeTimers();
    const api = createMockHostApi({ service: "running", host: fx.hostManaging, log: LOG_MANAGING });
    renderWith(<HostShell api={api} />);
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(api.calls).toEqual(["trayLabels", "status", "log"]);
    expect(ui("host.service")?.textContent).toBe("실행 중");
    await act(() => vi.advanceTimersByTimeAsync(2000));
    expect(api.calls).toEqual(["trayLabels", "status", "log", "status", "log"]);
    act(() => setVisibility("hidden"));
    await act(() => vi.advanceTimersByTimeAsync(10_000));
    expect(api.calls).toHaveLength(5);
    // Shown again: read at once, then every 2 s again.
    act(() => setVisibility("visible"));
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(api.calls).toHaveLength(7);
    await act(() => vi.advanceTimersByTimeAsync(2000));
    expect(api.calls).toHaveLength(9);
    // A service that is not installed has no log to read.
    api.set("not_installed");
    await act(() => vi.advanceTimersByTimeAsync(2000));
    expect(api.calls.slice(-2)).toEqual(["log", "status"]);
    expect(ui("host.service")?.textContent).toBe("설치 안 됨");
  });

  it("without the service: why it is needed, 서비스 설치 filled, and the page reads it running once installed", async () => {
    const api = createMockHostApi();
    renderWith(<HostShell api={api} intervalMs={60_000} />);
    await waitFor(() => expect(ui("host.service")?.textContent).toBe("설치 안 됨"));
    expect(ui("host.note")?.textContent).toBe(t("host.install.why"));
    const install = button("host.install");
    expect(install.classList.contains("ui-button--primary")).toBe(true);
    expect(button("host.open_app").classList.contains("ui-button--secondary")).toBe(true);
    // Nothing to stop or remove yet.
    expect(ui("host.stop")).toBeNull();
    expect(ui("host.uninstall")).toBeNull();
    fireEvent.click(install);
    await waitFor(() => expect(ui("host.service")?.textContent).toBe("실행 중"));
    expect(api.calls).toEqual(["trayLabels", "status", "install", "status", "log"]);
    // Running, nothing managed: where to start it, SECS 2R 열기 filled now.
    expect(ui("host.nothing")?.textContent).toContain("관리하는 대상 없음");
    expect(button("host.open_app").classList.contains("ui-button--primary")).toBe(true);
    expect(ui("host.install")).toBeNull();
    expect(ui("host.uninstall")).not.toBeNull();
    expect(ui("host.stop")).toBeNull();
  });

  it("says a declined install failed, keeps the raw text behind 자세히, and clears it on the next try", async () => {
    const api = createMockHostApi();
    api.failNext("install", { code: "apply_failed", params: {}, retryable: false, detail: "elevated install: The operation was canceled by the user. (os error 1223)" });
    renderWith(<HostShell api={api} intervalMs={60_000} />);
    await waitFor(() => expect(ui("host.install")).not.toBeNull());
    fireEvent.click(button("host.install"));
    await waitFor(() => expect(ui("host.failure")).not.toBeNull());
    expect(ui("host.failure")?.textContent).toContain("서비스를 설치하지 못했습니다.");
    expect(ui("host.failure")?.textContent).toContain(t("host.install.failed.next"));
    expect(ui("host.failure")?.querySelector("details pre")?.textContent).toBe("[apply_failed] elevated install: The operation was canceled by the user. (os error 1223)");
    expect(ui("host.service")?.textContent).toBe("설치 안 됨");
    fireEvent.click(button("host.install"));
    await waitFor(() => expect(ui("host.service")?.textContent).toBe("실행 중"));
    expect(ui("host.failure")).toBeNull();
  });

  it("shows what the service manages: CamillaDSP's state, program, port and design, Equalizer APO's line, last repair and folder", () => {
    page(SHELL_MANAGING);
    expect(ui("host.service")?.textContent).toBe("실행 중");
    expect(screen.getByText(t("host.shell.version", { version: fx.hostIdle.version }))).toBeTruthy();
    const camilla = ui("host.camilla")!;
    expect(ui("host.camilla.state")?.textContent).toBe("실행 중PID 4812");
    expect(ui("host.camilla.exe")?.textContent).toBe(fx.CAMILLA_EXE);
    expect(within(camilla).getByText("1234")).toBeTruthy();
    const design = within(camilla).getByText(fx.MOCK_DESIGN_ID.slice(0, 8));
    expect(design.getAttribute("title")).toBe(fx.MOCK_DESIGN_ID);
    expect(ui("host.eqapo.line")?.textContent).toBe("SECS 2R 줄 있음");
    expect(ui("host.eqapo.repair")?.textContent).toBe(clockText(hostManagingRepaired.last_repair!, HOST_NOW, "ko"));
    expect(ui("host.eqapo.folder")?.textContent).toBe(fx.managedEqapo.config_path);
    // 관리 끄기 and 서비스 제거 quiet; SECS 2R 열기 is the one filled action.
    expect(button("host.stop").classList.contains("ui-button--quiet")).toBe(true);
    expect(button("host.uninstall").classList.contains("ui-button--quiet")).toBe(true);
    expect(document.querySelectorAll(".ui-button--primary")).toHaveLength(1);
  });

  it("lists the latest 50 log lines newest first, each a time and a sentence with the raw line behind its disclosure", () => {
    page(SHELL_MANAGING);
    const rows = within(ui("host.log")!).getAllByRole("listitem");
    expect(rows.map((row) => row.querySelector(".host-log__text")?.textContent)).toEqual([
      "config.txt에 SECS 2R 줄을 다시 넣었습니다.",
      "CamillaDSP를 시작했습니다(PID 4812).",
      "사용자가 로그인했습니다.",
    ]);
    const newest = LOG_MANAGING.at(-1)!;
    expect(rows[0]!.querySelector("time")?.textContent).toBe(clockText(newest.timestamp, HOST_NOW, "ko"));
    // The raw JSON is in the closed disclosure, not in the sentence.
    const details = rows[0]!.querySelector("details")!;
    expect(details.open).toBe(false);
    expect(JSON.parse(details.querySelector("pre")!.textContent!)).toEqual(newest);
    expect(rows[0]!.querySelector("summary")?.textContent).not.toContain("{");
    // Restarts read in seconds; an unknown event says so, its raw line kept.
    const long: HostLogLine[] = [
      ...Array.from({ length: 60 }, (_, index) => ({ timestamp: new Date(HOST_NOW - (100 - index) * 1000).toISOString(), event: "session_logon", fields: { session: String(index) } })),
      { timestamp: new Date(HOST_NOW - 2000).toISOString(), event: "restart", fields: { after_ms: "4000" } },
      { timestamp: new Date(HOST_NOW - 1000).toISOString(), event: "telemetry_flush", fields: {} },
    ];
    document.body.innerHTML = "";
    page({ ...SHELL_MANAGING, log: long });
    const all = within(ui("host.log")!).getAllByRole("listitem");
    expect(all).toHaveLength(50);
    expect(all[0]!.querySelector(".host-log__text")?.textContent).toBe("알 수 없는 기록");
    expect(all[0]!.querySelector("pre")?.textContent).toContain("telemetry_flush");
    expect(all[1]!.querySelector(".host-log__text")?.textContent).toBe("CamillaDSP를 4초 뒤 다시 시작합니다.");
  });

  it("says CamillaDSP is given up with the next step in SECS 2R and the service's reason behind 자세히", () => {
    page(SHELL_GIVEN_UP);
    expect(ui("host.camilla.state")?.textContent).toBe("다시 시작 중단");
    expect(ui("host.camilla.reason")?.textContent).toBe(`${t("host.camilla.given_up.detail")} ${t("host.camilla.given_up.next.shell")}`);
    expect(ui("host.camilla")?.querySelector("details pre")?.textContent).toBe(hostGivenUp.camilla.state.kind === "given_up" ? hostGivenUp.camilla.state.reason : "");
    expect(within(ui("host.log")!).getAllByRole("listitem")[0]!.querySelector(".host-log__text")?.textContent).toBe(t("host.log.given_up"));
    expect(within(ui("host.log")!).getAllByRole("listitem")).toHaveLength(LOG_GIVEN_UP.length);
  });

  it("asks once before 서비스 제거 and 관리 끄기, then runs them and reads the service again", async () => {
    const api = createMockHostApi({ service: "running", host: fx.hostManaging, log: LOG_MANAGING });
    renderWith(<HostShell api={api} intervalMs={60_000} />);
    await waitFor(() => expect(ui("host.stop")).not.toBeNull());
    fireEvent.click(button("host.stop"));
    let dialog = screen.getByRole("dialog");
    expect(dialog).toBe(ui("host.stop.confirm"));
    expect(dialog.querySelector(".ui-dialog__body")?.textContent).toBe(t("host.stop.body.both"));
    fireEvent.click(ui("host.stop.confirm.cancel")!);
    expect(api.calls).not.toContain("stopManaging");
    fireEvent.click(button("host.stop"));
    fireEvent.click(ui("host.stop.confirm.confirm")!);
    await waitFor(() => expect(ui("host.nothing")).not.toBeNull());
    expect(api.calls).toContain("stopManaging");
    expect(ui("host.stop")).toBeNull();
    fireEvent.click(button("host.uninstall"));
    dialog = screen.getByRole("dialog");
    expect(dialog).toBe(ui("host.uninstall.confirm"));
    expect(within(dialog).getByRole("heading").textContent).toBe("SECS 2R Host 서비스 제거");
    expect(dialog.querySelector(".ui-dialog__body")?.textContent).toBe(t("host.uninstall.body"));
    expect(button("host.uninstall.confirm.confirm").textContent).toBe("서비스 제거");
    fireEvent.click(ui("host.uninstall.confirm.cancel")!);
    expect(api.calls).not.toContain("uninstall");
    fireEvent.click(button("host.uninstall"));
    fireEvent.click(ui("host.uninstall.confirm.confirm")!);
    await waitFor(() => expect(ui("host.service")?.textContent).toBe("설치 안 됨"));
    expect(api.calls).toContain("uninstall");
    expect(ui("host.install")).not.toBeNull();
  });

  it("opens SECS 2R, and says so when it could not", async () => {
    const api = createMockHostApi({ service: "running", host: fx.hostIdle });
    renderWith(<HostShell api={api} intervalMs={60_000} />);
    await waitFor(() => expect(ui("host.service")?.textContent).toBe("실행 중"));
    fireEvent.click(button("host.open_app"));
    await waitFor(() => expect(api.calls).toContain("openApp"));
    expect(ui("host.failure")).toBeNull();
    api.failNext("openApp", { code: "internal", params: {}, retryable: false, detail: "spawn SECS 2R.exe: not found" });
    fireEvent.click(button("host.open_app"));
    await waitFor(() => expect(ui("host.failure")?.textContent).toContain("SECS 2R을 열지 못했습니다."));
  });

  it("warns when the service cannot be read, keeps what it showed, and says what to do for a stopped service", async () => {
    vi.useFakeTimers();
    const api = createMockHostApi({ service: "running", host: fx.hostManaging, log: LOG_MANAGING });
    renderWith(<HostShell api={api} />);
    await act(() => vi.advanceTimersByTimeAsync(0));
    api.failNext("status", { code: "internal", params: {}, retryable: false, detail: "host_status: pipe closed" });
    await act(() => vi.advanceTimersByTimeAsync(2000));
    expect(ui("host.read_error")?.textContent).toContain("서비스 상태를 읽지 못했습니다.");
    expect(ui("host.camilla")).not.toBeNull();
    // The next read clears it.
    await act(() => vi.advanceTimersByTimeAsync(2000));
    expect(ui("host.read_error")).toBeNull();
    api.set("stopped");
    await act(() => vi.advanceTimersByTimeAsync(2000));
    expect(ui("host.service")?.textContent).toBe("정지됨");
    expect(ui("host.note")?.textContent).toBe(t("host.stopped.next"));
    expect(ui("host.log")).toBeNull();
    expect(ui("host.uninstall")).not.toBeNull();
  });
});
