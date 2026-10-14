# M5 step screens: the contract every screen packet follows (P30, P31a, P31b, P32)

The step screens are drawn by Opus 5.5 on the foundation P29 built, in
parallel, one packet per area. This file is the part all of them share;
each packet adds its screens, its inventory sections and its layout.
Claude reviews the diff and the screenshots and commits; do not commit.

## Read first

- `docs/M5.md` (all), `docs/M5-inventory.md` (your sections and §2, §4),
  `docs/DESIGN.md` (all: tokens, layout, the kit, ids, catalogs, gallery),
  `docs/adr/0003-stack-and-ipc.md`, `docs/ARCHITECTURE.md` §7 (three
  disclosure levels), `docs/DEVIATIONS.md` (U2–U9), `CLAUDE.md` (screen
  rules).
- The skill `anthropic-skills:make-interfaces-feel-better`: load it with the
  Skill tool before you draw anything. It is the review gate.
- The code you build on: `apps/secs2r/ui/src/ui/` (kit), `src/app/` (frame,
  `model.ts`, `prefs.tsx`), `src/gallery/` (specimens; `FrameBox` in
  `frameSpecimens.tsx` draws a frame from fixed data), `src/ipc/bindings/`
  (the generated types of every IPC method), `src/ipc/fixtures.ts`.
- The upstream screens you port, under `src/secs/wizard/` (SECS 2,
  GPL-3.0; this repository is GPL-3.0 too and reads it freely). Port every
  item of your inventory sections; copy behaviour and limits, not layout.

## What a screen is

A screen draws a view and reports what the user did. It never calls the
service: P33 wires every screen to the IPC afterwards, so the boundary
below is P33's input and must be exact.

Each area lives in `apps/secs2r/ui/src/screens/<step>/`:

- `types.ts`
  - `<Step>View`: plain data, no functions. Service data uses the
    generated binding types (`import type { ConfigResult } from
    "../../ipc/bindings/ConfigResult"`), never a re-declared copy. Add
    only what the service cannot know (for example which tab is open when
    P33 must keep it across visits). Purely local state (hover, an open
    popover, an edit in progress) stays in the component.
  - `<Step>Actions`: one callback per user effect, named for the effect
    (`applyPreset`, not `onPresetClick`), each with a JSDoc line naming the
    IPC method or host action it maps to (`/** config_apply_preset */`).
    Callbacks return `void` or a `Promise`; new data comes back as a new
    view, not as a return value. Destructive actions (U5: anything that
    clears measurements) return `Promise<string>`, the undo token, so the
    kit's `useConfirmAndUndo` can show the undo toast; the area also gets
    an `undo(token)` action (`/** undo */`).
- `<Step>Screen.tsx`: `export function <Step>Screen({ view, actions })`,
  the step's content area inside the frame, and
  `export function <step>Header(view, actions, t)` returning
  `{ primary?: PrimaryAction; status?: ReactNode }` for the frame's step
  header (one primary action per screen; its label says the effect).
- `<step>StepState(view): StepState` (in `types.ts` or its own file): what
  the rail shows for the step (done / ready / blocked with a reason),
  computed from the view the way upstream `can_advance()` decides. New
  reasons are appended to `BLOCKED_REASONS` in `src/app/model.ts` with
  their `frame.blocked.<reason>` and `.short` keys.
- `fixtures.ts`: typed views for every state the specimens draw. Values are
  plausible for the sample 7.1.4 room (`src/ipc/fixtures.ts`), reuse its
  roles and curves (`src/gallery/data.ts`).
- `specimens.tsx`: `SpecimenDef[]` with ids `screen.<step>.<state>`, 1280×800
  frames through `FrameBox`, plus one `screen.<step>.<state>.min` at
  1024×700 for the densest state. Register the section in
  `src/screens/specimens.ts` (one import, one entry).
- `<Step>Screen.test.tsx` (`// @vitest-environment happy-dom`): every
  fixture state renders; the main controls call their action with the right
  arguments; the step state and its reason; a constant `REACH_IDS` lists
  every id you wrote into `docs/M5-reach.md`, and the test checks each one
  is rendered in at least one state.

Long work is a job (measure, calibrate, export …): the screen shows that a
job of its kind is running (from the view) and disables what would start a
second one; progress, cancel and history live in the job tray (U8). No
overlays, no spinners covering the screen.

## Rules

- **Kit first.** Use `src/ui/` and the tokens. A new generic component goes
  in a new file under `src/ui/` and is exported by appending to
  `src/ui/index.ts`. Do not edit existing kit, frame, gallery or IPC files;
  the only shared edits allowed are appending to `src/ui/index.ts`,
  `src/screens/specimens.ts`, `BLOCKED_REASONS` in `src/app/model.ts`, the
  catalogs and your rows in `docs/M5-reach.md`. If an existing component
  needs a change, work around it and say so in the report.
- **Strings.** Every visible string is a catalog key. Your keys start with
  your area (`system.`, `connect.`, `measure.`, `calibrate.`, `export.`,
  `help.`, `tutorial.`); service `Notice` codes your screen shows map to
  `notice.<code>` (find the codes in `crates/secs-service/src` and
  `crates/secs-engine/src`; keep the params). Append your keys as one
  contiguous block at the end of both `src/i18n/ko.json` and `en.json`.
  Korean is the source: sentences in 합니다체, states and labels as noun
  phrases, labels say the effect, options named not numbered, errors with
  cause and next step, no exclamation marks. English is a draft; ASTRA
  polishes both in P34.
- **Numbers.** Format with units and `tabular-nums`. The screen never
  computes an engine value (curves, metrics, estimates, recommendations):
  they arrive in the view.
- **Disclosure.** Three levels (`ARCHITECTURE.md` §7): what most people
  need is visible; detail sits in `Advanced` (고급, collapsible); what
  upstream hid or only experts touch shows when `usePrefs().expert` is on
  (전문가 표시).
- **ids.** Every interactive element and every reach target carries
  `data-ui="<step>.<thing>"` (and the same `id` where a label points at it):
  stable, lower case, dot-separated, with `<role>` / `<n>` parts where they
  repeat. The tutorial (P32, P35) and the reach table use them.
- **Reach table.** Fill your sections of `docs/M5-reach.md`: one row per
  item of your inventory sections, its id(s), the deviation if any, your
  packet. A removed dead feature gets `—` and its deviation. No empty rows.
- **Layout.** Default window 1280×800, minimum 1024×700. The content area
  scrolls; the frame does not. Dense where data matters (tables, graphs),
  quiet elsewhere. Check both sizes in your screenshots.
- **Not yours.** No IPC calls (`useIpc`, `request`), no edits under
  `src/ipc/`, no Rust, no bindings, no new npm dependencies.

## Verification (foreground; finish before you report)

```sh
cd <your worktree>/apps/secs2r/ui
npm ci --no-audit --no-fund
npm run typecheck
npm test
npm run build
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome \
  node scripts/gallery-shots.mjs --out <scratchpad>/<packet>-shots
```

Use Node 24 (`export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH`).
The shots script fails on page errors and on text under 13 px. Open your
screenshots (light and dark, Korean and English, tutorial, the 1024×700
one) and fix what breaks the rules before you report.

## Report

Files; for each screen the exact `View` and `Actions` (every action with
its IPC method; P33 implements them); the states you drew; the reach rows
you filled; kit additions; the number of keys added; how each deviation in
your packet is met; the checks you ran with their results; what you left
for P33, P34 or P35.
