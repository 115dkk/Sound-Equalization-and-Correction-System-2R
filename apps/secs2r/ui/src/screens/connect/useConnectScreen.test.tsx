// @vitest-environment happy-dom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { createMockService } from "../../ipc/mock";
import { SessionProvider } from "../../state/react";
import { createSession } from "../../state/session";
import { levelRoles } from "./state";
import { useConnectScreen } from "./useConnectScreen";

let mock: ReturnType<typeof createMockService>;
let session: ReturnType<typeof createSession>;
let pending: Promise<unknown>[];
const ticks = (count: number) => { for (let i = 0; i < count; i++) mock.tick(); };
const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
async function perform(work: () => unknown) {
  await act(async () => { await work(); while (pending.length) await Promise.all(pending.splice(0)); });
}
beforeEach(async () => {
  mock = createMockService({ tickMs: 0 }); session = createSession(mock); await session.start();
  pending = []; vi.spyOn(session, "run").mockImplementation((work) => { pending.push(work().catch(() => {})); });
  mock.calls.length = 0;
});
afterEach(async () => {
  cleanup(); while (pending.length) await Promise.all(pending.splice(0));
  await session.monitor(false); session.dispose(); mock.dispose(); vi.restoreAllMocks(); vi.useRealTimers();
});

describe("useConnectScreen", () => {
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
      [() => a.setSweepLength(65536), { sweep_length: 65536 }],
      [() => a.setLoopbackChannel(2), { loopback_input_channel: 2 }],
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

  it("restarts monitoring on path and device changes and after failed sweep jobs", async () => {
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
    await perform(() => result.current.actions.choosePath("mock"));
    await perform(() => {});
    mock.failJobs("level_check", "internal");
    await perform(() => result.current.actions.checkSweepLevel("L"));
    await perform(() => {});
    expect(result.current.view?.sweepCheck.running).toBe(true);
    await perform(mock.drain);
    await perform(() => {});
    expect(result.current.view?.sweepCheck).toMatchObject({ running: false, result: null });
    expect(result.current.view?.level.monitor).toBe("starting");
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
    await perform(result.current.actions.launchRew);
    expect(mock.calls).toContainEqual({ method: "rew_launch" });
    expect(result.current.view?.rew.problem).toBeNull();
  });

  it("plays levels in row order, fills defaults and stops monitoring before sweep checks", async () => {
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
    await perform(() => a.checkSweepLevel("L"));
    expect(mock.calls).toContainEqual({ method: "level_check", params: { role: "L" } });
    expect(result.current.view?.sweepCheck).toMatchObject({ running: true, role: "L", result: null });
    const methods = mock.calls.map((call) => call.method);
    expect(methods.lastIndexOf("level_monitor_stop")).toBeLessThan(methods.lastIndexOf("level_check"));
    await perform(mock.drain);
    const level = result.current.view!.sweepCheck.result!;
    expect(level).not.toBeNull();
    await perform(() => a.applySweepLevel(level));
    expect(mock.calls).toContainEqual({ method: "level_apply", params: { level } });
    expect(result.current.view?.settings?.sweep_level_dbfs).toBe(level.recommended_sweep_dbfs);
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
    await act(async () => { releases[0]!(); while (pending.length) await Promise.all(pending.splice(0)); });
    expect(mock.calls.filter((call) => call.method === "level_play")).toHaveLength(1);
    expect(result.current.view?.level).toMatchObject({ playing: null, playingAll: false });
  });
});
