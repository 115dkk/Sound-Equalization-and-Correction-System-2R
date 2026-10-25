// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { App } from "../App";
import { translate } from "../i18n";
import { createMockService } from "../ipc/mock";
import { installFakeCanvas } from "../test/canvas";

beforeAll(() => installFakeCanvas());
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const must = (id: string) => {
  const node = ui(id);
  if (!node) throw new Error(`missing [data-ui="${id}"]`);
  return node;
};

it("draws nothing while the app settings load, as the blank window did", async () => {
  const mock = createMockService({ project: null, tickMs: 0 });
  const request = mock.request;
  let release!: () => void;
  const gate = new Promise<void>((resolve) => { release = resolve; });
  vi.spyOn(mock, "request").mockImplementation(async (params) => {
    if (params.method === "app_settings_get") await gate;
    return request(params);
  });
  const rendered = render(<App ipc={mock} />);
  try {
    await act(async () => {});
    expect(rendered.container.childElementCount).toBe(0);
    await act(async () => { release(); });
    await waitFor(() => expect(ui("frame")).not.toBeNull());
  } finally { rendered.unmount(); mock.dispose(); }
});

it("shows why the app settings did not load with a filled 다시 시도, keeps the panel while it reads again, and opens the app after a retry (TD-29)", async () => {
  // Before the settings arrive the window speaks the system's language; the
  // stored settings (Korean in the mock) take over once they load.
  vi.spyOn(navigator, "languages", "get").mockReturnValue(["en-US"]);
  const mock = createMockService({ project: null, tickMs: 0 });
  mock.failNext("app_settings_get", "project_io", "settings.json: permission denied");
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("app.settings_failed")).not.toBeNull());
    expect(ui("frame")).toBeNull();
    const panel = must("app.settings_failed");
    expect(panel.querySelector("h1")?.textContent).toBe(translate("en", "app.settings_failed.title"));
    expect(must("app.settings_failed.error").textContent).toContain(translate("en", "error.project_io"));
    expect(must("app.settings_failed.error").textContent).toContain("settings.json: permission denied");
    const retry = must("app.settings_failed.retry");
    expect(retry.textContent).toBe(translate("en", "app.settings_failed.retry"));
    expect(retry.classList.contains("ui-button--primary")).toBe(true);
    expect(panel.querySelectorAll(".ui-button--primary")).toHaveLength(1);
    expect(document.activeElement).toBe(retry);
    expect(document.documentElement.lang).toBe("en");

    // A retry that fails again: the panel stays, busy while it reads, and
    // the focus returns to the one action.
    const request = mock.request;
    let release!: () => void;
    let gate = new Promise<void>((resolve) => { release = resolve; });
    vi.spyOn(mock, "request").mockImplementation(async (params) => {
      if (params.method === "app_settings_get") await gate;
      return request(params);
    });
    mock.failNext("app_settings_get", "internal", "still unavailable");
    fireEvent.click(retry);
    await act(async () => {});
    expect(ui("app.settings_failed")).not.toBeNull();
    expect(must("app.settings_failed.retry").getAttribute("aria-busy")).toBe("true");
    await act(async () => { release(); });
    await waitFor(() => expect(must("app.settings_failed.retry").getAttribute("aria-busy")).toBeNull());
    expect(must("app.settings_failed.error").textContent).toContain(translate("en", "error.internal"));
    expect(document.activeElement).toBe(must("app.settings_failed.retry"));

    gate = new Promise<void>((resolve) => { release = resolve; });
    fireEvent.click(must("app.settings_failed.retry"));
    await act(async () => {});
    expect(must("app.settings_failed.retry").getAttribute("aria-busy")).toBe("true");
    await act(async () => { release(); });
    await waitFor(() => expect(ui("frame")).not.toBeNull());
    expect(ui("app.settings_failed")).toBeNull();
    expect(mock.calls.filter((call) => call.method === "app_settings_get")).toHaveLength(3);
    expect(document.documentElement.lang).toBe("ko");
    await waitFor(() => expect(ui("project.new")).not.toBeNull());
    expect(must("project.new").querySelector("h2")?.textContent).toBe(translate("ko", "project.new.title"));
  } finally { rendered.unmount(); mock.dispose(); }
});
