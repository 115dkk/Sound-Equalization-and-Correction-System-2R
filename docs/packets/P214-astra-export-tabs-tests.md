# P214 (ASTRA): the 내보내기 tests after the move to destination tabs

Worktree `E:\wt\export-tabs`, branch `feat/export-tabs`, folder
`E:\wt\export-tabs\apps\secs2r\ui`. No Rust build is needed and none may be
started. `node_modules` is installed. The screen was redrawn by the main
session; your job is the tests of that screen. Your files, and the only ones
you may edit:

- `E:\wt\export-tabs\apps\secs2r\ui\src\screens\export\ExportScreen.test.tsx`
- `E:\wt\export-tabs\apps\secs2r\ui\src\screens\export\apply.test.tsx`
- `E:\wt\export-tabs\apps\secs2r\ui\src\screens\export\manage.test.tsx`

Do not edit any other file: not the screen (`ExportScreen.tsx`, `apply.tsx`,
`fields.tsx`, `export.css`), not `types.ts`, not `fixtures.ts`, not the
catalogs. Do not commit, do not push. Run every command in the foreground
and end your turn only after the report. If a test can only pass by changing
the screen, do not change it and do not bend the test: leave that test
failing and say so in the report with what you saw.

## What changed on the screen

Read `ExportScreen.tsx`, `apply.tsx`, `types.ts` and `fixtures.ts` in
`src/screens/export` first; they are the specification. In short:

- The 'export files' card (`data-ui="export.files"`, title `내보낼 곳` /
  `Export to`) no longer lists every export. It holds a tab list
  (`data-ui="export.tabs"`, `role="tablist"`) of five destinations in the
  order of `EXPORT_TABS`: `camilla_dsp`, `equalizer_apo`, `convolution`,
  `filter_text`, `rew`. A tab is `data-ui="export.tab.<tab>"`, `role="tab"`,
  with `aria-selected`, `data-state` (`tabState`: `idle`, `busy`, `done`,
  `failed`, `blocked`), its name, what the place is (`export.tab.<tab>.use`)
  and, when the state is not `idle`, a status dot plus visually hidden words
  (`export.tab.state.<state>`). One tab stop; ArrowRight, ArrowLeft, Home
  and End call `actions.selectTab` and move focus.
- Under the tabs stands one panel (`data-ui="export.panel"`,
  `role="tabpanel"`, `data-tab`, labelled by the chosen tab). It draws only
  the chosen tab's exports (`TAB_KINDS`): one `section[data-ui="export.kind.<kind>"]`,
  and in the `rew` tab the REW mdat followed by 고급 (`export.advanced`)
  with the staged mdat. Exports of other tabs are not in the document.
- `ExportView.tab` is the tab on show and `ExportActions.selectTab` changes
  it (session host state in the app; `defaultExportTab` until the user picks
  one). Every fixture carries a `tab`.
- The step header has no primary action any more. `exportHeader(view, t)`
  returns `{ status }` only. Each panel's action stands beside its title in
  `.export-item__action`: the main export (`export.run.<kind>`) is the one
  filled button (`ui-button--primary`) while its target is 파일만 (or it has
  no target); with 적용 or 관리 the filled button is `export.apply.<target>`
  and `export.run.<kind>` stays beside it as a secondary button. The staged
  mdat's button is secondary. CamillaDSP's export reads
  `frame.step.export.primary` while it is the filled action and
  `export.kind.camilla_dsp.run` beside 적용.
- A disabled action names its reason with `aria-describedby`: the format's
  blocked sentence (`export.kind.<kind>.blocked`), the design name's message
  (`export.design_name-error`), Equalizer APO's own sentence (`eqapoReason`),
  else the header's pill (`frame.header.export.status`, which then holds
  `export.status.problem`). The header's problem pill speaks for the tab on
  show only (`tabProblem`).
- Inside a panel the order is: title and actions; the blocked sentence; how
  the last apply ended (`export.apply.<target>.outcome`, now outside
  `export.apply.<target>.panel`); how the last export ended
  (`export.outcome.<kind>`); 적용 방식 with its facts; what is written (the
  mdat's contents, the CamillaDSP devices, the extra rates); the design name
  (each filter tab's main export, not the tutorial); the folder line and
  what lands in it.
- The folder line (`export.folder`) is one line: name, path, the two
  buttons. Where the folder comes from (`export.folder.default` /
  `export.folder.stored`) moved into the name's help (`export.folder.help`).
  It is drawn in a tab's main export, never in the tutorial.
- Headings: h2 the card, h3 a tab's main export, h4 the staged mdat. The two
  group headings (`export.group.*`) and `section.export-group` are gone, and
  so is the `export.run.busy` pill (every export has a button now).

## What is already done in the test files

- `Mounted` in all three files keeps the tab in state: a click on a tab (or
  `actions.selectTab`) redraws the screen on that tab, as the app does.
- `show(kind)` in `ExportScreen.test.tsx` and `apply.test.tsx` clicks the
  tab that holds `kind`.
- `actions()` has `selectTab`, and the `exportHeader` calls take two
  arguments. Some imports for the work below are in place and unused yet.

## The work

`npx vitest run src/screens/export` now fails 28 tests in these three files
(and `npx tsc -b --pretty false` reports the leftovers). Rewrite those tests
for the tabbed screen. Keep what each test proves; change how it gets there.

1. A test that walked every row at once opens each tab in turn (`show(kind)`
   or a fixture with the right `tab`) and asserts the same thing there. Do
   not drop a kind from a loop because it is in another tab.
2. A test about the header's primary becomes a test about the panel's filled
   action: which button is `ui-button--primary`, that there is exactly one
   filled button on the screen in that state, that the step header holds no
   button (`document.querySelector(".step-header .ui-button")` is null),
   busy and disabled as before, and what `aria-describedby` points at.
3. The two tests about row order and the two titled groups are replaced by
   tests of the tabs: the five tabs in `EXPORT_TABS` order with their names
   and captions in Korean and English; roles and `aria-selected`; the panel
   labelled by the chosen tab and holding only that tab's sections; the
   heading levels; the design name once in each filter tab and absent from
   `rew`; the CamillaDSP devices in the CamillaDSP tab only; the arrows,
   Home and End (wrap at both ends) calling `selectTab`; a click calling
   `selectTab`.
4. Add tests for `tabState` and the dots: `busy` while the tab's export or
   apply runs, `failed`, `done`, `blocked` (the filter text with subs or
   MIMO, the REW tab with a stale cache unless the scope is measurements),
   how an apply ended counting only while its target is beyond 파일만 (an
   apply in flight is busy whatever the level), the priority
   busy > failed > done > blocked, the `data-state` on the tab, the hidden
   words, and no dot when idle. Add tests for `defaultExportTab`: CamillaDSP
   by default, Equalizer APO when it alone is beyond 파일만, CamillaDSP in
   the tutorial and off Windows.
5. Add a test for the header's pill following the tab: a design name problem
   shows the pill in a filter tab and not in `rew`; an ASIO side without a
   driver shows it in the CamillaDSP tab only.
6. The order inside a panel: assert with `compareDocumentPosition` that the
   outcome and the blocked sentence come before 적용 방식 and the fields, and
   that the folder line comes before the 'where' line.
7. `REACH_IDS`: add `export.tabs`, `export.panel` and the five
   `export.tab.<tab>`; the reach test must still find every id, drawing the
   states on every tab where needed.
8. The folder's origin: assert it through the help (`export.folder.help`
   opens a tooltip with the right sentence for a stored and a default
   folder), not as text in the row.

Do not weaken an assertion to make it pass (no swapping an exact string for
`toBeTruthy`, no deleting a case). Test names say what the screen does in
the tabbed words; rename the ones that speak of rows and the header's
primary. Keep the file's style (one `describe`, plain `it`, the `ui()`
helper, Korean strings asserted literally where they are today).

## Verify

```sh
cd /e/wt/export-tabs/apps/secs2r/ui
npx vitest run src/screens/export
npx tsc -b --pretty false
```

Both must be clean (the type check may still list errors outside these three
files; report them, do not fix them).

## Report

- The tests you rewrote, added and removed, one line each, with what a
  removed test was replaced by.
- Any test left failing, with the assertion and what the screen drew.
- Anything on the screen that looked wrong while you wrote the tests (a
  missing reason for a disabled button, two filled buttons, an id that
  appears twice). Describe it; do not fix it.
- The last lines of both commands' output.
