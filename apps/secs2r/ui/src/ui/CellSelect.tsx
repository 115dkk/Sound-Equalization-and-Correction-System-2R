// A select inside a table cell or a dense row, where a visible label above
// each control would repeat the column header. The accessible name comes
// from `label` (aria-label); the look is the kit's Select.
import "./CellSelect.css";
import { Icon } from "./Icon";
import type { Option } from "./controls";

export function CellSelect<V extends string>({
  id,
  label,
  value,
  options,
  onChange,
  invalid,
  describedBy,
  disabled,
}: {
  id: string;
  /** Accessible name ("L P4 채널"); the column header names it on screen. */
  label: string;
  value: V;
  options: readonly Option<V>[];
  onChange: (value: V) => void;
  invalid?: boolean | undefined;
  /** Id of the text that explains an invalid value. */
  describedBy?: string | undefined;
  disabled?: boolean | undefined;
}) {
  return (
    <div className={["ui-select", "ui-select--cell", invalid && "ui-select--invalid"].filter(Boolean).join(" ")}>
      <select
        id={id}
        data-ui={id}
        className="ui-select__native"
        aria-label={label}
        value={value}
        disabled={disabled}
        aria-invalid={invalid || undefined}
        aria-describedby={invalid ? describedBy : undefined}
        onChange={(event) => onChange(event.target.value as V)}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value} disabled={option.disabled}>
            {option.label}
          </option>
        ))}
      </select>
      <Icon name="chevronDown" size={16} className="ui-select__chevron" />
    </div>
  );
}
