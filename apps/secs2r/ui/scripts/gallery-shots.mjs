// Screenshots every gallery specimen (docs/DESIGN.md, the gallery) in each
// theme, language, accent and skin, one PNG per data-gallery-id, and fails when
// the gallery throws, logs an error, draws no specimen, or draws text under
// the 13 px caption floor (CLAUDE.md, screen rules). ci.yml's gallery job
// runs it on the output of `npm run build:gallery` and keeps the PNGs as a
// run artifact.
//
//   node scripts/gallery-shots.mjs [--dist dist] [--out gallery-shots]
//
// CHROMIUM_PATH points Playwright at an installed Chromium instead of the
// one `npx playwright install chromium` downloads.
import { existsSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { argv, env, exit } from "node:process";
import { chromium } from "playwright";
import { serveStatic } from "./static-server.mjs";

const arg = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback;
};
const dist = resolve(arg("--dist", "dist"));
const out = resolve(arg("--out", "gallery-shots"));
if (!existsSync(join(dist, "gallery.html"))) {
  console.error(`${dist}/gallery.html is missing; run 'npm run build:gallery' first`);
  exit(2);
}
mkdirSync(out, { recursive: true });

// The skins (docs/skins) are shot in Korean only: their difference from the
// default is in the chrome, not in the words.
const VARIANTS = [
  { theme: "light", lang: "ko", accent: "default", skin: "default" },
  { theme: "dark", lang: "ko", accent: "default", skin: "default" },
  { theme: "light", lang: "en", accent: "default", skin: "default" },
  { theme: "dark", lang: "en", accent: "default", skin: "default" },
  { theme: "light", lang: "ko", accent: "tutorial", skin: "default" },
  { theme: "light", lang: "ko", accent: "default", skin: "rack" },
  { theme: "dark", lang: "ko", accent: "default", skin: "rack" },
  { theme: "light", lang: "ko", accent: "default", skin: "matrix" },
  { theme: "dark", lang: "ko", accent: "default", skin: "matrix" },
];
const BROWSER_LOCALE = { ko: "ko-KR", en: "en-US" };
const FLOOR_PX = 13;

// The build is static and uses root-absolute asset paths, so a file server
// rooted at dist is all the gallery needs.
const server = await serveStatic(dist, "gallery.html");
const origin = server.origin;

const browser = await chromium.launch(env.CHROMIUM_PATH ? { executablePath: env.CHROMIUM_PATH } : {});
const problems = [];
let shots = 0;
try {
  for (const { theme, lang, accent, skin } of VARIANTS) {
    const label = `${theme}.${lang}${accent === "default" ? "" : `.${accent}`}${skin === "default" ? "" : `.${skin}`}`;
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 1,
      locale: BROWSER_LOCALE[lang],
      colorScheme: theme,
      reducedMotion: "reduce",
    });
    const page = await context.newPage();
    page.on("pageerror", (error) => problems.push(`${label}: page error: ${error.message}`));
    page.on("console", (message) => {
      if (message.type() === "error") problems.push(`${label}: console error: ${message.text()} (${message.location().url})`);
    });
    await page.goto(`${origin}/gallery.html?theme=${theme}&lang=${lang}&accent=${accent}&skin=${skin}`, { waitUntil: "networkidle" });
    await page.evaluate(() => document.fonts.ready);

    const ids = await page.$$eval("[data-gallery-id]", (nodes) => nodes.map((n) => n.getAttribute("data-gallery-id")));
    if (ids.length === 0) problems.push(`${label}: the gallery drew no specimen`);
    const duplicates = ids.filter((id, i) => ids.indexOf(id) !== i);
    if (duplicates.length) problems.push(`${label}: duplicate data-gallery-id ${[...new Set(duplicates)].join(", ")}`);

    // Text drawn by the app (not the gallery's own headings) must not go
    // under the caption floor. Canvas text (uPlot ticks) is styled from the
    // same tokens and is not visible to this check.
    const small = await page.$$eval(
      ".gal-specimen__stage *",
      (nodes, floor) =>
        nodes
          .filter((node) => {
            const own = [...node.childNodes].some((c) => c.nodeType === Node.TEXT_NODE && c.textContent.trim());
            if (!own) return false;
            const style = getComputedStyle(node);
            const box = node.getBoundingClientRect();
            // Visually hidden text (.visually-hidden) is a 1 px box.
            const hidden = style.visibility === "hidden" || style.display === "none" || box.width <= 1 || box.height <= 1;
            return !hidden && parseFloat(style.fontSize) < floor;
          })
          .map((node) => {
            const owner = node.closest("[data-gallery-id]")?.getAttribute("data-gallery-id");
            return `${owner}: "${node.textContent.trim().slice(0, 40)}" at ${getComputedStyle(node).fontSize}`;
          }),
      FLOOR_PX,
    );
    for (const line of small) problems.push(`${label}: text under ${FLOOR_PX} px in ${line}`);

    for (const id of ids) {
      const specimen = page.locator(`[data-gallery-id="${id}"]`);
      await specimen.scrollIntoViewIfNeeded();
      await specimen.screenshot({ path: join(out, `${id}.${label}.png`), animations: "disabled" });
      shots += 1;
    }
    await context.close();
  }
} finally {
  await browser.close();
  server.close();
}

console.log(`${shots} screenshots in ${out}`);
if (problems.length) {
  for (const problem of problems) console.error(`::error::${problem}`);
  exit(1);
}
