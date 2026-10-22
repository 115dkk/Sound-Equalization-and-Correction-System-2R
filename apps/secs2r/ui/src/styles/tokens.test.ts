// Token gate (DESIGN.md §2–3): no UI text style under 14 px (captions 13),
// every font size goes through a type token, no `transition: all`, and the
// two copies of the dark theme stay the same.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const src = fileURLToPath(new URL("..", import.meta.url));
const tokens = readFileSync(join(src, "styles/tokens.css"), "utf8");

function files(dir: string, ext: RegExp): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === "bindings" ? [] : files(path, ext);
    return ext.test(name) ? [path] : [];
  });
}

const cssFiles = files(src, /\.css$/);
const tsxFiles = files(src, /\.tsx?$/).filter((path) => !/\.test\.tsx?$/.test(path));

/** Declarations of the first rule block that starts at `start` (balanced braces). */
function block(text: string, start: number): string {
  const open = text.indexOf("{", start);
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === "{") depth++;
    if (text[i] === "}" && --depth === 0) return text.slice(open + 1, i);
  }
  throw new Error("unbalanced block");
}

const stripComments = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, "");

const declarations = (body: string) =>
  stripComments(body)
    .split(";")
    .map((line) => line.trim().replace(/\s+/g, " "))
    .filter((line) => line.startsWith("--") || line.startsWith("color-scheme"));

describe("type tokens", () => {
  const sizes = [...tokens.matchAll(/--text-([a-z-]+):\s*(\d+(?:\.\d+)?)px/g)].map((m) => ({ name: m[1]!, px: Number(m[2]) }));

  it("defines the type scale", () => {
    expect(sizes.map((s) => s.name)).toEqual(expect.arrayContaining(["caption", "ui", "body", "title-s", "title"]));
  });

  it("keeps UI text at 14 px or more and captions at 13 px or more", () => {
    for (const { name, px } of sizes) {
      expect(px, `--text-${name}`).toBeGreaterThanOrEqual(name === "caption" ? 13 : 14);
    }
  });

  it("sets every font size through a type token", () => {
    const offenders: string[] = [];
    for (const file of cssFiles) {
      for (const match of stripComments(readFileSync(file, "utf8")).matchAll(/font-size:\s*([^;]+);/g)) {
        const value = match[1]!.trim();
        if (!/^var\(--text-[a-z-]+\)$/.test(value) && value !== "inherit") offenders.push(`${file}: ${value}`);
      }
    }
    for (const file of tsxFiles) {
      const text = readFileSync(file, "utf8");
      if (/fontSize\s*[:=]|font-size=/.test(text)) offenders.push(`${file}: inline font size`);
    }
    expect(offenders).toEqual([]);
  });
});

describe("motion and theme tokens", () => {
  it("never transitions `all`", () => {
    const offenders = cssFiles.filter((file) => /transition(-property)?:\s*all\b/.test(stripComments(readFileSync(file, "utf8"))));
    expect(offenders).toEqual([]);
  });

  it("keeps the system and explicit dark themes identical", () => {
    const media = block(tokens, tokens.indexOf(':root:not([data-theme="light"])'));
    const explicit = block(tokens, tokens.indexOf('[data-theme="dark"] {'));
    expect(declarations(media)).toEqual(declarations(explicit));
  });

  it("gives light and dark the same colour tokens", () => {
    const light = block(tokens, tokens.indexOf('[data-theme="light"]'));
    const dark = block(tokens, tokens.indexOf('[data-theme="dark"] {'));
    const names = (body: string) => declarations(body).map((line) => line.split(":")[0]).sort();
    expect(names(dark)).toEqual(names(light));
  });

  it("zeroes durations under reduced motion", () => {
    const reduced = block(tokens, tokens.indexOf("@media (prefers-reduced-motion: reduce)"));
    expect(reduced).toMatch(/--dur-fast:\s*0ms/);
    expect(reduced).toMatch(/--dur-base:\s*0ms/);
    expect(reduced).toMatch(/--dur-slow:\s*0ms/);
  });

  it("names colours only in tokens.css and the skins' token blocks (components use roles)", () => {
    const raw = /#[0-9a-f]{3,8}\b|\b(?:rgb|rgba|hsl|hsla|oklch|lab)\(/i;
    const isSkin = (file: string) => /[\\/]styles[\\/]skins[\\/][^\\/]+\.css$/.test(file);
    const offenders = cssFiles
      .filter((file) => !file.endsWith("tokens.css") && !file.endsWith("gallery.css"))
      .filter((file) => raw.test(isSkin(file) ? skinStructure(readFileSync(file, "utf8")) : stripComments(readFileSync(file, "utf8"))));
    expect(offenders).toEqual([]);
  });

  it("finds the skins' token blocks", () => {
    const skins = cssFiles.filter((file) => /[\\/]styles[\\/]skins[\\/][^\\/]+\.css$/.test(file));
    for (const file of skins) {
      const text = stripComments(readFileSync(file, "utf8"));
      expect(skinStructure(text).length, file).toBeLessThan(text.length);
    }
  });
});

/**
 * A skin stylesheet (styles/skins/<id>.css, ADR 0010) without its token
 * blocks: the light, dark and accent blocks on the skin root, and the media
 * query that repeats the dark one. Colour values may live only in those.
 */
function skinStructure(css: string): string {
  const text = stripComments(css);
  const tokenBlock = /^(?:\[data-skin="[a-z]+"\](?:\[data-theme="dark"\]|\[data-accent\])?|@media \(prefers-color-scheme: dark\))$/;
  let out = "";
  let i = 0;
  while (i < text.length) {
    const open = text.indexOf("{", i);
    if (open < 0) {
      out += text.slice(i);
      break;
    }
    let depth = 0;
    let end = open;
    for (; end < text.length; end++) {
      if (text[end] === "{") depth++;
      if (text[end] === "}" && --depth === 0) break;
    }
    const selector = text.slice(i, open).trim();
    if (!tokenBlock.test(selector)) out += text.slice(i, end + 1);
    i = end + 1;
  }
  return out;
}
