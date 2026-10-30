import { describe, expect, it } from "vitest";
import { translate, type Locale } from "../../i18n";
import { ipcError } from "../../ipc/mock";
import { failureText } from "../../screens/export/apply";
import { JOB_NAME, errorText } from "./jobText";

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

describe.each(["ko", "en"] as const)("적용's job and error codes (%s)", (locale) => {
  const t = (key: Parameters<typeof translate>[1], params?: Record<string, string | number>) => translate(locale, key, params);

  it("names the apply job in the tray", () => {
    expect(t(JOB_NAME.apply)).toBe(locale === "ko" ? "보정 적용" : "Apply correction");
  });

  it.each(["apply_failed", "permission_denied", "unsupported"] as const)("writes %s as a cause and a next step, never the raw text", (code) => {
    const text = errorText(t, ipcError(code, "Access is denied. (os error 5)", { path: "C:/Program Files/EqualizerAPO/config/SECS2R", target: "equalizer_apo" }));
    expect(text.generic).toBe(false);
    expect(text.cause).toBe(t(`error.${code}`));
    expect(text.next).toBe(t(`error.${code}.next`));
    expect(`${text.cause} ${text.next}`).not.toMatch(/[{}]|os error|Program Files/);
  });

  it("writes the row's own words for an apply that failed", () => {
    const denied = ipcError("permission_denied", "Access is denied. (os error 5)", { path: "C:/Program Files/EqualizerAPO/config/SECS2R" });
    expect(failureText(t, "eqapo", denied)).toEqual({ cause: t("export.apply.eqapo.permission"), next: t("export.apply.eqapo.permission.next") });
    expect(failureText(t, "camilla", denied)).toEqual({ cause: t("export.apply.camilla.permission"), next: t("export.apply.camilla.permission.next") });
    expect(failureText(t, "eqapo", ipcError("unsupported", "Equalizer APO is not installed", { target: "equalizer_apo" })))
      .toEqual({ cause: t("export.apply.eqapo.not_installed.cause"), next: t("export.apply.eqapo.not_installed.next") });
    expect(failureText(t, "camilla", ipcError("apply_failed", "connect 127.0.0.1:1234: refused"))).toEqual({ cause: t("error.apply_failed"), next: t("error.apply_failed.next") });
  });
});
