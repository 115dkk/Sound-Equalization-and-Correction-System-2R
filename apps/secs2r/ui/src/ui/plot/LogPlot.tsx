// LogPlot: frequency response graphs on uPlot (MIT). Log-frequency x axis
// with fixed ticks 5 … 20k, colours and line rules from the tokens, an
// optional shaded band outside a range (the RC band; the upper edge may be
// open), a cursor readout, and an overlay slot for editable things (target
// points, the XO line) that gets the scale mapping. Dozens of curves of a few thousand points draw in one
// pass; hover highlight restyles the same instance, while a theme, accent or
// skin change rebuilds it so the tokens (colours and the axis font) are read again.
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { useNumber, useT } from "../../i18n/react";
import { readColor, readNumber, rgbaString, type Rgba } from "./color";
import { DEFAULT_RULES, NO_SCOPE, afterglowIds, colorTokens, glows, lineStyle, traceSegments, type Curve, type Highlight, type LineRules, type ScopeRules } from "./series";
import { FREQ_RANGE, formatHz, freqTicksIn } from "./ticks";

export type PlotScale = {
  xToPx: (hz: number) => number;
  pxToX: (px: number) => number;
  yToPx: (value: number) => number;
  pxToY: (px: number) => number;
  /** Plot area size in CSS pixels. */
  width: number;
  height: number;
  x: readonly [number, number];
  y: readonly [number, number];
};

export type LogPlotProps = {
  id: string;
  /** Accessible name of the graph. */
  label: string;
  freqs: ArrayLike<number>;
  curves: readonly Curve[];
  /** Every role of the project in order, so colours never move. */
  roleOrder: readonly string[];
  isSub: (role: string) => boolean;
  hiddenRoles?: ReadonlySet<string> | undefined;
  hiddenPositions?: ReadonlySet<number> | undefined;
  highlight?: Highlight | undefined;
  xRange?: readonly [number, number] | undefined;
  /** Fixed level range; null fits the visible curves. */
  yRange?: readonly [number, number] | null | undefined;
  yUnit?: string | undefined;
  height?: number | undefined;
  /**
   * Shade outside [lo, hi] (the RC band) and mark its edges. Without `hi`
   * (the automatic upper limit is not known yet) only the part below `lo`
   * is shaded and only that edge is marked.
   */
  band?: { lo: number; hi?: number | null | undefined } | null | undefined;
  /** A double click on empty graph space (not on an overlay control such as a target point). */
  onDoubleClick?: ((point: { hz: number; value: number }) => void) | undefined;
  overlay?: ((scale: PlotScale) => ReactNode) | undefined;
  /** Controls in the top-right corner (axis range button). */
  toolbar?: ReactNode | undefined;
  /** Results that no longer match the inputs: curves drawn faint. */
  stale?: boolean | undefined;
};

type Palette = {
  colors: Map<string, Rgba>;
  target: Rgba;
  grid: Rgba;
  axis: Rgba;
  text: Rgba;
  shade: Rgba;
  rules: LineRules;
  scope: ScopeRules;
  font: string;
};

const EMPTY_ROLES: ReadonlySet<string> = new Set();
const EMPTY_POSITIONS: ReadonlySet<number> = new Set();

/** The canvas palette and font as `element` sees them (exported for tests). */
export function readPalette(element: Element, roleOrder: readonly string[], isSub: (role: string) => boolean): Palette {
  const tokens = colorTokens(roleOrder, isSub);
  const colors = new Map<string, Rgba>();
  for (const [role, token] of tokens) colors.set(role, readColor(element, token));
  const style = getComputedStyle(element);
  const dash = (token: string, fallback: number[]) => {
    const parts = style.getPropertyValue(token).trim().split(/\s+/).map(Number).filter(Number.isFinite);
    return parts.length ? parts : fallback;
  };
  const caption = readNumber(element, "--text-caption", 13);
  const weight = readNumber(element, "--plot-weight", 400);
  const family = style.getPropertyValue("--font-ui").trim() || "system-ui, sans-serif";
  return {
    colors,
    target: readColor(element, "--plot-target"),
    grid: readColor(element, "--plot-grid"),
    axis: readColor(element, "--plot-axis"),
    text: readColor(element, "--plot-text"),
    shade: readColor(element, "--plot-shade"),
    rules: {
      pos1: readNumber(element, "--plot-w-pos1", DEFAULT_RULES.pos1),
      other: readNumber(element, "--plot-w-other", DEFAULT_RULES.other),
      otherAlpha: readNumber(element, "--plot-alpha-other", DEFAULT_RULES.otherAlpha),
      dimAlpha: readNumber(element, "--plot-alpha-dim", DEFAULT_RULES.dimAlpha),
      sub: readNumber(element, "--plot-w-sub", DEFAULT_RULES.sub),
      target: readNumber(element, "--plot-w-target", DEFAULT_RULES.target),
      dashTarget: dash("--plot-dash-target", DEFAULT_RULES.dashTarget),
      dashBefore: dash("--plot-dash-before", DEFAULT_RULES.dashBefore),
    },
    scope: {
      glow: readNumber(element, "--plot-glow", NO_SCOPE.glow),
      glowAlpha: readNumber(element, "--plot-glow-alpha", NO_SCOPE.glowAlpha),
      afterglowAlpha: readNumber(element, "--plot-afterglow-alpha", NO_SCOPE.afterglowAlpha),
      zeroAlpha: readNumber(element, "--plot-zero-alpha", NO_SCOPE.zeroAlpha),
    },
    font: `${weight} ${caption}px ${family}`,
  };
}

const pathOf = (ctx: CanvasRenderingContext2D, segment: readonly number[]) => {
  ctx.moveTo(segment[0]!, segment[1]!);
  for (let i = 2; i < segment.length; i += 2) ctx.lineTo(segment[i]!, segment[i + 1]!);
};

/**
 * The scope glass of a skin that sets the --plot-glow tokens, drawn under
 * the traces uPlot strokes next: the zero axis with the scope's fine hash
 * ticks, the afterglow around the highlighted trace, and the phosphor halo
 * around every solid trace (stroke overpainting, two passes, like EQ APO
 * XT's analysis monitor, only finer). Stale results draw no phosphor: their
 * beam is already faint.
 */
function drawScope(u: uPlot, state: { curves: readonly Curve[]; highlight: Highlight; stale: boolean; palette: Palette | null }) {
  const palette = state.palette;
  if (!palette) return;
  const scope = palette.scope;
  if (scope.glow <= 0 && scope.afterglowAlpha <= 0 && scope.zeroAlpha <= 0) return;
  const lo = u.scales.y?.min;
  const hi = u.scales.y?.max;
  if (typeof lo !== "number" || typeof hi !== "number" || !Number.isFinite(lo) || !Number.isFinite(hi)) return;
  const { ctx } = u;
  const { left, top, width, height } = u.bbox;
  const ratio = devicePixelRatio || 1;
  const zeroY = u.valToPos(0, "y", true);
  ctx.save();
  ctx.beginPath();
  ctx.rect(left, top, width, height);
  ctx.clip();
  if (scope.zeroAlpha > 0 && lo < 0 && hi > 0) {
    const y = Math.round(zeroY) + 0.5;
    ctx.lineWidth = Math.max(1, Math.round(ratio));
    ctx.strokeStyle = rgbaString(palette.text, scope.zeroAlpha);
    ctx.beginPath();
    ctx.moveTo(left, y);
    ctx.lineTo(left + width, y);
    ctx.stroke();
    ctx.strokeStyle = rgbaString(palette.text, scope.zeroAlpha * 0.45);
    ctx.beginPath();
    for (let x = left + 4 * ratio; x < left + width - 2 * ratio; x += 7 * ratio) {
      const px = Math.round(x) + 0.5;
      ctx.moveTo(px, y - 2 * ratio);
      ctx.lineTo(px, y + 2 * ratio);
    }
    ctx.stroke();
  }
  if (!state.stale && (scope.glow > 0 || scope.afterglowAlpha > 0)) {
    const xs = u.data[0];
    const traces = state.curves
      .map((curve, index) => ({ curve, index, style: lineStyle(curve, state.highlight, palette.rules) }))
      .filter(({ curve, index, style }) => u.series[index + 1]?.show && glows(curve, style));
    const persistent = afterglowIds(traces.map(({ curve }) => curve), state.highlight);
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    for (const { curve, index, style } of traces) {
      const values = u.data[index + 1];
      if (!values) continue;
      const color = palette.colors.get(curve.role) ?? palette.text;
      const segments = traceSegments(xs, values, (x) => u.valToPos(x, "x", true), (y) => u.valToPos(y, "y", true), ratio);
      // Widest first: the afterglow (three halos wide), then the outer and inner halo.
      const passes: (readonly [number, number])[] = [];
      if (scope.afterglowAlpha > 0 && persistent.has(curve.id)) passes.push([scope.glow * 3, scope.afterglowAlpha]);
      if (scope.glow > 0) passes.push([scope.glow, scope.glowAlpha * 0.5], [scope.glow / 2, scope.glowAlpha]);
      for (const [extra, alpha] of passes) {
        ctx.lineWidth = (style.width + extra) * ratio;
        ctx.strokeStyle = rgbaString(color, alpha);
        ctx.beginPath();
        for (const segment of segments) pathOf(ctx, segment);
        ctx.stroke();
      }
    }
  }
  ctx.restore();
}

function niceRange(curves: readonly Curve[], hiddenRoles: ReadonlySet<string>, hiddenPositions: ReadonlySet<number>): [number, number] {
  let lo = Infinity;
  let hi = -Infinity;
  for (const curve of curves) {
    if (hiddenRoles.has(curve.role) || hiddenPositions.has(curve.position ?? 1)) continue;
    for (let i = 0; i < curve.values.length; i++) {
      const v = curve.values[i];
      if (v === null || v === undefined || !Number.isFinite(v)) continue;
      if (v < lo) lo = v;
      if (v > hi) hi = v;
    }
  }
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) return [-30, 10];
  const pad = Math.max(2, (hi - lo) * 0.08);
  return [Math.floor((lo - pad) / 5) * 5, Math.ceil((hi + pad) / 5) * 5];
}

export function LogPlot({
  id,
  label,
  freqs,
  curves,
  roleOrder,
  isSub,
  hiddenRoles = EMPTY_ROLES,
  hiddenPositions = EMPTY_POSITIONS,
  highlight = null,
  xRange = FREQ_RANGE,
  yRange = null,
  yUnit = "dB",
  height = 320,
  band = null,
  onDoubleClick,
  overlay,
  toolbar,
  stale,
}: LogPlotProps) {
  const t = useT();
  const number = useNumber();
  const descriptionId = useId();
  const wrap = useRef<HTMLDivElement>(null);
  const host = useRef<HTMLDivElement>(null);
  const plot = useRef<uPlot | null>(null);
  const [over, setOver] = useState<HTMLDivElement | null>(null);
  const [version, setVersion] = useState(0);
  // Bumped when the theme, accent or skin around the graph changes: the chart is rebuilt with the tokens read again.
  const [themeTick, setThemeTick] = useState(0);
  const [cursor, setCursor] = useState<{ hz: number; value: number } | null>(null);

  // Everything the draw callbacks read lives in one ref, so a prop change
  // restyles without rebuilding the chart.
  const live = useRef({
    curves,
    highlight,
    hiddenRoles,
    hiddenPositions,
    band,
    yRange,
    xRange,
    stale: !!stale,
    palette: null as Palette | null,
    onDoubleClick,
  });
  live.current = { ...live.current, curves, highlight, hiddenRoles, hiddenPositions, band, yRange, xRange, stale: !!stale, onDoubleClick };

  // The level range: fixed when given, otherwise fitted to the visible curves.
  const currentY = (): readonly [number, number] =>
    live.current.yRange ?? niceRange(live.current.curves, live.current.hiddenRoles, live.current.hiddenPositions);

  const structure = useMemo(() => `${freqs.length}:${curves.map((curve) => `${curve.id}/${curve.kind}`).join("|")}`, [freqs, curves]);
  const roleKey = roleOrder.join("|");

  /** Restyles in place with the palette the build read. */
  const applyStyles = () => {
    const u = plot.current;
    const palette = live.current.palette;
    if (!u || !palette) return;
    live.current.curves.forEach((curve, index) => {
      const series = u.series[index + 1];
      if (!series) return;
      const style = lineStyle(curve, live.current.highlight, palette.rules);
      series.width = style.width;
      series.dash = style.dash ?? [];
    });
    u.redraw(false, false);
  };

  // Build (or rebuild when the set of curves changes).
  useLayoutEffect(() => {
    const element = host.current;
    const outer = wrap.current;
    if (!element || !outer) return;
    // uPlot fires setScale and setSize while constructing; the build bumps the version once, after.
    let building = false;
    live.current.palette = readPalette(outer, roleOrder, isSub);
    const color = (curve: Curve): Rgba =>
      curve.kind === "target" ? live.current.palette!.target : (live.current.palette!.colors.get(curve.role) ?? live.current.palette!.text);
    const stroke = (curve: Curve) => () => {
      const style = lineStyle(curve, live.current.highlight, live.current.palette!.rules);
      return rgbaString(color(curve), style.alpha * (live.current.stale && curve.kind !== "target" ? 0.45 : 1));
    };
    const paletteColor = (key: "grid" | "axis" | "text") => () => rgbaString(live.current.palette![key]);
    const series: uPlot.Series[] = [
      {},
      ...curves.map((curve) => {
        const style = lineStyle(curve, highlight, live.current.palette!.rules);
        return {
          label: curve.id,
          stroke: stroke(curve),
          width: style.width,
          ...(style.dash ? { dash: style.dash } : {}),
          points: { show: false },
          spanGaps: false,
          show: curve.kind === "target" || (!hiddenRoles.has(curve.role) && !hiddenPositions.has(curve.position ?? 1)),
        } satisfies uPlot.Series;
      }),
    ];
    const options: uPlot.Options = {
      width: Math.max(200, outer.clientWidth),
      height,
      legend: { show: false },
      cursor: {
        drag: { x: false, y: false, setScale: false },
        points: { show: false },
        focus: { prox: -1 },
        y: true,
      },
      select: { show: false, left: 0, top: 0, width: 0, height: 0 },
      scales: {
        x: {
          time: false,
          distr: 3,
          log: 10,
          range: () => [live.current.xRange[0], live.current.xRange[1]],
        },
        y: {
          range: () => {
            const [lo, hi] = currentY();
            return [lo, hi];
          },
        },
      },
      axes: [
        {
          scale: "x",
          stroke: paletteColor("text"),
          font: live.current.palette.font,
          size: 30,
          gap: 4,
          splits: (u, _axis, min, max) => freqTicksIn(min, max, u.bbox.width / devicePixelRatio),
          // Keep every fixed tick (uPlot's log filter would drop some).
          filter: (_u, splits) => splits,
          values: (_u, splits) => splits.map((value) => (value === null || value === undefined ? "" : formatHz(value))),
          grid: { stroke: paletteColor("grid"), width: 1 },
          ticks: { stroke: paletteColor("axis"), width: 1, size: 4 },
          border: { show: true, stroke: paletteColor("axis"), width: 1 },
        },
        {
          scale: "y",
          stroke: paletteColor("text"),
          font: live.current.palette.font,
          size: 46,
          gap: 6,
          values: (_u, splits) => splits.map((value) => (value === null || value === undefined ? "" : number(value, Number.isInteger(value) ? 0 : 1))),
          grid: { stroke: paletteColor("grid"), width: 1 },
          ticks: { stroke: paletteColor("axis"), width: 1, size: 4 },
          border: { show: true, stroke: paletteColor("axis"), width: 1 },
        },
      ],
      series,
      hooks: {
        drawAxes: [
          (u) => drawScope(u, live.current),
          (u) => {
            const range = live.current.band;
            const palette = live.current.palette;
            if (!range || !palette) return;
            const { ctx } = u;
            const { left, top, width, height: h } = u.bbox;
            const clampX = (hz: number) => Math.max(left, Math.min(left + width, u.valToPos(hz, "x", true)));
            const xLo = clampX(range.lo);
            const xHi = range.hi === null || range.hi === undefined ? null : clampX(range.hi);
            ctx.save();
            ctx.fillStyle = rgbaString(palette.shade);
            ctx.fillRect(left, top, xLo - left, h);
            if (xHi !== null) ctx.fillRect(xHi, top, left + width - xHi, h);
            ctx.strokeStyle = rgbaString(palette.axis);
            ctx.lineWidth = Math.max(1, devicePixelRatio);
            ctx.setLineDash([4 * devicePixelRatio, 3 * devicePixelRatio]);
            for (const x of xHi === null ? [xLo] : [xLo, xHi]) {
              ctx.beginPath();
              ctx.moveTo(Math.round(x) + 0.5, top);
              ctx.lineTo(Math.round(x) + 0.5, top + h);
              ctx.stroke();
            }
            ctx.restore();
          },
        ],
        setScale: [() => { if (!building) setVersion((v) => v + 1); }],
        setSize: [() => { if (!building) setVersion((v) => v + 1); }],
        setCursor: [
          (u) => {
            const { left, top } = u.cursor;
            if (left === undefined || top === undefined || left < 0 || top < 0) setCursor(null);
            else setCursor({ hz: u.posToVal(left, "x"), value: u.posToVal(top, "y") });
          },
        ],
      },
    };
    const data: uPlot.AlignedData = [freqs as number[], ...curves.map((curve) => curve.values as (number | null)[])];
    building = true;
    const u = new uPlot(options, data, element);
    building = false;
    plot.current = u;
    const overEl = u.over as HTMLDivElement;
    const onDbl = (event: MouseEvent) => {
      const handler = live.current.onDoubleClick;
      if (!handler) return;
      // The overlay lets pointer events through only to its controls (target
      // points, the XO handle); their double clicks are theirs. The listener
      // is native, so a React stopPropagation on the control comes too late.
      if (event.target instanceof Element && event.target.closest(".plot__overlay")) return;
      const rect = overEl.getBoundingClientRect();
      handler({ hz: u.posToVal(event.clientX - rect.left, "x"), value: u.posToVal(event.clientY - rect.top, "y") });
    };
    overEl.addEventListener("dblclick", onDbl);
    // The tokens were read just now, so from here on watch the theme, accent and
    // skin attributes on every ancestor (the app sets them on <html>, the gallery
    // scopes them to subtrees) and the OS colour scheme; any change rebuilds the
    // chart. Attached here, in the layout effect, so an attribute the host applies
    // in a later effect of this same commit (PrefsScope does) is not missed.
    const retheme = () => setThemeTick((tick) => tick + 1);
    const media = window.matchMedia?.("(prefers-color-scheme: dark)");
    media?.addEventListener?.("change", retheme);
    const themeObserver = typeof MutationObserver === "undefined" ? null : new MutationObserver(retheme);
    for (let node: HTMLElement | null = outer; node && themeObserver; node = node.parentElement) {
      themeObserver.observe(node, { attributes: true, attributeFilter: ["data-theme", "data-accent", "data-skin"] });
    }
    setOver(overEl);
    setVersion((v) => v + 1);
    return () => {
      media?.removeEventListener?.("change", retheme);
      themeObserver?.disconnect();
      overEl.removeEventListener("dblclick", onDbl);
      u.destroy();
      plot.current = null;
      setOver(null);
    };
    // Rebuild when the set of curves, the grid, the role order or the theme changes.
  }, [structure, roleKey, height, themeTick]);

  // New values on the same curves: swap the data, keep the instance.
  const firstData = useRef(true);
  useEffect(() => {
    if (firstData.current) {
      firstData.current = false;
      return;
    }
    plot.current?.setData([freqs as number[], ...curves.map((curve) => curve.values as (number | null)[])], true);
  }, [curves, freqs]);

  // Visibility toggles.
  useEffect(() => {
    const u = plot.current;
    if (!u) return;
    u.batch(() => {
      curves.forEach((curve, index) => {
        const show = curve.kind === "target" || (!hiddenRoles.has(curve.role) && !hiddenPositions.has(curve.position ?? 1));
        if (u.series[index + 1]?.show !== show) u.setSeries(index + 1, { show });
      });
      if (!yRange) {
        const [min, max] = currentY();
        u.setScale("y", { min, max });
      }
    });
  }, [curves, hiddenRoles, hiddenPositions, yRange, structure]);

  // Ranges. An explicit setScale bypasses the scale's range function, so
  // pass the computed limits.
  useEffect(() => {
    const u = plot.current;
    if (!u) return;
    const [min, max] = currentY();
    u.batch(() => {
      u.setScale("x", { min: xRange[0], max: xRange[1] });
      u.setScale("y", { min, max });
    });
  }, [xRange[0], xRange[1], yRange?.[0], yRange?.[1]]);

  // Highlight, staleness and band: restyle in place.
  useEffect(() => applyStyles(), [highlight, stale, band?.lo, band?.hi]);

  // Width follows the container.
  useEffect(() => {
    const outer = wrap.current;
    if (!outer || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      const u = plot.current;
      if (u && outer.clientWidth > 0 && Math.abs(u.width - outer.clientWidth) > 0.5) u.setSize({ width: outer.clientWidth, height });
    });
    observer.observe(outer);
    return () => observer.disconnect();
  }, [height]);

  const scale = useMemo<PlotScale | null>(() => {
    const u = plot.current;
    if (!u || !over) return null;
    const xs = u.scales.x;
    const ys = u.scales.y;
    // While an instance is still being built (its hooks fire before it has
    // data) there is no mapping: the overlay waits for the next version
    // rather than drawing at Infinity.
    if (![xs?.min, xs?.max, ys?.min, ys?.max].every((v) => typeof v === "number" && Number.isFinite(v))) return null;
    return {
      xToPx: (hz) => u.valToPos(hz, "x"),
      pxToX: (px) => u.posToVal(px, "x"),
      yToPx: (value) => u.valToPos(value, "y"),
      pxToY: (px) => u.posToVal(px, "y"),
      width: over.clientWidth,
      height: over.clientHeight,
      x: [xs?.min ?? xRange[0], xs?.max ?? xRange[1]],
      y: [ys?.min ?? 0, ys?.max ?? 1],
    };
  }, [version, over]);

  const visibleCount = curves.filter(
    (curve) => curve.kind === "target" || (!hiddenRoles.has(curve.role) && !hiddenPositions.has(curve.position ?? 1)),
  ).length;

  return (
    <figure className={["plot", stale && "plot--stale"].filter(Boolean).join(" ")} data-ui={id} id={id} aria-label={label} aria-describedby={descriptionId}>
      <p id={descriptionId} className="visually-hidden">
        {t("plot.summary", { count: visibleCount, lo: formatHz(xRange[0]), hi: formatHz(xRange[1]) })}
      </p>
      <div className="plot__frame" ref={wrap}>
        <span className="plot__unit" aria-hidden="true">
          {yUnit}
        </span>
        {toolbar && <div className="plot__toolbar">{toolbar}</div>}
        <div className="plot__canvas" ref={host} />
        <div className="plot__readout tabular" aria-hidden="true">
          {cursor && Number.isFinite(cursor.hz)
            ? t("plot.readout", { hz: formatHz(cursor.hz), value: number(cursor.value, 1), unit: yUnit })
            : ""}
        </div>
      </div>
      {over && scale && overlay && createPortal(<div className="plot__overlay">{overlay(scale)}</div>, over)}
    </figure>
  );
}
