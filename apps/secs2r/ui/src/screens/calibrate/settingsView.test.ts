import { describe, expect, it } from "vitest";
import { baseSettings } from "./fixtures";
import { infraState, normalizeSub, roomVolume, subOf } from "./settingsView";

// The edit rules themselves live in the service (calibration_settings_edit,
// checked against upstream by the service_settings_edit oracle); these are the
// read-side helpers the settings screen draws from.
describe("calibration settings view helpers", () => {
  it("keeps a sub's values inside the upstream limits", () => {
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

  it("gives a sub without an entry the defaults", () => {
    expect(subOf(baseSettings, "SW9")).toMatchObject({ mode: "auto", hp_hz: 0, lp_hz: 0 });
  });

  it("reads the room volume only from three valid dimensions", () => {
    expect(roomVolume([6, 5, 3])).toBe(90);
    expect(roomVolume([0.4, 5, 3])).toBeNull();
    expect(roomVolume(null)).toBeNull();
  });

  it("reads the infra sub from the filters: none without a lone low-pass", () => {
    expect(infraState(baseSettings, ["SW1", "SW2"]).role).toBeNull();
  });
});
