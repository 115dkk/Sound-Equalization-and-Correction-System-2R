import { describe, expect, it } from "vitest";
import type { ExportBlock } from "../../ipc/bindings/ExportBlock";
import type { ExportKind } from "../../ipc/bindings/ExportKind";
import { exportMimoNoSubs, exportNoResult, exportRcOnly, exportReady, exportStale } from "./fixtures";
import { EXPORT_KINDS, exportBlock, filterModeName, mimoInclusion, type ExportView } from "./types";

/** The sample result with its subs and MIMO switched off. */
const subsOnly: ExportView = { ...exportReady, settings: { ...exportReady.settings, mimo_on: false } };

const judged = (view: ExportView) => Object.fromEntries(EXPORT_KINDS.map((kind) => [kind, exportBlock(view, kind)])) as Record<ExportKind, ExportBlock | null>;

describe("export scope (the service's filter_export_block)", () => {
  it("lists the kinds in the order the screen draws them", () => {
    expect(EXPORT_KINDS).toEqual(["camilla_dsp", "filter_text", "equalizer_apo", "before_after_mdat", "staged_mdat"]);
  });

  it("judges every kind for subs × MIMO: text carries RC only (subs first), Equalizer APO RC and BA", () => {
    // The four fixtures are the four combinations.
    expect([subsOnly, exportReady, exportRcOnly, exportMimoNoSubs].map((view) => [view.results.calibration!.chain.subs.length > 0, mimoInclusion(view).included])).toEqual([
      [true, false],
      [true, true],
      [false, false],
      [false, true],
    ]);
    const open = { camilla_dsp: null, before_after_mdat: null, staged_mdat: null };
    expect(judged(subsOnly)).toEqual({ ...open, filter_text: "subs", equalizer_apo: null });
    expect(judged(exportReady)).toEqual({ ...open, filter_text: "subs", equalizer_apo: "mimo" });
    expect(judged(exportRcOnly)).toEqual({ ...open, filter_text: null, equalizer_apo: null });
    expect(judged(exportMimoNoSubs)).toEqual({ ...open, filter_text: "mimo", equalizer_apo: "mimo" });
  });

  it("counts MIMO only when it goes into the export, and blocks nothing without a result", () => {
    // Fewer than three positions, or a stale MIMO: left out, so it blocks nothing.
    expect(exportBlock({ ...exportMimoNoSubs, positions: 2 }, "equalizer_apo")).toBeNull();
    expect(exportBlock({ ...exportMimoNoSubs, results: { ...exportMimoNoSubs.results, mimo: { ...exportMimoNoSubs.results.mimo!, stale: true } } }, "filter_text")).toBeNull();
    expect(exportBlock(exportStale, "equalizer_apo")).toBeNull();
    expect(judged(exportNoResult)).toEqual(Object.fromEntries(EXPORT_KINDS.map((kind) => [kind, null])));
  });

  it("names the filter files' mode as upstream _filter_plan does", () => {
    expect(filterModeName({ mode: "mixed", ultra_ms: 200 })).toBe("mixed");
    expect(filterModeName({ mode: "zero", ultra_ms: 120 })).toBe("zero");
    expect(filterModeName({ mode: "ultra", ultra_ms: 200 })).toBe("ultra200");
    expect(filterModeName({ mode: "ultra", ultra_ms: 62.5 })).toBe("ultra62.5");
    expect(filterModeName({ mode: "ultra", ultra_ms: 30 })).toBe("ultra30");
  });
});
