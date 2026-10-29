// The DSP's output rate (ADR 0017). Design stays at 48 kHz; the exported
// filters are moved to the rate the DSP runs at. The screens count taps at
// that rate: the DSP card's tap choices, the 보정 budget lines and the export
// rows. 자동 is decided by the service when it exports, so the screens count
// it as 48 kHz, the rate it falls back to.
import type { OutputRateChoice } from "../../ipc/bindings/OutputRateChoice";

/** The rate measurement and design run at (`secs_dsp::rate::DESIGN_RATE`). */
export const DESIGN_RATE = 48_000;

/** The rates a DSP profile can run at, in the order the choice lists them (`secs_dsp::rate::OUTPUT_RATES`). */
export const OUTPUT_RATES = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000] as const;

/** The rate the screens count with: the fixed rate, or 48 kHz for 자동. */
export function countedRate(choice: OutputRateChoice): number {
  return choice === "auto" ? DESIGN_RATE : choice.fixed;
}

/**
 * A 48 kHz FIR length at `rate`, as the exporter writes it
 * (`secs_dsp::rate::taps_at`): the same time length, rounded half up, at
 * least one tap for a filter that has any.
 */
export function tapsAt(taps: number, rate: number): number {
  if (taps <= 0) return 0;
  return Math.max(1, Math.floor((taps * rate + DESIGN_RATE / 2) / DESIGN_RATE));
}

/** The select's value for a choice: `auto`, or the rate in Hz. */
export function rateValue(choice: OutputRateChoice): string {
  return choice === "auto" ? "auto" : String(choice.fixed);
}

/** The choice a select value names. */
export function rateChoice(value: string): OutputRateChoice {
  return value === "auto" ? "auto" : { fixed: Number(value) };
}

export function sameRate(a: OutputRateChoice, b: OutputRateChoice): boolean {
  return rateValue(a) === rateValue(b);
}

/**
 * The convolution IR's extra rates to send: the chosen ones the DSP can run
 * at, without the rate the files are written at (counted, so 48 kHz for
 * 자동), in rate order.
 */
export function extraRatesFor(rate: number, chosen: readonly number[]): number[] {
  return OUTPUT_RATES.filter((candidate) => candidate !== rate && chosen.includes(candidate));
}

/** "96 kHz", "44.1 kHz": one decimal only where the rate has one. */
export function rateText(number: (value: number, digits?: number) => string, rate: number): string {
  return `${number(rate / 1000, rate % 1000 === 0 ? 0 : 1)} kHz`;
}
