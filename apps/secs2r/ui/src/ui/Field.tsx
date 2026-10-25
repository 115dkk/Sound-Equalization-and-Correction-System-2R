// Label, control, hint and error in one column. Every control keeps a
// visible label tied to it; the hint says what the value changes; the error
// says what to enter instead.
import type { ReactNode } from "react";
import { HelpTip, helpContentId } from "./HelpTip";
import { Icon } from "./Icon";

type FieldProps = {
  id: string;
  label: ReactNode;
  hint?: ReactNode;
  /** Optional background explanation; current facts and validation stay inline. */
  help?: ReactNode;
  error?: ReactNode;
  /** Label beside the control instead of above it (toggles, dense rows). */
  inline?: boolean;
  className?: string;
  children: ReactNode;
};

const hintId = (id: string) => `${id}-hint`;
const errorId = (id: string) => `${id}-error`;

export function describedBy(id: string, hint: unknown, error: unknown, help?: unknown): string | undefined {
  return [error ? errorId(id) : null, hint ? hintId(id) : null, help ? helpContentId(id) : null].filter(Boolean).join(" ") || undefined;
}

export function Field({ id, label, hint, help, error, inline, className, children }: FieldProps) {
  return (
    <div className={["ui-field", inline && "ui-field--inline", error && "ui-field--invalid", className].filter(Boolean).join(" ")}>
      <label className="ui-field__label" htmlFor={id} id={`${id}-label`}>
        {label}
      </label>
      <div className="ui-field__control">{children}{help && <HelpTip id={`${id}.help`} labelledBy={`${id}-label`} content={help} />}</div>
      {error && (
        <p className="ui-field__error" id={errorId(id)}>
          <Icon name="error" size={16} />
          <span>{error}</span>
        </p>
      )}
      {hint && (
        <p className="ui-field__hint" id={hintId(id)}>
          {hint}
        </p>
      )}
    </div>
  );
}
