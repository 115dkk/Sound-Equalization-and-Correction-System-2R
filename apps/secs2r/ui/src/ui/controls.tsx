// Choice controls: select, stepper, toggle and segmented control. Options
// have names, never numbers; a toggle's label describes its ON state.
import { useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import { useT } from "../i18n/react";
import { Field, describedBy } from "./Field";
import { Icon } from "./Icon";

/* ── Select ─────────────────────────────────────────────────────────────── */

export type Option<V extends string> = { value: V; label: string; disabled?: boolean };

export function Select<V extends string>({
  id,
  label,
  value,
  options,
  onChange,
  hint,
  error,
  disabled,
}: {
  id: string;
  label: ReactNode;
  value: V;
  options: readonly Option<V>[];
  onChange: (value: V) => void;
  hint?: ReactNode;
  error?: ReactNode;
  disabled?: boolean;
}) {
  return (
    <Field id={id} label={label} hint={hint} error={error}>
      <div className={["ui-select", error && "ui-select--invalid"].filter(Boolean).join(" ")}>
        <select
          id={id}
          data-ui={id}
          className="ui-select__native"
          value={value}
          disabled={disabled}
          aria-invalid={error ? true : undefined}
          aria-describedby={describedBy(id, hint, error)}
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

/* ── Stepper ────────────────────────────────────────────────────────────── */

/** Whole numbers in a small range (sub count 0–8). ↑/↓, Home and End work on the value. */
export function Stepper({
  id,
  label,
  value,
  min,
  max,
  onChange,
  format,
  hint,
  disabled,
}: {
  id: string;
  label: ReactNode;
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  format?: (value: number) => string;
  hint?: ReactNode;
  disabled?: boolean;
}) {
  const t = useT();
  const labelId = useId();
  const set = (next: number) => {
    const clamped = Math.min(max, Math.max(min, next));
    if (clamped !== value) onChange(clamped);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const keys: Record<string, number> = { ArrowUp: value + 1, ArrowRight: value + 1, ArrowDown: value - 1, ArrowLeft: value - 1, Home: min, End: max };
    const next = keys[event.key];
    if (next !== undefined) {
      event.preventDefault();
      set(next);
    }
  };
  const text = format ? format(value) : String(value);
  return (
    <div className="ui-field">
      <span className="ui-field__label" id={labelId}>
        {label}
      </span>
      <div className={["ui-stepper", disabled && "ui-stepper--disabled"].filter(Boolean).join(" ")}>
        <button
          type="button"
          className="ui-stepper__button"
          tabIndex={-1}
          aria-label={t("ui.stepper.decrease")}
          disabled={disabled || value <= min}
          onClick={() => set(value - 1)}
        >
          <Icon name="minus" size={16} />
        </button>
        <div
          id={id}
          data-ui={id}
          className="ui-stepper__value tabular"
          role="spinbutton"
          tabIndex={disabled ? -1 : 0}
          aria-labelledby={labelId}
          aria-valuemin={min}
          aria-valuemax={max}
          aria-valuenow={value}
          aria-valuetext={text}
          aria-disabled={disabled || undefined}
          onKeyDown={disabled ? undefined : onKeyDown}
        >
          {text}
        </div>
        <button
          type="button"
          className="ui-stepper__button"
          tabIndex={-1}
          aria-label={t("ui.stepper.increase")}
          disabled={disabled || value >= max}
          onClick={() => set(value + 1)}
        >
          <Icon name="plus" size={16} />
        </button>
      </div>
      {hint && <p className="ui-field__hint">{hint}</p>}
    </div>
  );
}

/* ── Toggle ─────────────────────────────────────────────────────────────── */

export function Toggle({
  id,
  label,
  checked,
  onChange,
  hint,
  disabled,
}: {
  id: string;
  /** Names the ON state ("전문가 항목 표시"), never a negation. */
  label: ReactNode;
  checked: boolean;
  onChange: (checked: boolean) => void;
  hint?: ReactNode;
  disabled?: boolean;
}) {
  const hid = `${id}-hint`;
  return (
    <div className="ui-toggle-row">
      <button
        id={id}
        data-ui={id}
        type="button"
        role="switch"
        aria-checked={checked}
        aria-describedby={hint ? hid : undefined}
        disabled={disabled}
        className="ui-toggle"
        onClick={() => onChange(!checked)}
      >
        <span className="ui-toggle__track" aria-hidden="true">
          <span className="ui-toggle__thumb" />
        </span>
        <span className="ui-toggle__label">{label}</span>
      </button>
      {hint && (
        <p className="ui-field__hint ui-toggle__hint" id={hid}>
          {hint}
        </p>
      )}
    </div>
  );
}

/* ── Segmented control ──────────────────────────────────────────────────── */

/** A radio group drawn as segments. Arrow keys move and select; one tab stop. */
export function Segmented<V extends string>({
  id,
  label,
  value,
  options,
  onChange,
  hideLabel,
  size = "md",
}: {
  id: string;
  label: ReactNode;
  value: V;
  options: readonly Option<V>[];
  onChange: (value: V) => void;
  /** Keep the label for screen readers only (the context already names it). */
  hideLabel?: boolean;
  size?: "md" | "sm";
}) {
  const labelId = useId();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const enabled = options.filter((option) => !option.disabled);
  const move = (delta: number) => {
    const index = enabled.findIndex((option) => option.value === value);
    const next = enabled[(index + delta + enabled.length) % enabled.length];
    if (next) {
      onChange(next.value);
      refs.current[options.indexOf(next)]?.focus();
    }
  };
  return (
    <div className="ui-segmented-field">
      <span id={labelId} className={hideLabel ? "visually-hidden" : "ui-field__label"}>
        {label}
      </span>
      <div
        id={id}
        data-ui={id}
        role="radiogroup"
        aria-labelledby={labelId}
        className={["ui-segmented", size === "sm" && "ui-segmented--sm"].filter(Boolean).join(" ")}
        onKeyDown={(event) => {
          if (event.key === "ArrowRight" || event.key === "ArrowDown") {
            event.preventDefault();
            move(1);
          } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
            event.preventDefault();
            move(-1);
          }
        }}
      >
        {options.map((option, index) => {
          const selected = option.value === value;
          return (
            <button
              key={option.value}
              ref={(node) => {
                refs.current[index] = node;
              }}
              type="button"
              role="radio"
              aria-checked={selected}
              data-ui={`${id}.${option.value}`}
              tabIndex={selected ? 0 : -1}
              disabled={option.disabled}
              className="ui-segmented__option"
              onClick={() => onChange(option.value)}
            >
              {option.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}
