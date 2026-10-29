import { describe, expect, it } from "vitest";
import { DESIGN_RATE, OUTPUT_RATES, countedRate, extraRatesFor, rateChoice, rateText, rateValue, sameRate, tapsAt } from "./outputRate";

const number = (value: number, digits = 0) => value.toFixed(digits);

describe("output rate", () => {
  it("counts 자동 as 48 kHz and a fixed rate as itself", () => {
    expect(countedRate("auto")).toBe(DESIGN_RATE);
    for (const rate of OUTPUT_RATES) expect(countedRate({ fixed: rate })).toBe(rate);
  });

  it("converts 48 kHz taps to the output rate like the exporter (round half up, at least one)", () => {
    expect(tapsAt(2048, 48_000)).toBe(2048);
    expect(tapsAt(2048, 96_000)).toBe(4096);
    expect(tapsAt(2048, 192_000)).toBe(8192);
    // 2048 × 44100 / 48000 = 1881.6
    expect(tapsAt(2048, 44_100)).toBe(1882);
    // 1001 × 44100 / 48000 = 919.66; 3 × 88200 / 48000 = 5.5 rounds up
    expect(tapsAt(1001, 44_100)).toBe(920);
    expect(tapsAt(3, 88_200)).toBe(6);
    expect(tapsAt(65_536, 176_400)).toBe(240_845);
    expect(tapsAt(1, 44_100)).toBe(1);
    expect(tapsAt(0, 96_000)).toBe(0);
  });

  it("round-trips the select's values", () => {
    expect(rateValue("auto")).toBe("auto");
    expect(rateValue({ fixed: 88_200 })).toBe("88200");
    expect(rateChoice("auto")).toBe("auto");
    expect(rateChoice("44100")).toEqual({ fixed: 44_100 });
    expect(sameRate({ fixed: 96_000 }, { fixed: 96_000 })).toBe(true);
    expect(sameRate("auto", { fixed: 48_000 })).toBe(false);
  });

  it("keeps the extra rates the DSP can run at, without the written rate, in rate order", () => {
    expect(extraRatesFor(96_000, [192_000, 44_100, 96_000, 32_000])).toEqual([44_100, 192_000]);
    expect(extraRatesFor(48_000, [])).toEqual([]);
    expect(extraRatesFor(48_000, [48_000])).toEqual([]);
  });

  it("names a rate in kHz with a decimal only where it has one", () => {
    expect(rateText(number, 96_000)).toBe("96 kHz");
    expect(rateText(number, 44_100)).toBe("44.1 kHz");
    expect(rateText(number, 176_400)).toBe("176.4 kHz");
  });
});
