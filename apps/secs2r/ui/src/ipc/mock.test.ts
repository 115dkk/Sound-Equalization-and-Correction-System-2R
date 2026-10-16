import { describe, expect, it } from "vitest";
import { IpcFailure } from "./client";
import type { JobEvent } from "./bindings/JobEvent";
import { createMockService } from "./mock";
import { resultsSummary } from "./fixtures";

describe("mock service", () => {
  it("includes both roles for each conflicting output like config validation", async () => {
    const mock = createMockService({ tickMs: 0 });
    for (const index of [1, 2]) await mock.request({ method: "config_set_speaker", params: { index, role: null, size: null, band_index: 0, out: 1, pol: null } });
    const notices = await mock.request({ method: "config_validate" });
    expect(notices.filter((notice) => notice.code === "out_duplicate")).toEqual([
      { code: "out_duplicate", params: { out: "1", first_role: "L", role: "R" } },
      { code: "out_duplicate", params: { out: "1", first_role: "L", role: "C" } },
    ]);
    mock.dispose();
  });

  it("stores measured input levels rather than output or recommended levels", async () => {
    const mock = createMockService({ tickMs: 0 });
    const events: JobEvent[] = [];
    mock.events.subscribe((event) => events.push(event));
    await mock.request({ method: "level_monitor_start", params: { role: null } });
    await mock.request({ method: "level_play", params: { role: "L", master_dbfs: -20 } });
    for (let i = 0; i < 5; i++) mock.tick();
    const meter = events.find((event) => event.event === "meter");
    expect(meter).toMatchObject({ rms_dbfs: -30 });
    expect((await mock.request({ method: "levels_get" })).levels.L).toBe(-30);
    await mock.request({ method: "level_monitor_stop" });
    await mock.request({ method: "level_check", params: { role: "R" } });
    mock.drain();
    const finished = [...events].reverse().find((event) => event.event === "finished" && event.result?.kind === "level");
    if (finished?.event !== "finished" || finished.result?.kind !== "level") throw new Error("Missing level result");
    expect((await mock.request({ method: "levels_get" })).levels.R).toBe(finished.result.data.rms_dbfs);
    mock.dispose();
  });

  it("gives every BA sub a distinct deterministic curve before and after correction", async () => {
    const mock = createMockService({ tickMs: 0, results: resultsSummary });
    await mock.request({ method: "config_set_sub_count", params: { count: 3, confirm: true } });
    await mock.request({ method: "sample_load", params: { positions: 1 } });
    mock.drain();
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain();
    const request = { method: "result_curves", params: { tab: "ba", channel: null, group: null, xo_hz: null, corrected: null } } as const;
    const first = await mock.request(request);
    expect(await mock.request(request)).toEqual(first);
    for (const kind of ["before", "after"]) {
      const curves = first.curves.filter((curve) => curve.kind === kind && curve.role !== null);
      expect(curves.map((curve) => curve.role)).toEqual(["SW1", "SW2", "SW3"]);
      expect(new Set(curves.map((curve) => JSON.stringify(curve.values))).size).toBe(curves.length);
      expect(curves.every((curve) => curve.values.length === first.grid.length && curve.values.every(Number.isFinite))).toBe(true);
    }
    mock.dispose();
  });


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
