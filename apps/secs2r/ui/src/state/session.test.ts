import { describe, expect, it, vi } from "vitest";
import { createMockService } from "../ipc/mock";
import { levelResult } from "../ipc/fixtures";
import { IpcFailure } from "../ipc/client";
import { createSession } from "./session";
import { createResource } from "./store";
import { PROJECT_REPLACED, TRIGGERS, triggeredBy } from "./triggers";

const settle = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };

it("reloads results and the new default export folder after Save As", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start();
    const previous = session.stores.results.getSnapshot();
    mock.calls.length = 0;
    await session.call({ method: "project_save", params: { path: "D:/New Room/living.secs" } });
    expect(mock.calls.map((call) => call.method)).toEqual(expect.arrayContaining(["results", "export_folder_get"]));
    expect(session.stores.results.getSnapshot()).not.toBe(previous);
    expect(session.stores.folder.getSnapshot()?.folder).toMatch(/^D:\/New Room\/SECS_export\/\d{4}-\d{2}-\d{2} living$/);
  } finally { session.dispose(); await settle(); mock.dispose(); }
});

describe("service invalidation table", () => {
  it("reloads every project resource when a project is replaced", () => {
    for (const mutation of PROJECT_REPLACED) expect(triggeredBy({ mutation }).sort()).toEqual(Object.keys(TRIGGERS).sort());
  });
  it.each(["measure", "sample_load", "mdat_import"] as const)("reloads committed data after %s", (finished) => {
    expect(triggeredBy({ finished })).toEqual(expect.arrayContaining(["project", "records", "curves", "results", "resultCurves", "lines"]));
  });
  it("covers settings, results, export and explicit mutations without timers", () => {
    expect(triggeredBy({ mutation: "calibration_settings_edit" })).toEqual(expect.arrayContaining(["settings", "modeSpec", "subWarnings", "project", "results"]));
    expect(triggeredBy({ finished: "calibrate" })).toEqual(expect.arrayContaining(["results", "resultCurves", "lines"]));
    expect(triggeredBy({ finished: "mimo" })).toContain("results");
    expect(triggeredBy({ mutation: "project_save" })).toContain("project");
    expect(triggeredBy({ mutation: "position_clear" })).toContain("records");
  });
  it("does not let an older read overwrite a mutation", async () => {
    let resolve!: (value: number) => void;
    const resource = createResource(() => new Promise<number>((done) => { resolve = done; }), vi.fn());
    const pending = resource.load(); resource.set(2); resolve(1); await pending;
    expect(resource.getSnapshot()).toBe(2);
  });
});

it("starts status watch, reloads job dependencies and clears selection after measurement", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await settle();
    expect(mock.calls.some((r) => r.method === "status_watch_start")).toBe(true);
    await session.configure({ method: "mock" });
    session.host.set({ selected: 4 });
    await session.call({ method: "measure_position", params: { position: 4 } });
    const start = mock.calls.length;
    mock.drain(); await settle();
    expect(session.host.getSnapshot().selected).toBeNull();
    expect(session.stores.records.getSnapshot()?.records.some((r) => r.position === 4 && r.done)).toBe(true);
    expect(mock.calls.slice(start).map((r) => r.method)).toEqual(expect.arrayContaining(["project_info", "measurement_records", "measurement_curves"]));
  } finally { session.dispose(); await settle(); mock.dispose(); }
});

it("records an export that finishes before its start reply arrives", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession({ ...mock, request: async (request) => {
    const result = await mock.request(request);
    if (request.method === "export") mock.drain();
    return result;
  } });
  try {
    await session.start(); await session.call({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain(); await settle();
    await session.call({ method: "export", params: { kind: "camilla_dsp", folder: "" } }); await settle();
    expect(session.host.getSnapshot().runs.camilla_dsp?.status).toBe("done");
    expect(mock.calls.some((r) => r.method === "open_folder")).toBe(true);
  } finally { session.dispose(); await settle(); mock.dispose(); }
});

it("shares a curve generation between selection and automatic refreshes", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start();
    const request = mock.request;
    const replies: (() => void)[] = [];
    vi.spyOn(mock, "request").mockImplementation(async (params) => {
      const value = await request(params);
      if (params.method === "result_curves") await new Promise<void>((resolve) => replies.push(resolve));
      return value;
    });
    const old = session.stores.resultCurves.load(); await settle();
    const selected = session.refreshCurves({ tab: "gd", channel: "R", group: null, xo_hz: null, corrected: null }); await settle();
    replies[1]!(); await selected;
    replies[0]!(); await old;
    expect(session.stores.resultCurves.getSnapshot()).toMatchObject({ params: { tab: "gd", channel: "R" }, pending: null });
  } finally { session.dispose(); await settle(); mock.dispose(); vi.restoreAllMocks(); }
});

describe("level monitor silence", () => {
  it("reconciles jobs after a lost monitor finish and releases the next queued action after five seconds", async () => {
    vi.useFakeTimers();
    const mock = createMockService({ tickMs: 0 });
    const session = createSession({ ...mock, request: (async (request) => {
      const value = await mock.request(request);
      if (request.method === "level_monitor_stop" && value) return { ...(value as import("../ipc/bindings/JobSnapshot").JobSnapshot), status: "cancel_requested", cancellable: true };
      return value;
    }) as typeof mock.request, events: { subscribe: (listener) => mock.events.subscribe((event) => {
      if (event.event !== "finished") listener(event);
    }) } });
    try {
      await session.start(); await session.configure({ method: "mock" }); await session.monitor(true);
      const id = session.host.getSnapshot().jobs.find((job) => job.kind === "level_monitor")!.job;
      mock.calls.length = 0;
      const stopped = session.monitor(false);
      const next = session.call({ method: "project_open", params: { path: "C:/next.secs" } });
      await settle();
      await vi.advanceTimersByTimeAsync(4999);
      expect(mock.calls.some((call) => call.method === "project_open")).toBe(false);
      await vi.advanceTimersByTimeAsync(1); await settle();
      expect(mock.calls.some((call) => call.method === "jobs")).toBe(true);
      expect(mock.calls.some((call) => call.method === "project_open")).toBe(true);
      await Promise.all([stopped, next]);
      expect(session.host.getSnapshot().jobs.some((job) => job.job === id && ["running", "cancel_requested"].includes(job.status))).toBe(false);
    } finally { session.dispose(); await settle(); mock.dispose(); vi.useRealTimers(); }
  });
  it("leaves a native or mock monitor alone while it is quiet between plays", async () => {
    vi.useFakeTimers();
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await settle(); await session.configure({ method: "mock" }); await session.monitor(true);
      await vi.advanceTimersByTimeAsync(5000);
      expect(session.host.getSnapshot().monitor).not.toBe("stalled");
      expect(mock.calls.filter((call) => call.method === "level_monitor_start")).toHaveLength(1);
      expect(mock.calls.filter((call) => call.method === "level_monitor_stop")).toHaveLength(0);
    } finally { session.dispose(); await settle(); mock.dispose(); vi.useRealTimers(); }
  });

  it("restarts after one second, accepts only its own readings and leaves no timer after stopping", async () => {
    vi.useFakeTimers();
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await settle(); await session.configure({ method: "rew" }); await session.monitor(true);
      const states: string[] = [];
      const off = session.host.subscribe(() => states.push(session.host.getSnapshot().monitor));
      const first = session.host.getSnapshot().jobs.find((job) => job.kind === "level_monitor")!.job;
      const reading = { event: "meter" as const, job: first, rms_dbfs: -40, peak_dbfs: -30, spl_db: null, clipping: false };
      mock.events.emit(reading);
      expect(session.host.getSnapshot().monitor).toBe("receiving");
      await vi.advanceTimersByTimeAsync(999);
      expect(mock.calls.filter((call) => call.method === "level_monitor_start")).toHaveLength(1);
      await vi.advanceTimersByTimeAsync(1);
      expect(states).toContain("stalled");
      expect(session.host.getSnapshot().monitor).toBe("stalled");
      expect(mock.calls.filter((call) => ["level_monitor_start", "level_monitor_stop"].includes(call.method)).map((call) => call.method)).toEqual(["level_monitor_start", "level_monitor_stop", "level_monitor_start"]);
      const second = session.host.getSnapshot().jobs.find((job) => job.kind === "level_monitor" && job.status === "running")!.job;
      mock.events.emit(reading);
      expect(session.host.getSnapshot().monitor).toBe("stalled");
      mock.events.emit({ ...reading, job: second });
      expect(session.host.getSnapshot().monitor).toBe("receiving");
      await session.monitor(false);
      expect(session.host.getSnapshot().monitor).toBe("off");
      expect(vi.getTimerCount()).toBe(0);
      mock.events.emit({ ...reading, job: second });
      await vi.advanceTimersByTimeAsync(5000);
      expect(session.host.getSnapshot().monitor).toBe("off");
      expect(mock.calls.filter((call) => call.method === "level_monitor_start")).toHaveLength(2);
      off();
    } finally { session.dispose(); await settle(); mock.dispose(); vi.useRealTimers(); }
  });

  it("does not start again if switched off while the restart is stopping", async () => {
    vi.useFakeTimers();
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await settle(); await session.configure({ method: "rew" }); await session.monitor(true);
      const original = mock.request;
      let release!: () => void;
      vi.spyOn(mock, "request").mockImplementation(async (request) => {
        const value = await original(request);
        if (request.method === "level_monitor_stop") await new Promise<void>((resolve) => { release = resolve; });
        return value;
      });
      await vi.advanceTimersByTimeAsync(1000);
      const stopped = session.monitor(false);
      release(); await stopped;
      expect(session.host.getSnapshot().monitor).toBe("off");
      expect(vi.getTimerCount()).toBe(0);
      expect(mock.calls.filter((call) => call.method === "level_monitor_start")).toHaveLength(1);
    } finally { session.dispose(); await settle(); mock.dispose(); vi.restoreAllMocks(); vi.useRealTimers(); }
  });

  it("cancels pending restarts when a project is replaced", async () => {
    vi.useFakeTimers();
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await settle(); await session.configure({ method: "rew" }); await session.monitor(true);
      await session.call({ method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
      expect(vi.getTimerCount()).toBe(0);
      await vi.advanceTimersByTimeAsync(5000);
      expect(session.host.getSnapshot()).toMatchObject({ monitor: "off", meter: null });
      expect(mock.calls.filter((call) => call.method === "level_monitor_start")).toHaveLength(1);
    } finally { session.dispose(); await settle(); mock.dispose(); vi.useRealTimers(); }
  });
});

describe("measurement settings", () => {
  it.each(["measure_config_get", "measure_configure", "level_apply"] as const)("ignores an old %s reply after opening another project but returns it to the caller", async (method) => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start();
      await session.configure({ method: "mock" });
      await session.call({ method: "project_save", params: { path: "C:/rooms/next.secs" } });
      await session.configure({ method: "rew" });
      const request = mock.request;
      let release!: () => void;
      let delay = true;
      vi.spyOn(mock, "request").mockImplementation(async (params) => {
        const value = await request(params);
        if (params.method === method && delay) {
          delay = false;
          await new Promise<void>((resolve) => { release = resolve; });
        }
        return value;
      });
      const previous = method === "measure_configure" ? session.configure({ method: "rew" })
        : method === "level_apply" ? session.call({ method, params: { level: levelResult } }) : session.call({ method });
      await settle();
      await session.call({ method: "project_open", params: { path: "C:/rooms/next.secs" } });
      expect(session.stores.measurementSettings.getSnapshot()?.method).toBe("mock");
      release();
      expect((await previous).method).toBe("rew");
      expect(session.stores.measurementSettings.getSnapshot()?.method).toBe("mock");
      expect(session.host.getSnapshot().path).toBe("mock");
    } finally { session.dispose(); await settle(); mock.dispose(); vi.restoreAllMocks(); }
  });
  it("throws an old request failure without reporting it in the new project", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start();
      const request = mock.request;
      const failure = new IpcFailure({ code: "device_unavailable", params: {}, retryable: true, detail: "old device" });
      let reject!: (error: unknown) => void;
      let delay = true;
      vi.spyOn(mock, "request").mockImplementation(async (params) => {
        if (params.method === "measure_config_get" && delay) {
          delay = false;
          await new Promise<void>((_resolve, fail) => { reject = fail; });
        }
        return request(params);
      });
      const previous = session.call({ method: "measure_config_get" });
      const rejected = expect(previous).rejects.toBe(failure);
      await session.call({ method: "project_open", params: { path: "C:/rooms/next.secs" } });
      reject(failure); await rejected;
      expect(session.errors.getSnapshot()).toBeNull();
    } finally { session.dispose(); await settle(); mock.dispose(); vi.restoreAllMocks(); }
  });

  it("reads a reopened project's stored method instead of asking again", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock); await session.start();
    try {
      await session.configure({ method: "mock" });
      await session.call({ method: "project_save", params: { path: "C:/rooms/kept.secs" } });
      await session.call({ method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
      expect(session.host.getSnapshot().path).toBeNull();
      mock.calls.length = 0;
      await session.call({ method: "project_open", params: { path: "C:/rooms/kept.secs" } });
      expect(mock.calls).toContainEqual({ method: "measure_config_get" });
      expect(mock.calls.some((call) => call.method === "measure_configure")).toBe(false);
      expect(session.host.getSnapshot().path).toBe("mock");
    } finally { session.dispose(); mock.dispose(); }
  });

  it("reads the stored method when the window starts over a project the service already holds", async () => {
    const mock = createMockService({ tickMs: 0 });
    await mock.request({ method: "measure_configure", params: {
      method: "rew", input_device: null, output_device: null, input_channel: null, output_channels: [], sweep_length: null, repetitions: null,
      sweep_level_dbfs: null, timing: null, loopback_input_channel: null, timing_ref_output: null, share_mode: null,
    } });
    const session = createSession(mock);
    try {
      await session.start();
      expect(session.stores.measurementSettings.getSnapshot()?.method).toBe("rew");
      expect(session.host.getSnapshot().path).toBe("rew");
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });
});

describe("XO recommendation completion", () => {
  it.each(["failed", "cancelled"] as const)("does not apply recommendations or recalibrate after a %s recommendation", async (status) => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start();
      const before = await mock.request({ method: "calibration_settings_get" });
      if (status === "failed") mock.failJobs("recommend_xo", "engine_input");
      const started = await session.call({ method: "recommend_xo" });
      if (status === "cancelled") await session.call({ method: "job_cancel", params: { job: started.job } });
      mock.drain(); await settle();
      expect(session.stores.settings.getSnapshot()).toEqual(before);
      expect(mock.calls.some((call) => call.method === "calibrate")).toBe(false);
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });

  it("does not recalibrate a replacement project while the recommended settings reload is delayed", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start();
      const request = mock.request;
      let release!: () => void;
      let delay = true;
      vi.spyOn(mock, "request").mockImplementation(async (params) => {
        const value = await request(params);
        if (params.method === "calibration_settings_get" && delay) {
          delay = false;
          await new Promise<void>((resolve) => { release = resolve; });
        }
        return value;
      });
      await session.call({ method: "recommend_xo" });
      mock.drain(); await settle();
      await session.call({ method: "project_open", params: { path: "C:/rooms/next.secs" } });
      const settings = session.stores.settings.getSnapshot();
      release(); await settle();
      expect(session.stores.settings.getSnapshot()).toBe(settings);
      expect(mock.calls.some((call) => call.method === "calibrate")).toBe(false);
    } finally { session.dispose(); await settle(); mock.dispose(); vi.restoreAllMocks(); }
  });
  it("reloads recommended settings after completion and starts a full calibration", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start();
      const before = await mock.request({ method: "calibration_settings_get" });
      mock.calls.length = 0;
      await session.call({ method: "recommend_xo" });
      expect(await mock.request({ method: "calibration_settings_get" })).toEqual(before);
      expect(mock.calls.some((call) => call.method === "calibrate")).toBe(false);
      mock.drain(); await settle();
      expect(session.stores.settings.getSnapshot()?.xo_rec).toMatchObject({ "프론트": 80, "센터": 100 });
      expect(session.stores.settings.getSnapshot()?.xo).toMatchObject({ "프론트": 80, "센터": 100 });
      expect(mock.calls).toContainEqual({ method: "calibrate", params: { settings: null, subs_only: false } });
      expect(session.host.getSnapshot().jobs.some((job) => job.kind === "calibrate" && job.status === "running")).toBe(true);
      mock.drain(); await settle();
      expect(session.stores.results.getSnapshot()?.calibration?.stale).toBe(false);
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });
});

describe("host state after jobs and switches", () => {
  it("treats a cancelled calibration as no failure", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await settle();
      const started = await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
      await session.call({ method: "job_cancel", params: { job: started.job } });
      mock.drain(); await settle();
      expect(session.host.getSnapshot().jobs.find((job) => job.job === started.job)?.status).toBe("cancelled");
      expect(session.host.getSnapshot().failure).toBeNull();
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });

  it("keeps low-SNR warnings per run of a position and drops them with the position", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await settle();
      await session.configure({ method: "mock" });
      await session.call({ method: "measure_position", params: { position: 4 } }); mock.drain(); await settle();
      expect(session.host.getSnapshot().warnings).toEqual([{ position: 4, notices: [expect.objectContaining({ code: "measurement_low_snr" })] }]);
      await session.call({ method: "measure_position", params: { position: 4 } });
      expect(session.host.getSnapshot().warnings).toEqual([]);
      mock.drain(); await settle();
      expect(session.host.getSnapshot().warnings[0]?.notices).toHaveLength(1);
      await session.call({ method: "position_clear", params: { position: 4 } });
      expect(session.host.getSnapshot().warnings).toEqual([]);
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });

  it("forgets the recommended sweep level with the input and the project", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await settle();
      await session.configure({ method: "mock" });
      await session.call({ method: "level_check", params: { role: "L" } }); mock.drain(); await settle();
      expect(session.host.getSnapshot().sweepResult).not.toBeNull();
      await session.configure({ input_channel: 2 });
      expect(session.host.getSnapshot().sweepResult).toBeNull();
      await session.call({ method: "level_check", params: { role: "L" } }); mock.drain(); await settle();
      expect(session.host.getSnapshot().sweepResult).not.toBeNull();
      await session.call({ method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
      expect(session.host.getSnapshot().sweepResult).toBeNull();
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });

  it("takes REW's facts from a driver switch and forgets the level measured through the old driver", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await settle();
      await session.configure({ method: "mock" });
      await session.call({ method: "level_check", params: { role: "L" } }); mock.drain(); await settle();
      expect(session.host.getSnapshot().sweepResult).not.toBeNull();
      mock.calls.length = 0;
      await session.call({ method: "rew_driver_set", params: { driver: "ASIO" } });
      expect(session.stores.rew.getSnapshot()?.driver).toBe("ASIO");
      expect(session.host.getSnapshot().sweepResult).toBeNull();
      expect(mock.calls.map((call) => call.method)).toEqual(["rew_driver_set", "measure_methods"]);
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });

  it("asks mode_spec with the MIMO FIR choice (upstream 1.3.3)", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await settle();
      await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_fir", on: true } } });
      expect(mock.calls).toContainEqual({ method: "mode_spec", params: expect.objectContaining({ mimo_fir: true }) });
      expect(session.stores.modeSpec.getSnapshot()?.mimo_filter).toBe("fir");
      await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_fir", on: false } } });
      expect(session.stores.modeSpec.getSnapshot()?.mimo_filter).toBe("iir");
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });
});

it("counts each project opened or created, so a screen can forget what it hid for the last one", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start();
    expect(session.host.getSnapshot().opened).toBe(0);
    await session.call({ method: "project_open", params: { path: "C:/rooms/a.secs" } });
    expect(session.host.getSnapshot().opened).toBe(1);
    await session.call({ method: "project_save", params: { path: "C:/rooms/b.secs" } });
    expect(session.host.getSnapshot().opened).toBe(1);
    await session.call({ method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
    expect(session.host.getSnapshot().opened).toBe(2);
  } finally { session.dispose(); await settle(); mock.dispose(); }
});
