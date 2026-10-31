import { describe, expect, it } from "vitest";
import { bandEdges, bandKey, bandLayout, shadeSpans } from "./bands";

// The RC band with the extra bands of ADR 0024: what the graph shades and
// which edges it marks.
describe("RC band layout", () => {
  it("without extra bands shades below and above the RC band and marks its two edges, as before", () => {
    const layout = bandLayout({ lo: 20, hi: 310 });
    expect(layout).toEqual({ bands: [[20, 310]], primary: 0 });
    expect(shadeSpans(layout)).toEqual([[-Infinity, 20], [310, Infinity]]);
    expect(bandEdges(layout)).toEqual([20, 310]);
    expect(bandLayout({ lo: 20, hi: 310, extra: [] })).toEqual(layout);
  });

  it("with the upper edge open shades only below the lower edge", () => {
    const layout = bandLayout({ lo: 20, hi: null });
    expect(shadeSpans(layout)).toEqual([[-Infinity, 20]]);
    expect(bandEdges(layout)).toEqual([20]);
    // Extra bands above the lower edge fall inside the open band; one below it keeps a gap.
    const extra = bandLayout({ lo: 100, extra: [[15000, 20000], [20, 50]] });
    expect(extra.bands).toEqual([[20, 50], [100, Infinity]]);
    expect(extra.primary).toBe(1);
    expect(shadeSpans(extra)).toEqual([[-Infinity, 20], [50, 100]]);
  });

  it("shades the gaps between the RC band and the extra bands, and marks every edge", () => {
    const layout = bandLayout({ lo: 20, hi: 310, extra: [[15000, 20000], [2000, 4000]] });
    expect(layout).toEqual({ bands: [[20, 310], [2000, 4000], [15000, 20000]], primary: 0 });
    expect(shadeSpans(layout)).toEqual([[-Infinity, 20], [310, 2000], [4000, 15000], [20000, Infinity]]);
    expect(bandEdges(layout)).toEqual([20, 310, 2000, 4000, 15000, 20000]);
  });

  it("merges bands that overlap or touch, the RC band's among them", () => {
    const layout = bandLayout({ lo: 50, hi: 310, extra: [[20, 50], [250, 800], [800, 1000]] });
    expect(layout).toEqual({ bands: [[20, 1000]], primary: 0 });
    expect(shadeSpans(layout)).toEqual([[-Infinity, 20], [1000, Infinity]]);
  });

  it("keys the band so a changed extra band restyles the graph", () => {
    expect(bandKey(null)).toBe("");
    expect(bandKey({ lo: 20, hi: 310 })).not.toBe(bandKey({ lo: 20, hi: 310, extra: [[15000, 20000]] }));
    expect(bandKey({ lo: 20, hi: 310, extra: [[15000, 20000]] })).toBe(bandKey({ lo: 20, hi: 310, extra: [[15000, 20000]] }));
  });
});
