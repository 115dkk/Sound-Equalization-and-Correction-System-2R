// @vitest-environment happy-dom
// The measurement curves are read for 측정·가져오기 only (TD-237).
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "../App";
import { createMockService } from "../ipc/mock";
import type { Ipc } from "../ipc/context";
import type { Request } from "../ipc/bindings/Request";
import { translate } from "../i18n";
import { installFakeCanvas } from "../test/canvas";
import * as sessions from "./session";

beforeEach(() => installFakeCanvas());
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const chooseMock = (mock: ReturnType<typeof createMockService>) => mock.request({ method: "measure_configure", params: { method: "mock", input_device: null, output_device: null,
  input_channel: null, output_channels: [], sweep_length: null, repetitions: null,
  sweep_level_dbfs: null, timing: null, loopback_input_channel: null, timing_ref_output: null, share_mode: null } });
function capture() {
  const original = sessions.createSession;
  const captured: { session: sessions.Session | null } = { session: null };
  vi.spyOn(sessions, "createSession").mockImplementation((ipc) => (captured.session = original(ipc)));
  return captured;
}

it.each<Request>([
  { method: "position_clear", params: { position: 1 } },
  { method: "config_renumber" },
])("reads the curves only when 측정·가져오기 is shown, after $method on 보정", async (change) => {
  const mock = createMockService({ tickMs: 0 });
  await chooseMock(mock);
  const captured = capture();
  const curves = () => mock.calls.filter((call) => call.method === "measurement_curves");
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui("frame.rail.calibrate")).not.toBeNull());
    fireEvent.click(ui("frame.rail.calibrate")!);
    await waitFor(() => expect(ui("calibrate.compute")).not.toBeNull());
    const session = captured.session!;
    await act(async () => { await session.call(change); await session.settled(); });
    expect(curves()).toHaveLength(0);
    fireEvent.click(ui("frame.rail.measure")!);
    await waitFor(() => expect(ui("measure.graph")).not.toBeNull());
    await act(() => session.settled());
    expect(curves()).toHaveLength(1);
  } finally { rendered.unmount(); mock.dispose(); }
});

it("draws a loading state in place of the empty one while the first read runs, and keeps the curves with a status while a reread runs", async () => {
  const mock = createMockService({ tickMs: 0 });
  await chooseMock(mock);
  const gates: (() => void)[] = [];
  const ipc: Ipc = { ...mock, request: async (request) => {
    if (request.method === "measurement_curves") await new Promise<void>((resolve) => gates.push(resolve));
    return mock.request(request);
  } };
  const captured = capture();
  const rendered = render(<App ipc={ipc} />);
  try {
    await waitFor(() => expect(ui("frame.rail.measure")).not.toBeNull());
    fireEvent.click(ui("frame.rail.measure")!);
    const session = captured.session!;
    await waitFor(() => expect(ui("measure.graph.loading")).not.toBeNull());
    expect(ui("measure.graph.loading")?.textContent).toBe(translate("ko", "measure.graph.loading"));
    expect(ui("measure.graph.empty")).toBeNull();
    expect(gates).toHaveLength(1);
    await act(async () => { gates.shift()!(); await session.settled(); });
    expect(ui("measure.plot")).not.toBeNull();
    expect(ui("measure.graph.loading")).toBeNull();
    expect(ui("measure.graph.refreshing")?.textContent).toBe("");
    // A change while the screen is shown reads the curves again at once.
    let change!: Promise<unknown>;
    act(() => { change = session.call({ method: "position_clear", params: { position: 1 } }); });
    await waitFor(() => expect(gates).toHaveLength(1));
    await waitFor(() => expect(ui("measure.graph.refreshing")?.textContent).toBe(translate("ko", "measure.graph.refreshing")));
    expect(ui("measure.plot")).not.toBeNull();
    expect(ui("measure.graph.empty")).toBeNull();
    await act(async () => { gates.shift()!(); await change; await session.settled(); });
    expect(ui("measure.graph.refreshing")?.textContent).toBe("");
    expect(ui("measure.plot")).not.toBeNull();
  } finally { for (const release of gates) release(); rendered.unmount(); mock.dispose(); }
});
