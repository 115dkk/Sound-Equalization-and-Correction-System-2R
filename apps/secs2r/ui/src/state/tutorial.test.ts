import { expect, it, vi } from "vitest";
import { createMockService } from "../ipc/mock";
import { createSession } from "./session";

it.each([
  [undefined, "none", 16],
  ["system.output", "none", 16],
  ["system.dsp", "none", 16],
  ["connect.retry", "none", 6],
  ["connect.connection", "mock", 6],
  ["measure.sample", "mock", 6],
  ["calibrate.compute", "mock", 6],
  ["export.mdat", "mock", 6],
  ["export.camilla", "mock", 6],
] as const)("prepares %s with method %s and SW3 on output %s", async (quest, method, output) => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.configure({ method: "mock" }); await session.monitor(true); await session.settled();
    mock.calls.length = 0;
    await session.startTutorial(quest ? { quest } : {}); await session.settled();
    expect(mock.calls).toContainEqual({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } });
    const methods = mock.calls.map((request) => request.method);
    expect(methods.indexOf("level_monitor_stop")).toBeLessThan(methods.indexOf("project_new"));
    expect(mock.calls.filter((request) => request.method === "measure_configure")).toEqual([
      { method: "measure_configure", params: expect.objectContaining({ method }) },
    ]);
    expect(session.stores.project.getSnapshot()).toMatchObject({ tutorial: true, path: "", positions: [] });
    expect(session.stores.measurementSettings.getSnapshot()?.method).toBe(method);
    expect(session.stores.config.getSnapshot()?.speakers.find((speaker) => speaker.role === "SW3")?.bands[0]?.out).toBe(output);
    expect(session.host.getSnapshot()).toMatchObject({ monitor: "off", path: method === "none" ? null : "mock" });
    expect(session.tutorialReady.getSnapshot()).toBe(true);
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

it("prepares an existing gallery tutorial without erasing measurements or calibration, then restarts it", async () => {
  const mock = createMockService({ tickMs: 0 });
  await mock.request({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } });
  await mock.request({ method: "sample_load", params: { positions: null } }); mock.drain();
  await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
  const session = createSession(mock);
  try {
    await session.start(); await session.settled();
    const positions = session.stores.project.getSnapshot()!.positions;
    expect(positions.length).toBeGreaterThan(0);
    mock.calls.length = 0;
    await session.startTutorial({ quest: "export.mdat" }); await session.settled();
    expect(mock.calls.some((request) => request.method === "project_new")).toBe(false);
    expect(session.stores.project.getSnapshot()?.positions).toEqual(positions);
    expect(session.stores.results.getSnapshot()?.calibration).not.toBeNull();
    expect(session.tutorialReady.getSnapshot()).toBe(true);
    mock.calls.length = 0;
    await session.startTutorial(); await session.settled();
    expect(mock.calls.filter((request) => request.method === "project_new")).toHaveLength(1);
    expect(mock.calls.filter((request) => request.method === "measure_configure")).toHaveLength(1);
    expect(session.stores.project.getSnapshot()?.positions).toEqual([]);
    expect(session.stores.results.getSnapshot()?.calibration).toBeNull();
    expect(session.stores.measurementSettings.getSnapshot()?.method).toBe("none");
    expect(session.stores.config.getSnapshot()?.speakers.find((speaker) => speaker.role === "SW3")?.bands[0]?.out).toBe(16);
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

it("shares in-flight preparation and keeps both readiness and settled behind its last write", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  let release!: () => void;
  let entered!: () => void;
  const blocked = new Promise<void>((resolve) => { release = resolve; });
  const reached = new Promise<void>((resolve) => { entered = resolve; });
  try {
    await session.start(); await session.settled();
    const original = mock.request;
    vi.spyOn(mock, "request").mockImplementation(async (request) => {
      if (request.method === "config_set_speaker") { entered(); await blocked; }
      return original(request);
    });
    mock.calls.length = 0;
    const starting = session.startTutorial();
    expect(session.startTutorial()).toBe(starting);
    let done = false;
    const waiting = session.settled().then(() => { done = true; });
    await reached;
    expect(session.tutorialReady.getSnapshot()).toBe(false);
    expect(done).toBe(false);
    release(); await starting; await waiting;
    expect(session.tutorialReady.getSnapshot()).toBe(true);
    expect(mock.calls.filter((request) => request.method === "project_new")).toHaveLength(1);
    expect(mock.calls.filter((request) => request.method === "measure_configure")).toHaveLength(1);
  } finally { release(); vi.restoreAllMocks(); session.dispose(); await session.settled(); mock.dispose(); }
});

it("does not configure another project while the tutorial replacement's reload is pending", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  let release!: () => void;
  let entered!: () => void;
  const blocked = new Promise<void>((resolve) => { release = resolve; });
  const reached = new Promise<void>((resolve) => { entered = resolve; });
  try {
    await session.start(); await session.settled();
    const original = mock.request;
    let delayed = false;
    vi.spyOn(mock, "request").mockImplementation(async (request) => {
      const result = await original(request);
      if (request.method === "config_get" && !delayed) { delayed = true; entered(); await blocked; }
      return result;
    });
    mock.calls.length = 0;
    const starting = session.startTutorial({ quest: "measure.sample" });
    await reached;
    await session.call({ method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
    release(); await starting; await session.settled();
    expect(session.stores.project.getSnapshot()?.tutorial).toBe(false);
    expect(session.tutorialReady.getSnapshot()).toBe(false);
    expect(mock.calls.some((request) => request.method === "measure_configure")).toBe(false);
  } finally { release(); vi.restoreAllMocks(); session.dispose(); await session.settled(); mock.dispose(); }
});

it("does not declare a failed tutorial ready and allows its preparation to be retried", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.settled();
    mock.failNext("measure_configure", "invalid_request");
    await expect(session.startTutorial()).rejects.toThrow(); await session.settled();
    expect(session.tutorialReady.getSnapshot()).toBe(false);
    expect(session.errors.getSnapshot()?.error.code).toBe("invalid_request");
    await session.startTutorial(); await session.settled();
    expect(session.tutorialReady.getSnapshot()).toBe(true);
    expect(session.errors.getSnapshot()).toBeNull();
    await session.call({ method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
    expect(session.tutorialReady.getSnapshot()).toBe(false);
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});
