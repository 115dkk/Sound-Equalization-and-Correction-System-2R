// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { Ipc, RequestFn } from "../../ipc/context";
import type { JobSnapshot } from "../../ipc/bindings/JobSnapshot";
import type { Request } from "../../ipc/bindings/Request";
import { createEventBus } from "../../ipc/events";
import { activeJobs } from "./jobStore";
import { useJobs } from "./useJobs";

const snapshot = (job: number): JobSnapshot => ({ job, kind: "calibrate", status: "running", cancellable: true, progress: null, error: null });
afterEach(cleanup);
describe("tray snapshot races", () => {
  it("keeps older unknown active jobs without reviving an unknown finish after 100 newer completions", async () => {
    const events = createEventBus();
    let resolve!: (jobs: JobSnapshot[]) => void;
    const pending = new Promise<JobSnapshot[]>((done) => { resolve = done; });
    const ipc: Ipc = { events, request: (async (request: Request) => {
      if (request.method === "jobs") return pending;
      throw new Error("unexpected request");
    }) as RequestFn };
    const { result } = renderHook(() => useJobs(ipc));
    act(() => {
      events.emit({ event: "finished", job: 2, status: "succeeded", result: { kind: "calibration" }, error: null });
      for (let job = 100; job < 200; job++) {
        events.emit({ event: "started", job, kind: "calibrate" });
        events.emit({ event: "finished", job, status: "succeeded", result: { kind: "calibration" }, error: null });
      }
    });
    await act(async () => {
      resolve([snapshot(2), snapshot(3), ...Array.from({ length: 32 }, (_, i) => ({ ...snapshot(i + 4), status: "succeeded" as const, cancellable: false }))]);
      await pending;
    });
    expect(activeJobs(result.current.state).map((job) => job.id)).toEqual([3]);
    expect(result.current.state.jobs.some((job) => job.id === 2)).toBe(false);
    expect(result.current.state.jobs.filter((job) => job.status === "succeeded").map((job) => job.id)).toEqual(Array.from({ length: 32 }, (_, i) => i + 168));
    expect(result.current.state.history).toHaveLength(100);
  });

  it("ignores a delayed cancellation snapshot after its completed job was retired", async () => {
    const events = createEventBus();
    let resolve!: (job: JobSnapshot) => void;
    const pending = new Promise<JobSnapshot>((done) => { resolve = done; });
    const ipc: Ipc = { events, request: (async (request: Request) => {
      if (request.method === "jobs") return [];
      if (request.method === "job_cancel") return pending;
      throw new Error("unexpected request");
    }) as RequestFn };
    const { result } = renderHook(() => useJobs(ipc));
    await act(async () => { await Promise.resolve(); });
    act(() => events.emit({ event: "started", job: 1, kind: "calibrate" }));
    act(() => result.current.cancel(1));
    act(() => {
      events.emit({ event: "finished", job: 1, status: "cancelled", result: null, error: null });
      for (let job = 2; job <= 40; job++) {
        events.emit({ event: "started", job, kind: "calibrate" });
        events.emit({ event: "finished", job, status: "succeeded", result: { kind: "calibration" }, error: null });
      }
    });
    await act(async () => { resolve({ ...snapshot(1), status: "cancel_requested" }); await pending; });
    expect(activeJobs(result.current.state)).toEqual([]);
    expect(result.current.state.jobs).toHaveLength(32);
    expect(result.current.state.jobs.some((job) => job.id === 1)).toBe(false);
  });
});
