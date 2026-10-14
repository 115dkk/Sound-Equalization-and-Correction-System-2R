// Colour plumbing between CSS tokens and the canvas. Tokens are OKLCH; the
// canvas gets rgba() strings so alpha rules (faint positions, dimmed
// non-highlighted curves) work the same in every WebView.
export type Rgba = readonly [r: number, g: number, b: number, a: number];

const clamp01 = (x: number) => Math.min(1, Math.max(0, x));
const encode = (x: number) => {
  const v = clamp01(x);
  return v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
};

/** OKLCH (L 0–1, C, H degrees) to gamma-encoded sRGB 0–1, clipped to the gamut. */
export function oklchToSrgb(L: number, C: number, H: number): [number, number, number] {
  const h = (H * Math.PI) / 180;
  const a = C * Math.cos(h);
  const b = C * Math.sin(h);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    encode(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    encode(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    encode(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

const num = (token: string | undefined, percentScale = 1): number => {
  if (token === undefined) return Number.NaN;
  const t = token.trim();
  if (t.endsWith("%")) return (Number(t.slice(0, -1)) / 100) * percentScale;
  if (t === "none") return 0;
  return Number(t);
};

/** Parses #rgb, #rrggbb, rgb()/rgba() (comma or space syntax) and oklch(). Null if unknown. */
export function parseColor(input: string): Rgba | null {
  const text = input.trim().toLowerCase();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/.exec(text);
  if (hex?.[1]) {
    const h = hex[1].length === 3 ? [...hex[1]].map((c) => c + c).join("") : hex[1];
    const v = (i: number) => parseInt(h.slice(i, i + 2), 16) / 255;
    return [v(0), v(2), v(4), h.length === 8 ? v(6) : 1];
  }
  const fn = /^(rgba?|oklch)\(([^)]*)\)$/.exec(text);
  if (!fn?.[1] || fn[2] === undefined) return null;
  const [main, alphaPart] = fn[2].split("/");
  const parts = (main ?? "").replace(/,/g, " ").trim().split(/\s+/);
  let alpha = alphaPart !== undefined ? num(alphaPart, 1) : parts.length === 4 ? num(parts[3], 1) : 1;
  if (!Number.isFinite(alpha)) alpha = 1;
  if (fn[1] === "oklch") {
    const L = num(parts[0], 1);
    const C = num(parts[1], 0.4);
    const H = num(parts[2]);
    if (![L, C, H].every(Number.isFinite)) return null;
    const [r, g, b] = oklchToSrgb(L, C, H);
    return [r, g, b, clamp01(alpha)];
  }
  const [r, g, b] = [num(parts[0], 255), num(parts[1], 255), num(parts[2], 255)];
  if (![r, g, b].every(Number.isFinite)) return null;
  return [r / 255, g / 255, b / 255, clamp01(alpha)];
}

export function rgbaString([r, g, b, a]: Rgba, alpha = 1): string {
  const c = (x: number) => Math.round(clamp01(x) * 255);
  return `rgba(${c(r)}, ${c(g)}, ${c(b)}, ${Number((a * alpha).toFixed(3))})`;
}

let probe: CanvasRenderingContext2D | null | undefined;
const SENTINEL = "#010203";

/**
 * Any colour the engine understands (lab(), color(), calc() inside oklch()
 * …) resolved to sRGB by painting one pixel. Null without a canvas or when
 * the engine rejects the value.
 */
export function resolveWithCanvas(css: string): Rgba | null {
  if (probe === undefined) {
    const canvas = typeof document !== "undefined" ? document.createElement("canvas") : null;
    if (canvas) {
      canvas.width = 1;
      canvas.height = 1;
    }
    try {
      probe = canvas?.getContext("2d", { willReadFrequently: true }) ?? null;
    } catch {
      probe = null;
    }
  }
  if (!probe) return null;
  probe.fillStyle = SENTINEL;
  probe.fillStyle = css;
  if (String(probe.fillStyle).toLowerCase() === SENTINEL && css.trim().toLowerCase() !== SENTINEL) return null;
  probe.clearRect(0, 0, 1, 1);
  probe.fillRect(0, 0, 1, 1);
  const [r = 0, g = 0, b = 0, a = 255] = probe.getImageData(0, 0, 1, 1).data;
  return [r / 255, g / 255, b / 255, a / 255];
}

/**
 * Reads a colour token as the element sees it (its theme scope included).
 * Plain hex, rgb() and oklch() are parsed exactly; anything else (the build
 * may rewrite oklch() as lab()) goes through the canvas.
 */
export function readColor(element: Element, token: string, fallback: Rgba = [0.5, 0.5, 0.5, 1]): Rgba {
  const raw = getComputedStyle(element).getPropertyValue(token).trim();
  if (!raw) return fallback;
  return parseColor(raw) ?? resolveWithCanvas(raw) ?? fallback;
}

export function readNumber(element: Element, token: string, fallback: number): number {
  const value = Number.parseFloat(getComputedStyle(element).getPropertyValue(token));
  return Number.isFinite(value) ? value : fallback;
}
