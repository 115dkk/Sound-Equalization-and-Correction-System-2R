import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { IpcFailure } from "./client";
import type { JobEvent } from "./bindings/JobEvent";
import { createMockService } from "./mock";
import { calibrationSettings, gpuComputeStatus, modeLabel, projectInfo, resultsSummary } from "./fixtures";
import type { ExportKind } from "./bindings/ExportKind";
import type { Mode } from "./bindings/Mode";
import type { MimoExportDecision } from "./bindings/MimoExportDecision";
import { umikFile } from "../screens/connect/fixtures";

describe("mock mic calibration", () => {
  it("selects the fixture with the chosen filename, keeps old attachments and isolates instances", async () => {
    const mock = createMockService({ tickMs: 0, results: resultsSummary });
    const other = createMockService({ tickMs: 0 });
    try {
      const before = await mock.request({ method: "mic_calibration_get" });
      const records = (await mock.request({ method: "measurement_records" })).records;
      const count = records.reduce((total, record) => total + record.roles.length, 0);
      expect(before).toEqual({ file: null, attached: 0, attachable: count });
      const path = "D:\\Microphones\\my-mic.cal";
      const chosen = await mock.request({ method: "mic_calibration_set", params: { path } });
      expect(chosen).toEqual({ ...before, file: { ...umikFile, path, name: "my-mic.cal" } });
      expect((await mock.request({ method: "project_info" })).dirty).toBe(true);
      expect((await mock.request({ method: "results" })).calibration?.stale).toBe(false);
      chosen.file!.notes.push("do not mutate the fixture");
      await mock.request({ method: "project_save", params: { path: "/saved.secs" } });
      expect(await mock.request({ method: "mic_calibration_attach" })).toMatchObject({ attached: count, attachable: 0 });
      expect((await mock.request({ method: "results" })).calibration).toMatchObject({ stale: true, stale_reason: "measurements" });
      expect((await mock.request({ method: "project_info" })).dirty).toBe(false);
      expect(await mock.request({ method: "mic_calibration_clear" })).toEqual({ file: null, attached: count, attachable: 0 });
      expect((await mock.request({ method: "project_info" })).dirty).toBe(true);
      const replacement = await mock.request({ method: "mic_calibration_set", params: { path: "/replacement.txt" } });
      expect(replacement).toMatchObject({ file: { name: "replacement.txt", notes: umikFile.notes }, attached: count, attachable: 0 });
      expect(await other.request({ method: "mic_calibration_get" })).toEqual(before);
      expect(umikFile.notes).not.toContain("do not mutate the fixture");
    } finally { mock.dispose(); other.dispose(); }
  });

  it("supports the new dialog kind, queued cancellation and the existing target kind", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      expect(await mock.request({ method: "dialog_open_file", params: { kind: "mic_calibration" } })).toBe("C:/Users/demo/Documents/UMIK-1/7000001_90deg.txt");
      mock.queueDialog("dialog_open_file", null);
      expect(await mock.request({ method: "dialog_open_file", params: { kind: "mic_calibration" } })).toBeNull();
      expect(await mock.request({ method: "dialog_open_file", params: { kind: "target" } })).toBe("C:/Users/demo/Documents/SECS/target.json");
    } finally { mock.dispose(); }
  });

  it("requires an open project and a selected file, and refuses edits while measuring", async () => {
    const closed = createMockService({ tickMs: 0, project: null });
    const mock = createMockService({ tickMs: 0 });
    try {
      for (const request of [{ method: "mic_calibration_get" }, { method: "mic_calibration_set", params: { path: "/mic.txt" } }, { method: "mic_calibration_clear" }, { method: "mic_calibration_attach" }] as const) {
        await expect(closed.request(request)).rejects.toMatchObject({ error: { code: "project_not_open" } });
      }
      await expect(mock.request({ method: "mic_calibration_attach" })).rejects.toMatchObject({ error: { code: "measurement_input", params: { reason: "mic_calibration", issue: "no_file" } } });
      await mock.request({ method: "measure_position", params: { position: 1 } });
      for (const request of [{ method: "mic_calibration_set", params: { path: "/mic.txt" } }, { method: "mic_calibration_clear" }, { method: "mic_calibration_attach" }] as const) {
        await expect(mock.request(request)).rejects.toMatchObject({ error: { code: "job_busy" } });
      }
      expect((await mock.request({ method: "mic_calibration_get" })).file).toBeNull();
    } finally { closed.dispose(); mock.dispose(); }
  });
});

describe("mock MIMO export decisions", () => {
  it.each([
    ["off", false, 2, false, true, 0],
    ["too_few_positions", true, 2, false, true, 0],
    ["none", true, 3, false, true, 0],
    ["stale", true, 3, true, true, 0],
    ["no_supports", true, 3, true, false, 0],
    ["included", true, 3, true, false, 2],
  ] satisfies [MimoExportDecision, boolean, number, boolean, boolean, number][])("returns %s in service precedence order", async (decision, on, positions, computed, stale, supports) => {
    const mock = createMockService({ tickMs: 0, project: { ...projectInfo, positions: Array.from({ length: positions }, (_, i) => i + 1) } });
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on } } });
    mock.setResults({
      ...resultsSummary,
      calibration: { ...resultsSummary.calibration!, sub_roles: [] },
      mimo: computed ? { ...resultsSummary.mimo!, stale, support_count: supports } : null,
    });
    expect((await mock.request({ method: "results" })).mimo_export).toBe(decision);
    if (decision === "no_supports") {
      await expect(mock.request({ method: "export", params: { kind: "filter_text", folder: "" } })).resolves.toMatchObject({ kind: "export" });
      mock.drain();
    }
    mock.dispose();
  });

  it.each([false, true])("uses run positions for live results and project positions for cached results (cached: %s)", async (cached) => {
    const mock = createMockService({ tickMs: 0, project: { ...projectInfo, positions: [1, 2, 3] } });
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
    mock.setResults({ ...resultsSummary, calibration: { ...resultsSummary.calibration!, cached } });
    await mock.request({ method: "position_clear", params: { position: 3 } });
    expect((await mock.request({ method: "results" })).mimo_export).toBe(cached ? "too_few_positions" : "stale");
    mock.dispose();
  });

  it("refreshes the decision after calibration, MIMO, compute changes and project replacement", async () => {
    const mock = createMockService({ tickMs: 0 });
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain();
    expect((await mock.request({ method: "results" })).mimo_export).toBe("none");
    await mock.request({ method: "mimo", params: { settings: null } });
    mock.drain();
    expect((await mock.request({ method: "results" })).mimo_export).toBe("included");
    const settings = await mock.request({ method: "app_settings_get" });
    await mock.request({ method: "app_settings_set", params: { settings: { ...settings, compute: "cpu" } } });
    expect((await mock.request({ method: "results" })).mimo_export).toBe("stale");
    await mock.request({ method: "project_open", params: { path: "C:/another.secs" } });
    expect((await mock.request({ method: "results" })).mimo_export).toBe("off");
    mock.dispose();
  });
});

describe("mock filter exports", () => {
  afterEach(() => vi.restoreAllMocks());

  function resultWithoutSubs() {
    const results = structuredClone(resultsSummary);
    results.calibration!.sub_roles = [];
    results.calibration!.main_roles = results.calibration!.main_roles.slice(0, 2);
    results.mimo = null;
    return results;
  }

  async function exported(mock: ReturnType<typeof createMockService>, kind: ExportKind, folder = "") {
    const events: JobEvent[] = [];
    const unsubscribe = mock.events.subscribe((event) => events.push(event));
    await mock.request({ method: "export", params: { kind, folder } });
    mock.drain();
    unsubscribe();
    const last = events.at(-1);
    if (last?.event !== "finished" || last.result?.kind !== "export") throw new Error("Missing export result");
    return last.result.data;
  }

  it("rejects text for result subs before MIMO, without starting a job", async () => {
    const mock = createMockService({ tickMs: 0, project: { ...projectInfo, sub_count: 0 }, results: resultsSummary });
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
    mock.setResults(resultsSummary);
    const events: JobEvent[] = [];
    mock.events.subscribe((event) => events.push(event));
    await expect(mock.request({ method: "export", params: { kind: "filter_text", folder: "" } })).rejects.toMatchObject({ error: { code: "export_scope", params: { reason: "subs" } } });
    expect(events).toEqual([]);
    expect(await mock.request({ method: "jobs" })).toEqual([]);
    mock.dispose();
  });

  it.each(["filter_text"] as const)("rejects %s when current MIMO is included, without starting a job", async (kind) => {
    const results = resultWithoutSubs();
    results.mimo = structuredClone(resultsSummary.mimo);
    const mock = createMockService({ tickMs: 0, results });
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
    mock.setResults(results);
    const events: JobEvent[] = [];
    mock.events.subscribe((event) => events.push(event));
    await expect(mock.request({ method: "export", params: { kind, folder: "" } })).rejects.toMatchObject({ error: { code: "export_scope", params: { reason: "mimo" } } });
    expect(events).toEqual([]);
    expect(await mock.request({ method: "jobs" })).toEqual([]);
    mock.dispose();
  });

  it.each(["off", "positions", "none", "stale"] as const)("does not block either format when MIMO is excluded: %s", async (reason) => {
    const results = resultWithoutSubs();
    results.mimo = reason === "none" ? null : { ...structuredClone(resultsSummary.mimo!), stale: reason === "stale" };
    const mock = createMockService({ tickMs: 0, project: { ...projectInfo, positions: reason === "positions" ? [1, 2] : [1, 2, 3] } });
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: reason !== "off" } } });
    mock.setResults(results);
    for (const kind of ["filter_text", "equalizer_apo"] as const) expect((await exported(mock, kind, "C:/exports")).folder).toBe("C:/exports");
    mock.dispose();
  });

  it.each(["filter_text", "equalizer_apo"] as const)("preserves tutorial and missing-result errors for %s", async (kind) => {
    const mock = createMockService({ tickMs: 0, project: { ...projectInfo, tutorial: true } });
    await expect(mock.request({ method: "export", params: { kind, folder: "" } })).rejects.toMatchObject({ error: { code: "tutorial_blocked" } });
    mock.dispose();
    const empty = createMockService({ tickMs: 0 });
    await expect(empty.request({ method: "export", params: { kind, folder: "" } })).rejects.toMatchObject({ error: { code: "engine_input" } });
    expect(await empty.request({ method: "jobs" })).toEqual([]);
    empty.dispose();
  });

  it.each([
    ["zero", "zero", "최소 지연 모드"],
    ["mixed", "mixed", "저지연 모드"],
    ["linear", "linear", "표준 모드"],
    ["ultra", "ultra200", "초고지연 모드 200 ms"],
  ] as const)("uses service folders and ordered file lists in %s mode", async (mode: Mode, fileMode, label) => {
    vi.spyOn(Date, "now").mockReturnValue(new Date(2026, 9, 15, 9, 41, 7).getTime());
    const mock = createMockService({ tickMs: 0, results: resultWithoutSubs() });
    await mock.request({ method: "calibration_settings_set", params: { settings: { ...calibrationSettings, mode, ultra_ms: 200, mimo_on: false } } });
    // The service names the folder after the result's mode.
    const results = resultWithoutSubs();
    results.calibration!.mode = mode;
    mock.setResults(results);
    await mock.request({ method: "export_folder_set", params: { folder: "C:/exports/" } });
    for (const kind of ["filter_text", "equalizer_apo"] as const) {
      const folder = `C:/exports/SECS_${kind === "filter_text" ? "text" : "eqapo"}_RC_BA_${mode}_20261015-094107`;
      const prefix = `secs_${fileMode}`;
      const firFolder = kind === "filter_text" ? folder : `${folder}/${prefix}`;
      const files = kind === "filter_text" ? [`${folder}/SECS 필터 ${label}.txt`] : [
        `${folder}/config.txt`, `${folder}/${prefix}/${prefix}.txt`, `${folder}/EqAPO 적용 방법.txt`,
      ];
      if (mode !== "zero") files.push(`${firFolder}/${prefix}_L_fir.wav`, `${firFolder}/${prefix}_R_fir.wav`);
      expect(await exported(mock, kind)).toEqual({ folder, files, stale: false });
    }
    mock.dispose();
  });

  it("exports current MIMO through Equalizer APO", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
      mock.setResults(resultsSummary);
      const result = await exported(mock, "equalizer_apo");
      expect(result.folder).toContain("RC_BA_MIMO");
      expect(result.files).toContain(`${result.folder}/config.txt`);
    } finally { mock.dispose(); }
  });

  it("allows BA in EqAPO and writes directly into an explicit folder", async () => {
    const results = structuredClone(resultsSummary);
    results.mimo = null;
    const mock = createMockService({ tickMs: 0, results });
    const folder = "C:/exports/chosen";
    expect(await exported(mock, "equalizer_apo", folder)).toEqual({ folder, stale: false, files: [
      `${folder}/config.txt`, `${folder}/secs_mixed/secs_mixed.txt`, `${folder}/EqAPO 적용 방법.txt`,
      ...results.calibration!.main_roles.map((role) => `${folder}/secs_mixed/secs_mixed_${role}_fir.wav`),
    ] });
    mock.dispose();
  });

  it("matches the service's clamped, six-significant-digit ultra labels", () => {
    expect(modeLabel("ultra", 20)).toBe("초고지연 모드 30 ms");
    expect(modeLabel("ultra", 123.456789)).toBe("초고지연 모드 123.457 ms");
    expect(modeLabel("ultra", 250)).toBe("초고지연 모드 200 ms");
  });
});

describe("mock service", () => {
  it("preserves full results when only the subs are calibrated", async () => {
    const mock = createMockService({ tickMs: 0, results: resultsSummary });
    try {
      const before = await mock.request({ method: "results" });
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: true } });
      mock.drain();
      expect(await mock.request({ method: "results" })).toEqual(before);
      expect((await mock.request({ method: "result_lines" })).ba?.subs_only).toBe(true);
    } finally { mock.dispose(); }
  });
  it("delivers compact summaries and metadata without DSP payloads in job notifications", async () => {
    const mock = createMockService({ tickMs: 0 });
    const events: JobEvent[] = [];
    const off = mock.events.subscribe((event) => events.push(event));
    try {
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
      expect((await mock.request({ method: "jobs" }))[0]).not.toHaveProperty("result");
      mock.drain();
      expect(events.at(-1)).toMatchObject({ result: { kind: "calibration" } });
      expect(events.at(-1)).not.toHaveProperty("result.data");
      const calibration = (await mock.request({ method: "results" })).calibration!;
      expect(calibration).not.toHaveProperty("chain");
      expect(calibration.main_roles).toEqual(calibration.scorecard.map((score) => score.role));
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_fir", on: true } } });
      await mock.request({ method: "mimo", params: { settings: null } }); mock.drain();
      expect(events.at(-1)).toMatchObject({ result: { kind: "mimo" } });
      expect(events.at(-1)).not.toHaveProperty("result.data");
      const mimo = (await mock.request({ method: "results" })).mimo!;
      expect(mimo).not.toHaveProperty("paths");
      expect(mimo.input_count).toBe(calibration.main_roles.length);
      expect(mimo.fir_taps_total).toBe(mimo.support_count * 3);
    } finally { off(); mock.dispose(); }
  });

  it("preserves GPU preference while IIR uses CPU and FIR uses GPU", async () => {
    const mock = createMockService({ tickMs: 0, computeStatus: gpuComputeStatus, results: resultsSummary });
    try {
      const settings = await mock.request({ method: "app_settings_get" });
      await mock.request({ method: "app_settings_set", params: { settings: { ...settings, compute: "gpu" } } });
      await mock.request({ method: "mimo", params: { settings: null } }); mock.drain();
      expect((await mock.request({ method: "results" })).mimo?.compute).toMatchObject({
        requested: "gpu", backend: "cpu", stage: null, reason: "iir_reference", accelerated_systems: 0,
      });
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_fir", on: true } } });
      await mock.request({ method: "mimo", params: { settings: null } }); mock.drain();
      expect((await mock.request({ method: "results" })).mimo?.compute).toMatchObject({
        requested: "gpu", backend: "cuda", stage: "ideal_fir_sweep", reason: "user_gpu",
      });
      expect((await mock.request({ method: "app_settings_get" })).compute).toBe("gpu");
    } finally { mock.dispose(); }
  });

  it("includes both roles for each conflicting output like config validation", async () => {
    const mock = createMockService({ tickMs: 0 });
    for (const index of [1, 2]) await mock.request({ method: "config_set_speaker", params: { index, role: null, size: null, band_index: 0, out: 1, pol: null } });
    const notices = await mock.request({ method: "config_validate" });
    expect(notices.filter((notice) => notice.code === "out_duplicate")).toEqual([
      { code: "out_duplicate", params: { out: "1", first_role: "L", role: "R" } },
      { code: "out_duplicate", params: { out: "1", first_role: "L", role: "C" } },
    ]);
    mock.dispose();
  });

  it("stores measured input levels rather than output levels", async () => {
    const mock = createMockService({ tickMs: 0 });
    const events: JobEvent[] = [];
    mock.events.subscribe((event) => events.push(event));
    await mock.request({ method: "level_monitor_start", params: { role: null } });
    await mock.request({ method: "level_play", params: { role: "L", master_dbfs: -20 } });
    for (let i = 0; i < 5; i++) mock.tick();
    const meter = events.find((event) => event.event === "meter");
    expect(meter).toMatchObject({ rms_dbfs: -30 });
    expect((await mock.request({ method: "levels_get" })).levels.L).toBe(-30);
    mock.dispose();
  });

  it("gives every BA sub a distinct deterministic curve before and after correction", async () => {
    const mock = createMockService({ tickMs: 0, results: resultsSummary });
    await mock.request({ method: "config_set_sub_count", params: { count: 3, confirm: true } });
    await mock.request({ method: "sample_load", params: { positions: 1 } });
    mock.drain();
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain();
    const request = { method: "result_curves", params: { tab: "ba", channel: null, group: null, xo_hz: null, corrected: null } } as const;
    const first = await mock.request(request);
    expect(await mock.request(request)).toEqual(first);
    for (const kind of ["before", "after"]) {
      const curves = first.curves.filter((curve) => curve.kind === kind && curve.role !== null);
      expect(curves.map((curve) => curve.role)).toEqual(["SW1", "SW2", "SW3"]);
      expect(new Set(curves.map((curve) => JSON.stringify(curve.values))).size).toBe(curves.length);
      expect(curves.every((curve) => curve.values.length === first.grid.length && curve.values.every(Number.isFinite))).toBe(true);
    }
    mock.dispose();
  });


  it("answers through the client contract with typed fixtures", async () => {
    const mock = createMockService({ tickMs: 0 });
    const info = await mock.request({ method: "bootstrap" });
    expect(info.app_name).toBe("SECS 2R");
    // The build reads both files; the mock answers with what they say now.
    const root = resolve(import.meta.dirname, "../../../../..");
    const cargo = readFileSync(resolve(root, "Cargo.toml"), "utf8").replace(/\r\n/g, "\n");
    const pinned = readFileSync(resolve(root, "UPSTREAM.toml"), "utf8");
    expect(cargo).toContain(`[workspace.package]\nversion = "${info.version}"`);
    for (const key of ["name", "version", "commit", "date"] as const) expect(pinned).toContain(`\n${key} = "${info.upstream[key]}"`);
    const project = await mock.request({ method: "project_info" });
    expect(project.positions).toEqual([1, 2, 3]);
    expect(mock.calls.map((c) => c.method)).toEqual(["bootstrap", "project_info"]);
  });

  it("fails on request with an error envelope, never a rejection of the command", async () => {
    const mock = createMockService({ tickMs: 0 });
    mock.failNext("project_save", "project_io", "EACCES");
    const envelope = await mock.invoke("secs_call", { request: { method: "project_save", params: { path: null } } });
    expect(envelope).toMatchObject({ ok: false, error: { code: "project_io", detail: "EACCES" } });
    await expect(mock.request({ method: "project_save", params: { path: null } })).resolves.toMatchObject({ dirty: false });
    mock.failNext("results", "internal");
    await expect(mock.request({ method: "results" })).rejects.toBeInstanceOf(IpcFailure);
  });

  it("runs jobs as pushed events and stops them on cancel", async () => {
    const mock = createMockService({ tickMs: 0 });
    const events: JobEvent[] = [];
    mock.events.subscribe((event) => events.push(event));
    const { job } = await mock.request({ method: "export", params: { kind: "camilla_dsp", folder: "/tmp" } }).catch((e: unknown) => {
      expect((e as IpcFailure).error.code).toBe("engine_input");
      return { job: -1 };
    });
    expect(job).toBe(-1);
    const started = await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    expect(events[0]).toEqual({ event: "started", job: started.job, kind: "calibrate" });
    mock.tick();
    expect(events[1]).toMatchObject({ event: "progress", stage: "engine.design", done: 1 });
    await expect(mock.request({ method: "calibrate", params: { settings: null, subs_only: false } })).rejects.toMatchObject({ error: { code: "job_busy" } });
    const snapshot = await mock.request({ method: "job_cancel", params: { job: started.job } });
    expect(snapshot.status).toBe("cancel_requested");
    mock.tick();
    expect(events.at(-1)).toMatchObject({ event: "finished", status: "cancelled", error: { code: "cancelled" } });
    expect(await mock.request({ method: "jobs" })).toEqual([]);
  });

  it("finishes a job with its result", async () => {
    const mock = createMockService({ tickMs: 0 });
    const events: JobEvent[] = [];
    mock.events.subscribe((event) => events.push(event));
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain();
    const last = events.at(-1);
    expect(last).toMatchObject({ event: "finished", status: "succeeded", result: { kind: "calibration" } });
    const results = await mock.request({ method: "results" });
    expect(results.calibration?.stale).toBe(false);
  });
});
