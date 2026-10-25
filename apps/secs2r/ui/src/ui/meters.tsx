// Level meter and progress bar.
//
// The level meter spans −60…0 dBFS with a tick every 10 dB. The −30…−15 band
// is the level the measurement wants; the fill and the word beside the value
// say where the level sits: low, in range, high, or clipping.
import type { ReactNode } from "react";
import { useNumber, useT } from "../i18n/react";

const METER_MIN = -60;
const METER_MAX = 0;
const METER_BAND: readonly [number, number] = [-30, -15];

type LevelState = "none" | "low" | "ok" | "high" | "clip";

export function levelState(dbfs: number | null, clipping = false): LevelState {
  if (clipping) return "clip";
  if (dbfs === null || !Number.isFinite(dbfs)) return "none";
  if (dbfs < METER_BAND[0]) return "low";
  if (dbfs <= METER_BAND[1]) return "ok";
  return "high";
}

const pct = (dbfs: number) => ((Math.min(METER_MAX, Math.max(METER_MIN, dbfs)) - METER_MIN) / (METER_MAX - METER_MIN)) * 100;

export function LevelMeter({
  id,
  label,
  value,
  peak,
  clipping = false,
  showTicks = true,
}: {
  id: string;
  label: ReactNode;
  /** RMS level in dBFS, or null before the first reading. */
  value: number | null;
  /** Held peak in dBFS. */
  peak?: number | null;
  clipping?: boolean;
  showTicks?: boolean;
}) {
  const t = useT();
  const number = useNumber();
  const state = levelState(value, clipping);
  const stateText = {
    none: t("ui.meter.none"),
    low: t("ui.meter.low"),
    ok: t("ui.meter.ok"),
    high: t("ui.meter.high"),
    clip: t("ui.meter.clip"),
  }[state];
  const valueText = value === null ? "—" : t("ui.meter.value", { value: number(value, 1) });
  const ticks = [-60, -50, -40, -30, -20, -10, 0];
  return (
    <div className={`ui-meter ui-meter--${state}`} data-ui={id}>
      <div className="ui-meter__head">
        <span className="ui-meter__label" id={`${id}-label`}>
          {label}
        </span>
        <span className="ui-meter__readout">
          <span className="ui-meter__value tabular">{valueText}</span>
          <span className="ui-meter__state">{stateText}</span>
        </span>
      </div>
      <div
        id={id}
        className="ui-meter__track"
        role="meter"
        aria-labelledby={`${id}-label`}
        aria-valuemin={METER_MIN}
        aria-valuemax={METER_MAX}
        aria-valuenow={value ?? METER_MIN}
        aria-valuetext={value === null ? stateText : `${valueText}, ${stateText}`}
      >
        <span
          className="ui-meter__band"
          style={{ left: `${pct(METER_BAND[0])}%`, width: `${pct(METER_BAND[1]) - pct(METER_BAND[0])}%` }}
          aria-hidden="true"
        />
        {value !== null && <span className="ui-meter__fill" style={{ width: `${pct(value)}%` }} aria-hidden="true" />}
        {peak !== undefined && peak !== null && (
          <span className="ui-meter__peak" style={{ left: `${pct(peak)}%` }} aria-hidden="true" />
        )}
      </div>
      {showTicks && (
        <div className="ui-meter__ticks" aria-hidden="true">
          {ticks.map((tick) => (
            <span key={tick} className="ui-meter__tick tabular" style={{ left: `${pct(tick)}%` }}>
              {number(tick)}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export function ProgressBar({
  id,
  label,
  done,
  total,
  indeterminate,
  valueText,
}: {
  id?: string;
  /** Names what is progressing (for screen readers; visible text sits beside the bar). */
  label: string;
  done?: number;
  total?: number;
  indeterminate?: boolean;
  valueText?: string;
}) {
  const known = !indeterminate && total !== undefined && total > 0 && done !== undefined;
  const ratio = known ? Math.min(1, Math.max(0, done / total)) : 0;
  return (
    <div
      id={id}
      className={["ui-progress", !known && "ui-progress--indeterminate"].filter(Boolean).join(" ")}
      role="progressbar"
      aria-label={label}
      aria-valuemin={known ? 0 : undefined}
      aria-valuemax={known ? total : undefined}
      aria-valuenow={known ? done : undefined}
      aria-valuetext={valueText}
    >
      <span className="ui-progress__fill" style={known ? { width: `${ratio * 100}%` } : undefined} />
    </div>
  );
}
