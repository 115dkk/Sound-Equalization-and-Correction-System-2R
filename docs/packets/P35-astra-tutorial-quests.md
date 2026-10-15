# P35 (ASTRA): the tutorial quests on the new screens

Worktree: `/home/user/wt/p35` (branch `m5/p35`). Do not commit; Claude
reviews the diff and commits.

The tutorial engine is in place (P32: overlay, coach card, quest list,
pause, glossary; `apps/secs2r/ui/src/screens/tutorial/`), and the app runs
it on a tutorial project with five **sample** quests (`sampleQuests.ts`,
wired in `src/App.tsx`). This packet ports upstream's real quest list onto
the new screens' `data-ui` ids, so the tutorial walks the whole app.

Read first: `CLAUDE.md`, `docs/M5.md` (§3 U-items, §6), `docs/M5-reach.md`
(upstream widget → new `data-ui` id, the map you port by),
`apps/secs2r/ui/src/screens/tutorial/{types.ts,engine.ts,sampleQuests.ts,Tutorial.tsx}`,
`src/App.tsx`, each `src/screens/<step>/types.ts` and `use<Step>Screen.ts`
(what a quest can read and do), and upstream
`src/secs/wizard/tutorial_quests.py` (the 38 quests, the rules in its
docstring, `GLOSSARY`), `src/secs/wizard/en_g9.py` (their English) and
`src/secs/wizard/tutorial_overlay.py` (how they run). The upstream tree is
read-only.

## What you build

1. **`src/screens/tutorial/quests.ts`**: `tutorialQuests(host: TutorialHost): Quest<TutorialView>[]`,
   upstream's quests in upstream order, one entry each, ids
   `<step>.<name>` (e.g. `connect.retry`). Map upstream screens to steps:
   1 → `connect` (devices), 2 → `system` (configuration), 3 → `connect`
   (levels), 4 → `system` (layout), 5 → `measure`, `CAL_STEP` →
   `calibrate`, 8 → `export`. Where the new app puts two upstream screens on
   one step, keep upstream order inside the step and order the steps as the
   rail does (system, connect, measure, calibrate, export); say in the
   report which quests moved and why.
   - `targets` are `data-ui` ids from `docs/M5-reach.md` and the screens.
     A tab or section that must be open first: use the screen's host state
     through `TutorialHost` (as upstream `_on_tab` does), not a click
     simulation.
   - `look` quests: text from upstream `CAPTIONS`/quest text, rewritten for
     the new screen (its labels, not upstream's), 합니다체.
   - `act` quests: `done`/`base` read `TutorialView`; `auto` calls the same
     screen action a user would (through `TutorialHost`), with `note`
     saying what was done; `blocked` gives the reason when the action is
     not possible now; `busy` while a job runs; `keep` the ids the card
     must not cover (meters, status lines); `when` as upstream (BA and
     infra quests need two subs).
   - The measure step's demo-data quest uses the tutorial-only 데모 데이터
     button (`measure.sample.load`, `sample_load`).
   - Glossary `terms`/`whyTerms` as upstream.
2. **`TutorialView` and `TutorialHost`** (same file or `questHost.ts`):
   the view is built in `App.tsx` from the screen hooks' views and the
   host state (step, help open, tabs); the host is the screen hooks'
   actions plus the host setters the quests need. Keep both small: only
   what a quest reads or does.
3. **The app** (`src/App.tsx`): replace `sampleQuests` with
   `tutorialQuests`. Keep `sampleQuests.ts` for the gallery and the engine
   tests.
4. **Catalog**: keys `tutorial.q.<id>.<field>` (`title`, `text`, `why`,
   `hint`, `ok`, `note`, `busy`, `blocked`) in `ko.json` and `en.json`.
   Korean from upstream, reworded to the new labels; English from
   `en_g9.py`, reworded likewise. Plain sentences: no exclamation marks, no
   "~볼까요?" cheerleading unless the act needs a question; one idea per
   sentence. Remove the `tutorial.sample.*` keys only if nothing uses them
   (the gallery does — keep them).
5. **Gallery**: one specimen `tutorial.live` that runs `App` on the mock
   with a tutorial project and shows the first act quest of each step
   (five shots, light/dark, ko/en by the existing shot script).

## Tests (vitest)

- **Every target exists**: for each quest, render its step's screen with
  the fixture view P30–P32 use for the tutorial state (or the app on the
  mock, after the steps the quests before it would have done) and assert
  each `data-ui` id is in the DOM.
- **Act quests**: for each, `done` is false on the starting view and true
  after the user action (call the hook action on the mock); `auto` reaches
  the same `done`; `blocked` returns a reason in the blocked state.
- **Order and count**: the list matches upstream order; the test lists the
  upstream quests that have no counterpart (if any) with the reason.
- **Catalog**: every key a quest names exists in both catalogs with the same
  placeholders.
- **Walk**: the app on the mock with a tutorial project, pressing 계속 (and
  the second 계속 for act quests), reaches the end card without an error.

## Rules

- No markup or style changes: do not edit the screens' `.tsx`
  components, their CSS, the kit, the frame or the tutorial engine's
  components. A quest whose target has no `data-ui` id: point at the
  nearest id that exists, and list it in the report (Claude adds the id).
- No new npm dependencies.

## Files you may change

`apps/secs2r/ui/src/screens/tutorial/quests.ts` (new), `questHost.ts`
(new, if you split), their tests, `src/App.tsx`, `src/gallery/` (the
`tutorial.live` specimen), `src/i18n/{ko,en}.json`, `docs/M5-reach.md`
(the tutorial rows: point them at `quests.ts`).

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p35/apps/secs2r/ui
export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH
npm ci --no-audit --no-fund
npm run typecheck
npm test
npm run build
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/gallery-shots.mjs --out /tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/shots/p35
```

Look at the `tutorial.live` shots, then delete the shots folder except
those PNGs.

## Report

The quest table (id, step, kind, targets, upstream line), quests moved or
dropped and why, targets pointed at a nearer id, the tests, and the checks
with their results.
