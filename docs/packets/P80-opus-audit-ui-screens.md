# P80 (Opus 5.5): audit #92 — screens for the High findings (U1b)

Worktree: `/home/user/wt-ui` (branch `wip/u1b`, created by Claude after P79's
commits). Work only there. Do not run cargo, do not push, do not spawn
sub-agents. Commit once per finding (subjects below); Claude reviews, re-runs
the gates and re-authors the commits.

Read first: `CLAUDE.md` (the screen rules: 합니다체 sentences, noun-phrase
states and labels, labels that name the effect, one filled default action
per screen, UI text ≥ 14px; new copy = a key in both `ko.json` and
`en.json`), `docs/DESIGN.md`, `docs/UI-TERMS.md`, `docs/skins/README.md` and
ADR 0010 (three skins share one DOM — anything you add must look right in
default, Hardware Rack and Signal Matrix), `docs/ARCHITECTURE.md` §7 (P55,
P57: state + an action that fixes it, no prose pointing to other steps),
`TECH_DEBT_AUDIT.md` rows TD-09, TD-29, TD-31, and P79's report (the view
fields and actions it added). Load the `anthropic-skills:make-interfaces-feel-better`
skill before drawing.

Design tokens: `apps/secs2r/ui/src/styles/tokens.css` (and the skin files in
`styles/skins/`); components in `apps/secs2r/ui/src/ui/` (`Button`,
`Dialog`, notices). Follow the neighbours named below; do not invent new
tokens unless no existing one fits (then define it in `tokens.css` and each
skin).

## Findings, in order

1. **TD-09** — Export screen, mdat formats blocked with `stale_cache`
   (`screens/export/ExportScreen.tsx`, the existing `BLOCK_TEXT` path that
   renders `export.blocked.filter_text.*`). Render the block for staged mdat
   and before/after mdat with exactly the text of `export.blocked.mdat.stale_cache`
   ("구 버전에서 측정한 값입니다. mdat으로 내보내려면 다시 보정해야 합니다.") and a
   button labelled 재보정 (key `export.blocked.mdat.recalibrate`, en
   "Recalibrate") wired to P79's `recalibrate()` action, disabled with a
   busy state while `recalibrating`. Place it where the existing block
   message sits; the button is secondary unless it is the only enabled
   action on that card. Test: render with a stale cached result → text and
   button present; click → action called. Commit:
   `fix(audit): A-11 낡은 캐시 결과의 mdat 막힘에 재보정 버튼을 보입니다 (#92 TD-09)`
2. **TD-29** — App start, settings failed to load (`App.tsx` `Preferences`
   returns `null` today). Using P79's load-state hook, render outside
   `Shell`: a loading state (nothing heavier than today's blank frame), and
   on failure a small full-window panel with a title (key
   `app.settings_failed.title`), one sentence with the typed error's text
   (existing error-to-text helper in `app/jobs/jobText.ts` or the toast's),
   and a filled 다시 시도 button (key `app.settings_failed.retry`). Follow
   the visual language of the existing first-run / dialog surfaces. Test:
   failing mock → panel; retry → app renders. Commit:
   `fix(audit): A-29 앱 설정을 읽지 못하면 다시 시도 화면을 보입니다 (#92 TD-29)`
3. **TD-31** — `screens/system/SystemScreen.tsx:344` and `:677` build one
   option per output number up to the largest `out` in the project
   (`Array.from`). Bound the generated range by the device's channel count
   when known, else by the service's bound (P74: outputs are `1..=256`);
   a stored value outside the range is shown as one extra option, not a
   range up to it. Test: a project with `out = 1_000_000_000` renders at
   most the bound plus one options per select. Commit:
   `fix(audit): A-31 출력 번호 선택지를 장치와 상한으로 제한합니다 (#92 TD-31)`

## Gates (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui && PATH=/opt/node24/bin:$PATH npm run typecheck && PATH=/opt/node24/bin:$PATH npm test && PATH=/opt/node24/bin:$PATH npm run build
```

## Report (final answer)

Per finding: commit hash, files, test names, red failure, green result, new
keys (ko and en text). Then the last gate output tail and anything left
undone.
