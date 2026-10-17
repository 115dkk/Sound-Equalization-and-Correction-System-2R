import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { DEFAULT_LOGO, LOGOS, SMALL_LOGOS } from "./logo";

const upstream = resolve(import.meta.dirname, "../../../../../src/secs/wizard/assets");
const own = resolve(import.meta.dirname, "../../../icons/logo");
// Vite inlines small files as data URLs (quotes and spaces rewritten) and
// serves larger ones by path.
const squash = (text: string) => text.replace(/["'\s]/g, "");

describe("logos", () => {
  it("are upstream's confirmed logo, the default, and SECS 2R's own", () => {
    expect(DEFAULT_LOGO).toBe("expert");
    for (const [icon, folder, file] of [["expert", upstream, "logo_b8"], ["civilian", own, "civilian"]] as const) {
      for (const [urls, name] of [[LOGOS, `${file}.svg`], [SMALL_LOGOS, `${file}_small.svg`]] as const) {
        const url = urls[icon];
        if (url.startsWith("data:")) expect(squash(decodeURIComponent(url.slice(url.indexOf(",") + 1)))).toBe(squash(readFileSync(resolve(folder, name), "utf8")));
        else expect(url).toContain(name);
      }
    }
  });
});
