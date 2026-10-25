// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { createMockService } from "../../ipc/mock";
import { createSession } from "../../state/session";
import { SessionProvider } from "../../state/react";
import { useMeasureScreen } from "../../screens/measure/useMeasureScreen";
import { useJobs } from "./useJobs";

const settle = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });
describe("tray ledger view", () => {
  it("shares one startup read and event subscription across mounts and stops only its display clock on unmount", async () => {
    vi.useFakeTimers();
    const mock = createMockService({ tickMs: 0 });
    const subscribe = vi.spyOn(mock.events, "subscribe");
    const session = createSession(mock);
    const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
    try {
      await session.start(); await settle();
      const first = renderHook(() => useJobs(), { wrapper });
      const second = renderHook(() => useJobs(), { wrapper });
      expect(mock.calls.filter((call) => call.method === "jobs")).toHaveLength(1);
      expect(subscribe).toHaveBeenCalledTimes(1);
      let job = 0;
      await act(async () => { job = (await session.call({ method: "measure_position", params: { position: 4 } })).job; });
      expect(first.result.current.state.jobs.map((job) => job.id)).toEqual([job]);
      expect(second.result.current.state.jobs.map((job) => job.id)).toEqual([job]);
      const before = first.result.current.now;
      await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
      expect(first.result.current.now).toBe(before + 1000);
      first.unmount(); second.unmount();
      expect(vi.getTimerCount()).toBe(0);
      await act(async () => { mock.drain(); await settle(); });
      const remount = renderHook(() => useJobs(), { wrapper });
      expect(remount.result.current.state.history.some((entry) => entry.type === "finished" && entry.job === job)).toBe(true);
      expect(mock.calls.filter((call) => call.method === "jobs")).toHaveLength(1);
      expect(subscribe).toHaveBeenCalledTimes(1);
      remount.unmount();
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });

  it("shares tray cancellation with the measure view and clears history for every tray consumer", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
    try {
      await session.start(); await session.configure({ method: "mock" });
      const first = renderHook(() => ({ tray: useJobs(), measure: useMeasureScreen() }), { wrapper });
      const second = renderHook(() => useJobs(), { wrapper });
      let job = 0;
      await act(async () => { job = (await session.call({ method: "measure_position", params: { position: 4 } })).job; });
      expect(first.result.current.measure.view?.job).toMatchObject({ job, position: 4, status: "running" });
      await act(async () => { first.result.current.tray.cancel(job); });
      expect(first.result.current.measure.view?.job?.status).toBe("cancel_requested");
      expect(second.result.current.state.jobs[0]?.status).toBe("cancel_requested");
      await act(async () => { mock.tick(); await settle(); });
      expect(first.result.current.measure.view?.job).toBeNull();
      expect(second.result.current.state.history).toEqual([expect.objectContaining({ type: "finished", status: "cancelled", job })]);
      act(() => first.result.current.tray.clearHistory());
      expect(second.result.current.state.history).toEqual([]);
      expect(session.jobs.getSnapshot().history).toEqual([]);
      first.unmount(); second.unmount();
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });
});
