// Target-curve point editing, as pure functions (upstream
// target_editor.py and step_calibration.py:1017-1109). Points live in
// 10–24000 Hz and ±30 dB, stay sorted by frequency, and there are always at
// least two. Dragging cannot pass a neighbour, so the dragged point keeps its
// index for the whole drag.
export type TargetPoint = { hz: number; db: number };

export const TARGET_LIMITS = { hzMin: 10, hzMax: 24000, dbMin: -30, dbMax: 30, minPoints: 2 } as const;

/** Closest two neighbours may get (ratio in frequency), so they stay distinct. */
const NEIGHBOUR_RATIO = 1.01;

const clamp = (value: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, value));
const round = (value: number, digits: number) => {
  const f = 10 ** digits;
  return Math.round(value * f) / f;
};

export function clampPoint(point: TargetPoint): TargetPoint {
  return {
    hz: round(clamp(point.hz, TARGET_LIMITS.hzMin, TARGET_LIMITS.hzMax), 1),
    db: round(clamp(point.db, TARGET_LIMITS.dbMin, TARGET_LIMITS.dbMax), 2),
  };
}

export function sortPoints(points: readonly TargetPoint[]): TargetPoint[] {
  return [...points].sort((a, b) => a.hz - b.hz);
}

/** Moves point `index` to `to`, clamped to the limits and between its neighbours. */
export function movePoint(points: readonly TargetPoint[], index: number, to: TargetPoint): TargetPoint[] {
  const current = points[index];
  if (!current) return [...points];
  const prev = points[index - 1];
  const next = points[index + 1];
  const lo = prev ? prev.hz * NEIGHBOUR_RATIO : TARGET_LIMITS.hzMin;
  const hi = next ? next.hz / NEIGHBOUR_RATIO : TARGET_LIMITS.hzMax;
  const moved = clampPoint({ hz: clamp(to.hz, lo, hi), db: to.db });
  return points.map((point, i) => (i === index ? moved : point));
}

/** Adds a point (clamped) and returns the list with the new point's index. */
export function addPoint(points: readonly TargetPoint[], point: TargetPoint): { points: TargetPoint[]; index: number } {
  const added = clampPoint(point);
  if (points.some((p) => Math.max(p.hz, added.hz) / Math.min(p.hz, added.hz) < NEIGHBOUR_RATIO)) {
    // Too close to an existing point: treat as a move of that point instead.
    const index = points.findIndex((p) => Math.max(p.hz, added.hz) / Math.min(p.hz, added.hz) < NEIGHBOUR_RATIO);
    return { points: points.map((p, i) => (i === index ? { hz: p.hz, db: added.db } : p)), index };
  }
  const sorted = sortPoints([...points, added]);
  return { points: sorted, index: sorted.indexOf(added) };
}

/** Removes point `index` unless that would leave fewer than two points. */
export function removePoint(points: readonly TargetPoint[], index: number): TargetPoint[] {
  if (points.length <= TARGET_LIMITS.minPoints || index < 0 || index >= points.length) return [...points];
  return points.filter((_, i) => i !== index);
}

export const canRemove = (points: readonly TargetPoint[]) => points.length > TARGET_LIMITS.minPoints;

/** Keyboard nudge: `octaves` along the axis (1/24 per arrow), `db` up or down. */
export function nudgePoint(points: readonly TargetPoint[], index: number, octaves: number, db: number): TargetPoint[] {
  const point = points[index];
  if (!point) return [...points];
  return movePoint(points, index, { hz: point.hz * 2 ** octaves, db: point.db + db });
}

/**
 * Target level at `hz` as the service samples it (secs-types target.rs
 * `TargetCurve::at`, upstream model.TargetCurve): a smoothstep in
 * log-frequency between neighbouring points, flat outside the first and
 * last point, 0 without points. Same steps, so the drawn target is the one
 * the correction aims at.
 */
export function targetAt(points: readonly TargetPoint[], hz: number): number {
  const first = points[0];
  const last = points[points.length - 1];
  if (!first || !last) return 0;
  if (hz <= first.hz) return first.db;
  if (hz >= last.hz) return last.db;
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1]!;
    const b = points[i]!;
    if (a.hz <= hz && hz <= b.hz) {
      const t = (Math.log(hz) - Math.log(a.hz)) / (Math.log(b.hz) - Math.log(a.hz));
      const w = t * t * (3 - 2 * t);
      return a.db + (b.db - a.db) * w;
    }
  }
  return 0;
}
