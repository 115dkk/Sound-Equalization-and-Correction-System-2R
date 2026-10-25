import { describe, expect, it, vi } from "vitest";
import type { RequestFn } from "../ipc/context";
import { IpcFailure } from "../ipc/client";
import type { JobSnapshot } from "../ipc/bindings/JobSnapshot";
import type { Request } from "../ipc/bindings/Request";
import { activeJobs, createJobLedger, elapsedMs, errorCount, initialLedger, ledgerReducer, remainingMs, trayJobs, type FinishedJob } from "./jobLedger";

const snapshot = (job: number): JobSnapshot => ({ job, kind: "calibrate", status: "running", cancellable: true, progress: null, error: null });
const finished = (job: number): FinishedJob => ({ event: "finished", job, status: "succeeded", result: { kind: "calibration" }, error: null });
const request: Request = { method: "calibrate", params: { settings: null, subs_only: false } };
function setup(raw: RequestFn = (async () => []) as RequestFn) {
  const complete = vi.fn();
  const started = vi.fn();
  let at = 0;
  const ledger = createJobLedger({ request: raw, now: () => at, finished: complete, started });
  const run = (job: number) => {
    const pending = ledger.begin(request)!;
    ledger.event({ event: "started", job, kind: "calibrate" });
    ledger.reply(pending, { job, kind: "calibrate" }); ledger.settle(pending);
    ledger.event(finished(job));
  };
  return { ledger, complete, started, run, state: ledger.getSnapshot, clock: (value: number) => { at = value; } };
}

describe("job ledger", () => {
  it("preserves the completion payload when a terminal cancel reply arrives before Finished", async () => {
    const { ledger, state, complete } = setup((async () => ({ ...snapshot(1), kind: "export", status: "succeeded", cancellable: false })) as RequestFn);
    const pending = ledger.begin({ method: "export", params: { kind: "camilla_dsp", folder: "C:/room" } })!;
    ledger.reply(pending, { job: 1, kind: "export" }); ledger.settle(pending);
    await ledger.cancel(1);
    expect(state().jobs[0]?.status).toBe("succeeded");
    const outcome: FinishedJob = { event: "finished", job: 1, status: "succeeded", result: { kind: "export", data: { files: ["room.yml"], folder: "C:/room", stale: false } }, error: null };
    ledger.event(outcome);
    expect(complete).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ job: 1, request: null }), outcome, expect.objectContaining({ method: "export" }));
    expect(state().history.filter((entry) => entry.type === "finished")).toHaveLength(1);
  });
  it("exposes event observation order to fence a snapshot between progress and completion", () => {
    const { ledger, state } = setup();
    ledger.event({ event: "started", job: 1, kind: "calibrate" });
    expect(state().observedOrder).toBe(1);
    ledger.event({ event: "progress", job: 1, stage: "design", done: 1, total: 2 });
    expect(state().observedOrder).toBe(2);
    ledger.event({ event: "log", job: 1, code: "note", params: {} });
    expect(state().observedOrder).toBe(3);
    ledger.event(finished(1));
    expect(state().jobs[0]?.completionOrder).toBe(4);
  });
  it("merges a cancel reply's metadata without undoing optimistic cancellation or newer progress", async () => {
    const { ledger, state } = setup((async () => ({ ...snapshot(1), cancellable: false })) as RequestFn);
    ledger.event({ event: "started", job: 1, kind: "calibrate" });
    ledger.event({ event: "progress", job: 1, stage: "new", done: 3, total: 8 });
    await ledger.cancel(1);
    expect(state().jobs[0]).toMatchObject({ status: "cancel_requested", cancellable: false, progress: { stage: "new", done: 3, total: 8 } });
  });
  it("replays gallery events without mutating its input and ignores non-job events", () => {
    const initial = structuredClone(initialLedger);
    const state = ledgerReducer(initial, { type: "event", at: 100, event: { event: "started", job: 1, kind: "measure" } });
    expect(initial).toEqual(initialLedger);
    const progressed = ledgerReducer(state, { type: "event", at: 200, event: { event: "progress", job: 1, stage: "L", done: 1, total: 4 } });
    expect(state.jobs[0]?.progress).toBeNull();
    expect(progressed.jobs[0]).toMatchObject({ progress: { stage: "L", done: 1, total: 4 }, startedAt: 100, stageStartedAt: 200 });
    expect(ledgerReducer(progressed, { type: "event", at: 300, event: { event: "meter", job: 1, rms_dbfs: -22, peak_dbfs: -10, spl_db: null, clipping: false } })).toBe(progressed);
    expect(ledgerReducer(initial, { type: "event", at: 0, event: { event: "progress", job: 5, stage: "x", done: 1, total: 2 } })).toBe(initial);
  });

  it("notifies subscribers with stable snapshots and detaches listeners", () => {
    const { ledger, state } = setup();
    const changed = vi.fn();
    const off = ledger.subscribe(changed);
    const before = state();
    expect(state()).toBe(before);
    ledger.event({ event: "progress", job: 5, stage: "x", done: 1, total: 2 });
    expect(changed).not.toHaveBeenCalled();
    ledger.event({ event: "started", job: 1, kind: "measure" });
    expect(changed).toHaveBeenCalledOnce();
    expect(state()).not.toBe(before);
    off(); ledger.event(finished(1));
    expect(changed).toHaveBeenCalledOnce();
  });

  it("restarts the estimate on a new count but keeps one count across measurement roles", () => {
    const { ledger, state, clock } = setup();
    ledger.event({ event: "started", job: 1, kind: "measure" });
    expect(remainingMs(state().jobs[0]!, 500)).toBeNull();
    clock(1000); ledger.event({ event: "progress", job: 1, stage: "L", done: 1, total: 4 });
    clock(3000); ledger.event({ event: "progress", job: 1, stage: "R", done: 2, total: 4 });
    expect(remainingMs(state().jobs[0]!, 3000)).toBe(4000);
    clock(4000); ledger.event({ event: "progress", job: 1, stage: "next", done: 0, total: 11 });
    expect(remainingMs(state().jobs[0]!, 5000)).toBeNull();
    clock(6000); ledger.event({ event: "progress", job: 1, stage: "next", done: 11, total: 11 });
    expect(remainingMs(state().jobs[0]!, 6000)).toBeNull();
  });

  it("keeps newer progress and finishes over a delayed startup snapshot", async () => {
    let release!: (jobs: JobSnapshot[]) => void;
    const delayed = new Promise<JobSnapshot[]>((resolve) => { release = resolve; });
    const { ledger, state, complete, clock } = setup((async () => delayed) as RequestFn);
    const reading = ledger.readSnapshots();
    ledger.event({ event: "started", job: 1, kind: "calibrate" });
    clock(2000); ledger.event({ event: "progress", job: 1, stage: "new", done: 5, total: 10 });
    clock(3000); ledger.event(finished(2));
    release([snapshot(1), snapshot(2)]); await reading;
    expect(state().jobs.find((job) => job.job === 1)?.progress).toEqual({ stage: "new", done: 5, total: 10 });
    expect(state().jobs.find((job) => job.job === 2)).toMatchObject({ status: "succeeded", finishedAt: 3000, completionHandled: true });
    expect(complete).not.toHaveBeenCalled();
    ledger.event(finished(2)); expect(complete).not.toHaveBeenCalled();
  });

  it("does not turn a finished job back to running when cancel is refused late", async () => {
    let reject!: (error: unknown) => void;
    const delayed = new Promise<never>((_resolve, fail) => { reject = fail; });
    const { ledger, state } = setup((async () => delayed) as RequestFn);
    ledger.event({ event: "started", job: 1, kind: "calibrate" });
    const cancelling = ledger.cancel(1);
    ledger.event({ ...finished(1), status: "failed", error: { code: "engine_input", params: {}, detail: "bad input", retryable: false } });
    reject(new Error("offline")); await cancelling;
    expect(state().jobs[0]?.status).toBe("failed");
    expect(errorCount(state())).toBe(2);
  });
  it("handles a finish once when the startup snapshot already reported its terminal status", async () => {
    const { ledger, state, complete } = setup((async () => [{ ...snapshot(1), status: "succeeded", cancellable: false }]) as RequestFn);
    await ledger.readSnapshots();
    expect(complete).not.toHaveBeenCalled();
    ledger.event(finished(1)); ledger.event(finished(1));
    expect(complete).toHaveBeenCalledOnce();
    expect(state().jobs[0]?.completionHandled).toBe(true);
  });
  it.each(["cancelled", "missing", "running"] as const)("reconciles a monitor after a lost finish (%s)", async (status) => {
    const snapshots: JobSnapshot[] = status === "missing" ? [] : [{ ...snapshot(1), kind: "level_monitor", status, cancellable: status === "running" }];
    const raw = vi.fn(async () => snapshots);
    const { ledger, state, complete } = setup(raw as RequestFn);
    ledger.event({ event: "started", job: 1, kind: "level_monitor" });
    await ledger.readSnapshots({ reconcile: 1 });
    expect(raw).toHaveBeenCalledExactlyOnceWith({ method: "jobs" });
    expect(state().jobs.find((job) => job.job === 1)?.status).toBe(status === "missing" ? undefined : status);
    expect(complete).toHaveBeenCalledTimes(status === "cancelled" ? 1 : 0);
    expect(state().history).toEqual([]);
  });
  it("consumes an early completion once even after its terminal metadata is retired", () => {
    const { ledger, complete, run, state, clock } = setup();
    const pending = ledger.begin({ ...request, params: { settings: null, subs_only: true } })!;
    ledger.event({ event: "started", job: 1, kind: "calibrate" });
    clock(2500); ledger.event(finished(1));
    expect(complete).not.toHaveBeenCalled();
    for (let id = 2; id <= 102; id++) run(id);
    expect(state().jobs.some((job) => job.job === 1)).toBe(false);
    ledger.reply(pending, { job: 1, kind: "calibrate" }); ledger.settle(pending);
    expect(complete).toHaveBeenCalledTimes(102);
    expect(complete.mock.calls.at(-1)?.[2]).toMatchObject({ method: "calibrate", params: { subs_only: true } });
    expect(complete.mock.calls.at(-1)?.[0]).toMatchObject({ finishedAt: 2500, completionOrder: expect.any(Number) });
    expect(state().jobs.some((job) => job.job === 1)).toBe(false);
    expect(state().jobs.every((job) => job.request === null)).toBe(true);
  });

  it("transfers a provisional early finish when the wrong same-kind owner is rejected", () => {
    const { ledger, complete, state } = setup();
    const rejected = ledger.begin(request)!;
    ledger.replaceProject();
    const accepted = ledger.begin({ ...request, params: { settings: null, subs_only: true } })!;
    ledger.event({ event: "started", job: 1, kind: "calibrate" }); ledger.event(finished(1));
    ledger.settle(rejected);
    ledger.reply(accepted, { job: 1, kind: "calibrate" }); ledger.settle(accepted);
    expect(complete).toHaveBeenCalledOnce();
    expect(complete.mock.calls[0]?.[2]).toMatchObject({ epoch: 1, params: { subs_only: true } });
    expect(state().jobs.every((job) => job.request === null)).toBe(true);
  });

  it("does not replay duplicate finishes or resurrect retired jobs from snapshots", async () => {
    const { ledger, run, state, complete } = setup((async () => [snapshot(1)]) as RequestFn);
    run(1); ledger.event(finished(1)); ledger.event({ event: "started", job: 1, kind: "calibrate" });
    ledger.event({ event: "progress", job: 1, stage: "late", done: 0, total: 1 });
    await ledger.readSnapshots();
    expect(complete).toHaveBeenCalledOnce();
    expect(state().jobs[0]?.status).toBe("succeeded");
    for (let id = 2; id <= 40; id++) run(id);
    await ledger.readSnapshots(); ledger.event(finished(1));
    expect(state().jobs).toHaveLength(32);
    expect(state().jobs.some((job) => job.job === 1)).toBe(false);
    expect(complete).toHaveBeenCalledTimes(40);
  });

  it("keeps recent completions when the delayed initial snapshot contains 32 older terminals", async () => {
    let release!: (jobs: JobSnapshot[]) => void;
    const pending = new Promise<JobSnapshot[]>((resolve) => { release = resolve; });
    const { ledger, run, state } = setup((async () => pending) as RequestFn);
    const reading = ledger.readSnapshots();
    for (let id = 100; id < 200; id++) run(id);
    release([...Array.from({ length: 32 }, (_, i) => ({ ...snapshot(i + 1), status: "succeeded" as const, cancellable: false })), snapshot(50)]);
    await reading;
    expect(state().jobs.filter((job) => job.status === "succeeded").map((job) => job.job)).toEqual(Array.from({ length: 32 }, (_, i) => i + 168));
    expect(state().jobs.find((job) => job.job === 50)?.status).toBe("running");
  });

  it("preserves a first Started whose allocated id arrives after newer jobs have retired", () => {
    const { ledger, run, state, complete, started } = setup();
    ledger.event({ event: "started", job: 1, kind: "measure" });
    for (let id = 100; id < 140; id++) run(id);
    ledger.event({ event: "started", job: 50, kind: "export" });
    ledger.event({ event: "started", job: 50, kind: "export" });
    expect(activeJobs(state()).map((job) => job.job)).toEqual([1, 50]);
    expect(state().jobs).toHaveLength(34);
    expect(started.mock.calls.filter(([job]) => job.job === 50)).toHaveLength(1);
    ledger.event(finished(50)); ledger.event(finished(50));
    expect(complete.mock.calls.filter(([job]) => job.job === 50)).toHaveLength(1);
    expect(activeJobs(state()).map((job) => job.job)).toEqual([1]);
  });

  it("accepts an owned reply before delayed Started despite its id lying inside the snapshot fence", () => {
    const { ledger, run, state, started, complete } = setup();
    ledger.event({ event: "started", job: 1, kind: "measure" });
    const pending = ledger.begin({ method: "export", params: { kind: "camilla_dsp", folder: "C:/old" } })!;
    ledger.replaceProject();
    for (let id = 100; id < 140; id++) run(id);
    ledger.reply(pending, { job: 50, kind: "export" }); ledger.reply(pending, { job: 50, kind: "export" }); ledger.settle(pending);
    expect(state().jobs.find((job) => job.job === 50)).toMatchObject({ status: "running", kind: "export", epoch: 0,
      request: { method: "export", epoch: 0, params: { kind: "camilla_dsp", folder: "C:/old" } } });
    ledger.event({ event: "started", job: 50, kind: "export" });
    expect(started.mock.calls.filter(([job]) => job.job === 50)).toHaveLength(1);
    const outcome: FinishedJob = { event: "finished", job: 50, status: "succeeded", result: { kind: "export", data: { files: ["old.yml"], folder: "C:/old", stale: false } }, error: null };
    ledger.event(outcome); ledger.event(outcome);
    const completions = complete.mock.calls.filter(([job]) => job.job === 50);
    expect(completions).toHaveLength(1);
    expect(completions[0]?.[2]).toMatchObject({ method: "export", epoch: 0 });
    expect(state().jobs.find((job) => job.job === 50)?.request).toBeNull();
    expect(activeJobs(state()).map((job) => job.job)).toEqual([1]);
  });

  it("does not keep external completions waiting for unrelated future requests", () => {
    const { ledger, complete, state } = setup();
    for (let id = 1; id <= 100; id++) { ledger.event({ event: "started", job: id, kind: "calibrate" }); ledger.event(finished(id)); }
    const next = ledger.begin(request)!;
    ledger.event({ event: "started", job: 101, kind: "calibrate" }); ledger.reply(next, { job: 101, kind: "calibrate" }); ledger.settle(next);
    expect(complete).toHaveBeenCalledTimes(100);
    ledger.event(finished(101));
    expect(complete).toHaveBeenCalledTimes(101);
    expect(state().jobs).toHaveLength(32);
    expect(state().jobs.every((job) => job.request === null)).toBe(true);
  });

  it("ignores a delayed cancellation snapshot after its completed job was retired", async () => {
    let release!: (job: JobSnapshot) => void;
    const pending = new Promise<JobSnapshot>((resolve) => { release = resolve; });
    const { ledger, run, state } = setup((async () => pending) as RequestFn);
    ledger.event({ event: "started", job: 1, kind: "calibrate" });
    const cancelling = ledger.cancel(1);
    ledger.event({ ...finished(1), status: "cancelled", result: null });
    for (let job = 2; job <= 40; job++) run(job);
    release({ ...snapshot(1), status: "cancel_requested" }); await cancelling;
    expect(activeJobs(state())).toEqual([]);
    expect(state().jobs).toHaveLength(32);
    expect(state().jobs.some((job) => job.job === 1)).toBe(false);
  });

  it("does not revive an unknown finish or replace recent terminal history with a delayed bootstrap snapshot", async () => {
    let release!: (jobs: JobSnapshot[]) => void;
    const delayed = new Promise<JobSnapshot[]>((resolve) => { release = resolve; });
    const { ledger, state } = setup((async () => delayed) as RequestFn);
    const reading = ledger.readSnapshots();
    ledger.event(finished(2));
    for (let job = 100; job < 200; job++) { ledger.event({ event: "started", job, kind: "calibrate" }); ledger.event(finished(job)); }
    release([snapshot(2), snapshot(3), ...Array.from({ length: 32 }, (_, i) => ({ ...snapshot(i + 4), status: "succeeded" as const, cancellable: false }))]);
    await reading;
    expect(state().jobs.some((job) => job.job === 2)).toBe(false);
    expect(activeJobs(state()).map((job) => job.job)).toEqual([3]);
    expect(state().jobs.filter((job) => job.status === "succeeded").map((job) => job.job)).toEqual(Array.from({ length: 32 }, (_, i) => i + 168));
    expect(state().history).toHaveLength(100);
  });
  it("reports a failed startup read in history and rejects it for the session to report", async () => {
    const failure = new Error("offline");
    const { ledger, state } = setup((async () => { throw failure; }) as RequestFn);
    await expect(ledger.readSnapshots()).rejects.toBe(failure);
    expect(state().history).toEqual([expect.objectContaining({ type: "request_failed", job: null, error: expect.objectContaining({ code: "internal", detail: "Error: offline" }) })]);
  });
  it("applies a terminal cancellation reply and handles the later finish once", async () => {
    const { ledger, state, complete } = setup((async () => ({ ...snapshot(1), status: "cancelled", cancellable: false })) as RequestFn);
    ledger.event({ event: "started", job: 1, kind: "calibrate" });
    await ledger.cancel(1);
    expect(state().jobs[0]).toMatchObject({ status: "cancelled", request: null, cancellable: false });
    expect(state().history).toEqual([expect.objectContaining({ type: "finished", status: "cancelled" })]);
    expect(complete).not.toHaveBeenCalled();
    ledger.event({ ...finished(1), status: "cancelled", result: null });
    ledger.event({ ...finished(1), status: "cancelled", result: null });
    expect(complete).toHaveBeenCalledOnce();
    expect(state().history.filter((entry) => entry.type === "finished")).toHaveLength(1);
  });
  it("publishes cancel_requested immediately and offers cancel again after refusal", async () => {
    let refuse!: (error: unknown) => void;
    const pending = new Promise<never>((_resolve, reject) => { refuse = reject; });
    const raw = vi.fn((async () => pending) as RequestFn);
    const { ledger, state } = setup(raw as RequestFn);
    ledger.event({ event: "started", job: 1, kind: "calibrate" });
    const cancelling = ledger.cancel(1);
    expect(state().jobs[0]?.status).toBe("cancel_requested");
    expect(raw).toHaveBeenCalledExactlyOnceWith({ method: "job_cancel", params: { job: 1 } });
    const error = { code: "internal" as const, params: {}, retryable: false, detail: "lost" };
    refuse(new IpcFailure(error));
    await cancelling;
    expect(state().jobs[0]?.status).toBe("running");
    expect(state().history).toEqual([expect.objectContaining({ type: "request_failed", job: 1, kind: "calibrate", error })]);
    expect(errorCount(state())).toBe(1);
  });
  it("keeps 32 terminal jobs, all active jobs and the newest 200 history entries", () => {
    const { ledger, state, run } = setup();
    ledger.event({ event: "started", job: 1, kind: "measure" });
    for (let id = 2; id <= 151; id++) {
      ledger.event({ event: "started", job: id, kind: "calibrate" });
      ledger.event({ event: "log", job: id, code: "note", params: {} });
      ledger.event(finished(id));
    }
    expect(state().jobs).toHaveLength(33);
    expect(activeJobs(state()).map((job) => job.job)).toEqual([1]);
    expect(state().jobs.filter((job) => job.status === "succeeded").map((job) => job.job)).toEqual(Array.from({ length: 32 }, (_, i) => i + 120));
    expect(state().jobs.every((job) => job.request === null && !("result" in job))).toBe(true);
    expect(state().history).toHaveLength(200);
    expect(state().history[0]).toMatchObject({ type: "finished", job: 151 });
    expect(state().history.at(-1)).toMatchObject({ type: "log", job: 52 });
    const before = state();
    ledger.event(finished(151));
    expect(state()).toBe(before);
    ledger.clearHistory();
    expect(state().history).toEqual([]);
    expect(state().jobs).toHaveLength(33);
    run(152);
    expect(state().history[0]?.id).toBe("h301");
    expect(errorCount(state())).toBe(0);
  });
  it("retains hidden kinds in the ledger but excludes them and their history from the tray", () => {
    const { ledger, state } = setup();
    ledger.event({ event: "started", job: 1, kind: "level_monitor" });
    ledger.event({ event: "started", job: 2, kind: "status_watch" });
    ledger.event({ event: "started", job: 3, kind: "measure" });
    ledger.event({ event: "meter", job: 3, rms_dbfs: -22, peak_dbfs: -10, spl_db: null, clipping: false });
    ledger.event({ event: "finished", job: 1, status: "cancelled", result: null, error: null });
    ledger.event({ event: "log", job: 2, code: "background", params: {} });
    expect(state().jobs.map((job) => job.job).sort()).toEqual([1, 2, 3]);
    expect(trayJobs(state()).jobs.map((job) => job.job)).toEqual([3]);
    expect(trayJobs(state()).history).toEqual([]);
  });
  it("estimates time left from the current stage's rate and freezes elapsed time on finish", () => {
    const { ledger, state, clock } = setup();
    ledger.event({ event: "started", job: 1, kind: "calibrate" });
    clock(2000); ledger.event({ event: "progress", job: 1, stage: "engine.design", done: 1, total: 11 });
    clock(14000); ledger.event({ event: "progress", job: 1, stage: "engine.design", done: 5, total: 11 });
    expect(elapsedMs(state().jobs[0]!, 16000)).toBe(16000);
    expect(remainingMs(state().jobs[0]!, 16000)).toBe(21000);
    clock(17000); ledger.event(finished(1));
    expect(elapsedMs(state().jobs[0]!, 30000)).toBe(17000);
    expect(activeJobs(state())).toEqual([]);
  });
  it("preserves unknown older active jobs and an unseen finish during a delayed initial snapshot", async () => {
    let release!: (jobs: JobSnapshot[]) => void;
    const delayed = new Promise<JobSnapshot[]>((resolve) => { release = resolve; });
    const { ledger, run, state, complete } = setup((async () => delayed) as RequestFn);
    ledger.event({ event: "started", job: 1, kind: "level_monitor" });
    const reading = ledger.readSnapshots();
    ledger.event(finished(40));
    for (let id = 100; id <= 220; id++) run(id);
    release([snapshot(40), snapshot(50), snapshot(100)]); await reading;
    expect(state().jobs.some((job) => job.job === 40)).toBe(false);
    await ledger.readSnapshots();
    expect(state().jobs.some((job) => job.job === 40)).toBe(false);
    expect(state().jobs.find((job) => job.job === 50)?.status).toBe("running");
    expect(state().jobs.find((job) => job.job === 1)?.status).toBe("running");
    expect(state().jobs.some((job) => job.job === 100)).toBe(false);
    expect(state().jobs).toHaveLength(34);
    expect(complete.mock.calls.filter(([job]) => job.job === 40)).toHaveLength(0);
  });
  it("resolves an early completion to the reply's project without replaying it", () => {
    const { ledger, state, complete } = setup();
    const first = ledger.begin(request)!;
    ledger.replaceProject();
    const second = ledger.begin(request)!;
    ledger.event({ event: "started", job: 10, kind: "calibrate" }); ledger.event(finished(10));
    ledger.event({ event: "started", job: 11, kind: "calibrate" }); ledger.event(finished(11));
    ledger.reply(second, { job: 10, kind: "calibrate" }); ledger.settle(second);
    ledger.reply(first, { job: 11, kind: "calibrate" }); ledger.settle(first);
    expect(complete.mock.calls.map(([job, , context]) => [job.job, context.epoch])).toEqual([[10, 1], [11, 0]]);
    expect(state().jobs.map((job) => [job.job, job.epoch, job.request])).toEqual([[10, 1, null], [11, 0, null]]);
    ledger.event(finished(10)); ledger.event(finished(11));
    expect(complete).toHaveBeenCalledTimes(2);
  });
  it.each([false, true])("announces an accepted local start once with resolved ownership (early=%s)", (early) => {
    const { ledger, started, complete, state } = setup();
    const pending = ledger.begin(request)!;
    ledger.event({ event: "started", job: 1, kind: "calibrate" });
    expect(started).not.toHaveBeenCalled();
    if (early) ledger.event(finished(1));
    ledger.reply(pending, { job: 1, kind: "calibrate" });
    ledger.reply(pending, { job: 1, kind: "calibrate" });
    ledger.settle(pending);
    expect(started).toHaveBeenCalledOnce();
    if (!early) ledger.event(finished(1));
    expect(complete).toHaveBeenCalledOnce();
    expect(complete.mock.calls[0]?.[2]).toMatchObject({ method: "calibrate", epoch: 0 });
    expect(state().jobs[0]).toMatchObject({ job: 1, status: "succeeded", request: null });
    expect(state().jobs[0]).not.toHaveProperty("result");
  });
});
