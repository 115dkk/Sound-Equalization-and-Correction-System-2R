import { expect, it, vi } from "vitest";
import { createMockService } from "../ipc/mock";
import { createSession } from "./session";
import { RULES, effectsOf, type ResourceKey, type Trigger } from "./effects";
import type { Request } from "../ipc/bindings/Request";
import { MOCK_DESIGN_ID, applyStatus, eqapoXt, xoRecommendation } from "../ipc/fixtures";

const reloadCases: [ResourceKey, Request, Request["method"]][] = [
  ["project", { method: "project_save", params: { path: "/room.secs" } }, "project_info"],
  ["config", { method: "config_renumber" }, "config_get"],
  ["notices", { method: "config_renumber" }, "config_validate"],
  ["dsp", { method: "dsp_set", params: { preset: "custom", fields: null } }, "dsp_get"],
  ["layout", { method: "config_renumber" }, "layout_get"],
  ["estimate", { method: "config_renumber" }, "position_estimate"],
  ["methods", { method: "rew_launch" }, "measure_methods"],
  ["records", { method: "position_clear", params: { position: 1 } }, "measurement_records"],
  ["curves", { method: "position_clear", params: { position: 1 } }, "measurement_curves"],
  ["micCalibration", { method: "mic_calibration_set", params: { path: "/mic.txt" } }, "mic_calibration_get"],
  ["levels", { method: "levels_fill_defaults" }, "levels_get"],
  ["settings", { method: "calibration_settings_edit", params: { edit: { kind: "mimo_fir", on: true } } }, "calibration_settings_get"],
  ["modeSpec", { method: "calibration_settings_edit", params: { edit: { kind: "mimo_fir", on: true } } }, "mode_spec"],
  ["target", { method: "target_apply_preset", params: { preset: "flat" } }, "target_get"],
  ["results", { method: "project_save", params: { path: "/room.secs" } }, "results"],
  ["resultCurves", { method: "target_apply_preset", params: { preset: "flat" } }, "result_curves"],
  ["lines", { method: "target_apply_preset", params: { preset: "flat" } }, "result_lines"],
  ["subWarnings", { method: "config_renumber" }, "sub_warnings"],
  ["rew", { method: "rew_launch" }, "rew_info"],
  ["folder", { method: "export_folder_set", params: { folder: "/exports" } }, "export_folder_get"],
  ["computeStatus", { method: "app_settings_set", params: { settings: { language: "ko", theme: "system", icon: "expert", skin: "default", compute: "cpu", local_api: { enabled: false, port: 4712 }, apply: { camilla: { level: "files", address: "127.0.0.1:1234" }, eqapo: { level: "files" } }, recent_projects: [], last_window_size: null } } }, "compute_status"],
  ["appSettings", { method: "project_save", params: { path: "/room.secs" } }, "app_settings_get"],
];
it.each(reloadCases)("reload.%s refreshes its public store", async (key, request, getter) => {
  const effects = effectsOf({ request: request.method });
  expect(effects.rules.find((rule) => rule.name === `reload.${key}`)?.reload).toEqual([key]);
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.settled();
    const previous = session.stores[key].getSnapshot();
    mock.calls.length = 0;
    await session.call(request); await session.settled();
    expect(mock.calls.filter((call) => call.method === getter)).toHaveLength(1);
    expect(session.stores[key].getSnapshot()).not.toBeNull();
    expect(session.stores[key].getSnapshot()).not.toBe(previous);
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

it.each(["project_new", "project_open"] as const)("%s reloads every project resource", (request) => {
  expect(effectsOf({ request }).reload.sort()).toEqual(reloadCases.map(([key]) => key).sort());
});
it.each(["measure", "sample_load", "mdat_import"] as const)("reloads committed data after %s regardless of terminal status", (finished) => {
  for (const status of ["succeeded", "failed", "cancelled"] as const) {
    expect(effectsOf({ finished, status }).reload).toEqual(["project", "records", "curves", "micCalibration", "results", "resultCurves", "lines"]);
  }
});
it("refreshes mic calibration counts after every measurement edit and only invalidates results on attachment", () => {
  for (const request of ["position_clear", "undo", "layout_set", "config_apply_preset", "config_set_sub_count", "config_set_speaker", "config_add_speaker", "config_remove_speaker", "config_renumber"] as const) {
    expect(effectsOf({ request }).reload, request).toContain("micCalibration");
  }
  for (const request of ["mic_calibration_set", "mic_calibration_clear"] as const) {
    expect(effectsOf({ request }).reload).toEqual(["project", "micCalibration"]);
  }
  expect(effectsOf({ request: "mic_calibration_attach" }).reload).toEqual(["curves", "micCalibration", "results", "resultCurves", "lines"]);
});

it("reloads attached counts and stale results without making the project dirty", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.settled();
    await session.call({ method: "mic_calibration_set", params: { path: "/mic.txt" } });
    expect(session.stores.project.getSnapshot()?.dirty).toBe(true);
    const count = session.stores.micCalibration.getSnapshot()!.attachable;
    await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain(); await session.settled();
    await session.call({ method: "project_save", params: { path: "/mic.secs" } });
    mock.calls.length = 0;
    await session.call({ method: "mic_calibration_attach" });
    expect(session.stores.micCalibration.getSnapshot()).toMatchObject({ attached: count, attachable: 0 });
    expect(session.stores.results.getSnapshot()?.calibration).toMatchObject({ stale: true, stale_reason: "measurements" });
    expect(session.stores.lines.getSnapshot()?.stale).toBe(true);
    expect(session.stores.resultCurves.getSnapshot()?.curves?.stale).toBe(true);
    expect(mock.calls.map((call) => call.method)).toEqual(expect.arrayContaining(["measurement_curves", "mic_calibration_get", "results", "result_curves", "result_lines"]));
    expect(session.stores.project.getSnapshot()?.dirty).toBe(false);
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

it("covers settings, results, export and explicit mutations without timers", () => {
  expect(effectsOf({ request: "calibration_settings_edit" }).reload).toEqual(expect.arrayContaining(["settings", "modeSpec", "subWarnings", "project", "results"]));
  expect(effectsOf({ finished: "calibrate", status: "succeeded" }).reload).toEqual(expect.arrayContaining(["results", "resultCurves", "lines"]));
  expect(effectsOf({ finished: "mimo", status: "succeeded" }).reload).toContain("results");
  expect(effectsOf({ request: "project_save" }).reload).toContain("project");
  expect(effectsOf({ request: "position_clear" }).reload).toContain("records");
});

it("has a table test row for every stable rule name", () => {
  expect(RULES.map((rule) => rule.name).sort()).toEqual([
    ...reloadCases.map(([key]) => `reload.${key}`), ...responseCases.map(({ name }) => name),
    ...finishedCases.map(({ name }) => name), "project.replace", "warnings.low-snr",
  ].sort());
});

it.each(["start", "curves", "monitor"] as const)("settled waits for a directly started %s operation", async (operation) => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  let release!: () => void;
  let entered!: () => void;
  const blocked = new Promise<void>((resolve) => { release = resolve; });
  const reached = new Promise<void>((resolve) => { entered = resolve; });
  try {
    if (operation !== "start") { await session.start(); await session.settled(); }
    const original = mock.request;
    const method = operation === "start" ? "project_info" : operation === "curves" ? "result_curves" : "level_monitor_start";
    vi.spyOn(mock, "request").mockImplementation(async (request) => {
      if (request.method === method) { entered(); await blocked; }
      return original(request);
    });
    const work = operation === "start" ? session.start() : operation === "curves"
      ? session.refreshCurves({ tab: "gd", channel: "R", group: null, xo_hz: null, corrected: null }) : session.monitor(true);
    let done = false;
    const waiting = session.settled().then(() => { done = true; });
    await reached;
    await Promise.resolve();
    expect(done).toBe(false);
    release(); await work; await waiting;
    expect(done).toBe(true);
  } finally { release(); vi.restoreAllMocks(); session.dispose(); await session.settled(); mock.dispose(); }
});

it.each(["call", "reload", "run"] as const)("settled waits for %s failures and resolves for all concurrent waiters", async (operation) => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  await session.start(); await session.settled();
  let release!: () => void;
  const blocked = new Promise<void>((resolve) => { release = resolve; });
  const original = mock.request;
  vi.spyOn(mock, "request").mockImplementation(async (request) => {
    if (request.method === "target_get") { await blocked; throw new Error("read failed"); }
    return original(request);
  });
  try {
    const failed = operation === "call" ? session.call({ method: "target_get" }).catch(() => {})
      : operation === "reload" ? session.stores.target.load() : session.run(async () => { await blocked; throw new Error("work failed"); });
    let count = 0;
    const waits = [session.settled().then(() => { count++; }), session.settled().then(() => { count++; })];
    await Promise.resolve();
    expect(count).toBe(0);
    release(); await failed; await Promise.all(waits);
    expect(count).toBe(2);
    await session.settled();
    if (operation !== "run") expect(session.errors.getSnapshot()?.error.detail).toContain("read failed");
  } finally { release(); vi.restoreAllMocks(); session.dispose(); await session.settled(); mock.dispose(); }
});

it("settled includes export completion's nested folder request", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  await session.start();
  await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
  mock.drain(); await session.settled();
  let release!: () => void;
  let entered!: () => void;
  const blocked = new Promise<void>((resolve) => { release = resolve; });
  const reached = new Promise<void>((resolve) => { entered = resolve; });
  const original = mock.request;
  vi.spyOn(mock, "request").mockImplementation(async (request) => {
    if (request.method === "open_folder") { entered(); await blocked; }
    return original(request);
  });
  try {
    await session.call({ method: "export", params: { kind: "camilla_dsp", folder: "/export" } });
    mock.drain();
    let done = false;
    const waiting = session.settled().then(() => { done = true; });
    await reached; await Promise.resolve();
    expect(done).toBe(false);
    expect(session.host.getSnapshot().runs.camilla_dsp?.status).toBe("done");
    release(); await waiting;
    expect(mock.calls).toContainEqual({ method: "open_folder", params: { path: "/export" } });
  } finally { release(); vi.restoreAllMocks(); session.dispose(); await session.settled(); mock.dispose(); }
});

it("settled waits across a save dialog and its following save", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  await session.start(); await session.settled();
  session.stores.project.set({ ...session.stores.project.getSnapshot()!, path: "" });
  mock.queueDialog("dialog_save_project", "/saved.secs");
  const original = mock.request;
  let release!: () => void;
  let entered!: () => void;
  const blocked = new Promise<void>((resolve) => { release = resolve; });
  const reached = new Promise<void>((resolve) => { entered = resolve; });
  vi.spyOn(mock, "request").mockImplementation(async (request) => {
    if (request.method === "project_save") { entered(); await blocked; }
    return original(request);
  });
  try {
    const saving = session.ensureSaved();
    let done = false;
    const waiting = session.settled().then(() => { done = true; });
    await reached;
    expect(done).toBe(false);
    release(); await waiting;
    expect(await saving).toBe(true);
    expect(session.stores.project.getSnapshot()?.path).toBe("/saved.secs");
  } finally { release(); vi.restoreAllMocks(); session.dispose(); await session.settled(); mock.dispose(); }
});

it("settled waits for disposal scheduled by the session", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  await session.start(); await session.settled();
  const original = mock.request;
  let release!: () => void;
  let entered!: () => void;
  const blocked = new Promise<void>((resolve) => { release = resolve; });
  const reached = new Promise<void>((resolve) => { entered = resolve; });
  vi.spyOn(mock, "request").mockImplementation(async (request) => {
    if (request.method === "status_watch_stop") { entered(); await blocked; }
    return original(request);
  });
  try {
    session.dispose();
    let done = false;
    const waiting = session.settled().then(() => { done = true; });
    await reached;
    expect(done).toBe(false);
    release(); await waiting;
    expect(mock.calls.some((request) => request.method === "status_watch_stop")).toBe(true);
  } finally { release(); vi.restoreAllMocks(); await session.settled(); mock.dispose(); }
});

it("records low-SNR logs for the active measurement position through its rule", async () => {
  expect(effectsOf({ log: "measurement_low_snr" }).rules.map((rule) => rule.name)).toEqual(["warnings.low-snr"]);
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.configure({ method: "mock" });
    await session.call({ method: "measure_position", params: { position: 4 } });
    mock.drain(); await session.settled();
    expect(session.host.getSnapshot().warnings).toEqual([{ position: 4, notices: [expect.objectContaining({ code: "measurement_low_snr" })] }]);
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

const finishedCases = [
  { name: "compute.failure", trigger: { finished: "calibrate", status: "failed" }, run: async (session, mock) => {
    mock.failJobs("calibrate", "engine_input");
    await session.call({ method: "calibrate", params: { settings: null, subs_only: true } });
    mock.drain(); await session.settled();
    expect(session.host.getSnapshot().failure).toMatchObject({ job: "calibrate_subs", error: { code: "engine_input" } });
  } },
  { name: "measurement.finished", trigger: { finished: "measure", status: "cancelled" }, run: async (session, mock) => {
    session.host.set({ selected: 4 });
    const started = await session.call({ method: "measure_position", params: { position: 4 } });
    await session.call({ method: "job_cancel", params: { job: started.job } });
    mock.drain(); await session.settled();
    expect(session.host.getSnapshot().selected).toBeNull();
    expect(session.stores.records.getSnapshot()).not.toBeNull();
  } },
  { name: "xo.result", trigger: { finished: "recommend_xo", status: "succeeded" }, run: async (session, mock) => {
    await session.call({ method: "recommend_xo" });
    mock.drain(); await session.settled();
    expect(session.host.getSnapshot().xoRecommendation).toEqual(xoRecommendation);
  } },
  { name: "export.invalidate", trigger: { finished: "calibrate", status: "succeeded" }, run: async (session, mock) => {
    session.host.set({ runs: { camilla_dsp: { status: "done", files: ["old.yml"], folder: "/old", rate: 48000, notes: [] } }, applyRuns: { camilla: { status: "applied", designId: "old", folder: "/applied" } } });
    await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain(); await session.settled();
    expect(session.host.getSnapshot().runs).toEqual({});
    expect(session.host.getSnapshot().applyRuns).toEqual({});
  } },
  { name: "apply.finished", trigger: { finished: "apply", status: "succeeded" }, run: async (session, mock) => {
    await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain(); await session.settled();
    await session.call({ method: "apply_settings_set", params: { camilla: { level: "apply", address: "127.0.0.1:1234" }, eqapo: { level: "apply" } } });
    mock.calls.length = 0;
    await session.call({ method: "apply_camilla", params: {} });
    mock.drain(); await session.settled();
    // The row records how it ended and the status is read again; nothing is opened.
    expect(session.host.getSnapshot().applyRuns.camilla).toMatchObject({ status: "applied", designId: MOCK_DESIGN_ID });
    expect(mock.calls.filter((call) => call.method === "apply_status")).toHaveLength(1);
    expect(mock.calls.some((call) => call.method === "open_folder")).toBe(false);
    // A failed job lands in its target's row.
    mock.setApplyStatus({ ...applyStatus, eqapo: eqapoXt });
    mock.failJobs("apply", "permission_denied", "Access is denied. (os error 5)", { path: "C:/Program Files/EqualizerAPO/config/SECS2R" });
    await session.call({ method: "apply_eqapo" });
    mock.drain(); await session.settled();
    expect(session.host.getSnapshot().applyRuns.eqapo).toMatchObject({ status: "failed", error: { code: "permission_denied" } });
    expect(session.host.getSnapshot().applyRuns.camilla?.status).toBe("applied");
  } },
  { name: "export.finished", trigger: { finished: "export", status: "succeeded" }, run: async (session, mock) => {
    await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain(); await session.settled();
    await session.call({ method: "export", params: { kind: "camilla_dsp", folder: "/export" } });
    mock.drain(); await session.settled();
    expect(session.host.getSnapshot().runs.camilla_dsp).toMatchObject({ status: "done", folder: "/export" });
    expect(mock.calls).toContainEqual({ method: "open_folder", params: { path: "/export" } });
  } },
  { name: "xo.recalibrate", trigger: { finished: "recommend_xo", status: "succeeded" }, run: async (session, mock) => {
    await session.call({ method: "recommend_xo" });
    mock.drain(); await session.settled();
    expect(session.stores.settings.getSnapshot()?.xo).toMatchObject({ "프론트": 80, "센터": 100 });
    expect(mock.calls).toContainEqual({ method: "calibrate", params: { settings: null, subs_only: false, mimo: false } });
    expect(session.jobs.getSnapshot().jobs.some((job) => job.kind === "calibrate" && job.status === "running")).toBe(true);
  } },
] satisfies { name: string; trigger: Trigger; run: (session: ReturnType<typeof createSession>, mock: ReturnType<typeof createMockService>) => Promise<void> }[];

it.each(finishedCases)("$name applies its completion effect", async ({ name, trigger, run }) => {
  expect(effectsOf(trigger).rules.map((rule) => rule.name)).toContain(name);
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.configure({ method: "mock" }); await session.settled();
    await run(session, mock); await session.settled();
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

it("xo.recalibrate sends the MIMO choice 보정 계산 would send and the ledger keeps it", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.configure({ method: "mock" }); await session.settled();
    const positions = session.stores.project.getSnapshot()?.positions.length ?? 0;
    await session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
    await session.settled();
    await session.call({ method: "recommend_xo" });
    mock.drain(); await session.settled();
    expect(positions).toBeGreaterThanOrEqual(3);
    const mimo = true;
    expect(mock.calls).toContainEqual({ method: "calibrate", params: { settings: null, subs_only: false, mimo } });
    const job = session.jobs.getSnapshot().jobs.find((entry) => entry.kind === "calibrate");
    expect(job?.request).toMatchObject({ method: "calibrate", params: { subs_only: false, mimo } });
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

const responseCases = [
  { name: "measurement.response", trigger: { request: "measure_config_get" }, run: async (session) => {
    session.host.set({ path: null });
    await session.call({ method: "measure_config_get" });
    expect(session.host.getSnapshot().path).toBe("mock");
    expect(session.stores.measurementSettings.getSnapshot()?.method).toBe("mock");
  } },
  { name: "measurement.refresh", trigger: { request: "config_renumber" }, run: async (session, mock) => {
    mock.calls.length = 0;
    await session.call({ method: "config_renumber" });
    expect(mock.calls.at(-1)?.method).toBe("measure_config_get");
    expect(session.stores.measurementSettings.getSnapshot()?.method).toBe("mock");
  } },
  { name: "warnings.replace-position", trigger: { request: "position_clear" }, run: async (session) => {
    session.host.set({ warnings: [{ position: 1, notices: [] }, { position: 4, notices: [] }] });
    await session.call({ method: "position_clear", params: { position: 4 } });
    expect(session.host.getSnapshot().warnings).toEqual([{ position: 1, notices: [] }]);
  } },
  { name: "warnings.replace-sample", trigger: { request: "sample_load" }, run: async (session) => {
    session.host.set({ warnings: [{ position: 4, notices: [] }] });
    await session.call({ method: "sample_load", params: { positions: null } });
    expect(session.host.getSnapshot().warnings).toEqual([]);
  } },
  { name: "rew.driver-response", trigger: { request: "rew_driver_set" }, run: async (session, mock) => {
    mock.calls.length = 0;
    await session.call({ method: "rew_driver_set", params: { driver: "ASIO" } });
    expect(session.stores.rew.getSnapshot()?.driver).toBe("ASIO");
    expect(mock.calls.map((request) => request.method)).toEqual(["rew_driver_set", "measure_methods"]);
  } },
] satisfies { name: string; trigger: Trigger; run: (session: ReturnType<typeof createSession>, mock: ReturnType<typeof createMockService>) => Promise<void> }[];

it.each(responseCases)("$name applies its response effect", async ({ name, trigger, run }) => {
  expect(effectsOf(trigger).rules.map((rule) => rule.name)).toContain(name);
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.configure({ method: "mock" }); await session.settled();
    await run(session, mock); await session.settled();
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

it("resets a replaced project before reloads and restores its stored measurement method after them", async () => {
  expect(effectsOf({ request: "project_open" }).rules.map((rule) => rule.name)).toContain("project.replace");
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.settled();
    await session.configure({ method: "mock" });
    await session.call({ method: "project_save", params: { path: "/kept.secs" } });
    session.host.set({ selected: 4, channel: "R", micCalBusy: "pick", micCalError: { code: "measurement_input", params: {}, retryable: false, detail: "old project" } });
    let duringReload: unknown;
    const off = session.stores.project.subscribe(() => { duringReload = session.host.getSnapshot(); });
    await session.call({ method: "project_open", params: { path: "/kept.secs" } });
    await session.settled(); off();
    expect(duringReload).toMatchObject({ selected: null, channel: null, path: null, micCalBusy: null, micCalError: null, opened: 1 });
    expect(session.host.getSnapshot().path).toBe("mock");
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

it("reads but never configures a newly replaced tutorial's measurement method", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.settled();
    mock.calls.length = 0;
    await session.call({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } });
    await session.settled();
    expect(mock.calls.filter((request) => request.method === "measure_config_get")).toHaveLength(1);
    expect(mock.calls.some((request) => request.method === "measure_configure")).toBe(false);
    expect(session.stores.measurementSettings.getSnapshot()?.method).toBe("none");
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

it("reloads saved project, results, folder and recent settings from the rule table", async () => {
  expect(effectsOf({ request: "project_save" }).reload).toEqual(["project", "results", "folder", "appSettings"]);
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.settled();
    await session.call({ method: "project_save", params: { path: "D:/New Room/living.secs" } });
    await session.settled();
    expect(session.stores.project.getSnapshot()?.path).toBe("D:/New Room/living.secs");
    expect(session.stores.folder.getSnapshot()?.folder).toMatch(/^D:\/New Room\/SECS_export\/\d{4}-\d{2}-\d{2} living$/);
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});
