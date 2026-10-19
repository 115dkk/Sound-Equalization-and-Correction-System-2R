// Focused regression captures for the GPU preference. Run after build:gallery.
// Uses the same static server and browser selection as gallery-shots.mjs.
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { chromium } from "playwright";
import { serveStatic } from "./static-server.mjs";
const onlyIndex = process.argv.indexOf("--only");
const only = onlyIndex >= 0 ? process.argv[onlyIndex + 1] : null;
const filter = only ?? "screen.calibrate.gpu,screen.calibrate.mimo-settings";
const out = resolve("screen-shots/gpu");
mkdirSync(out, { recursive: true });
const server = await serveStatic(resolve("dist"));
const browser = await chromium.launch(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {});
const problems = [];
const evidence = [];
try {
  for (const theme of ["light", "dark"]) for (const lang of ["ko", "en"]) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 980 }, colorScheme: theme, reducedMotion: "reduce" });
    const page = await context.newPage();
    page.on("pageerror", error => problems.push(String(error)));
    await page.goto(`${server.origin}/gallery.html?only=${encodeURIComponent(filter)}&theme=${theme}&lang=${lang}`, { waitUntil: "networkidle" });
    await page.evaluate(() => document.fonts.ready);
    const specimens = page.locator("[data-gallery-id]");
    if (await specimens.count() !== (only ? 1 : 12)) throw new Error("GPU specimens missing");
    for (const specimen of await specimens.all()) {
      const id = await specimen.getAttribute("data-gallery-id");
      await specimen.scrollIntoViewIfNeeded();
      const control = specimen.locator('[data-ui="calibrate.compute.preference"]');
      if (!await control.isDisabled()) await control.focus();
      if (id === "screen.calibrate.gpu-unavailable" || id === "screen.calibrate.gpu-request-unavailable") await specimen.locator(".calibrate-compute__help summary").click();
      const check = await specimen.evaluate(root => {
        const control = root.querySelector('[data-ui="calibrate.compute.preference"]');
        const wrapper = root.querySelector(".calibrate-compute");
        const rect = control.getBoundingClientRect();
        const canvas = document.createElement("canvas"); canvas.width = canvas.height = 1;
        const ctx = canvas.getContext("2d");
        const rgb = color => { ctx.clearRect(0, 0, 1, 1); ctx.fillStyle = color; ctx.fillRect(0, 0, 1, 1); return [...ctx.getImageData(0, 0, 1, 1).data].slice(0, 3); };
        const lum = color => rgb(color).map(v => { v /= 255; return v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4; }).reduce((s, v, i) => s + v * [.2126, .7152, .0722][i], 0);
        const texts = [...wrapper.querySelectorAll("p, label, summary")].filter(e => e.getBoundingClientRect().height > 0);
        const contrast = texts.map(e => {
          let bg = e;
          while (bg.parentElement && getComputedStyle(bg).backgroundColor === "rgba(0, 0, 0, 0)") bg = bg.parentElement;
          const fgLum = lum(getComputedStyle(e).color), bgLum = lum(getComputedStyle(bg).backgroundColor);
          return (Math.max(fgLum, bgLum) + .05) / (Math.min(fgLum, bgLum) + .05);
        });
        return { id: root.getAttribute("data-gallery-id"), width: rect.width, height: rect.height,
          disabled: control.disabled, focused: document.activeElement === control, value: control.value,
          gpuDisabled: control.querySelector('[value="gpu"]').disabled,
          overflow: wrapper.scrollWidth > wrapper.clientWidth,
          minHitPx: Math.min(...[...wrapper.querySelectorAll("select, button, summary")].filter(e => e.getBoundingClientRect().height > 0).map(e => e.getBoundingClientRect().height)),
          minTextPx: Math.min(...texts.map(e => parseFloat(getComputedStyle(e).fontSize))),
          minContrast: Math.min(...contrast),
          focusOutline: getComputedStyle(control.parentElement).outline,
        };
      });
      if (check.height < 40 || check.minHitPx < 40 || check.minTextPx < 14 || check.overflow || check.minContrast < 4.5 || (check.focused && !check.focusOutline.includes("solid"))) problems.push(`${theme}.${lang}: ${JSON.stringify(check)}`);
      evidence.push({ theme, lang, ...check });
      await specimen.screenshot({ path: join(out, `${id}.${theme}.${lang}.png`), animations: "disabled" });
    }
    await context.close();
  }
} finally { await browser.close(); server.close(); }
writeFileSync(join(out, only ? `checks.${only.replace(/[^a-z0-9.-]/gi, "_")}.json` : "checks.json"), JSON.stringify({ evidence, problems }, null, 2));
console.log(`${evidence.length} GPU captures; ${problems.length} problems; ${out}`);
if (problems.length) { console.error(problems); process.exitCode = 1; }
