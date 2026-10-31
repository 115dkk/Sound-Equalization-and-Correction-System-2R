// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { App } from "../App";
import { LOGOS } from "../app/logo";
import { createMockService } from "../ipc/mock";
import { installFakeCanvas } from "../test/canvas";

beforeAll(() => installFakeCanvas());
afterEach(cleanup);
const ui = (id: string) => {
  const node = document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
  if (!node) throw new Error(`missing ${id}`);
  return node;
};

it("walks a non-tutorial sample project through mock measurements, calibration and a real export call", async () => {
  const mock = createMockService({ project: null, tickMs: 0 });
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(document.querySelector('[data-ui="project.new"]')).not.toBeNull());
    fireEvent.click(screen.getByRole("radio", { name: "데모 방" }));
    fireEvent.click(ui("project.new.create"));
    await waitFor(() => expect(mock.calls.some((r) => r.method === "project_new" && r.params.sample && !r.params.tutorial)).toBe(true));
    await waitFor(() => expect(document.querySelector('[data-ui="project.new"]')).toBeNull());
    // The sample starts at one position; choose the room's full nine-position plan.
    fireEvent.click(ui("system.arrangement.9"));
    await waitFor(() => expect(mock.calls.some((r) => r.method === "layout_set" && r.params.count === 9)).toBe(true));
    fireEvent.click(ui("frame.rail.connect"));
    await waitFor(() => expect(document.querySelector('[data-ui="connect.method"]')).not.toBeNull());
    fireEvent.click(ui("connect.method.mock"));
    await waitFor(() => expect(mock.calls.some((r) => r.method === "measure_configure" && r.params.method === "mock")).toBe(true));
    fireEvent.click(ui("frame.rail.measure"));
    await waitFor(() => expect(document.querySelector('[data-ui="measure.position.start"]')).not.toBeNull());
    const methods = await mock.request({ method: "measure_methods" });
    for (const position of methods.mock_positions) {
      await waitFor(() => expect(ui("measure.position.start").hasAttribute("disabled")).toBe(false));
      fireEvent.click(ui("measure.position.start"));
      await waitFor(() => expect(mock.calls.some((r) => r.method === "measure_position" && r.params.position === position)).toBe(true));
      await act(async () => { mock.drain(); });
      await waitFor(() => expect(document.querySelector(`[data-ui="measure.positions.${position}"]`)).not.toBeNull());
    }
    await waitFor(() => expect(document.querySelector('[data-ui="measure.next"]')).not.toBeNull());
    fireEvent.click(ui("frame.rail.calibrate"));
    await waitFor(() => expect(document.querySelector('[data-ui="calibrate.compute"]')).not.toBeNull());
    fireEvent.click(ui("calibrate.compute"));
    await waitFor(() => expect(mock.calls.some((r) => r.method === "recommend_xo")).toBe(true));
    await act(async () => { mock.drain(); });
    await waitFor(() => expect(mock.calls.some((r) => r.method === "calibrate")).toBe(true));
    await act(async () => { mock.drain(); });
    await waitFor(() => expect(document.querySelector('[data-ui="calibrate.verdict"]')).not.toBeNull());
    fireEvent.click(ui("frame.rail.export"));
    await waitFor(() => expect(document.querySelector('[data-ui="export.run.camilla_dsp"]')).not.toBeNull());
    fireEvent.click(ui("export.run.camilla_dsp"));
    await waitFor(() => expect(mock.calls.some((r) => r.method === "export")).toBe(true));
    await act(async () => { mock.drain(); });
    await waitFor(() => expect(mock.calls.some((r) => r.method === "open_folder")).toBe(true));
    expect(mock.calls.filter((r) => ["project_new", "measure_position", "recommend_xo", "calibrate", "export"].includes(r.method)).map((r) => r.method))
      .toEqual(["project_new", ...methods.mock_positions.map(() => "measure_position"), "recommend_xo", "calibrate", "export"]);
    expect(mock.calls.some((r) => r.method === "sample_load")).toBe(false);
    expect(mock.calls.find((r) => r.method === "export")).toEqual({ method: "export", params: { kind: "camilla_dsp", folder: "" } });
  } finally { rendered.unmount(); mock.dispose(); }
  // The whole flow renders the App through four steps; on a loaded runner it
  // can exceed the default 5 s, so it gets a wider bound rather than a retry.
}, 30_000);

it("shows cancel_requested in both the measure view and tray when the measure screen cancels (TD-72)", async () => {
  const mock = createMockService({ tickMs: 0 });
  await mock.request({ method: "measure_configure", params: {
    method: "mock", input_device: null, output_device: null, input_channel: null, output_channels: [], sweep_length: null, repetitions: null,
    sweep_level_dbfs: null, timing: null, loopback_input_channel: null, timing_ref_output: null, share_mode: null,
  } });
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(document.querySelector('[data-ui="frame.rail.measure"]')).not.toBeNull());
    fireEvent.click(ui("frame.rail.measure"));
    await waitFor(() => expect(document.querySelector('[data-ui="measure.position.start"]')).not.toBeNull());
    fireEvent.click(ui("measure.position.start"));
    await waitFor(() => expect(document.querySelector('[data-ui="measure.cancel"]')).not.toBeNull());
    expect(ui("jobs.cancel.measure").textContent).toBe("취소");
    await act(async () => { fireEvent.click(ui("measure.cancel")); });
    expect(mock.calls.filter((call) => call.method === "job_cancel")).toHaveLength(1);
    expect.soft(ui("measure.cancel").hasAttribute("disabled")).toBe(true);
    expect.soft(ui("measure.status").textContent).toContain("취소 중");
    expect.soft(ui("jobs.cancel.measure").textContent).toBe("취소하는 중");
    await act(async () => { mock.tick(); });
    expect(document.querySelector('[data-ui="measure.run"]')).toBeNull();
    expect(document.querySelector('[data-ui="jobs.job.measure"]')).toBeNull();
  } finally { rendered.unmount(); mock.dispose(); }
});

it("keeps the chosen logo in the app settings and shows it again on the next start", async () => {
  const mock = createMockService({ project: null, tickMs: 0 });
  const logo = () => ui("frame.logo").getAttribute("src");
  const first = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(document.querySelector('[data-ui="frame.logo"]')).not.toBeNull());
    expect(logo()).toBe(LOGOS.expert);
    fireEvent.click(ui("frame.settings"));
    expect(ui("frame.settings.logo.expert").getAttribute("aria-checked")).toBe("true");
    fireEvent.click(ui("frame.settings.logo.civilian"));
    await waitFor(() => expect(mock.calls.some((r) => r.method === "app_settings_set" && r.params.settings.icon === "civilian")).toBe(true));
    // A display choice shows once the service has accepted it (TD-68).
    await waitFor(() => expect(logo()).toBe(LOGOS.civilian));
  } finally { first.unmount(); }
  const second = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(document.querySelector('[data-ui="frame.logo"]')).not.toBeNull());
    expect(logo()).toBe(LOGOS.civilian);
  } finally { second.unmount(); mock.dispose(); }
});

it("keeps the chosen skin in the app settings and applies it again on the next start", async () => {
  const mock = createMockService({ project: null, tickMs: 0 });
  const first = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(document.querySelector('[data-ui="frame.settings"]')).not.toBeNull());
    expect(document.documentElement.hasAttribute("data-skin")).toBe(false);
    fireEvent.click(ui("frame.settings"));
    expect(ui("frame.settings.skin.default").getAttribute("aria-checked")).toBe("true");
    fireEvent.click(ui("frame.settings.skin.rack"));
    await waitFor(() => expect(mock.calls.some((r) => r.method === "app_settings_set" && r.params.settings.skin === "rack")).toBe(true));
    await waitFor(() => expect(document.documentElement.dataset.skin).toBe("rack"));
  } finally { first.unmount(); delete document.documentElement.dataset.skin; }
  const second = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(document.querySelector('[data-ui="frame.settings"]')).not.toBeNull());
    expect(document.documentElement.dataset.skin).toBe("rack");
  } finally { second.unmount(); mock.dispose(); delete document.documentElement.dataset.skin; }
});

it("keeps the compute choice when the theme changes while that choice is being saved (TD-68)", async () => {
  const mock = createMockService({ tickMs: 0 });
  await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
  const original = mock.request;
  let release!: () => void;
  let entered!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  const reached = new Promise<void>((resolve) => { entered = resolve; });
  let writes = 0;
  // The service stores the compute choice at once; only its answer is late.
  const slow = vi.spyOn(mock, "request").mockImplementation(async (request) => {
    const data = await original(request);
    if (request.method === "app_settings_set" && ++writes === 1) { entered(); await held; }
    return data;
  });
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(document.querySelector('[data-ui="frame.rail.calibrate"]')).not.toBeNull());
    fireEvent.click(ui("frame.rail.calibrate"));
    await waitFor(() => expect((ui("calibrate.compute.preference") as HTMLSelectElement).disabled).toBe(false));
    fireEvent.change(ui("calibrate.compute.preference"), { target: { value: "cpu" } });
    await act(async () => { await reached; });
    fireEvent.click(ui("frame.settings"));
    fireEvent.click(ui("frame.settings.theme.dark"));
    await act(async () => { release(); });
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
    expect(await original({ method: "app_settings_get" })).toMatchObject({ compute: "cpu", theme: "dark" });
  } finally { release(); rendered.unmount(); slow.mockRestore(); mock.dispose(); delete document.documentElement.dataset.theme; }
});

it("applies the saved matrix skin on startup before any user action", async () => {
  const mock = createMockService({ project: null, tickMs: 0 });
  const settings = await mock.request({ method: "app_settings_get" });
  await mock.request({ method: "app_settings_set", params: { settings: { ...settings, skin: "matrix" } } });
  mock.calls.length = 0;
  delete document.documentElement.dataset.skin;
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(document.querySelector('[data-ui="frame.settings"]')).not.toBeNull());
    expect(document.documentElement.dataset.skin).toBe("matrix");
    expect(mock.calls.some((r) => r.method === "app_settings_set")).toBe(false);
  } finally { rendered.unmount(); mock.dispose(); delete document.documentElement.dataset.skin; }
});

it("keeps all modes, local device sharing and project values unchanged when either logo is selected", async () => {
  const mock = createMockService({ tickMs: 0 });
  const rendered = render(<App ipc={mock} />);
  const values = async () => ({
    project: await mock.request({ method: "project_info" }),
    config: await mock.request({ method: "config_get" }),
    measurement: await mock.request({ method: "measure_config_get" }),
    correction: await mock.request({ method: "calibration_settings_get" }),
  });
  try {
    await waitFor(() => expect(document.querySelector('[data-ui="frame.rail.connect"]')).not.toBeNull());
    fireEvent.click(ui("frame.rail.connect"));
    fireEvent.click(ui("connect.method.native"));
    await waitFor(() => expect(document.querySelector('[data-ui="connect.devices.details"]')).not.toBeNull());
    fireEvent.click(ui("connect.devices.details"));
    fireEvent.click(ui("connect.share_mode.shared"));
    await waitFor(() => expect(mock.calls.some((call) => call.method === "measure_configure" && call.params.share_mode === "shared")).toBe(true));
    fireEvent.click(ui("frame.rail.calibrate"));
    await waitFor(() => expect(document.querySelector('[data-ui="calibrate.mode.ultra"]')).not.toBeNull());
    fireEvent.click(ui("calibrate.mode.ultra"));
    await waitFor(() => expect(document.querySelector('[data-ui="calibrate.mode.ultra_ms"]')).not.toBeNull());
    const before = await values();
    const mutating = () => mock.calls.filter((call) => ["measure_configure", "calibration_settings_edit", "config_set_speaker", "config_apply_preset"].includes(call.method)).length;
    const writes = mutating();
    for (const icon of ["civilian", "expert"] as const) {
      fireEvent.click(ui("frame.settings"));
      expect(document.querySelector('[data-ui="frame.settings.expert"]')).toBeNull();
      fireEvent.click(ui(`frame.settings.logo.${icon}`));
      await waitFor(() => expect(ui("frame.logo").getAttribute("src")).toBe(LOGOS[icon]));
      fireEvent.click(ui("frame.settings"));
      for (const mode of ["zero", "mixed", "linear", "ultra"]) expect(document.querySelector(`[data-ui="calibrate.mode.${mode}"]`)).not.toBeNull();
      expect(ui("calibrate.mode.ultra").getAttribute("aria-checked")).toBe("true");
      expect((ui("calibrate.mode.ultra_ms") as HTMLInputElement).value).toBe(String(before.correction.ultra_ms));
      fireEvent.click(ui("frame.rail.connect"));
      await waitFor(() => expect(document.querySelector('[data-ui="connect.share_mode.shared"]')).not.toBeNull());
      expect(ui("connect.share_mode.shared").getAttribute("aria-checked")).toBe("true");
      expect(await values()).toEqual(before);
      expect(mutating()).toBe(writes);
      fireEvent.click(ui("frame.rail.calibrate"));
      await waitFor(() => expect(document.querySelector('[data-ui="calibrate.mode.ultra"]')).not.toBeNull());
    }
  } finally { rendered.unmount(); mock.dispose(); }
}, 15_000);
