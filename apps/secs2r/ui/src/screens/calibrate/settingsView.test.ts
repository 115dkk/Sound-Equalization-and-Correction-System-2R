import { describe, expect, it } from "vitest";
import { ipcError } from "../../ipc/mock";
import { baseSettings, refusedBandEdit } from "./fixtures";
import { infraState, nextBand, normalizeSub, rangeRejection, rejectionPlace, roomVolume, subOf, upperChoice, upperEdits, withBandEdge, type RangeRejection } from "./settingsView";

// The edit rules themselves live in the service (calibration_settings_edit,
// checked against upstream by the service_settings_edit oracle); these are the
// read-side helpers the settings screen draws from.
describe("calibration settings view helpers", () => {
  it("keeps a sub's values inside the upstream limits", () => {
    expect(normalizeSub({ gain_db: 40, delay_ms: -2, allpass: [[0, 1], [5, 30], [60, 0.7], [80, 1]], hp_hz: 4, polarity: -3 })).toMatchObject({
      gain_db: 20,
      delay_ms: 0,
      allpass: [
        [10, 20],
        [60, 0.7],
      ],
      hp_hz: 10,
      polarity: -1,
    });
  });

  it("gives a sub without an entry the defaults", () => {
    expect(subOf(baseSettings, "SW9")).toMatchObject({ mode: "auto", hp_hz: 0, lp_hz: 0 });
  });

  it("reads the room volume only from three valid dimensions", () => {
    expect(roomVolume([6, 5, 3])).toBe(90);
    expect(roomVolume([0.4, 5, 3])).toBeNull();
    expect(roomVolume(null)).toBeNull();
  });

  it("reads the infra sub from the filters: none without a lone low-pass", () => {
    expect(infraState(baseSettings, ["SW1", "SW2"]).role).toBeNull();
  });

  it("chooses the RC upper limit's basis with the edits the service takes (ADR 0021)", () => {
    const manual = { ...baseSettings, rc_hi: 300, rc_hi_basis: "lenient" as const };
    expect(upperChoice(baseSettings)).toBe("strict");
    expect(upperChoice(manual)).toBe("manual");
    // An automatic basis to another: the basis alone; to 직접 입력: the limit in force.
    expect(upperEdits(baseSettings, "lenient", 154)).toEqual([{ kind: "rc_hi_basis", basis: "lenient" }]);
    expect(upperEdits(baseSettings, "agreement", 154)).toEqual([{ kind: "rc_hi_basis", basis: "agreement" }]);
    expect(upperEdits(baseSettings, "manual", 154)).toEqual([{ kind: "rc_hi", hz: 154 }]);
    // Back from 직접 입력: the basis first, then the limit cleared, even for the stored basis.
    expect(upperEdits(manual, "strict", 300)).toEqual([{ kind: "rc_hi_basis", basis: "strict" }, { kind: "rc_hi", hz: null }]);
    expect(upperEdits(manual, "lenient", 300)).toEqual([{ kind: "rc_hi_basis", basis: "lenient" }, { kind: "rc_hi", hz: null }]);
    // The current choice sends nothing.
    expect(upperEdits(baseSettings, "strict", 154)).toEqual([]);
    expect(upperEdits(manual, "manual", 300)).toEqual([]);
    // 직접 입력 never starts at or below the lower edge (the service refuses it, ADR 0024): the next 50 Hz step above it.
    expect(upperEdits({ ...baseSettings, rc_lo: 200 }, "manual", 154)).toEqual([{ kind: "rc_hi", hz: 250 }]);
    expect(upperEdits({ ...baseSettings, rc_lo: 150 }, "manual", 154)).toEqual([{ kind: "rc_hi", hz: 200 }]);
  });
});

describe("extra correction bands (ADR 0024)", () => {
  const band = (lo_hz: number, hi_hz: number) => ({ lo_hz, hi_hz });

  it("starts a new band at 15–20 kHz, or at the next span clear of the primary band and the others", () => {
    expect(nextBand(baseSettings, 310)).toEqual(band(15000, 20000));
    expect(nextBand({ ...baseSettings, rc_bands: [band(15000, 20000)] }, 310)).toEqual(band(5000, 10000));
    // A span touching a band merges with it, so the next one keeps a gap.
    expect(nextBand({ ...baseSettings, rc_bands: [band(15000, 20000), band(5000, 10000)] }, 310)).toEqual(band(1000, 2000));
    // The primary band ends at the manual upper limit, otherwise at the limit in force.
    expect(nextBand({ ...baseSettings, rc_lo: 100, rc_bands: [band(15000, 20000)] }, 6000)).toEqual(band(20, 50));
    expect(nextBand({ ...baseSettings, rc_lo: 100, rc_hi: 400, rc_bands: [band(15000, 20000)] }, 6000)).toEqual(band(5000, 10000));
    // Nothing clear (the primary band reaches 18 kHz): 15–20 kHz, still a valid band.
    expect(nextBand({ ...baseSettings, rc_hi: 18000 }, 310)).toEqual(band(15000, 20000));
    // Every start is a valid band.
    for (const bands of [[], [band(15000, 20000)], [band(1000, 2000), band(3000, 5000), band(8000, 10000)]]) {
      const next = nextBand({ ...baseSettings, rc_bands: bands }, 500);
      expect(next.lo_hz).toBeGreaterThanOrEqual(20);
      expect(next.hi_hz).toBeLessThanOrEqual(20000);
      expect(next.lo_hz).toBeLessThan(next.hi_hz);
    }
  });

  it("sends the whole list with one edge changed", () => {
    const bands = [band(2000, 4000), band(15000, 20000)];
    expect(withBandEdge(bands, 1, "lower", 12000)).toEqual([band(2000, 4000), band(12000, 20000)]);
    expect(withBandEdge(bands, 0, "upper", 5000)).toEqual([band(2000, 5000), band(15000, 20000)]);
    expect(bands).toEqual([band(2000, 4000), band(15000, 20000)]);
  });

  it("takes in place only the range edits' own refusals", () => {
    const refused = ipcError("invalid_request", "", { field: "rc_lo", reason: "rc_lo_range", min: "20", max: "200" });
    expect(rangeRejection({ kind: "rc_lo", hz: 15 }, refused)).toEqual({ edit: { kind: "rc_lo", hz: 15 }, error: refused });
    // Another edit, another field, an unknown reason or another code stays a global notice.
    expect(rangeRejection({ kind: "boost_max", db: 3 }, refused)).toBeNull();
    expect(rangeRejection({ kind: "rc_hi", hz: 10 }, refused)).toBeNull();
    expect(rangeRejection({ kind: "rc_lo", hz: 15 }, { ...refused, params: { field: "rc_lo", reason: "other" } })).toBeNull();
    expect(rangeRejection({ kind: "rc_lo", hz: 15 }, { ...refused, code: "internal" })).toBeNull();
  });

  it("finds the input a refusal concerns", () => {
    const two = { ...baseSettings, rc_bands: [band(2000, 4000), band(15000, 20000)] };
    const refusal = (edit: RangeRejection["edit"], params: Record<string, string>): RangeRejection => ({ edit, error: ipcError("invalid_request", "", { field: edit.kind, ...params }) });
    expect(rejectionPlace(refusal({ kind: "rc_lo", hz: 15 }, { reason: "rc_lo_range" }), two)).toEqual({ at: "lower" });
    // The upper edge's field while 직접 입력 is chosen, the basis choice while the limit is automatic.
    expect(rejectionPlace(refusal({ kind: "rc_hi", hz: 100 }, { reason: "rc_hi_below_lo" }), { ...two, rc_hi: 300 })).toEqual({ at: "upper" });
    expect(rejectionPlace(refusal({ kind: "rc_hi", hz: 100 }, { reason: "rc_hi_below_lo" }), two)).toEqual({ at: "basis" });
    // A band: the edge that changed, or the one outside 20–20000 Hz.
    expect(rejectionPlace(refusedBandEdit, two)).toEqual({ at: "band", index: 1, edge: "upper" });
    expect(rejectionPlace(refusal({ kind: "rc_bands", bands: [band(5000, 4000), band(15000, 20000)] }, { reason: "rc_band_range", index: "1", lo_hz: "5000", hi_hz: "4000" }), two)).toEqual({ at: "band", index: 0, edge: "lower" });
    expect(rejectionPlace(refusal({ kind: "rc_bands", bands: [band(2000, 4000), band(15000, 25000)] }, { reason: "rc_band_range", index: "2", lo_hz: "15000", hi_hz: "25000" }), two)).toEqual({ at: "band", index: 1, edge: "upper" });
    // Too many bands, or a band past the rows shown: at the add action.
    expect(rejectionPlace(refusal({ kind: "rc_bands", bands: [] }, { reason: "rc_band_count", max: "4" }), two)).toEqual({ at: "add" });
    expect(rejectionPlace(refusal({ kind: "rc_bands", bands: [] }, { reason: "rc_band_range", index: "3", lo_hz: "10", hi_hz: "50" }), two)).toEqual({ at: "add" });
  });
});
