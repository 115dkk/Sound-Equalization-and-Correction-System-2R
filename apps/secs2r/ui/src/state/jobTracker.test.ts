import { describe, expect, it, vi } from "vitest";
import type { JobSnapshot } from "../ipc/bindings/JobSnapshot";
import type { Request } from "../ipc/bindings/Request";
import { createJobTracker, type FinishedJob, type JobRequestContext, type SessionJob } from "./jobTracker";
import { TERMINAL_JOB_LIMIT } from "./jobRetention";

const snapshot = (job: number): JobSnapshot => ({ job, kind: "calibrate", status: "running", cancellable: true, progress: null, error: null });
const finished = (job: number): FinishedJob => ({ event: "finished", job, status: "succeeded", result: { kind: "calibration" }, error: null });
const request: Request = { method: "calibrate", params: { settings: null, subs_only: false } };
function setup() {
  let state: { jobs: SessionJob[]; jobRequests: Record<number, JobRequestContext> } = { jobs: [], jobRequests: {} };
  const complete = vi.fn();
  const started = vi.fn();
  const tracker = createJobTracker(() => state, (next) => { state = next; }, complete, started);
  const run = (id: number) => {
    const pending = tracker.begin(request)!;
    tracker.observeStart(id, "calibrate");
    tracker.reply(pending, { job: id, kind: "calibrate" }); tracker.settle(pending);
    tracker.finish(finished(id));
  };
  return { tracker, complete, started, run, state: () => state };
}

describe("bounded session job metadata", () => {
  it.each([false, true])("announces an accepted local start once with resolved ownership (early=%s)", (early) => {
    const { tracker, started, complete } = setup();
    const pending = tracker.begin(request)!;
    tracker.observeStart(1, "calibrate");
    expect(started).not.toHaveBeenCalled();
    if (early) tracker.finish(finished(1));
    tracker.reply(pending, { job: 1, kind: "calibrate" });
    tracker.reply(pending, { job: 1, kind: "calibrate" });
    tracker.settle(pending);
    expect(started).toHaveBeenCalledOnce();
    if (!early) tracker.finish(finished(1));
    expect(complete).toHaveBeenCalledOnce();
  });

  it("keeps the oldest active job and the latest 32 terminal jobs after 150 runs", () => {
    const { tracker, run, state } = setup();
    tracker.observeStart(1, "level_monitor");
    for (let id = 2; id <= 151; id++) run(id);
    expect(state().jobs).toHaveLength(TERMINAL_JOB_LIMIT + 1);
    expect(state().jobs[0]).toMatchObject({ job: 1, status: "running" });
    expect(state().jobs.filter((job) => job.status === "succeeded").map((job) => job.job)).toEqual(Array.from({ length: 32 }, (_, i) => i + 120));
    expect(state().jobRequests).toEqual({});
    expect(state().jobs.every((job) => !("result" in job))).toBe(true);
  });

  it("consumes an early completion once even after its terminal metadata is retired", () => {
    const { tracker, complete, run, state } = setup();
    const pending = tracker.begin({ ...request, params: { settings: null, subs_only: true } })!;
    tracker.observeStart(1, "calibrate"); tracker.finish(finished(1));
    expect(complete).not.toHaveBeenCalled();
    for (let id = 2; id <= 102; id++) run(id);
    expect(state().jobs.some((job) => job.job === 1)).toBe(false);
    tracker.reply(pending, { job: 1, kind: "calibrate" }); tracker.settle(pending);
    expect(complete).toHaveBeenCalledTimes(102);
    expect(complete.mock.calls.at(-1)?.[2]).toMatchObject({ method: "calibrate", params: { subs_only: true } });
    expect(state().jobs.some((job) => job.job === 1)).toBe(false);
    expect(state().jobRequests).toEqual({});
  });

  it("transfers a provisional early finish when the wrong same-kind owner is rejected", () => {
    const { tracker, complete, state } = setup();
    const rejected = tracker.begin(request)!;
    tracker.replaceProject();
    const accepted = tracker.begin({ ...request, params: { settings: null, subs_only: true } })!;
    tracker.observeStart(1, "calibrate"); tracker.finish(finished(1));
    tracker.settle(rejected); // its request receives JobBusy before the accepted reply
    tracker.reply(accepted, { job: 1, kind: "calibrate" }); tracker.settle(accepted);
    expect(complete).toHaveBeenCalledOnce();
    expect(complete.mock.calls[0]?.[2]).toMatchObject({ epoch: 1, params: { subs_only: true } });
    expect(state().jobRequests).toEqual({});
  });

  it("matches out-of-order same-kind replies to their own project epoch", () => {
    const { tracker, complete } = setup();
    const first = tracker.begin(request)!;
    tracker.replaceProject();
    const second = tracker.begin(request)!;
    tracker.observeStart(10, "calibrate"); tracker.finish(finished(10));
    tracker.observeStart(11, "calibrate"); tracker.finish(finished(11));
    tracker.reply(second, { job: 10, kind: "calibrate" }); tracker.settle(second);
    tracker.reply(first, { job: 11, kind: "calibrate" }); tracker.settle(first);
    expect(complete.mock.calls.map(([job, , context]) => [job.job, context.epoch])).toEqual([[10, 1], [11, 0]]);
  });

  it("does not replay duplicate finishes or resurrect retired jobs from snapshots", () => {
    const { tracker, run, state, complete } = setup();
    run(1); tracker.finish(finished(1)); tracker.observeStart(1, "calibrate");
    tracker.progress({ event: "progress", job: 1, stage: "late", done: 0, total: 1 });
    tracker.snapshots([snapshot(1)]);
    expect(complete).toHaveBeenCalledOnce();
    expect(state().jobs[0]?.status).toBe("succeeded");
    for (let id = 2; id <= 40; id++) run(id);
    tracker.snapshots([snapshot(1)]); tracker.finish(finished(1));
    expect(state().jobs).toHaveLength(32);
    expect(state().jobs.some((job) => job.job === 1)).toBe(false);
    expect(complete).toHaveBeenCalledTimes(40);
  });

  it("preserves unknown older active jobs and an unseen finish during a delayed initial snapshot", () => {
    const { tracker, run, state, complete } = setup();
    tracker.observeStart(1, "level_monitor");
    const read = tracker.beginSnapshots();
    tracker.finish(finished(40)); // started before this window subscribed
    for (let id = 100; id <= 220; id++) run(id);
    tracker.snapshots([snapshot(40), snapshot(50), snapshot(100)], read);
    tracker.endSnapshots(read);
    expect(state().jobs.some((job) => job.job === 40)).toBe(false);
    tracker.snapshots([snapshot(40)]);
    expect(state().jobs.some((job) => job.job === 40)).toBe(false);
    expect(state().jobs.find((job) => job.job === 50)?.status).toBe("running");
    expect(state().jobs.find((job) => job.job === 1)?.status).toBe("running");
    expect(state().jobs.some((job) => job.job === 100)).toBe(false);
    expect(state().jobs).toHaveLength(34);
    expect(complete.mock.calls.filter(([job]) => job.job === 40)).toHaveLength(0);
  });

  it("keeps recent completions when the delayed initial snapshot contains 32 older terminals", () => {
    const { tracker, run, state } = setup();
    const read = tracker.beginSnapshots();
    for (let id = 100; id < 200; id++) run(id);
    tracker.snapshots([
      ...Array.from({ length: 32 }, (_, i) => ({ ...snapshot(i + 1), status: "succeeded" as const, cancellable: false })),
      snapshot(50),
    ], read);
    tracker.endSnapshots(read);
    expect(state().jobs.filter((job) => job.status === "succeeded").map((job) => job.job)).toEqual(Array.from({ length: 32 }, (_, i) => i + 168));
    expect(state().jobs.find((job) => job.job === 50)?.status).toBe("running");
  });

  it("preserves a first Started whose allocated id arrives after newer jobs have retired", () => {
    const { tracker, run, state, complete } = setup();
    tracker.observeStart(1, "measure");
    for (let id = 100; id < 140; id++) run(id);
    tracker.observeStart(50, "export");
    tracker.observeStart(50, "export"); // a known retained id remains deduplicated
    expect(state().jobs.filter((job) => job.status === "running").map((job) => job.job)).toEqual([1, 50]);
    expect(state().jobs).toHaveLength(34);
    const outcome: FinishedJob = { event: "finished", job: 50, status: "succeeded", result: { kind: "export", data: { files: ["old.yml"], folder: "C:/old", stale: false } }, error: null };
    tracker.finish(outcome); tracker.finish(outcome);
    expect(complete.mock.calls.filter(([job]) => job.job === 50)).toHaveLength(1);
    expect(state().jobs.filter((job) => job.status === "running").map((job) => job.job)).toEqual([1]);
  });

  it("accepts an owned reply before delayed Started despite its id lying inside the snapshot fence", () => {
    const { tracker, run, state, started, complete } = setup();
    tracker.observeStart(1, "measure");
    const pending = tracker.begin({ method: "export", params: { kind: "camilla_dsp", folder: "C:/old" } })!;
    tracker.replaceProject();
    for (let id = 100; id < 140; id++) run(id);
    tracker.reply(pending, { job: 50, kind: "export" });
    tracker.reply(pending, { job: 50, kind: "export" });
    tracker.settle(pending);
    expect(state().jobs.find((job) => job.job === 50)).toMatchObject({ status: "running", kind: "export", epoch: 0 });
    expect(state().jobRequests[50]).toMatchObject({ method: "export", epoch: 0, params: { kind: "camilla_dsp", folder: "C:/old" } });
    tracker.observeStart(50, "export");
    expect(started.mock.calls.filter(([job]) => job.job === 50)).toHaveLength(1);
    const outcome: FinishedJob = { event: "finished", job: 50, status: "succeeded", result: { kind: "export", data: { files: ["old.yml"], folder: "C:/old", stale: false } }, error: null };
    tracker.finish(outcome); tracker.finish(outcome);
    const completions = complete.mock.calls.filter(([job]) => job.job === 50);
    expect(completions).toHaveLength(1);
    expect(completions[0]?.[2]).toMatchObject({ method: "export", epoch: 0 });
    expect(state().jobRequests[50]).toBeUndefined();
    expect(state().jobs.filter((job) => job.status === "running").map((job) => job.job)).toEqual([1]);
  });

  it("does not keep external completions waiting for unrelated future requests", () => {
    const { tracker, complete, state } = setup();
    for (let id = 1; id <= 100; id++) { tracker.observeStart(id, "calibrate"); tracker.finish(finished(id)); }
    const next = tracker.begin(request)!;
    tracker.observeStart(101, "calibrate"); tracker.reply(next, { job: 101, kind: "calibrate" }); tracker.settle(next);
    expect(complete).toHaveBeenCalledTimes(100);
    tracker.finish(finished(101));
    expect(complete).toHaveBeenCalledTimes(101);
    expect(state().jobs).toHaveLength(32);
    expect(state().jobRequests).toEqual({});
  });
});
