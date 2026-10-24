// What a graph draws and how each line looks.
//
// Colour follows the speaker, never its rank: mains take --ch-1… in the
// project's role order and subs take --sub-1…, so hiding a chip never
// repaints the others. Line rules (DESIGN.md §3): position 1 (the listening
// position) bold, other positions thin and faint, subwoofers thicker than
// any main, the target dashed in the neutral ink. Hovering a chip keeps
// that channel or position at full strength and dims the rest.
export type CurveKind = "main" | "sub" | "target" | "before";

export type Curve = {
  /** Unique id (role + position + variant). */
  id: string;
  /** Speaker role ("L", "SW1"); the target uses "target". */
  role: string;
  kind: CurveKind;
  /** Measurement position, 1 = listening position. */
  position?: number;
  /** One value per frequency of the shared grid; null leaves a gap. */
  values: ArrayLike<number | null>;
};

export type Highlight = { role?: string; position?: number } | null;

export type LineRules = {
  pos1: number;
  other: number;
  otherAlpha: number;
  dimAlpha: number;
  sub: number;
  target: number;
  dashTarget: number[];
  dashBefore: number[];
};

export const DEFAULT_RULES: LineRules = {
  pos1: 2.25,
  other: 1,
  otherAlpha: 0.38,
  dimAlpha: 0.12,
  sub: 3,
  target: 2,
  dashTarget: [6, 4],
  dashBefore: [2, 3],
};

const MAIN_COLORS = 16;
const SUB_COLORS = 8;

/** Token name for each role, in project order. Unknown roles keep their slot order. */
export function colorTokens(roleOrder: readonly string[], isSub: (role: string) => boolean): Map<string, string> {
  const tokens = new Map<string, string>();
  let mains = 0;
  let subs = 0;
  for (const role of roleOrder) {
    if (tokens.has(role)) continue;
    if (isSub(role)) tokens.set(role, `--sub-${(subs++ % SUB_COLORS) + 1}`);
    else tokens.set(role, `--ch-${(mains++ % MAIN_COLORS) + 1}`);
  }
  return tokens;
}

type LineStyle = { width: number; alpha: number; dash: number[] | undefined };

export function lineStyle(curve: Curve, highlight: Highlight, rules: LineRules = DEFAULT_RULES): LineStyle {
  if (curve.kind === "target") return { width: rules.target, alpha: 1, dash: rules.dashTarget };
  const dash = curve.kind === "before" ? rules.dashBefore : undefined;
  const isSub = curve.kind === "sub";
  const position = curve.position ?? 1;
  const strong = isSub ? rules.sub : rules.pos1;
  const base: LineStyle =
    position === 1 ? { width: strong, alpha: 1, dash } : { width: isSub ? rules.other + 0.5 : rules.other, alpha: rules.otherAlpha, dash };
  if (!highlight || (highlight.role === undefined && highlight.position === undefined)) return base;
  const matches =
    (highlight.role === undefined || highlight.role === curve.role) &&
    (highlight.position === undefined || highlight.position === position);
  if (matches) return { width: Math.max(base.width, strong) + 0.5, alpha: 1, dash };
  return { width: rules.other, alpha: rules.dimAlpha, dash };
}

/**
 * A skin that draws its graphs as a scope glass (Hardware Rack, after EQ APO
 * XT's analysis monitor) sets these through the --plot-glow tokens; every
 * other skin leaves them at zero and draws the DESIGN.md §3 lines alone.
 * Widths are CSS pixels added to the trace's own width.
 */
export type ScopeRules = {
  /** Phosphor halo around a solid trace at full strength (outer pass; the inner pass is half of it). */
  glow: number;
  glowAlpha: number;
  /**
   * Afterglow of the highlighted trace: a wide, faint halo hugging the beam,
   * the persistence a slow phosphor leaves. XT fills its magnitude trace down
   * to the unity rail instead, but a room response falls far below 0 dB
   * outside its band and such a fill would flood the glass.
   */
  afterglowAlpha: number;
  /** The zero axis with its hash ticks, in the axis-figure ink. */
  zeroAlpha: number;
};

export const NO_SCOPE: ScopeRules = { glow: 0, glowAlpha: 0, afterglowAlpha: 0, zeroAlpha: 0 };

/** Phosphor burns only on solid traces at full strength: never the target, a dashed before-curve, a faint position or a dimmed channel. */
export function glows(curve: Curve, style: LineStyle): boolean {
  return curve.kind !== "target" && !style.dash && style.alpha >= 1;
}

/**
 * The traces that leave an afterglow: of the glowing traces, the one the
 * legend highlights (its listening position, or the highlighted position),
 * or the only one there is. Many washes stacked would fill the glass.
 */
export function afterglowIds(glowing: readonly Curve[], highlight: Highlight): ReadonlySet<string> {
  if (glowing.length === 1) return new Set([glowing[0]!.id]);
  if (!highlight || (highlight.role === undefined && highlight.position === undefined)) return new Set();
  const position = highlight.position ?? 1;
  return new Set(
    glowing
      .filter((curve) => (highlight.role === undefined || highlight.role === curve.role) && (curve.position ?? 1) === position)
      .map((curve) => curve.id),
  );
}

/**
 * A trace as polylines in device pixels, broken where the curve has no value
 * (the beam blanks there). Points closer than `step` pixels to the last kept
 * one are skipped, except a segment's last point: a halo several pixels wide
 * shows no detail finer than a pixel, and thousands of points per trace would
 * otherwise double the time a dense glass takes to draw.
 */
export function traceSegments(
  xs: ArrayLike<number | null | undefined>,
  values: ArrayLike<number | null | undefined>,
  toX: (x: number) => number,
  toY: (y: number) => number,
  step = 0,
): number[][] {
  const segments: number[][] = [];
  let current: number[] = [];
  let pending: [number, number] | null = null;
  const close = () => {
    if (pending) current.push(pending[0], pending[1]);
    if (current.length >= 4) segments.push(current);
    current = [];
    pending = null;
  };
  for (let i = 0; i < values.length; i++) {
    const x = xs[i];
    const y = values[i];
    if (x === null || x === undefined || y === null || y === undefined || !Number.isFinite(x) || !Number.isFinite(y)) {
      close();
      continue;
    }
    const px = toX(x);
    const py = toY(y);
    if (current.length === 0 || Math.abs(px - current[current.length - 2]!) >= step) {
      current.push(px, py);
      pending = null;
    } else {
      pending = [px, py];
    }
  }
  close();
  return segments;
}

/** Curves to draw after the legend's toggles. */
export function visibleCurves(curves: readonly Curve[], hiddenRoles: ReadonlySet<string>, hiddenPositions: ReadonlySet<number>): Curve[] {
  return curves.filter(
    (curve) => curve.kind === "target" || (!hiddenRoles.has(curve.role) && !hiddenPositions.has(curve.position ?? 1)),
  );
}
