# P63 (Opus 5.5): the Hardware Rack and Signal Matrix skins

Checkout: `/home/user/Sound-Equalization-and-Correction-System-2R` (branch
`ccr-aa2605d0-38ypeu`). Do not commit; Claude reviews the diff and commits.
Use absolute paths. Another worker edits only `crates/` and
`src/ipc/bindings/` in a separate worktree; you never touch those.

SECS 2R gets skin sets (ADR 0010): the default tokens stay the Instrument
skin, and two alternatives arrive, `rack` (a 19-inch rack faceplate) and
`matrix` (a routing board of cells on a grid). The plumbing is already in:
`PrefsScope` applies `data-skin="rack|matrix"` on `<html>` (app) or on the
`.prefs-scope` div (gallery), the gallery toolbar and
`gallery.html?skin=` choose it, `scripts/gallery-shots.mjs` shoots both
skins in Korean light and dark, and `scripts/screen-tour.mjs` shoots them
in Korean dark. What is missing is the skins themselves and the setting
that picks one.

Read first, in this order: `CLAUDE.md` (the screen rules),
`docs/adr/0010-skin-sets.md` (the mechanism and the differentiation gate),
`docs/skins/README.md` (the shared articles), `docs/skins/rack.md` and
`docs/skins/matrix.md` (the two constitutions: these are your design
brief, answer their "새 요소를 이 스킨답게 만드는 법" checklists for every
component), `docs/skins/default.md` (what the default looks like so that
you do not copy it), `docs/DESIGN.md` (tokens, frame, components, gallery),
`src/styles/tokens.css`, `src/styles/contrast.test.ts` and
`src/styles/tokens.test.ts` (the gates your files must pass),
`src/app/prefs.tsx` (`SkinChoice`), then the stylesheets you will override:
`src/ui/ui.css`, `src/app/app.css`, `src/ui/plot/plot.css`,
`src/ui/maps/maps.css`, `src/ui/cellSelect.css`, `src/screens/*/*.css`.
Load the skill `anthropic-skills:make-interfaces-feel-better` with the
Skill tool before editing and read its `color.md` and `surfaces.md`.

## What you build

1. **`src/styles/skins/rack.css`** and **`src/styles/skins/matrix.css`**,
   imported by `src/styles/base.css` right after `tokens.css`. Each file
   has two parts, in this order:
   - *Token overrides.* A light block `[data-skin="rack"] { … }`, the
     explicit dark block `[data-skin="rack"][data-theme="dark"] { … }` and
     the system-preference copy
     `@media (prefers-color-scheme: dark) { [data-skin="rack"]:not([data-theme="light"]) { … } }`
     (the two dark blocks must be identical: `contrast.test.ts` compares
     them). Override the `tokens.css` names the constitution's colour table
     lists (backgrounds, surfaces, rail, text, borders, status colours,
     plot and map and meter tokens, shadows, radii, `--press-scale`), set
     the accent through its inputs (`--accent-hue`, `--accent-chroma`,
     `--accent-l`, `--accent-hover-l`, `--accent-soft-l`, `--accent-soft-c`,
     `--on-accent`) and repeat the accent inputs under
     `[data-skin="rack"][data-accent]` so the tutorial accent does not take
     the lamp colour (both constitutions say the tutorial keeps the skin's
     accent). Skin-private tokens are named `--skin-*` (for example
     `--skin-segment`, `--skin-well`, `--skin-bronze`, `--skin-film`,
     `--skin-grid-pitch`); every colour-valued token in the light block
     needs a dark value too (the test checks name parity on colour-valued
     names). Values are OKLCH. Start from the tables in the constitutions
     and move lightness, never hue, when `contrast.test.ts` reports a pair
     under its floor.
   - *Structural overrides.* `[data-skin="rack"] .ui-button { … }` and so
     on for the frame, the kit, the plots, the maps and the screens: this
     is where the eight gate items (ADR 0010 §3) are answered. Read tokens
     only here; raw colours live in the token blocks above. Draw everything
     with gradients, `box-shadow` and pseudo-elements; no images, no SVG
     assets, no new fonts. Do not change the DOM, handlers, `aria-*`,
     focus order, `data-ui` ids or catalog strings; do not hide or reorder
     elements. Keep the 14 px floor, the 40 px hit areas, visible keyboard
     focus and zero motion under reduced motion.

   Rack in one line: brushed faceplates with 3 px machined corners, bevels
   instead of shadows (light from the top-left), four slotted screws on
   cards, bezel LEDs for every status, LCD wells (dark in both finishes,
   phosphor-green mono digits) for every number, machined caps for buttons
   that sink when pressed, amber for the accent and warm ivory ink.
   Matrix in one line: radius 0 and 1 px rules everywhere, a 24 px grid
   texture on the board surface between cards (never inside card bodies),
   coordinate cells on the rail and in tables, every authoritative number
   in a sunken mono box cell, crosspoint hover (row band × column band),
   cyan only for engaged/selected/routed data, and cancelled-flight styling
   (dotted rule, hollow lamp, strikethrough) for disabled things that stay
   on the board. Both constitutions have the full answer per component.
2. **The setting.** In `src/app/menus.tsx`, between the theme and the logo
   choices, add a `Segmented<SkinChoice>` with `id="frame.settings.skin"`,
   label `t("frame.settings.skin")`, options `default`, `rack`, `matrix`
   labelled `frame.settings.skin.default|rack|matrix`, value
   `prefs.skin ?? "default"`, `onChange={(skin) => prefs.set({ skin })}`.
   Add the four keys to `src/i18n/ko.json` and `en.json` next to the theme
   keys with these provisional strings (ASTRA polishes them later): ko
   `스킨`, `기본`, `하드웨어 랙`, `시그널 매트릭스`; en `Skin`, `Default`,
   `Hardware Rack`, `Signal Matrix`. Persistence to the service is not
   yours (ASTRA wires `AppSettings.skin` in `App.tsx`); the gallery's
   `frame.settings` specimen shows the new row.
3. **The gates.** `src/styles/tokens.test.ts` currently allows colour
   values only in `tokens.css`; extend its filter so files under
   `styles/skins/` may hold colours too (they are token files). Add a short
   vitest test in `src/app/AppFrame.test.tsx` or a new
   `src/app/prefs.test.tsx` that a `PrefsScope` with `skin: "rack"` sets
   `data-skin="rack"` on its scope and `skin: "default"` sets none.
4. **Component hooks, only if a constitution's answer cannot be drawn from
   the existing DOM.** You may add a neutral class or `data-*` attribute
   to a kit component (never a wrapper element, never a token value
   change in `tokens.css`). When you do, prove the default skin did not
   move: shoot the affected specimens before and after with
   `gallery-shots.mjs` and compare the PNG bytes (`cmp`). List every such
   hook in the report with its proof.

## Rules

- `docs/skins/README.md` "세 스킨이 함께 지키는 것" and "공유 조문" bind
  you; the constitutions' "하지 말 것" sections are hard limits.
- The differentiation gate: for each of the eight items, Rack and Matrix
  must differ from the default and from each other in structure (shape,
  depth, texture, lamp grammar, type treatment), not in palette alone.
  Put the answers in the report as a table.
- `tokens.css` values are off limits (the default skin was audited in
  this round). Skin files override; they do not edit.
- Do not touch `crates/`, `src/ipc/bindings/`, `features.toml`, `docs/`,
  `CHANGELOG.md`, the Python scripts, or `scripts/*.mjs` beyond what a
  skin needs (nothing, normally).
- Korean has no case; `text-transform: uppercase` is fine (it affects
  English only), but never change a catalog string to get a look.
- Canvas plots read tokens through `src/ui/plot/color.ts` and repaint on
  theme change; check in the browser that a skin's `--plot-*` tokens reach
  the graphs (the gallery `plot.*` specimens).

## Verification (foreground; finish every command before you report)

```sh
cd /home/user/Sound-Equalization-and-Correction-System-2R/apps/secs2r/ui
export PATH=/opt/node24/bin:$PATH
npm run typecheck
npm test
npm run build
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/gallery-shots.mjs --out /tmp/claude-0/-home-user/ac5d27f7-127b-55fa-93b4-77862e05bb47/scratchpad/shots/p63-gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/screen-tour.mjs --out /tmp/claude-0/-home-user/ac5d27f7-127b-55fa-93b4-77862e05bb47/scratchpad/shots/p63-tour
CONTRAST_TABLE=1 npx vitest run src/styles/contrast.test.ts --reporter=verbose
```

`npm ci` has already been run. The shot scripts fail on a page error, a
console error, a missing specimen or text under 13 px; a failure is yours
to fix. Look at the PNGs (the Read tool shows images): every `*.rack.png`
and `*.matrix.png` specimen in both themes, and the tour's
`*.dark.ko.rack.png` and `*.dark.ko.matrix.png` scenes. Iterate until the
screens read as the constitutions describe; a skin that only recolours is
a failed round. Keep only the PNGs you cite in the report; delete the rest.

## Report

The files you changed; the differentiation gate table (eight rows, three
columns: Instrument as-is, Rack, Matrix, each a phrase about structure);
any component hook you added with its before/after byte-compare proof;
the measured contrast table for both skins (from `CONTRAST_TABLE=1`) and
which values you moved from the constitutions' tables and why; the output
tail of every verification command; the PNG paths you kept; what you could
not do.
