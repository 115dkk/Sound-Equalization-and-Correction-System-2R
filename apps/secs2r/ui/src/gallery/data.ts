// Synthetic but plausible room responses for the gallery: a log grid, low
// room modes that move with the position, a main-speaker roll-off below
// about 45 Hz, a subwoofer low-pass, a gentle treble tilt and smooth ripple.
// Deterministic (seeded), so screenshots are stable.
import type { Curve } from "../ui/plot/series";

export function logGrid(lo: number, hi: number, n: number): Float64Array {
  const grid = new Float64Array(n);
  for (let i = 0; i < n; i++) grid[i] = lo * (hi / lo) ** (i / (n - 1));
  return grid;
}

function rng(seed: number) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13;
    s ^= s >>> 17;
    s ^= s << 5;
    return (s >>> 0) / 4294967296;
  };
}

const hash = (text: string) => [...text].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7);

export function response(freqs: ArrayLike<number>, role: string, position: number, sub: boolean): Float64Array {
  const r = rng(hash(role) * 131 + position * 977);
  const modes = Array.from({ length: 5 }, (_, i) => ({
    f: [34, 52, 78, 115, 170][i]! * (0.92 + r() * 0.16),
    gain: (r() - 0.45) * (sub ? 16 : 12) * (position === 1 ? 1 : 1.2),
    width: 0.06 + r() * 0.08,
  }));
  const ripple = Array.from({ length: 4 }, () => ({ k: 3 + r() * 9, phase: r() * Math.PI * 2, amp: 0.4 + r() * 1.2 }));
  const tilt = -0.8 - r() * 0.8;
  const offset = (r() - 0.5) * 2;
  const out = new Float64Array(freqs.length);
  for (let i = 0; i < freqs.length; i++) {
    const f = freqs[i] ?? 1;
    const lf = Math.log10(f);
    let db = offset;
    for (const mode of modes) db += mode.gain * Math.exp(-(((lf - Math.log10(mode.f)) / mode.width) ** 2));
    for (const w of ripple) db += w.amp * Math.sin(lf * w.k * Math.PI + w.phase) * (f > 300 ? 1 : 0.4);
    if (sub) {
      db += -24 * Math.log2(Math.max(1, f / 95)) - 18 * Math.log2(Math.max(1, 18 / f));
    } else {
      db += -14 * Math.log2(Math.max(1, 48 / f));
      db += tilt * Math.log2(Math.max(1, f / 1500));
    }
    out[i] = Math.max(-60, db);
  }
  return out;
}

export function measurementCurves(freqs: ArrayLike<number>, roles: readonly string[], positions: number, isSub: (role: string) => boolean): Curve[] {
  const curves: Curve[] = [];
  for (let p = 1; p <= positions; p++) {
    for (const role of roles) {
      const sub = isSub(role);
      curves.push({ id: `${role}@${p}`, role, position: p, kind: sub ? "sub" : "main", values: response(freqs, role, p, sub) });
    }
  }
  return curves;
}
