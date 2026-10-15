# P40 (Opus 5.5): screen polish after the M5 gallery review

Worktree: `/home/user/wt/p40` (branch `m5/p40`). Do not commit; Claude
reviews the diff and commits.

The M5 gallery review (`docs/M5-review.md`) found no HIGH and two LOW
items. Wiring (P33) and the reach table left three more small things. This
packet fixes all five without changing what any screen is for.

Read first: `CLAUDE.md` (the screen rules), `docs/DESIGN.md` (tokens, the
1024×700 minimum frame, the gallery), `docs/UI-TERMS.md`,
`apps/secs2r/ui/src/styles/` and `src/ui/` (the kit and its tokens), and
load the skill `anthropic-skills:make-interfaces-feel-better` with the Skill
tool before editing.

## What you fix

1. **Measure: position list rows wrap in English.** In the measure step's
   position list, labels such as "Front right · 20 cm up"
   (`measure.point.6`) break onto two lines in the label column, so rows in
   one list have different heights. Make every row one line in both
   languages at 1280 and at the 1024×700 minimum frame (the label column
   takes the room the row has; the status column keeps its width). Do not
   shorten the strings. Files: `src/screens/measure/` (the list component
   and its CSS).
2. **Calibrate: the verdict tiles' baselines.** On the verdict card, the
   "GD spread" tile (`calibrate.verdict.gd`) puts its band ("20–100 Hz") on
   a second line in English at 1024×700 while the other tiles do not, so the
   numbers sit at different heights. Give every tile the same form — label
   line, band line when the tile has a band, then the value — so the value
   baselines line up in both languages and both frames. Files:
   `src/screens/calibrate/Results.tsx` and its CSS.
3. **Calibrate: engine notes as upstream.** The verdict card shows the
   engine notes (`calibrate.verdict.notes`) only when 전문가 표시 is on.
   Upstream always shows them next to the total latency
   (`src/secs/wizard/step_calibration.py:2300`). Show the collapsed
   `<details>` whenever there are notes, expert or not. Keep the card's
   height at 1024×700 in check (the notes stay collapsed by default).
4. **Project: initial focus of the new-project dialog.** Opening
   새 프로젝트 puts focus on the dialog's close button. Put it on the
   first choice the user makes (the start choice, 빈 구성 / 샘플 방), using
   the kit's `data-autofocus` (`src/ui/Dialog.tsx` reads it). Check the
   other project dialogs (`src/screens/project/ProjectDialogs.tsx`): a form
   dialog focuses its first field, a confirm dialog keeps focus on its safe
   button (the kit's default for confirms). List what each dialog focuses
   in the report.
5. **Frame: the 'custom' preset in the status panel.** After the user edits
   a preset, the project status panel shows the raw value `custom`
   (`src/app/ProjectStatus.tsx`, `frame.status.config.value`). Show the
   catalog word the system step already uses for it, `system.preset.custom`
   (사용자 지정 / Custom). No new keys.

## Rules

- Change only what these five items need: the components and CSS named
  above, their tests, and gallery fixtures if a specimen needs a state to
  show the fix. Do not change IPC, state stores, hooks' actions, the
  catalogs, `data-ui` ids, or other screens.
- Tokens only (no new raw colours or sizes outside the token files), the
  14 px floor, `tabular-nums` where numbers line up.
- Add or extend vitest tests where behaviour changed: the notes render
  without expert; the new-project dialog's focused element; the status
  panel shows the catalog word for `custom`.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p40/apps/secs2r/ui
export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH
npm run typecheck
npm test
npm run build
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/gallery-shots.mjs --out /tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/shots/p40
```

`npm ci` has already been run in this worktree. The shot script captures
the viewport only; to see the position list, which sits below the fold,
take your own full-page screenshots of the measure specimens with
Playwright (same Chromium path) into the same shots folder. Look at: the
measure specimens with the position list (en and ko, 1280 and the `.min`
1024×700 ones), the calibrate verdict specimens (`screen.calibrate.spl`,
`screen.calibrate.ba.min`, en and ko), `screen.project.new`, and
`frame.aside` or `app.live` for the status panel. Keep only the PNGs you
cite in the report; delete the rest.

## Report

Per item: what you changed (files), the before/after you saw in the shots
(with the PNG paths you kept), and the tests. Then the checks with their
results.
