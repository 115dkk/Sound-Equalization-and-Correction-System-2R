// The log-frequency axis has fixed ticks (upstream plot.py FREQ_TICKS): the
// default log ticks overlapped at wide windows and switched to exponents at
// narrow ones.
export const FREQ_TICKS = [5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000] as const;

/** Default frequency range of measurement graphs (upstream step 5). */
export const FREQ_RANGE: readonly [number, number] = [5, 22000];

/** 20 → "20", 1000 → "1k", 2500 → "2.5k". */
export function formatHz(hz: number): string {
  if (hz >= 1000) {
    const k = hz / 1000;
    return `${Number.isInteger(k) ? k : Number(k.toFixed(k >= 10 ? 1 : 2))}k`;
  }
  return `${Number.isInteger(hz) ? hz : Number(hz.toFixed(hz >= 100 ? 0 : 1))}`;
}

/** Narrowest room per label before the axis falls back to decades (CSS px). */
export const MIN_TICK_SPACING = 26;

/**
 * The fixed ticks inside [min, max] (inclusive, with float slack). When the
 * axis is too narrow for all of them, only the decades remain (10, 100, 1k,
 * 10k) so labels never overlap.
 */
export function freqTicksIn(min: number, max: number, widthPx = Infinity): number[] {
  const all = FREQ_TICKS.filter((tick) => tick >= min * 0.999 && tick <= max * 1.001);
  if (all.length < 2 || widthPx / all.length >= MIN_TICK_SPACING) return all;
  const decades = all.filter((tick) => Number.isInteger(Math.log10(tick)));
  return decades.length >= 2 ? decades : all;
}

/** 1/n-octave step in frequency: hz × 2^(steps/n). */
export function octaveStep(hz: number, steps: number, perOctave = 24): number {
  return hz * 2 ** (steps / perOctave);
}
