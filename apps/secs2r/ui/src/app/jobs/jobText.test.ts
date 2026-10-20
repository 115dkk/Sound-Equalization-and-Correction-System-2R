import { describe, expect, it } from "vitest";
import { translate, type Locale } from "../../i18n";
import { ipcError } from "../../ipc/mock";
import { errorText } from "./jobText";

const cases = [
  { reason: "measurement_sample_rate", params: { position: "3", role: "Tfl", actual_hz: "44100", expected_hz: "48000" }, required: ["position", "role", "actual_hz", "expected_hz"], shown: ["3", "Ltf", "44,100", "48,000"] },
  { reason: "unsupported_resampling_rate", params: { position: "3", role: "Tfl", actual_hz: "4000", min_hz: "8000", max_hz: "384000" }, required: ["position", "role", "actual_hz", "min_hz", "max_hz"], shown: ["3", "Ltf", "4,000", "8,000", "384,000"] },
  { reason: "invalid_measurement", params: { position: "3", role: "Tfl" }, required: ["position", "role"], shown: ["3", "Ltf"] },
  { reason: "duplicate_measurement", params: { position: "3", role: "Tfl" }, required: ["position", "role"], shown: ["3", "Ltf"] },
  { reason: "missing_reference_position", params: { position: "3" }, required: ["position"], shown: ["3"] },
  { reason: "missing_main_measurement", params: { position: "3" }, required: ["position"], shown: ["3"] },
] satisfies { reason: string; params: Record<string, string>; required: string[]; shown: string[] }[];

const words = (locale: Locale, params: Record<string, string>) => errorText((key, values) => translate(locale, key, values), ipcError("engine_input", "private diagnostic: SW3 44100", params));

describe.each(["ko", "en"] as const)("structured calculation errors (%s)", (locale) => {
  it.each(cases)("uses $reason only with its complete fields and displays user-facing role names", ({ reason, params, shown }) => {
    const text = words(locale, { ...params, reason });
    expect(text.generic).toBe(false);
    for (const value of shown) expect(text.cause).toContain(value);
    expect(`${text.cause} ${text.next}`).not.toMatch(/[{}]|Tfl|private diagnostic/);
  });

  it.each(cases)("falls back safely when any required $reason value is absent", ({ reason, params, required }) => {
    for (const key of required) {
      const incomplete: Record<string, string> = { ...params, reason };
      delete incomplete[key];
      const text = words(locale, incomplete);
      expect(text).toEqual({
        cause: translate(locale, "error.engine_input"),
        next: translate(locale, "error.engine_input.next"),
        generic: true,
      });
      expect(`${text.cause} ${text.next}`).not.toMatch(/[{}]|private diagnostic/);
    }
  });

  it("does not infer a measurement cause from diagnostic text or malformed metadata", () => {
    for (const params of [
      {}, { reason: "future_reason" }, { reason: "constructor" },
      { reason: "invalid_measurement", position: "1", role: " " },
      ...["-1", "1.5", "NaN", "1e3", "4294967296"].map((position) => ({ reason: "invalid_measurement", position, role: "SW1" })),
    ]) {
      expect(words(locale, params).generic).toBe(true);
      expect(words(locale, params).cause).toBe(translate(locale, "error.engine_input"));
    }
  });
});
