// Resolves the colour tokens of a theme × skin × accent combination from the
// token stylesheets and measures WCAG contrast, so docs/DESIGN.md reports
// numbers that were computed, not estimated (make-interfaces-feel-better,
// color.md: "Measure contrast, don't estimate it").
//
// The resolver is a small model of the cascade the stylesheets rely on: the
// theme-independent `:root` block, the light block, the explicit dark block,
// the tutorial accent block, a skin's light and dark blocks, then the accent
// composition. `var()` references resolve lazily, so a skin that only moves
// `--accent-hue` still gets a recomposed `--accent`, and a skin that sets
// `--accent` outright wins because its file is imported after tokens.css.
// Colours are OKLCH (optionally with alpha) and are gamut-mapped to sRGB the
// way CSS Color 4 does for display: chroma is reduced until the colour fits.

type Rgb = [number, number, number];
export type Theme = "light" | "dark";

/** One `selector { declarations }` block, custom properties only. */
type Block = { selector: string; declarations: Map<string, string> };

const stripComments = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, "");

/** Every top-level and nested block of `css` with its selector text (media wrappers are flattened). */
export function parseBlocks(css: string): Block[] {
  const text = stripComments(css);
  const blocks: Block[] = [];
  const walk = (from: number, to: number) => {
    let i = from;
    while (i < to) {
      const open = text.indexOf("{", i);
      if (open < 0 || open >= to) return;
      const selector = text.slice(i, open).trim();
      let depth = 1;
      let j = open + 1;
      while (j < to && depth > 0) {
        if (text[j] === "{") depth++;
        else if (text[j] === "}") depth--;
        j++;
      }
      const body = text.slice(open + 1, j - 1);
      if (selector.startsWith("@")) {
        walk(open + 1, j - 1);
      } else {
        const declarations = new Map<string, string>();
        for (const line of body.split(";")) {
          const colon = line.indexOf(":");
          if (colon < 0) continue;
          const name = line.slice(0, colon).trim();
          if (name.startsWith("--")) declarations.set(name, line.slice(colon + 1).trim().replace(/\s+/g, " "));
        }
        blocks.push({ selector: selector.replace(/\s+/g, " "), declarations });
      }
      i = j;
    }
  };
  walk(0, text.length);
  return blocks;
}

const selectorIs = (block: Block, selector: string) => block.selector.split(",").map((s) => s.trim()).includes(selector);

/**
 * The tokens of one combination as raw declarations in cascade order. `css`
 * is tokens.css followed by the skin stylesheets.
 */
export function cascade(css: string[], options: { theme: Theme; skin?: string | undefined; accent?: "default" | "tutorial" | undefined }): Map<string, string> {
  const blocks = css.flatMap(parseBlocks);
  const tokens = new Map<string, string>();
  const apply = (predicate: (block: Block) => boolean) => {
    for (const block of blocks) if (predicate(block)) for (const [name, value] of block.declarations) tokens.set(name, value);
  };
  apply((b) => b.selector === ":root");
  apply((b) => selectorIs(b, '[data-theme="light"]'));
  if (options.theme === "dark") apply((b) => b.selector === '[data-theme="dark"]');
  if (options.accent === "tutorial") apply((b) => b.selector === '[data-accent="tutorial"]');
  // The accent composition (`:root, [data-theme], [data-accent], [data-skin]`).
  apply((b) => selectorIs(b, "[data-theme]"));
  if (options.skin) {
    const skin = `[data-skin="${options.skin}"]`;
    apply((b) => selectorIs(b, skin) || selectorIs(b, `${skin}[data-accent]`));
    if (options.theme === "dark") apply((b) => selectorIs(b, `${skin}[data-theme="dark"]`));
  }
  return tokens;
}

/** `value` with every `var(--x)` and simple `calc(a * b)` substituted. */
function resolve(tokens: Map<string, string>, value: string, depth = 0): string {
  if (depth > 32) throw new Error(`token cycle in ${value}`);
  let out = value;
  for (;;) {
    const match = /var\((--[a-z0-9-]+)(?:,\s*([^)]*))?\)/i.exec(out);
    if (!match) break;
    const name = match[1]!;
    const replacement = tokens.get(name) ?? match[2];
    if (replacement === undefined) throw new Error(`unknown token ${name}`);
    out = out.slice(0, match.index) + resolve(tokens, replacement, depth + 1) + out.slice(match.index + match[0].length);
  }
  for (;;) {
    const calc = /calc\(([^()]*)\)/.exec(out);
    if (!calc) break;
    const terms = calc[1]!.split("*").map((t) => Number(t.trim()));
    if (terms.some((t) => Number.isNaN(t))) throw new Error(`unsupported calc ${calc[0]}`);
    out = out.slice(0, calc.index) + String(terms.reduce((a, b) => a * b, 1)) + out.slice(calc.index + calc[0].length);
  }
  return out;
}

type Oklch = { l: number; c: number; h: number; alpha: number };

/** `oklch(L C H)` or `oklch(L C H / A)`; L in 0–1 (or percent). */
function parseOklch(text: string): Oklch {
  const m = /^oklch\(\s*([\d.]+)(%?)\s+([\d.]+)\s+([\d.]+)\s*(?:\/\s*([\d.]+)(%?))?\s*\)$/i.exec(text.trim());
  if (!m) throw new Error(`not an oklch colour: ${text}`);
  const l = Number(m[1]) / (m[2] ? 100 : 1);
  const alpha = m[5] === undefined ? 1 : Number(m[5]) / (m[6] ? 100 : 1);
  return { l, c: Number(m[3]), h: Number(m[4]), alpha };
}

function oklabToLinearSrgb(L: number, a: number, b: number): Rgb {
  const l_ = L + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = L - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = L - 0.0894841775 * a - 1.291485548 * b;
  const l = l_ ** 3;
  const m = m_ ** 3;
  const s = s_ ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

const inGamut = (rgb: Rgb, eps = 1e-6) => rgb.every((v) => v >= -eps && v <= 1 + eps);

/** Linear sRGB of an OKLCH colour, chroma reduced until it fits the gamut. */
function oklchToLinearSrgb({ l, c, h }: Oklch): Rgb {
  if (l <= 0) return [0, 0, 0];
  if (l >= 1) return [1, 1, 1];
  const rad = (h * Math.PI) / 180;
  const convert = (chroma: number) => oklabToLinearSrgb(l, chroma * Math.cos(rad), chroma * Math.sin(rad));
  let rgb = convert(c);
  if (inGamut(rgb)) return rgb.map((v) => Math.min(1, Math.max(0, v))) as Rgb;
  let lo = 0;
  let hi = c;
  for (let i = 0; i < 40; i++) {
    const mid = (lo + hi) / 2;
    rgb = convert(mid);
    if (inGamut(rgb)) lo = mid;
    else hi = mid;
  }
  return convert(lo).map((v) => Math.min(1, Math.max(0, v))) as Rgb;
}

/** Alpha-composites `over` on `under` (both linear). */
const composite = (over: Rgb, alpha: number, under: Rgb): Rgb => over.map((v, i) => v * alpha + under[i]! * (1 - alpha)) as Rgb;

/** WCAG relative luminance of linear sRGB. */
const luminance = ([r, g, b]: Rgb) => 0.2126 * r + 0.7152 * g + 0.0722 * b;

/** WCAG 2 contrast ratio, 1–21. */
function contrast(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

/** Hex of linear sRGB (for tables and docs). */
export function hex(rgb: Rgb): string {
  const channel = (v: number) => {
    const s = v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
    return Math.round(Math.min(1, Math.max(0, s)) * 255)
      .toString(16)
      .padStart(2, "0");
  };
  return `#${rgb.map(channel).join("")}`;
}

/** The resolved colour of token `name`, composited on `on` when it has alpha. */
export function colour(tokens: Map<string, string>, name: string, on?: Rgb): Rgb {
  const parsed = parseOklch(resolve(tokens, `var(${name})`));
  const rgb = oklchToLinearSrgb(parsed);
  if (parsed.alpha >= 1) return rgb;
  if (!on) throw new Error(`${name} has alpha; pass the surface it sits on`);
  return composite(rgb, parsed.alpha, on);
}

/** Contrast of token `fg` on token `bg` (each composited on `bg`'s own ground when translucent). */
export function pair(tokens: Map<string, string>, fg: string, bg: string): number {
  const ground = colour(tokens, "--bg");
  const under = colour(tokens, bg, ground);
  return contrast(colour(tokens, fg, under), under);
}

/** OKLCH hue of token `name`, or null when it is achromatic (chroma 0). */
export function hue(tokens: Map<string, string>, name: string): number | null {
  const parsed = parseOklch(resolve(tokens, `var(${name})`));
  return parsed.c === 0 ? null : parsed.h;
}
