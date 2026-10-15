import { describe, expect, it } from "vitest";
import { translate, type Locale } from "../../i18n";
import type { Translate } from "../../i18n/react";
import type { Notice } from "../../ipc/bindings/Notice";
import { noteLines, noticeText, type FormatNumber } from "./text";

// The calculation notes as secs-service sends them (calibration.rs `notices`:
// one record per role, filter or mode, scalar string params), turned into the
// verdict card's lines in both languages.
function words(locale: Locale): { t: Translate; number: FormatNumber } {
  const t: Translate = (key, params) => translate(locale, key, params);
  const number: FormatNumber = (value, digits = 0) =>
    new Intl.NumberFormat(locale === "ko" ? "ko-KR" : "en-US", { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value).replace("-", "−");
  return { t, number };
}

const lines = (locale: Locale, notices: Notice[]) => {
  const { t, number } = words(locale);
  return noteLines(t, number, notices);
};
const n = (code: string, params: Record<string, string>): Notice => ({ code, params });

describe("calculation notes", () => {
  it("sub_filters: a sub's HP, LP and each fixed PEQ on one line", () => {
    const notes = [
      n("sub_filters", { role: "SW1", hp_hz: "10", hp_type: "LR12", lp_hz: "80", lp_type: "LR24", peq_count: "1" }),
      n("sub_filter_peq", { role: "SW1", index: "0", hz: "40", q: "1", gain_db: "-2" }),
    ];
    expect(lines("ko", notes)).toEqual(["서브 고정 필터(SW1): HP 10 Hz LR 12 · LP 80 Hz LR 24 · PEQ 40 Hz Q 1.00 −2.0 dB"]);
    expect(lines("en", notes)).toEqual(["Sub fixed filters (SW1): HP 10 Hz LR 12 · LP 80 Hz LR 24 · PEQ 40 Hz Q 1.00 −2.0 dB"]);
    // Read alone (no PEQ records at hand), the PEQs are counted; a sub with only an LP names only that.
    const { t, number } = words("ko");
    expect(noticeText(t, number, notes[0]!)).toBe("서브 고정 필터(SW1): HP 10 Hz LR 12 · LP 80 Hz LR 24 · PEQ 1개");
    expect(lines("en", [n("sub_filters", { role: "SW3", hp_hz: "0", hp_type: "LR24", lp_hz: "25", lp_type: "LR24", peq_count: "0" })])).toEqual([
      "Sub fixed filters (SW3): LP 25 Hz LR 24",
    ]);
  });

  it("sub_filter_peq: a PEQ without its sub's line still reads as the sub's fixed filters", () => {
    const peq = [n("sub_filter_peq", { role: "SW2", index: "0", hz: "38", q: "5", gain_db: "-4" }), n("sub_filter_peq", { role: "SW2", index: "1", hz: "63", q: "2", gain_db: "1.5" })];
    expect(lines("ko", peq)).toEqual(["서브 고정 필터(SW2): PEQ 38 Hz Q 5.00 −4.0 dB · PEQ 63 Hz Q 2.00 +1.5 dB"]);
    expect(lines("en", peq)).toEqual(["Sub fixed filters (SW2): PEQ 38 Hz Q 5.00 −4.0 dB · PEQ 63 Hz Q 2.00 +1.5 dB"]);
  });

  it("phase_strength: every main's phase correction strength on one line", () => {
    const notes = [n("phase_strength", { role: "L", strength: "0.5" }), n("phase_strength", { role: "Tfl", strength: "1" })];
    expect(lines("ko", notes)).toEqual(["위상 보정 세기: L 0.50, Ltf 1.00"]);
    expect(lines("en", notes)).toEqual(["Phase correction strength: L 0.50, Ltf 1.00"]);
  });

  it("phase_band: joins the strength line (the same fraction) and marks the mains without phase correction", () => {
    const notes = [
      n("phase_strength", { role: "L", strength: "0.7" }),
      n("phase_strength", { role: "R", strength: "0" }),
      n("phase_band", { role: "L", enabled: "true", hz: "0.7" }),
      n("phase_band", { role: "R", enabled: "false" }),
    ];
    expect(lines("ko", notes)).toEqual(["위상 보정 세기: L 0.70, R 끔"]);
    expect(lines("en", notes)).toEqual(["Phase correction strength: L 0.70, R off"]);
    // Band records alone read the same way.
    expect(lines("ko", notes.slice(2))).toEqual(["위상 보정 세기: L 0.70, R 끔"]);
  });

  it("ba_cost: the alignment's cost, two decimals as upstream", () => {
    expect(lines("ko", [n("ba_cost", { value: "1.5" })])).toEqual(["BA 비용 1.50"]);
    expect(lines("en", [n("ba_cost", { value: "1.5" })])).toEqual(["BA cost 1.50"]);
  });

  it("ba_bus_pass: the cost the pass ends with, and when the first result is kept", () => {
    const better = n("ba_bus_pass", { first: "1.5", second: "1.25", kept_first: "false" });
    const kept = n("ba_bus_pass", { first: "1.5", second: "1.75", kept_first: "true" });
    expect(lines("ko", [better, kept])).toEqual(["BA 버스 PEQ 반영 비용 1.50 → 1.25", "BA 버스 PEQ 반영 비용 1.50 → 1.50 (1차 답 유지)"]);
    expect(lines("en", [better, kept])).toEqual(["BA cost with the sub bus PEQ 1.50 → 1.25", "BA cost with the sub bus PEQ 1.50 → 1.50 (first result kept)"]);
  });

  it("joins the per-record room modes, GD cuts and two-way subs into their existing lines", () => {
    const notes = [
      n("long_mode_boost", { cap_db: "3", hz: "40", t60_s: "2" }),
      n("long_mode_boost", { cap_db: "3", hz: "63.4", t60_s: "1.2" }),
      n("gd_cuts", { role: "L", index: "0", hz: "50", q: "2", gain_db: "-3" }),
      n("gd_cuts", { role: "L", index: "1", hz: "70", q: "2", gain_db: "-2" }),
      n("gd_cuts", { role: "R", index: "0", hz: "55", q: "2", gain_db: "-3" }),
      n("two_way", { role: "SW1", gain_db: "-1", xo_hz: "50" }),
      n("two_way", { role: "SW2", gain_db: "-1", xo_hz: "50" }),
    ];
    expect(lines("ko", notes)).toEqual([
      "오래 우는 룸 모드 2곳(40 Hz, 63 Hz)의 부스트를 3.0 dB로 제한했습니다.",
      "L: 군지연 피크를 줄이는 컷 2개를 넣었습니다.",
      "R: 군지연 피크를 줄이는 컷 1개를 넣었습니다.",
      "2웨이 서브(SW1·SW2): 저역 서브 게인 −1.0 dB, 크로스오버 50 Hz",
    ]);
    expect(lines("en", notes)[0]).toBe("Boost limited to 3.0 dB at long-ringing room modes: 2 (40 Hz, 63 Hz).");
  });

  it("shows a code the catalog has no sentence for as an unknown notice, never raw", () => {
    expect(lines("ko", [n("made_up_note", { role: "L", value: "1" })])).toEqual(["알 수 없는 알림"]);
    expect(lines("en", [n("made_up_note", { role: "L", value: "1" })])).toEqual(["Unknown notice"]);
  });
});
