// A number with a unit, a range and a step. Typing never fights the user:
// the draft stays as typed, an out-of-range or non-numeric draft is marked
// invalid with the range in words, and only a valid value is committed (on
// Enter or leaving the field). ↑/↓ step by `step` (Shift ×10) inside the
// range; Escape restores the last committed value.
import { useEffect, useState, type KeyboardEvent, type ReactNode } from "react";
import { useNumber, useT } from "../i18n/react";
import { Field, describedBy } from "./Field";

export type NumberCheck = { ok: true; value: number } | { ok: false; reason: "empty" | "nan" | "below" | "above" };

/** Reads a typed number (accepts "−", "-", "," as a decimal mark) and checks the range. */
export function checkNumber(text: string, min: number, max: number): NumberCheck {
  const cleaned = text.trim().replace(/−/g, "-").replace(",", ".");
  if (cleaned === "") return { ok: false, reason: "empty" };
  if (!/^[-+]?(\d+\.?\d*|\.\d+)$/.test(cleaned)) return { ok: false, reason: "nan" };
  const value = Number(cleaned);
  if (!Number.isFinite(value)) return { ok: false, reason: "nan" };
  if (value < min) return { ok: false, reason: "below" };
  if (value > max) return { ok: false, reason: "above" };
  return { ok: true, value };
}

/** One arrow-key step from `value`, kept inside [min, max] and rounded to `digits`. */
export function stepNumber(value: number, delta: number, min: number, max: number, digits: number): number {
  const next = Math.min(max, Math.max(min, value + delta));
  const factor = 10 ** digits;
  return Math.round(next * factor) / factor;
}

export type NumberFieldProps = {
  id: string;
  label: ReactNode;
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step: number;
  unit?: string;
  /** Decimals shown and kept. */
  digits?: number;
  hint?: ReactNode;
  help?: ReactNode;
  /** Show "range min–max unit" under the field even while valid. */
  showRange?: boolean;
  disabled?: boolean;
  /** Width of the input box in characters (numbers, not labels). */
  size?: number;
};

export function NumberField({
  id,
  label,
  value,
  onChange,
  min,
  max,
  step,
  unit,
  digits = 0,
  hint,
  help,
  showRange,
  disabled,
  size = 7,
}: NumberFieldProps) {
  const t = useT();
  const number = useNumber();
  const shown = (v: number) => number(v, digits).replace(/,/g, "");
  const [draft, setDraft] = useState(() => shown(value));
  const [touched, setTouched] = useState(false);

  // Follow outside changes while the user is not mid-edit.
  useEffect(() => {
    if (!touched) setDraft(shown(value));
  }, [value, digits, touched]);

  const check = checkNumber(draft, min, max);
  const range = t("ui.number.range", { min: shown(min), max: shown(max), unit: unit ?? "" }).trim();
  const error = touched && !check.ok ? (check.reason === "nan" || check.reason === "empty" ? t("ui.number.nan", { range }) : t("ui.number.out_of_range", { range })) : null;
  const inlineHint = hint ?? (showRange ? range : undefined);

  const commit = () => {
    if (check.ok) {
      const factor = 10 ** digits;
      const rounded = Math.round(check.value * factor) / factor;
      setTouched(false);
      setDraft(shown(rounded));
      if (rounded !== value) onChange(rounded);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      const base = check.ok ? check.value : value;
      const delta = (event.key === "ArrowUp" ? step : -step) * (event.shiftKey ? 10 : 1);
      const next = stepNumber(base, delta, min, max, digits);
      setTouched(false);
      setDraft(shown(next));
      if (next !== value) onChange(next);
    } else if (event.key === "Enter") {
      commit();
    } else if (event.key === "Escape" && touched) {
      event.stopPropagation();
      setTouched(false);
      setDraft(shown(value));
    }
  };

  return (
    <Field id={id} label={label} hint={inlineHint} help={help} error={error}>
      <div className={["ui-number", error && "ui-number--invalid", disabled && "ui-number--disabled"].filter(Boolean).join(" ")}>
        <input
          id={id}
          data-ui={id}
          className="ui-number__input tabular"
          type="text"
          inputMode="decimal"
          autoComplete="off"
          spellCheck={false}
          role="spinbutton"
          aria-valuemin={min}
          aria-valuemax={max}
          aria-valuenow={value}
          aria-valuetext={unit ? `${shown(value)} ${unit}` : shown(value)}
          aria-invalid={error ? true : undefined}
          aria-describedby={describedBy(id, inlineHint, error, help)}
          size={size}
          value={draft}
          disabled={disabled}
          onChange={(event) => {
            setTouched(true);
            setDraft(event.target.value);
          }}
          onBlur={commit}
          onKeyDown={onKeyDown}
        />
        {unit && (
          <span className="ui-number__unit" aria-hidden="true">
            {unit}
          </span>
        )}
      </div>
    </Field>
  );
}
