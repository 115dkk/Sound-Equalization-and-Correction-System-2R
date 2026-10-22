// The screen tour (docs/packets/P43): every scene of src/gallery/tour/scenes.ts
// is the real App on the mock service, captured full-window at the app's
// default size in four variants, plus manifest.json for ui-gallery.yml's
// review issue. Fails (exit 1, `::error::` lines) on a page or console
// error, a scene that does not get ready or whose op fails, no scenes, a
// repeated id, text under the 13 px caption floor, or a missing capture.
//
//   node scripts/screen-tour.mjs [--dist dist] [--out screen-shots] [--only <id prefix,…>]
//
// Run it on the output of `npm run build:gallery`. CHROMIUM_PATH points
// Playwright at an installed Chromium instead of the one
// `npx playwright install chromium` downloads.
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { argv, env, exit } from "node:process";
import { chromium } from "playwright";
import { serveStatic } from "./static-server.mjs";

const arg = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback;
};
const dist = resolve(arg("--dist", "dist"));
const out = resolve(arg("--out", "screen-shots"));
const only = arg("--only", "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);
if (!existsSync(join(dist, "gallery.html"))) {
  console.error(`${dist}/gallery.html is missing; run 'npm run build:gallery' first`);
  exit(2);
}
mkdirSync(out, { recursive: true });

// tauri.conf.json's default window.
const VIEWPORT = { width: 1280, height: 800 };
// The two skins (docs/skins) ride along in Korean and dark, where their
// chrome differs most from the default.
const VARIANTS = [
  { theme: "light", lang: "ko", skin: "default" },
  { theme: "dark", lang: "ko", skin: "default" },
  { theme: "light", lang: "en", skin: "default" },
  { theme: "dark", lang: "en", skin: "default" },
  { theme: "dark", lang: "ko", skin: "rack" },
  { theme: "dark", lang: "ko", skin: "matrix" },
];
const variantName = ({ theme, lang, skin }) => `${theme}.${lang}${skin === "default" ? "" : `.${skin}`}`;
const BROWSER_LOCALE = { ko: "ko-KR", en: "en-US" };
const FLOOR_PX = 13;
const READY_MS = 10_000;
const OP_MS = 5_000;
const PHASE_MS = 15_000;
const TRANSFER_MS = 35_000;

/** Bound browser promises that do not have their own Playwright timeout. */
async function phase(label, name, action, timeout = PHASE_MS) {
  let timer;
  try {
    return await Promise.race([
      action(),
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`timed out after ${timeout / 1000} s`)), timeout);
      }),
    ]);
  } catch (error) {
    throw new Error(`${label}: ${name}: ${String(error?.message ?? error).split("\n")[0]}`);
  } finally {
    clearTimeout(timer);
  }
}

const server = await serveStatic(dist, "gallery.html");
const browser = await chromium.launch(env.CHROMIUM_PATH ? { executablePath: env.CHROMIUM_PATH } : {});
const problems = [];
let scenes = [];
let shots = 0;

/** Two animation frames: whatever the last op changed has been laid out and painted. */
const twoFrames = (page, label) => phase(label, "animation frames", () => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done())))));

try {
  // The scene list comes from the page, never from this script.
  {
    const context = await phase("scene list", "new context", () => browser.newContext({ viewport: VIEWPORT, locale: BROWSER_LOCALE.ko }));
    await phase("scene list", "route build", () => server.route(context));
    const page = await phase("scene list", "new page", () => context.newPage());
    let closing = false;
    page.on("requestfailed", (request) => {
      if (!closing) problems.push(`scene list: request failed: ${request.url()}: ${request.failure()?.errorText}`);
    });
    page.on("pageerror", (error) => problems.push(`scene list: page error: ${error.message}`));
    await phase("scene list", "navigate", () => page.goto(`${server.origin}/gallery.html?tour=`, { waitUntil: "load" }), TRANSFER_MS);
    await page.waitForFunction(() => Array.isArray(window.__tourScenes), null, { timeout: READY_MS });
    scenes = await phase("scene list", "read scenes", () => page.evaluate(() => window.__tourScenes));
    closing = true;
    await phase("scene list", "close context", () => context.close());
  }
  if (scenes.length === 0) problems.push("the tour has no scene");
  const ids = scenes.map((scene) => scene.id);
  const repeated = ids.filter((id, i) => ids.indexOf(id) !== i);
  if (repeated.length) problems.push(`repeated scene id ${[...new Set(repeated)].join(", ")}`);
  const chosen = scenes.filter((scene) => only.length === 0 || only.some((prefix) => scene.id.startsWith(prefix)));

  for (const { theme, lang, skin } of VARIANTS) {
    const variant = variantName({ theme, lang, skin });
    const context = await phase(variant, "new context", () => browser.newContext({
      viewport: VIEWPORT,
      deviceScaleFactor: 1,
      locale: BROWSER_LOCALE[lang],
      // The tour page stops Date.now() at 09:41 Seoul time (src/gallery/tour/clock.ts).
      timezoneId: "Asia/Seoul",
      colorScheme: theme,
      reducedMotion: "reduce",
    }));
    await phase(variant, "route build", () => server.route(context));
    // A full navigation resets the app document for every scene without
    // repeatedly creating browser targets during the long tour.
    const page = await phase(variant, "new page", () => context.newPage());
    let currentLabel = variant;
    let closing = false;
    const requestLabels = new WeakMap();
    page.on("request", (request) => requestLabels.set(request, currentLabel));
    page.on("requestfailed", (request) => {
      if (!closing) problems.push(`${requestLabels.get(request) ?? currentLabel}: request failed: ${request.url()}: ${request.failure()?.errorText}`);
    });
    page.on("pageerror", (error) => problems.push(`${currentLabel}: page error: ${error.message}`));
    page.on("console", (message) => {
      if (message.type() === "error") problems.push(`${currentLabel}: console error: ${message.text()}`);
    });
    for (const scene of chosen) {
      const label = `${scene.id}.${variant}`;
      currentLabel = label;
      console.log(`tour: ${label}`);
      try {
        await phase(label, "reset pointer", () => page.mouse.move(0, 0));
        await phase(label, "navigate", () => page.goto(`${server.origin}/gallery.html?tour=${encodeURIComponent(scene.id)}&theme=${theme}&lang=${lang}&skin=${skin}`, { waitUntil: "load" }), TRANSFER_MS);
        const ready = await phase(label, "scene ready", () => page
          .waitForFunction(() => document.documentElement.dataset.tourReady, null, { timeout: READY_MS })
          .then((handle) => handle.jsonValue())
          .catch(() => null));
        if (ready !== "1") throw new Error(ready === null ? `not ready within ${READY_MS / 1000} s` : `tour page says "${ready}"`);
        // The bundled faces (src/fonts) must be in: a shot set in a fallback
        // font is not the product's screen.
        const fonts = await phase(label, "fonts", () => page.evaluate(() => document.fonts.ready.then(() => document.fonts.check('14px "Pretendard"'))));
        if (fonts !== true) throw new Error("the bundled Pretendard did not load");

        const { opCount } = await phase(label, "read scene", () => page.evaluate(() => window.__tour.scene));
        for (let i = 0; i < opCount; i++) {
          const op = await phase(label, `read op ${i}`, () => page.evaluate((n) => window.__tour.op(n), i));
          try {
            if (op.kind === "press") await phase(label, `press op ${i}`, () => page.keyboard.press(op.key));
            else if (op.kind === "click") {
              await page.locator(op.selector).click({ timeout: OP_MS });
              // Park the pointer on the rail's brand so no tooltip opens under it.
              await phase(label, `park pointer ${i}`, () => page.mouse.move(0, 0));
            } else if (op.kind === "hover") await page.locator(op.selector).hover({ timeout: OP_MS });
            else await phase(label, `page op ${i}`, () => page.evaluate((n) => window.__tour.run(n), i));
          } catch (error) {
            throw new Error(`op ${i} (${op.kind}${op.key ? ` ${op.key}` : ""}${op.selector ? ` ${op.selector}` : ""}) failed: ${String(error?.message ?? error).split("\n")[0]}`);
          }
          await twoFrames(page, label);
        }
        await phase(label, "settle", () => page.evaluate(() => window.__tour.settle()));
        await twoFrames(page, label);

        // The same caption-floor check as gallery-shots.mjs, over the whole window.
        // Canvas text (uPlot ticks) is styled from the same tokens and is not visible to it.
        const small = await phase(label, "text size check", () => page.$$eval(
          "body *",
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
              .map((node) => `"${node.textContent.trim().slice(0, 40)}" at ${getComputedStyle(node).fontSize}`),
          FLOOR_PX,
        ));
        for (const line of small) problems.push(`${label}: text under ${FLOOR_PX} px: ${line}`);

        await phase(label, "screenshot", () => page.screenshot({ path: join(out, `${label}.png`), animations: "disabled" }), TRANSFER_MS);
        shots += 1;
      } catch (error) {
        problems.push(`${label}: ${String(error?.message ?? error).split("\n")[0]}`);
      }
    }
    closing = true;
    await phase(variant, "close page", () => page.close());
    await phase(variant, "close context", () => context.close());
  }

  const manifest = {
    viewport: VIEWPORT,
    variants: VARIANTS.map(variantName),
    scenes: chosen.map(({ id, step, title, note }) => ({ id, step, title, ...(note ? { note } : {}) })),
  };
  writeFileSync(join(out, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  const expected = chosen.length * VARIANTS.length;
  if (shots !== expected) problems.push(`${shots} captures for ${chosen.length} scenes × ${VARIANTS.length} variants (${expected} expected)`);
  console.log(`${shots} screenshots of ${chosen.length} scenes in ${out}`);
} finally {
  try {
    await phase("tour", "close browser", () => browser.close());
  } finally {
    await phase("tour", "close server", () => server.close());
  }
}

if (problems.length) {
  for (const problem of problems) console.error(`::error::${problem}`);
  exit(1);
}
