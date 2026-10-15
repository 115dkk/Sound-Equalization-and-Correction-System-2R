// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, expect, it } from "vitest";
import { App } from "../App";
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
    await waitFor(() => expect(mock.calls.some((r) => r.method === "calibrate")).toBe(true));
    await act(async () => { mock.drain(); });
    await waitFor(() => expect(document.querySelector('[data-ui="calibrate.verdict"]')).not.toBeNull());
    fireEvent.click(ui("frame.rail.export"));
    await waitFor(() => expect(document.querySelector('[data-ui="export.run.camilla_dsp"]')).not.toBeNull());
    fireEvent.click(ui("export.run.camilla_dsp"));
    await waitFor(() => expect(mock.calls.some((r) => r.method === "export")).toBe(true));
    await act(async () => { mock.drain(); });
    await waitFor(() => expect(mock.calls.some((r) => r.method === "open_folder")).toBe(true));
    expect(mock.calls.filter((r) => ["project_new", "measure_position", "calibrate", "export"].includes(r.method)).map((r) => r.method))
      .toEqual(["project_new", ...methods.mock_positions.map(() => "measure_position"), "calibrate", "export"]);
    expect(mock.calls.some((r) => r.method === "sample_load")).toBe(false);
    expect(mock.calls.find((r) => r.method === "export")).toEqual({ method: "export", params: { kind: "camilla_dsp", folder: "" } });
  } finally { rendered.unmount(); mock.dispose(); }
});
