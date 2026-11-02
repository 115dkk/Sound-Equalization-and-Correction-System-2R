import type { CalibrationSettings } from "../ipc/bindings/CalibrationSettings";
import type { RcBand } from "../ipc/bindings/RcBand";
import type { SettingChange, SettingEdit } from "../screens/calibrate/types";
import { nextBand, withBandEdge } from "../screens/calibrate/settingsView";

const sameBand = (a: RcBand, b: RcBand) => a.lo_hz === b.lo_hz && a.hi_hz === b.hi_hz;

export function settingEdit(change: SettingChange, latest: CalibrationSettings, index?: number): SettingEdit | null {
  if (change.kind === "rc_band_add") return { kind: "rc_bands", bands: [...latest.rc_bands, nextBand(latest, change.primaryHi)] };
  if (change.kind !== "rc_band_edge" && change.kind !== "rc_band_remove") return change;
  const at = index ?? latest.rc_bands.findIndex((band) => sameBand(band, change.band));
  if (at < 0) return null;
  return { kind: "rc_bands", bands: change.kind === "rc_band_remove"
    ? latest.rc_bands.filter((_, i) => i !== at)
    : withBandEdge(latest.rc_bands, at, change.edge, change.hz) };
}

/** Queued edits keep their band even when an accepted edit sorts or removes rows. */
export function createBandTargets() {
  type Target = { value: RcBand | null };
  const targets = new WeakMap<RcBand, Target>();
  const get = (band: RcBand): Target => {
    let target = targets.get(band);
    if (!target) { target = { value: band }; targets.set(band, target); }
    return target;
  };
  return {
    get,
    index(bands: RcBand[], target: Target) {
      if (!target.value) return -1;
      const direct = bands.indexOf(target.value);
      return direct >= 0 ? direct : bands.findIndex((band) => sameBand(band, target.value!));
    },
    commit(before: RcBand[], edit: SettingEdit, change: SettingChange, index: number, accepted: RcBand[]) {
      const rows = before.map((band) => ({ band, target: get(band) }));
      let removed: Target | undefined;
      if (edit.kind === "rc_bands") {
        if (change.kind === "rc_band_remove") removed = rows.splice(index, 1)[0]?.target;
        else if (change.kind === "rc_band_edge") rows[index]!.band = edit.bands[index]!;
        else if (change.kind === "rc_band_add") rows.push({ band: edit.bands.at(-1)!, target: { value: null } });
      }
      const remaining = [...accepted];
      for (const row of rows) {
        const at = remaining.findIndex((band) => sameBand(band, row.band));
        const value = at < 0 ? null : remaining.splice(at, 1)[0]!;
        row.target.value = value;
        if (value) targets.set(value, row.target);
      }
      if (removed) removed.value = null;
    },
  };
}
