// Graph legend: a chip per channel and a chip per measurement position.
// A chip shows or hides its curves (aria-pressed) and, while hovered or
// focused, highlights them and dims the rest. Position chips appear only
// with more than one position; position 1 is the listening position and
// its curves are the bold ones.
import { useT } from "../../i18n/react";
import { colorTokens, type Highlight } from "./series";

export type LegendChannel = { role: string; label: string; sub: boolean };

export type LegendProps = {
  id: string;
  channels: readonly LegendChannel[];
  roleOrder: readonly string[];
  positions: readonly number[];
  hiddenRoles: ReadonlySet<string>;
  hiddenPositions: ReadonlySet<number>;
  onToggleRole: (role: string) => void;
  onTogglePosition: (position: number) => void;
  onHighlight: (highlight: Highlight) => void;
  /** Show the dashed target entry. */
  target?: boolean | undefined;
};

export function Legend({
  id,
  channels,
  roleOrder,
  positions,
  hiddenRoles,
  hiddenPositions,
  onToggleRole,
  onTogglePosition,
  onHighlight,
  target,
}: LegendProps) {
  const t = useT();
  const subs = new Set(channels.filter((channel) => channel.sub).map((channel) => channel.role));
  const tokens = colorTokens(roleOrder, (role) => subs.has(role));
  const leave = () => onHighlight(null);
  return (
    <div className="plot-legend" data-ui={id}>
      <div className="plot-legend__row" role="group" aria-label={t("plot.legend.channels")}>
        {channels.map((channel) => {
          const shown = !hiddenRoles.has(channel.role);
          return (
            <button
              key={channel.role}
              type="button"
              className={["plot-chip", channel.sub && "plot-chip--sub"].filter(Boolean).join(" ")}
              data-ui={`${id}.role.${channel.role}`}
              aria-pressed={shown}
              onClick={() => onToggleRole(channel.role)}
              onPointerEnter={() => shown && onHighlight({ role: channel.role })}
              onPointerLeave={leave}
              onFocus={() => shown && onHighlight({ role: channel.role })}
              onBlur={leave}
            >
              <span className="plot-chip__swatch" style={{ background: `var(${tokens.get(channel.role) ?? "--text-muted"})` }} aria-hidden="true" />
              <span className="plot-chip__label">{channel.label}</span>
            </button>
          );
        })}
        {target && (
          <span className="plot-legend__static">
            <span className="plot-chip__swatch plot-chip__swatch--target" aria-hidden="true" />
            {t("plot.legend.target")}
          </span>
        )}
      </div>
      {positions.length > 1 && (
        <div className="plot-legend__row" role="group" aria-label={t("plot.legend.positions")}>
          <span className="plot-legend__caption">{t("plot.legend.position")}</span>
          {positions.map((position) => {
            const shown = !hiddenPositions.has(position);
            return (
              <button
                key={position}
                type="button"
                className={["plot-chip", "plot-chip--position", position === 1 && "plot-chip--main"].filter(Boolean).join(" ")}
                data-ui={`${id}.position.${position}`}
                aria-pressed={shown}
                aria-label={t("plot.legend.position_n", { n: position })}
                onClick={() => onTogglePosition(position)}
                onPointerEnter={() => shown && onHighlight({ position })}
                onPointerLeave={leave}
                onFocus={() => shown && onHighlight({ position })}
                onBlur={leave}
              >
                <span className="plot-chip__label tabular">{position}</span>
              </button>
            );
          })}
          <span className="plot-legend__note">{t("plot.legend.bold_note")}</span>
        </div>
      )}
    </div>
  );
}

/** Toggle helper for the hidden sets kept by the screen. */
export function toggled<T>(set: ReadonlySet<T>, value: T): Set<T> {
  const next = new Set(set);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  return next;
}
