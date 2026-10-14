import { describe, expect, it } from "vitest";
import { catalogs, pickLocale, translate } from "./index";

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("catalogs", () => {
  it("every locale has the same keys and placeholders as Korean", () => {
    const keys = Object.keys(catalogs.ko).sort();
    for (const [locale, catalog] of Object.entries(catalogs)) {
      expect(Object.keys(catalog).sort(), locale).toEqual(keys);
      for (const key of keys) {
        const source = (catalogs.ko as Record<string, string>)[key] ?? "";
        const target = (catalog as Record<string, string>)[key] ?? "";
        expect(placeholders(target), `${locale}:${key}`).toEqual(placeholders(source));
      }
    }
  });

  it("keeps the Korean register: 합니다체 sentences, no 해요체 statements", () => {
    // -세요 instructions are allowed; statements ending in -어요/-아요/-해요/-예요 are not.
    const friendly = /(?:어요|아요|해요|여요|예요|에요|래요|네요|군요|지요|죠)[.!?]?$/;
    const offenders = Object.entries(catalogs.ko).filter(([, text]) => text.split(/(?<=[.!?])\s+/).some((sentence) => friendly.test(sentence.trim())));
    expect(offenders).toEqual([]);
  });

  it("has no exclamation marks", () => {
    for (const [locale, catalog] of Object.entries(catalogs)) {
      const offenders = Object.entries(catalog).filter(([, text]) => /!/.test(text));
      expect(offenders, locale).toEqual([]);
    }
  });

  it("fills placeholders and picks a supported locale", () => {
    expect(translate("ko", "app.upstream", { version: "Beta 1.2.0" })).toBe("SECS 2 Beta 1.2.0 기준");
    expect(pickLocale(["en-US", "ko-KR"])).toBe("en");
    expect(pickLocale(["fr-FR"])).toBe("ko");
  });
});
