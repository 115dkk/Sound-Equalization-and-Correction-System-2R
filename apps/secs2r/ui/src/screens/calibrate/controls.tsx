// Small compositions of kit parts the 보정 screen needs: a stat of the
// verdict (label, value, unit).
import type { ReactNode } from "react";

/** One figure of the verdict: a caption, a value with its unit, and an optional detail. */
export function Stat({ id, label, detail, children, lead }: { id: string; label: ReactNode; detail?: ReactNode; children: ReactNode; lead?: boolean }) {
  return (
    <div className={["calibrate-stat", lead && "calibrate-stat--lead"].filter(Boolean).join(" ")} data-ui={id}>
      <dt className="calibrate-stat__label">
        {label}
        {detail && <span className="calibrate-stat__detail">{detail}</span>}
      </dt>
      <dd className="calibrate-stat__value tabular">{children}</dd>
    </div>
  );
}
