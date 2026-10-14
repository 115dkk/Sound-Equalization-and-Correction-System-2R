import { describe, expect, it } from "vitest";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import { baseSettings } from "./fixtures";
import { applySettingEdit, infraState, normalizeSub, roomVolume, subFilterWarnings, subOf } from "./settingsEdit";

const SUBS = ["SW1", "SW2", "SW3"];
const apply = (settings: CalibrationSettings, ...edits: Parameters<typeof applySettingEdit>[1][]) => edits.reduce(applySettingEdit, settings);

describe("calibration settings edits (upstream CalibrationSettings.set_sub, set_layout, set_infra)", () => {
  it("keeps values inside the upstream limits", () => {
    const s = apply(baseSettings, { kind: "rc_lo", hz: 5 }, { kind: "boost_max", db: 14 }, { kind: "xo", group: "프론트", hz: 87.4 }, { kind: "ultra_ms", ms: 400 }, { kind: "mimo_max", count: 3.6 });
    expect(s.rc_lo).toBe(10);
    expect(s.boost_max_db).toBe(10);
    expect(s.xo.프론트).toBe(87);
    expect(s.ultra_ms).toBe(200);
    expect(s.mimo_max_supports).toBe(4);
    expect(baseSettings.rc_lo).toBe(20);
    expect(normalizeSub({ gain_db: 40, delay_ms: -2, allpass: [[0, 1], [5, 30], [60, 0.7], [80, 1]], hp_hz: 4, polarity: -3 })).toMatchObject({
      gain_db: 20,
      delay_ms: 0,
      allpass: [
        [10, 20],
        [60, 0.7],
      ],
      hp_hz: 10,
      polarity: -1,
    });
  });

  it("sets the volume from three dimensions and drops dimensions a typed volume no longer matches", () => {
    const dims = apply(baseSettings, { kind: "room_dims", dims: [6, 5, 3] });
    expect(dims.room_volume_m3).toBe(90);
    expect(dims.room_dims_m).toEqual([6, 5, 3]);
    expect(apply(dims, { kind: "room_volume", m3: 90.3 }).room_dims_m).toEqual([6, 5, 3]);
    expect(apply(dims, { kind: "room_volume", m3: 120 }).room_dims_m).toBeNull();
    expect(apply(dims, { kind: "room_volume", m3: 0 }).room_volume_m3).toBeNull();
    expect(apply(dims, { kind: "room_dims", dims: [6, 0, 3] })).toMatchObject({ room_dims_m: null, room_volume_m3: 90 });
    expect(roomVolume([0.4, 5, 3])).toBeNull();
  });

  it("turns a band filter on at 20 or 80 Hz and off again", () => {
    const on = apply(baseSettings, { kind: "sub_band_type", role: "SW1", band: "hp", type: "BW12" }, { kind: "sub_band_type", role: "SW1", band: "lp", type: "LR48" });
    expect(subOf(on, "SW1")).toMatchObject({ hp_hz: 20, hp_type: "BW12", lp_hz: 80, lp_type: "LR48" });
    expect(subOf(apply(on, { kind: "sub_band_type", role: "SW1", band: "hp", type: "off" }), "SW1").hp_hz).toBe(0);
  });

  it("creates a group's layout when its first sub joins, and shares filters inside a DBA or multi-sub group", () => {
    let s = apply(baseSettings, { kind: "sub", role: "SW1", patch: { hp_hz: 25, peq: [[40, 2, -3]] } }, { kind: "sub_mode", role: "SW1", mode: "bundle:A" });
    expect(s.bundle_layouts.A).toEqual({ kind: "values", rear_trim_ms: 0 });
    s = apply(s, { kind: "sub_mode", role: "SW2", mode: "bundle:A" });
    // 상대값 고정 groups keep their own filters.
    expect(subOf(s, "SW2").hp_hz).toBe(0);
    s = apply(s, { kind: "sub_layout", bundle: "A", layout: "dba" });
    expect(subOf(s, "SW2")).toMatchObject({ hp_hz: 25, peq: [[40, 2, -3]] });
    s = apply(s, { kind: "sub", role: "SW2", patch: { lp_hz: 90 } }, { kind: "rear_trim", bundle: "A", ms: 30 });
    expect(subOf(s, "SW1").lp_hz).toBe(90);
    expect(s.bundle_layouts.A).toEqual({ kind: "dba", rear_trim_ms: 20 });
    // A sub joining a layout group takes the group's filters.
    s = apply(s, { kind: "sub_mode", role: "SW3", mode: "bundle:A" });
    expect(subOf(s, "SW3")).toMatchObject({ hp_hz: 25, lp_hz: 90 });
  });

  it("puts the infra low-pass on one sub and the matching high-pass on the others, and takes it away", () => {
    const s = apply(baseSettings, { kind: "infra", role: "SW3", hz: 25, type: "LR24", subs: SUBS });
    expect(subOf(s, "SW3")).toMatchObject({ lp_hz: 25, hp_hz: 0 });
    expect(subOf(s, "SW1")).toMatchObject({ hp_hz: 25, lp_hz: 0 });
    expect(infraState(s, SUBS)).toEqual({ role: "SW3", hz: 25, type: "LR24" });
    const moved = apply(s, { kind: "infra", role: "SW1", hz: 30, type: "BW24", subs: SUBS });
    expect(infraState(moved, SUBS)).toEqual({ role: "SW1", hz: 30, type: "BW24" });
    expect(subOf(moved, "SW3")).toMatchObject({ hp_hz: 30, lp_hz: 0 });
    const off = apply(moved, { kind: "infra", role: null, hz: 30, type: "BW24", subs: SUBS });
    expect(infraState(off, SUBS).role).toBeNull();
    expect(SUBS.map((r) => [subOf(off, r).hp_hz, subOf(off, r).lp_hz])).toEqual([
      [0, 0],
      [0, 0],
      [0, 0],
    ]);
  });

  it("warns when a sub's LP sits below twice its HP", () => {
    const s = apply(baseSettings, { kind: "sub", role: "SW2", patch: { hp_hz: 40, lp_hz: 60 } }, { kind: "sub", role: "SW1", patch: { hp_hz: 20, lp_hz: 80 } });
    expect(subFilterWarnings(s, SUBS)).toEqual([{ role: "SW2", hp_hz: 40, lp_hz: 60 }]);
  });
});
