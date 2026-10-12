// Message catalogs keyed by stable ids (docs/adr/0003). Korean is the source
// language; every locale must carry every key with the same placeholders,
// which i18n.test.ts checks.
import en from "./en.json";
import ko from "./ko.json";

export type MessageKey = keyof typeof ko;
export const catalogs = { ko, en } as const;
export type Locale = keyof typeof catalogs;

export function pickLocale(languages: readonly string[]): Locale {
  for (const language of languages) {
    const base = language.toLowerCase().split("-")[0];
    if (base === "ko" || base === "en") return base;
  }
  return "ko";
}

export function translate(locale: Locale, key: MessageKey, params: Record<string, string> = {}): string {
  const template: string = catalogs[locale][key] ?? catalogs.ko[key];
  return template.replace(/\{(\w+)\}/g, (match, name: string) => params[name] ?? match);
}
