import { describe, expect, it } from "vitest";
import type { JobEvent } from "../ipc/bindings/JobEvent";
import type { Request } from "../ipc/bindings/Request";
import type { RequestFn } from "../ipc/context";
import { IpcFailure } from "../ipc/client";
import { createMockService, ipcError } from "../ipc/mock";
import { levelResult, resultsSummary, xoRecommendation } from "../ipc/fixtures";
import { createSession } from "./session";

const settle = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
const newProject: Request = { method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } };

describe("session completion ownership", () => {
  it.each([false, true])("reloads once and clears the previous failure for a new local run (early=%s)", async (early) => {
    const mock = createMockService({ tickMs: 0 });
    let finished: JobEvent | undefined;
    const off = mock.events.subscribe((event) => { if (event.event === "finished" && event.result?.kind === "calibration") finished = event; });
    const session = createSession({ ...mock, request: async (request) => {
      const value = await mock.request(request);
      if (early && request.method === "calibrate") mock.drain();
      return value;
    } });
    try {
      await session.start(); await settle();
      session.host.set({ failure: { job: "calibrate", error: ipcError("engine_input", "previous") } });
      mock.calls.length = 0;
      const started = await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
      expect(session.host.getSnapshot().failure).toBeNull();
      if (!early) mock.drain();
      await settle();
      expect(mock.calls.filter((request) => request.method === "results")).toHaveLength(1);
      expect(session.jobs.getSnapshot().jobs.find((job) => job.job === started.job)?.request).toBeNull();
      expect(session.jobs.getSnapshot().jobs.find((job) => job.job === started.job)).not.toHaveProperty("result");
      mock.events.emit(finished!); await settle();
      expect(mock.calls.filter((request) => request.method === "results")).toHaveLength(1);
    } finally { off(); session.dispose(); await settle(); mock.dispose(); }
  });

  it("consumes the accepted same-kind completion after the provisional owner receives JobBusy", async () => {
    const mock = createMockService({ tickMs: 0 });
    let calls = 0;
    let rejectFirst!: () => void;
    let resolveSecond!: () => void;
    const first = new Promise<void>((resolve) => { rejectFirst = resolve; });
    const second = new Promise<void>((resolve) => { resolveSecond = resolve; });
    const session = createSession({ ...mock, request: (async (request: Request) => {
      if (request.method === "calibrate") {
        calls++;
        if (calls === 1) { await first; throw new IpcFailure(ipcError("job_busy")); }
        const started = await mock.request(request); mock.drain(); await second; return started;
      }
      return mock.request(request);
    }) as RequestFn });
    try {
      await session.start(); await settle(); mock.calls.length = 0;
      const rejected = session.call({ method: "calibrate", params: { settings: null, subs_only: false } }).catch(() => {});
      const accepted = session.call({ method: "calibrate", params: { settings: null, subs_only: true } });
      await settle(); rejectFirst(); await rejected;
      resolveSecond(); await accepted; await settle();
      expect(mock.calls.filter((request) => request.method === "results")).toHaveLength(1);
      expect(session.stores.results.getSnapshot()?.calibration).toBeNull();
      expect(session.jobs.getSnapshot().jobs.every((job) => job.request === null)).toBe(true);
    } finally { rejectFirst(); resolveSecond(); session.dispose(); await settle(); mock.dispose(); }
  });

  it.each(["calibrate", "export"] as const)("keeps global metadata but ignores a previous project's late %s reply and finished payload", async (kind) => {
    const mock = createMockService({ tickMs: 0, results: resultsSummary });
    let release!: () => void;
    const delayed = new Promise<void>((resolve) => { release = resolve; });
    const session = createSession({ ...mock, request: async (request) => {
      const value = await mock.request(request);
      if (request.method === kind) { mock.drain(); await delayed; }
      return value;
    } });
    try {
      await session.start(); await settle();
      const old = kind === "export" ? session.call({ method: "export", params: { kind: "camilla_dsp", folder: "C:/old" } })
        : session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
      await settle();
      await session.call(newProject); await settle();
      session.host.set({ failure: { job: "calibrate", error: ipcError("engine_input", "new project") } });
      mock.calls.length = 0;
      release(); await old; await settle();
      expect(session.host.getSnapshot().runs).toEqual({});
      expect(session.host.getSnapshot().failure?.error.detail).toBe("new project");
      expect(session.jobs.getSnapshot().jobs.some((job) => job.kind === kind && job.status === "succeeded")).toBe(true);
      expect(session.jobs.getSnapshot().jobs.every((job) => job.request === null)).toBe(true);
      expect(mock.calls.some((request) => request.method === "open_folder")).toBe(false);
    } finally { release(); session.dispose(); await settle(); mock.dispose(); }
  });

  it("ignores old-project XO, level and failed compute side effects while preserving active lifecycle", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await settle();
      const xo = await session.call({ method: "recommend_xo" });
      const level = await session.call({ method: "level_check", params: { role: "L" } });
      mock.events.emit({ event: "started", job: 1000, kind: "calibrate" });
      await session.call(newProject); await settle();
      mock.calls.length = 0;
      mock.events.emit({ event: "finished", job: xo.job, status: "succeeded", result: { kind: "xo", data: xoRecommendation }, error: null });
      mock.events.emit({ event: "finished", job: level.job, status: "succeeded", result: { kind: "level", data: levelResult }, error: null });
      mock.events.emit({ event: "finished", job: 1000, status: "failed", result: null, error: ipcError("engine_input", "old") });
      await settle();
      expect(session.host.getSnapshot()).toMatchObject({ xoRecommendation: null, sweepResult: null, failure: null });
      expect(session.jobs.getSnapshot().jobs.every((job) => job.request === null)).toBe(true);
      expect(session.jobs.getSnapshot().jobs.find((job) => job.job === 1000)?.status).toBe("failed");
      expect(mock.calls.some((request) => request.method === "results")).toBe(false);
    } finally { session.dispose(); await settle(); mock.dispose(); }
  });
});
