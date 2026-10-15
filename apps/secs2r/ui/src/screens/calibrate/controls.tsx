// Small compositions of kit parts the 보정 screen needs: a stat of the
// verdict (label, value, unit).
import type { ReactNode } from "react";

/**
 * One figure of the verdict: the label line, the band line when the figure
 * has a band (the detail), then the value with its unit. Every stat has this
 * form, so the values of a row of stats share one baseline (calibrate.css).
 */
export function Stat({ id, label, detail, children, lead }: { id: string; label: ReactNode; detail?: ReactNode; children: ReactNode; lead?: boolean }) {
  return (
    <div className={["calibrate-stat", lead && "calibrate-stat--lead"].filter(Boolean).join(" ")} data-ui={id}>
      <dt className="calibrate-stat__label">
        <span className="calibrate-stat__name">{label}</span>
        {detail && <span className="calibrate-stat__detail">{detail}</span>}
      </dt>
      <dd className="calibrate-stat__value tabular">{children}</dd>
    </div>
  );
}
