# P209 (Opus 5.5): audit #186, B-6 and B-29 as Category A items (screen performance)

Worktree `E:\wt\audit-ui`, branch `audit/186-ui-perf` (Claude creates it
from the current `fix/audit-186`; `git status` is clean when you start).
Work only in `apps/secs2r/ui/src`. `node_modules` is installed. **No cargo,
no `check_feature_tests.py`** (it starts cargo and builds a target into the
worktree). Do not edit `src/ipc/bindings/**`, `src/ipc/recorded/**` or the
i18n catalogs. One commit per item with the titles given, trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push, do
not rebase, do not touch ADRs, `docs/**`, `CHANGELOG.md`, `audit-state.md`,
Rust files or `features.toml`.

Both findings were tried as pure refactors (Category B) earlier and stopped
because existing tests assert the old structure. They are Category A now:
first a test that states the new behaviour and fails on today's code (Red),
then the change (Green), then the old tests that pinned the old structure
are updated to the new seam and listed in the report. The rendered screens
(ids, text, order) must not change.

Read first: `CLAUDE.md` (screen rules), `src/state/session.ts`,
`src/state/store.ts`, `src/state/react.ts`, `src/state/effects.ts`,
`src/App.tsx`, `src/screens/calibrate/useCalibrateScreen.ts`,
`src/screens/connect/useConnectScreen.ts`, `src/state/session.test.ts`.

## 1. B-6 · TD-176 · meter events reach only what reads the meter

Finding (audit #186, translated): `HostState` holds the UI state of all
five screens, so every patch (meter events from `session.ts` around the
`event.event === "meter"` branch, the XO drag's `previewHz`, the
`applyChecking` switch) reruns the hooks of all five screens and the
current screen renders again. An earlier commit (b5a51c64) already made
`host.set` skip identical patches. What is left: the level-monitor fields
(`meter`, `monitor`, `playing`, `playingAll`, `masterDbfs`, `monitorError`)
move out of `HostState` into their own store (`session.stores.monitor` or
the name the session's store list uses), read only by the hooks that show
them (connect's level check and wherever else `grep` finds a reader).

- Red: a test (in `src/state/session.test.ts` or a new
  `src/app/renderCount.test.tsx`) that renders the calibrate screen (or
  `App` on the calibrate step) with a render counter on its hook, feeds a
  burst of mock meter events through the session, and asserts the calibrate
  hook did not run again. It fails today.
- Green: the separate store and its subscribers. The meter display keeps
  its text and update rate.
- Update the tests that read `session.host.getSnapshot().meter` (one in
  `session.test.ts` around line 267, and any others `grep -rn "host.*meter"`
  finds) to read the new store; list them.
- Commit: `fix(audit): B-6 레벨 측정기 이벤트가 오면 측정기를 읽는 화면만 다시 표시합니다 (#186 TD-176)`

## 2. B-29 · TD-237 · measurement curves load when the measure stage needs them

Finding (translated): every position's and role's curves are read at start
and on every measurement or configuration change, at every stage, partly
only to know which roles were measured. An earlier commit (deabd22d) already
takes `measuredRoles` from `measurement_records`. What is left: the
`measurement_curves` read (`session.ts` around line 117, the resource list
that reads `records` and `curves`) happens only while the measure/import
stage is shown, and again when it is shown after a change that made the
cached curves stale; other stages never trigger it.

- Red: a test that opens the sample project on the calibrate step, makes a
  measurement-changing or configuration-changing call through the mock, and
  asserts that the mock received no `measurement_curves` request; then
  navigates to the measure step and asserts exactly one. It fails today.
- Green: make the curves resource lazy (read on first subscription or when
  the measure screen mounts, invalidated by the same events that invalidate
  it today).
- Update the three tests that relied on the eager reload (the earlier
  attempt named them as depending on it; find them by running `npm test`
  after the change, or by grepping for `measurement_curves` in the tests),
  and list them.
- Commit: `fix(audit): B-29 측정 곡선을 측정 단계가 보일 때만 읽습니다 (#186 TD-237)`

## Verification (foreground, in `apps/secs2r/ui`)

```sh
npm run typecheck
npx vitest run src/state src/app src/screens/connect src/screens/measure src/screens/calibrate
npm test
npm run build
```

Other sessions build Rust on this machine: a test that times out only in the
full run and passes alone (tutorial quests, navigation, flow, reach tables)
is load; rerun it alone and report it.

## Report

Per item: the Red test and its first failure line, the commit hash, the
files, the old tests updated and how, and
`git log --oneline fix/audit-186..audit/186-ui-perf`. Finish the report
before ending your turn.
