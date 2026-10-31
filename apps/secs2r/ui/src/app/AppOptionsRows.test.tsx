// @vitest-environment happy-dom
// 낡은 결과 자동 갱신 (ADR 0029) and the log folder (ADR 0027) in the
// settings popover: where they sit, what they send, and the live hook over
// the session (existing calls only: the app settings writer, open_folder).
import { act, cleanup, fireEvent, render, renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { afterEach, describe, expect, it, vi, type Mock } from "vitest";
import { bootstrapInfo } from "../ipc/fixtures";
import ko from "../i18n/ko.json";
import { createMockService } from "../ipc/mock";
import { SessionProvider } from "../state/react";
import { createSession, type Session } from "../state/session";
import { useAppOptions, type AppOptions, type AppOptionsActions, type AppOptionsView } from "./appOptions";
import { SettingsButton } from "./menus";
import { PrefsScope } from "./prefs";

afterEach(cleanup);

const LOGS = "C:\\Users\\sample\\AppData\\Roaming\\io.github.115dkk.secs2r\\logs";

function makeOptions(view: Partial<AppOptionsView> = {}): AppOptions & { actions: { [K in keyof AppOptionsActions]: Mock<AppOptionsActions[K]> } } {
  return {
    view: { autoRefresh: false, saving: false, logDir: LOGS, ...view },
    actions: { setAutoRefresh: vi.fn<AppOptionsActions["setAutoRefresh"]>(), openLogFolder: vi.fn<AppOptionsActions["openLogFolder"]>() },
  };
}

function popover(options: AppOptions, locale: "ko" | "en" = "ko") {
  return render(
    <PrefsScope initial={{ locale, theme: "light", accent: "default" }}>
      <SettingsButton initiallyOpen inline appOptions={options} />
    </PrefsScope>,
  );
}

const ui = (id: string) => {
  const node = document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
  if (!node) throw new Error(`missing [data-ui="${id}"]`);
  return node;
};

describe("settings popover", () => {
  it("puts 낡은 결과 자동 갱신 after the display settings and the log folder last", () => {
    popover(makeOptions());
    const panel = ui("frame.settings.panel");
    const order = [...panel.children].map((node) => node.getAttribute("data-ui") ?? node.tagName.toLowerCase());
    expect(order.indexOf("app.auto_refresh.row")).toBe(order.indexOf("frame.settings.local_api.row") - 1);
    expect(order.at(-1)).toBe("app.log");
    expect(ui("app.log.open").textContent).toBe("로그 폴더 열기");
    expect(ui("app.log.open").getAttribute("aria-describedby")).toBe("app.log.help");
    expect(document.getElementById("app.log.help")?.textContent).toBe(ko["app.log.help"]);
  });

  it("switches auto_refresh, off by default, with what it does behind its help button", () => {
    const options = makeOptions();
    popover(options);
    const toggle = ui("app.auto_refresh");
    expect(toggle.getAttribute("role")).toBe("switch");
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(toggle.textContent).toBe("낡은 결과 자동 갱신");
    expect(document.getElementById("app.auto_refresh.help-content")?.textContent).toMatch(/1\.5초.*절반.*주의: 설정을 바꿀 때마다 다시 계산하므로 팬 소음이 커지기도 합니다\.$/);
    fireEvent.click(toggle);
    expect(options.actions.setAutoRefresh).toHaveBeenCalledWith(true);
  });

  it("drops a press while a change is saved and waits for the settings before it can be used", () => {
    const saving = makeOptions({ autoRefresh: true, saving: true });
    popover(saving);
    fireEvent.click(ui("app.auto_refresh"));
    expect(saving.actions.setAutoRefresh).not.toHaveBeenCalled();
    cleanup();
    popover(makeOptions({ autoRefresh: null }));
    expect((ui("app.auto_refresh") as HTMLButtonElement).disabled).toBe(true);
  });

  it("opens the log folder, and has no log row when the app keeps no log", () => {
    const options = makeOptions();
    popover(options, "en");
    fireEvent.click(ui("app.log.open"));
    expect(options.actions.openLogFolder).toHaveBeenCalledOnce();
    expect(ui("app.log.open").textContent).toBe("Open log folder");
    cleanup();
    popover(makeOptions({ logDir: null }));
    expect(document.querySelector('[data-ui="app.log"]')).toBeNull();
  });
});

const wrapper = (session?: Session) => ({ children }: { children: ReactNode }) => createElement(PrefsScope, {
  initial: { locale: "ko", theme: "light", accent: "default" },
  children: session ? createElement(SessionProvider, { value: session, children }) : children,
});

describe("useAppOptions", () => {
  it("shows the default and does nothing without a session", async () => {
    const { result } = renderHook(() => useAppOptions(), { wrapper: wrapper() });
    await act(async () => { result.current.actions.setAutoRefresh(true); result.current.actions.openLogFolder(); });
    expect(result.current.view).toEqual({ autoRefresh: false, saving: false, logDir: null });
  });

  it("saves auto_refresh through the app settings writer and opens the bootstrap's log folder", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    await session.stores.appSettings.load();
    session.stores.bootstrap.set({ ...bootstrapInfo, log_dir: LOGS });
    const hook = renderHook(() => useAppOptions(), { wrapper: wrapper(session) });
    try {
      expect(hook.result.current.view.logDir).toBe(LOGS);
      await act(async () => { hook.result.current.actions.setAutoRefresh(true); await session.settled(); });
      await waitFor(() => expect(hook.result.current.view).toMatchObject({ autoRefresh: true, saving: false }));
      expect((await mock.request({ method: "app_settings_get" })).auto_refresh).toBe(true);
      await act(async () => { hook.result.current.actions.openLogFolder(); await session.settled(); });
      expect(mock.calls).toContainEqual({ method: "open_folder", params: { path: LOGS } });
    } finally { hook.unmount(); mock.dispose(); }
  });
});
