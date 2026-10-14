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

export const MAIN_COLORS = 16;
export const SUB_COLORS = 8;

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

export type LineStyle = { width: number; alpha: number; dash: number[] | undefined };

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

/** Curves to draw after the legend's toggles. */
export function visibleCurves(curves: readonly Curve[], hiddenRoles: ReadonlySet<string>, hiddenPositions: ReadonlySet<number>): Curve[] {
  return curves.filter(
    (curve) => curve.kind === "target" || (!hiddenRoles.has(curve.role) && !hiddenPositions.has(curve.position ?? 1)),
  );
}
