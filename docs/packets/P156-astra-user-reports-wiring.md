# P156 (ASTRA): wire 'XO 추천 포함' and '낡은 결과 자동 갱신' into the screen state (ADR 0028, 0029)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`,
branch `claude/user-reports`. Do not commit, do not push. Runs after P154
(Daybreak, Rust and bindings) and P155 (Opus, views) are done.

Read first: `CLAUDE.md`, `docs/adr/0028-xo-rec-include.md`,
`docs/adr/0029-auto-refresh.md`, `apps/secs2r/ui/src/state/effects.ts` (the
`xo.recalibrate` rule and the reload rules), `src/state/session.ts`,
`src/state/jobLedger.ts`, the calibrate actions (find `recommendXo` and the
action behind '보정 계산' / '다시 보정하기'), `src/ipc/mock.ts` and its tests,
and the reports of P154 and P155 (the coordinator gives you the action names
and flags P155 declared).

## What to build (fixed)

1. **'XO 추천 포함' routing.** The '보정 계산' and '다시 보정하기' actions:
   when the project has subs and `settings.xo_rec_include !== false`, send
   `recommend_xo` instead of `calibrate`; the existing `xo.recalibrate` effect
   then starts `calibrate` with the same MIMO choice. Keep 'XO 추천 계산'
   (recommend only, then recalibrate as today). Set
   `view.xoRecTurnedOffByEdit` to true when a settings response shows that a
   manual XO edit turned `xo_rec_include` off, false when the user turns it
   on again or the project changes.
2. **Auto refresh scheduler.** When `appSettings.auto_refresh` is true and the
   calibration result exists and is stale (`results.calibration.stale`), and no
   job is running, start the same flow as '보정 계산' 1.5 s after the last
   settings, target or measurement mutation, with `background: true` on the
   request (and on the `calibrate` that `xo.recalibrate` starts after a
   background recommendation). If a mutation arrives while an auto-refresh job
   runs, cancel that job (`job_cancel`) and schedule again; never cancel a job
   the user started. No auto refresh when there is no result yet, during
   measurement, or while a dialog asks to save. The jobs ledger must show these
   jobs like others (check that progress and failures still render).
3. **Mock.** `mock.ts` accepts `background` on both requests, applies
   `xo_rec_include`, turns it off on a manual XO edit, returns `log_dir`, and
   sets `xo_rec_stale` after a settings edit that follows a recommendation.
4. **Tests** (vitest): routing with and without subs and with the option off;
   the scheduler debounces several quick edits into one background run, cancels
   its own run on a new edit and restarts, does not cancel a user job, and does
   nothing when the setting is off.
5. **Catalog review.** Rewrite the draft text of the new keys P155 added
   (`app.log.*`, `app.auto_refresh*`, `calibrate.xo_rec_include*`,
   `calibrate.xo.rec_stale`) for natural 합니다체 Korean and plain English,
   following `C:\Users\32170336\.claude\CLAUDE.md` (no em-dash, no physical
   metaphors, 작은따옴표 for terms). Keep the sentence
   '주의: 설정을 바꿀 때마다 다시 계산하므로 팬 소음이 커지기도 합니다.' in
   meaning.

## You may edit

`apps/secs2r/ui/src/state/**`, `apps/secs2r/ui/src/ipc/mock.ts` and its tests,
the calibrate actions file(s), `apps/secs2r/ui/src/i18n/{ko,en}.json` (only the
keys above). Not views, styles or markup.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui && npm run typecheck && npm test && npm run build
```

## Report

The routing, the scheduler (timers, cancellation rules), mock changes, tests
added, final catalog text per key (ko and en), and the last lines of each
command.
