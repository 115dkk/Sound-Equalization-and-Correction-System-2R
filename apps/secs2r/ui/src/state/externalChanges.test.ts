import { afterEach, describe, expect, it, vi } from "vitest";
import { coalesceExternalChanges, externalRefresh } from "./externalChanges";
import { effectsOf } from "./effects";
import { createSession } from "./session";
import { createMockService } from "../ipc/mock";

afterEach(() => vi.useRealTimers());

const methods = [
  "project_new", "project_open", "project_save", "calibrate", "mimo", "export", "mdat_import",
  "measure_position", "measure_configure", "config_apply_preset", "config_set_speaker", "config_renumber",
  "dsp_set", "layout_set", "target_set_points", "target_apply_preset", "target_load", "target_save",
  "calibration_settings_set", "calibration_settings_edit", "app_settings_set", "export_folder_set",
  "position_clear", "undo", "sample_load", "recommend_xo", "job_cancel", "future_mutation",
];
it.each(methods)("refreshes all potentially changed resources for %s without replaying actions", (method) => {
  expect(externalRefresh(method)).toEqual({
    replaceProject: ["project_new", "project_open"].includes(method),
    resources: [...effectsOf({ request: "project_open" }).reload, "measurementSettings"], jobs: true,
  });
});

it("coalesces a burst and serializes notifications that arrive during a refresh", async () => {
  vi.useFakeTimers();
  let release!: () => void;
  const refresh = vi.fn().mockImplementationOnce(() => new Promise<void>((resolve) => { release = resolve; })).mockResolvedValue(undefined);
  const failed = vi.fn();
  const batch = coalesceExternalChanges(refresh, failed);
  batch.push("target_set_points"); batch.push("target_set_points"); batch.push("app_settings_set");
  await vi.advanceTimersByTimeAsync(24);
  expect(refresh).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1);
  expect(refresh).toHaveBeenCalledExactlyOnceWith(["target_set_points", "app_settings_set"]);
  batch.push("project_open"); batch.push("export_folder_set");
  await vi.advanceTimersByTimeAsync(100);
  expect(refresh).toHaveBeenCalledTimes(1);
  release();
  await vi.advanceTimersByTimeAsync(25);
  expect(refresh).toHaveBeenLastCalledWith(["project_open", "export_folder_set"]);
  expect(failed).not.toHaveBeenCalled();
  batch.push("calibrate"); batch.dispose();
  await vi.advanceTimersByTimeAsync(25);
  expect(refresh).toHaveBeenCalledTimes(2);
});

it("reports a failed refresh and continues with the next burst", async () => {
  vi.useFakeTimers();
  const error = new Error("read failed");
  const refresh = vi.fn().mockRejectedValueOnce(error).mockResolvedValue(undefined);
  const failed = vi.fn();
  const batch = coalesceExternalChanges(refresh, failed);
  batch.push("project_save"); await vi.advanceTimersByTimeAsync(25);
  expect(failed).toHaveBeenCalledWith(error);
  batch.push("app_settings_set"); await vi.advanceTimersByTimeAsync(25);
  expect(refresh).toHaveBeenCalledTimes(2);
  batch.dispose();
});

function setup(dropJobEvents = false) {
  const mock = createMockService({ tickMs: 0 });
  let listener: ((method: string) => void) | undefined;
  const off = vi.fn(() => { listener = undefined; });
  const subscribe = vi.fn(async (next: (method: string) => void) => { listener = next; return off; });
  const session = createSession({ ...mock, ...(dropJobEvents ? { events: { subscribe: () => () => {} } } : {}), externalChanges: { subscribe } });
  return { mock, session, subscribe, off, emit: (method: string) => listener?.(method) };
}

describe("session external changes", () => {
  it("subscribes once across StrictMode setup/cleanup/setup and cleans up on disposal", async () => {
    vi.useFakeTimers();
    const { mock, session, subscribe, off, emit } = setup();
    await session.start();
    session.dispose();
    await session.start(); await session.settled();
    expect(subscribe).toHaveBeenCalledTimes(1);
    expect(off).not.toHaveBeenCalled();
    mock.calls.length = 0;
    emit("app_settings_set");
    session.dispose(); await session.settled();
    expect(off).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(25);
    expect(mock.calls.some((r) => r.method === "app_settings_get")).toBe(false);
    mock.dispose();
  });

  it("releases a listener whose asynchronous registration completes after disposal", async () => {
    const mock = createMockService({ tickMs: 0 });
    let release!: (off: () => void) => void;
    const off = vi.fn();
    const session = createSession({ ...mock, externalChanges: { subscribe: () => new Promise((resolve) => { release = resolve; }) } });
    const starting = session.start();
    session.dispose();
    release(off);
    await starting; await session.settled();
    expect(off).toHaveBeenCalledTimes(1);
    mock.dispose();
  });

  it("refreshes external settings, project inputs, results and folder once per burst", async () => {
    vi.useFakeTimers();
    const { mock, session, emit } = setup();
    try {
      await session.start(); await session.settled();
      await mock.request({ method: "target_set_points", params: { points: [[20, 3], [20000, -2]] } });
      await mock.request({ method: "export_folder_set", params: { folder: "/external" } });
      const app = { ...await mock.request({ method: "app_settings_get" }), theme: "dark" as const, local_api: { enabled: true, port: 5000 } };
      await mock.request({ method: "app_settings_set", params: { settings: app } });
      mock.calls.length = 0;
      emit("target_set_points"); emit("export_folder_set"); emit("app_settings_set");
      await vi.advanceTimersByTimeAsync(25); await session.settled();
      expect(session.stores.target.getSnapshot()?.points).toEqual([[20, 3], [20000, -2]]);
      expect(session.stores.folder.getSnapshot()?.folder).toBe("/external");
      expect(session.stores.appSettings.getSnapshot()).toEqual(app);
      for (const method of ["project_info", "results", "app_settings_get", "calibration_settings_get", "export_folder_get", "jobs", "measure_config_get"] as const) {
        expect(mock.calls.filter((r) => r.method === method), method).toHaveLength(1);
      }
      expect(mock.calls.some((r) => r.method === "app_settings_set")).toBe(false);
    } finally { session.dispose(); await session.settled(); mock.dispose(); }
  });

  it("resets project selections and epoch without replaying the external open", async () => {
    vi.useFakeTimers();
    const { mock, session, emit } = setup();
    try {
      await session.start(); await session.settled();
      session.host.set({ selected: 2, channel: "L", xoGroup: "front" });
      const epoch = session.jobs.epoch;
      const opened = session.host.getSnapshot().opened;
      await mock.request({ method: "project_new", params: { path: "/external.secs", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
      mock.calls.length = 0;
      emit("project_new"); emit("project_new");
      await vi.advanceTimersByTimeAsync(25); await session.settled();
      expect(session.jobs.epoch).toBe(epoch + 1);
      expect(session.host.getSnapshot()).toMatchObject({ selected: null, channel: null, xoGroup: null, mdat: null, opened: opened + 1 });
      expect(session.stores.project.getSnapshot()?.path).toBe("/external.secs");
      expect(mock.calls.some((r) => r.method === "project_new")).toBe(false);
    } finally { session.dispose(); await session.settled(); mock.dispose(); }
  });

  it("resumes polling jobs found at startup and stops when their snapshots are no longer retained", async () => {
    vi.useFakeTimers();
    const { mock, session } = setup(true);
    const started = await mock.request({ method: "sample_load", params: { positions: 1 } });
    try {
      await session.start(); await session.settled();
      await vi.advanceTimersByTimeAsync(25); await session.settled();
      expect(session.jobs.getSnapshot().jobs.find((job) => job.job === started.job)?.status).toBe("running");
      mock.drain();
      await vi.advanceTimersByTimeAsync(1025); await session.settled();
      expect(session.jobs.getSnapshot().jobs.some((job) => job.job === started.job)).toBe(false);
      mock.calls.length = 0;
      await vi.advanceTimersByTimeAsync(2000);
      expect(mock.calls.filter((r) => r.method === "jobs")).toHaveLength(0);
    } finally { session.dispose(); await session.settled(); mock.dispose(); }
  });

  it("follows jobs started externally through completion even without a UI channel", async () => {
    vi.useFakeTimers();
    const { mock, session, emit } = setup(true);
    try {
      await session.start(); await session.settled();
      // API clients' jobs have no channel to this window.
      const service = mock.request;
      const started = await service({ method: "calibrate", params: { settings: null, subs_only: false } });
      emit("calibrate");
      await vi.advanceTimersByTimeAsync(25); await session.settled();
      expect(session.jobs.getSnapshot().jobs.find((job) => job.job === started.job)?.status).toBe("running");
      mock.drain();
      // The service retains terminal snapshots; this mock normally drops them.
      const request = session.ipc.request;
      vi.spyOn(session.ipc, "request").mockImplementation(async (params) => {
        const value = await request(params);
        return params.method === "jobs" ? [...value as object[], { job: started.job, kind: "calibrate", status: "succeeded", cancellable: false, progress: null, error: null }] as typeof value : value;
      });
      await vi.advanceTimersByTimeAsync(1025); await session.settled();
      expect(session.jobs.getSnapshot().jobs.find((job) => job.job === started.job)?.status).toBe("succeeded");
      expect(session.stores.results.getSnapshot()?.calibration).not.toBeNull();
      mock.calls.length = 0;
      await vi.advanceTimersByTimeAsync(2000);
      expect(mock.calls.filter((r) => r.method === "jobs")).toHaveLength(0);
    } finally { session.dispose(); await session.settled(); mock.dispose(); }
  });
});
