// Measured contrast (docs/DESIGN.md §2.4, docs/skins): every text pair of
// every skin in both themes reaches 4.5:1, every control border, focus ring
// and status glyph 3:1, and the dark copies of each skin stay identical.
// Run with CONTRAST_TABLE=1 to print the Markdown table DESIGN.md quotes.
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { cascade, colour, hex, hue, pair, parseBlocks, type Theme } from "./palette";

const dir = fileURLToPath(new URL(".", import.meta.url));
const tokens = readFileSync(join(dir, "tokens.css"), "utf8");
const skinDir = join(dir, "skins");
// A checkout without skins still measures the default (git keeps no empty directory).
const skinFiles = existsSync(skinDir) ? readdirSync(skinDir).filter((name) => name.endsWith(".css")).sort() : [];
const skins = skinFiles.map((name) => ({ id: name.replace(/\.css$/, ""), css: readFileSync(join(dir, "skins", name), "utf8") }));
const css = [tokens, ...skins.map((s) => s.css)];

type Pair = { fg: string; bg: string; min: number };
const TEXT = 4.5;
const GRAPHIC = 3;

/** The pairs every combination must hold (fg on bg). */
const PAIRS: Pair[] = [
  { fg: "--text", bg: "--bg", min: TEXT },
  { fg: "--text", bg: "--surface", min: TEXT },
  { fg: "--text", bg: "--surface-sunken", min: TEXT },
  { fg: "--text", bg: "--surface-raised", min: TEXT },
  { fg: "--text", bg: "--rail-bg", min: TEXT },
  { fg: "--text-muted", bg: "--bg", min: TEXT },
  { fg: "--text-muted", bg: "--surface", min: TEXT },
  { fg: "--text-muted", bg: "--surface-sunken", min: TEXT },
  { fg: "--text-muted", bg: "--surface-raised", min: TEXT },
  { fg: "--text-muted", bg: "--rail-bg", min: TEXT },
  { fg: "--text-faint", bg: "--surface", min: GRAPHIC },
  { fg: "--border-control", bg: "--surface", min: GRAPHIC },
  { fg: "--border-control", bg: "--surface-sunken", min: GRAPHIC },
  { fg: "--accent", bg: "--surface", min: TEXT },
  { fg: "--accent", bg: "--bg", min: GRAPHIC },
  { fg: "--on-accent", bg: "--accent", min: TEXT },
  { fg: "--on-accent", bg: "--accent-hover", min: TEXT },
  { fg: "--success", bg: "--surface", min: TEXT },
  { fg: "--warning", bg: "--surface", min: TEXT },
  { fg: "--danger", bg: "--surface", min: TEXT },
  { fg: "--info", bg: "--surface", min: TEXT },
  { fg: "--success", bg: "--success-soft", min: TEXT },
  { fg: "--warning", bg: "--warning-soft", min: TEXT },
  { fg: "--danger", bg: "--danger-soft", min: TEXT },
  { fg: "--info", bg: "--info-soft", min: TEXT },
  { fg: "--on-danger", bg: "--danger", min: TEXT },
  { fg: "--on-success", bg: "--success", min: TEXT },
  { fg: "--plot-text", bg: "--plot-bg", min: TEXT },
  { fg: "--map-ink", bg: "--map-bg", min: TEXT },
  { fg: "--meter-fill", bg: "--meter-track", min: GRAPHIC },
];

const combos: { skin: string | undefined; theme: Theme; accent?: "tutorial" }[] = [
  { skin: undefined, theme: "light" },
  { skin: undefined, theme: "dark" },
  { skin: undefined, theme: "light", accent: "tutorial" },
  { skin: undefined, theme: "dark", accent: "tutorial" },
  ...skins.flatMap((s) => [{ skin: s.id, theme: "light" as const }, { skin: s.id, theme: "dark" as const }]),
];

const label = (c: (typeof combos)[number]) => `${c.skin ?? "default"} ${c.theme}${c.accent ? ` ${c.accent}` : ""}`;

describe("measured contrast", () => {
  for (const combo of combos) {
    it(`${label(combo)} keeps every pair above its floor`, () => {
      const palette = cascade(css, combo);
      const short: string[] = [];
      for (const { fg, bg, min } of PAIRS) {
        const ratio = pair(palette, fg, bg);
        if (ratio < min) short.push(`${fg} on ${bg}: ${ratio.toFixed(2)} < ${min}`);
      }
      expect(short).toEqual([]);
    });
  }

  it("prints the table DESIGN.md quotes (CONTRAST_TABLE=1)", () => {
    if (!process.env["CONTRAST_TABLE"]) return;
    const lines = ["| 쌍 | " + combos.map(label).join(" | ") + " |", "|---|" + combos.map(() => "---").join("|") + "|"];
    for (const { fg, bg } of PAIRS) {
      lines.push(`| \`${fg}\` / \`${bg}\` | ` + combos.map((c) => pair(cascade(css, c), fg, bg).toFixed(2)).join(" | ") + " |");
    }
    lines.push("", "| 토큰 | " + combos.map(label).join(" | ") + " |", "|---|" + combos.map(() => "---").join("|") + "|");
    for (const name of ["--bg", "--surface", "--text", "--text-muted", "--accent", "--on-accent", "--success", "--warning", "--danger", "--info"]) {
      lines.push(`| \`${name}\` | ` + combos.map((c) => hex(colour(cascade(css, c), name, colour(cascade(css, c), "--bg")))).join(" | ") + " |");
    }
    console.log(lines.join("\n"));
  });
});

describe("hue discipline (color.md)", () => {
  const SEMANTIC = ["--success", "--warning", "--danger", "--info"];
  const apart = (a: number, b: number) => Math.min(Math.abs(a - b), 360 - Math.abs(a - b));

  for (const combo of combos.filter((c) => !c.accent)) {
    it(`${label(combo)} keeps the accent at least 15° from every status hue`, () => {
      const palette = cascade(css, combo);
      const accent = hue(palette, "--accent");
      expect(accent).not.toBeNull();
      for (const name of SEMANTIC) {
        const h = hue(palette, name);
        if (h === null) continue;
        expect(apart(accent!, h), `${name} (${h}°) vs accent (${accent}°)`).toBeGreaterThanOrEqual(15);
      }
    });

    it(`${label(combo)} keeps the accent off the teal band (170–215°) unless the skin's constitution claims it`, () => {
      // 2026-10-22 audit (docs/skins/default.md): the default accent left
      // teal. Signal Matrix's cyan lamp is the one documented exception.
      if (combo.skin === "matrix") return;
      const accent = hue(cascade(css, combo), "--accent")!;
      expect(accent < 170 || accent > 215, `accent hue ${accent}°`).toBe(true);
    });

    it(`${label(combo)} holds one neutral temperature across the ramp`, () => {
      const palette = cascade(css, combo);
      const hues = ["--bg", "--surface", "--surface-sunken", "--surface-raised", "--rail-bg", "--text", "--text-muted", "--border", "--border-control"]
        .map((name) => hue(palette, name))
        .filter((h): h is number => h !== null);
      for (const h of hues) expect(apart(h, hues[0]!), `neutral hues ${hues.join(", ")}`).toBeLessThanOrEqual(20);
    });

    it(`${label(combo)} has no pure black or white ground`, () => {
      const palette = cascade(css, combo);
      for (const name of ["--bg", "--surface", "--surface-raised", "--rail-bg"]) {
        const h = hex(colour(palette, name));
        expect(h, name).not.toBe("#000000");
        expect(h, name).not.toBe("#ffffff");
      }
    });
  }
});

describe.runIf(skins.length > 0)("skin stylesheets", () => {
  for (const skin of skins) {
    const blocks = parseBlocks(skin.css);
    const selector = `[data-skin="${skin.id}"]`;

    it(`${skin.id}: keeps the system and explicit dark blocks identical`, () => {
      const system = blocks.filter((b) => b.selector === `${selector}:not([data-theme="light"])`);
      const explicit = blocks.filter((b) => b.selector === `${selector}[data-theme="dark"]`);
      expect(system.length, "system-preference dark block").toBeGreaterThan(0);
      expect(explicit.length, "explicit dark block").toBeGreaterThan(0);
      const flat = (list: typeof blocks) => list.flatMap((b) => [...b.declarations.entries()].map(([k, v]) => `${k}: ${v}`));
      expect(flat(system)).toEqual(flat(explicit));
    });

    it(`${skin.id}: overrides the same colour tokens in light and dark`, () => {
      // A token whose value is a colour (oklch, a var() chain or a shadow)
      // needs a value in both themes; theme-independent tokens (fonts,
      // sizes, pitches) live in the light block only.
      const colourNames = (list: typeof blocks) =>
        list.flatMap((b) => [...b.declarations.entries()]).filter(([, v]) => /oklch\(|var\(--/.test(v)).map(([k]) => k).sort();
      const light = colourNames(blocks.filter((b) => b.selector === selector));
      const dark = colourNames(blocks.filter((b) => b.selector === `${selector}[data-theme="dark"]`));
      expect(dark).toEqual(light);
    });
  }
});
