import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { DEFAULT_LOGO, LOGOS, SMALL_LOGOS } from "./logo";

const assets = resolve(import.meta.dirname, "../../../../../src/secs/wizard/assets");
// Vite inlines small files as data URLs (quotes and spaces rewritten) and
// serves larger ones by path.
const squash = (text: string) => text.replace(/["'\s]/g, "");

describe("logos", () => {
  it("are upstream's drawings, with the confirmed one as the default", () => {
    expect(DEFAULT_LOGO).toBe("drip");
    for (const [icon, file] of [["drip", "logo_b8"], ["bubble", "logo_b6"]] as const) {
      for (const [urls, name] of [[LOGOS, `${file}.svg`], [SMALL_LOGOS, `${file}_small.svg`]] as const) {
        const url = urls[icon];
        if (url.startsWith("data:")) expect(squash(decodeURIComponent(url.slice(url.indexOf(",") + 1)))).toBe(squash(readFileSync(resolve(assets, name), "utf8")));
        else expect(url).toContain(name);
      }
    }
  });
});
