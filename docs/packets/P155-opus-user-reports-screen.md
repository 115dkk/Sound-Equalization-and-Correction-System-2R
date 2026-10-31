# P155 (Opus 5.5): log folder, stale recommendation, 'XO 추천 포함' and '낡은 결과 자동 갱신' on screen

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`,
branch `claude/user-reports`. Do not commit, do not push. Daybreak changes
Rust crates at the same time (P154); ASTRA wires state after you (P156).
Touch only `apps/secs2r/ui/**`, and in it only views, specimens, fixtures and
the catalogs; leave `src/state/**` and `src/ipc/**` to ASTRA except where a
view needs a new action name (declare it in the actions type with a no-op or
an existing call and say so in the report).

Read first: `CLAUDE.md` (screen rules: 합니다체 sentences, noun-phrase
labels, labels that say the effect, 14 px minimum, new text as keys in both
`ko` and `en`), `docs/adr/0027-app-log.md`, `docs/adr/0028-xo-rec-include.md`,
`docs/adr/0029-auto-refresh.md` (sections on the screen), the view that shows
the app-level settings (update policy, tray icon, local API; start from
`apps/secs2r/ui/src/App.tsx`), the crossover card and the 'MIMO 함께 계산'
toggles in `apps/secs2r/ui/src/screens/calibrate/{Settings.tsx,ComputeControl.tsx,Results.tsx}`,
`HelpTip`, `Toggle`, and the gallery specimens next to each screen.

Bindings after P154: `BootstrapInfo.log_dir?: string`,
`ResultsSummary.xo_rec_stale?: boolean`, `CalibrationSettings.xo_rec_include?: boolean`
(missing means on), `SettingEdit` kind `xo_rec_include`, `AppSettings.auto_refresh: boolean`.

## Design (fixed)

1. **Log folder** (app settings view): a quiet button '로그 폴더 열기'
   (key `app.log.open`, `data-ui="app.log.open"`) that sends `open_folder`
   with `bootstrap.log_dir`, and one sentence (key `app.log.help`): the log
   stays on this computer, and `secs2r.log` in that folder helps when
   reporting a problem. Hidden when `log_dir` is absent.
2. **'낡은 결과 자동 갱신'** (app settings view): a `Toggle` (key
   `app.auto_refresh`, `data-ui="app.auto_refresh"`) with a `HelpTip` (i)
   whose text (key `app.auto_refresh.help`) says what it recomputes (the same
   as '보정 계산': XO recommendation when 'XO 추천 포함' is on, then the
   correction, and MIMO when 'MIMO 함께 계산' is on), that it waits 1.5 s after
   the last change and uses at most half of the CPU, and ends with
   '주의: 설정을 바꿀 때마다 다시 계산하므로 팬 소음이 커지기도 합니다.' Default off.
3. **'XO 추천 포함'**: a `Toggle` (key `calibrate.xo_rec_include`,
   `data-ui="calibrate.xo_rec_include"`, help key
   `calibrate.xo_rec_include.help`) beside each 'MIMO 함께 계산' toggle that
   sits next to the calibrate buttons, and in the crossover card. Checked when
   `settings.xo_rec_include !== false`; sends `{ kind: "xo_rec_include", on }`
   through `actions.changeSetting`. Hidden when the project has no subs. A
   normal hint (key `calibrate.xo_rec_include.off_manual`) under it when
   `view.xoRecTurnedOffByEdit` (declare it as an optional boolean on the view
   type; ASTRA fills it) is true.
4. **Stale recommendation**: when `results.xo_rec_stale` is true, the
   crossover card keeps the recommended values and adds a warning hint (key
   `calibrate.xo.rec_stale`, `data-ui="calibrate.xo.rec_stale"`) that the
   correction settings or measurements changed since the recommendation and
   '추천 계산' should be run again.
5. **MIMO hint placement (user report, 2026-10-31).** In `Results.tsx`
   `MimoChoice`, the hint 'calibrate.run.mimo.unavailable' ('측정 위치 3곳 이상
   필요') renders at the end of the toggle's line, right before [보정 계산], so
   it reads as a condition of the calibration. Put it on its own line directly
   under the 'MIMO 함께 계산' toggle, inside the same `calibrate-run__mimo`
   group, so it belongs to the toggle; keep the row's height stable (reserve the
   line or align the buttons so they do not jump), and do the same for the new
   'XO 추천 포함' toggle's hints. Update the comment above `MimoChoice` and the
   run-row specimens (fewer than three positions) to show the new placement.
6. **Specimens**: app settings with the log button and the auto-refresh
   toggle (its help open), the crossover card with the stale hint, the result
   card with both toggles, and the manual-edit hint.
7. **Catalog**: add every key to `ko.json` and `en.json` with a first draft;
   ASTRA reviews the text afterwards. Keep keys and placeholders stable.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui && npm ci && npm run typecheck && npm test && npm run build
```

Node 24 (npm 11). Do not edit `src/ipc/recorded/**` or the bindings. If the
bindings are not regenerated yet when you start, wait for them (check
`git status` for `src/ipc/bindings/`) or report the type errors that remain.

## Report

Files changed, where each control sits, keys with draft ko/en, specimens
added, the action names you declared for ASTRA, and the last lines of each
command.
