# P142 (Opus 5.5): MIMO를 보정 계산 앞에서 고릅니다

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-upper`
(branch `claude/rc-upper`). Do not commit, do not push. Work only under
`apps/secs2r/ui/`. Start after P141 (service: `CalibrateParams.mimo`) is in
the tree. P140 (RC upper basis on the range card) runs in the same branch;
if one agent does both, do P140 first.

Read first: `CLAUDE.md` (screen rules), `docs/DESIGN.md` (§1: the button
beside what it acts on), load the `make-interfaces-feel-better` skill, then
`apps/secs2r/ui/src/screens/calibrate/Settings.tsx` (`MimoSection`, the order
of the cards in `Settings`), `Results.tsx` (`RunControls`, `Verdict`: the plan
sentence and 보정 계산), `useCalibrateScreen.ts` (`calibrate`, `computeMimo`),
`settingsView.ts` (`MIMO_MIN_POSITIONS`), `fixtures.ts`, `specimens.tsx`,
`src/gallery/tour/scenes.ts` (`calibrate.ready`), `src/ipc/mock.ts`, the
bindings `CalibrateParams.ts`, and P141's report for the error params.

## The defect

A user pressed 보정 계산 and did not know MIMO needs a second press of
'MIMO 계산' in the fifth card at the bottom of the settings. The gallery's
before-compute calibrate scene does not even show MIMO. The maintainer:
MIMO is the most important choice on the before-compute screen and must sit
there as a plain checkbox or switch.

## What changes on screen

1. **MIMO 함께 계산** (`data-ui="calibrate.run.mimo"`, a checkbox or switch,
   your call) sits in the run controls right before 보정 계산 / 다시 보정하기,
   so the choice is beside the button it changes. It reads and writes the
   same `mimo_on` setting as the MIMO card's toggle (one source; both stay in
   sync). One help sentence by effect: 다른 스피커의 저역 출력으로 늦게 남는
   울림을 줄이고, 계산 시간이 늘어납니다.
2. Fewer than three positions: the control is disabled and says why
   (`calibrate.mimo.unavailable` already has the text; reuse or adapt).
3. 보정 계산 sends `calibrate` with `mimo: mimo_on && positions >= 3`. The
   busy line names both while MIMO is included (e.g. '보정과 MIMO를
   계산하는 중'). After the run the verdict and the MIMO card show the MIMO
   result as they already do.
4. A MIMO failure from the combined job (`stage: "mimo"`) shows in the MIMO
   card's failure slot, not as a correction failure; the correction result
   stays visible.
5. The MIMO card keeps its details and its 'MIMO 계산' button, which now
   recomputes MIMO alone after MIMO-only changes (서포트 주파수 상한 and the
   고급 fields). Its label or hint says that; the stale text (`calibrate.mimo.state.stale`)
   tells users to press 보정 계산 when the correction changed and 'MIMO 계산'
   when only MIMO settings changed (pick by which key is stale if the view
   can tell; otherwise one sentence covering both).
6. The plan sentence before compute mentions MIMO when it is included.

Keep the run controls' height steady when the control is disabled or hidden;
follow neighbouring controls for tokens and states.

## Strings, specimens, tests

- New keys in `ko.json` and `en.json` (합니다체, effect labels, no em-dash,
  no particle right after a placeholder). List them; ASTRA reviews the
  catalogs afterwards.
- Gallery: the before-compute scene (`calibrate.ready` in the tour and the
  calibrate specimens) shows the MIMO control, on with three positions and
  disabled with two; the combined running state; a MIMO failure after a
  successful correction.
- Tests: the control toggles `mimo_on`; 보정 계산 sends `mimo: true` only when
  on and three or more positions; disabled with the reason below three; the
  busy text; the failure lands in the MIMO card; the MIMO card toggle and the
  run control stay in sync; the ids exist.
- The mock runs MIMO inside `calibrate` when `mimo` is true (so the tour and
  tests use the real flow) and returns the `stage: "mimo"` failure when asked.

## Files

You may edit: `apps/secs2r/ui/src/screens/calibrate/**`,
`apps/secs2r/ui/src/ipc/mock.ts` (calibrate and MIMO only),
`apps/secs2r/ui/src/i18n/ko.json`, `en.json`, `apps/secs2r/ui/src/gallery/**`,
`src/screens/specimens.ts` for registration.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
```

## Report

Files changed, where the control sits and why, keys with ko and en,
specimens and tour scenes, tests, and the commands' last lines.
