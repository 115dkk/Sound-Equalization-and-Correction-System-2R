// A state as a noun phrase with a filled dot. The hue carries the meaning
// (success, warning, danger, info) and the words carry it too; neutral is
// for states with nothing to report. `busy` swaps the dot for a spinner.
import type { HTMLAttributes, ReactNode } from "react";
import { Spinner } from "./Icon";

export type PillTone = "neutral" | "success" | "warning" | "danger" | "info" | "busy";

export function StatusPill({
  tone = "neutral",
  children,
  className,
  ...rest
}: HTMLAttributes<HTMLSpanElement> & { tone?: PillTone; children: ReactNode }) {
  return (
    <span className={["ui-pill", `ui-pill--${tone}`, className].filter(Boolean).join(" ")} {...rest}>
      {tone === "busy" ? <Spinner size={14} className="ui-pill__spinner" /> : <span className="ui-pill__dot" aria-hidden="true" />}
      <span className="ui-pill__label">{children}</span>
    </span>
  );
}
