// Measurement arrangement map (upstream ArrayLayoutView, layout_map.py).
// Screen at the top, the chair or sofa, the listening head, a 1 m circle, a
// 30 cm scale and one numbered marker per position. Position 1 (the
// listening position) is larger; a half-filled marker sits 20 cm above or
// below ear height; done positions are filled with the success colour and
// the current one with the accent. When selectable, the map is one tab stop
// and the arrow keys move the selection (←/↑ previous, →/↓ next).
import type { KeyboardEvent } from "react";
import { useT } from "../../i18n/react";
import { arrangementPoints, type MicPoint } from "./geometry";

const W = 360;
const H = 300;

/** Upstream key handling: with no selection → picks 1, ← picks the last; otherwise ±1 inside the range. */
export function nextPosition(current: number | null, key: string, count: number): number | null {
  const step = key === "ArrowLeft" || key === "ArrowUp" ? -1 : key === "ArrowRight" || key === "ArrowDown" ? 1 : 0;
  if (key === "Home") return count > 0 ? 1 : null;
  if (key === "End") return count > 0 ? count : null;
  if (step === 0 || count === 0) return null;
  if (current === null) return step > 0 ? 1 : count;
  const target = current + step;
  return target >= 1 && target <= count ? target : null;
}

function halfPath(x: number, y: number, r: number, h: 1 | -1): string {
  // Upper half for +20 cm, lower half for −20 cm.
  const sweep = h === 1 ? 1 : 0;
  return `M${x - r},${y} A${r},${r} 0 0 ${sweep} ${x + r},${y} Z`;
}

type ArrangementMapProps = {
  id: string;
  count: number;
  /** Positions already measured (1-based). */
  done?: readonly number[] | undefined;
  /** The selected or next position (1-based). */
  current?: number | null | undefined;
  onSelect?: ((position: number) => void) | undefined;
  /** Positions from the service; defaults to the upstream arrangement. */
  points?: readonly MicPoint[] | undefined;
};

export function ArrangementMap({ id, count, done = [], current = null, onSelect, points }: ArrangementMapProps) {
  const t = useT();
  const list = points ?? arrangementPoints(count);
  const k = list.length >= 13 ? 1.55 : 1.9;
  const cx = W / 2;
  const cy = H * 0.56;
  const px = (cm: number) => cx + cm * k;
  const py = (cm: number) => cy - cm * k;
  const selectable = !!onSelect;
  const doneSet = new Set(done);
  const sofa = list.length >= 13;
  const heightText = (h: number) => (h === 1 ? t("map.height.up") : h === -1 ? t("map.height.down") : t("map.height.ear"));
  const stateText = (n: number) =>
    doneSet.has(n) && n === current ? t("map.position.redo") : doneSet.has(n) ? t("map.position.done") : n === current ? t("map.position.next") : t("map.position.waiting");
  const optionId = (n: number) => `${id}-pos-${n}`;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!onSelect) return;
    const next = nextPosition(current, event.key, list.length);
    if (next !== null) {
      event.preventDefault();
      onSelect(next);
    } else if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
      event.preventDefault();
    }
  };

  return (
    <div className="map map--arrangement">
      <div
        className="map__frame"
        data-ui={id}
        id={id}
        role={selectable ? "listbox" : "img"}
        aria-label={t("map.arrangement.label", { count: list.length })}
        aria-activedescendant={selectable && current !== null ? optionId(current) : undefined}
        tabIndex={selectable ? 0 : undefined}
        onKeyDown={onKeyDown}
      >
        <svg viewBox={`0 0 ${W} ${H}`} width={W} height={H} className="map__svg">
          <rect className="map__screen" x={W * 0.2} y={8} width={W * 0.6} height={6} rx={2} />
          <text className="map__caption map__caption--screen" x={cx} y={32} textAnchor="middle">
            {t("map.screen_front")}
          </text>
          {/* The caption's meaning without words: an arrow under the screen
              pointing at it. The default skin hides it; a skin may show it in
              place of the caption. */}
          <g className="map__front" aria-hidden="true">
            <line className="map__front-shaft" x1={cx} y1={25} x2={cx} y2={38} />
            <path className="map__front-head" d={`M${cx},17 L${cx + 4},25 L${cx - 4},25 Z`} />
          </g>
          <circle className="map__guide" cx={cx} cy={cy} r={50 * k} />
          {sofa ? (
            <g className="map__furniture">
              <rect x={px(-95)} y={py(28)} width={190 * k} height={56 * k} rx={6} />
              <rect className="map__furniture-dark" x={px(-95)} y={py(-16)} width={190 * k} height={12 * k} rx={3} />
              <rect className="map__furniture-dark" x={px(-95)} y={py(28)} width={14 * k} height={56 * k} rx={3} />
              <rect className="map__furniture-dark" x={px(81)} y={py(28)} width={14 * k} height={56 * k} rx={3} />
            </g>
          ) : (
            <g className="map__furniture">
              <rect x={px(-30)} y={py(28)} width={60 * k} height={56 * k} rx={6} />
              <rect className="map__furniture-dark" x={px(-30)} y={py(-16)} width={60 * k} height={12 * k} rx={3} />
            </g>
          )}
          <circle className="map__head" cx={cx} cy={cy} r={9 * k} />
          <g className="map__scale">
            <line x1={W - 18 - 30 * k} y1={H - 16} x2={W - 18} y2={H - 16} />
            <line x1={W - 18 - 30 * k} y1={H - 21} x2={W - 18 - 30 * k} y2={H - 11} />
            <line x1={W - 18} y1={H - 21} x2={W - 18} y2={H - 11} />
            <text className="map__caption map__caption--scale" x={W - 18 - 15 * k} y={H - 24} textAnchor="middle">
              {t("map.scale_30")}
            </text>
          </g>
          {list.map((point, index) => {
            const n = index + 1;
            const x = px(point.x);
            const y = py(point.y);
            const r = n === 1 ? 13 : 11;
            const isDone = doneSet.has(n);
            const isCurrent = n === current;
            const state = isCurrent ? "current" : isDone ? "done" : "waiting";
            return (
              <g
                key={n}
                id={optionId(n)}
                className={`map-marker map-marker--${state}${n === 1 ? " map-marker--main" : ""}${isDone ? " map-marker--is-done" : ""}`}
                role={selectable ? "option" : undefined}
                aria-selected={selectable ? isCurrent : undefined}
                aria-label={t("map.position.aria", {
                  n,
                  x: Math.abs(point.x),
                  side: point.x > 0 ? t("map.side.right") : point.x < 0 ? t("map.side.left") : t("map.side.centre"),
                  y: Math.abs(point.y),
                  depth: point.y > 0 ? t("map.depth.front") : point.y < 0 ? t("map.depth.back") : t("map.depth.centre"),
                  height: heightText(point.h),
                  state: stateText(n),
                })}
                data-ui={`${id}.position.${n}`}
                onClick={selectable ? () => onSelect?.(n) : undefined}
              >
                {/* Rounded rects, not circles: rx = r draws the same disc, and a skin may square the cell (ADR 0010). */}
                {isCurrent && <rect className="map-marker__halo" x={x - r - 5} y={y - r - 5} width={2 * r + 10} height={2 * r + 10} rx={r + 5} />}
                <rect className="map-marker__hit" x={x - r - 4} y={y - r - 4} width={2 * r + 8} height={2 * r + 8} rx={r + 4} />
                <rect className="map-marker__body" x={x - r} y={y - r} width={2 * r} height={2 * r} rx={r} />
                {point.h !== 0 && <path className="map-marker__half" d={halfPath(x, y, r, point.h)} />}
                <text className="map-marker__number" x={x} y={y} textAnchor="middle" dominantBaseline="central">
                  {n}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
      <ul className="map__legend" aria-hidden="true">
        <li>
          <svg width="14" height="14" viewBox="0 0 14 14" className="map__legend-mark">
            <rect x="1" y="1" width="12" height="12" rx="6" className="map-marker__body" />
          </svg>
          {t("map.height.ear")}
        </li>
        <li>
          <svg width="14" height="14" viewBox="0 0 14 14" className="map__legend-mark">
            <rect x="1" y="1" width="12" height="12" rx="6" className="map-marker__body" />
            <path d={halfPath(7, 7, 6, 1)} className="map-marker__half" />
          </svg>
          {t("map.height.up")}
        </li>
        <li>
          <svg width="14" height="14" viewBox="0 0 14 14" className="map__legend-mark">
            <rect x="1" y="1" width="12" height="12" rx="6" className="map-marker__body" />
            <path d={halfPath(7, 7, 6, -1)} className="map-marker__half" />
          </svg>
          {t("map.height.down")}
        </li>
        <li>
          <svg width="14" height="14" viewBox="0 0 14 14" className="map__legend-mark">
            <circle cx="7" cy="7" r="6" className="map__guide" />
          </svg>
          {t("map.one_metre")}
        </li>
      </ul>
    </div>
  );
}

/** The small map on an arrangement card: positions only, decorative. */
export function ArrangementThumb({ count, width = 96 }: { count: number; width?: number }) {
  const list = arrangementPoints(count);
  const k = 0.42;
  const w = 96;
  const h = 72;
  const cx = w / 2;
  const cy = h * 0.55;
  const sofa = count >= 13;
  return (
    <svg viewBox={`0 0 ${w} ${h}`} width={width} height={(width * h) / w} className="map-thumb" aria-hidden="true">
      <rect className="map__screen" x={w * 0.2} y={3} width={w * 0.6} height={3} rx={1.5} />
      {sofa ? (
        <rect className="map-thumb__seat" x={cx - 95 * k} y={cy - 28 * k} width={190 * k} height={56 * k} rx={3} />
      ) : (
        <rect className="map-thumb__seat" x={cx - 30 * k} y={cy - 28 * k} width={60 * k} height={56 * k} rx={3} />
      )}
      {list.map((point, index) => (
        <circle
          key={index}
          className={index === 0 ? "map-thumb__dot map-thumb__dot--main" : "map-thumb__dot"}
          cx={cx + point.x * k}
          cy={cy - point.y * k}
          r={index === 0 ? 3.6 : 2.6}
        />
      ))}
    </svg>
  );
}
