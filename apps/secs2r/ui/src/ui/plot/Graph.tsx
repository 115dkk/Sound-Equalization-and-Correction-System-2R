// A graph with its legend and axis-range control wired together: LogPlot +
// Legend + AxisRangeButton, holding the toggles, the hover highlight and the
// chosen range. Screens give it curves and, for editing, an overlay. The
// chosen range is the graph's own unless the screen holds it (`range` and
// `onRangeChange`), for example to keep it for the run across visits.
import { useMemo, useState, type ReactNode } from "react";
import { AxisRangeButton, type AxisRangeValue } from "./AxisRange";
import { Legend, toggled, type LegendChannel } from "./Legend";
import { LogPlot, type LogPlotProps, type PlotScale } from "./LogPlot";
import type { Curve, Highlight } from "./series";
import { FREQ_RANGE } from "./ticks";

const UNSET: AxisRangeValue = { x: null, y: null };

type GraphProps = {
  id: string;
  label: string;
  freqs: ArrayLike<number>;
  curves: readonly Curve[];
  channels: readonly LegendChannel[];
  /** Every role of the project in order (colours stay put). */
  roleOrder: readonly string[];
  positions?: readonly number[] | undefined;
  xRange?: readonly [number, number] | undefined;
  /** The graph's own level range; the axis popover can override it. */
  yRange?: readonly [number, number] | null | undefined;
  yUnit?: string | undefined;
  height?: number | undefined;
  band?: LogPlotProps["band"];
  stale?: boolean | undefined;
  target?: boolean | undefined;
  onDoubleClick?: ((point: { hz: number; value: number }) => void) | undefined;
  overlay?: ((scale: PlotScale) => ReactNode) | undefined;
  /**
   * The range picked in the axis popover, held by the screen. Without it the
   * graph keeps the pick itself for as long as it is mounted.
   */
  range?: AxisRangeValue | undefined;
  /** Called with each new pick (자동 sets the limits back to null); with `range` the screen stores it. */
  onRangeChange?: ((range: AxisRangeValue) => void) | undefined;
  /** Gallery: start with these toggles. */
  initialHiddenRoles?: readonly string[] | undefined;
  initialHighlight?: Highlight | undefined;
};

export function Graph({
  id,
  label,
  freqs,
  curves,
  channels,
  roleOrder,
  positions = [],
  xRange = FREQ_RANGE,
  yRange = null,
  yUnit = "dB",
  height = 320,
  band,
  stale,
  target,
  onDoubleClick,
  overlay,
  range: heldRange,
  onRangeChange,
  initialHiddenRoles = [],
  initialHighlight = null,
}: GraphProps) {
  const [hiddenRoles, setHiddenRoles] = useState<ReadonlySet<string>>(() => new Set(initialHiddenRoles));
  const [hiddenPositions, setHiddenPositions] = useState<ReadonlySet<number>>(() => new Set());
  const [highlight, setHighlight] = useState<Highlight>(initialHighlight);
  const [ownRange, setOwnRange] = useState<AxisRangeValue>(UNSET);
  const range = heldRange ?? ownRange;
  const setRange = (next: AxisRangeValue) => {
    if (!heldRange) setOwnRange(next);
    onRangeChange?.(next);
  };
  const subs = useMemo(() => new Set(channels.filter((c) => c.sub).map((c) => c.role)), [channels]);
  const isSub = useMemo(() => (role: string) => subs.has(role), [subs]);
  const fallbackY: readonly [number, number] = yRange ?? [-35, 20];
  return (
    <div className="graph" data-ui={`${id}.graph`}>
      <LogPlot
        id={id}
        label={label}
        freqs={freqs}
        curves={curves}
        roleOrder={roleOrder}
        isSub={isSub}
        hiddenRoles={hiddenRoles}
        hiddenPositions={hiddenPositions}
        highlight={highlight}
        xRange={range.x ?? xRange}
        yRange={range.y ?? yRange}
        yUnit={yUnit}
        height={height}
        band={band ?? null}
        stale={stale}
        onDoubleClick={onDoubleClick}
        overlay={overlay}
        toolbar={<AxisRangeButton id={`${id}.axes`} value={range} defaults={{ x: xRange, y: fallbackY }} onChange={setRange} yUnit={yUnit} />}
      />
      <Legend
        id={`${id}.legend`}
        channels={channels}
        roleOrder={roleOrder}
        positions={positions}
        hiddenRoles={hiddenRoles}
        hiddenPositions={hiddenPositions}
        onToggleRole={(role) => setHiddenRoles((set) => toggled(set, role))}
        onTogglePosition={(p) => setHiddenPositions((set) => toggled(set, p))}
        onHighlight={setHighlight}
        target={target}
      />
    </div>
  );
}
