// Empty and error states.
//
// Empty: what this place is (a noun phrase), how to fill it (one line), and
// one action. Error: what happened and what to do next, in the user's words;
// the raw text from the service sits behind 자세히 for bug reports.
import type { ReactNode } from "react";
import { useT } from "../i18n/react";
import { Icon } from "./Icon";

export function EmptyState({
  title,
  description,
  action,
  id,
  compact,
}: {
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  id?: string;
  compact?: boolean;
}) {
  return (
    <div className={["ui-empty", compact && "ui-empty--compact"].filter(Boolean).join(" ")} data-ui={id}>
      <p className="ui-empty__title">{title}</p>
      {description && <p className="ui-empty__description">{description}</p>}
      {action && <div className="ui-empty__action">{action}</div>}
    </div>
  );
}

export function ErrorNotice({
  cause,
  next,
  detail,
  code,
  action,
  id,
  tone = "danger",
}: {
  /** What happened, in the user's terms. */
  cause: ReactNode;
  /** What to do now. */
  next?: ReactNode;
  /** The raw text (service `detail`); shown only behind 자세히. */
  detail?: string;
  /** The stable error code, shown with the raw text. */
  code?: string;
  action?: ReactNode;
  id?: string;
  tone?: "danger" | "warning";
}) {
  const t = useT();
  return (
    <div className={`ui-error ui-error--${tone}`} role={tone === "danger" ? "alert" : "status"} data-ui={id}>
      <Icon name={tone === "danger" ? "error" : "warning"} size={18} className="ui-error__icon" />
      <div className="ui-error__text">
        <p className="ui-error__cause">{cause}</p>
        {next && <p className="ui-error__next">{next}</p>}
        {(detail || code) && (
          <details className="ui-details">
            <summary>{t("ui.details")}</summary>
            <pre className="ui-details__raw">{[code && `[${code}]`, detail].filter(Boolean).join(" ")}</pre>
          </details>
        )}
      </div>
      {action && <div className="ui-error__action">{action}</div>}
    </div>
  );
}
