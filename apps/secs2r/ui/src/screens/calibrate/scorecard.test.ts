import { describe, expect, it } from "vitest";
import type { ChannelResultLine } from "../../ipc/bindings/ChannelResultLine";
import type { LateEnergy } from "../../ipc/bindings/LateEnergy";
import type { MimoExportDecision } from "../../ipc/bindings/MimoExportDecision";
import type { MimoLateEnergy } from "../../ipc/bindings/MimoLateEnergy";
import type { ResultsSummary } from "../../ipc/bindings/ResultsSummary";
import * as ipc from "../../ipc/fixtures";
import { scorecardModel, shownMimo } from "./scorecard";

const line = (role: string, within: [number, number], gd: [number, number]): ChannelResultLine => ({
  role,
  within_1db_pct: within,
  gd_spread_ms: gd,
  peq_count: 2,
  fir_taps: null,
  xo_hz: null,
  mimo_support_count: null,
});

// The result lines in configuration order; main_roles below is in another order on purpose.
const LINES = [line("L", [38, 64], [88.4, 47.5]), line("R", [41, 65], [85.2, 47.3]), line("C", [36, 62], [90.6, 47.7])];

const late = (role: string, position: number, before_db: number, after_db: number): LateEnergy => ({ role, position, before_db, after_db });
const mimoLate = (role: string, position: number, after_mimo_db: number): MimoLateEnergy => ({ role, position, after_mimo_db });
const afterMimo = (role: string, in1_20: number, gd_20: number) => ({ role, after_mimo: { ...ipc.scorecard[0]!.after, in1_20, gd_20 } });

const LATE: LateEnergy[] = [
  late("L", 3, -24.5, -15.8),
  late("L", 1, -23.9, -16.3),
  late("L", 2, -22.2, -14.4),
  late("R", 1, -24.0, -17.0),
  late("R", 2, -21.5, -14.6),
  late("C", 2, -15.7, -14.3),
  late("C", 1, -17.0, -16.0),
];

function results({
  lateEnergy = LATE,
  mimo = "fresh",
  scorecard = [afterMimo("L", 54, 41.1), afterMimo("R", 56, 45.2), afterMimo("C", 55, 45.1)],
  mimoLateEnergy = [mimoLate("L", 1, -23.1), mimoLate("L", 2, -20.1), mimoLate("R", 1, -21.6), mimoLate("C", 2, -16.1)],
  exportDecision = "included",
}: {
  lateEnergy?: LateEnergy[];
  mimo?: "fresh" | "stale" | null;
  scorecard?: ReturnType<typeof afterMimo>[];
  mimoLateEnergy?: MimoLateEnergy[];
  exportDecision?: MimoExportDecision;
} = {}): ResultsSummary {
  return {
    calibration: { ...ipc.calibrationSummary, late_energy: lateEnergy, main_roles: ["C", "L", "R"] },
    mimo: mimo === null ? null : { ...ipc.mimoSummary, stale: mimo === "stale", scorecard, late_energy: mimoLateEnergy },
    mimo_export: mimo === null ? "none" : mimo === "stale" ? "stale" : exportDecision,
  };
}

describe("scorecardModel", () => {
  it("takes the late bass at the smallest measured position and keeps the result lines' order", () => {
    const model = scorecardModel(results({ mimo: null }), LINES);
    expect(model.mainPosition).toBe(1);
    expect(model.rows.map((row) => row.role)).toEqual(["L", "R", "C"]);
    expect(model.rows[0]).toEqual({
      role: "L",
      within: { before: 38, after: 64, mimo: null },
      gd: { before: 88.4, after: 47.5, mimo: null },
      late: { before: -23.9, after: -16.3, mimo: null },
    });
    expect(model.late).toBe(true);
    expect(model.mimo).toBe(false);
    // Without position 1 the main position is the next one measured.
    expect(scorecardModel(results({ mimo: null, lateEnergy: LATE.filter((e) => e.position !== 1) }), LINES).mainPosition).toBe(2);
  });

  it("adds the MIMO value to every figure of a role a current MIMO result has", () => {
    const model = scorecardModel(results(), LINES);
    expect(model.mimo).toBe(true);
    expect(model.rows[0]).toEqual({
      role: "L",
      within: { before: 38, after: 64, mimo: 54 },
      gd: { before: 88.4, after: 47.5, mimo: 41.1 },
      late: { before: -23.9, after: -16.3, mimo: -23.1 },
    });
    // C has a MIMO scorecard row but no MIMO late energy at position 1: the late figure stays two-valued.
    expect(model.rows[2]!.within.mimo).toBe(55);
    expect(model.rows[2]!.late).toEqual({ before: -17.0, after: -16.0, mimo: null });
  });

  it("needs the role's MIMO scorecard row for any MIMO value, the late one included", () => {
    const model = scorecardModel(results({ scorecard: [afterMimo("R", 56, 45.2)] }), LINES);
    expect(model.rows[0]!.within.mimo).toBeNull();
    expect(model.rows[0]!.gd.mimo).toBeNull();
    // L has MIMO late energy at position 1, but no MIMO scorecard row.
    expect(model.rows[0]!.late!.mimo).toBeNull();
    expect(model.rows[1]!.late!.mimo).toBe(-21.6);
    expect(model.mimo).toBe(true);
  });

  it("stays two-valued when MIMO is absent, stale, or not shown (off, too few positions)", () => {
    const cases = [results({ mimo: null }), results({ mimo: "stale" }), results({ exportDecision: "off" }), results({ exportDecision: "too_few_positions" })];
    for (const summary of cases) {
      const model = scorecardModel(summary, LINES);
      expect(model.mimo, summary.mimo_export).toBe(false);
      expect(model.rows.every((row) => row.within.mimo === null && row.gd.mimo === null && row.late?.mimo == null), summary.mimo_export).toBe(true);
      expect(model.positions?.mimo, summary.mimo_export).toBe(false);
    }
    // A MIMO result without support paths is still the one the graph draws.
    expect(shownMimo(results({ exportDecision: "no_supports" }))).not.toBeNull();
    expect(shownMimo(results({ mimo: "stale" }))).toBeNull();
  });

  it("leaves the late figure out for a role without an entry, and the column out without any", () => {
    const model = scorecardModel(results({ mimo: null, lateEnergy: LATE.filter((e) => e.role !== "R") }), LINES);
    expect(model.rows[1]!.late).toBeNull();
    expect(model.late).toBe(true);
    const none = scorecardModel(results({ mimo: null, lateEnergy: [] }), LINES);
    expect(none).toMatchObject({ late: false, mainPosition: null, positions: null });
    expect(none.rows.every((row) => row.late === null)).toBe(true);
  });

  it("lists the late bass by position in main_roles order and ascending positions, with MIMO under the same rule", () => {
    const table = scorecardModel(results(), LINES).positions!;
    expect(table.positions).toEqual([1, 2, 3]);
    expect(table.rows.map((row) => row.role)).toEqual(["C", "L", "R"]);
    const [c, l, r] = table.rows;
    expect(l!.cells).toEqual([
      { before: -23.9, after: -16.3, mimo: -23.1 },
      { before: -22.2, after: -14.4, mimo: -20.1 },
      { before: -24.5, after: -15.8, mimo: null },
    ]);
    // Positions a role was not measured at are empty.
    expect(r!.cells[2]).toBeNull();
    expect(c!.cells[0]).toEqual({ before: -17.0, after: -16.0, mimo: null });
    expect(c!.cells[1]).toEqual({ before: -15.7, after: -14.3, mimo: -16.1 });
    expect(table.mimo).toBe(true);
  });

  it("has no positions table with one measured position", () => {
    const model = scorecardModel(results({ mimo: null, lateEnergy: LATE.filter((e) => e.position === 2) }), LINES);
    expect(model.positions).toBeNull();
    expect(model.mainPosition).toBe(2);
    expect(model.rows[0]!.late).toEqual({ before: -22.2, after: -14.4, mimo: null });
  });
});
