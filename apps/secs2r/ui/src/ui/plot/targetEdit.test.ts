import { describe, expect, it } from "vitest";
import { TARGET_LIMITS, addPoint, clampPoint, movePoint, nudgePoint, removePoint, targetAt, type TargetPoint } from "./targetEdit";

const points: TargetPoint[] = [
  { hz: 20, db: 4 },
  { hz: 200, db: 0 },
  { hz: 2000, db: -2 },
];

describe("target editing", () => {
  it("clamps points to 10–24000 Hz and ±30 dB", () => {
    expect(clampPoint({ hz: 3, db: 45 })).toEqual({ hz: TARGET_LIMITS.hzMin, db: TARGET_LIMITS.dbMax });
    expect(clampPoint({ hz: 40000, db: -99 })).toEqual({ hz: TARGET_LIMITS.hzMax, db: TARGET_LIMITS.dbMin });
  });

  it("adds a clamped point in frequency order and returns its index", () => {
    const { points: next, index } = addPoint(points, { hz: 50000, db: 40 });
    expect(index).toBe(3);
    expect(next[3]).toEqual({ hz: 24000, db: 30 });
    const middle = addPoint(points, { hz: 60, db: 2 });
    expect(middle.index).toBe(1);
    expect(middle.points.map((p) => p.hz)).toEqual([20, 60, 200, 2000]);
  });

  it("never drops below two points", () => {
    const two = removePoint(points, 1);
    expect(two).toHaveLength(2);
    expect(removePoint(two, 0)).toEqual(two);
  });

  it("drags inside the neighbours and the level limits", () => {
    const moved = movePoint(points, 1, { hz: 5000, db: -80 });
    expect(moved[1]!.hz).toBeLessThan(2000);
    expect(moved[1]!.db).toBe(-30);
    expect(moved.map((p) => p.hz)).toEqual([...moved.map((p) => p.hz)].sort((a, b) => a - b));
    const first = movePoint(points, 0, { hz: 1, db: 0 });
    expect(first[0]!.hz).toBe(10);
  });

  it("nudges by octave fractions and decibels", () => {
    const up = nudgePoint(points, 1, 1, 0.5);
    expect(up[1]!.hz).toBeCloseTo(400, 5);
    expect(up[1]!.db).toBe(0.5);
  });

  it("interpolates the target in log frequency and holds it flat outside", () => {
    expect(targetAt(points, 10)).toBe(4);
    expect(targetAt(points, 63.2455532)).toBeCloseTo(2, 5);
    expect(targetAt(points, 20000)).toBe(-2);
  });
});
