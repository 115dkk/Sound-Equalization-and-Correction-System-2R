// Message catalogs keyed by stable ids (docs/adr/0003). Korean is the source
// language; every locale must carry every key with the same placeholders,
// which i18n.test.ts checks. Keys are the ids docs/M5-reach.md cites.
import en from "./en.json";
import ko from "./ko.json";

export type MessageKey = keyof typeof ko;
export const catalogs = { ko, en } as const;
export type Locale = keyof typeof catalogs;
export const LOCALES: readonly Locale[] = ["ko", "en"];

export type MessageParams = Record<string, string | number>;

export function pickLocale(languages: readonly string[]): Locale {
  for (const language of languages) {
    const base = language.toLowerCase().split("-")[0];
    if (base === "ko" || base === "en") return base;
  }
  return "ko";
}

/** True when `key` names a catalog entry (for codes that arrive from the service). */
export function isMessageKey(key: string): key is MessageKey {
  return Object.prototype.hasOwnProperty.call(catalogs.ko, key);
}

export function translate(locale: Locale, key: MessageKey, params: MessageParams = {}): string {
  const template: string = catalogs[locale][key] ?? catalogs.ko[key];
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = params[name];
    return value === undefined ? match : String(value);
  });
}
