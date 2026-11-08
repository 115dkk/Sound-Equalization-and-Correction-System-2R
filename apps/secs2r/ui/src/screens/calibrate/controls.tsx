// Small compositions of kit parts the 보정 screen needs: a stat of the
// verdict (label, value, unit), and one row of the settings table.
import type { ReactNode } from "react";
import { useNumber, useT } from "../../i18n/react";
import { CALIBRATION_LIMITS } from "../../ipc/bindings/rules";
import { NumberField, Segmented, Toggle } from "../../ui";
import { rowEdit, settingRow, type SettingRowId } from "./settingRows";
import type { CalibrateActions, CalibrateView } from "./types";

/**
 * One figure of the verdict: the label line, the band line when the figure
 * has a band (the detail), then the value with its unit. Every stat has this
 * form, so the values of a row of stats share one baseline (calibrate.css).
 */
export function Stat({ id, label, detail, children, lead }: { id: string; label: ReactNode; detail?: ReactNode; children: ReactNode; lead?: boolean }) {
  return (
    <div className={["calibrate-stat", lead && "calibrate-stat--lead"].filter(Boolean).join(" ")} data-ui={id}>
      <dt className="calibrate-stat__label">
        <span className="calibrate-stat__name">{label}</span>
        {detail && <span className="calibrate-stat__detail">{detail}</span>}
      </dt>
      <dd className="calibrate-stat__value tabular">{children}</dd>
    </div>
  );
}

/**
 * One row of the settings table (settingRows.ts), drawn with the kit control
 * its `control` names; nothing while the row has no meaning. A change sends
 * the row's kind through `change` (calibration_settings_edit). The range card
 * passes the refusal of an edge (`rejected`) and, for a number that is unset,
 * the value to show instead (`unset`, 0 when not given).
 */
export function SettingRow({ id, view, change, rejected, unset = 0 }: {
  id: SettingRowId;
  view: CalibrateView;
  change: CalibrateActions["changeSetting"];
  rejected?: ReactNode;
  unset?: number;
}) {
  const t = useT();
  const number = useNumber();
  const row = settingRow(id);
  const s = view.settings;
  if (row.shownWhen && !row.shownWhen(s, view)) return null;
  const value = s[row.field];
  const send = (next: unknown) => void change(rowEdit(row, next));
  const label = t(row.label);
  const help = row.help && t(row.help);
  const shown = row.hint?.(s, view, t) ?? null;
  const hint = shown === null ? undefined : typeof shown === "string" ? shown : <span data-ui={shown.ui}>{shown.text}</span>;
  const disabled = row.disabledWhen?.(s, view) ?? false;
  switch (row.control) {
    case "toggle": {
      const nullable = row.nullable;
      const nullOn = nullable?.null === "on";
      const on = nullable ? (value === null) === nullOn : value === true;
      return (
        <Toggle
          id={row.id}
          label={label}
          help={help}
          hint={hint}
          checked={!disabled && on}
          disabled={disabled}
          onChange={(next) => send(nullable ? (next === nullOn ? null : nullable.start) : next)}
        />
      );
    }
    case "number": {
      const [min, max] = CALIBRATION_LIMITS[row.limits];
      return (
        <NumberField
          id={row.id}
          label={label}
          value={typeof value === "number" ? value : unset}
          min={min}
          max={max}
          step={row.step}
          {...(row.digits !== undefined ? { digits: row.digits } : {})}
          {...(row.unit !== undefined ? { unit: row.unit } : {})}
          {...(row.size !== undefined ? { size: row.size } : {})}
          {...(row.showRange ? { showRange: true } : {})}
          help={help}
          hint={hint}
          rejected={rejected}
          onChange={send}
        />
      );
    }
    case "segmented":
      return (
        <Segmented
          id={row.id}
          label={label}
          {...(row.hideLabel ? { hideLabel: true } : {})}
          help={help}
          hint={hint}
          value={String(value)}
          options={row.choices.map((choice) => ({ value: String(choice.value), label: typeof choice.label === "function" ? choice.label(number) : t(choice.label) }))}
          onChange={(next) => send(row.choices.find((choice) => String(choice.value) === next)?.value ?? next)}
        />
      );
  }
}
