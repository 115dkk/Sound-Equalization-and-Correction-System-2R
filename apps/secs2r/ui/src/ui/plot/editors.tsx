// Things the user moves on a graph: target points and the XO line. Both are
// drawn in the plot's overlay with the scale LogPlot hands over, both work
// with the pointer and the keyboard, and both use the accent because they
// are the interactive parts of the graph.
//
// Both report twice: `onChange` with every step of a gesture (the parent
// holds that value and passes it back, so the graph follows the pointer),
// and `onCommit` once when the gesture ends: the pointer is released after a
// drag, the key is released after nudges (a held key repeats), or focus
// leaves. A removal is a gesture of its own and commits at once. Screens
// store the value in `onCommit`, so one drag is one change.
import { useRef, useState, type KeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";
import { useNumber, useT } from "../../i18n/react";
import type { PlotScale } from "./LogPlot";
import { canRemove, movePoint, nudgePoint, removePoint, targetAt, type TargetPoint } from "./targetEdit";
import { formatHz } from "./ticks";

/* ── Target points ──────────────────────────────────────────────────────── */

export function TargetHandles({
  id,
  points,
  onChange,
  onCommit,
  scale,
  disabled,
}: {
  id: string;
  points: readonly TargetPoint[];
  /** Every step of a drag or a nudge, and removals. */
  onChange: (points: TargetPoint[]) => void;
  /** Once per gesture, with the points it ended on. */
  onCommit?: ((points: TargetPoint[]) => void) | undefined;
  scale: PlotScale;
  disabled?: boolean;
}) {
  const t = useT();
  const number = useNumber();
  const [dragging, setDragging] = useState<number | null>(null);
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  // The points the running gesture last reported; committed when it ends.
  const pending = useRef<TargetPoint[] | null>(null);
  const report = (next: TargetPoint[]) => {
    pending.current = next;
    onChange(next);
  };
  const commit = () => {
    const next = pending.current;
    pending.current = null;
    if (next) onCommit?.(next);
  };
  const remove = (index: number) => {
    const next = removePoint(points, index);
    pending.current = null;
    onChange(next);
    onCommit?.(next);
  };

  // The dashed target line, sampled like the service samples it (targetAt).
  const steps = 96;
  const [x0, x1] = scale.x;
  const path: string[] = [];
  for (let i = 0; i <= steps; i++) {
    const hz = x0 * (x1 / x0) ** (i / steps);
    path.push(`${i === 0 ? "M" : "L"}${scale.xToPx(hz).toFixed(1)},${scale.yToPx(targetAt(points, hz)).toFixed(1)}`);
  }

  const move = (index: number, event: ReactPointerEvent<HTMLButtonElement>) => {
    const rect = (event.currentTarget.offsetParent as HTMLElement | null)?.getBoundingClientRect();
    if (!rect) return;
    report(movePoint(points, index, { hz: scale.pxToX(event.clientX - rect.left), db: scale.pxToY(event.clientY - rect.top) }));
  };
  const endDrag = (event: ReactPointerEvent<HTMLButtonElement>) => {
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    setDragging(null);
    commit();
  };

  const onKey = (index: number, event: KeyboardEvent<HTMLButtonElement>) => {
    const coarse = event.shiftKey;
    const map: Record<string, [number, number]> = {
      ArrowLeft: [coarse ? -1 / 3 : -1 / 24, 0],
      ArrowRight: [coarse ? 1 / 3 : 1 / 24, 0],
      ArrowUp: [0, coarse ? 3 : 0.5],
      ArrowDown: [0, coarse ? -3 : -0.5],
    };
    const delta = map[event.key];
    if (delta) {
      event.preventDefault();
      report(nudgePoint(points, index, delta[0], delta[1]));
    } else if ((event.key === "Delete" || event.key === "Backspace") && canRemove(points)) {
      event.preventDefault();
      remove(index);
      refs.current[Math.max(0, index - 1)]?.focus();
    }
  };

  return (
    <div className="plot-target" data-ui={id}>
      <svg className="plot-target__line" width={scale.width} height={scale.height} aria-hidden="true">
        <path d={path.join("")} />
      </svg>
      {points.map((point, index) => (
        <button
          key={index}
          ref={(node) => {
            refs.current[index] = node;
          }}
          type="button"
          className={["plot-handle", dragging === index && "plot-handle--active"].filter(Boolean).join(" ")}
          data-ui={`${id}.point.${index + 1}`}
          style={{ left: scale.xToPx(point.hz), top: scale.yToPx(point.db) }}
          disabled={disabled}
          aria-label={t("plot.target.point", { n: index + 1, hz: formatHz(point.hz), db: number(point.db, 1) })}
          aria-describedby={`${id}-help`}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.stopPropagation();
            event.currentTarget.setPointerCapture?.(event.pointerId);
            pending.current = null;
            setDragging(index);
          }}
          onPointerMove={(event) => dragging === index && move(index, event)}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
          onDoubleClick={(event) => event.stopPropagation()}
          onContextMenu={(event) => {
            event.preventDefault();
            if (canRemove(points)) remove(index);
          }}
          onKeyDown={(event) => onKey(index, event)}
          onKeyUp={commit}
          onBlur={commit}
        />
      ))}
      <p id={`${id}-help`} className="visually-hidden">
        {t("plot.target.help")}
      </p>
    </div>
  );
}

/* ── XO line ────────────────────────────────────────────────────────────── */

export function XoLine({
  id,
  hz,
  onChange,
  onCommit,
  scale,
  min = 40,
  max = 200,
  step = 5,
  disabled,
}: {
  id: string;
  hz: number;
  /** Every snapped step of a drag or a key. */
  onChange: (hz: number) => void;
  /** Once per gesture, with the frequency it ended on. */
  onCommit?: ((hz: number) => void) | undefined;
  scale: PlotScale;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
}) {
  const t = useT();
  const [dragging, setDragging] = useState(false);
  const pending = useRef<number | null>(null);
  const snap = (value: number) => Math.min(max, Math.max(min, Math.round(value / step) * step));
  const x = scale.xToPx(hz);
  const set = (value: number) => {
    const next = snap(value);
    if (next === hz) return;
    pending.current = next;
    onChange(next);
  };
  const commit = () => {
    const next = pending.current;
    pending.current = null;
    if (next !== null) onCommit?.(next);
  };
  const endDrag = (event: ReactPointerEvent<HTMLButtonElement>) => {
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    setDragging(false);
    commit();
  };
  return (
    <div
      className={["plot-xo", dragging && "plot-xo--active", disabled && "plot-xo--disabled"].filter(Boolean).join(" ")}
      style={{ left: x }}
      data-ui={id}
    >
      <span className="plot-xo__line" aria-hidden="true" />
      <button
        type="button"
        className="plot-xo__handle tabular"
        data-ui={`${id}.handle`}
        role="slider"
        aria-label={t("plot.xo.label")}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={hz}
        aria-valuetext={t("plot.xo.value", { hz: formatHz(hz) })}
        aria-orientation="horizontal"
        disabled={disabled}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.stopPropagation();
          event.currentTarget.setPointerCapture?.(event.pointerId);
          pending.current = null;
          setDragging(true);
        }}
        onPointerMove={(event) => {
          if (!dragging) return;
          const parent = (event.currentTarget.parentElement?.offsetParent as HTMLElement | null)?.getBoundingClientRect();
          if (parent) set(scale.pxToX(event.clientX - parent.left));
        }}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={(event) => event.stopPropagation()}
        onKeyUp={commit}
        onBlur={commit}
        onKeyDown={(event) => {
          const keys: Record<string, number> = {
            ArrowLeft: hz - step,
            ArrowDown: hz - step,
            ArrowRight: hz + step,
            ArrowUp: hz + step,
            PageDown: hz - step * 4,
            PageUp: hz + step * 4,
            Home: min,
            End: max,
          };
          const next = keys[event.key];
          if (next !== undefined) {
            event.preventDefault();
            set(next);
          }
        }}
      >
        {t("plot.xo.value", { hz: formatHz(hz) })}
      </button>
    </div>
  );
}
