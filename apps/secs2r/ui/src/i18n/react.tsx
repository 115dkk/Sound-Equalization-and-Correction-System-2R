// React access to the catalogs: one provider at the root, `useT()` below it.
// Changing the locale re-renders every string; nothing caches text.
import { createContext, useContext, useMemo, type ReactNode } from "react";
import { translate, type Locale, type MessageKey, type MessageParams } from "./index";

export type Translate = (key: MessageKey, params?: MessageParams) => string;

type I18nValue = { locale: Locale; t: Translate; number: (value: number, digits?: number) => string };

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ locale, children }: { locale: Locale; children: ReactNode }) {
  const value = useMemo<I18nValue>(() => {
    const formats = new Map<number, Intl.NumberFormat>();
    const tag = locale === "ko" ? "ko-KR" : "en-US";
    return {
      locale,
      t: (key, params) => translate(locale, key, params),
      number: (value, digits = 0) => {
        let format = formats.get(digits);
        if (!format) {
          format = new Intl.NumberFormat(tag, { minimumFractionDigits: digits, maximumFractionDigits: digits });
          formats.set(digits, format);
        }
        // A real minus sign, not a hyphen, so negative dB values read as numbers.
        return format.format(value).replace("-", "−");
      },
    };
  }, [locale]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error("useT() needs an <I18nProvider> above it");
  return value;
}

export const useT = (): Translate => useI18n().t;
export const useLocale = (): Locale => useI18n().locale;
export const useNumber = () => useI18n().number;

/** The current language's number format, as `useNumber` gives it: `number(value, digits)`. */
export type FormatNumber = (value: number, digits?: number) => string;

/** "+1.5" / "−3.0": a sign on every value but one that rounds to zero. */
export function signedNumber(number: FormatNumber, value: number, digits: number): string {
  const text = number(value, digits);
  return value > 0 && text !== number(0, digits) ? `+${text}` : text;
}

/** A value without group separators ("20000", not "20,000"), for numbers the user types back into a field. */
export function plainNumber(number: FormatNumber, value: number, digits = 0): string {
  return number(value, digits).replace(/,/g, "");
}

/** m:ss for elapsed and remaining time (h:mm:ss past an hour). */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}
