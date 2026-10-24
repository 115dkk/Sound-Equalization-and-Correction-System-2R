# P82 (ASTRA): audit #92 — screen wiring, mock and catalog Medium/Low findings (U2a)

Worktree: `/home/user/wt-ui` (branch `wip/u2a`, created by Claude). Work only
there. Do not run cargo, do not push, do not spawn sub-agents. Commit once per
finding (subjects below); Claude reviews, re-runs the gates and re-authors the
commits.

Scope rule (as P79): call code, state, mock, fixtures, tests, catalogs and
comments only — no markup, CSS or layout. Where a screen needs a new view
field or action, add it to the types and hook; P83 (Opus) draws it.

Read first: `CLAUDE.md`, `TECH_DEBT_AUDIT.md` rows TD-66, TD-67, TD-69,
TD-71, TD-83, TD-84, TD-85, TD-86, TD-87, TD-101, TD-102, and the files
they cite.

Method: A items test first (red with the reason, minimum green); B (TD-84)
keeps every test passing with no new tests and reports before/after counts;
C items are gated by typecheck, tests and the i18n tests.

## Findings, in order

1. **TD-66** (A) — `screens/measure/format.ts:63` treats only `""` as a
   direct measurement; the service sends `"measured"`
   (`crates/secs-service/src/measurement.rs:417-424`). Normalise `"measured"`
   (and `""` for older data) to direct; make the mock and fixtures send
   `"measured"` like the service. Test: sources `["measured", "file.mdat"]` →
   the summary names one direct measurement and one file. Commit:
   `fix(audit): A-61 직접 측정한 출처를 가져온 파일로 보이지 않습니다 (#92 TD-66)`
2. **TD-67** (A, state) — `ResultGraph.tsx:307`: a failed target save leaves
   the local draft on the graph; `setTargetPoints` returns nothing. Make the
   target and XO edit actions return `Promise<boolean>` (true when the
   service accepted it); expose in the view whether an edit is pending or
   failed (`targetEdit: "idle" | "saving" | "failed"`, same for XO). The
   component change (reset the draft on failure, show the state) is P83's;
   you may change the single line in `ResultGraph.tsx` that awaits the
   result. Test: mock rejects `target_set` → action resolves false and the
   view says failed. Commit:
   `fix(audit): A-62 타겟과 XO 저장 실패를 결과로 돌려줍니다 (#92 TD-67)`
3. **TD-69** (A) — `state/triggers.ts:27,32`: `project_save` (Save As) is
   missing from the results and export-folder reload triggers. Add it. Test:
   session + mock: save to a new path → `results` and the export folder are
   reloaded (the folder shows the new path). Commit:
   `fix(audit): A-63 다른 이름으로 저장한 뒤 결과와 내보내기 폴더를 다시 읽습니다 (#92 TD-69)`
4. **TD-71** (A, test debt) — the mock diverges from the service and tests pin
   the divergence: (1) it rejects MIMO Equalizer APO exports although F12
   allows them (`mock.ts:550`, `ipc/mock.test.ts:45-56`); (2) a subs-only
   calibration replaces the full result (`mock.ts:514`; the service sets only
   `state.subs`, `crates/secs-service/src/calibration.rs:123-127`); (3) XO
   fixtures use `front`/`center` keys (`ipc/fixtures.ts:309`, read at
   `mock.ts:340`, `:374`) while the contract uses the Korean group keys
   (`crates/secs-types/src/role.rs:38`). Fix all three in the mock and change
   the tests to expect the service's behaviour. Commit:
   `fix(audit): A-65 모의 서비스의 어긋남 셋을 서비스 동작에 맞춥니다 (#92 TD-71)`
5. **TD-83** (A) — `screens/connect/useConnectScreen.ts:61`: the
   `level_play_end` subscription and 8 s timer created before `level_play`
   are not released when the request fails. Keep the release function and
   call it in `finally` on failure/cancel/end. Test: failing `level_play` →
   no subscription and no pending timer afterwards (fake timers). Commit:
   `fix(audit): A-66 레벨 재생 요청이 실패하면 기다림을 바로 정리합니다 (#92 TD-83)`
6. **TD-84** (B) — delete unused code: `settingsView.ts`
   `SUB_FILTER_DEFAULT_HZ`, `DEFAULT_XO_HZ`, `FILTER_KEYS`, `bundleMembers`;
   `MeasureScreen.tsx:563` `projectFileName` (a one-line removal is allowed);
   `HOST_TRIGGERS.runs`/`.selected` (`state/triggers.ts:40-41`);
   `session.waitForJob` (`session.ts:177-192`) and the tests that only test
   it; `useProjectScreen.restartTutorial` (`:89-92`); the `jobStore.isActive`
   alias. Then run `npx -y knip@5 --config <cfg>` with entries `src/main.tsx`,
   `src/gallery/main.tsx`, `scripts/*.mjs`, `src/**/*.test.{ts,tsx}` and
   remove the `export` keyword from symbols used only in their own file;
   delete symbols used nowhere. Do not touch `src/ipc/bindings/**`. Report
   the test counts before and after (must be equal except for removed
   `waitForJob` tests, which you list). Commit:
   `refactor(audit): B-6 쓰이지 않는 화면 코드와 내보내기를 지웁니다 (#92 TD-84)`
7. **TD-85** (C) — stale "service gap" comments:
   `screens/measure/types.ts:31`, `screens/calibrate/types.ts:81-88`,
   `screens/export/types.ts:35-39`. Remove the resolved ones and name the
   response fields that now carry the data; make `measure/types.ts` re-export
   the generated `PositionRecord` binding instead of a hand copy if they are
   identical. Commit:
   `chore(audit): C-10 해소된 service gap 주석을 지웁니다 (#92 TD-85)`
8. **TD-86** (C) — catalogs: (1) the loopback troubleshooting text
   (`ko.json:1175` and its `en.json` twin) tells the user to pick a loopback
   input channel in 연결·점검, but no such control exists — read
   `screens/connect/ConnectScreen.tsx` and `docs/DEVIATIONS.md` (R7: REW
   channels are read-only) and rewrite the text to point at the place it is
   actually set (REW's own settings) in 합니다체, no prose pointing to other
   app steps (ARCHITECTURE §7 P57); (2) `ko.json:691` and `en.json` use
   U+2011 in `‑api`; use ASCII `-api`, and add a test to `i18n.test.ts` that
   no catalog string contains U+2010–U+2015 next to a Latin letter. Commit:
   `chore(audit): C-11 루프백 안내와 실행 옵션의 하이픈을 고칩니다 (#92 TD-86)`
9. **TD-87** (C) — `scripts/build-fonts.py:15`: record the tool versions
   (`fonttools[woff]==<version>`, `brotli==<version>`; pick the versions that
   reproduce the shipped subsets — check `src/fonts/README.md` and the font
   manifest for hints; if unknown, state the versions you verified by
   regenerating into a temp dir and comparing bytes, without replacing the
   shipped fonts) and the Orbit source revision and SHA-256 in
   `src/fonts/README.md`. Commit:
   `chore(audit): C-12 글꼴 생성 도구 판과 원본 해시를 적습니다 (#92 TD-87)`
10. **TD-101** (A) — `screens/project/useProjectScreen.ts:20` decides
    whether to ask about unsaved changes from `project.dirty`, which changes
    only after an edit's response and the follow-up reload. Opening or
    closing right after an edit can skip the dialog. Track pending project
    edit requests in the state layer and have the guard await them (or
    treat a pending edit as dirty) before deciding. Test: a delayed edit
    response, then open → the confirm dialog is requested. Commit:
    `fix(audit): A-90 편집 응답 전에 열거나 닫아도 저장 안 한 변경을 묻습니다 (#92 TD-101)`
11. **TD-102** (A) — `state/session.ts:233-242` waits for the monitor's
    completion event with no limit; a lost event blocks the monitor queue
    and project work. Add a timeout (5 s), after which the session reads
    `jobs` and reconciles. Test with fake timers: no completion event → the
    wait ends after the timeout and the next queued action runs. Commit:
    `fix(audit): A-91 모니터 종료 기다림에 시간 제한을 둡니다 (#92 TD-102)`

## Gates (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui && PATH=/opt/node24/bin:$PATH npm run typecheck && PATH=/opt/node24/bin:$PATH npm test
```

after each finding, and `npm run build` at the end.

## Report (final answer)

Per finding: commit hash, files, test names, red failure (A), counts (B),
new view/action fields for P83. Then the last gate output tail and anything
left undone.
