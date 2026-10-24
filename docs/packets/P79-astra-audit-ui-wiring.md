# P79 (ASTRA): audit #92 — screen wiring for the High findings (U1a)

Worktree: `/home/user/wt-ui` (branch `wip/u1a`, created by Claude from a base
that already holds P71/P72: the service applies XO recommendations,
`CalibrationSummary.cached` and `ExportBlock` `stale_cache` exist in
`apps/secs2r/ui/src/ipc/bindings/`). Work only there. Do not run cargo (the
main tree is building). Do not push, do not spawn sub-agents. Commit once per
finding (subjects below); Claude reviews, re-runs the gates and re-authors the
commits.

Scope rule: you change call code and state only — `state/**`, `screens/*/use*Screen.ts`,
`screens/*/types.ts`, `ipc/mock.ts`, `ipc/fixtures.ts` and their tests. Do
not change markup (`*.tsx` component bodies), CSS or layout; where a screen
needs a new action or view field, add it to the view/actions types and the
hook, and leave the drawing to packet P80 (Opus). New text keys: add the key
to `i18n/ko.json` and `i18n/en.json` with natural text in 합니다체 (ko) and
plain English; P80 may reuse them.

Read first: `CLAUDE.md` (screen rules), `docs/ARCHITECTURE.md` §7,
`docs/DESIGN.md` §7, `TECH_DEBT_AUDIT.md` (rows TD-09, TD-27, TD-28, TD-29,
TD-30), `apps/secs2r/ui/src/state/{session.ts,store.ts,triggers.ts,jobTracker.ts}`,
`App.tsx`, `screens/calibrate/{useCalibrateScreen.ts,types.ts,ResultGraph.tsx}`,
`screens/export/{useExportScreen.ts,types.ts}`, `ipc/mock.ts`, and the tests
next to them. Upstream XO behaviour: `src/secs/wizard/step_calibration.py:1883-1890`.

Method: test first (vitest, the existing test helpers: mock service, session,
`renderHook`). Red (confirm the reason), minimum green, then `npm test`.

## Findings, in order

1. **TD-30** — `state/session.ts:109`: only job-start replies are fenced by
   the project epoch; a late `measure_configure`/`measure_config_get`/
   `level_apply` reply from the previous project overwrites
   `measurementSettings` and `host.path` after a project switch. Capture
   `tracker.epoch` when the call starts and skip the response-owned store
   writes and host changes when it changed (still return/throw the result to
   the caller). Test: delayed mock reply for `measure_config_get` resolved
   after `project_open` → the new project's settings remain. Commit:
   `fix(audit): A-30 프로젝트를 바꾼 뒤 도착한 이전 응답을 버립니다 (#92 TD-30)`
2. **TD-27** (screen part) — the service now stores `xo`/`xo_rec` when an XO
   recommendation finishes (P71). Upstream then recomputes the calibration.
   In `completeJob`, after a succeeded `recommend_xo`: reload the calibration
   settings (add `recommend_xo` to the finished-job triggers that reload
   `calibrationSettings` if it is not there) and start a full calibration
   with the current settings, exactly as the calibrate action does. Make the
   mock match the service: it must not edit settings in the recommendation
   request (`mock.ts:523`) but on completion, like the service. Test (session
   + mock): recommend → finished → settings show `xo_rec`, and a `calibrate`
   request follows. Commit:
   `fix(audit): A-27 XO 추천이 끝나면 설정을 다시 읽고 보정을 다시 계산합니다 (#92 TD-27)`
3. **TD-28** — `ResultGraph.tsx:294` falls back to the first XO group when
   `host.xoGroup` is null, while the service draws the selected channel's
   group (`crates/secs-service/src/curves.rs:283-290`); dragging edits the
   wrong group. Move the choice into state: when the XO tab opens or the
   channel changes, set `host.xoGroup` to the group of the selected channel
   (use the same group mapping the screen already uses for labels); the
   view exposes the resolved group, and the component reads that instead of
   its own fallback (that one-line read change in `ResultGraph.tsx` is
   allowed). Test: select C on SPL → open XO → the edit sent on drag targets
   센터. Commit:
   `fix(audit): A-28 XO 탭의 그룹을 선택한 채널에 맞춥니다 (#92 TD-28)`
4. **TD-29** (state part) — `App.tsx:210`: if `app_settings_get` fails once,
   `Preferences` returns `null` forever. Expose the load state of
   `appSettings` (loading / failed with the typed error / ready) and a
   `retry()` that calls `appSettings.load()` again, through a small hook
   (e.g. `useAppSettingsLoad()` in `state/`). Do not draw anything; P80
   renders the failure screen. Test: first call fails, `retry()` → ready.
   Commit: `fix(audit): A-29 앱 설정 조회 실패를 상태로 드러내고 다시 시도할 수 있게 합니다 (#92 TD-29)`
5. **TD-09** (screen part) — mdat formats are blocked when the result was
   restored from the cache and is stale (`exportBlock` returns
   `stale_cache`). Add an export-screen action `recalibrate()` that starts a
   full calibration with the current settings (the same call the calibrate
   screen makes) and, in the view, `recalibrating: boolean` while that job
   runs. Test (mock): a reopened stale project → mdat kinds blocked with
   `stale_cache`; `recalibrate()` sends `calibrate`; after it finishes the
   block is gone. Commit:
   `fix(audit): A-11 낡은 캐시 결과의 mdat 막힘에서 바로 다시 보정할 수 있게 합니다 (#92 TD-09)`

## Gates (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui && PATH=/opt/node24/bin:$PATH npm ci && PATH=/opt/node24/bin:$PATH npm run typecheck && PATH=/opt/node24/bin:$PATH npm test
```

(`npm ci` once at the start.) Run typecheck and the tests after each finding.

## Report (final answer)

Per finding: commit hash, files, test names, red failure message, green
result, new view/action fields and text keys for P80. Then the last gate
output tail and anything left undone.
