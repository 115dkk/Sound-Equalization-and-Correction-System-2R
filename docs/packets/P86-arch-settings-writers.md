# P86: audit #92 architecture C16 — one ordered writer per settings bundle, one place per setting (A-80, with TD-68)

Two parts: **P86a (ASTRA)** state and call code; **P86b (Opus 5.5)** the
screens. Worktree `/home/user/wt-ui`, on the branch the run brief names. Do
not run cargo, do not push, do not spawn sub-agents. Claude reviews, re-runs
the gates and re-authors the commits.

Read first: `CLAUDE.md`, `docs/ARCHITECTURE.md` §7 (P58: one canonical place
per setting), `docs/DESIGN.md`, `TECH_DEBT_AUDIT.md` row TD-68, and in full:
`src/App.tsx` (the `PrefsScope` `onChange` that writes app settings),
`src/screens/calibrate/useCalibrateScreen.ts` (compute preference write),
`src/screens/project/useProjectScreen.ts` (recent projects write),
`src/screens/system/useSystemScreen.ts` (the DSP queue and rebase — the only
writer that is ordered today), `src/state/session.ts` (`configure`),
`src/state/effects.ts`, `src/state/store.ts`, and their tests.

## Problem

App settings are written from three places, each sending a full snapshot it
read earlier, in no order: changing the theme while the compute preference is
being saved can put the old preference back (TD-68). Only the DSP fields are
written in order and rebased on the latest response. The UI keeps 20 recent
projects while the service keeps 10 (`crates/secs-service/src/project_edit.rs:790`).
Start delay and sweep length can be changed on two screens each (start
delay on 시스템 and 측정·가져오기, sweep length on 연결·점검 and 측정·가져오기),
against P58. The maintainer chose **측정·가져오기** as the one place for both.

## P86a (ASTRA) — writers

Design (fixed):

```ts
// src/state/writers.ts
export type Patch<T> = Partial<T> | ((latest: T) => Partial<T>);
export function createWriter<T>(deps: {
  latest: () => T | null;              // the store snapshot the service last answered
  write: (next: T) => Promise<T>;      // the request; resolves with the service's answer
}): { edit(patch: Patch<T>): Promise<boolean> }; // true when the service accepted it
```

- Edits run one at a time in call order. Each one builds `next` from
  `latest()` at the moment it is sent (not when it was queued) plus its patch,
  so a later edit never overwrites an earlier one with stale fields. A failed
  edit resolves `false` (the session's `call` already reports the error) and
  the queue continues.
- The session owns one writer per bundle and exposes them as
  `session.writers`: `app` (`app_settings_set`, `AppSettings`), `dsp`
  (`dsp_set`; a preset change and a fields patch are both edits), `layout`
  (`layout_set` fields `start_delay` and `timing`; arrangement changes with
  `confirm` stay a direct call because they return an undo token), and
  `measurement` (today's `configure(patch)` becomes `writers.measurement.edit`
  with the same params it builds now; keep `configure` as a thin alias if
  many callers use it).
- Callers: the `PrefsScope` `onChange` in `App.tsx`, the compute preference
  in `useCalibrateScreen.ts`, the recent projects in `useProjectScreen.ts`
  (a function patch; send at most 10, the service's bound, and say in a
  comment where that bound lives), the DSP actions in `useSystemScreen.ts`
  (its local queue and rebase go away), the start delay and timing actions,
  and every `configure` caller.
- Each setting's options come from one place (for example the sweep
  lengths from the service's `MeasureConfigResult.sweep_lengths`, the start
  delays from one constant); remove a duplicated list where a screen keeps
  its own.

Tests (`src/state/writers.test.ts`, written first): two edits to different
fields in quick succession both land; an edit queued behind a slow one is
built from the slow one's answer; a failed edit returns false and the next
still runs; per bundle in the session (app: theme during a pending compute
save keeps the compute choice — TD-68's scenario, red today through the
session and mock; dsp: today's rebase test moves here; recent projects keep
10).

Commit: `fix(audit): A-80 설정 묶음마다 순서 있는 작성기 하나로 씁니다 (#92 C16, TD-68)`

## P86b (Opus 5.5) — one place per setting

- Remove the start delay control from 시스템 (`SystemScreen.tsx`, id
  `system.start_delay`, its `setStartDelay` action and view field) and the
  sweep length control from 연결·점검 (`ConnectScreen.tsx`, id
  `connect.sweep_length`, its `setSweepLength` action and view field).
  측정·가져오기 keeps both (`measure.options.delay`, `measure.options.sweep`).
  Where a removed control leaves a gap, close it so the section still reads
  as designed in all three skins.
- The tutorial step `system.timing` (`src/screens/tutorial/quests.ts:48`)
  highlights `system.start_delay`; make it highlight what remains in that
  section, or move the start-delay part of the step to the measure step's
  quest — read the quest list and keep the tutorial's flow intact; update the
  tests and `docs/M5-reach.md` (rows for both settings and the quest table).
- Remove the keys only the removed controls used from `ko.json` and
  `en.json`; run the i18n tests.
- Update the gallery specimens and tour scenes that show the removed
  controls.

Commit: `fix(audit): A-80 시작 지연과 스윕 길이를 측정 화면에서만 정합니다 (#92 C16)`

## Gates (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui && PATH=/opt/node24/bin:$PATH npm run typecheck && PATH=/opt/node24/bin:$PATH npm test && PATH=/opt/node24/bin:$PATH npm run build && PATH=/opt/node24/bin:$PATH npm run build:gallery
```

## Report (final answer)

Commit hash, files, tests added/moved/deleted, red failure, green result,
counts before/after, removed keys (P86b), the last gate output tail, and
anything left undone.
