import { describe, expect, it, vi } from "vitest";
import { createMockService } from "../ipc/mock";
import { createSession } from "./session";
import { createResource } from "./store";
import { PROJECT_REPLACED, TRIGGERS, triggeredBy } from "./triggers";

const settle = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };

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

describe("measurement settings", () => {
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
});
