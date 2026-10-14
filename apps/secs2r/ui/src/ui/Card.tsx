// Surfaces. A card is one unit with a title that names it; a section is a
// titled group inside a card; "고급" folds the second tier of options away
// (ARCHITECTURE §7: main path, folded advanced, expert display).
import { useId, useState, type HTMLAttributes, type ReactNode } from "react";
import { useT } from "../i18n/react";
import { Icon } from "./Icon";

type CardProps = Omit<HTMLAttributes<HTMLElement>, "title"> & {
  title?: ReactNode;
  /** One line under the title; omit when the title says enough. */
  description?: ReactNode;
  /** Controls at the right end of the header (quiet buttons, a pill). */
  actions?: ReactNode;
  /** Results that no longer match the inputs: dimmed body and a marker. */
  stale?: boolean;
  /** Tight padding for dense data cards (tables, graphs). */
  dense?: boolean;
  children?: ReactNode;
};

export function Card({ title, description, actions, stale, dense, className, children, ...rest }: CardProps) {
  const titleId = useId();
  return (
    <section
      className={["ui-card", dense && "ui-card--dense", stale && "ui-card--stale", className].filter(Boolean).join(" ")}
      aria-labelledby={title ? titleId : undefined}
      {...rest}
    >
      {(title || actions) && (
        <header className="ui-card__header">
          <div className="ui-card__heading">
            {title && (
              <h2 id={titleId} className="ui-card__title">
                {title}
              </h2>
            )}
            {description && <p className="ui-card__description">{description}</p>}
          </div>
          {actions && <div className="ui-card__actions">{actions}</div>}
        </header>
      )}
      <div className="ui-card__body">{children}</div>
    </section>
  );
}

export function Section({
  title,
  description,
  children,
  className,
  ...rest
}: Omit<HTMLAttributes<HTMLElement>, "title"> & { title: ReactNode; description?: ReactNode; children: ReactNode }) {
  const titleId = useId();
  return (
    <section className={["ui-section", className].filter(Boolean).join(" ")} aria-labelledby={titleId} {...rest}>
      <h3 id={titleId} className="ui-section__title">
        {title}
      </h3>
      {description && <p className="ui-section__description">{description}</p>}
      <div className="ui-section__body">{children}</div>
    </section>
  );
}

/** The folded "고급" part of a section. Hidden content is removed, not disabled. */
export function Advanced({
  children,
  defaultOpen = false,
  id,
  count,
}: {
  children: ReactNode;
  defaultOpen?: boolean;
  /** Stable id for the toggle, so the tutorial can point at it. */
  id?: string;
  /** How many options are inside, shown while folded. */
  count?: number;
}) {
  const t = useT();
  const [open, setOpen] = useState(defaultOpen);
  const panelId = useId();
  return (
    <div className={["ui-advanced", open && "ui-advanced--open"].filter(Boolean).join(" ")}>
      <button
        type="button"
        id={id}
        data-ui={id}
        className="ui-advanced__toggle"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => setOpen((value) => !value)}
      >
        <Icon name="chevronRight" size={16} className="ui-advanced__chevron" />
        <span>{t("ui.advanced")}</span>
        {count !== undefined && !open && <span className="ui-advanced__count tabular">{t("ui.advanced.count", { count })}</span>}
      </button>
      {open && (
        <div id={panelId} className="ui-advanced__panel">
          {children}
        </div>
      )}
    </div>
  );
}
