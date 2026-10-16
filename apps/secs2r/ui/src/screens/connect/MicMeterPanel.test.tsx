// @vitest-environment happy-dom
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { App } from "../../App";
import { createMockService } from "../../ipc/mock";
import { installFakeCanvas } from "../../test/canvas";

beforeAll(() => installFakeCanvas());
afterEach(cleanup);

const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);

it("floats the mic meter on Ctrl+M with the focus on its close button; Esc inside closes it and the focus goes back", async () => {
  const mock = createMockService({ project: null, tickMs: 0 });
  await mock.request({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: false } });
  // The UMIK-1 on input 1 (kept by the service when 자체 측정 is chosen below): the panel names it as the level check does, not by its device id.
  await mock.request({ method: "measure_configure", params: { method: "native", input_device: "wasapi:{0.0.1.00000000}.{mic}", output_device: null, input_channel: 1, output_channels: [], sweep_length: null, repetitions: null, sweep_level_dbfs: null, timing: null, loopback_input_channel: null, timing_ref_output: null, share_mode: null } });
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("frame.rail.connect")).not.toBeNull(), { timeout: 8000 });
    const opener = ui("frame.rail.connect")!;
    fireEvent.click(opener);
    await waitFor(() => expect(ui("connect.method.native")).not.toBeNull());
    fireEvent.click(ui("connect.method.native")!);
    await waitFor(() => expect(ui("connect.level")).not.toBeNull());
    opener.focus();
    fireEvent.keyDown(window, { key: "m", code: "KeyM", ctrlKey: true });
    await waitFor(() => expect(ui("app.mic")).not.toBeNull());
    // Named by its title, the focus on the way out; not modal (no aria-modal, nothing else made inert).
    const close = ui("app.mic.close")!;
    expect(document.activeElement).toBe(close);
    const panel = close.closest("section")!;
    expect(document.getElementById(panel.getAttribute("aria-labelledby")!)?.textContent).toBe("마이크 입력");
    expect(panel.hasAttribute("aria-modal")).toBe(false);
    await waitFor(() => expect(ui("app.mic.input")?.textContent).toBe("입력 1 · UMIK-1"));
    // Esc inside the panel is the panel's: nothing behind it hears it.
    const behind = vi.fn();
    window.addEventListener("keydown", behind);
    fireEvent.keyDown(close, { key: "Escape" });
    window.removeEventListener("keydown", behind);
    expect(behind).not.toHaveBeenCalled();
    await waitFor(() => expect(ui("app.mic")).toBeNull());
    expect(document.activeElement).toBe(opener);
  } finally {
    rendered.unmount();
    mock.dispose();
  }
}, 15000);
