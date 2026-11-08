import { describe, expect, it } from "vitest";
import type { Request } from "./bindings/Request";
import type { SettingEdit } from "./bindings/SettingEdit";
import type { UpdatePhase } from "./bindings/UpdatePhase";
import { SCALAR_EDITS } from "./bindings/rules";
import { createMockService, updateStates, type MockService } from "./mock";
import { RECORDED, RECORDED_EDITS, RECORDED_EXPORTS, RECORDED_RESULTS, RECORDED_RESULT_LINES, RECORDED_UPDATE_TRANSITIONS, recordKey } from "./recorded";
import { newProjectSettings } from "./fixtures";

// Each recorded file is replayed on the project crates/secs-service/tests/recorded.rs
// answered it on, and the mock must answer as recorded (ADR 0014, ADR 0052).

const created = (sample: boolean, preset = "2.0"): Request => ({ method: "project_new", params: { path: "/recorded.secs", preset, sub_count: sample ? 3 : 0, sample, tutorial: false } });

async function withMock(run: (mock: MockService) => Promise<void>) {
  const mock = createMockService({ tickMs: 0, project: null });
  try { await run(mock); } finally { mock.dispose(); }
}

const noInput = Object.entries(RECORDED).flatMap(([method, entries]) =>
  entries.map((entry) => [`${method} ${JSON.stringify(entry.params)}`, method, entry.params, entry.data] as const),
);

describe("recorded no-input responses", () => {
  it.each(noInput)("the mock answers %s as recorded", (_name, method, params, data) => withMock(async (mock) => {
    await mock.request(created(false));
    const request = (params === null ? { method } : { method, params }) as Request;
    expect(await mock.invoke("secs_call", { request })).toEqual({ ok: true, data: { method, data } });
  }));
});

/** The edits of recorded.rs `result_cases`, in its order: the records' inputs. */
const RESULT_CASES: [string, SettingEdit[]][] = [
  ["default", []],
  ["strict", [{ kind: "rc_hi_basis", basis: "strict" }]],
  ["lenient", [{ kind: "rc_hi_basis", basis: "lenient" }]],
  ["agreement", [{ kind: "rc_hi_basis", basis: "agreement" }]],
  ["transition 1", [{ kind: "rc_hi_transition", factor: 1 }]],
  ["transition 2", [{ kind: "rc_hi_transition", factor: 2 }]],
  ["transition 4", [{ kind: "rc_hi_transition", factor: 4 }]],
  ["manual", [{ kind: "rc_hi", hz: 500 }]],
  ["no volume", [{ kind: "room_volume", m3: 0 }]],
  ["low protect", [{ kind: "low_protect", on: true }]],
  ["input high-pass", [{ kind: "input_hp", hz: 25 }]],
  ["extra bands", [{ kind: "rc_bands", bands: [{ lo_hz: 1000, hi_hz: 2000 }, { lo_hz: 15000, hi_hz: 20000 }] }]],
];

describe("recorded calibrations", () => {
  it("files every recorded case under its settings' key", () => {
    expect(RECORDED_RESULTS.map((entry) => entry.params.engine_key)).toEqual(RECORDED_RESULT_LINES.map((entry) => entry.params.engine_key));
    expect(RESULT_CASES).toHaveLength(RECORDED_RESULTS.length);
  });

  it.each(RESULT_CASES.map(([name, edits], index) => [name, edits, index] as const))("the mock answers the %s calibration of the sample room as recorded", (_name, edits, index) => withMock(async (mock) => {
    await mock.request(created(true));
    await mock.request({ method: "sample_load", params: { positions: 9 } });
    mock.drain();
    for (const edit of edits) await mock.request({ method: "calibration_settings_edit", params: { edit } });
    const settings = await mock.request({ method: "calibration_settings_get" });
    expect(recordKey(settings)).toBe(RECORDED_RESULTS[index]!.params.engine_key);
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain();
    expect(await mock.request({ method: "results" })).toEqual(RECORDED_RESULTS[index]!.data);
    expect(await mock.request({ method: "result_lines" })).toEqual(RECORDED_RESULT_LINES[index]!.data);
  }));

  it("answers a combination that was not recorded with the defaults' record, current or stale as this session's state says", () => withMock(async (mock) => {
    await mock.request(created(true));
    await mock.request({ method: "sample_load", params: { positions: 9 } });
    mock.drain();
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mode", mode: "linear" } } });
    expect(RECORDED_RESULTS.some((entry) => entry.params.engine_key === recordKey({ ...newProjectSettings, mode: "linear" }))).toBe(false);
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain();
    expect(await mock.request({ method: "results" })).toEqual(RECORDED_RESULTS[0]!.data);
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "boost_max", db: 6 } } });
    const stale = RECORDED_RESULTS[0]!.data.calibration!;
    expect((await mock.request({ method: "results" })).calibration).toEqual({ ...stale, stale: true, stale_reason: "settings" });
    expect((await mock.request({ method: "result_lines" })).stale).toBe(true);
  }));
});

/** The recorded edits the scalar table decides, and every recorded refusal. */
const scalarKinds = new Set<string>(SCALAR_EDITS.map((rule) => rule.kind));
const edits = RECORDED_EDITS.filter((entry) => !entry.data.ok || scalarKinds.has(entry.params.edit.kind))
  .map((entry) => [JSON.stringify(entry.params.edit), entry.params.edit, entry.data] as const);

describe("recorded settings edits", () => {
  it.each(edits)("the mock answers %s on the sample room as recorded", (_name, edit, data) => withMock(async (mock) => {
    await mock.request(created(true, "7.1.4"));
    expect(await mock.invoke("secs_call", { request: { method: "calibration_settings_edit", params: { edit } } })).toEqual(data);
  }));
});

describe("recorded exports", () => {
  it.each(RECORDED_EXPORTS.map((entry) => [entry.params.kind, entry] as const))("the mock writes the %s export as recorded", (kind, entry) => withMock(async (mock) => {
    await mock.request(created(false));
    await mock.request({ method: "sample_load", params: { positions: 9 } });
    mock.drain();
    // The exports were recorded from a calibration of the 2.0 project: two mains, no subs.
    const calibration = RECORDED_RESULTS[0]!.data.calibration!;
    mock.setResults({ ...RECORDED_RESULTS[0]!.data, calibration: { ...calibration, main_roles: ["L", "R"], sub_roles: [] } });
    const finished: unknown[] = [];
    mock.events.subscribe((event) => { if (event.event === "finished") finished.push(event.result); });
    const folder = `C:/recorded/${entry.data.folder}`;
    await mock.request({ method: "export", params: { kind, folder, ...(kind === "before_after_mdat" ? { mdat_scope: "all" } : {}) } });
    mock.drain();
    expect(finished).toEqual([{ kind: "export", data: { ...entry.data, folder, files: entry.data.files.map((file) => `${folder}/${file}`) } }]);
  }));
});

/** A phase of every kind, for the updater replay. */
const PHASES: Record<UpdatePhase["kind"], UpdatePhase> = {
  off: { kind: "off" }, idle: { kind: "idle" }, checking: { kind: "checking" }, up_to_date: { kind: "up_to_date" },
  available: updateStates.available.phase, downloading: updateStates.downloading.phase, ready: updateStates.readyOnExit.phase,
  installing: { kind: "installing", version: "1.2.0" }, portable: updateStates.portable.phase, failed: updateStates.failed.phase,
};

describe("recorded updater transitions", () => {
  it("covers every request in every phase", () => {
    const kinds = new Set(RECORDED_UPDATE_TRANSITIONS.map((entry) => entry.params.phase));
    for (const method of ["app_settings_set", "update_check", "update_download", "update_restart", "update_notice_dismiss"] as const) {
      for (const kind of kinds) expect(RECORDED_UPDATE_TRANSITIONS.some((entry) => entry.params.method === method && entry.params.phase === kind), `${method} ${kind}`).toBe(true);
    }
  });

  it.each(RECORDED_UPDATE_TRANSITIONS.map((entry) => [`${entry.params.method} (${entry.params.policy}, ${entry.params.phase})`, entry] as const))("the mock answers %s as recorded", (_name, entry) => withMock(async (mock) => {
    const { method, policy, phase } = entry.params;
    // app_settings_set records the policy chosen; the updater starts in another one.
    mock.setUpdateStatus({ ...updateStates.upToDate, policy: method !== "app_settings_set" ? policy : policy === "auto" ? "notify" : "auto", phase: PHASES[phase] });
    const request: Request = method === "app_settings_set"
      ? { method, params: { settings: { ...await mock.request({ method: "app_settings_get" }), updates: policy } } }
      : { method };
    const answer = await mock.invoke("secs_call", { request });
    if ("error" in entry.data) {
      expect(answer).toMatchObject({ ok: false, error: entry.data.error });
      expect((await mock.request({ method: "update_status" })).phase).toEqual(PHASES[phase]);
    } else {
      expect(answer).toMatchObject({ ok: true });
      expect(await mock.request({ method: "update_status" })).toMatchObject({ policy: entry.data.policy, phase: { kind: entry.data.phase } });
    }
  }));
});
