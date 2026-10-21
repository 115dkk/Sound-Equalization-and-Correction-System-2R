import { describe, expect, it } from "vitest";
import type { JobEvent } from "./bindings/JobEvent";
import type { Request } from "./bindings/Request";
import type { MeasureConfigureParams } from "./bindings/MeasureConfigureParams";
import type { SettingEdit } from "./bindings/SettingEdit";
import { createMockService } from "./mock";
import * as fx from "./fixtures";

const configure: MeasureConfigureParams = {
  method: "mock", input_device: null, output_device: null, input_channel: null, output_channels: [], sweep_length: null,
  repetitions: null, sweep_level_dbfs: null, timing: null, loopback_input_channel: null, timing_ref_output: null, share_mode: null,
};
const newProject = (sample = true, tutorial = false): Request => ({ method: "project_new", params: { path: "/demo.secs", preset: "2.0", sub_count: 0, sample, tutorial } });

describe("P33 mock wiring", () => {
  it("keeps sample configuration separate from tutorial and starts with no measurements", async () => {
    const mock = createMockService({ tickMs: 0 });
    for (const sample of [true, false]) for (const tutorial of [true, false]) {
      await mock.request(newProject(sample, tutorial));
      expect(await mock.request({ method: "project_info" })).toMatchObject({ preset: sample || tutorial ? "7.1.4" : "2.0", sub_count: sample || tutorial ? 3 : 0, tutorial, positions: [] });
      expect((await mock.request({ method: "config_get" })).speakers).toHaveLength(sample || tutorial ? 14 : 2);
      expect((await mock.request({ method: "measurement_records" })).records.every((r) => !r.done && r.roles.length === 0)).toBe(true);
      expect((await mock.request({ method: "measurement_curves", params: { positions: null, roles: null } })).curves).toEqual([]);
    }
  });

  it("preserves nullable measurement patches, wiring, repetitions and level state", async () => {
    const mock = createMockService({ tickMs: 0 });
    const first = await mock.request({ method: "measure_configure", params: { ...configure, input_device: "mic", output_device: "hdmi", repetitions: 4, output_channels: fx.SAMPLE_ROLES.map((_, i) => 16 - i) } });
    expect(first.method).toBe("mock");
    const second = await mock.request({ method: "measure_configure", params: { ...configure, sweep_length: 65536 } });
    expect(second).toMatchObject({ input_device: "mic", output_device: "hdmi", repetitions: 4, sweep_length: 65536, output_channels: first.output_channels });
    expect((await mock.request({ method: "config_get" })).speakers[0]?.bands[0]?.out).toBe(16);
    const monitor = await mock.request({ method: "level_monitor_start", params: { role: null } });
    const events: JobEvent[] = []; mock.events.subscribe((event) => events.push(event));
    await mock.request({ method: "level_play", params: { role: "L", master_dbfs: -20 } });
    for (let i = 0; i < 5; i++) mock.tick();
    expect((await mock.request({ method: "levels_fill_defaults" })).levels).toMatchObject({ L: -30, R: -12 });
    const ended = events.findIndex((event) => event.event === "log" && event.job === monitor.job && event.code === "level_play_end");
    const lastMeter = events.reduce((index, event, current) => event.event === "meter" && event.job === monitor.job ? current : index, -1);
    expect(ended).toBeGreaterThan(lastMeter);
    expect(await mock.request({ method: "level_apply", params: { level: { ...fx.levelResult, recommended_sweep_dbfs: -18 } } })).toMatchObject({ sweep_level_dbfs: -18, repetitions: 4 });
    await expect(mock.request({ method: "measure_configure", params: { ...configure, repetitions: 3 } })).rejects.toMatchObject({ error: { code: "invalid_request" } });
  });

  it("records the requested position and imports assigned roles with their sources", async () => {
    const mock = createMockService({ tickMs: 0 });
    await mock.request(newProject());
    await mock.request({ method: "layout_set", params: { count: 9, confirm: true, start_delay: null, timing: null } });
    await mock.request({ method: "measure_position", params: { position: 7 } }); mock.drain();
    expect((await mock.request({ method: "project_info" })).positions).toEqual([7]);
    expect((await mock.request({ method: "measurement_records" })).records.find((r) => r.position === 7)).toMatchObject({ done: true, roles: expect.arrayContaining([...fx.SAMPLE_ROLES, "SW3"]) });
    const curves = await mock.request({ method: "measurement_curves", params: { positions: [7], roles: ["L"] } });
    expect(curves.curves).toHaveLength(1);
    expect(curves.curves[0]).toMatchObject({ position: 7, role: "L" });
    expect(curves.curves[0]?.values).toHaveLength(curves.grid.length);
    await mock.request({ method: "mdat_import", params: { path: "/room.mdat", position: 3, assignments: [{ measurement: "Left", role: "L", index: 0, position: null }] } }); mock.drain();
    expect((await mock.request({ method: "measurement_records" })).records.find((r) => r.position === 3)).toMatchObject({ done: false, roles: ["L"], sources: { L: "room.mdat" } });
    const cleared = await mock.request({ method: "position_clear", params: { position: 7 } });
    expect((await mock.request({ method: "project_info" })).positions).toEqual([3]);
    await mock.request({ method: "undo", params: { token: cleared.token } });
    expect((await mock.request({ method: "project_info" })).positions).toEqual([3, 7]);
    await expect(mock.request({ method: "undo", params: { token: cleared.token } })).rejects.toMatchObject({ error: { code: "invalid_request" } });
  });

  it("requires confirmation and restores exact configuration/layout with destructive undo tokens", async () => {
    const mock = createMockService({ tickMs: 0 });
    const config = await mock.request({ method: "config_get" });
    await expect(mock.request({ method: "config_apply_preset", params: { preset: "2.0", confirm: false } })).rejects.toMatchObject({ error: { code: "confirm_required" } });
    const changed = await mock.request({ method: "config_apply_preset", params: { preset: "2.0", confirm: true } });
    expect(changed.undo_token).not.toBeNull();
    expect((await mock.request({ method: "project_info" })).positions).toEqual([]);
    await mock.request({ method: "undo", params: { token: changed.undo_token! } });
    expect(await mock.request({ method: "config_get" })).toEqual(config);
    expect((await mock.request({ method: "project_info" })).positions).toEqual([1, 2, 3]);
    const layout = await mock.request({ method: "layout_set", params: { count: 1, start_delay: null, timing: null, confirm: true } });
    await mock.request({ method: "undo", params: { token: layout.undo_token! } });
    expect((await mock.request({ method: "layout_get" })).count).toBe(9);
    const subs = await mock.request({ method: "config_set_sub_count", params: { count: 1, confirm: true } });
    expect(subs.config.summary.sub_count).toBe(1);
    await mock.request({ method: "undo", params: { token: subs.undo_token! } });
    expect(await mock.request({ method: "config_get" })).toEqual(config);
  });

  it("keeps monitors alive while draining jobs, and stops them with Finished/stopped", async () => {
    const mock = createMockService({ tickMs: 0 });
    const events: JobEvent[] = []; mock.events.subscribe((event) => events.push(event));
    const watch = await mock.request({ method: "status_watch_start" });
    const meter = await mock.request({ method: "level_monitor_start", params: { role: null } });
    mock.drain(); expect(events).toHaveLength(2);
    mock.tick(); mock.tick();
    expect(events.filter((e) => e.event === "meter")).toHaveLength(2);
    expect(events.filter((e) => e.event === "measure_status")).toHaveLength(2);
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
    expect((await mock.request({ method: "jobs" })).map((j) => j.job)).toEqual([watch.job, meter.job]);
    expect(await mock.request({ method: "level_monitor_stop" })).toMatchObject({ status: "succeeded" });
    await mock.request({ method: "status_watch_stop" });
    expect(events.at(-1)).toMatchObject({ event: "finished", job: watch.job, result: { kind: "stopped" } });
    expect(await mock.request({ method: "jobs" })).toEqual([]);
  });

  it("updates calibration, MIMO and export results, and preserves job failure injection", async () => {
    const mock = createMockService({ tickMs: 0 });
    const events: JobEvent[] = []; mock.events.subscribe((event) => events.push(event));
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mode", mode: "linear" } } });
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
    expect((await mock.request({ method: "results" })).calibration).toMatchObject({ mode: "linear", stale: false });
    expect((await mock.request({ method: "result_lines" })).ba?.rows).toHaveLength(2);
    await mock.request({ method: "mimo", params: { settings: null } }); mock.drain();
    expect(events.at(-1)).toMatchObject({ result: { kind: "mimo" } });
    await mock.request({ method: "target_apply_preset", params: { preset: "flat" } });
    expect((await mock.request({ method: "results" })).mimo?.stale).toBe(true);
    await mock.request({ method: "export_folder_set", params: { folder: "/exports" } });
    expect(await mock.request({ method: "export_folder_get" })).toEqual({ folder: "/exports", stored: true });
    await mock.request({ method: "export", params: { kind: "camilla_dsp", folder: "" } }); mock.drain();
    expect(events.at(-1)).toMatchObject({ result: { kind: "export", data: { folder: "/exports/camilladsp", files: ["/exports/camilladsp/secs_camilladsp.yml", "/exports/camilladsp/fir_L.wav"], stale: true } } });
    mock.failJobs("measure", "device_unavailable");
    await mock.request({ method: "measure_position", params: { position: 7 } }); mock.drain();
    expect(events.at(-1)).toMatchObject({ status: "failed", error: { code: "device_unavailable" } });
    expect((await mock.request({ method: "project_info" })).positions).not.toContain(7);
  });

  it("applies every SettingEdit variant without a numerical engine", async () => {
    const mock = createMockService({ tickMs: 0 });
    const edits = {
      mode: { kind: "mode", mode: "ultra" }, ultra_ms: { kind: "ultra_ms", ms: 999 }, rc_lo: { kind: "rc_lo", hz: 25 }, rc_hi: { kind: "rc_hi", hz: 500 },
      sample_rate_policy: { kind: "sample_rate_policy", policy: "resample" },
      room_volume: { kind: "room_volume", m3: 60 }, room_dims: { kind: "room_dims", dims: [5, 4, 3] }, boost_max: { kind: "boost_max", db: 6 },
      xo: { kind: "xo", group: "front", hz: 100 }, xo_peq: { kind: "xo_peq", on: false, lo: 30, hi: 150 },
      infra: { kind: "infra", role: "SW1", hz: 40, type: "LR24", subs: ["SW1", "SW2"] }, infra_boost: { kind: "infra_boost", db: 18 },
      sub_mode: { kind: "sub_mode", role: "SW1", mode: "bundle:A" }, sub_layout: { kind: "sub_layout", bundle: "A", layout: "dba" },
      rear_trim: { kind: "rear_trim", bundle: "A", ms: 3 }, sub: { kind: "sub", role: "SW1", patch: { gain_db: -4 } },
      sub_band_type: { kind: "sub_band_type", role: "SW1", band: "hp", type: "off" }, main_delay: { kind: "main_delay", ms: 2 },
      mimo_on: { kind: "mimo_on", on: true }, fsiso: { kind: "fsiso", hz: 100 }, mimo_low: { kind: "mimo_low", hz: 20 },
      mimo_max: { kind: "mimo_max", count: 4 }, mimo_fir: { kind: "mimo_fir", on: true },
    } satisfies { [K in SettingEdit["kind"]]: Extract<SettingEdit, { kind: K }> };
    for (const edit of Object.values(edits)) await mock.request({ method: "calibration_settings_edit", params: { edit } });
    expect(await mock.request({ method: "calibration_settings_get" })).toMatchObject({ mode: "ultra", sample_rate_policy: "resample", ultra_ms: 200, room_volume_m3: 60, room_dims_m: [5, 4, 3], mimo_on: true, mimo_fir: true, subs_manual: { SW1: { mode: "bundle", gain_db: -4, hp_hz: 0 } } });
  });

  it("answers every generated Request variant with its tagged typed response", async () => {
    // This mapped type makes newly generated methods a compile-time test gap.
    const requests = {
      compute_status: { method: "compute_status" },
      open_gpu_help: { method: "open_gpu_help" },
      bootstrap: { method: "bootstrap" }, project_info: { method: "project_info" },
      project_new: { method: "project_new", params: { path: "/new.secs", preset: "2.0", sub_count: 0, sample: true, tutorial: false } },
      project_open: { method: "project_open", params: { path: "/saved.secs" } }, project_save: { method: "project_save", params: { path: null } },
      sample_load: { method: "sample_load", params: { positions: 1 } }, calibrate: { method: "calibrate", params: { settings: null, subs_only: false } }, recommend_xo: { method: "recommend_xo" },
      mimo: { method: "mimo", params: { settings: null } }, results: { method: "results" }, export: { method: "export", params: { kind: "before_after_mdat", folder: "" } },
      job_cancel: { method: "job_cancel", params: { job: 1 } }, jobs: { method: "jobs" }, measure_methods: { method: "measure_methods" }, measure_config_get: { method: "measure_config_get" }, measure_configure: { method: "measure_configure", params: configure },
      measurement_records: { method: "measurement_records" }, levels_get: { method: "levels_get" }, levels_fill_defaults: { method: "levels_fill_defaults" }, level_stop: { method: "level_stop" },
      level_check: { method: "level_check", params: { role: "L" } }, level_apply: { method: "level_apply", params: { level: fx.levelResult } },
      measure_position: { method: "measure_position", params: { position: 4 } }, position_clear: { method: "position_clear", params: { position: 1 } }, undo: { method: "undo", params: { token: "undo-1" } },
      rew_launch: { method: "rew_launch" }, mdat_scan: { method: "mdat_scan", params: { path: "/room.mdat" } },
      mdat_import: { method: "mdat_import", params: { path: "/room.mdat", position: 1, assignments: [] } }, config_get: { method: "config_get" },
      config_apply_preset: { method: "config_apply_preset", params: { preset: "2.0", confirm: true } }, config_set_sub_count: { method: "config_set_sub_count", params: { count: 1, confirm: true } },
      config_set_speaker: { method: "config_set_speaker", params: { index: 0, role: null, size: "large", band_index: null, out: null, pol: null } },
      config_add_speaker: { method: "config_add_speaker" }, config_remove_speaker: { method: "config_remove_speaker", params: { index: 0 } }, config_renumber: { method: "config_renumber" }, config_validate: { method: "config_validate" },
      patch_sheet_csv: { method: "patch_sheet_csv", params: { path: "/patch.csv" } }, dsp_get: { method: "dsp_get" }, dsp_set: { method: "dsp_set", params: { preset: "minidsp", fields: null } },
      layout_get: { method: "layout_get" }, layout_set: { method: "layout_set", params: { count: 1, start_delay: null, timing: null, confirm: true } }, position_estimate: { method: "position_estimate" },
      target_get: { method: "target_get" }, target_set_points: { method: "target_set_points", params: { points: [[20, 0], [20000, -3]] } }, target_apply_preset: { method: "target_apply_preset", params: { preset: "flat" } },
      target_save: { method: "target_save", params: { path: "/target.json" } }, target_load: { method: "target_load", params: { path: "/target.json" } }, calibration_settings_get: { method: "calibration_settings_get" },
      calibration_settings_set: { method: "calibration_settings_set", params: { settings: fx.calibrationSettings } }, calibration_settings_edit: { method: "calibration_settings_edit", params: { edit: { kind: "mode", mode: "zero" } } },
      sub_warnings: { method: "sub_warnings" }, rew_info: { method: "rew_info" }, rew_driver_set: { method: "rew_driver_set", params: { driver: "ASIO" } }, mode_spec: { method: "mode_spec", params: { mode: "mixed", ultra_ms: null, mimo_fir: true } },
      app_settings_get: { method: "app_settings_get" }, app_settings_set: { method: "app_settings_set", params: { settings: fx.appSettings } },
      measurement_curves: { method: "measurement_curves", params: { positions: null, roles: null } }, result_curves: { method: "result_curves", params: { tab: "spl", channel: null, group: null, xo_hz: null, corrected: null } },
      result_lines: { method: "result_lines" }, sub_take_values: { method: "sub_take_values" }, dialog_open_project: { method: "dialog_open_project" }, dialog_save_project: { method: "dialog_save_project", params: { suggested: "new.secs" } },
      dialog_pick_mdat: { method: "dialog_pick_mdat" }, dialog_pick_folder: { method: "dialog_pick_folder", params: { start: null } }, dialog_save_file: { method: "dialog_save_file", params: { kind: "target", suggested: "target.json" } },
      dialog_open_file: { method: "dialog_open_file", params: { kind: "target" } }, open_folder: { method: "open_folder", params: { path: "/exports" } },
      level_monitor_start: { method: "level_monitor_start", params: { role: null } }, level_monitor_stop: { method: "level_monitor_stop" }, level_play: { method: "level_play", params: { role: "L", master_dbfs: -20 } },
      status_watch_start: { method: "status_watch_start" }, status_watch_stop: { method: "status_watch_stop" }, export_folder_get: { method: "export_folder_get" }, export_folder_set: { method: "export_folder_set", params: { folder: "/exports" } },
    } satisfies { [K in Request["method"]]: Extract<Request, { method: K }> };
    for (const request of Object.values(requests)) {
      const mock = createMockService({ tickMs: 0, results: fx.resultsSummary });
      if (request.method === "undo") await mock.request({ method: "position_clear", params: { position: 1 } });
      if (request.method === "job_cancel") await mock.request({ method: "sample_load", params: { positions: 1 } });
      expect(await mock.invoke("secs_call", { request }), request.method).toMatchObject({ ok: true, data: { method: request.method } });
      mock.drain(); mock.dispose();
    }
  });

  it("supports dialog cancellation and independent service instances without fixture mutation", async () => {
    const a = createMockService({ tickMs: 0 }); const b = createMockService({ tickMs: 0 });
    a.queueDialog("dialog_open_project", null);
    expect(await a.request({ method: "dialog_open_project" })).toBeNull();
    expect(await a.request({ method: "dialog_open_project" })).toBe(fx.projectInfo.path);
    const before = await b.request({ method: "target_get" });
    await a.request({ method: "target_set_points", params: { points: [[30, 9]] } });
    expect(await b.request({ method: "target_get" })).toEqual(before);
    const settings = await a.request({ method: "calibration_settings_get" }); settings.mode = "zero";
    expect((await a.request({ method: "calibration_settings_get" })).mode).toBe("mixed");
  });
});
