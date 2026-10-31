import { describe, expect, it } from "vitest";
import type { JobEvent } from "./bindings/JobEvent";
import type { Request } from "./bindings/Request";
import type { MeasureConfigureParams } from "./bindings/MeasureConfigureParams";
import type { SettingEdit } from "./bindings/SettingEdit";
import { CALIBRATION_LIMITS } from "./bindings/rules";
import { createMockService, updateStates } from "./mock";
import * as fx from "./fixtures";
import { CAMILLA_EXE_FILE } from "../screens/export/types";
import { mimoView } from "../screens/calibrate/fixtures";
import { scorecardModel } from "../screens/calibrate/scorecard";

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

  it("tracks mic curves across native sweeps, sample sweeps, imports, undo and project replacement", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      await mock.request(newProject(false));
      expect(await mock.request({ method: "mic_calibration_get" })).toEqual({ file: null, attached: 0, attachable: 0 });
      await mock.request({ method: "mic_calibration_set", params: { path: "/mic.txt" } });
      await mock.request({ method: "measure_configure", params: { ...configure, method: "native" } });
      await mock.request({ method: "measure_position", params: { position: 1 } }); mock.drain();
      expect(await mock.request({ method: "mic_calibration_get" })).toMatchObject({ attached: 2, attachable: 0 });
      await mock.request({ method: "mic_calibration_clear" });
      await mock.request({ method: "measure_position", params: { position: 2 } }); mock.drain();
      expect(await mock.request({ method: "mic_calibration_get" })).toEqual({ file: null, attached: 2, attachable: 2 });
      await mock.request({ method: "mic_calibration_set", params: { path: "/next.txt" } });
      await mock.request({ method: "mdat_import", params: { path: "/import.mdat", position: 2, assignments: [{ measurement: "L", role: "L", index: 0, position: null }] } }); mock.drain();
      expect(await mock.request({ method: "mic_calibration_attach" })).toMatchObject({ attached: 3, attachable: 0 });
      const cleared = await mock.request({ method: "position_clear", params: { position: 1 } });
      expect(await mock.request({ method: "mic_calibration_get" })).toMatchObject({ attached: 1, attachable: 0 });
      await mock.request({ method: "undo", params: { token: cleared.token } });
      expect(await mock.request({ method: "mic_calibration_get" })).toMatchObject({ attached: 3, attachable: 0 });
      await mock.request({ method: "project_save", params: { path: "/kept.secs" } });
      await mock.request(newProject(false));
      expect(await mock.request({ method: "mic_calibration_get" })).toEqual({ file: null, attached: 0, attachable: 0 });
      await mock.request({ method: "project_open", params: { path: "/kept.secs" } });
      expect(await mock.request({ method: "mic_calibration_get" })).toMatchObject({ file: { name: "next.txt" }, attached: 3, attachable: 0 });
      // Sample sweeps stay uncalibrated even with a selected native file.
      await mock.request({ method: "sample_load", params: { positions: 1 } }); mock.drain();
      expect(await mock.request({ method: "mic_calibration_get" })).toMatchObject({ attached: 1, attachable: 2 });
      await mock.request({ method: "measure_configure", params: configure });
      await mock.request({ method: "measure_position", params: { position: 3 } }); mock.drain();
      expect(await mock.request({ method: "mic_calibration_get" })).toMatchObject({ attached: 1, attachable: 4 });
      const changed = await mock.request({ method: "config_apply_preset", params: { preset: "2.0", confirm: true } });
      expect(await mock.request({ method: "mic_calibration_get" })).toMatchObject({ attached: 0, attachable: 0 });
      await mock.request({ method: "undo", params: { token: changed.undo_token! } });
      expect(await mock.request({ method: "mic_calibration_get" })).toMatchObject({ attached: 1, attachable: 4 });
    } finally { mock.dispose(); }
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

  it("matches the gallery's scorecard and late bass before and after a MIMO run", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      expect(fx.calibrationSummary.scorecard).toEqual(mimoView.results.calibration!.scorecard);
      expect(fx.calibrationSummary.late_energy).toEqual(mimoView.results.calibration!.late_energy);
      expect(fx.mimoSummary.scorecard).toEqual(mimoView.results.mimo!.scorecard);
      expect(fx.mimoSummary.late_energy).toEqual(mimoView.results.mimo!.late_energy);
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
      const before = await mock.request({ method: "results" });
      const lines = await mock.request({ method: "result_lines" });
      expect(before.calibration!.late_energy).toEqual(fx.calibrationSummary.late_energy);
      expect(scorecardModel(before, lines.channels)).toMatchObject({ late: true, mimo: false });
      await mock.request({ method: "mimo", params: { settings: null } }); mock.drain();
      const after = await mock.request({ method: "results" });
      expect(after.mimo_export).toBe("included");
      expect(after.mimo!.scorecard).toEqual(fx.mimoSummary.scorecard);
      expect(after.mimo!.late_energy).toEqual(fx.mimoSummary.late_energy);
      expect(scorecardModel(after, lines.channels)).toEqual(scorecardModel(mimoView.results, mimoView.lines.channels));
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
      expect(scorecardModel(await mock.request({ method: "results" }), lines.channels)).toMatchObject({ late: true, mimo: false });
    } finally { mock.dispose(); }
  });

  it.each([[1], [2, 7, 9]])("limits late bass to the project's measured positions %j", async (...positions) => {
    const mock = createMockService({ tickMs: 0, project: { ...fx.projectInfo, preset: "2.0", sub_count: 0, positions } });
    try {
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
      const calibration = (await mock.request({ method: "results" })).calibration!;
      expect(calibration.main_roles).toEqual(["L", "R"]);
      expect(calibration.late_energy.map(({ role, position }) => [role, position])).toEqual(positions.flatMap((position) => [["L", position], ["R", position]]));
      expect(calibration.late_energy.every((entry) => Number.isFinite(entry.before_db) && Number.isFinite(entry.after_db))).toBe(true);
      await mock.request({ method: "mimo", params: { settings: null } }); mock.drain();
      const mimo = (await mock.request({ method: "results" })).mimo!;
      expect(mimo.support_count).toBe(0);
      expect(mimo.scorecard).toEqual(calibration.scorecard.map((score) => ({ role: score.role, after_mimo: score.after })));
      expect(mimo.late_energy).toEqual(calibration.late_energy.map(({ role, position, after_db }) => ({ role, position, after_mimo_db: after_db })));
    } finally { mock.dispose(); }
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
    // export_folder.rs: a timestamped folder below the stored base, CamillaDSP in its `camilladsp`.
    const folder = expect.stringMatching(/^\/exports\/SECS_camilla_RC_BA_linear_\d{8}-\d{6}\/camilladsp$/);
    expect(events.at(-1)).toMatchObject({ result: { kind: "export", data: { folder, files: expect.arrayContaining([
      expect.stringMatching(/camilladsp\/configs\/secs_linear\.yml$/),
      expect.stringMatching(/camilladsp\/configs\/secs_linear_bypass\.yml$/),
      expect.stringMatching(/camilladsp\/coeffs\/secs_linear_L_fir\.wav$/),
      expect.stringMatching(/camilladsp\/secs-design\.json$/),
    ]), stale: true } } });
    mock.failJobs("measure", "device_unavailable");
    await mock.request({ method: "measure_position", params: { position: 7 } }); mock.drain();
    expect(events.at(-1)).toMatchObject({ status: "failed", error: { code: "device_unavailable" } });
    expect((await mock.request({ method: "project_info" })).positions).not.toContain(7);
  });

  it("exports a newly calibrated stereo project as named convolution pairs through job events", async () => {
    const mock = createMockService({ tickMs: 0 });
    const events: JobEvent[] = [];
    const off = mock.events.subscribe((event) => events.push(event));
    try {
      expect((await mock.request({ method: "bootstrap" })).upstream.repository).toBe("https://github.com/Kim-O-O/Sound-Equalization-and-Correction-System-2");
      await mock.request(newProject(false));
      await mock.request({ method: "sample_load", params: { positions: 1 } }); mock.drain();
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
      await mock.request({ method: "export_folder_set", params: { folder: "/exports" } });
      const { job } = await mock.request({ method: "export", params: { kind: "convolution", folder: "", design_name: "Stereo" } });
      expect(events.at(-1)).toMatchObject({ event: "started", job, kind: "export" });
      mock.drain();
      const folder = "/exports/SECS_convolution_Stereo";
      expect(events.at(-1)).toMatchObject({ event: "finished", job, status: "succeeded", result: { kind: "export", data: { folder, stale: false, files: [
        `${folder}/L_to_L.wav`, `${folder}/L_to_L.csv`, `${folder}/R_to_R.wav`, `${folder}/R_to_R.csv`,
        `${folder}/컨볼루션 적용 방법.txt`, `${folder}/routing.json`, `${folder}/secs-design.json`,
      ] } } });
    } finally { off(); mock.dispose(); }
  });

  it("applies every SettingEdit variant without a numerical engine", async () => {
    const mock = createMockService({ tickMs: 0 });
    const edits = {
      mode: { kind: "mode", mode: "ultra" }, ultra_ms: { kind: "ultra_ms", ms: 999 }, rc_lo: { kind: "rc_lo", hz: 25 }, rc_hi: { kind: "rc_hi", hz: 500 },
      rc_bands: { kind: "rc_bands", bands: [{ lo_hz: 15000, hi_hz: 20000 }, { lo_hz: 2000, hi_hz: 4000 }] },
      rc_hi_basis: { kind: "rc_hi_basis", basis: "lenient" }, rc_hi_transition: { kind: "rc_hi_transition", factor: 4 },
      sample_rate_policy: { kind: "sample_rate_policy", policy: "resample" }, low_protect: { kind: "low_protect", on: true },
      lf_align: { kind: "lf_align", on: true }, mode_decay: { kind: "mode_decay", on: true },
      room_volume: { kind: "room_volume", m3: 60 }, room_dims: { kind: "room_dims", dims: [5, 4, 3] }, boost_max: { kind: "boost_max", db: 6 },
      xo: { kind: "xo", group: "front", hz: 100 }, xo_peq: { kind: "xo_peq", on: false, lo: 30, hi: 150 },
      infra: { kind: "infra", role: "SW1", hz: 40, type: "LR24", subs: ["SW1", "SW2"] }, infra_boost: { kind: "infra_boost", db: 18 },
      sub_mode: { kind: "sub_mode", role: "SW1", mode: "bundle:A" }, sub_layout: { kind: "sub_layout", bundle: "A", layout: "dba" },
      rear_trim: { kind: "rear_trim", bundle: "A", ms: 3 }, sub: { kind: "sub", role: "SW1", patch: { gain_db: -4 } },
      sub_band_type: { kind: "sub_band_type", role: "SW1", band: "hp", type: "off" }, main_delay: { kind: "main_delay", ms: 2 },
      mimo_on: { kind: "mimo_on", on: true }, fsiso: { kind: "fsiso", hz: 100 }, mimo_low: { kind: "mimo_low", hz: 20 },
      mimo_max: { kind: "mimo_max", count: 4 }, mimo_fir: { kind: "mimo_fir", on: true },
      analytic_derivatives: { kind: "analytic_derivatives", peq: true, mimo_iir: true },
    } satisfies { [K in SettingEdit["kind"]]: Extract<SettingEdit, { kind: K }> };
    for (const edit of Object.values(edits)) await mock.request({ method: "calibration_settings_edit", params: { edit } });
    // The service takes only the transition factors 1, 2 and 4 (ADR 0021).
    await expect(mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "rc_hi_transition", factor: 3 } } })).rejects.toMatchObject({ error: { code: "invalid_request", params: { field: "rc_hi_transition" } } });
    expect(await mock.request({ method: "calibration_settings_get" })).toMatchObject({ mode: "ultra", sample_rate_policy: "resample", low_protect: true, lf_align: true, mode_decay: true, ultra_ms: CALIBRATION_LIMITS.ultra_ms[1], rc_hi: 500, rc_hi_basis: "lenient", rc_hi_transition: 4, rc_bands: [{ lo_hz: 2000, hi_hz: 4000 }, { lo_hz: 15000, hi_hz: 20000 }], room_volume_m3: 60, room_dims_m: [5, 4, 3], mimo_on: true, mimo_fir: true, peq_analytic_derivatives: true, mimo_iir_analytic_derivatives: true, subs_manual: { SW1: { mode: "bundle", gain_db: -4, hp_hz: 0 } } });
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
      mic_calibration_get: { method: "mic_calibration_get" }, mic_calibration_set: { method: "mic_calibration_set", params: { path: "/mic.txt" } },
      mic_calibration_clear: { method: "mic_calibration_clear" }, mic_calibration_attach: { method: "mic_calibration_attach" },
      measurement_records: { method: "measurement_records" }, levels_get: { method: "levels_get" }, levels_fill_defaults: { method: "levels_fill_defaults" }, level_stop: { method: "level_stop" },
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
      local_api_status: { method: "local_api_status" }, local_api_token_regenerate: { method: "local_api_token_regenerate" },
      update_status: { method: "update_status" }, update_check: { method: "update_check" }, update_download: { method: "update_download" },
      update_restart: { method: "update_restart" }, update_notice_dismiss: { method: "update_notice_dismiss" },
      update_open_release: { method: "update_open_release", params: { version: "0.36.0" } },
      app_settings_get: { method: "app_settings_get" }, app_settings_set: { method: "app_settings_set", params: { settings: fx.appSettings } },
      apply_status: { method: "apply_status" }, apply_settings_set: { method: "apply_settings_set", params: fx.appSettings.apply },
      apply_camilla: { method: "apply_camilla", params: {} }, apply_eqapo: { method: "apply_eqapo" }, revert_eqapo: { method: "revert_eqapo" },
      host_install: { method: "host_install" }, host_uninstall: { method: "host_uninstall" }, host_stop_managing: { method: "host_stop_managing" },
      measurement_curves: { method: "measurement_curves", params: { positions: null, roles: null } }, result_curves: { method: "result_curves", params: { tab: "spl", channel: null, group: null, xo_hz: null, corrected: null } },
      result_lines: { method: "result_lines" }, sub_take_values: { method: "sub_take_values" }, dialog_open_project: { method: "dialog_open_project" }, dialog_save_project: { method: "dialog_save_project", params: { suggested: "new.secs" } },
      dialog_pick_mdat: { method: "dialog_pick_mdat" }, dialog_pick_folder: { method: "dialog_pick_folder", params: { start: null } }, dialog_save_file: { method: "dialog_save_file", params: { kind: "target", suggested: "target.json" } },
      dialog_open_file: { method: "dialog_open_file", params: { kind: "target" } }, open_folder: { method: "open_folder", params: { path: "/exports" } },
      level_monitor_start: { method: "level_monitor_start", params: { role: null } }, level_monitor_stop: { method: "level_monitor_stop" }, level_play: { method: "level_play", params: { role: "L", master_dbfs: -20 } },
      status_watch_start: { method: "status_watch_start" }, status_watch_stop: { method: "status_watch_stop" }, export_folder_get: { method: "export_folder_get" }, export_folder_set: { method: "export_folder_set", params: { folder: "/exports" } },
    } satisfies { [K in Request["method"]]: Extract<Request, { method: K }> };
    for (const request of Object.values(requests)) {
      const mock = createMockService({ tickMs: 0, results: fx.resultsSummary });
      if (request.method === "update_download") mock.setUpdateStatus(updateStates.available);
      if (request.method === "update_restart") mock.setUpdateStatus(updateStates.readyOnExit);
      if (request.method === "mic_calibration_attach") await mock.request({ method: "mic_calibration_set", params: { path: "/mic.txt" } });
      if (request.method === "undo") await mock.request({ method: "position_clear", params: { position: 1 } });
      if (request.method === "job_cancel") await mock.request({ method: "sample_load", params: { positions: 1 } });
      // 적용 needs its target set to 적용, and Equalizer APO an install.
      if (request.method === "apply_camilla" || request.method === "apply_eqapo" || request.method === "revert_eqapo") {
        await mock.request({ method: "apply_settings_set", params: { camilla: { level: "apply", address: "127.0.0.1:1234" }, eqapo: { level: "apply" } } });
        mock.setApplyStatus({ ...fx.applyStatus, eqapo: fx.eqapoXt });
      }
      // 관리 끄기 needs a service to reach.
      if (request.method === "host_stop_managing") mock.setApplyStatus({ ...fx.applyStatus, host: "running", host_status: fx.hostManaging });
      expect(await mock.invoke("secs_call", { request }), request.method).toMatchObject({ ok: true, data: { method: request.method } });
      mock.drain(); mock.dispose();
    }
  });

  it("scripts updater states and guards downloads and restarts", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      for (const state of Object.values(updateStates)) {
        mock.setUpdateStatus(state);
        expect(await mock.request({ method: "update_status" })).toEqual(state);
      }
      for (const state of [updateStates.upToDate, updateStates.portable, updateStates.failed, updateStates.downloading]) {
        mock.setUpdateStatus(state);
        await expect(mock.request({ method: "update_download" })).rejects.toMatchObject({ error: { code: "invalid_request" } });
        await expect(mock.request({ method: "update_restart" })).rejects.toMatchObject({ error: { code: "invalid_request" } });
      }
      mock.setUpdateStatus(updateStates.available);
      expect(await mock.request({ method: "update_download" })).toMatchObject({ phase: { kind: "downloading", received: 0 } });
      for (const state of [updateStates.readyOnExit, updateStates.readyOnRequest]) {
        mock.setUpdateStatus(state);
        expect(await mock.request({ method: "update_restart" })).toBeNull();
        expect(await mock.request({ method: "update_status" })).toMatchObject({ phase: { kind: "installing" } });
      }
      mock.setUpdateStatus(updateStates.skippedNotice);
      expect(await mock.request({ method: "update_notice_dismiss" })).toMatchObject({ notice: null });
      await mock.request({ method: "app_settings_set", params: { settings: { ...fx.appSettings, updates: "off" } } });
      expect(await mock.request({ method: "update_status" })).toMatchObject({ policy: "off", phase: { kind: "off" } });
      expect(await mock.request({ method: "update_check" })).toMatchObject({ policy: "off", phase: { kind: "checking" } });
      mock.setUpdateStatus(updateStates.available);
      await mock.request({ method: "app_settings_set", params: { settings: fx.appSettings } });
      expect(await mock.request({ method: "update_status" })).toMatchObject({ policy: "auto", phase: { kind: "downloading" } });
    } finally { mock.dispose(); }
  });

  it("reads no CamillaDSP and no Equalizer APO until a test or specimen flips the status", async () => {
    const mock = createMockService({ tickMs: 0 });
    expect(await mock.request({ method: "apply_status" })).toEqual(fx.applyStatus);
    expect(fx.applyStatus).toMatchObject({ camilla: { reachable: false, error: { code: "apply_failed" }, status: null, design: { kind: "unknown" } }, eqapo: { installed: false } });
    mock.setApplyStatus({ ...fx.applyStatus, eqapo: fx.eqapoXt });
    expect((await mock.request({ method: "apply_status" })).eqapo).toEqual(fx.eqapoXt);
    // The answer is a copy: changing it does not change the next one.
    (await mock.request({ method: "apply_status" })).eqapo.include = "damaged";
    expect((await mock.request({ method: "apply_status" })).eqapo.include).toBe("absent");
    expect((await createMockService({ tickMs: 0, applyStatus: { ...fx.applyStatus, eqapo: fx.eqapoXt } }).request({ method: "apply_status" })).eqapo.installed).toBe(true);
  });

  it("reports the current design as soon as a result exists, without an apply job", async () => {
    const mock = createMockService({ tickMs: 0 });
    expect((await mock.request({ method: "apply_status" })).current_design_id).toBeNull();
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
    expect((await mock.request({ method: "apply_status" })).current_design_id).toBe(fx.MOCK_DESIGN_ID);
    await mock.request({ method: "target_apply_preset", params: { preset: "flat" } });
    expect((await mock.request({ method: "apply_status" })).current_design_id).toBe(fx.MOCK_DESIGN_ID);
    await mock.request(newProject(false));
    expect((await mock.request({ method: "apply_status" })).current_design_id).toBeNull();
    mock.dispose();
    const reopened = createMockService({ tickMs: 0, results: { ...fx.resultsSummary, calibration: { ...fx.resultsSummary.calibration!, cached: true } } });
    expect((await reopened.request({ method: "apply_status" })).current_design_id).toBe(fx.MOCK_DESIGN_ID);
    reopened.dispose();
  });

  it("stores and echoes ApplySettings, refusing an address the service refuses", async () => {
    const mock = createMockService({ tickMs: 0 });
    const settings = { camilla: { level: "apply", address: "192.168.0.40:1234" }, eqapo: { level: "files" } } as const;
    expect(await mock.request({ method: "apply_settings_set", params: settings })).toEqual(settings);
    expect((await mock.request({ method: "app_settings_get" })).apply).toEqual(settings);
    for (const address of ["localhost", ":1234", "host:0", "host:no", "a:b:1", " 127.0.0.1:1234"]) {
      await expect(mock.request({ method: "apply_settings_set", params: { ...settings, camilla: { level: "apply", address } } }), address)
        .rejects.toMatchObject({ error: { code: "invalid_request", params: { field: "camilla.address" } } });
    }
    expect((await mock.request({ method: "app_settings_get" })).apply).toEqual(settings);
  });

  it("applies through `apply` jobs that finish with an Applied result, one export-group job at a time", async () => {
    const mock = createMockService({ tickMs: 0, results: fx.resultsSummary });
    const events: JobEvent[] = []; mock.events.subscribe((event) => events.push(event));
    // 파일만 refuses before any job, naming the target.
    await expect(mock.request({ method: "apply_camilla", params: {} })).rejects.toMatchObject({ error: { code: "unsupported", params: { target: "camilladsp" } } });
    await mock.request({ method: "apply_settings_set", params: { camilla: { level: "apply", address: "127.0.0.1:1234" }, eqapo: { level: "apply" } } });
    await expect(mock.request({ method: "apply_eqapo" })).rejects.toMatchObject({ error: { code: "unsupported", params: { target: "equalizer_apo" } } });
    await expect(mock.request({ method: "revert_eqapo" })).rejects.toMatchObject({ error: { code: "unsupported" } });

    const camilla = await mock.request({ method: "apply_camilla", params: {} });
    expect(camilla.kind).toBe("apply");
    // Export and apply share the export group.
    await expect(mock.request({ method: "export", params: { kind: "camilla_dsp", folder: "" } })).rejects.toMatchObject({ error: { code: "job_busy" } });
    mock.drain();
    expect(events.find((event) => event.event === "finished" && event.job === camilla.job)).toMatchObject({
      status: "succeeded", result: { kind: "apply", data: { outcome: { kind: "applied" }, design_id: fx.MOCK_DESIGN_ID, folder: expect.stringContaining("applied/camilladsp") } },
    });

    mock.setApplyStatus({ ...fx.applyStatus, eqapo: fx.eqapoXt });
    const eqapo = await mock.request({ method: "apply_eqapo" });
    await expect(mock.request({ method: "apply_camilla", params: {} })).rejects.toMatchObject({ error: { code: "job_busy" } });
    mock.drain();
    expect(events.find((event) => event.event === "finished" && event.job === eqapo.job)).toMatchObject({
      status: "succeeded", result: { kind: "apply", data: { outcome: { kind: "applied" }, folder: `${fx.eqapoXt.config_path}/SECS2R` } },
    });
    expect((await mock.request({ method: "apply_status" })).eqapo).toMatchObject({ include: "present", applied_design_id: fx.MOCK_DESIGN_ID });
    // 빼기 takes the lines out; the SECS2R folder and its design stay.
    expect(await mock.request({ method: "revert_eqapo" })).toBeNull();
    expect((await mock.request({ method: "apply_status" })).eqapo).toMatchObject({ include: "absent", applied_design_id: fx.MOCK_DESIGN_ID });
    // A failing apply job carries its error like an export.
    mock.failJobs("apply", "permission_denied", "Access is denied. (os error 5)", { path: "C:/Program Files/EqualizerAPO/config/SECS2R" });
    const denied = await mock.request({ method: "apply_eqapo" });
    mock.drain();
    expect(events.find((event) => event.event === "finished" && event.job === denied.job)).toMatchObject({ status: "failed", error: { code: "permission_denied", params: { path: "C:/Program Files/EqualizerAPO/config/SECS2R" } } });
  });

  it("installs, hands over, stops managing and removes the SECS 2R Host service as the service does (관리)", async () => {
    const mock = createMockService({ tickMs: 0, results: fx.resultsSummary });
    const events: JobEvent[] = []; mock.events.subscribe((event) => events.push(event));
    const status = () => mock.request({ method: "apply_status" });
    expect((await status()).host).toBe("not_installed");
    // 관리 끄기 cannot reach a service that is not there.
    await expect(mock.request({ method: "host_stop_managing" })).rejects.toMatchObject({ error: { code: "apply_failed" } });
    // 관리 without camilladsp.exe is refused before any job, naming the setting.
    await mock.request({ method: "apply_settings_set", params: { camilla: { level: "manage", address: "127.0.0.1:1300" }, eqapo: { level: "manage" } } });
    await expect(mock.request({ method: "apply_camilla", params: {} })).rejects.toMatchObject({ error: { code: "invalid_request", params: { field: "apply.camilla.exe_path" } } });
    // With it but no service, 적용 runs and the hand-over fails the job.
    await mock.request({ method: "apply_settings_set", params: { camilla: { level: "manage", address: "127.0.0.1:1300", exe_path: fx.CAMILLA_EXE }, eqapo: { level: "manage" } } });
    const refused = await mock.request({ method: "apply_camilla", params: {} });
    mock.drain();
    expect(events.find((event) => event.event === "finished" && event.job === refused.job)).toMatchObject({ status: "failed", error: { code: "apply_failed" } });
    // Installing starts the service, managing nothing yet.
    expect(await mock.request({ method: "host_install" })).toBeNull();
    expect(await status()).toMatchObject({ host: "running", host_status: fx.hostIdle });
    const camilla = await mock.request({ method: "apply_camilla", params: {} });
    mock.drain();
    // No CamillaDSP answers: the design is handed over, and the service starts CamillaDSP with it.
    expect(events.find((event) => event.event === "finished" && event.job === camilla.job)).toMatchObject({ status: "succeeded", result: { kind: "apply", data: { outcome: { kind: "handed_over" } } } });
    expect((await status()).host_status).toMatchObject({ managed: { camilla: { exe_path: fx.CAMILLA_EXE, port: 1300, design_id: fx.MOCK_DESIGN_ID } }, camilla: { state: { kind: "running" }, pid: 4812 } });
    mock.setApplyStatus({ ...(await status()), eqapo: fx.eqapoXt });
    await mock.request({ method: "apply_eqapo" });
    mock.drain();
    expect((await status()).host_status).toMatchObject({ managed: { camilla: { exe_path: fx.CAMILLA_EXE }, eqapo: { config_path: fx.eqapoXt.config_path, design_id: fx.MOCK_DESIGN_ID } }, eqapo_include: "present" });
    // 관리 끄기 lets go of both targets; the service keeps running.
    expect(await mock.request({ method: "host_stop_managing" })).toBeNull();
    expect(await status()).toMatchObject({ host: "running", host_status: { managed: {}, camilla: { state: { kind: "stopped" }, pid: null } } });
    // Removing it forgets its status.
    expect(await mock.request({ method: "host_uninstall" })).toBeNull();
    const removed = await status();
    expect(removed.host).toBe("not_installed");
    expect(removed.host_status).toBeUndefined();
    // The open-file dialog picks camilladsp.exe for 관리.
    expect(await mock.request({ method: "dialog_open_file", params: { kind: CAMILLA_EXE_FILE } })).toBe(fx.CAMILLA_EXE);
    mock.dispose();
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
