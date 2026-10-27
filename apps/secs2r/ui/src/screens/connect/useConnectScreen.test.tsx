// @vitest-environment happy-dom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { createMockService } from "../../ipc/mock";
import { IpcFailure } from "../../ipc/client";
import { SessionProvider } from "../../state/react";
import { createSession } from "../../state/session";
import { levelRoles } from "./state";
import { useConnectScreen } from "./useConnectScreen";

let mock: ReturnType<typeof createMockService>;
let session: ReturnType<typeof createSession>;
const ticks = (count: number) => { for (let i = 0; i < count; i++) mock.tick(); };
const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
async function perform(work: () => unknown) {
  await act(async () => { await work(); await session.settled(); });
}
beforeEach(async () => {
  mock = createMockService({ tickMs: 0 }); session = createSession(mock); await session.start();
  mock.calls.length = 0;
});
afterEach(async () => {
  cleanup(); await session.settled();
  await session.monitor(false); session.dispose(); mock.dispose(); vi.restoreAllMocks(); vi.useRealTimers();
});

describe("useConnectScreen mic calibration", () => {
  it("loads the file only on an active native path and exposes the resource", async () => {
    await session.configure({ method: "native" });
    session.stores.micCalibration.set(null);
    mock.calls.length = 0;
    const { result, rerender } = renderHook(({ active }) => useConnectScreen(active), { wrapper, initialProps: { active: false } });
    expect(result.current.view?.micCalibration).toEqual({ status: null, busy: null, error: null });
    expect(mock.calls).toEqual([]);
    await perform(() => rerender({ active: true }));
    expect(mock.calls.filter((call) => call.method === "mic_calibration_get")).toHaveLength(1);
    expect(result.current.view?.micCalibration.status).toEqual(session.stores.micCalibration.getSnapshot());
    await perform(() => rerender({ active: false }));
    await perform(() => result.current.actions.choosePath("rew"));
    mock.calls.length = 0;
    await perform(() => rerender({ active: true }));
    expect(mock.calls.some((call) => call.method === "mic_calibration_get")).toBe(false);
    await perform(() => result.current.actions.choosePath("native"));
    expect(mock.calls.filter((call) => call.method === "mic_calibration_get")).toHaveLength(1);
  });

  it("picks, attaches and clears a file while cancellation leaves it unchanged", async () => {
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    const a = result.current.actions;
    const count = result.current.view!.micCalibration.status!.attachable;
    await perform(a.pickMicCalibration);
    expect(mock.calls).toContainEqual({ method: "dialog_open_file", params: { kind: "mic_calibration" } });
    expect(mock.calls).toContainEqual({ method: "mic_calibration_set", params: { path: "C:/Users/demo/Documents/UMIK-1/7000001_90deg.txt" } });
    const selected = result.current.view!.micCalibration.status;
    expect(selected?.file).toMatchObject({ name: "7000001_90deg.txt", points: 615 });
    mock.queueDialog("dialog_open_file", null); mock.calls.length = 0;
    await perform(a.pickMicCalibration);
    expect(result.current.view?.micCalibration.status).toEqual(selected);
    expect(mock.calls).toEqual([{ method: "dialog_open_file", params: { kind: "mic_calibration" } }]);
    await perform(a.attachMicCalibration);
    expect(result.current.view?.micCalibration.status).toMatchObject({ attached: count, attachable: 0 });
    await perform(a.clearMicCalibration);
    expect(result.current.view?.micCalibration).toEqual({ status: { file: null, attached: count, attachable: 0 }, busy: null, error: null });
    expect(session.stores.project.getSnapshot()?.dirty).toBe(true);
  });

  it.each([
    ["dialog_open_file", "pickMicCalibration"], ["mic_calibration_set", "pickMicCalibration"],
    ["mic_calibration_clear", "clearMicCalibration"], ["mic_calibration_attach", "attachMicCalibration"],
  ] as const)("keeps %s failures in the field, then clears them on the next attempt", async (method, action) => {
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    await perform(result.current.actions.pickMicCalibration);
    const globalErrors: unknown[] = [];
    const off = session.errors.subscribe(() => { if (session.errors.getSnapshot()) globalErrors.push(session.errors.getSnapshot()); });
    try {
      mock.failNext(method, "measurement_input", "invalid mic curve", { reason: "mic_calibration", issue: "row", file: "bad.txt", line: "3" });
      await perform(result.current.actions[action]);
      expect(result.current.view?.micCalibration).toMatchObject({ busy: null, error: { code: "measurement_input", params: { issue: "row", line: "3" } } });
      expect(globalErrors).toEqual([]);
      await perform(result.current.actions[action]);
      expect(result.current.view?.micCalibration).toMatchObject({ busy: null, error: null });
    } finally { off(); }
  });

  it.each([
    ["mic_calibration_set", "pickMicCalibration", "pick"],
    ["mic_calibration_clear", "clearMicCalibration", "clear"],
    ["mic_calibration_attach", "attachMicCalibration", "attach"],
  ] as const)("holds %s busy until the reply and ignores duplicate actions", async (method, action, busy) => {
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    await perform(result.current.actions.pickMicCalibration);
    let release!: () => void;
    const blocked = new Promise<void>((resolve) => { release = resolve; });
    const original = mock.request;
    vi.spyOn(mock, "request").mockImplementation(async (request) => {
      if (request.method === method) await blocked;
      return original(request);
    });
    mock.calls.length = 0;
    try {
      act(result.current.actions[action]);
      expect(result.current.view?.micCalibration.busy).toBe(busy);
      act(result.current.actions[action]);
      await act(async () => { release(); await session.settled(); });
      expect(mock.calls.filter((call) => call.method === method)).toHaveLength(1);
      expect(result.current.view?.micCalibration.busy).toBeNull();
    } finally { release(); }
  });

  it.each([false, true])("discards an old project's file dialog reply (failure: %s)", async (failed) => {
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    const original = mock.request;
    let release!: () => void;
    const blocked = new Promise<void>((resolve) => { release = resolve; });
    vi.spyOn(mock, "request").mockImplementation(async (request) => {
      if (request.method === "dialog_open_file") {
        await blocked;
        if (failed) throw new IpcFailure({ code: "measurement_input", params: {}, retryable: false, detail: "old dialog" });
      }
      return original(request);
    });
    try {
      act(result.current.actions.pickMicCalibration);
      expect(result.current.view?.micCalibration.busy).toBe("pick");
      await act(async () => { await session.call({ method: "project_new", params: { path: "/new.secs", preset: "2.0", sub_count: 0, sample: false, tutorial: false } }); });
      expect(result.current.view?.micCalibration).toEqual({ status: { file: null, attached: 0, attachable: 0 }, busy: null, error: null });
      await act(async () => { release(); await session.settled(); });
      expect(mock.calls.some((call) => call.method === "mic_calibration_set")).toBe(false);
      expect(result.current.view?.micCalibration.error).toBeNull();
    } finally { release(); }
  });

  it("preserves reporting and rethrowing of unexpected errors", async () => {
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    const error = new Error("unexpected failure");
    const original = mock.request;
    vi.spyOn(mock, "request").mockImplementation(async (request) => {
      if (request.method === "mic_calibration_clear") throw error;
      return original(request);
    });
    let work: Promise<unknown> | undefined;
    vi.spyOn(session, "run").mockImplementation((run) => { work = run(); });
    await act(async () => {
      result.current.actions.clearMicCalibration();
      await expect(work).rejects.toBe(error);
    });
    expect(result.current.view?.micCalibration).toMatchObject({ busy: null, error: null });
    expect(session.errors.getSnapshot()?.error).toMatchObject({ code: "internal", detail: "Error: unexpected failure" });
  });
});

describe("useConnectScreen", () => {
  it("releases the end subscription and timer immediately when level_play fails", async () => {
    vi.useFakeTimers();
    const subscribe = mock.events.subscribe;
    const releases: ReturnType<typeof vi.fn>[] = [];
    vi.spyOn(mock.events, "subscribe").mockImplementation((listener) => {
      const release = vi.fn(subscribe(listener)); releases.push(release); return release;
    });
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    await perform(() => result.current.actions.choosePath("mock"));
    mock.failNext("level_play", "device_unavailable");
    await perform(() => result.current.actions.playLevel("L"));
    expect(releases).toHaveLength(1);
    expect(releases[0]).toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
    expect(result.current.view?.level.playing).toBeNull();
  });
  it("chooses every path and wires every device preference with nullable fields", async () => {
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    const a = result.current.actions;
    for (const path of ["mock", "native", "rew", "mdat"] as const) {
      await perform(() => a.choosePath(path));
      expect(result.current.view?.path).toBe(path);
      expect(mock.calls).toContainEqual({ method: "measure_configure", params: expect.objectContaining({ method: path, output_channels: [], input_device: null, sweep_length: null }) });
    }
    await perform(() => a.choosePath("mock"));
    for (const [work, patch] of [
      [() => a.setInputDevice("input"), { input_device: "input" }],
      [() => a.setInputChannel(2), { input_channel: 2 }],
      [() => a.setOutputDevice("output"), { output_device: "output" }],
      [() => a.setShareMode("shared"), { share_mode: "shared" }],
    ] as const) {
      await perform(work);
      expect(mock.calls).toContainEqual({ method: "measure_configure", params: {
        method: "mock", input_device: null, output_device: null, input_channel: null, output_channels: [], sweep_length: null,
        repetitions: null, sweep_level_dbfs: null, timing: null, loopback_input_channel: null, timing_ref_output: null, share_mode: null, ...patch,
      } });
    }
    await perform(a.refreshMethods);
    expect(mock.calls).toContainEqual({ method: "measure_methods" });
    mock.calls.length = 0;
    await perform(() => { a.setMasterLevel(-14); a.next(); });
    expect(result.current.view?.level.masterDbfs).toBe(-14);
    expect(session.host.getSnapshot().current).toBe("measure");
    expect(mock.calls).toEqual([]);
  });

  it("switches REW's driver from its reply and reads REW again after opening a chosen REW device", async () => {
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    await perform(() => result.current.actions.choosePath("rew"));
    await perform(() => session.stores.rew.load());
    expect(result.current.view?.rew.details.driver).toBe("Java");
    mock.calls.length = 0;
    await perform(() => result.current.actions.setRewDriver("ASIO"));
    expect(mock.calls).toContainEqual({ method: "rew_driver_set", params: { driver: "ASIO" } });
    expect(mock.calls).not.toContainEqual({ method: "rew_info" });
    expect(result.current.view?.rew.details.driver).toBe("ASIO");
    expect(result.current.view?.rew.switchingDriver).toBe(false);
    mock.calls.length = 0;
    await perform(() => result.current.actions.setOutputDevice("Speakers (Realtek)"));
    const methods = mock.calls.map((call) => call.method);
    expect(methods.indexOf("rew_info")).toBeGreaterThan(methods.indexOf("measure_configure"));
    expect(methods.indexOf("measure_configure")).toBe(0);
    expect(result.current.view?.rew.details.current?.outputDevice).toBe("Speakers (Realtek)");
  });

  it("refreshes REW on each opening, monitors only a ready active path, and stops on leaving", async () => {
    await session.configure({ method: "mock" });
    const { result, rerender } = renderHook(({ active }) => useConnectScreen(active), { wrapper, initialProps: { active: false } });
    await perform(() => rerender({ active: true }));
    expect(mock.calls).toContainEqual({ method: "rew_info" });
    expect(mock.calls).toContainEqual({ method: "level_monitor_start", params: { role: null } });
    await perform(mock.tick);
    expect(result.current.view?.level.reading).not.toBeNull();
    await perform(() => rerender({ active: false }));
    expect(mock.calls).toContainEqual({ method: "level_monitor_stop" });
    expect(result.current.view?.level.monitor).toBe("off");
    mock.calls.length = 0;
    await perform(() => rerender({ active: true }));
    expect(mock.calls).toContainEqual({ method: "rew_info" });
    expect(mock.calls).toContainEqual({ method: "level_monitor_start", params: { role: null } });
  });

  it("restarts monitoring on path and device changes", async () => {
    await session.configure({ method: "mock" });
    const { result } = renderHook(() => useConnectScreen(true), { wrapper });
    await perform(() => {});
    mock.calls.length = 0;
    await perform(() => result.current.actions.choosePath("native"));
    await perform(() => {});
    expect(result.current.view?.level.monitor).toBe("starting");
    expect(mock.calls).toContainEqual({ method: "level_monitor_start", params: { role: null } });
    mock.calls.length = 0;
    await perform(() => result.current.actions.setInputChannel(2));
    await perform(() => {});
    expect(mock.calls).toContainEqual({ method: "level_monitor_stop" });
    expect(mock.calls).toContainEqual({ method: "level_monitor_start", params: { role: null } });
  });

  it("loads REW details and handles typed launch failures without rejecting callbacks", async () => {
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    await perform(result.current.actions.readRewSettings);
    expect(mock.calls).toContainEqual({ method: "rew_info" });
    expect(result.current.view?.rew.readingSettings).toBe(false);
    expect(result.current.view?.rew.details.executable).toBe(session.stores.rew.getSnapshot()?.executable_path);
    mock.failNext("rew_launch", "rew_failed", "not installed", { reason: "exe_not_found" });
    await perform(result.current.actions.launchRew);
    expect(result.current.view?.rew).toMatchObject({ launch: "idle", problem: "exe_not_found", problemDetail: "not installed" });
    mock.calls.length = 0;
    await perform(result.current.actions.launchRew);
    expect(mock.calls).toEqual([{ method: "rew_launch" }, { method: "measure_methods" }, { method: "rew_info" }]);
    expect(result.current.view?.rew.problem).toBeNull();
  });

  it("plays levels in row order and fills defaults", async () => {
    await session.configure({ method: "mock" });
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    const a = result.current.actions;
    const plays = () => mock.calls.filter((call) => call.method === "level_play");
    act(() => a.playLevel("L"));
    await waitFor(() => expect(mock.calls).toContainEqual({ method: "level_play", params: { role: "L", master_dbfs: -20 } }));
    // The mock burst ends after five monitor ticks with level_play_end.
    await act(async () => { ticks(5); });
    await perform(() => {});
    expect(result.current.view?.level).toMatchObject({ playing: null, playingAll: false });
    await perform(a.stopLevel);
    expect(mock.calls).toContainEqual({ method: "level_stop" });
    mock.calls.length = 0;
    act(a.playAllLevels);
    const roles = levelRoles(result.current.view!);
    for (let i = 0; i < roles.length; i++) {
      await waitFor(() => expect(plays()).toHaveLength(i + 1));
      await act(async () => { ticks(5); });
    }
    await perform(() => {});
    expect(plays().map((call) => call.params.role)).toEqual(roles);
    expect(mock.calls).toContainEqual({ method: "project_info" });
    expect(result.current.view?.level).toMatchObject({ playing: null, playingAll: false });
    await perform(a.skipLevels);
    expect(mock.calls).toContainEqual({ method: "levels_fill_defaults" });
  });

  it("moves on when the monitor reports the burst ended, and stop ends the wait", async () => {
    await session.configure({ method: "mock" });
    const plays = () => mock.calls.filter((call) => call.method === "level_play");
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    act(result.current.actions.playAllLevels);
    await waitFor(() => expect(plays()).toHaveLength(1));
    await act(async () => { ticks(4); });
    expect(plays()).toHaveLength(1);
    await act(async () => { ticks(1); });
    await waitFor(() => expect(plays()).toHaveLength(2));
    await perform(result.current.actions.stopLevel);
    expect(result.current.view?.level).toMatchObject({ playing: null, playingAll: false });
    await act(async () => { ticks(10); });
    expect(plays()).toHaveLength(2);
  });

  it("gives up on a lost end event after 8 s, and stop cancels that timer", async () => {
    await session.configure({ method: "mock" });
    vi.useFakeTimers();
    const plays = () => mock.calls.filter((call) => call.method === "level_play");
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    act(result.current.actions.playAllLevels);
    await act(async () => { await vi.advanceTimersByTimeAsync(7999); });
    expect(plays()).toHaveLength(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(plays()).toHaveLength(2);
    await perform(result.current.actions.stopLevel);
    expect(vi.getTimerCount()).toBe(0);
    expect(result.current.view?.level).toMatchObject({ playing: null, playingAll: false });
  });

  it("awaits each blocking burst and a stop cancels the remaining local queue", async () => {
    await session.configure({ method: "mock" });
    const original = mock.request;
    const releases: (() => void)[] = [];
    vi.spyOn(mock, "request").mockImplementation(async (request) => {
      const value = await original(request);
      if (request.method === "level_play") await new Promise<void>((resolve) => releases.push(resolve));
      return value;
    });
    const { result } = renderHook(() => useConnectScreen(false), { wrapper });
    act(() => result.current.actions.playAllLevels());
    await waitFor(() => expect(releases).toHaveLength(1));
    expect(mock.calls.filter((call) => call.method === "level_play")).toHaveLength(1);
    act(() => result.current.actions.stopLevel());
    await act(async () => { releases[0]!(); await session.settled(); });
    expect(mock.calls.filter((call) => call.method === "level_play")).toHaveLength(1);
    expect(result.current.view?.level).toMatchObject({ playing: null, playingAll: false });
  });
});
