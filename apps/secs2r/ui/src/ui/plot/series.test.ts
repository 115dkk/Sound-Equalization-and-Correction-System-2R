import { describe, expect, it } from "vitest";
import { oklchToSrgb, parseColor, rgbaString } from "./color";
import { DEFAULT_RULES, afterglowIds, colorTokens, glows, lineStyle, traceSegments, visibleCurves, type Curve } from "./series";
import { FREQ_TICKS, formatHz, freqTicksIn } from "./ticks";

const curve = (role: string, kind: Curve["kind"], position = 1): Curve => ({ id: `${role}@${position}`, role, kind, position, values: [] });

describe("graph styling", () => {
  it("gives position 1 a bold line, other positions thin faint lines, and subs the thickest", () => {
    expect(lineStyle(curve("L", "main", 1), null)).toEqual({ width: DEFAULT_RULES.pos1, alpha: 1, dash: undefined });
    expect(lineStyle(curve("L", "main", 2), null)).toEqual({ width: DEFAULT_RULES.other, alpha: DEFAULT_RULES.otherAlpha, dash: undefined });
    expect(lineStyle(curve("SW1", "sub", 1), null).width).toBeGreaterThan(DEFAULT_RULES.pos1);
    expect(lineStyle(curve("target", "target"), null).dash).toEqual(DEFAULT_RULES.dashTarget);
  });

  it("dims everything but the highlighted chip", () => {
    expect(lineStyle(curve("R", "main", 1), { role: "L" }).alpha).toBe(DEFAULT_RULES.dimAlpha);
    expect(lineStyle(curve("L", "main", 2), { role: "L" }).alpha).toBe(1);
    expect(lineStyle(curve("L", "main", 2), { position: 3 }).alpha).toBe(DEFAULT_RULES.dimAlpha);
  });

  it("colours follow the role, not the visible rank", () => {
    const tokens = colorTokens(["L", "R", "SW1", "C", "SW2"], (role) => role.startsWith("SW"));
    expect(Object.fromEntries(tokens)).toEqual({ L: "--ch-1", R: "--ch-2", SW1: "--sub-1", C: "--ch-3", SW2: "--sub-2" });
    const curves = [curve("L", "main"), curve("R", "main"), curve("target", "target")];
    expect(visibleCurves(curves, new Set(["R"]), new Set()).map((c) => c.role)).toEqual(["L", "target"]);
  });

  it("lights the scope's phosphor only on solid traces at full strength", () => {
    const style = (c: Curve, highlight: Parameters<typeof lineStyle>[1] = null) => lineStyle(c, highlight);
    expect(glows(curve("L", "main", 1), style(curve("L", "main", 1)))).toBe(true);
    expect(glows(curve("SW1", "sub", 1), style(curve("SW1", "sub", 1)))).toBe(true);
    // A faint position, a dimmed channel, a dashed before-curve and the target stay plain.
    expect(glows(curve("L", "main", 2), style(curve("L", "main", 2)))).toBe(false);
    expect(glows(curve("R", "main", 1), style(curve("R", "main", 1), { role: "L" }))).toBe(false);
    expect(glows(curve("L", "before", 1), style(curve("L", "before", 1)))).toBe(false);
    expect(glows(curve("target", "target"), style(curve("target", "target")))).toBe(false);
  });

  it("leaves an afterglow on the highlighted trace or the only one, never on many at once", () => {
    const mains = [curve("L", "main", 1), curve("R", "main", 1), curve("L", "main", 2)];
    expect([...afterglowIds(mains, null)]).toEqual([]);
    expect([...afterglowIds(mains, { role: "L" })]).toEqual(["L@1"]);
    expect([...afterglowIds(mains, { role: "L", position: 2 })]).toEqual(["L@2"]);
    expect([...afterglowIds(mains, { position: 1 })]).toEqual(["L@1", "R@1"]);
    expect([...afterglowIds([curve("L", "main", 1)], null)]).toEqual(["L@1"]);
  });

  it("blanks the beam where a curve has no value", () => {
    const xs = [1, 2, 3, 4, 5, 6];
    const values = [0, 1, null, 2, Number.NaN, 3];
    expect(traceSegments(xs, values, (x) => x * 10, (y) => -y)).toEqual([[10, -0, 20, -1]]);
    expect(traceSegments(xs, [0, 1, 2, null, 4, 5], (x) => x, (y) => y)).toEqual([[1, 0, 2, 1, 3, 2], [5, 4, 6, 5]]);
    // Finer than a pixel adds nothing to a halo: the points between are skipped, the ends kept.
    const dense = Array.from({ length: 11 }, (_, i) => i / 10);
    expect(traceSegments(dense, dense, (x) => x, (y) => y * 2, 0.5)).toEqual([[0, 0, 0.5, 1, 1, 2]]);
  });

  it("reads token colours into canvas strings", () => {
    expect(parseColor("#ff8000")).toEqual([1, 128 / 255, 0, 1]);
    expect(parseColor("rgb(0 0 255 / 50%)")).toEqual([0, 0, 1, 0.5]);
    const [r, g, b] = oklchToSrgb(1, 0, 0);
    expect([r, g, b].map((v) => Math.round(v * 255))).toEqual([255, 255, 255]);
    expect(parseColor("oklch(0.5 0.088 195 / 0.2)")?.[3]).toBe(0.2);
    expect(rgbaString([1, 0, 0, 1], 0.38)).toBe("rgba(255, 0, 0, 0.38)");
    expect(parseColor("lab(50% 0 0)")).toBeNull();
  });

  it("uses the fixed frequency ticks", () => {
    expect(FREQ_TICKS).toEqual([5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]);
    expect(freqTicksIn(5, 500)).toEqual([5, 10, 20, 50, 100, 200, 500]);
    expect(FREQ_TICKS.map(formatHz)).toEqual(["5", "10", "20", "50", "100", "200", "500", "1k", "2k", "5k", "10k", "20k"]);
    expect(formatHz(2500)).toBe("2.5k");
    expect(freqTicksIn(5, 22000, 200)).toEqual([10, 100, 1000, 10000]);
    expect(freqTicksIn(5, 22000, 800)).toHaveLength(12);
  });
});
