// The RC band a graph shades around: the primary band (its upper edge may
// not be known yet) and the extra correction bands of ADR 0024. The graph
// shades outside their union and marks the union's edges.

/**
 * The primary band `[lo, hi]` and the extra bands. Without `hi` (the
 * automatic upper limit is not known yet) the primary band is open upwards,
 * so the extra bands above `lo` fall inside it.
 */
export type PlotBand = {
  lo: number;
  hi?: number | null | undefined;
  extra?: readonly (readonly [number, number])[] | undefined;
};

export type BandLayout = {
  /** The union, sorted and merged where bands overlap or touch; an open primary ends at Infinity. */
  bands: [number, number][];
  /**
   * The union band the primary band went into. An inverted primary band (`lo`
   * above an automatic upper limit) holds no frequency; it stays a band of its
   * own here, so the graph still labels `lo` and `hi` as before.
   */
  primary: number;
};

export function bandLayout(band: PlotBand): BandLayout {
  const primary: [number, number] = [band.lo, band.hi ?? Infinity];
  const all = [primary, ...(band.extra ?? []).map(([lo, hi]) => [lo, hi] as [number, number])].sort((a, b) => a[0] - b[0]);
  const bands: [number, number][] = [];
  let primaryIndex = -1;
  for (const entry of all) {
    const [lo, hi] = entry;
    const last = bands.at(-1);
    if (last && lo <= last[1]) last[1] = Math.max(last[1], hi);
    else bands.push([lo, hi]);
    if (entry === primary) primaryIndex = bands.length - 1;
  }
  return { bands, primary: primaryIndex };
}

/** The spans to shade: below the first band, between bands and above the last one (open ends at ±Infinity). */
export function shadeSpans({ bands }: BandLayout): [number, number][] {
  const spans: [number, number][] = [];
  let from = -Infinity;
  for (const [lo, hi] of bands) {
    spans.push([from, lo]);
    from = hi;
  }
  if (from !== Infinity) spans.push([from, Infinity]);
  return spans;
}

/** The edges to mark with a dashed line: every finite edge of the union. */
export const bandEdges = ({ bands }: BandLayout): number[] => bands.flatMap(([lo, hi]) => (Number.isFinite(hi) ? [lo, hi] : [lo]));

/** A key that changes when the band does (the graph restyles in place on it). */
export const bandKey = (band: PlotBand | null | undefined): string => (band ? JSON.stringify([band.lo, band.hi ?? null, band.extra ?? []]) : "");
