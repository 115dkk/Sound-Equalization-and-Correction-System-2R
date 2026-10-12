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

  it("fills placeholders and picks a supported locale", () => {
    expect(translate("ko", "app.upstream", { version: "Beta 1.2.0" })).toBe("SECS 2 Beta 1.2.0 기준");
    expect(pickLocale(["en-US", "ko-KR"])).toBe("en");
    expect(pickLocale(["fr-FR"])).toBe("ko");
  });
});
