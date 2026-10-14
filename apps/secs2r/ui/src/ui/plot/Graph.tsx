// A graph with its legend and axis-range control wired together: LogPlot +
// Legend + AxisRangeButton, holding the toggles, the hover highlight and the
// chosen range. Screens give it curves and, for editing, an overlay.
import { useMemo, useState, type ReactNode } from "react";
import { AxisRangeButton, type AxisRangeValue } from "./AxisRange";
import { Legend, toggled, type LegendChannel } from "./Legend";
import { LogPlot, type PlotScale } from "./LogPlot";
import type { Curve, Highlight } from "./series";
import { FREQ_RANGE } from "./ticks";

export type GraphProps = {
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
  band?: { lo: number; hi: number } | null | undefined;
  stale?: boolean | undefined;
  target?: boolean | undefined;
  onDoubleClick?: ((point: { hz: number; value: number }) => void) | undefined;
  overlay?: ((scale: PlotScale) => ReactNode) | undefined;
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
  initialHiddenRoles = [],
  initialHighlight = null,
}: GraphProps) {
  const [hiddenRoles, setHiddenRoles] = useState<ReadonlySet<string>>(() => new Set(initialHiddenRoles));
  const [hiddenPositions, setHiddenPositions] = useState<ReadonlySet<number>>(() => new Set());
  const [highlight, setHighlight] = useState<Highlight>(initialHighlight);
  const [range, setRange] = useState<AxisRangeValue>({ x: null, y: null });
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
