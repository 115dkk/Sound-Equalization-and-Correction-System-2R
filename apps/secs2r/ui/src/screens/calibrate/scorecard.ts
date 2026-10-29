// What the scorecard shows, derived from view.results and the result lines:
// per channel the ±1 dB share, the GD spread and the late bass at the main
// position, each before → after, with a third value after MIMO when the
// screen shows a MIMO result for that channel; and the late bass at every
// measured position for the 위치별 늦은 저음 table. Pure, so the rules are
// tested without rendering.
import type { ChannelResultLine } from "../../ipc/bindings/ChannelResultLine";
import type { LateEnergy } from "../../ipc/bindings/LateEnergy";
import type { MimoLateEnergy } from "../../ipc/bindings/MimoLateEnergy";
import type { MimoSummary } from "../../ipc/bindings/MimoSummary";
import type { ResultsSummary } from "../../ipc/bindings/ResultsSummary";

/** One figure before and after correction, and after MIMO when the screen shows it (null otherwise). */
export type ChangeValues = { before: number; after: number; mimo: number | null };

export type ScoreRow = {
  role: string;
  /** Share within ±1 dB, 20–200 Hz, in percent. */
  within: ChangeValues;
  /** Group-delay spread, 20–100 Hz, in ms. */
  gd: ChangeValues;
  /** Late 20–150 Hz energy at the main position, in dB; null without an entry. */
  late: ChangeValues | null;
};

export type LatePositions = {
  /** Measured positions in ascending order; the first is the main position. */
  positions: number[];
  /** Main roles in `main_roles` order, one cell per position (null without an entry). */
  rows: { role: string; cells: (ChangeValues | null)[] }[];
  /** Some cell has a MIMO value. */
  mimo: boolean;
};

export type ScorecardModel = {
  /** In the order of the result lines (the configuration order). */
  rows: ScoreRow[];
  /** The smallest measured position, where the engine takes the scorecard's figures; null without late-energy entries. */
  mainPosition: number | null;
  /** Some row has a late-energy entry: the scorecard has its 늦은 저음 column. */
  late: boolean;
  /** Some figure has a MIMO value: the key that names the third value is shown. */
  mimo: boolean;
  /** The late energy by position; null when it covers one position or none. */
  positions: LatePositions | null;
};

/**
 * The MIMO result the screen compares against: present, current, and the
 * one the graph draws (MIMO on and computed on enough positions, with or
 * without support paths). Off or stale, the figures stay two-valued.
 */
export function shownMimo(results: ResultsSummary): MimoSummary | null {
  const mimo = results.mimo;
  if (!mimo || mimo.stale) return null;
  return results.mimo_export === "included" || results.mimo_export === "no_supports" ? mimo : null;
}

const at = (role: string, position: number) => `${role}\u0000${position}`;

function index<T extends { role: string; position: number }>(entries: readonly T[]): Map<string, T> {
  return new Map(entries.map((entry) => [at(entry.role, entry.position), entry]));
}

export function scorecardModel(results: ResultsSummary, channels: readonly ChannelResultLine[]): ScorecardModel {
  const late: readonly LateEnergy[] = results.calibration?.late_energy ?? [];
  const mimo = shownMimo(results);
  const lateAt = index(late);
  const mimoLateAt = index<MimoLateEnergy>(mimo?.late_energy ?? []);
  const mimoScore = new Map((mimo?.scorecard ?? []).map((score) => [score.role, score.after_mimo]));
  const positions = [...new Set(late.map((entry) => entry.position))].sort((a, b) => a - b);
  const mainPosition = positions[0] ?? null;

  // A role's MIMO figures need its MIMO scorecard row; the late one also the MIMO entry at that position.
  const lateValues = (role: string, position: number): ChangeValues | null => {
    const entry = lateAt.get(at(role, position));
    if (!entry) return null;
    const after = mimoScore.has(role) ? mimoLateAt.get(at(role, position)) : undefined;
    return { before: entry.before_db, after: entry.after_db, mimo: after ? after.after_mimo_db : null };
  };

  const rows = channels.map((line): ScoreRow => {
    const score = mimoScore.get(line.role);
    return {
      role: line.role,
      within: { before: line.within_1db_pct[0], after: line.within_1db_pct[1], mimo: score ? score.in1_20 : null },
      gd: { before: line.gd_spread_ms[0], after: line.gd_spread_ms[1], mimo: score ? score.gd_20 : null },
      late: mainPosition === null ? null : lateValues(line.role, mainPosition),
    };
  });

  let table: LatePositions | null = null;
  if (positions.length > 1) {
    const measured = new Set(late.map((entry) => entry.role));
    const mains = results.calibration?.main_roles ?? [];
    // main_roles order; a role the late energy names outside it goes last rather than missing.
    const roles = [...mains, ...[...measured].filter((role) => !mains.includes(role))].filter((role) => measured.has(role));
    const tableRows = roles.map((role) => ({ role, cells: positions.map((position) => lateValues(role, position)) }));
    table = { positions, rows: tableRows, mimo: tableRows.some((row) => row.cells.some((cell) => cell?.mimo != null)) };
  }

  return {
    rows,
    mainPosition,
    late: rows.some((row) => row.late !== null),
    mimo: rows.some((row) => row.within.mimo !== null || row.gd.mimo !== null || row.late?.mimo != null),
    positions: table,
  };
}
