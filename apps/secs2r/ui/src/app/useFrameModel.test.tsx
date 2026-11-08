// @vitest-environment happy-dom
import { act, cleanup, fireEvent, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { ReactNode } from "react";
import { translate, type Locale } from "../i18n";
import type { Notice } from "../ipc/bindings/Notice";
import { createMockService } from "../ipc/mock";
import { SessionProvider } from "../state/react";
import { createSession } from "../state/session";
import { renderWith } from "../test/render";
import { useSystemScreen } from "../screens/system/useSystemScreen";
import { useConnectScreen } from "../screens/connect/useConnectScreen";
import { useMeasureScreen } from "../screens/measure/useMeasureScreen";
import { useFrameModel } from "./useFrameModel";
import { ProjectStatus } from "./ProjectStatus";
import { STEPS } from "./model";

let mock: ReturnType<typeof createMockService>;
let session: ReturnType<typeof createSession>;
const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
const useModel = () => useFrameModel({ system: useSystemScreen().view, connect: useConnectScreen(false).view, measure: useMeasureScreen().view, calibrate: null, export: null }).model;
beforeEach(async () => {
  mock = createMockService({ tickMs: 0 }); session = createSession(mock); await session.start();
});
afterEach(async () => { cleanup(); await session.monitor(false); session.dispose(); mock.dispose(); });

it("preserves the mdat path and reports it ready without a device or connect view", async () => {
  const { result } = renderHook(() => useFrameModel({ system: null, connect: null, measure: null, calibrate: null, export: null }), { wrapper });
  await act(async () => { session.host.set({ path: "mdat" }); });
  expect(result.current.model.project.path).toEqual({ kind: "mdat", state: "ok" });
  await act(async () => { session.host.set({ path: "native" }); });
  expect(result.current.model.project.path.state).toBe("warning");
});

it("keeps invalid system configuration editable and blocks later steps", async () => {
  const { result } = renderHook(useModel, { wrapper });
  await act(async () => { session.stores.notices.set([{ code: "out_duplicate", params: { out: "1", first_role: "L", role: "R" } }]); });
  expect(result.current.steps.system).toEqual({ status: "ready" });
  for (const step of STEPS.filter((step) => step !== "system")) expect(result.current.steps[step]).toEqual({ status: "blocked", reason: "config_invalid", count: 1 });
  await act(async () => { session.stores.notices.set([]); });
  expect(result.current.steps.system.status).toBe("done");
  expect(result.current.steps.connect.status).not.toBe("blocked");
});

it("keeps the mdat connect step skipped even while configuration needs repair", async () => {
  const { result } = renderHook(useModel, { wrapper });
  await act(async () => { session.host.set({ path: "mdat" }); session.stores.notices.set([{ code: "out_duplicate", params: {} }]); });
  expect(result.current.steps.connect.status).toBe("skipped");
  expect(result.current.steps.measure.reason).toBe("config_invalid");
});

it("shows every step waiting for a project, even with stale path and views", async () => {
  const { result } = renderHook(useModel, { wrapper });
  await act(async () => { session.host.set({ path: "mdat" }); session.stores.project.set(null); });
  for (const step of STEPS) expect(result.current.steps[step]).toEqual({ status: "blocked", reason: "no_project" });
});

describe("the app settings notice (ADR 0051)", () => {
  const notices = {
    app_settings_unreadable: { code: "app_settings_unreadable", params: { path: "C:/Users/demo/AppData/Roaming/SECS2R/settings.json", error: "expected value at line 1 column 1" } },
    app_settings_newer: { code: "app_settings_newer", params: { version: "2" } },
  } satisfies Record<string, Notice>;
  /** The rail's status as the frame draws it: the frame model's summary in ProjectStatus. */
  function Status() {
    const { model } = useFrameModel({ system: null, connect: null, measure: null, calibrate: null, export: null });
    return <ProjectStatus project={model.project} />;
  }
  const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
  async function frameWith(notice: Notice | undefined, locale: Locale = "ko", project = true) {
    mock.dispose(); session.dispose();
    mock = createMockService({ tickMs: 0, ...(project ? {} : { project: null }), ...(notice ? { bootstrap: { settings_notice: notice } } : {}) });
    session = createSession(mock); await session.start();
    renderWith(<Status />, { session, locale });
    await act(async () => { await session.settled(); });
  }

  it.each(Object.entries(notices))("says %s in the status as one sentence, with or without a project", async (_code, notice) => {
    for (const project of [true, false]) {
      await frameWith(notice, "ko", project);
      const shown = ui("frame.status.settings_notice")!;
      expect(shown.getAttribute("role")).toBe("status");
      expect(shown.className).toContain("ui-error--warning");
      expect(shown.querySelector("h3")?.textContent).toBe(translate("ko", "frame.status.settings_notice.title"));
      const { error, ...params } = notice.params as Record<string, string>;
      expect(shown.querySelector("p")?.textContent).toBe(translate("ko", `notice.${notice.code}` as "notice.app_settings_newer", params));
      // The service's own error stays behind 자세히.
      expect(shown.querySelector(".ui-details__raw")?.textContent ?? null).toBe(error ?? null);
      cleanup();
    }
  });

  it("hides the notice for the session once closed, and draws nothing without one", async () => {
    await frameWith(notices.app_settings_newer, "en");
    const close = ui("frame.status.settings_notice.close")!;
    expect(close.getAttribute("aria-label")).toBe("Close app settings warning");
    expect(ui("frame.status.settings_notice")?.querySelector("p")?.textContent).toBe(translate("en", "notice.app_settings_newer", { version: "2" }));
    await act(async () => { fireEvent.click(close); });
    expect(ui("frame.status.settings_notice")).toBeNull();
    // A project opened later does not bring it back.
    await act(async () => { await session.call({ method: "project_open", params: { path: "/B.secs" } }); await session.settled(); });
    expect(ui("frame.status.settings_notice")).toBeNull();
    cleanup();
    await frameWith(undefined);
    expect(ui("frame.status")).not.toBeNull();
    expect(ui("frame.status.settings_notice")).toBeNull();
  });
});
