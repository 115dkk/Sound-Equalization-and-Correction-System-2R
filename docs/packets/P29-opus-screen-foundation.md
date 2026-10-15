# P29 (Opus 5.5): tokens, the app frame, the component kit, graphs, maps and a gallery

Worktree: `/home/user/wt/p29` (branch `m5/p29`, from `m5/plan`). Work only
there and only under `apps/secs2r/ui/` and `docs/DESIGN.md`. Do not commit;
Claude reviews the diff and commits. Install with Node 24 when available
(`npm ci`); if only Node 22 / npm 10 is present, say so in the report.

Read first: `docs/M5.md` (all of it), `docs/M5-inventory.md` (what the
screens must reach later), `docs/adr/0003-stack-and-ipc.md` (stack, screen
rules, one IPC command, pushed events, codes not sentences),
`docs/ARCHITECTURE.md` §7, `CLAUDE.md` (screen rules), the skill
`anthropic-skills:make-interfaces-feel-better` (load it with the Skill tool
before you draw anything; it is the review gate), and the current screen:
`apps/secs2r/ui/src/{App.tsx,main.tsx,styles.css,ipc/client.ts,i18n/*}` and
the generated types in `src/ipc/bindings/`.

## What the app is for

SECS 2R measures a home-theatre or studio room through a microphone
(native audio, REW, or a simulated room), designs room correction for every
speaker and subwoofer (levels, delays, EQ, crossovers, sub alignment,
optional MIMO), and exports a CamillaDSP configuration and REW files. Its
users are enthusiasts and installers who read graphs and numbers; the
upstream screens were called confusing because of layout, not because of
the number of options (ADR 0003). The screen must feel like a calm,
precise instrument: dense where data matters, quiet everywhere else, one
obvious next action per step, long computations that never block the
screen and stop when cancelled.

It is a desktop app (Tauri 2, WebView): default window 1280×800, minimum
1024×700; mouse and keyboard; offline (no web fonts or CDNs).

## Scope of this packet

The foundation the step screens (P30–P32) are built from, shown in a
gallery. No step screen yet beyond placeholders; no real IPC wiring (P33);
catalog wording is drafted here and polished later by ASTRA (P34).

1. **Design tokens** in `src/styles/tokens.css` (CSS custom properties
   only; no second style system, no CSS-in-JS): colour (neutrals, one
   accent, success, warning, danger, info; the tutorial variant changes
   only the accent), the channel colour sequence and line rules for
   graphs (sub lines thicker, position 1 bold, others thin and faint),
   type scale (UI text 14 px floor, captions 13 px floor; system font
   stack with Korean fallbacks: "Pretendard", "Apple SD Gothic Neo",
   "Malgun Gothic", "Noto Sans KR", system-ui), spacing, radius, borders,
   shadows, focus ring, motion (short, restrained, honours
   `prefers-reduced-motion`). Light and dark themes from the same tokens;
   default follows the system, overridable (`data-theme`). Numbers that
   change use `tabular-nums`. Write the token reference and the rules in
   `docs/DESIGN.md` (Korean, 합니다체, like the other docs).
2. **App frame** (`src/app/`): the five-step rail (시스템, 연결·점검,
   측정·가져오기, 보정, 내보내기) with done / current / blocked states and
   the reason a step is blocked; the project status summary (구성 요약,
   측정 위치 k/N, 보정 상태, 측정 경로 상태, 파일 이름 or 저장 안 됨); the
   project menu (새로 만들기, 열기, 저장, 다른 이름으로 저장) and settings
   (언어, 전문가 표시, 테마); the step header (title, one-line purpose, the
   one primary action, the blocked reason); keyboard shortcuts (Alt+←/→,
   Ctrl+1…5, Ctrl+S, Ctrl+Shift+S, Ctrl+O, F1) as a single registry.
3. **Job tray** (`src/app/jobs/`): one place for every running job
   (U8): name, stage, progress `done/total` or indeterminate, elapsed and
   estimate, a cancel that calls `job_cancel`, and a history of `Log`
   events and errors (cause and next step, raw `detail` behind "자세히").
   It renders from `JobEvent`s (`Started`, `Progress`, `Log`, `Finished`)
   and from the job snapshots `jobs` returns; no polling.
4. **Component kit** (`src/ui/`): card, section with a collapsible
   "고급", status pill, buttons (primary, secondary, quiet, danger; one
   primary per view), number field (unit, range, step, invalid state),
   select, stepper, toggle, segmented control, table with group header
   rows, level meter (−60…0 dBFS, the −30…−15 band shaded, clipping),
   progress bar, toast, dialog (with confirm-and-undo for actions that
   clear measurements, U5), tooltip (800 ms delay, width cap), empty and
   error states. Accessible by keyboard, visible focus, labels tied to
   inputs, roles and `aria-*` where needed.
5. **Graphs** (`src/ui/plot/`): a `LogPlot` on uPlot (MIT; pin an exact
   version) with the log-frequency axis and fixed ticks 5, 10, 20, 50, 100,
   200, 500, 1k, 2k, 5k, 10k, 20k; series styling from the tokens; legend
   with channel chips and per-position chips (toggle, hover highlight);
   an axis-range popover (frequency and level limits, Auto, Apply, the
   "lower must be below upper" error); target-point editing (drag, double
   click adds, remove; clamp 10–24000 Hz, ±30 dB, at least 2 points);
   a draggable vertical line (the XO, bounded); shaded band outside a
   range (the RC band). Smooth with dozens of curves of a few thousand
   points. Theme changes restyle without a reload.
6. **Maps** (`src/ui/maps/`, hand-written SVG, no library): the room map
   (screen, seat, a marker per speaker: circle main, dashed circle
   top/height, square sub; code and outputs; unassigned row; selectable),
   the arrangement map (screen, seat or sofa, 1 m circle, 30 cm scale,
   numbered positions, position 1 larger, half fill for ±20 cm height,
   done / current, keyboard arrows to move the selection), and the small
   arrangement thumbnail. They take data as props (positions and
   geometry come from the service later; use fixtures now that copy
   upstream `layout_map.py`'s geometry).
7. **Mock service** (`src/ipc/mock.ts`): an `Invoke` implementation for
   `makeClient` that answers from fixtures typed by the generated
   bindings, can emit `JobEvent`s on a timer, and can be told to fail, so
   the gallery and tests run without Tauri. Keep `client.ts`'s contract.
8. **Gallery** (`gallery.html` as a second Vite entry, not shipped in the
   app bundle's main route): every component and the frame in each state
   (empty, normal, busy with a job, stale, error, tutorial accent), light
   and dark, Korean and English, with a stable `data-gallery-id` on each
   specimen so CI can screenshot them one by one.
9. **Tests** (vitest; add `@testing-library/react` and a DOM environment
   pinned to exact versions): the frame renders each rail state and the
   blocked reason; the job tray renders a job from events and calls
   `job_cancel`; number field ranges; the target editor clamps; the map
   keyboard selection; the catalog test still passes (same keys and
   placeholders in `ko` and `en`); a token test that no UI text style is
   under 14 px (captions 13).

## Rules

- `make-interfaces-feel-better`: sentences in 합니다체, states and labels
  as noun phrases, labels say the effect, options named not numbered, one
  primary action per view, one accent, 14 px floor, tabular numbers,
  errors with cause and next step.
- Every string through a catalog key; add keys to both `ko.json` and
  `en.json` (draft English; ASTRA polishes later). Keys are stable ids
  that the reach table (`docs/M5-reach.md`) will cite.
- Stable `id`s or `data-ui` attributes on every interactive element the
  tutorial will point at later.
- React 19 function components, TypeScript strict; no state library unless
  you can say why in the report; no CSS framework.
- Dependencies: only what this packet needs, exact versions, MIT/Apache/BSD
  licences (the CI runs `cargo deny` for Rust only; list the npm licences
  in the report).

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p29/apps/secs2r/ui
npm ci --no-audit --no-fund
npm run typecheck
npm test
npm run build
npx vite build --config vite.config.ts   # also builds the gallery entry
```

Then open the gallery in the pre-installed Chromium with Playwright
(`PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers`; do not run `playwright
install`) and screenshot the frame and a few specimens in light and dark
into `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/p29-shots/`
to check them yourself against the rules. Do not commit the screenshots.

## Report

What you built (files), the tokens and themes, each component and its
states, the dependencies with versions and licences, the tests, the
screenshots' paths, the rule checks you did, and anything left for
P30–P34.
