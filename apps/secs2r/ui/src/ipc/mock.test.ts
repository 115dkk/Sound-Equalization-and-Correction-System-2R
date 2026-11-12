import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { IpcFailure } from "./client";
import type { JobEvent } from "./bindings/JobEvent";
import { createMockService } from "./mock";
import { applyStatus, eqapoXt, gpuComputeStatus, projectInfo, resultsSummary } from "./fixtures";
import { recordedRefusal } from "./recorded";
import type { ExportKind } from "./bindings/ExportKind";
import type { ExportParams } from "./bindings/ExportParams";
import type { Request } from "./bindings/Request";
import { umikFile } from "../screens/connect/fixtures";

describe("mock MIMO early interval", () => {
  it("defaults to 60 ms from recorded settings and persists edits when reopening a saved project", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      expect((await mock.request({ method: "calibration_settings_get" })).mimo_early_ms).toBe(60);
      expect(await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_early", ms: 20 } } })).toMatchObject({ mimo_early_ms: 20 });
      await mock.request({ method: "project_save", params: { path: "/mimo-early.secs" } });
      await mock.request({ method: "project_new", params: { path: "/new-mimo-early.secs", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
      expect((await mock.request({ method: "calibration_settings_get" })).mimo_early_ms).toBe(60);
      await mock.request({ method: "project_open", params: { path: "/mimo-early.secs" } });
      expect((await mock.request({ method: "calibration_settings_get" })).mimo_early_ms).toBe(20);
    } finally { mock.dispose(); }
  });

  it.each([5, 20.5, 60])("accepts an early interval of %s ms without clamping", async (ms) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      expect(await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_early", ms } } })).toMatchObject({ mimo_early_ms: ms });
    } finally { mock.dispose(); }
  });

  it.each([4, 61, NaN, Infinity, -Infinity])("refuses invalid early interval %s without changing settings or results", async (ms) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false, mimo: true } });
      mock.drain();
      await mock.request({ method: "project_save", params: { path: "/mimo-early.secs" } });
      const settings = await mock.request({ method: "calibration_settings_get" });
      const results = await mock.request({ method: "results" });
      const project = await mock.request({ method: "project_info" });
      await expect(mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_early", ms } } })).rejects.toMatchObject({ error: { code: "invalid_request", params: { field: "mimo_early_ms", reason: "mimo_early_range", min: "5", max: "60" } } });
      expect(await mock.request({ method: "calibration_settings_get" })).toEqual(settings);
      expect(await mock.request({ method: "results" })).toEqual(results);
      expect(await mock.request({ method: "project_info" })).toEqual(project);
    } finally { mock.dispose(); }
  });
});

describe("mock MIMO seat guard", () => {
  it("defaults off from recorded settings and persists edits when reopening a saved project", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      expect((await mock.request({ method: "calibration_settings_get" })).mimo_seat_guard).toBe(false);
      expect(await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_seat_guard", on: true } } })).toMatchObject({ mimo_seat_guard: true });
      await mock.request({ method: "project_save", params: { path: "/seat-guard.secs" } });
      await mock.request({ method: "project_new", params: { path: "/new-seat-guard.secs", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
      expect((await mock.request({ method: "calibration_settings_get" })).mimo_seat_guard).toBe(false);
      await mock.request({ method: "project_open", params: { path: "/seat-guard.secs" } });
      expect((await mock.request({ method: "calibration_settings_get" })).mimo_seat_guard).toBe(true);
    } finally { mock.dispose(); }
  });
});

describe("mock job groups", () => {
  it.each([
    [{ method: "calibrate", params: { settings: null, subs_only: false } }, { method: "recommend_xo" }],
    [{ method: "recommend_xo" }, { method: "mimo", params: { settings: null } }],
    [{ method: "mimo", params: { settings: null } }, { method: "calibrate", params: { settings: null, subs_only: false } }],
    [{ method: "sample_load", params: { positions: 1 } }, { method: "mdat_import", params: { path: "/room.mdat", position: 1, assignments: [] } }],
    [{ method: "mdat_import", params: { path: "/room.mdat", position: 1, assignments: [] } }, { method: "level_monitor_start", params: { role: null } }],
    [{ method: "level_monitor_start", params: { role: null } }, { method: "measure_position", params: { position: 1 } }],
    [{ method: "measure_position", params: { position: 1 } }, { method: "sample_load", params: { positions: 1 } }],
    [{ method: "export", params: { kind: "camilla_dsp", folder: "" } }, { method: "apply_camilla", params: {} }],
    [{ method: "apply_camilla", params: {} }, { method: "export", params: { kind: "camilla_dsp", folder: "" } }],
  ] satisfies [import("./bindings/Request").Request, import("./bindings/Request").Request][]) ("refuses a different job in the same group: %j then %j", async (first, second) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
      const app = await mock.request({ method: "app_settings_get" });
      await mock.request({ method: "apply_settings_set", params: { ...app.apply, camilla: { ...app.apply.camilla, level: "apply" } } });
      const started = await mock.request(first);
      await expect(mock.request(second)).rejects.toMatchObject({ error: { code: "job_busy" } });
      expect(await mock.request({ method: "jobs" })).toEqual([expect.objectContaining(started)]);
    } finally { mock.dispose(); }
  });
});

describe("recommendation and app options", () => {
  it("accepts background requests, preserves the include choice and marks old recommendations", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      expect(await mock.request({ method: "bootstrap" })).toMatchObject({ log_dir: expect.stringContaining("logs") });
      const app = await mock.request({ method: "app_settings_get" });
      expect(app.auto_refresh).toBe(false);
      expect(await mock.request({ method: "app_settings_set", params: { settings: { ...app, auto_refresh: true } } })).toMatchObject({ auto_refresh: true });
      const background = { method: "recommend_xo" as const, params: { background: true } };
      await mock.request(background); mock.drain();
      expect(await mock.request({ method: "results" })).toMatchObject({ xo_rec_stale: false });
      expect(await mock.request({ method: "calibration_settings_get" })).toMatchObject({ xo_rec_key: expect.any(String) });
      const manual = await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "xo", group: "프론트", hz: 95 } } });
      expect(manual).toMatchObject({ xo_rec_include: false, xo: { 프론트: 95 } });
      expect(await mock.request({ method: "results" })).toMatchObject({ xo_rec_stale: false });
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "boost_max", db: 4 } } });
      expect(await mock.request({ method: "results" })).toMatchObject({ xo_rec_stale: true });
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false, background: true } }); mock.drain();
      expect((await mock.request({ method: "results" })).calibration?.stale).toBe(false);
      expect((await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "xo_rec_include", on: true } } })).xo_rec_include).toBe(true);
    } finally { mock.dispose(); }
  });
});

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

describe("standalone mock MIMO preconditions", () => {
  // The state the mock keeps: which correction MIMO runs on (the service's own preconditions are not copied).
  it.each(["missing calibration", "cached calibration", "stale calibration", "mismatched settings"] as const)("refuses %s before starting MIMO", async (condition) => {
    const mock = createMockService({ tickMs: 0, project: { ...projectInfo, positions: [1, 2, 3] } });
    const events: JobEvent[] = [];
    mock.events.subscribe((event) => events.push(event));
    try {
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
      if (condition !== "missing calibration") {
        await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
      }
      if (condition === "cached calibration") {
        await mock.request({ method: "project_save", params: { path: "/cached.secs" } });
        await mock.request({ method: "project_open", params: { path: "/cached.secs" } });
      }
      if (condition === "stale calibration") await mock.request({ method: "target_apply_preset", params: { preset: "flat" } });
      const settings = await mock.request({ method: "calibration_settings_get" });
      events.length = 0;
      await expect(mock.request({ method: "mimo", params: { settings: condition === "mismatched settings" ? { ...settings, boost_max_db: 4 } : null } })).rejects.toMatchObject({ error: { code: condition === "stale calibration" || condition === "mismatched settings" ? "stale_inputs" : "engine_input" } });
      expect(events).toEqual([]);
      expect(await mock.request({ method: "jobs" })).toEqual([]);
      expect(await mock.request({ method: "calibration_settings_get" })).toEqual(settings);
    } finally { mock.dispose(); }
  });

  it.each(["output channels", "output device", "unchanged wiring", "measurement only", "empty output device"] as const)("uses service config freshness after measure_configure: %s", async (change) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
      const measurement = await mock.request({ method: "measure_config_get" });
      await mock.request({ method: "measure_configure", params: { ...measurement,
        output_channels: change === "output channels" ? measurement.output_channels.map((out) => out + 1) : measurement.output_channels,
        output_device: change === "output device" ? "changed output" : change === "empty output device" ? "" : null,
        input_device: change === "measurement only" ? "changed input" : null,
        sweep_level_dbfs: change === "measurement only" ? -30 : measurement.sweep_level_dbfs,
      } });
      const stale = change === "output channels" || change === "output device";
      const mimo = mock.request({ method: "mimo", params: { settings: null } });
      if (stale) await expect(mimo).rejects.toMatchObject({ error: { code: "stale_inputs" } });
      else { await expect(mimo).resolves.toMatchObject({ kind: "mimo" }); mock.drain(); }
      expect((await mock.request({ method: "results" })).calibration).toMatchObject(stale ? { stale: true, stale_reason: "config" } : { stale: false });
    } finally { mock.dispose(); }
  });

  it("accepts MIMO-only overrides without changing stored calibration settings", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
      const settings = await mock.request({ method: "calibration_settings_get" });
      await mock.request({ method: "mimo", params: { settings: { ...settings, fsiso: 100 } } }); mock.drain();
      expect((await mock.request({ method: "results" })).mimo?.fsiso).toBe(100);
      expect(await mock.request({ method: "calibration_settings_get" })).toEqual(settings);
    } finally { mock.dispose(); }
  });
});

describe("mock MIMO export state", () => {
  // Not the service's decision (no record has MIMO): what the mock keeps after its own runs.
  it("marks a MIMO run included, a later change stale, and a new project off", async () => {
    const mock = createMockService({ tickMs: 0 });
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain();
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

describe("mock REW mdat scopes", () => {
  afterEach(() => vi.restoreAllMocks());

  async function exported(mock: ReturnType<typeof createMockService>, params: ExportParams) {
    const events: JobEvent[] = [];
    const unsubscribe = mock.events.subscribe((event) => events.push(event));
    try {
      await mock.request({ method: "export", params });
      mock.drain();
      const last = events.at(-1);
      if (last?.event !== "finished" || last.result?.kind !== "export") throw new Error("Missing export result");
      // The REW mdat is always written at 48 kHz (ADR 0017).
      const { output_rate, notes, ...written } = last.result.data;
      expect({ output_rate, notes }).toEqual({ output_rate: 48000, notes: [] });
      return written;
    } finally { unsubscribe(); }
  }

  it("exports measurements without a result into an explicit folder", async () => {
    const mock = createMockService({ tickMs: 0, project: { ...projectInfo, positions: [1, 2] } });
    try {
      expect((await mock.request({ method: "results" })).calibration).toBeNull();
      expect(await exported(mock, { kind: "before_after_mdat", folder: "C:/chosen", mdat_scope: "measurements" })).toMatchObject({ folder: "C:/chosen", stale: false });
    } finally { mock.dispose(); }
  });

  it("refuses empty measurements and tutorials before starting a job", async () => {
    for (const tutorial of [false, true]) {
      const mock = createMockService({ tickMs: 0, project: { ...projectInfo, positions: [], tutorial } });
      try {
        await expect(mock.request({ method: "export", params: { kind: "before_after_mdat", folder: "", mdat_scope: "measurements" } })).rejects.toMatchObject({
          error: tutorial ? { code: "tutorial_blocked" } : { code: "engine_input", params: { reason: "no_measurements" } },
        });
        expect(await mock.request({ method: "jobs" })).toEqual([]);
      } finally { mock.dispose(); }
    }
  });

  it.each(["all", "results", undefined] as const)("requires a result for scope %s", async (scope) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      await expect(mock.request({ method: "export", params: { kind: "before_after_mdat", folder: "", ...(scope ? { mdat_scope: scope } : {}) } })).rejects.toMatchObject({ error: { code: "engine_input" } });
      expect(await mock.request({ method: "jobs" })).toEqual([]);
    } finally { mock.dispose(); }
  });

  it("allows measurements through a stale cache while refusing result-bearing mdat exports", async () => {
    const mock = createMockService({ tickMs: 0, results: { ...resultsSummary, calibration: { ...resultsSummary.calibration!, cached: true, stale: true } } });
    try {
      for (const scope of ["all", "results", undefined] as const) {
        await expect(mock.request({ method: "export", params: { kind: "before_after_mdat", folder: "", ...(scope ? { mdat_scope: scope } : {}) } })).rejects.toMatchObject({ error: { code: "export_scope", params: { reason: "stale_cache" } } });
      }
      expect(await exported(mock, { kind: "before_after_mdat", folder: "C:/chosen", mdat_scope: "measurements" })).toMatchObject({ folder: "C:/chosen", stale: false });
    } finally { mock.dispose(); }
  });

  it.each(["camilla_dsp", "filter_text", "equalizer_apo", "staged_mdat"] as const)("refuses mdat_scope for %s even without a result", async (kind) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      for (const scope of ["all", "measurements", "results"] as const) {
        await expect(mock.request({ method: "export", params: { kind, folder: "", mdat_scope: scope } })).rejects.toMatchObject({ error: { code: "invalid_request", params: { reason: "mdat_scope" } } });
      }
      expect(await mock.request({ method: "jobs" })).toEqual([]);
    } finally { mock.dispose(); }
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

  /** The export's result without its rate, which `rated` returns: these exports run on the default DSP at 48 kHz. */
  async function exported(mock: ReturnType<typeof createMockService>, kind: ExportKind, folder = "", extra: Pick<ExportParams, "design_name" | "camilla_devices" | "extra_rates"> = {}) {
    const { output_rate, notes, ...written } = await rated(mock, kind, folder, extra);
    expect({ output_rate, notes }).toEqual({ output_rate: 48000, notes: [] });
    return written;
  }

  async function rated(mock: ReturnType<typeof createMockService>, kind: ExportKind, folder = "", extra: Pick<ExportParams, "design_name" | "camilla_devices" | "extra_rates"> = {}) {
    const events: JobEvent[] = [];
    const unsubscribe = mock.events.subscribe((event) => events.push(event));
    await mock.request({ method: "export", params: { kind, folder, ...extra } });
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

  it("allows BA in EqAPO and writes the recorded files directly into an explicit folder", async () => {
    const results = structuredClone(resultsSummary);
    results.mimo = null;
    const mock = createMockService({ tickMs: 0, results });
    const folder = "C:/exports/chosen";
    const written = await exported(mock, "equalizer_apo", folder);
    expect(written).toMatchObject({ folder, stale: false });
    expect(written.files.every((file) => file.startsWith(`${folder}/`))).toBe(true);
    mock.dispose();
  });

  it("keeps a stale cached result stale in what it writes", async () => {
    const results = resultWithoutSubs();
    results.calibration!.cached = true;
    results.calibration!.stale = true;
    const mock = createMockService({ tickMs: 0, results });
    try {
      expect(await exported(mock, "convolution", "C:/chosen")).toMatchObject({ folder: "C:/chosen", stale: true });
    } finally { mock.dispose(); }
  });

  it("refuses extra rates outside the generated output rates or on another kind, before a job", async () => {
    const mock = createMockService({ tickMs: 0, results: resultWithoutSubs() });
    try {
      for (const [kind, rates] of [["convolution", [32000]], ["camilla_dsp", [44100]]] as const) {
        await expect(mock.request({ method: "export", params: { kind, folder: "", extra_rates: [...rates] } })).rejects.toMatchObject({ error: { code: "invalid_request" } });
      }
      expect(await mock.request({ method: "jobs" })).toEqual([]);
    } finally { mock.dispose(); }
  });

  it.each(["fir_taps", "fir_budget", "iir_bands", "allpass_bands", "bm_path_delays"] as const)("makes the profile custom when %s changes along with the output rate", async (field) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      const dsp = await mock.request({ method: "dsp_get" });
      const edited = await mock.request({ method: "dsp_set", params: { preset: null, fields: { ...dsp.fields, [field]: dsp.fields[field] + 1, output_rate: "auto" } } });
      expect(edited).toMatchObject({ profile: "custom", fields: { [field]: dsp.fields[field] + 1, output_rate: "auto", output_rate_editable: true } });
    } finally { mock.dispose(); }
  });

  it("keeps the profile while only the output rate changes, without making the result stale", async () => {
    const mock = createMockService({ tickMs: 0, results: resultWithoutSubs() });
    try {
      const camilla = await mock.request({ method: "dsp_get" });
      const pc = await mock.request({ method: "dsp_set", params: { preset: null, fields: { ...camilla.fields, output_rate: { fixed: 96000 } } } });
      expect(pc).toMatchObject({ profile: "camilla", fields: { output_rate: { fixed: 96000 }, output_rate_editable: true } });
      expect((await mock.request({ method: "results" })).calibration?.stale).toBe(false);
      const automatic = await mock.request({ method: "dsp_set", params: { preset: null, fields: { ...pc.fields, output_rate: "auto" } } });
      expect(automatic).toMatchObject({ profile: "camilla", name: camilla.name, fields: { output_rate: "auto", output_rate_editable: true } });
      expect((await mock.request({ method: "results" })).calibration?.stale).toBe(false);
    } finally { mock.dispose(); }
  });

  it.each([
    ["", "length"], [" ", "length"], ["a".repeat(65), "length"], ["bad\nname", "control"],
    ["bad/name", "character"], ["bad.", "trailing"], ["con.txt", "reserved"],
  ])("rejects design name %j before starting a job", async (design_name, problem) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      await expect(mock.request({ method: "export", params: { kind: "convolution", folder: "", design_name } })).rejects.toMatchObject({ error: { code: "invalid_request", params: { reason: `design_name_${problem}` } } });
      expect(await mock.request({ method: "jobs" })).toEqual([]);
    } finally { mock.dispose(); }
  });

  it.each(["before_after_mdat", "staged_mdat"] as const)("refuses design names on %s", async (kind) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      await expect(mock.request({ method: "export", params: { kind, folder: "", design_name: "Cinema" } })).rejects.toMatchObject({ error: { code: "invalid_request", params: { reason: "design_name" } } });
      expect(await mock.request({ method: "jobs" })).toEqual([]);
    } finally { mock.dispose(); }
  });

  it("validates CamillaDSP-only devices, ASIO names and loopback sharing before a job", async () => {
    const mock = createMockService({ tickMs: 0, results: resultWithoutSubs() });
    try {
      const files = { capture: { type: "file" }, playback: { type: "file" } } as const;
      for (const kind of ["filter_text", "equalizer_apo", "convolution", "before_after_mdat", "staged_mdat"] as const) {
        await expect(mock.request({ method: "export", params: { kind, folder: "", camilla_devices: files } })).rejects.toMatchObject({ error: { code: "invalid_request", params: { reason: "camilla_devices" } } });
      }
      for (const side of ["capture", "playback"] as const) {
        await expect(mock.request({ method: "export", params: { kind: "camilla_dsp", folder: "", camilla_devices: { ...files, [side]: { type: "asio", device: " " } } } })).rejects.toMatchObject({ error: { code: "invalid_request", params: { reason: "camilla_asio_device" } } });
      }
      await expect(mock.request({ method: "export", params: { kind: "camilla_dsp", folder: "", camilla_devices: { ...files, capture: { type: "wasapi", device: null, format: "F32", loopback: true, exclusive: true } } } })).rejects.toMatchObject({ error: { code: "invalid_request", params: { reason: "camilla_wasapi_loopback_exclusive" } } });
      expect(await mock.request({ method: "jobs" })).toEqual([]);
      const result = await exported(mock, "camilla_dsp", "C:/chosen", { camilla_devices: { capture: { type: "wasapi", device: null, format: "F32", loopback: true, exclusive: false }, playback: { type: "asio", device: "Tesira ASIO" } } });
      expect(result.files).toContain("C:/chosen/configs/secs_mixed_bypass.yml");
    } finally { mock.dispose(); }
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
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
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
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
      await mock.request({ method: "mimo", params: { settings: null } }); mock.drain();
      expect((await mock.request({ method: "results" })).mimo?.compute).toMatchObject({
        requested: "gpu", backend: "cpu", stage: null, reason: "iir_reference", accelerated_systems: 0,
      });
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
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

describe("mock calibrate with MIMO (P141, P142)", () => {
  const withMimo = { settings: null, subs_only: false, mimo: true } as const;
  async function mimoOn(mock: ReturnType<typeof createMockService>) {
    await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
  }

  it("runs the MIMO stages after the correction's in the same job and stores both", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      const events: JobEvent[] = [];
      mock.events.subscribe((event) => events.push(event));
      await mimoOn(mock);
      const started = await mock.request({ method: "calibrate", params: withMimo });
      expect(started.kind).toBe("calibrate");
      mock.drain();
      const stages = [...new Set(events.flatMap((event) => (event.event === "progress" ? [event.stage] : [])))];
      expect(stages).toEqual(["engine.design", "engine.mimo", "engine.mimo_iir"]);
      expect(events.at(-1)).toMatchObject({ event: "finished", job: started.job, status: "succeeded", result: { kind: "calibration" } });
      const results = await mock.request({ method: "results" });
      expect(results.calibration?.stale).toBe(false);
      expect(results.mimo?.stale).toBe(false);
      expect(results.mimo_export).toBe("included");
    } finally { mock.dispose(); }
  });

  it("keeps the correction when MIMO fails after it, and marks the failure with stage mimo", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      const events: JobEvent[] = [];
      mock.events.subscribe((event) => events.push(event));
      await mimoOn(mock);
      mock.failJobs("mimo", "engine_input", "MIMO support design produced nonfinite filters", { detail_param: "x" });
      await mock.request({ method: "calibrate", params: withMimo });
      mock.drain();
      expect(events.at(-1)).toMatchObject({ event: "finished", status: "failed", result: null, error: { code: "engine_input", params: { stage: "mimo", detail_param: "x" } } });
      const results = await mock.request({ method: "results" });
      expect(results.calibration).toMatchObject({ stale: false });
      expect(results.mimo).toBeNull();
    } finally { mock.dispose(); }
  });

  it("fails a correction failure before storing anything, without the MIMO stages", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      const events: JobEvent[] = [];
      mock.events.subscribe((event) => events.push(event));
      await mimoOn(mock);
      mock.failJobs("calibrate", "engine_input", "invalid input: position 1 SW3: empty or nonfinite IR", { reason: "invalid_measurement", position: "1", role: "SW3" });
      await mock.request({ method: "calibrate", params: withMimo });
      mock.drain();
      expect(events.some((event) => event.event === "progress" && event.stage.startsWith("engine.mimo"))).toBe(false);
      expect(events.at(-1)).toMatchObject({ event: "finished", status: "failed", error: { params: { reason: "invalid_measurement" } } });
      expect(events.at(-1)).not.toHaveProperty("error.params.stage");
      expect((await mock.request({ method: "results" })).calibration).toBeNull();
    } finally { mock.dispose(); }
  });

  it("keeps the new correction and no MIMO result when cancelled during the MIMO stages", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      const events: JobEvent[] = [];
      mock.events.subscribe((event) => events.push(event));
      await mimoOn(mock);
      const { job } = await mock.request({ method: "calibrate", params: withMimo });
      while (!events.some((event) => event.event === "progress" && event.stage === "engine.mimo")) mock.tick();
      await mock.request({ method: "job_cancel", params: { job } });
      mock.tick();
      expect(events.at(-1)).toMatchObject({ event: "finished", status: "cancelled" });
      const results = await mock.request({ method: "results" });
      expect(results.calibration).toMatchObject({ stale: false });
      expect(results.mimo).toBeNull();
    } finally { mock.dispose(); }
  });

  it("leaves the correction current after a MIMO-only edit, so MIMO alone can follow", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      await mimoOn(mock);
      await mock.request({ method: "calibrate", params: withMimo });
      mock.drain();
      for (const edit of [{ kind: "fsiso", hz: 120 }, { kind: "mimo_low", hz: 25 }, { kind: "mimo_max", count: 2 }, { kind: "mimo_fir", on: true }, { kind: "mimo_early", ms: 20 }, { kind: "mimo_early", ms: 60 }, { kind: "mimo_seat_guard", on: true }, { kind: "mimo_seat_guard", on: false }] as const) {
        await mock.request({ method: "calibration_settings_edit", params: { edit } });
        const results = await mock.request({ method: "results" });
        expect(results.calibration?.stale, edit.kind).toBe(false);
        expect(results.mimo?.stale, edit.kind).toBe(true);
        await mock.request({ method: "mimo", params: { settings: null } });
        mock.drain();
        expect((await mock.request({ method: "results" })).mimo?.stale, edit.kind).toBe(false);
      }
      // Turning MIMO off and on changes neither result; a correction setting makes both stale.
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: false } } });
      await mimoOn(mock);
      expect(await mock.request({ method: "results" })).toMatchObject({ calibration: { stale: false }, mimo: { stale: false } });
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "boost_max", db: 6 } } });
      expect(await mock.request({ method: "results" })).toMatchObject({ calibration: { stale: true }, mimo: { stale: true } });
    } finally { mock.dispose(); }
  });
});

describe("mock measurement graph reference (#127, U17)", () => {
  const all = { positions: null, roles: null };

  it("picks the reference before the request's filter, so a filtered curve keeps its values", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      const full = await mock.request({ method: "measurement_curves", params: all });
      const filtered = await mock.request({ method: "measurement_curves", params: { positions: [2], roles: ["R"] } });
      expect(filtered.reference).toEqual(full.reference);
      expect(filtered.curves).toEqual(full.curves.filter((curve) => curve.role === "R" && curve.position === 2));
    } finally { mock.dispose(); }
  });
});

describe("mock 극저역 보호 (ADR 0022)", () => {
  const calibrate = async (mock: ReturnType<typeof createMockService>) => {
    await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain();
    return (await mock.request({ method: "results" })).calibration!.notes.filter((note) => note.code === "low_protect");
  };

  it("applies the edit, marks the result stale and leaves no notes while off", async () => {
    const mock = createMockService({ tickMs: 0, results: resultsSummary });
    try {
      expect((await mock.request({ method: "calibration_settings_get" })).low_protect).toBe(false);
      expect(await calibrate(mock)).toEqual([]);
      const settings = await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "low_protect", on: true } } });
      expect(settings.low_protect).toBe(true);
      expect((await mock.request({ method: "results" })).calibration).toMatchObject({ stale: true, stale_reason: "settings" });
    } finally { mock.dispose(); }
  });

});

describe("mock 저음 도착 맞추기, 고역 위상 맞추기 and 방 울림 줄이기 (ADR 0025, 0030)", () => {
  it("applies each switch in every mode, Zero included, and marks the result stale", async () => {
    const mock = createMockService({ tickMs: 0, results: resultsSummary });
    const edit = (edit: import("./bindings/SettingEdit").SettingEdit) => mock.request({ method: "calibration_settings_edit", params: { edit } });
    try {
      await edit({ kind: "mode", mode: "zero" });
      expect(await edit({ kind: "lf_align", on: true })).toMatchObject({ mode: "zero", lf_align: true });
      expect((await mock.request({ method: "results" })).calibration).toMatchObject({ stale: true, stale_reason: "settings" });
      expect(await edit({ kind: "mode_decay", on: true })).toMatchObject({ lf_align: true, mode_decay: true });
      // Switching modes keeps the stored choice.
      expect(await edit({ kind: "mode", mode: "linear" })).toMatchObject({ mode: "linear", lf_align: true, mode_decay: true });
      expect(await edit({ kind: "lf_align", on: false })).toMatchObject({ lf_align: false, mode_decay: true });
      expect(await edit({ kind: "mode_decay", on: false })).toMatchObject({ lf_align: false, mode_decay: false });
      expect(await edit({ kind: "mode", mode: "zero" })).toMatchObject({ mode: "zero" });
      expect(await edit({ kind: "hf_phase", on: true })).toMatchObject({ mode: "zero", hf_phase: true, lf_align: false });
      expect(await edit({ kind: "hf_phase", on: false })).toMatchObject({ hf_phase: false });
    } finally { mock.dispose(); }
  });
});

describe("mock main alignment delay off (ADR 0061)", () => {
  it.each(["zero", "mixed", "linear", "ultra"] as const)("defaults off, persists both edits and marks fresh results stale in %s mode", async (mode) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      expect((await mock.request({ method: "calibration_settings_get" })).main_delay_off).toBe(false);
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mode", mode } } });
      for (const on of [true, false]) {
        await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
        mock.drain();
        expect((await mock.request({ method: "results" })).calibration?.stale).toBe(false);
        expect(await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "main_delay_off", on } } })).toMatchObject({ mode, main_delay_off: on });
        expect((await mock.request({ method: "calibration_settings_get" })).main_delay_off).toBe(on);
        expect((await mock.request({ method: "results" })).calibration).toMatchObject({ stale: true, stale_reason: "settings" });
      }
    } finally { mock.dispose(); }
  });
});

describe("mock dip correction limit (ADR 0034)", () => {
  it.each(["zero", "mixed", "linear", "ultra"] as const)("defaults off, persists both edits and marks fresh results stale in %s mode", async (mode) => {
    const mock = createMockService({ tickMs: 0 });
    try {
      expect((await mock.request({ method: "calibration_settings_get" })).dip_limit).toBe(false);
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mode", mode } } });
      for (const on of [true, false]) {
        await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
        mock.drain();
        expect((await mock.request({ method: "results" })).calibration?.stale).toBe(false);
        expect(await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "dip_limit", on } } })).toMatchObject({ mode, dip_limit: on });
        expect((await mock.request({ method: "calibration_settings_get" })).dip_limit).toBe(on);
        expect((await mock.request({ method: "results" })).calibration).toMatchObject({ stale: true, stale_reason: "settings" });
      }
    } finally { mock.dispose(); }
  });
});

describe("mock extra correction bands (ADR 0024)", () => {
  type Edit = import("./bindings/SettingEdit").SettingEdit;
  const refusal = (mock: ReturnType<typeof createMockService>, edit: Edit) =>
    mock.request({ method: "calibration_settings_edit", params: { edit } }).then(() => null, (failure: { error: { code: string; params: Record<string, string> } }) => failure.error);

  it("stores a band list sorted and clears it with an empty one, and refuses only what the service was recorded refusing", async () => {
    const mock = createMockService({ tickMs: 0 });
    try {
      const band = (lo_hz: number, hi_hz: number) => ({ lo_hz, hi_hz });
      const before = await mock.request({ method: "calibration_settings_get" });
      expect(await refusal(mock, { kind: "rc_bands", bands: [band(2000, 1000)] })).toEqual(recordedRefusal({ kind: "rc_bands", bands: [band(2000, 1000)] }));
      expect((await mock.request({ method: "calibration_settings_get" })).rc_bands).toEqual(before.rc_bands);
      const stored = await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "rc_bands", bands: [band(15000, 20000), band(2000, 4000)] } } });
      expect(stored.rc_bands).toEqual([band(2000, 4000), band(15000, 20000)]);
      expect((await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "rc_bands", bands: [] } } })).rc_bands).toEqual([]);
    } finally { mock.dispose(); }
  });

  it("hands the graph the bands on SPL and GD", async () => {
    const mock = createMockService({ tickMs: 0, results: resultsSummary });
    const curves = (tab: "spl" | "gd" | "xo") => mock.request({ method: "result_curves", params: { tab, channel: null, group: null, xo_hz: null, corrected: null } });
    try {
      expect((await curves("spl")).rc_bands_hz).toEqual([]);
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "rc_bands", bands: [{ lo_hz: 15000, hi_hz: 20000 }, { lo_hz: 300, hi_hz: 800 }] } } });
      expect((await curves("spl")).rc_bands_hz).toEqual([[300, 800], [15000, 20000]]);
      expect((await curves("gd")).rc_bands_hz).toEqual([[300, 800], [15000, 20000]]);
      expect((await curves("xo")).rc_bands_hz).toEqual([]);
    } finally { mock.dispose(); }
  });
});


describe("mock export progress", () => {
  const cases: { request: Extract<Request, { method: "export" | "apply_camilla" | "apply_eqapo" }>; stage: string; contents: number; bytes: number; files?: number }[] = [
    { request: { method: "export", params: { kind: "camilla_dsp", folder: "" } }, stage: "export.prepare", contents: 11, bytes: 8 * 1024 ** 2, files: 14 },
    { request: { method: "export", params: { kind: "equalizer_apo", folder: "" } }, stage: "export.prepare", contents: 11, bytes: 8 * 1024 ** 2, files: 14 },
    { request: { method: "export", params: { kind: "filter_text", folder: "" } }, stage: "export.prepare", contents: 11, bytes: 8 * 1024 ** 2, files: 14 },
    { request: { method: "export", params: { kind: "convolution", folder: "" } }, stage: "filters.convolution", contents: 338, bytes: 64 * 1024 ** 2, files: 341 },
    { request: { method: "export", params: { kind: "before_after_mdat", folder: "" } }, stage: "export.prepare", contents: 234, bytes: 32 * 1024 ** 2 },
    { request: { method: "export", params: { kind: "staged_mdat", folder: "" } }, stage: "export.prepare", contents: 585, bytes: 80 * 1024 ** 2 },
    { request: { method: "apply_camilla", params: {} }, stage: "export.prepare", contents: 11, bytes: 8 * 1024 ** 2, files: 14 },
    { request: { method: "apply_eqapo" }, stage: "export.prepare", contents: 11, bytes: 8 * 1024 ** 2, files: 14 },
  ];

  async function setup() {
    const mock = createMockService({ tickMs: 0, results: {
      ...resultsSummary, calibration: { ...resultsSummary.calibration!, sub_roles: [] },
    } });
    const app = await mock.request({ method: "app_settings_get" });
    await mock.request({ method: "apply_settings_set", params: {
      ...app.apply, camilla: { ...app.apply.camilla, level: "apply" }, eqapo: { level: "apply" },
    } });
    mock.setApplyStatus({ ...applyStatus, eqapo: eqapoXt });
    return mock;
  }

  it.each(cases)("plays counted contents, byte writes and, for a bundle, the files moved into place for $request", async ({ request, stage, contents, bytes, files }) => {
    const mock = await setup();
    const events: JobEvent[] = [];
    const unsubscribe = mock.events.subscribe((event) => events.push(event));
    try {
      const started = await mock.request(request);
      expect(events).toEqual([{ event: "started", ...started }]);
      mock.drain();
      const progress = events.filter((event) => event.event === "progress");
      expect(progress.every((event) => event.job === started.job)).toBe(true);
      expect(progress.map((event) => event.stage).filter((name, index, stages) => index === 0 || name !== stages[index - 1]))
        .toEqual(files === undefined ? [stage, "export.write"] : [stage, "export.write", "export.install", "export.manifest"]);
      for (const [name, total] of [[stage, contents], ["export.write", bytes], ...(files === undefined ? [] : [["export.install", files], ["export.manifest", files]] as const)] as const) {
        const reports = progress.filter((event) => event.stage === name);
        expect(reports[0]).toMatchObject({ done: 0, total });
        expect(reports.at(-1)).toMatchObject({ done: total, total });
        expect(reports.every((event, index) => event.total === total && total > 0
          && event.done >= 0 && event.done <= total && (index === 0 || event.done >= reports[index - 1]!.done))).toBe(true);
      }
      expect(progress.filter((event) => event.stage === stage).map((event) => event.done))
        .toEqual([0, Math.floor(contents / 3), Math.floor(contents * 2 / 3), contents]);
      expect(progress.filter((event) => event.stage === "export.write").map((event) => event.done))
        .toEqual([0, bytes / 2, bytes]);
      for (const moved of ["export.install", "export.manifest"]) {
        expect(progress.filter((event) => event.stage === moved).map((event) => event.done))
          .toEqual(files === undefined ? [] : [0, Math.floor(files / 2), files]);
      }
      expect(events.at(-2)).toMatchObject(files === undefined
        ? { event: "progress", stage: "export.write", done: bytes, total: bytes }
        : { event: "progress", stage: "export.manifest", done: files, total: files });
      expect(events.at(-1)).toMatchObject({ event: "finished", job: started.job, status: "succeeded", error: null });
      expect(await mock.request({ method: "jobs" })).toEqual([]);
    } finally { unsubscribe(); mock.dispose(); }
  });

  it.each(["export.prepare", "export.write", "export.install", "export.manifest"])("still cancels an export or apply during %s", async (stage) => {
    for (const request of [cases[0]!.request, cases[6]!.request]) {
      const mock = await setup();
      const events: JobEvent[] = [];
      const unsubscribe = mock.events.subscribe((event) => events.push(event));
      try {
        const started = await mock.request(request);
        for (let ticks = 0; ticks < 20; ticks++) {
          mock.tick();
          if (events.some((event) => event.event === "progress" && event.stage === stage)) break;
        }
        expect(events.at(-1)).toMatchObject({ event: "progress", stage, done: 0 });
        const reports = events.filter((event) => event.event === "progress").length;
        await mock.request({ method: "job_cancel", params: { job: started.job } });
        mock.drain();
        expect(events.filter((event) => event.event === "progress")).toHaveLength(reports);
        expect(events.at(-1)).toMatchObject({ event: "finished", job: started.job, status: "cancelled", result: null });
      } finally { unsubscribe(); mock.dispose(); }
    }
  });
});
