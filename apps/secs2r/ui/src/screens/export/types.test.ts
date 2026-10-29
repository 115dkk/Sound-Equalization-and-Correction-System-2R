import { describe, expect, it } from "vitest";
import type { ApplyLevel } from "../../ipc/bindings/ApplyLevel";
import type { ExportBlock } from "../../ipc/bindings/ExportBlock";
import type { ExportKind } from "../../ipc/bindings/ExportKind";
import type { MimoExportDecision } from "../../ipc/bindings/MimoExportDecision";
import { MOCK_DESIGN_ID } from "../../ipc/fixtures";
import {
  DESIGN_NAME_SAMPLES,
  EXPORT_DEVICES,
  applyCamillaApplied,
  applyCamillaBadAddress,
  applyCamillaOther,
  applyCamillaRolledBack,
  applyCamillaThis,
  applyEqapoAbsent,
  applyEqapoMissing,
  applyEqapoNotWindows,
  applyEqapoOfficialUnknown,
  applyEqapoOther,
  applyEqapoPresent,
  exportDevicesAsioUnnamed,
  exportMimoNoSubs,
  exportNoResult,
  exportRcOnly,
  exportReady,
  exportStale,
} from "./fixtures";
import {
  APPLY_LEVELS,
  EXPORT_KINDS,
  FILTER_KINDS,
  MDAT_SCOPES,
  addressProblem,
  applyLevel,
  applyProblem,
  applyWanted,
  canApply,
  canExport,
  currentDesignId,
  eqapoDesign,
  exportGroupBusy,
  exportStepState,
  isLoopback,
  designNameFor,
  designNameProblem,
  deviceProblem,
  devicesFor,
  exportBlock,
  filterModeName,
  inputProblem,
  isFilterKind,
  mdatFileName,
  mdatScopeFor,
  mimoInclusion,
  type AddressProblem,
  type DesignNameProblem,
  type ExportView,
  type MimoInclusion,
} from "./types";

/** The sample result with its subs and MIMO switched off. */
const subsOnly: ExportView = { ...exportReady, settings: { ...exportReady.settings, mimo_on: false }, results: { ...exportReady.results, mimo_export: "off" } };

const judged = (view: ExportView) => Object.fromEntries(EXPORT_KINDS.map((kind) => [kind, exportBlock(view, kind)])) as Record<ExportKind, ExportBlock | null>;

describe("MIMO decision presentation", () => {
  const expected = {
    included: { included: true, kind: "iir", supports: 22, channels: 11 },
    off: { included: false, reason: "off" },
    none: { included: false, reason: "none" },
    stale: { included: false, reason: "stale" },
    too_few_positions: { included: false, reason: "positions" },
    no_supports: { included: true, kind: "iir", supports: 0, channels: 11 },
  } satisfies Record<MimoExportDecision, MimoInclusion>;

  it.each(Object.entries(expected) as [MimoExportDecision, MimoInclusion][])("maps %s to the existing screen shape", (decision, inclusion) => {
    const mimo = decision === "none" ? null : { ...exportReady.results.mimo!, support_count: decision === "no_supports" ? 0 : 22 };
    // Deliberately conflicting local inputs: only the service decision selects the presentation.
    expect(mimoInclusion({
      ...exportReady,
      settings: { ...exportReady.settings, mimo_on: false },
      positions: 0,
      results: { ...exportReady.results, mimo, mimo_export: decision },
    })).toEqual(inclusion);
  });
});

describe("export scope (the service's filter_export_block)", () => {
  it("lists the kinds in the order the screen draws them", () => {
    expect(EXPORT_KINDS).toEqual(["camilla_dsp", "filter_text", "equalizer_apo", "convolution", "before_after_mdat", "staged_mdat"]);
    // The filter exports (a design name, secs-design.json) lead in the same order; the REW mdat follow.
    expect(FILTER_KINDS).toEqual(EXPORT_KINDS.slice(0, 4));
    expect(EXPORT_KINDS.filter(isFilterKind)).toEqual(FILTER_KINDS);
  });

  it("judges every kind for subs × MIMO: text carries RC only (subs first), Equalizer APO and the convolution IR everything (F12)", () => {
    // The four fixtures are the four combinations.
    expect([subsOnly, exportReady, exportRcOnly, exportMimoNoSubs].map((view) => [view.results.calibration!.sub_roles.length > 0, mimoInclusion(view).included])).toEqual([
      [true, false],
      [true, true],
      [false, false],
      [false, true],
    ]);
    const open = { camilla_dsp: null, convolution: null, before_after_mdat: null, staged_mdat: null };
    expect(judged(subsOnly)).toEqual({ ...open, filter_text: "subs", equalizer_apo: null });
    expect(judged(exportReady)).toEqual({ ...open, filter_text: "subs", equalizer_apo: null });
    expect(judged(exportRcOnly)).toEqual({ ...open, filter_text: null, equalizer_apo: null });
    expect(judged(exportMimoNoSubs)).toEqual({ ...open, filter_text: "mimo", equalizer_apo: null });
  });

  it("counts MIMO only when it goes into the export, and blocks stale cached mdat", () => {
    // Fewer than three positions, or a stale MIMO: left out, so it blocks nothing.
    expect(exportBlock({ ...exportMimoNoSubs, positions: 2, results: { ...exportMimoNoSubs.results, mimo_export: "too_few_positions" } }, "filter_text")).toBeNull();
    expect(exportBlock({ ...exportMimoNoSubs, results: { ...exportMimoNoSubs.results, mimo: { ...exportMimoNoSubs.results.mimo!, stale: true }, mimo_export: "stale" } }, "filter_text")).toBeNull();
    expect(exportBlock(exportStale, "equalizer_apo")).toBeNull();
    const staleCached = {
      ...exportStale,
      results: { ...exportStale.results, calibration: { ...exportStale.results.calibration!, cached: true } },
    };
    expect(judged(staleCached)).toEqual({
      camilla_dsp: null,
      filter_text: "subs",
      equalizer_apo: null,
      convolution: null,
      before_after_mdat: "stale_cache",
      staged_mdat: "stale_cache",
    });
    expect(judged(exportNoResult)).toEqual(Object.fromEntries(EXPORT_KINDS.map((kind) => [kind, null])));
    // The original measurements need no design: an old cache blocks only the results.
    expect(judged({ ...staleCached, mdatScope: "measurements" })).toEqual({
      camilla_dsp: null,
      filter_text: "subs",
      equalizer_apo: null,
      convolution: null,
      before_after_mdat: null,
      staged_mdat: "stale_cache",
    });
    expect(exportBlock({ ...staleCached, mdatScope: "results" }, "before_after_mdat")).toBe("stale_cache");
  });

  it("writes the REW mdat with the chosen contents, or the measurements alone without a result", () => {
    expect(MDAT_SCOPES).toEqual(["all", "measurements", "results"]);
    for (const scope of MDAT_SCOPES) {
      expect(mdatScopeFor(exportReady.results.calibration, scope)).toBe(scope);
      expect(mdatScopeFor(exportNoResult.results.calibration, scope)).toBe("measurements");
    }
  });

  it("names the REW mdat by what it holds, with the mode as CalibrationSettings::label writes it", () => {
    expect(mdatFileName("measurements", { mode: "mixed", ultra_ms: 200 })).toBe("SECS 원 측정.mdat");
    expect(mdatFileName("all", { mode: "mixed", ultra_ms: 200 })).toBe("SECS 원 측정+보정 결과 저지연 모드.mdat");
    expect(mdatFileName("results", { mode: "zero", ultra_ms: 200 })).toBe("SECS 보정 결과 최소 지연 모드.mdat");
    expect(mdatFileName("results", { mode: "linear", ultra_ms: 200 })).toBe("SECS 보정 결과 표준 모드.mdat");
    // secs-types::mode_labels_match_upstream: the limit clamps to 30–200 ms and prints as Python's :g.
    expect(mdatFileName("all", { mode: "ultra", ultra_ms: 20 })).toBe("SECS 원 측정+보정 결과 초고지연 모드 30 ms.mdat");
    expect(mdatFileName("all", { mode: "ultra", ultra_ms: 123.456789 })).toBe("SECS 원 측정+보정 결과 초고지연 모드 123.457 ms.mdat");
    expect(mdatFileName("all", { mode: "ultra", ultra_ms: 250 })).toBe("SECS 원 측정+보정 결과 초고지연 모드 200 ms.mdat");
    // The words decide it: a measurement file never says 보정 결과, a result file never says 원 측정.
    for (const scope of MDAT_SCOPES) {
      const name = mdatFileName(scope, { mode: "mixed", ultra_ms: 200 });
      expect(name.includes("원 측정"), scope).toBe(scope !== "results");
      expect(name.includes("보정 결과"), scope).toBe(scope !== "measurements");
    }
  });

  it("checks a design name by the service's rules, in its order, one problem per rule", () => {
    // secs-service::invalid_design_names_are_rejected_before_a_folder_is_chosen, plus the field's empty text.
    const cases: [string, DesignNameProblem | null][] = [
      ["", null],
      ["Cinema", null],
      ["  Cinema  ", null],
      ["거실 영화", null],
      ["x".repeat(64), null],
      ["거".repeat(64), null],
      // Code points, not UTF-16 units: 64 astral characters fit.
      ["𝄞".repeat(64), null],
      ["CONCERT", null],
      ["COM10", null],
      ["COM0", null],
      ["LPT", null],
      ["a.CON", null],
      [" ", "length"],
      ["\t\n", "length"],
      ["x".repeat(65), "length"],
      ["𝄞".repeat(65), "length"],
      ["line\nfeed", "control"],
      ["tab\there", "control"],
      ["del\u007fete", "control"],
      ...[..."\\/:*?\"<>|"].map((ch): [string, DesignNameProblem] => [`bad${ch}name`, "character"]),
      ["trailing.", "trailing"],
      ["trailing.  ", "trailing"],
      ["CON", "reserved"],
      ["con", "reserved"],
      ["CON.txt", "reserved"],
      ["Nul.거실", "reserved"],
      ["prn", "reserved"],
      ["aux", "reserved"],
      ["COM1", "reserved"],
      ["com9", "reserved"],
      ["LPT1", "reserved"],
      ["lpt9.wav", "reserved"],
    ];
    for (const [name, problem] of cases) expect(designNameProblem(name), JSON.stringify(name)).toBe(problem);
    // The rules apply in the service's order: length before characters, characters before the ending.
    expect(designNameProblem(`${"/".repeat(65)}`)).toBe("length");
    expect(designNameProblem("a\n/b")).toBe("control");
    expect(designNameProblem("a/b.")).toBe("character");
    expect(designNameProblem("CON.")).toBe("trailing");
    // The samples the gallery and the screen tests draw, one per rule.
    for (const [problem, name] of Object.entries(DESIGN_NAME_SAMPLES)) expect(designNameProblem(name), problem).toBe(problem);
  });

  it("uses the trimmed name the service will use, and none in the tutorial or with a problem", () => {
    expect(designNameFor({ designName: "", tutorial: false })).toBeNull();
    expect(designNameFor({ designName: "  거실 영화 ", tutorial: false })).toBe("거실 영화");
    expect(designNameFor({ designName: "거실/영화", tutorial: false })).toBeNull();
    expect(designNameFor({ designName: "거실 영화", tutorial: true })).toBeNull();
  });

  it("holds the filter exports for a bad design name and CamillaDSP for an unnamed ASIO driver", () => {
    const named = { ...exportRcOnly, designName: "거실/영화" };
    for (const kind of EXPORT_KINDS) {
      expect(inputProblem(named, kind), kind).toEqual(isFilterKind(kind) ? { field: "design_name", problem: "character" } : null);
      // canExport is about the result and the jobs; the field holds the buttons separately.
      expect(canExport(named, kind), kind).toBe(true);
    }
    expect(FILTER_KINDS.map((kind) => inputProblem({ ...exportRcOnly, designName: "" }, kind))).toEqual([null, null, null, null]);
    // The tutorial has no name field and writes nothing.
    expect(inputProblem({ ...named, tutorial: true }, "filter_text")).toBeNull();
    const asio: ExportView = { ...exportRcOnly, camillaDevices: { capture: { type: "asio", device: "  " }, playback: { type: "asio", device: "" } } };
    expect(inputProblem(asio, "camilla_dsp")).toEqual({ field: "capture", problem: "asio_device" });
    expect(inputProblem({ ...asio, camillaDevices: { ...asio.camillaDevices, capture: { type: "file" } } }, "camilla_dsp")).toEqual({ field: "playback", problem: "asio_device" });
    for (const kind of EXPORT_KINDS.filter((kind) => kind !== "camilla_dsp")) expect(inputProblem(asio, kind), kind).toBeNull();
    // The name comes first.
    expect(inputProblem({ ...asio, designName: "COM1" }, "camilla_dsp")).toEqual({ field: "design_name", problem: "reserved" });
    expect(deviceProblem({ type: "asio", device: "Tesira ASIO" })).toBeNull();
    expect(deviceProblem({ type: "wasapi", device: null, format: "F32", exclusive: false })).toBeNull();
    expect(deviceProblem({ type: "file" })).toBeNull();
  });

  it("lists inputs for capture, outputs for playback and for loopback capture, one per name", () => {
    const names = (side: "capture" | "playback", loopback = false) => devicesFor(EXPORT_DEVICES, side, loopback).map((device) => device.name);
    expect(names("capture")).toEqual(["CABLE Output (VB-Audio Virtual Cable)", "Line (Tesira)", "UMIK-1"]);
    expect(names("playback")).toEqual(["Speakers (Tesira)", "CABLE Input (VB-Audio Virtual Cable)"]);
    expect(names("capture", true)).toEqual(names("playback"));
    const twice = [...EXPORT_DEVICES, { ...EXPORT_DEVICES[0]!, id: "wasapi:other" }];
    expect(devicesFor(twice, "playback").map((device) => device.name)).toEqual(names("playback"));
  });

  it("names the filter files' mode as upstream _filter_plan does", () => {
    expect(filterModeName({ mode: "mixed", ultra_ms: 200 })).toBe("mixed");
    expect(filterModeName({ mode: "zero", ultra_ms: 120 })).toBe("zero");
    expect(filterModeName({ mode: "ultra", ultra_ms: 200 })).toBe("ultra200");
    expect(filterModeName({ mode: "ultra", ultra_ms: 62.5 })).toBe("ultra62.5");
    expect(filterModeName({ mode: "ultra", ultra_ms: 30 })).toBe("ultra30");
  });
});

describe("적용 (ADR 0018)", () => {
  it("checks a CamillaDSP address by the service's validate_address rules, on the trimmed text", () => {
    // crates/secs-service/src/apply.rs `addresses_require_a_nonzero_port`.
    for (const address of ["127.0.0.1:1234", "localhost:1", "[::1]:1234", "camilla.local:65535", "+host:+80", " 127.0.0.1:1234 "]) {
      expect(addressProblem(address), address).toBeNull();
    }
    const problems: [string, AddressProblem][] = [
      ["localhost", "format"], [":1234", "format"], ["host:", "format"], ["a:b:1", "format"], ["my host:1234", "format"],
      ["[::1]", "format"], ["[]:1234", "format"], ["", "format"],
      ["host:0", "port"], ["host:no", "port"], ["host:65536", "port"], ["host:-1", "port"], ["[::1]:0", "port"],
    ];
    for (const [address, problem] of problems) expect(addressProblem(address), address).toBe(problem);
  });

  it("says which addresses stay on this computer", () => {
    for (const address of ["127.0.0.1:1234", "127.10.0.3:1", "localhost:1234", "LOCALHOST:1", "[::1]:1234"]) expect(isLoopback(address), address).toBe(true);
    for (const address of ["192.168.0.40:1234", "camilla.local:1234", "0.0.0.0:1234", "[fe80::1]:1234", "128.0.0.1:1234"]) expect(isLoopback(address), address).toBe(false);
  });

  it("draws 관리 as 적용, Equalizer APO off Windows and anything in the tutorial as 파일만", () => {
    expect(APPLY_LEVELS).toEqual(["files", "apply"]);
    const at = (camilla: ApplyLevel, eqapo: ApplyLevel, windows = true, tutorial = false): ExportView => ({
      ...exportReady, tutorial, apply: { ...exportReady.apply, windows, settings: { camilla: { level: camilla, address: "127.0.0.1:1234" }, eqapo: { level: eqapo } } },
    });
    expect([applyLevel(at("manage", "manage"), "camilla"), applyLevel(at("manage", "manage"), "eqapo")]).toEqual(["apply", "apply"]);
    expect([applyLevel(at("apply", "apply", false), "camilla"), applyLevel(at("apply", "apply", false), "eqapo")]).toEqual(["apply", "files"]);
    expect([applyLevel(at("apply", "apply", true, true), "camilla"), applyLevel(at("apply", "apply", true, true), "eqapo")]).toEqual(["files", "files"]);
    expect(applyWanted(at("files", "files"))).toBe(false);
    expect(applyWanted(at("files", "apply", false))).toBe(false);
    expect(applyWanted(at("files", "apply"))).toBe(true);
  });

  it("holds 적용 for the address, an unnamed ASIO driver, Windows and the install, and while the export group is busy", () => {
    expect(canApply(applyCamillaThis, "camilla")).toBe(true);
    expect(applyProblem(applyCamillaBadAddress, "camilla")).toEqual({ field: "address", problem: "format" });
    expect(canApply(applyCamillaBadAddress, "camilla")).toBe(false);
    expect(applyProblem({ ...applyCamillaThis, camillaDevices: exportDevicesAsioUnnamed.camillaDevices }, "camilla")).toEqual({ field: "capture", problem: "asio_device" });
    // The design name is the export's business, not 적용's.
    expect(canApply({ ...applyCamillaThis, designName: "CON" }, "camilla")).toBe(true);
    expect(applyProblem(applyEqapoNotWindows, "eqapo")).toEqual({ field: "eqapo", problem: "windows" });
    expect(applyProblem({ ...applyEqapoAbsent, apply: { ...applyEqapoAbsent.apply, status: null } }, "eqapo")).toEqual({ field: "eqapo", problem: "checking" });
    expect(applyProblem(applyEqapoMissing, "eqapo")).toEqual({ field: "eqapo", problem: "not_installed" });
    expect(canApply(applyEqapoAbsent, "eqapo")).toBe(true);
    // Not set to 적용, no result, computing, or another export-group job.
    expect(canApply(exportReady, "camilla")).toBe(false);
    expect(canApply({ ...applyCamillaThis, results: exportNoResult.results }, "camilla")).toBe(false);
    expect(canApply({ ...applyCamillaThis, computing: true }, "camilla")).toBe(false);
    expect(canApply({ ...applyCamillaThis, exporting: "convolution" }, "camilla")).toBe(false);
    expect(canApply({ ...applyEqapoAbsent, apply: { ...applyEqapoAbsent.apply, applying: "camilla" } }, "eqapo")).toBe(false);
    expect(canApply({ ...applyEqapoAbsent, apply: { ...applyEqapoAbsent.apply, reverting: true } }, "eqapo")).toBe(false);
    expect(exportGroupBusy({ ...exportReady, apply: { ...exportReady.apply, applying: "eqapo" } })).toBe(true);
    // An apply job holds every export off, as another export does.
    expect(canExport({ ...exportReady, apply: { ...exportReady.apply, applying: "eqapo" } }, "convolution")).toBe(false);
  });

  it("compares Equalizer APO's design with this one only once this session knows this one's id", () => {
    expect(eqapoDesign(applyEqapoAbsent)).toBe("none");
    expect(eqapoDesign(applyEqapoPresent)).toBe("this");
    expect(eqapoDesign(applyEqapoOther)).toBe("other");
    expect(eqapoDesign(applyEqapoOfficialUnknown)).toBe("unknown");
    expect(currentDesignId(applyCamillaThis)).toBe(MOCK_DESIGN_ID);
    expect(currentDesignId(applyCamillaOther)).toBeNull();
  });

  it("counts an apply as finishing the step, like an export", () => {
    expect(exportStepState(applyCamillaThis)).toEqual({ status: "ready" });
    expect(exportStepState(applyCamillaApplied)).toEqual({ status: "done" });
    expect(exportStepState(applyCamillaRolledBack)).toEqual({ status: "ready" });
  });
});
