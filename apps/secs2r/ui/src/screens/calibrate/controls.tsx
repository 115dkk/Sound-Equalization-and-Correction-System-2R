// Small compositions of kit parts the 보정 screen needs in dense rows: a
// select with its label beside it (graph bar, per-sub mode list), a number
// field whose 0 means "none" or "auto", and a stat (label, value, unit).
import type { ReactNode } from "react";
import { Field } from "../../ui/Field";
import { Icon } from "../../ui/Icon";
import type { Option } from "../../ui";

/** A select with the label on its left (the kit's Select puts it above). */
export function InlineSelect<V extends string>({
  id,
  label,
  value,
  options,
  onChange,
  disabled,
  hint,
}: {
  id: string;
  label: ReactNode;
  value: V;
  options: readonly Option<V>[];
  onChange: (value: V) => void;
  disabled?: boolean;
  hint?: string;
}) {
  return (
    <Field id={id} label={label} inline hint={hint} className="calibrate-inline-select">
      <div className="ui-select">
        <select
          id={id}
          data-ui={id}
          className="ui-select__native calibrate-inline-select__native"
          value={value}
          disabled={disabled}
          aria-describedby={hint ? `${id}-hint` : undefined}
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
    </Field>
  );
}

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
