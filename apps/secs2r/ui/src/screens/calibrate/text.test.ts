import { describe, expect, it } from "vitest";
import { translate, type Locale } from "../../i18n";
import type { Translate } from "../../i18n/react";
import type { Notice } from "../../ipc/bindings/Notice";
import { ipcError } from "../../ipc/mock";
import { baseSettings, lfNoteCases, lowProtectNotes, rcBandsNotes, rcLimitNotes, refusedBandEdit } from "./fixtures";
import { rejectionPlace, type RangeRejection } from "./settingsView";
import { LOW_PROTECT_NEXT, lowProtectFailures, noteLines, noticeText, rangeNotices, rangeRejectionText, type FormatNumber } from "./text";

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
  it("formats the reported resampling count and target rate in both locales", () => {
    expect(lines("ko", [n("measurement_resampled", { count: "1500", target_hz: "48000" })])).toEqual(["측정 1,500개를 계산용 48,000 Hz 데이터로 변환했습니다."]);
    expect(lines("en", [n("measurement_resampled", { count: "1", target_hz: "48000" })])).toEqual(["Converted 1 measurement to 48,000 Hz for calculation."]);
    expect(lines("en", [n("measurement_resampled", { count: "2", target_hz: "48000" })])).toEqual(["Converted 2 measurements to 48,000 Hz for calculation."]);
  });
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

  it("ba_cost: the alignment's score (upstream's cost), two decimals as upstream", () => {
    expect(lines("ko", [n("ba_cost", { value: "1.5" })])).toEqual(["BA 평가값 1.50 (낮을수록 좋음)"]);
    expect(lines("en", [n("ba_cost", { value: "1.5" })])).toEqual(["BA score 1.50 (lower is better)"]);
  });

  it("ba_bus_pass: the cost the pass ends with, and when the first result is kept", () => {
    const better = n("ba_bus_pass", { first: "1.5", second: "1.25", kept_first: "false" });
    const kept = n("ba_bus_pass", { first: "1.5", second: "1.75", kept_first: "true" });
    expect(lines("ko", [better, kept])).toEqual(["서브 버스 PEQ 반영 후 BA 평가값 1.50 → 1.25 (낮을수록 좋음)", "서브 버스 PEQ 반영 후 BA 평가값 1.50 → 1.50 (낮을수록 좋음 · 첫 결과 유지)"]);
    expect(lines("en", [better, kept])).toEqual(["BA score with the sub-bus PEQ 1.50 → 1.25 (lower is better)", "BA score with the sub-bus PEQ 1.50 → 1.50 (lower is better; first result kept)"]);
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
      "울림이 오래 남는 룸 모드 2곳(40 Hz, 63 Hz)의 부스트를 3.0 dB로 제한했습니다.",
      "L: GD 피크를 줄이는 컷 2개를 넣었습니다.",
      "R: GD 피크를 줄이는 컷 1개를 넣었습니다.",
      "2웨이 서브(SW1·SW2): 저역 서브 게인 −1.0 dB, 크로스오버 50 Hz",
    ]);
    expect(lines("en", notes)[0]).toBe("Boost limited to 3.0 dB at long-ringing room modes: 2 (40 Hz, 63 Hz).");
  });

  it("rc_limit: the upper limit with its basis and evidence (ADR 0021), older notes as before", () => {
    const text = (locale: Locale) => {
      const { t, number } = words(locale);
      return rcLimitNotes.map((note) => noticeText(t, number, note));
    };
    expect(text("ko")).toEqual([
      "RC 상한 154 Hz(엄격한 슈뢰더 154 Hz)",
      "RC 상한 308 Hz(엄격한 슈뢰더 154 Hz × 2)",
      "RC 상한 1,242 Hz(엄격한 슈뢰더 310 Hz × 4)",
      "RC 상한 200 Hz(너그러운 슈뢰더, 200 Hz 대역 T60 0.50초)",
      "RC 상한 400 Hz(너그러운 슈뢰더 200 Hz × 2, 대역 T60 0.50초)",
      // The band (160 Hz) is below the strict value, which the lenient basis keeps.
      "RC 상한 250 Hz(너그러운 슈뢰더, 엄격한 값 250 Hz)",
      "RC 상한 500 Hz(너그러운 슈뢰더, 엄격한 값 250 Hz × 2)",
      "RC 상한 287 Hz(위치 9곳 일치도)",
      "RC 상한 300 Hz(직접 입력)",
      // No room volume: the earlier line, and the hint to enter the volume after it.
      "RC 상한 500 Hz(자동)",
      "방 부피가 없어 자동 상한은 500 Hz입니다. 방 크기를 넣으면 슈뢰더 주파수로 정해집니다.",
      // A note from before the bases keeps its line.
      "RC 상한 310 Hz(자동, 슈뢰더 310 Hz)",
    ]);
    expect(text("en").slice(0, 8)).toEqual([
      "RC upper limit 154 Hz (strict Schroeder 154 Hz)",
      "RC upper limit 308 Hz (strict Schroeder 154 Hz × 2)",
      "RC upper limit 1,242 Hz (strict Schroeder 310 Hz × 4)",
      "RC upper limit 200 Hz (lenient Schroeder, T60 0.50 s in the 200 Hz band)",
      "RC upper limit 400 Hz (lenient Schroeder 200 Hz × 2, band T60 0.50 s)",
      "RC upper limit 250 Hz (lenient Schroeder, strict value 250 Hz)",
      "RC upper limit 500 Hz (lenient Schroeder, strict value 250 Hz × 2)",
      "RC upper limit 287 Hz (agreement of 9 positions)",
    ]);
    // Without a band the lenient basis falls back to the strict value; a strict note's band is not evidence.
    const { t, number } = words("ko");
    expect(noticeText(t, number, n("rc_limit", { hz: "200", auto: "true", basis: "lenient", factor: "1", band_hz: "200", band_t60_s: "0.5" }))).toBe("RC 상한 200 Hz(너그러운 슈뢰더, 200 Hz 대역 T60 0.50초)");
    expect(noticeText(t, number, n("rc_limit", { hz: "154", auto: "true", schroeder_hz: "154", basis: "strict", factor: "1", band_hz: "200", band_t60_s: "0.5" }))).toBe("RC 상한 154 Hz(엄격한 슈뢰더 154 Hz)");
  });

  it("drops the room volume lines from the range card when agreement set the limit", () => {
    const volume = [n("rc_room_volume", { volume_m3: "62.6" }), n("rc_no_room_volume", {}), n("rc_auto_cap", { hz: "287" })];
    const agreement = n("rc_limit", { hz: "287", auto: "true", basis: "agreement", factor: "1", positions: "9", schroeder_hz: "400" });
    expect(rangeNotices([agreement, ...volume])).toEqual([agreement]);
    const strict = n("rc_limit", { hz: "154", auto: "true", basis: "strict", factor: "1", schroeder_hz: "154" });
    expect(rangeNotices([strict, volume[0]!])).toEqual([strict, volume[0]]);
    // A manual limit on the agreement basis keeps whatever follows it.
    const manual = n("rc_limit", { hz: "300", auto: "false", basis: "agreement", factor: "1" });
    expect(rangeNotices([manual, volume[0]!])).toEqual([manual, volume[0]]);
  });

  it("shows a code the catalog has no sentence for as an unknown notice, never raw", () => {
    expect(lines("ko", [n("made_up_note", { role: "L", value: "1" })])).toEqual(["알 수 없는 알림"]);
    expect(lines("en", [n("made_up_note", { role: "L", value: "1" })])).toEqual(["Unknown notice"]);
  });

  it("states 극저역 보호 per channel: the roll-off, a sub's high-pass, and the bus that could not hold as a warning (ADR 0022)", () => {
    // A sub with the high-pass the engine adds (the roll-off over √2), a sub keeping the user's, a main, the bus.
    expect(lines("ko", lowProtectNotes)).toEqual([
      "극저역 보호(SW1): 22 Hz 아래 부스트 안 함, 16 Hz 하이패스",
      "극저역 보호(SW2): 28 Hz 아래 부스트 안 함, 25 Hz 하이패스",
      "극저역 보호(L): 30 Hz 아래 부스트 안 함",
      "극저역 보호(SW1 + SW2): 22 Hz 아래 부스트를 막지 못해 원래 필터를 그대로 썼습니다.",
    ]);
    expect(lines("en", lowProtectNotes)).toEqual([
      "Low-end protection (SW1): no boost below 22 Hz, 16 Hz high-pass",
      "Low-end protection (SW2): no boost below 28 Hz, 25 Hz high-pass",
      "Low-end protection (L): no boost below 30 Hz",
      "Low-end protection (SW1 + SW2): boost below 22 Hz could not be limited, so the original filters were kept.",
    ]);
    // The engine's roll-offs are unrounded; each role reads as the screen's code, in a group too.
    expect(lines("ko", [n("low_protect", { role: "Lb", rolloff_hz: "31.498026247371826", limited: "true" })])).toEqual(["극저역 보호(Lrs): 31 Hz 아래 부스트 안 함"]);
    expect(lines("ko", [n("low_protect", { role: "Tfl + Tfr", rolloff_hz: "40", limited: "false" })])).toEqual(["극저역 보호(Ltf + Rtf): 40 Hz 아래 부스트를 막지 못해 원래 필터를 그대로 썼습니다."]);
    // Only the bus that kept its fit is a warning, never a success line; it says what to do.
    expect(lowProtectFailures(lowProtectNotes)).toEqual([lowProtectNotes[3]]);
    const { t } = words("ko");
    expect(t(LOW_PROTECT_NEXT)).toBe("부스트 상한을 낮추거나 RC 하한을 올린 뒤 '다시 보정하기'를 누르십시오.");
    expect(words("en").t(LOW_PROTECT_NEXT)).toBe("Lower the boost limit or raise the RC lower limit, then press 'Correct again'.");
  });
});

describe("저음 도착 맞추기 and 방 울림 줄이기 (ADR 0025)", () => {
  it("reads every case the service sends as a line per record, in both languages", () => {
    expect(lines("ko", lfNoteCases)).toEqual([
      "저음 도착 맞추기(L): 위상 보정 강도 100 %, 고음과의 도착 시간 차이 +0.4 ms (+는 저음이 늦게 도착)",
      "저음 도착 맞추기(Ltf): 위상 보정 강도 75 %, 고음과의 도착 시간 차이 −1.2 ms (+는 저음이 늦게 도착), 지연·앞울림 한도로 도착을 완전히 맞추지 못함",
      "저음 도착 맞추기(C): 위상 보정 강도 0 %, 지연·앞울림 한도로 도착을 완전히 맞추지 못함",
      "방 울림 줄이기(L): 42 Hz 룸 모드의 측정 T60 0.82초, 목표 0.31초, 8.4 dB 컷",
      "방 울림 줄이기(L): 63 Hz 룸 모드의 측정 T60 0.55초, 목표 0.31초, 4.9 dB 컷",
      "방 울림 줄이기(Lrs): 목표 T60 0.31초, 컷을 적용할 룸 모드 없음",
    ]);
    expect(lines("en", lfNoteCases)).toEqual([
      "Align bass arrival (L): phase correction strength 100%, bass arrival relative to treble +0.4 ms (+ means bass arrives later)",
      "Align bass arrival (Ltf): phase correction strength 75%, bass arrival relative to treble −1.2 ms (+ means bass arrives later), partial alignment due to delay and pre-ringing limits",
      "Align bass arrival (C): phase correction strength 0%, partial alignment due to delay and pre-ringing limits",
      "Shorten room ringing (L): 42 Hz room mode measured T60 0.82 s, target 0.31 s, 8.4 dB cut",
      "Shorten room ringing (L): 63 Hz room mode measured T60 0.55 s, target 0.31 s, 4.9 dB cut",
      "Shorten room ringing (Lrs): target T60 0.31 s, no room mode eligible for a cut",
    ]);
  });

  it("rounds the engine's unrounded values and reads a cut depth by its size whatever its sign", () => {
    expect(lines("ko", [n("lf_align", { role: "R", strength: "0.6666666666666666", arrival_ms: "2.4999999999999996", limited: "false" })])).toEqual([
      "저음 도착 맞추기(R): 위상 보정 강도 67 %, 고음과의 도착 시간 차이 +2.5 ms (+는 저음이 늦게 도착)",
    ]);
    // `limited` other than "true" adds nothing; a strength it cannot read is shown as sent.
    expect(lines("en", [n("lf_align", { role: "R", strength: "x", limited: "false" })])).toEqual(["Align bass arrival (R): phase correction strength x"]);
    expect(lines("ko", [n("mode_decay", { role: "C", hz: "51.8", t60_s: "0.6123", target_s: "0.25", depth_db: "6" })])).toEqual([
      "방 울림 줄이기(C): 52 Hz 룸 모드의 측정 T60 0.61초, 목표 0.25초, 6.0 dB 컷",
    ]);
  });
});

describe("extra correction bands (ADR 0024)", () => {
  it("rc_bands: every band of the merged set after the calculation, in both languages", () => {
    const [one, two, merged] = rcBandsNotes;
    expect(one!.params.bands).toBe("20-310.42,15000-20000");
    expect(lines("ko", [one!])).toEqual(["RC 보정 구간 20~310 Hz, 15,000~20,000 Hz"]);
    expect(lines("en", [one!])).toEqual(["RC correction bands 20–310 Hz, 15,000–20,000 Hz"]);
    expect(lines("ko", [two!])).toEqual(["RC 보정 구간 20~310 Hz, 2,000~4,000 Hz, 15,000~20,000 Hz"]);
    // A band overlapping the RC band is merged into it by the engine: one band.
    expect(lines("ko", [merged!])).toEqual(["RC 보정 구간 20~800 Hz"]);
    // A list it cannot read is shown as sent.
    expect(lines("en", [n("rc_bands", { bands: "garbled" })])).toEqual(["RC correction bands garbled"]);
  });

  it("says at the refused input what to enter instead, for each reason the service gives", () => {
    const two = { ...baseSettings, rc_bands: [{ lo_hz: 2000, hi_hz: 4000 }, { lo_hz: 15000, hi_hz: 20000 }] };
    const text = (locale: Locale, rejection: RangeRejection, settings = two) => {
      const { t, number } = words(locale);
      return rangeRejectionText(t, number, rejection, rejectionPlace(rejection, settings));
    };
    const refusal = (edit: RangeRejection["edit"], params: Record<string, string>): RangeRejection => ({ edit, error: ipcError("invalid_request", "", { field: edit.kind, ...params }) });
    expect(text("ko", refusal({ kind: "rc_lo", hz: 15 }, { reason: "rc_lo_range", min: "20", max: "200" }))).toBe("20–200 Hz 사이의 값을 넣으십시오.");
    expect(text("en", refusal({ kind: "rc_lo", hz: 15 }, { reason: "rc_lo_range", min: "20", max: "200" }))).toBe("Enter a value in 20–200 Hz.");
    expect(text("ko", refusal({ kind: "rc_lo", hz: 160 }, { reason: "rc_lo_above_hi", hi_hz: "150" }))).toBe("상한 150 Hz보다 낮은 값을 넣으십시오.");
    expect(text("ko", refusal({ kind: "rc_hi", hz: 120 }, { reason: "rc_hi_below_lo", lo_hz: "150" }))).toBe("하한 150 Hz보다 높은 값을 넣으십시오.");
    expect(text("en", refusal({ kind: "rc_hi", hz: 120 }, { reason: "rc_hi_below_lo", lo_hz: "150" }))).toBe("Enter a value above the lower limit, 150 Hz.");
    expect(text("ko", refusal({ kind: "rc_hi", hz: 30000 }, { reason: "rc_hi_range", min: "100", max: "20000" }))).toBe("100–20000 Hz 사이의 값을 넣으십시오.");
    expect(text("ko", refusal({ kind: "rc_bands", bands: [] }, { reason: "rc_band_count", max: "4" }))).toBe("추가 구간은 4개까지 둘 수 있습니다.");
    // A band: the upper edge set below the lower one says to stay above it; the lower edge set above the upper one, below it.
    expect(text("ko", refusedBandEdit)).toBe("하한 15,000 Hz보다 높은 값을 넣으십시오.");
    expect(text("en", refusedBandEdit)).toBe("Enter a value above the lower limit, 15,000 Hz.");
    expect(text("ko", refusal({ kind: "rc_bands", bands: [] }, { reason: "rc_band_range", index: "1", lo_hz: "5000", hi_hz: "4000" }))).toBe("상한 4,000 Hz보다 낮은 값을 넣으십시오.");
    expect(text("ko", refusal({ kind: "rc_bands", bands: [] }, { reason: "rc_band_range", index: "2", lo_hz: "15000", hi_hz: "25000" }))).toBe("20–20000 Hz 사이의 값을 넣으십시오.");
    expect(text("ko", refusal({ kind: "rc_bands", bands: [] }, { reason: "rc_band_range", index: "3", lo_hz: "10", hi_hz: "50" }))).toBe("하한이 상한보다 낮고 20–20000 Hz 안에 드는 구간을 넣으십시오.");
  });
});
