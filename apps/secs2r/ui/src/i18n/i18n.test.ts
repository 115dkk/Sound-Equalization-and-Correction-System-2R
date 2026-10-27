import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { catalogs, pickLocale, translate } from "./index";

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

/* ── Where the code names keys ──────────────────────────────────────────── */

const src = fileURLToPath(new URL("..", import.meta.url));

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === "bindings" ? [] : sources(path);
    return /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) ? [path] : [];
  });
}

// `data-ui="…"` and `id="…"` attributes are element ids (docs/DESIGN.md §11).
// They share the key namespace, so they are taken out before looking for keys.
const code = sources(src)
  .map((path) => readFileSync(path, "utf8").replace(/(?<![\w-])(?:data-ui|id)="[^"]*"/g, ""))
  .join("\n");

/**
 * Keys the code builds from a code it gets at run time (a service notice, a
 * job stage, a quest id). Each entry is the template as the source writes it
 * and the keys it can produce; the template must still be in the source.
 */
const BUILT: readonly { template: string; keys: RegExp }[] = [
  { template: "jobs.log.${code}", keys: /^jobs\.log\.[a-z_]+$/ },
  { template: "jobs.stage.${stage}", keys: /^jobs\.stage\.[a-z_.]+$/ },
  { template: "notice.${notice.code}", keys: /^notice\.[a-z_]+$/ },
  { template: "project.warning.${code}", keys: /^project\.warning\.[a-z_.]+$/ },
  { template: "measure.point.${n}", keys: /^measure\.point\.\d+$/ },
  { template: "measure.role.${base}", keys: /^measure\.role\.[A-Za-z]+$/ },
  { template: "tutorial.q.${id}.${field}", keys: /^tutorial\.q\.[a-z_]+\.[a-z_]+\.[a-z]+$/ },
  { template: "calibrate.range.dims.${name}", keys: /^calibrate\.range\.dims\.[a-z]$/ },
  { template: "calibrate.ba.layout.${value}", keys: /^calibrate\.ba\.layout\.[a-z]+$/ },
  { template: "calibrate.ba.${band}", keys: /^calibrate\.ba\.(?:hp|lp)$/ },
  { template: "calibrate.analytic.${scope}", keys: /^calibrate\.analytic\.(?:off|peq|mimo|both)$/ },
  { template: "frame.settings.logo.${icon}", keys: /^frame\.settings\.logo\.(?:expert|civilian)$/ },
];

const named = (key: string) => code.includes(`"${key}"`) || code.includes(`'${key}'`) || code.includes(`\`${key}\``);

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

  it("uses every key: each one is named in src/ or built from a listed template", () => {
    const unused = Object.keys(catalogs.ko).filter((key) => !named(key) && !BUILT.some((built) => built.keys.test(key)));
    expect(unused).toEqual([]);
  });

  it("lists only templates the code still builds, each producing a catalog key", () => {
    for (const { template, keys } of BUILT) {
      expect(code.includes(`\`${template}`), template).toBe(true);
      expect(Object.keys(catalogs.ko).some((key) => keys.test(key)), template).toBe(true);
    }
  });

  it("keeps the Korean register: 합니다체 sentences, no 해요체 statements", () => {
    // -세요 instructions are allowed; statements ending in -어요/-아요/-해요/-예요 are not.
    const friendly = /(?:어요|아요|해요|여요|예요|에요|래요|네요|군요|지요|죠)[.!?]?$/;
    const offenders = Object.entries(catalogs.ko).filter(([, text]) => text.split(/(?<=[.!?])\s+/).some((sentence) => friendly.test(sentence.trim())));
    expect(offenders).toEqual([]);
  });

  it("attaches Korean particles to the word before them", () => {
    // "REW 가", "MIMO 는": a space between a Latin word and its particle.
    const spaced = /[A-Za-z0-9\]] (?:이|가|은|는|을|를|의|로|으로|와|과|에|에서|도)(?=[\s.,)]|$)/;
    const offenders = Object.entries(catalogs.ko).filter(([, text]) => spaced.test(text));
    expect(offenders).toEqual([]);
  });

  it("uses ASCII hyphens next to Latin letters", () => {
    for (const [locale, catalog] of Object.entries(catalogs)) {
      const offenders = Object.entries(catalog).filter(([, text]) => /[A-Za-z][‐-―]|[‐-―][A-Za-z]/.test(text));
      expect(offenders, locale).toEqual([]);
    }
  });

  it("has no exclamation marks", () => {
    for (const [locale, catalog] of Object.entries(catalogs)) {
      const offenders = Object.entries(catalog).filter(([, text]) => /!/.test(text));
      expect(offenders, locale).toEqual([]);
    }
  });

  it("writes English without 'please'", () => {
    const offenders = Object.entries(catalogs.en).filter(([, text]) => /\bplease\b/i.test(text));
    expect(offenders).toEqual([]);
  });

  it("fills placeholders and picks a supported locale", () => {
    expect(translate("ko", "help.title", { step: "시스템" })).toBe("도움말 · 시스템");
    expect(translate("en", "measure.where.title", { n: 4, count: 9 })).toBe("Position 4 / 9");
    expect(pickLocale(["en-US", "ko-KR"])).toBe("en");
    expect(pickLocale(["fr-FR"])).toBe("ko");
  });
});
