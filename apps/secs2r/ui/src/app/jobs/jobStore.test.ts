import { describe, expect, it } from "vitest";
import { activeJobs, elapsedMs, initialJobs, jobsReducer, remainingMs, type JobsAction } from "./jobStore";

const run = (actions: JobsAction[]) => actions.reduce(jobsReducer, initialJobs);

describe("job store", () => {
  it("estimates time left from the current stage's rate only", () => {
    const state = run([
      { type: "event", at: 0, event: { event: "started", job: 1, kind: "calibrate" } },
      { type: "event", at: 2000, event: { event: "progress", job: 1, stage: "engine.design", done: 1, total: 11 } },
      { type: "event", at: 14000, event: { event: "progress", job: 1, stage: "engine.design", done: 5, total: 11 } },
    ]);
    const job = state.jobs[0]!;
    expect(elapsedMs(job, 16000)).toBe(16000);
    // (16000 − 2000) / (5 − 1) per unit × 6 units left
    expect(remainingMs(job, 16000)).toBe(21000);
  });

  it("restarts the estimate when a new stage starts its own count", () => {
    const state = run([
      { type: "event", at: 0, event: { event: "started", job: 1, kind: "mimo" } },
      { type: "event", at: 1000, event: { event: "progress", job: 1, stage: "engine.mimo", done: 2, total: 2 } },
      { type: "event", at: 3000, event: { event: "progress", job: 1, stage: "engine.mimo_iir", done: 0, total: 11 } },
    ]);
    expect(remainingMs(state.jobs[0]!, 4000)).toBeNull();
  });

  it("keeps one count across measurement stages named by role", () => {
    const state = run([
      { type: "event", at: 0, event: { event: "started", job: 1, kind: "measure" } },
      { type: "event", at: 1000, event: { event: "progress", job: 1, stage: "L", done: 1, total: 4 } },
      { type: "event", at: 3000, event: { event: "progress", job: 1, stage: "R", done: 2, total: 4 } },
    ]);
    expect(remainingMs(state.jobs[0]!, 3000)).toBe(4000);
  });

  it("is indeterminate without a total and leaves the running list when finished", () => {
    let state = run([{ type: "event", at: 0, event: { event: "started", job: 2, kind: "sample_load" } }]);
    expect(state.jobs[0]!.total).toBe(0);
    expect(remainingMs(state.jobs[0]!, 5000)).toBeNull();
    state = jobsReducer(state, { type: "event", at: 5000, event: { event: "finished", job: 2, status: "succeeded", result: null, error: null } });
    expect(activeJobs(state)).toHaveLength(0);
    expect(state.history[0]).toMatchObject({ type: "finished", status: "succeeded", kind: "sample_load" });
  });

  it("adds jobs it learns about from snapshots and marks cancel requests", () => {
    let state = jobsReducer(initialJobs, {
      type: "snapshots",
      at: 0,
      jobs: [{ job: 9, kind: "export", status: "running", cancellable: true, progress: { stage: "design", done: 0, total: 1 }, error: null }],
    });
    expect(activeJobs(state).map((job) => job.id)).toEqual([9]);
    state = jobsReducer(state, { type: "cancel_requested", job: 9 });
    expect(state.jobs[0]!.status).toBe("cancel_requested");
  });

  it("ignores events for jobs it never saw start", () => {
    const state = jobsReducer(initialJobs, { type: "event", at: 0, event: { event: "progress", job: 5, stage: "x", done: 1, total: 2 } });
    expect(state).toBe(initialJobs);
  });

  it("keeps the level monitor, the status watch and meter readings out of the tray", () => {
    const state = run([
      { type: "event", at: 0, event: { event: "started", job: 1, kind: "level_monitor" } },
      { type: "event", at: 0, event: { event: "started", job: 2, kind: "status_watch" } },
      { type: "event", at: 0, event: { event: "started", job: 3, kind: "measure" } },
      { type: "event", at: 1, event: { event: "meter", job: 3, rms_dbfs: -22, peak_dbfs: -10, spl_db: null, clipping: false } },
      { type: "event", at: 2, event: { event: "finished", job: 1, status: "cancelled", result: null, error: null } },
      {
        type: "snapshots",
        at: 3,
        jobs: [{ job: 4, kind: "status_watch", status: "running", cancellable: true, progress: null, error: null }],
      },
    ]);
    expect(state.jobs.map((job) => job.id)).toEqual([3]);
    expect(state.history).toEqual([]);
  });

  it("retains 32 terminal jobs, every active job and at most 200 history entries", () => {
    let state = run([{ type: "event", at: 0, event: { event: "started", job: 1, kind: "measure" } }]);
    for (let id = 2; id <= 151; id++) {
      state = jobsReducer(state, { type: "event", at: id, event: { event: "started", job: id, kind: "calibrate" } });
      state = jobsReducer(state, { type: "event", at: id, event: { event: "log", job: id, code: "note", params: {} } });
      state = jobsReducer(state, { type: "event", at: id, event: { event: "finished", job: id, status: "succeeded", result: { kind: "calibration" }, error: null } });
    }
    expect(state.jobs).toHaveLength(33);
    expect(activeJobs(state).map((job) => job.id)).toEqual([1]);
    expect(state.jobs.filter((job) => job.status === "succeeded").map((job) => job.id)).toEqual(Array.from({ length: 32 }, (_, i) => i + 120));
    expect(state.history).toHaveLength(200);
    const before = state;
    state = jobsReducer(state, { type: "event", at: 200, event: { event: "finished", job: 151, status: "succeeded", result: { kind: "calibration" }, error: null } });
    expect(state).toBe(before);
    state = jobsReducer(state, { type: "snapshots", at: 200, jobs: [{ job: 2, kind: "calibrate", status: "running", cancellable: true, progress: null, error: null }] });
    expect(activeJobs(state).map((job) => job.id)).toEqual([1]);
    state = jobsReducer(state, { type: "clear_history" });
    expect(state.history).toEqual([]);
    expect(state.jobs).toHaveLength(33);
  });

  it("keeps a first late Started even when its id is between already retired ids", () => {
    let state = run([{ type: "event", at: 0, event: { event: "started", job: 1, kind: "measure" } }]);
    for (let job = 100; job < 140; job++) {
      state = jobsReducer(state, { type: "event", at: job, event: { event: "started", job, kind: "calibrate" } });
      state = jobsReducer(state, { type: "event", at: job, event: { event: "finished", job, status: "succeeded", result: { kind: "calibration" }, error: null } });
    }
    const started: JobsAction = { type: "event", at: 200, event: { event: "started", job: 50, kind: "export" } };
    state = jobsReducer(state, started);
    expect(activeJobs(state).map((job) => job.id)).toEqual([1, 50]);
    expect(state.jobs).toHaveLength(34);
    expect(jobsReducer(state, started)).toBe(state);
    const finished: JobsAction = { type: "event", at: 201, event: { event: "finished", job: 50, status: "succeeded", result: null, error: null } };
    state = jobsReducer(state, finished);
    expect(jobsReducer(state, finished)).toBe(state);
    expect(activeJobs(state).map((job) => job.id)).toEqual([1]);
    expect(state.history.filter((entry) => entry.job === 50 && entry.type === "finished")).toHaveLength(1);
  });

  it("offers cancel again when job_cancel is refused", () => {
    const state = run([
      { type: "event", at: 0, event: { event: "started", job: 1, kind: "calibrate" } },
      { type: "cancel_requested", job: 1 },
      { type: "request_failed", job: 1, error: { code: "internal", params: {}, retryable: false, detail: "lost" }, at: 1 },
    ]);
    expect(state.jobs[0]!.status).toBe("running");
    expect(state.history.map((entry) => entry.type)).toEqual(["request_failed"]);
  });
});
