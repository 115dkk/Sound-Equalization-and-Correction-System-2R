import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockService } from "../ipc/mock";
import { projectInfo, resultsSummary } from "../ipc/fixtures";
import type { Request } from "../ipc/bindings/Request";
import { createSession } from "./session";
import { startCalibration } from "./calibration";
import { errorCount } from "./jobLedger";

let mock: ReturnType<typeof createMockService>;
let session: ReturnType<typeof createSession>;
const starts = () => mock.calls.filter((request) => ["calibrate", "recommend_xo"].includes(request.method));
const edit = (db: number) => session.call({ method: "calibration_settings_edit", params: { edit: { kind: "boost_max", db } } });
async function advance(ms: number) { await vi.advanceTimersByTimeAsync(ms); await session.settled(); }
async function enable() { await session.writers.app.edit({ auto_refresh: true }); }
async function withoutXo() { await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "xo_rec_include", on: false } } }); }
beforeEach(async () => {
  vi.useFakeTimers();
  mock = createMockService({ tickMs: 0, results: resultsSummary });
  session = createSession(mock);
  await session.start(); await session.settled(); mock.calls.length = 0;
});
afterEach(async () => { session.dispose(); await session.settled(); mock.dispose(); vi.useRealTimers(); });

describe("automatic correction refresh", () => {
  it("debounces settings, target and measurement edits into one background flow and keeps MIMO", async () => {
    await enable();
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
    await edit(4); await advance(900);
    await session.call({ method: "target_apply_preset", params: { preset: "flat" } }); await advance(900);
    await session.call({ method: "position_clear", params: { position: 9 } });
    await advance(1499); expect(starts()).toEqual([]);
    await advance(1);
    expect(starts()).toEqual([{ method: "recommend_xo", params: { background: true } }]);
    mock.drain(); await session.settled();
    expect(starts()).toEqual([{ method: "recommend_xo", params: { background: true } }, { method: "calibrate", params: { settings: null, subs_only: false, mimo: true, background: true } }]);
    mock.drain(); await session.settled();
    await advance(5000); expect(starts()).toHaveLength(2);
    expect(session.stores.results.getSnapshot()?.calibration?.stale).toBe(false);
    expect(session.stores.results.getSnapshot()?.mimo?.stale).toBe(false);
  });

  it.each([true, false])("cancels only its own %s-XO run and restarts after the last edit", async (xo) => {
    if (!xo) await withoutXo();
    await enable(); await edit(4); await advance(1500);
    const own = session.jobs.getSnapshot().jobs.find((job) => job.request?.background)!;
    expect(own.request?.autoRefreshRevision).toBeDefined();
    await edit(5);
    expect(mock.calls).toContainEqual({ method: "job_cancel", params: { job: own.job } });
    expect(session.jobs.getSnapshot().jobs.find((job) => job.job === own.job)?.status).toBe("cancel_requested");
    mock.drain(); await session.settled();
    await advance(1499); expect(starts()).toHaveLength(1);
    await advance(1); expect(starts()).toHaveLength(2);
    expect(starts().at(-1)).toMatchObject({ method: xo ? "recommend_xo" : "calibrate", params: { background: true } });
  });

  it("does not cancel a user run, then refreshes its stale result after it finishes", async () => {
    await withoutXo(); await enable();
    await startCalibration(session);
    await edit(5); await advance(2000);
    expect(mock.calls.some((request) => request.method === "job_cancel")).toBe(false);
    expect(starts()).toHaveLength(1);
    mock.drain(); await session.settled(); await advance(0);
    expect(starts()).toHaveLength(2);
    expect(starts()[1]).toMatchObject({ method: "calibrate", params: { background: true } });
  });

  it("does nothing when disabled or when no correction result exists", async () => {
    await edit(4); await advance(5000); expect(starts()).toEqual([]);
    mock.setResults({ calibration: null, mimo: null, mimo_export: "none" });
    await session.stores.results.load(); await enable(); await edit(5);
    await advance(5000); expect(starts()).toEqual([]);
  });

  it("waits during measurement and restarts its debounce when measured data arrives", async () => {
    await enable(); await edit(4);
    await session.call({ method: "measure_position", params: { position: 4 } });
    await advance(2000); expect(starts()).toEqual([]);
    mock.drain(); await session.settled();
    await advance(1499); expect(starts()).toEqual([]);
    await advance(1); expect(starts()).toHaveLength(1);
  });

  it.each(["measure", "sample_load", "mdat_import", "level_monitor"] as const)("cancels its running recommendation when %s starts and resumes only after it ends", async (kind) => {
    await enable(); await edit(4); await advance(1500);
    const own = session.jobs.getSnapshot().jobs.find((job) => job.request?.background)!;
    // External measurement jobs use the same pushed lifecycle as local ones.
    mock.events.emit({ event: "started", job: 1000, kind });
    await advance(0);
    expect(mock.calls.filter((request) => request.method === "job_cancel")).toEqual([{ method: "job_cancel", params: { job: own.job } }]);
    mock.drain(); await session.settled();
    expect(session.jobs.getSnapshot().jobs.find((job) => job.job === own.job)?.status).toBe("cancelled");
    await advance(3000);
    expect(starts()).toHaveLength(1);
    mock.events.emit({ event: "finished", job: 1000, status: "succeeded", result: null, error: null });
    await session.settled(); await advance(1500);
    expect(starts()).toEqual([
      { method: "recommend_xo", params: { background: true } },
      { method: "recommend_xo", params: { background: true } },
    ]);
  });

  it("cancels an automatic calibration when a local measurement starts, not the measurement", async () => {
    await withoutXo(); await enable(); await edit(4); await advance(1500);
    const own = session.jobs.getSnapshot().jobs.find((job) => job.request?.background)!;
    const measurement = await session.call({ method: "measure_position", params: { position: 4 } });
    await session.settled();
    expect(mock.calls.filter((request) => request.method === "job_cancel")).toEqual([{ method: "job_cancel", params: { job: own.job } }]);
    expect(session.jobs.getSnapshot().jobs.find((job) => job.job === measurement.job)?.status).toBe("running");
    await advance(3000); expect(starts()).toHaveLength(1);
    mock.drain(); await session.settled(); await advance(1500);
    expect(starts()).toHaveLength(2);
    expect(starts()[1]).toMatchObject({ method: "calibrate", params: { background: true } });
  });

  it("does not cancel a user's calibration when measurement starts", async () => {
    await withoutXo(); await enable(); await startCalibration(session);
    await session.call({ method: "measure_position", params: { position: 4 } });
    await session.settled(); await advance(3000);
    expect(mock.calls.some((request) => request.method === "job_cancel")).toBe(false);
    expect(starts()).toHaveLength(1);
  });

  it("waits while the save prompt is open", async () => {
    await enable(); await edit(4); session.setSavePromptOpen(true);
    await advance(2000); expect(starts()).toEqual([]);
    session.setSavePromptOpen(false); await advance(0);
    expect(starts()).toHaveLength(1);
  });

  it("defers the recommendation follow-up while saving and resumes after the prompt closes", async () => {
    await enable(); await edit(4); await advance(1500);
    session.setSavePromptOpen(true);
    mock.drain(); await session.settled();
    await advance(2000); expect(starts()).toHaveLength(1);
    session.setSavePromptOpen(false); await advance(0);
    expect(starts()).toHaveLength(2);
    mock.drain(); await session.settled();
    expect(starts().at(-1)).toMatchObject({ method: "calibrate", params: { background: true } });
  });

  it("stops a queued refresh when the preference is switched off", async () => {
    await enable(); await edit(4); await advance(1000);
    await session.writers.app.edit({ auto_refresh: false });
    await advance(5000); expect(starts()).toEqual([]);
  });

  it("waits for a native save dialog without including future timers in settled", async () => {
    await enable(); await edit(4);
    let release!: () => void;
    const held = new Promise<void>((resolve) => { release = resolve; });
    const request = mock.request;
    vi.spyOn(mock, "request").mockImplementation(async (value) => {
      if (value.method === "dialog_save_project") await held;
      return request(value);
    });
    const saving = session.call({ method: "dialog_save_project", params: { suggested: "" } });
    try {
      await vi.advanceTimersByTimeAsync(2000); expect(starts()).toEqual([]);
    } finally { release(); await saving; }
    await advance(0); expect(starts()).toHaveLength(1);
  });

  it("retains progress and failure history, hides diagnostic settings and does not retry a failed revision", async () => {
    await withoutXo(); await enable(); mock.failJobs("calibrate", "engine_input", "failed");
    await edit(4); await advance(1500);
    const own = session.jobs.getSnapshot().jobs.find((job) => job.request?.background)!;
    mock.events.emit({ event: "log", job: own.job, code: "calibration_settings", params: { engine_key: "private" } });
    mock.tick();
    expect(session.jobs.getSnapshot().jobs.find((job) => job.job === own.job)?.progress).not.toBeNull();
    expect(session.jobs.getSnapshot().history.some((entry) => entry.type === "log" && entry.code === "calibration_settings")).toBe(false);
    mock.drain(); await session.settled(); await advance(10000);
    expect(starts()).toHaveLength(1);
    expect(session.host.getSnapshot().failure).toMatchObject({ job: "calibrate", error: { code: "engine_input" } });
    expect(session.jobs.getSnapshot().history).toContainEqual(expect.objectContaining({ job: own.job, type: "finished", status: "failed" }));
  });

  it("drops a completed recommendation's follow-up if an edit arrives before its reload completes", async () => {
    await enable(); await edit(4); await advance(1500);
    let release!: () => void;
    let entered!: () => void;
    const held = new Promise<void>((resolve) => { release = resolve; });
    const reached = new Promise<void>((resolve) => { entered = resolve; });
    const request = mock.request;
    let hold = true;
    vi.spyOn(mock, "request").mockImplementation(async (value) => {
      if (value.method === "calibration_settings_get" && hold) { hold = false; entered(); await held; }
      return request(value);
    });
    mock.drain(); await reached;
    try { await edit(5); } finally { release(); }
    await session.settled(); expect(starts()).toHaveLength(1);
    await advance(1500); expect(starts()).toHaveLength(2);
    expect(starts()[1]?.method).toBe("recommend_xo");
  });

  it("cancels a background start whose reply arrives after an edit", async () => {
    await enable(); await edit(4);
    let release!: () => void;
    const held = new Promise<void>((resolve) => { release = resolve; });
    const request = mock.request;
    vi.spyOn(mock, "request").mockImplementation(async (value) => {
      const result = await request(value);
      if (value.method === "recommend_xo") await held;
      return result;
    });
    await vi.advanceTimersByTimeAsync(1500);
    try { await edit(5); } finally { release(); }
    await session.settled();
    expect(mock.calls.filter((value) => value.method === "job_cancel")).toHaveLength(1);
  });

  it("clears timers on dispose and does not carry an old project's debounce to a new empty project", async () => {
    await enable(); await edit(4);
    const fresh: Request = { method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } };
    await session.call(fresh); await advance(2000); expect(starts()).toEqual([]);
    mock.setResults(resultsSummary); await edit(5);
    session.dispose(); await session.settled(); await advance(2000); expect(starts()).toEqual([]);
  });
});

describe("full correction routing", () => {
  it.each([false, true])("continues full correction after scoring fails, preserving XO, MIMO and the failed job (background=%s)", async (background) => {
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
    const xo = structuredClone(session.stores.settings.getSnapshot()!.xo);
    mock.failJobs("recommend_xo", "engine_input", "position 2 L: missing SW1 for crossover scoring");
    if (background) { await enable(); await edit(4); await advance(1500); }
    else await startCalibration(session);
    const recommendation = session.jobs.getSnapshot().jobs.find((job) => job.kind === "recommend_xo")!;
    expect(recommendation.request?.calibrateOnFailure).toBe(true);
    mock.drain(); await session.settled();
    expect(starts()).toEqual([
      background ? { method: "recommend_xo", params: { background: true } } : { method: "recommend_xo" },
      { method: "calibrate", params: { settings: null, subs_only: false, mimo: true, ...(background ? { background: true } : {}) } },
    ]);
    expect(session.stores.settings.getSnapshot()!.xo).toEqual(xo);
    mock.drain(); await session.settled();
    expect(session.stores.results.getSnapshot()?.calibration?.stale).toBe(false);
    expect(session.stores.results.getSnapshot()?.mimo?.stale).toBe(false);
    expect(session.jobs.getSnapshot().jobs.find((job) => job.job === recommendation.job)).toMatchObject({ kind: "recommend_xo", status: "failed", error: { code: "engine_input" } });
    expect(session.jobs.getSnapshot().history).toContainEqual(expect.objectContaining({ job: recommendation.job, kind: "recommend_xo", type: "finished", status: "failed", error: expect.objectContaining({ code: "engine_input" }) }));
    expect(errorCount(session.jobs.getSnapshot())).toBe(1);
    await advance(3000); expect(starts()).toHaveLength(2);
  });

  it("does not continue a failed recommendation from the dedicated XO button", async () => {
    mock.failJobs("recommend_xo", "engine_input", "position 2 L: missing SW1 for crossover scoring");
    await session.call({ method: "recommend_xo" });
    mock.drain(); await session.settled();
    expect(starts()).toEqual([{ method: "recommend_xo" }]);
    expect(session.host.getSnapshot().failure).toMatchObject({ job: "recommend_xo", error: { code: "engine_input" } });
    expect(errorCount(session.jobs.getSnapshot())).toBe(1);
  });

  it("does not continue a cancelled recommendation from full correction", async () => {
    const job = await startCalibration(session);
    await session.jobs.cancel(job.job);
    mock.drain(); await session.settled();
    expect(starts()).toEqual([{ method: "recommend_xo" }]);
    expect(session.jobs.getSnapshot().jobs.find((entry) => entry.job === job.job)?.status).toBe("cancelled");
  });

  it("clears the automatic-off notice on undo and leaves it clear when inclusion is then turned off manually", async () => {
    const undo = await session.call({ method: "position_clear", params: { position: 9 } });
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "xo", group: "프론트", hz: 95 } } });
    expect(session.host.getSnapshot().xoRecTurnedOffByEdit).toBe(true);
    await session.call({ method: "undo", params: { token: undo.token } });
    expect(session.host.getSnapshot().xoRecTurnedOffByEdit).toBe(false);
    expect(session.stores.settings.getSnapshot()?.xo_rec_include).not.toBe(false);
    await withoutXo();
    expect(session.stores.settings.getSnapshot()?.xo_rec_include).toBe(false);
    expect(session.host.getSnapshot().xoRecTurnedOffByEdit).toBe(false);
  });

  it("clears the automatic-off notice on an explicit off edit even when already off", async () => {
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "xo", group: "프론트", hz: 95 } } });
    expect(session.host.getSnapshot().xoRecTurnedOffByEdit).toBe(true);
    await withoutXo();
    expect(session.host.getSnapshot().xoRecTurnedOffByEdit).toBe(false);
  });

  it.each([
    { subs: 2, include: undefined, method: "recommend_xo" },
    { subs: 2, include: true, method: "recommend_xo" },
    { subs: 2, include: false, method: "calibrate" },
    { subs: 0, include: true, method: "calibrate" },
  ] as const)("routes subs=$subs include=$include to $method", async ({ subs, include, method }) => {
    session.dispose(); await session.settled(); mock.dispose();
    mock = createMockService({ tickMs: 0, project: { ...projectInfo, sub_count: subs } });
    session = createSession(mock); await session.start(); await session.settled();
    if (include !== undefined) await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "xo_rec_include", on: include } } });
    mock.calls.length = 0;
    await startCalibration(session);
    expect(starts()).toHaveLength(1);
    expect(starts()[0]?.method).toBe(method);
    expect(starts()[0]).not.toMatchObject({ params: { background: true } });
  });

  it("preserves the original MIMO choice through recommendation and resets the manual-edit notice", async () => {
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
    await startCalibration(session);
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: false } } });
    mock.drain(); await session.settled();
    expect(starts().at(-1)).toMatchObject({ method: "calibrate", params: { mimo: true } });
    mock.drain(); await session.settled();
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "xo", group: "프론트", hz: 95 } } });
    expect(session.host.getSnapshot().xoRecTurnedOffByEdit).toBe(true);
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "xo_rec_include", on: true } } });
    expect(session.host.getSnapshot().xoRecTurnedOffByEdit).toBe(false);
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "xo", group: "프론트", hz: 90 } } });
    await session.call({ method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
    expect(session.host.getSnapshot().xoRecTurnedOffByEdit).toBe(false);
  });
});
