import { describe, expect, it } from "vitest";
import { IpcFailure } from "./client";
import type { JobEvent } from "./bindings/JobEvent";
import { createMockService } from "./mock";

describe("mock service", () => {
  it("answers through the client contract with typed fixtures", async () => {
    const mock = createMockService({ tickMs: 0 });
    const info = await mock.request({ method: "bootstrap" });
    expect(info.app_name).toBe("SECS 2R");
    const project = await mock.request({ method: "project_info" });
    expect(project.positions).toEqual([1, 2, 3]);
    expect(mock.calls.map((c) => c.method)).toEqual(["bootstrap", "project_info"]);
  });

  it("fails on request with an error envelope, never a rejection of the command", async () => {
    const mock = createMockService({ tickMs: 0 });
    mock.failNext("project_save", "project_io", "EACCES");
    const envelope = await mock.invoke("secs_call", { request: { method: "project_save", params: { path: null } } });
    expect(envelope).toMatchObject({ ok: false, error: { code: "project_io", detail: "EACCES" } });
    await expect(mock.request({ method: "project_save", params: { path: null } })).resolves.toMatchObject({ dirty: false });
    mock.failNext("results", "internal");
    await expect(mock.request({ method: "results" })).rejects.toBeInstanceOf(IpcFailure);
  });

  it("runs jobs as pushed events and stops them on cancel", async () => {
    const mock = createMockService({ tickMs: 0 });
    const events: JobEvent[] = [];
    mock.events.subscribe((event) => events.push(event));
    const { job } = await mock.request({ method: "export", params: { kind: "camilla_dsp", folder: "/tmp" } }).catch((e: unknown) => {
      expect((e as IpcFailure).error.code).toBe("engine_input");
      return { job: -1 };
    });
    expect(job).toBe(-1);
    const started = await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    expect(events[0]).toEqual({ event: "started", job: started.job, kind: "calibrate" });
    mock.tick();
    expect(events[1]).toMatchObject({ event: "progress", stage: "engine.design", done: 1 });
    await expect(mock.request({ method: "calibrate", params: { settings: null, subs_only: false } })).rejects.toMatchObject({ error: { code: "job_busy" } });
    const snapshot = await mock.request({ method: "job_cancel", params: { job: started.job } });
    expect(snapshot.status).toBe("cancel_requested");
    mock.tick();
    expect(events.at(-1)).toMatchObject({ event: "finished", status: "cancelled", error: { code: "cancelled" } });
    expect(await mock.request({ method: "jobs" })).toEqual([]);
  });

  it("finishes a job with its result", async () => {
    const mock = createMockService({ tickMs: 0 });
    const events: JobEvent[] = [];
    mock.events.subscribe((event) => events.push(event));
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain();
    const last = events.at(-1);
    expect(last).toMatchObject({ event: "finished", status: "succeeded", result: { kind: "calibration" } });
    const results = await mock.request({ method: "results" });
    expect(results.calibration?.stale).toBe(false);
  });
});
