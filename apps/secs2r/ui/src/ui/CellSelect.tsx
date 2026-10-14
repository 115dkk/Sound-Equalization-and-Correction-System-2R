// A select for dense table cells (the patch sheet's channel, size and output
// columns). The column header is the visible label, so the control carries
// an aria-label. The closed control can show richer content than the option
// text (a code in full weight and a muted name, cut with an ellipsis); the
// native <select> sits on top of it, invisible, so the keyboard, screen
// readers and the platform's option list behave as with any select. The
// select reaches 40 px of hit height while the box stays 32 px.
import type { ReactNode } from "react";
import type { Option } from "./controls";
import { Icon } from "./Icon";
import "./cellSelect.css";

export function CellSelect<V extends string>({
  id,
  label,
  value,
  options,
  onChange,
  display,
  invalid,
  disabled,
}: {
  id: string;
  /** Names the control for screen readers ("L 측정 출력"). */
  label: string;
  value: V;
  options: readonly Option<V>[];
  onChange: (value: V) => void;
  /** What the closed control shows; defaults to the chosen option's label. */
  display?: ReactNode;
  /** Marks the value as a problem (out of range, duplicate). */
  invalid?: boolean | undefined;
  disabled?: boolean | undefined;
}) {
  const current = options.find((option) => option.value === value);
  return (
    <div className={["ui-cell-select", invalid && "ui-cell-select--invalid", disabled && "ui-cell-select--disabled"].filter(Boolean).join(" ")}>
      <span className="ui-cell-select__display" aria-hidden="true">
        {display ?? current?.label ?? ""}
      </span>
      <Icon name="chevronDown" size={16} className="ui-cell-select__chevron" />
      <select
        id={id}
        data-ui={id}
        className="ui-cell-select__native"
        aria-label={label}
        aria-invalid={invalid ? true : undefined}
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value as V)}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value} disabled={option.disabled}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  );
}
