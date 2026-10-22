// Font gate (DESIGN.md §4): the bundled files are the ones manifest.json
// describes (same bytes), fonts.css references exactly them, every family
// shipped is one a stylesheet uses, the set stays under its size budget, and
// every character the catalogs can show has a glyph in the faces that must
// carry it (scripts/build-fonts.py writes the manifest from the files).
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

type Font = {
  file: string;
  family: string;
  weight: number | [number, number];
  bytes: number;
  sha256: string;
  codepoints: number;
  coverage: string;
};

const dir = fileURLToPath(new URL(".", import.meta.url));
const src = join(dir, "..");
const manifest = JSON.parse(readFileSync(join(dir, "manifest.json"), "utf8")) as { fonts: Font[] };
const sheet = readFileSync(join(dir, "fonts.css"), "utf8");
const SIZE_BUDGET = 6 * 1024 * 1024;

/** Code points a manifest entry maps, from its "20-7E A0 A2-FF" ranges. */
function points(font: Font): Set<number> {
  const set = new Set<number>();
  for (const range of font.coverage.split(" ")) {
    const [start, end = start] = range.split("-").map((hex) => parseInt(hex, 16));
    for (let p = start!; p <= end!; p++) set.add(p);
  }
  return set;
}

function strings(value: unknown, out: string[] = []): string[] {
  if (typeof value === "string") out.push(value);
  else if (Array.isArray(value)) value.forEach((v) => strings(v, out));
  else if (value && typeof value === "object") Object.values(value).forEach((v) => strings(v, out));
  return out;
}

/** Every code point the ko and en catalogs can put on screen (no controls). */
const catalog = new Set<number>();
for (const locale of ["ko", "en"]) {
  for (const text of strings(JSON.parse(readFileSync(join(src, "i18n", `${locale}.json`), "utf8")))) {
    for (const ch of text) {
      const p = ch.codePointAt(0)!;
      if (p > 0x20) catalog.add(p);
    }
  }
}
const hangul = (p: number) => p >= 0xac00 && p <= 0xd7a3;
const missing = (font: Font, wanted: Iterable<number>) => {
  const have = points(font);
  return [...wanted].filter((p) => !have.has(p)).map((p) => `${String.fromCodePoint(p)} U+${p.toString(16).toUpperCase().padStart(4, "0")}`);
};
const family = (name: string) => manifest.fonts.filter((f) => f.family === name);

describe("bundled fonts", () => {
  it("ships exactly the files the manifest lists, unchanged", () => {
    const files = readdirSync(dir).filter((name) => name.endsWith(".woff2")).sort();
    expect(files).toEqual(manifest.fonts.map((f) => f.file).sort());
    for (const font of manifest.fonts) {
      const bytes = readFileSync(join(dir, font.file));
      expect(bytes.length, font.file).toBe(font.bytes);
      expect(createHash("sha256").update(bytes).digest("hex"), font.file).toBe(font.sha256);
    }
  });

  it("references every bundled file from fonts.css, and nothing else", () => {
    const urls = [...sheet.matchAll(/url\("\.\/([^"]+)"\)/g)].map((m) => m[1]!).sort();
    expect(urls).toEqual(manifest.fonts.map((f) => f.file).sort());
    for (const font of manifest.fonts) {
      const face = new RegExp(`font-family: "${font.family}";\\s*src: url\\("\\./${font.file.replace(".", "\\.")}"\\)`);
      expect(sheet, font.file).toMatch(face);
    }
  });

  it("ships only families a stylesheet uses", () => {
    const sheets = readdirSync(join(src, "styles"), { recursive: true })
      .map(String)
      .filter((name) => name.endsWith(".css"))
      .map((name) => readFileSync(join(src, "styles", name), "utf8"))
      .join("\n");
    for (const name of new Set(manifest.fonts.map((f) => f.family))) {
      expect(sheets, name).toContain(`"${name}"`);
    }
  });

  it("stays under the size budget", () => {
    const total = manifest.fonts.reduce((sum, f) => sum + f.bytes, 0);
    expect(total).toBeLessThanOrEqual(SIZE_BUDGET);
  });

  it("maps every Hangul syllable in the label and readout faces", () => {
    const all = Array.from({ length: 0xd7a3 - 0xac00 + 1 }, (_, i) => 0xac00 + i);
    for (const font of [...family("Pretendard"), ...family("Sarasa Mono K")]) {
      expect(missing(font, all).length, font.file).toBe(0);
    }
  });

  it("gives every catalog character a glyph in the label and readout faces", () => {
    for (const font of [...family("Pretendard"), ...family("Sarasa Mono K")]) {
      expect(missing(font, catalog), font.file).toEqual([]);
    }
  });

  it("gives every catalog Hangul syllable and ASCII character a glyph in the band face", () => {
    // Orbit carries KS X 1001's syllables and Latin; anything else in a band
    // label falls back to Pretendard (fonts.css lists it next), which is
    // acceptable for a symbol but not for a word.
    const wanted = [...catalog].filter((p) => hangul(p) || p < 0x7f);
    for (const font of family("Orbit")) {
      expect(missing(font, wanted), font.file).toEqual([]);
    }
  });
});
