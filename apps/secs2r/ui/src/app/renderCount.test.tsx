// @vitest-environment happy-dom
// What a service event renders again (TD-176): `Shell` runs the five
// screens' hooks together, so a field that changes many times a second must
// not reach it.
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "../App";
import { createMockService } from "../ipc/mock";
import * as calibrateHook from "../screens/calibrate/useCalibrateScreen";
import * as sessions from "../state/session";
import { isActiveJob } from "../state/jobRetention";
import { installFakeCanvas } from "../test/canvas";

beforeEach(() => installFakeCanvas());
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);

function capture() {
  const original = sessions.createSession;
  const captured: { session: sessions.Session | null } = { session: null };
  vi.spyOn(sessions, "createSession").mockImplementation((ipc) => (captured.session = original(ipc)));
  const hook = calibrateHook.useCalibrateScreen;
  const runs = { calibrate: 0 };
  vi.spyOn(calibrateHook, "useCalibrateScreen").mockImplementation((...args) => { runs.calibrate++; return hook(...args); });
  return { captured, runs };
}

it.each(["connect", "calibrate"] as const)("a burst of meter readings on %s does not run the calibrate screen's hook again", async (step) => {
  const mock = createMockService({ tickMs: 0 });
  await mock.request({ method: "measure_configure", params: { method: "mock", input_device: null, output_device: null,
    input_channel: null, output_channels: [], sweep_length: null, repetitions: null,
    sweep_level_dbfs: null, timing: null, loopback_input_channel: null, timing_ref_output: null, share_mode: null } });
  const { captured, runs } = capture();
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(ui(`frame.rail.${step}`)).not.toBeNull());
    fireEvent.click(ui(`frame.rail.${step}`)!);
    const session = captured.session!;
    await waitFor(() => expect(session.host.getSnapshot().current).toBe(step));
    // 연결·점검 starts the monitor itself; elsewhere the test starts it, as nothing in the app does.
    if (step === "calibrate") await act(() => session.monitor(true));
    await waitFor(() => expect(session.jobs.getSnapshot().jobs.some((job) => job.kind === "level_monitor" && isActiveJob(job.status))).toBe(true));
    await act(() => session.settled());
    const job = session.jobs.getSnapshot().jobs.find((job) => job.kind === "level_monitor" && isActiveJob(job.status))!.job;
    // The floating mic meter (Ctrl+M) draws the readings too.
    fireEvent.keyDown(window, { key: "m", code: "KeyM", ctrlKey: true });
    await waitFor(() => expect(ui("app.mic")).not.toBeNull());
    const before = runs.calibrate;
    for (let i = 0; i < 12; i++) {
      act(() => mock.events.emit({ event: "meter", job, rms_dbfs: -40 + i, peak_dbfs: -30 + i, spl_db: null, clipping: false }));
    }
    expect(runs.calibrate - before).toBe(0);
    // The meters still show each reading as it arrives.
    expect(ui("app.mic.meter")?.textContent).toContain("29.0");
    if (step === "connect") expect(ui("connect.mic.meter")?.textContent).toContain("29.0");
  } finally { rendered.unmount(); await captured.session?.monitor(false).catch(() => {}); mock.dispose(); }
});
