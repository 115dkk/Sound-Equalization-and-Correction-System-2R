// @vitest-environment happy-dom
// The settings row table (ADR 0055) against the generated edit table: each
// row edits the field `SCALAR_EDITS` names for its kind, within the limits it
// names, and Settings.tsx draws every row once beside the controls it keeps
// by hand (the structured editors).
import { cleanup, fireEvent } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createElement } from "react";
import { describe, expect, it, vi } from "vitest";
import { CALIBRATION_LIMITS, SCALAR_EDITS } from "../../ipc/bindings/rules";
import { renderWith } from "../../test/render";
import { SettingRow } from "./controls";
import * as fx from "./fixtures";
import { SETTING_ROWS, rowEdit } from "./settingRows";

const rules = new Map<string, (typeof SCALAR_EDITS)[number]>(SCALAR_EDITS.map((rule) => [rule.kind, rule]));
const source = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "Settings.tsx"), "utf8");
const count = (pattern: RegExp) => [...source.matchAll(pattern)].length;

describe("settingRows", () => {
  it.each(SETTING_ROWS.map((row) => [row.id, row] as const))("%s edits the field and limits SCALAR_EDITS names for its kind", (_id, row) => {
    // Every row sends a scalar kind: the structured edits (rc_bands, room_dims, xo, xo_peq, infra, sub…) stay hand-written.
    const rule = rules.get(row.kind);
    expect(rule).toBeDefined();
    expect(row.field).toBe(rule!.field);
    expect(Object.keys(rowEdit(row, 1))).toEqual(["kind", row.param]);
    if ("limits" in row) {
      expect(Object.keys(CALIBRATION_LIMITS)).toContain(row.limits);
      expect(row.limits).toBe(rule!.limit);
    }
    // A switch over a number that may be null starts the number inside the kind's limits.
    if ("nullable" in row) {
      const [lo, hi] = CALIBRATION_LIMITS[rule!.limit as keyof typeof CALIBRATION_LIMITS];
      expect(row.nullable.start).toBeGreaterThanOrEqual(lo);
      expect(row.nullable.start).toBeLessThanOrEqual(hi);
    }
  });

  // The screen tests drive the input cut (null is off) and the transition factor; this is the switch whose on state is null.
  it("sends null from MIMO 하한 자동 and 20 Hz when it is turned off", () => {
    const change = vi.fn(async () => true);
    for (const [mimo_low, sent] of [[30, null], [null, 20]] as const) {
      renderWith(createElement(SettingRow, { id: "calibrate.mimo.low_auto", view: { ...fx.mimoView, settings: { ...fx.mimoView.settings, mimo_low } }, change }));
      const toggle = document.querySelector<HTMLElement>('[data-ui="calibrate.mimo.low_auto"]')!;
      expect(toggle.getAttribute("aria-checked")).toBe(String(mimo_low === null));
      fireEvent.click(toggle);
      expect(change).toHaveBeenLastCalledWith({ kind: "mimo_low", hz: sent });
      cleanup();
    }
  });

  it.each(["zero", "mixed", "linear", "ultra"] as const)("sends main_delay_off edits in %s mode without disabling the switch", (mode) => {
    const change = vi.fn(async () => true);
    for (const view of Object.values(fx.mainDelayOffViews)) {
      renderWith(createElement(SettingRow, { id: "calibrate.mode.main_delay_off", view: { ...view, settings: { ...view.settings, mode } }, change }));
      const toggle = document.querySelector<HTMLButtonElement>('[data-ui="calibrate.mode.main_delay_off"]')!;
      expect(toggle.disabled).toBe(false);
      expect(toggle.getAttribute("aria-checked")).toBe(String(view.settings.main_delay_off));
      fireEvent.click(toggle);
      expect(change).toHaveBeenLastCalledWith({ kind: "main_delay_off", on: !view.settings.main_delay_off });
      cleanup();
    }
  });

  it("names every control once", () => {
    const ids = SETTING_ROWS.map((row) => row.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("is drawn once per row by Settings.tsx, which keeps the rest of its controls by hand", () => {
    const drawn = [...source.matchAll(/<SettingRow id="([^"]+)"/g)].map((match) => match[1]);
    expect([...drawn].sort()).toEqual(SETTING_ROWS.map((row) => row.id).sort());
    const rows = (control: string) => SETTING_ROWS.filter((row) => row.control === control).length;
    const table = { toggle: rows("toggle"), number: rows("number"), segmented: rows("segmented"), select: rows("select") };
    const byHand = { toggle: count(/<Toggle\b/g), number: count(/<NumberField\b/g), segmented: count(/<Segmented\b/g), select: count(/<Select\b/g) };
    expect(table).toEqual({ toggle: 11, number: 12, segmented: 3, select: 0 });
    // The controls Settings.tsx drew before the table (ADR 0055: Toggle 13, NumberField 27, Segmented 8, Select 6), and the toggle of ADR 0061.
    expect({ toggle: table.toggle + byHand.toggle, number: table.number + byHand.number, segmented: table.segmented + byHand.segmented, select: table.select + byHand.select })
      .toEqual({ toggle: 14, number: 27, segmented: 8, select: 6 });
  });
});
