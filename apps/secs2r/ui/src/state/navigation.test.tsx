// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "../App";
import { createMockService } from "../ipc/mock";
import { measureMethods, projectInfo } from "../ipc/fixtures";
import { installFakeCanvas } from "../test/canvas";
import * as sessions from "./session";

beforeEach(() => installFakeCanvas());
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const setup = (mock: ReturnType<typeof createMockService>, method: string) => mock.request({
  method: "measure_configure", params: { method, input_device: null, output_device: null,
    input_channel: null, output_channels: [], sweep_length: null, repetitions: null,
    sweep_level_dbfs: null, timing: null, loopback_input_channel: null, timing_ref_output: null, share_mode: null },
});

it("opens and focuses the measurement list from a structured calculation error without starting a measurement", async () => {
  const mock = createMockService({ tickMs: 0 });
  await setup(mock, "native");
  mock.failJobs("recommend_xo", "engine_input", "invalid input: position 1 SW2: empty or nonfinite IR", {
    reason: "invalid_measurement", position: "1", role: "SW2",
  });
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("frame.rail.calibrate")).not.toBeNull());
    fireEvent.click(ui("frame.rail.calibrate")!);
    await waitFor(() => expect(ui("calibrate.xo.recommend")).not.toBeNull());
    fireEvent.click(ui("calibrate.xo.recommend")!);
    await waitFor(() => expect(mock.calls.some((call) => call.method === "recommend_xo")).toBe(true));
    await act(async () => mock.drain());
    await waitFor(() => expect(ui("calibrate.xo.failed.measurements")).not.toBeNull());
    const writes = () => mock.calls.filter((call) => ["measure_position", "sample_load", "mdat_import", "measure_configure"].includes(call.method));
    const before = writes().length;
    fireEvent.click(ui("calibrate.xo.failed.measurements")!);
    await waitFor(() => expect(ui("measure.positions")?.contains(document.activeElement)).toBe(true));
    expect(document.activeElement?.getAttribute("aria-current")).toBe("true");
    expect(writes()).toHaveLength(before);
  } finally { rendered.unmount(); mock.dispose(); }
});

it("opens sample-rate settings from the failure, persists a keyboard choice and restores it without recalculating", async () => {
  const mock = createMockService({ tickMs: 0 });
  await setup(mock, "native");
  mock.failJobs("recommend_xo", "engine_input", "invalid input: position 1 SW2: sample rate 44100 (expected 48000)", {
    reason: "measurement_sample_rate", position: "1", role: "SW2", actual_hz: "44100", expected_hz: "48000",
  });
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("frame.rail.calibrate")).not.toBeNull());
    fireEvent.click(ui("frame.rail.calibrate")!);
    await waitFor(() => expect(ui("calibrate.xo.recommend")).not.toBeNull());
    expect(ui("calibrate.input.details")?.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(ui("calibrate.xo.recommend")!);
    await waitFor(() => expect(mock.calls.some((call) => call.method === "recommend_xo")).toBe(true));
    await act(async () => mock.drain());
    await waitFor(() => expect(ui("calibrate.xo.failed.sample_rate")).not.toBeNull());
    const started = () => mock.calls.filter((call) => ["calibrate", "recommend_xo", "mimo", "measure_position"].includes(call.method)).length;
    const before = started();
    fireEvent.click(ui("calibrate.xo.failed.sample_rate")!);
    await waitFor(() => expect(document.activeElement).toBe(ui("calibrate.sample_rate_policy.reject")));
    expect(mock.calls.some((call) => call.method === "calibration_settings_edit" && call.params.edit.kind === "sample_rate_policy")).toBe(false);
    fireEvent.keyDown(ui("calibrate.sample_rate_policy.reject")!, { key: "ArrowRight" });
    await waitFor(() => expect(ui("calibrate.sample_rate_policy.resample")?.getAttribute("aria-checked")).toBe("true"));
    expect(mock.calls).toContainEqual({ method: "calibration_settings_edit", params: { edit: { kind: "sample_rate_policy", policy: "resample" } } });
    expect(started()).toBe(before);
    fireEvent.click(ui("frame.rail.system")!);
    await waitFor(() => expect(ui("system")).not.toBeNull());
    fireEvent.click(ui("frame.rail.calibrate")!);
    await waitFor(() => expect(ui("calibrate.sample_rate_policy.resample")?.getAttribute("aria-checked")).toBe("true"));
    expect(ui("calibrate.input.details")?.getAttribute("aria-expanded")).toBe("true");
    expect(started()).toBe(before);
  } finally { rendered.unmount(); mock.dispose(); }
});

it("opens the native output picker without changing its selection, then reflects changes back on System", async () => {
  const mock = createMockService({ tickMs: 0 });
  await setup(mock, "native");
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("system.device.configure")).not.toBeNull());
    expect(ui("system.device")?.textContent).toContain("기본 장치");
    const writes = () => mock.calls.filter((call) => call.method === "measure_configure");
    const before = writes().length;
    fireEvent.click(ui("system.device.configure")!);
    await waitFor(() => expect(document.activeElement).toBe(ui("connect.output_device")));
    expect(writes()).toHaveLength(before);
    const output = measureMethods.native_devices.find((device) => device.output_channels === 16)!;
    fireEvent.change(ui("connect.output_device")!, { target: { value: output.id } });
    await waitFor(() => expect(writes().at(-1)?.params.output_device).toBe(output.id));
    fireEvent.click(ui("frame.rail.system")!);
    await waitFor(() => expect(ui("system.device")?.textContent).toContain(output.name));
    expect(ui("system.device")?.textContent).toContain("16");
    fireEvent.click(ui("system.device.configure")!);
    await waitFor(() => expect(document.activeElement).toBe(ui("connect.output_device")));
    fireEvent.change(ui("connect.output_device")!, { target: { value: "" } });
    await waitFor(() => expect(writes().at(-1)?.params.output_device).toBe(""));
    fireEvent.click(ui("frame.rail.system")!);
    await waitFor(() => expect(ui("system.device")?.textContent).toContain("기본 장치"));
    expect(ui("system.device")?.textContent).not.toContain(output.name);
  } finally { rendered.unmount(); mock.dispose(); }
});

it("focuses method selection when no method is chosen and does not choose one", async () => {
  const mock = createMockService({ tickMs: 0 });
  await setup(mock, "none");
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("system.device.configure")).not.toBeNull());
    expect(ui("system.device")?.textContent).toContain("미선택");
    const before = mock.calls.filter((call) => call.method === "measure_configure").length;
    fireEvent.click(ui("system.device.configure")!);
    await waitFor(() => expect(document.activeElement).toBe(ui("connect.method.native")));
    expect(mock.calls.filter((call) => call.method === "measure_configure")).toHaveLength(before);
    expect(ui("connect.method.native")?.getAttribute("aria-pressed")).toBe("false");
  } finally { rendered.unmount(); mock.dispose(); }
});

it("waits for loaded measurement settings before offering device setup", async () => {
  const mock = createMockService({ tickMs: 0 });
  await setup(mock, "native");
  const request = mock.request;
  let release!: () => void;
  vi.spyOn(mock, "request").mockImplementation(async (message) => {
    if (message.method === "measure_config_get") await new Promise<void>((resolve) => { release = resolve; });
    return request(message);
  });
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("system.device")?.textContent).toContain("장치 확인 중"));
    expect(ui("system.device.configure")).toBeNull();
    await act(() => release());
    await waitFor(() => expect(ui("system.device.configure")).not.toBeNull());
    fireEvent.click(ui("system.device.configure")!);
    await waitFor(() => expect(document.activeElement).toBe(ui("connect.output_device")));
  } finally { rendered.unmount(); mock.dispose(); }
});

it("arrives at the disabled output field when the native device list is empty", async () => {
  const mock = createMockService({ tickMs: 0 });
  await setup(mock, "native");
  const request = mock.request;
  vi.spyOn(mock, "request").mockImplementation(async (message) => message.method === "measure_methods"
    ? { ...measureMethods, native_devices: [], methods: ["rew", "mock"] } as never : request(message));
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("system.device.configure")).not.toBeNull());
    fireEvent.click(ui("system.device.configure")!);
    await waitFor(() => expect(ui("connect.output_device")?.hasAttribute("disabled")).toBe(true));
    expect(document.activeElement).toBe(document.querySelector('label[for="connect.output_device"]'));
  } finally { rendered.unmount(); mock.dispose(); }
});

it("does not retain a focus request after a navigation was refused", async () => {
  const mock = createMockService({ tickMs: 0 });
  await setup(mock, "native");
  const original = sessions.createSession;
  let session!: sessions.Session;
  let navigate!: sessions.Session["navigate"];
  vi.spyOn(sessions, "createSession").mockImplementation((ipc) => {
    session = original(ipc); navigate = session.navigate;
    session.navigate = vi.fn();
    return session;
  });
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("system.device.configure")).not.toBeNull());
    fireEvent.click(ui("system.device.configure")!);
    expect(session.host.getSnapshot().current).toBe("system");
    await act(() => navigate("connect"));
    await waitFor(() => expect(ui("connect.output_device")).not.toBeNull());
    expect(document.activeElement).not.toBe(ui("connect.output_device"));
  } finally { rendered.unmount(); mock.dispose(); }
});

it("does not retry a missing focus target on a later visit", async () => {
  const mock = createMockService({ tickMs: 0 });
  await setup(mock, "native");
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("system.device.configure")).not.toBeNull());
    const query = document.querySelector.bind(document);
    const search = vi.spyOn(document, "querySelector").mockImplementation((selector: string) =>
      selector === '[data-ui="connect.output_device"]' ? null : query(selector));
    fireEvent.click(ui("system.device.configure")!);
    await waitFor(() => expect(ui("connect.method")).not.toBeNull());
    search.mockRestore();
    fireEvent.click(ui("frame.rail.system")!);
    await waitFor(() => expect(ui("system.device.configure")).not.toBeNull());
    fireEvent.click(ui("frame.rail.connect")!);
    await waitFor(() => expect(ui("connect.output_device")).not.toBeNull());
    expect(document.activeElement).not.toBe(ui("connect.output_device"));
  } finally { rendered.unmount(); mock.dispose(); }
});

it("takes a configuration error to the invalid output rather than a preset", async () => {
  const mock = createMockService({ tickMs: 0 });
  await mock.request({ method: "config_set_speaker", params: { index: 1, role: null, size: null, band_index: 0, out: 1, pol: null } });
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("frame.rail.connect")).not.toBeNull());
    fireEvent.click(ui("frame.rail.connect")!);
    await waitFor(() => expect(ui("frame.recovery")).not.toBeNull());
    fireEvent.click(ui("frame.recovery")!);
    await waitFor(() => expect(document.activeElement).toBe(document.querySelector('[data-ui^="system.patch."][data-ui*=".out"][aria-invalid="true"]')));
    expect(document.activeElement).not.toBeNull();
  } finally { rendered.unmount(); mock.dispose(); }
});

it.each(["calibrate", "export"])("recovers %s with no measurements and no method at the method picker", async (step) => {
  const mock = createMockService({ tickMs: 0, project: { path: "room.secs", name: "Room", preset: "2.0", sub_count: 0, positions: [], dirty: false, tutorial: false, migration_warnings: [] } });
  await setup(mock, "none");
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui(`frame.rail.${step}`)).not.toBeNull());
    fireEvent.click(ui(`frame.rail.${step}`)!);
    await waitFor(() => expect(ui("frame.recovery")).not.toBeNull());
    fireEvent.click(ui("frame.recovery")!);
    await waitFor(() => expect(document.activeElement).toBe(ui("connect.method.native")));
    expect(ui("connect.method.native")?.getAttribute("aria-pressed")).toBe("false");
  } finally { rendered.unmount(); mock.dispose(); }
});

it("offers the level-check action without playing sound when levels are missing", async () => {
  const mock = createMockService({ tickMs: 0, project: { path: "room.secs", name: "Room", preset: "2.0", sub_count: 0, positions: [], dirty: false, tutorial: false, migration_warnings: [] } });
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("frame.rail.calibrate")).not.toBeNull());
    fireEvent.click(ui("frame.rail.calibrate")!);
    await waitFor(() => expect(ui("frame.recovery")).not.toBeNull());
    fireEvent.click(ui("frame.recovery")!);
    await waitFor(() => expect(document.activeElement).toBe(ui("connect.level.play_all")));
    expect(mock.calls.some((call) => call.method === "level_play")).toBe(false);
  } finally { rendered.unmount(); mock.dispose(); }
});

it.each([
  { reachable: true, pro: false, target: "connect.method.native" },
  { reachable: false, pro: null, target: "connect.rew.launch" },
])("recovers REW with reachable=$reachable at $target without changing the method or launching it", async ({ reachable, pro, target }) => {
  const mock = createMockService({ tickMs: 0, project: { path: "room.secs", name: "Room", preset: "2.0", sub_count: 0, positions: [], dirty: false, tutorial: false, migration_warnings: [] } });
  await setup(mock, "rew");
  const request = mock.request;
  vi.spyOn(mock, "request").mockImplementation(async (message) => message.method === "measure_methods"
    ? { ...measureMethods, methods: ["native", "mock"], rew: { running: reachable, reachable, pro } } as never : request(message));
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("frame.rail.calibrate")).not.toBeNull());
    fireEvent.click(ui("frame.rail.calibrate")!);
    await waitFor(() => expect(ui("frame.recovery")).not.toBeNull());
    const before = mock.calls.filter((call) => call.method === "measure_configure").length;
    fireEvent.click(ui("frame.recovery")!);
    await waitFor(() => expect(document.activeElement).toBe(ui(target)));
    expect(mock.calls.filter((call) => call.method === "measure_configure")).toHaveLength(before);
    expect(mock.calls.some((call) => call.method === "rew_launch")).toBe(false);
  } finally { rendered.unmount(); mock.dispose(); }
});

it("lists the project's warnings in the rail, opens a position to measure it again and lists them again after the next open", async () => {
  const warnings = [
    { code: "measurement.ir_path.outside", entry: "position 2, role L" },
    { code: "measurement.ir_path.outside", entry: "position 3, role R" },
  ];
  const mock = createMockService({ tickMs: 0, project: { ...projectInfo, migration_warnings: warnings } });
  await setup(mock, "native");
  const original = sessions.createSession;
  let session!: sessions.Session;
  vi.spyOn(sessions, "createSession").mockImplementation((ipc) => (session = original(ipc)));
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("frame.status.warnings.position.3")).not.toBeNull());
    expect(ui("frame.status.warnings.position.2")).not.toBeNull();
    fireEvent.click(ui("frame.status.warnings.position.3.fix")!);
    await waitFor(() => expect(document.activeElement).toBe(ui("measure.position.start")));
    expect(ui("measure.positions.3")?.getAttribute("aria-current")).toBe("true");
    expect(ui("measure.position.start")?.textContent).toContain("이 위치 다시 측정");
    expect(mock.calls.some((call) => call.method === "measure_position")).toBe(false);
    fireEvent.click(ui("frame.status.warnings.close")!);
    expect(ui("frame.status.warnings")).toBeNull();
    await act(async () => { await session.call({ method: "project_save", params: { path: "C:/rooms/room.secs" } }); });
    expect(ui("frame.status.warnings")).toBeNull();
    await act(async () => { await session.call({ method: "project_open", params: { path: "C:/rooms/room.secs" } }); });
    await waitFor(() => expect(ui("frame.status.warnings")?.textContent).toContain("프로젝트 경고 2건"));
  } finally { rendered.unmount(); mock.dispose(); }
});
