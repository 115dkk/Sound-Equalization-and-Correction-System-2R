# P192 (Opus 5.5): audit #186, the screen side

Tree: `E:\wt\audit-ui`, a git worktree on branch `audit/186-ui`, created by
Claude from the then-current `fix/audit-186` after P183..P191 have been
rebased into it (so the regenerated `bindings/Note.ts`, `bindings/rules.ts`,
the recorded JSON files and the Rust side of every ADR below are already on
the base). Check `git log -1` and `git status` before the first edit; if the
branch is not `audit/186-ui` or the tree is dirty, stop and report. Run
`npm ci` once in `apps/secs2r/ui` (Node 24, npm 11; npm 10 fails peer
resolution). Commit on that branch, one commit per item, in the order below.
Do not push, do not rebase, do not spawn sub-agents, do not touch
`audit-state.md`, do not run cargo.

Allowed files: `apps/secs2r/ui/**` except `src/ipc/bindings/**` and
`src/ipc/recorded/**` (generated and recorded elsewhere; this packet consumes
them), and `features.toml` only where an entry's `note` names a UI test file
that this packet renames or deletes.

Forbidden: everything outside the UI, including `crates/**`, `docs/**`,
`CHANGELOG.md`, catalogs of other apps.

Read first: `CLAUDE.md` (the screen rules: 하십시오체 sentences, labels as noun
phrases, labels that say the effect, one filled primary action per screen,
14 px minimum; new strings get a key in both `ko.json` and `en.json`),
`docs/adr/0003-*.md`, `docs/adr/0014-*.md`, `docs/adr/0042-engine-note-typed-value.md`
(stage 4), `docs/adr/0052-mock-rules-as-recorded-answers.md`,
`apps/secs2r/ui/src/ipc/bindings/Note.ts` and `rules.ts` (generated),
`apps/secs2r/ui/src/ipc/recorded/*.json`, `src/screens/calibrate/text.ts`
(`noticeText` 155, the `split` parsing at 202-206, 242-249, 254-270, 288-289,
364), `src/screens/calibrate/text.test.ts`, `src/ipc/mock.ts` (the copies the
ADR lists: `correctionSettings` 122, `EXPORT_FOLDER_TAG` 143, `rcLimitNote` 153,
`mockRcLines` 183, `mockRcBandsNotes` 191, `rcEditRefusal` 213,
`mockLowProtectNotes` 313, `mockInputHpNotes` 329, `MIMO_ONLY_EDITS` 334,
`editSettings` 642-745, the notes at 1000-1001, `update_*` 1260-1293),
`src/ipc/mock.test.ts`, `src/ipc/mockWiring.test.ts`, `src/ipc/recorded.ts`,
`src/screens/calibrate/{fixtures.ts,specimens.tsx,CalibrateScreen.test.tsx}`,
`src/state/{session.ts,store.ts,react.ts,effects.ts,jobRetention.ts,jobLedger.ts}`,
`src/screens/project/useProjectScreen.ts`, `src/screens/export/useExportScreen.ts`,
`src/screens/calibrate/useCalibrateScreen.ts`, `tsconfig.json`, and the
finding texts in `C:\Users\32170336\AppData\Local\Temp\claude\8a2ed516e7e71310\9cfb4797-a42c-54d9-a68d-f128d938515e\scratchpad\td-table.md`
(sections `### TD-234`, `TD-173`, `TD-174`, `TD-235`, `TD-175`, `TD-176`,
`TD-232`, `TD-237`, `TD-85`, `TD-233`, `TD-236`, `TD-238`).

## Why

The monthly audit (issue #186) found one behaviour defect on the screens
(TD-234), two ADR-driven changes whose Rust side is already on the base (typed
engine notes, ADR 0042; mock rules replaced by generated tables and recorded
answers, ADR 0052), four pure refactors and five config/type/test items. The
categories follow `/115dkk-skills:resolve-audit`: A is test-first at the
named seam (vitest), B changes no observable behaviour and adds no test (the
vitest suite before and after is the gate, with the counts in the report), C
is gated by `tsc`, vitest and the build. Design decisions below are fixed;
copy follows make-interfaces-feel-better and the catalog sentences are
ASTRA's to polish later (write a first version yourself and list every new
key in the report).

## The protocol

- A item: Red (write the vitest test; run it; quote the failing assertion),
  Green (minimal), Tidy (only this item's lines), gate, commit.
- X/B items: run `npm test` before touching code and record the counts; make
  the change; run `npm test` again; the pass count must not drop and no test
  may fail; if one does, stop, undo, and report it as a reclassification.
  Deleting tests that pinned a copy of a service rule (ADR 0052 names them) is
  allowed and expected; say which.
- C item: apply; `npm run typecheck`, `npm test`, `npm run build` pass; commit.
- Gate for every commit: `cd apps/secs2r/ui && npm run typecheck && npm test && npm run build`
  in the foreground.
- Commit with `git add <the files you touched>` and (Korean, 하십시오체, no
  em-dash, no program-as-subject verbs 묻다/적다/넘기다, "추가하다" not "더하다"):

  ```
  fix(audit): A-47 <한국어 한 문장> (#186 TD-234)
  refactor(audit): X-1 <한국어 한 문장> (#186 A1, ADR 0042)
  refactor(audit): B-n <한국어 한 문장> (#186 TD-xxx)
  chore(audit): C-n <한국어 한 문장> (#186 TD-xxx)

  <한두 문장.>

  Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>
  ```

## Items

### A-47 · TD-234 · dialog errors also raised globally

Finding: "프로젝트 대화상자의 호출은 실패를 대화상자에 보이면서 `session.call`도
전역에 알려(억제 목록은 마이크 보정·호스트·구간 편집만), 같은 오류가
대화상자를 닫은 뒤에도 `app.error`로 남습니다." Recommendation: add the
dialog-owned project methods to the suppression list at
`src/state/session.ts:278-290`.

Seam: `session.call` with a failing project-dialog method. Red test
`project_dialog_failures_are_not_published_as_a_global_notice_td234` in
`src/state/session.test.ts` (next to the mic-calibration suppression test if
one exists; else a new file following the hook tests' `createMockService`
pattern with `failNext`): make `project_open` (and `project_save_as`,
`project_new`: the methods `useProjectScreen.ts:19-24` performs) fail; assert
`app.error` (the global notice store) stays `null` while the dialog's `error`
holds the `IpcError`; a failing `calibrate` request still sets `app.error`
(the control case). Green: the suppression list names the project-dialog
methods. Tidy: nothing else.

Commit: `fix(audit): A-47 프로젝트 대화상자의 오류를 전역 알림으로 다시 띄우지 않습니다 (#186 TD-234)`.

### X-1 · ADR 0042 stage 4 (screen side) · typed notes

The base already carries `bindings/Note.ts` (a discriminated union tagged by
`code` with named fields: `dips: Dip[]`, `bands: PairBand[] | Band[]`,
`modes: Mode[]`, `RoleNote<T>` for role-keyed lists) and the summaries'
`notes: Note[]`; service-made notices stay `Notice { code, params }`.
`noticeText` in `src/screens/calibrate/text.ts` takes both: a typed `Note`
reads fields, a `Notice` reads `params` as today. Delete the `split`
parsing and the `valid` guards for `rc_bands`, `dip_limit`, `pair_phase`,
`polarity_reversed`'s role lists and `rolloff` (`:202-206, 242-270, 283-289, 364`);
the catalog keys stay (89 `notice.*` keys unchanged). `text.test.ts`: the
cases that fed mini-format strings become typed-note cases with the same
expected sentences (literal strings stay; inputs change); delete the cases
that asserted the tolerant handling of a malformed string. `fixtures.ts`,
`specimens.tsx`, `mock.ts` and `useCalibrateScreen` consume `Note[]`;
`npm run typecheck` must be clean again (the base may be red here until this
item lands; the report lists the errors you closed).

Commit: `refactor(audit): X-1 화면이 typed Note의 필드를 읽고 문자열 파싱을 지웁니다 (#186 A1, ADR 0042)`.

### X-11 · ADR 0052 · the mock reads tables and recordings

The base already exports in `bindings/rules.ts`: `ENGINE_KEY_FIELDS`,
`MANUAL_KEY_FIELDS`, `MIMO_KEY_FIELDS`, `JOB_GROUPS`, `SCALAR_EDITS`,
`EXPORT_FOLDER_TAGS`, `MIMO_MIN_POSITIONS`, `DESIGN_RATE`, `OUTPUT_RATES`,
`SUB_LIMITS`, `INFRA_DEFAULT_HZ`, `ALLPASS_Q_DEFAULT`, `TRANSITION_FACTORS`,
`AGREEMENT_MIN_POSITIONS`, `REPETITIONS`, `NEW_PROJECT_PRESETS`,
`INPUT_HP_START`; and in `src/ipc/recorded/`: `calibration_settings_edit.json`,
`results.json`, `result_lines.json`, `export.json`, `update_transitions.json`
(each a list of `{ params, data }`). Do, in two commits:

1. `refactor(audit): X-11 …` (TD-174 part): replace the hand copies with the
   generated constants: `screens/calibrate/settingsView.ts:23-30, 34-36, 54, 56`
   (`SUB_LIMITS`, `INFRA_DEFAULT_HZ`, `ALLPASS_Q_DEFAULT`, `TRANSITION_FACTORS`,
   `AGREEMENT_MIN_POSITIONS`), `screens/export/types.ts:62`, the two
   `MIMO_MIN_POSITIONS`, the two `REPETITIONS`, `DESIGN_RATE`/`OUTPUT_RATES`,
   `NEW_PROJECT_PRESETS`, `INPUT_HP_START` (grep each name; delete the local
   declaration, import from `../../ipc/bindings/rules`). No behaviour change.
2. `refactor(audit): X-11 …` (TD-173 + TD-235): `mock.ts` deletes `rcLimitNote`,
   `mockRcBandsNotes`, `mockLowProtectNotes`, `mockInputHpNotes`,
   `rcEditRefusal`, `correctionSettings`, `MIMO_ONLY_EDITS`, `mimoExport`,
   `EXPORT_FOLDER_TAG` and the folder/file-list synthesis (`1102-1134`), the
   output-rate resolution (`1071-1077`), the MIMO precondition (`771-777`),
   the DSP rules (`1202-1222`), the updater branches (`1260-1293`), and the
   always-on notes at `1000-1001`. Replacements per ADR 0052: stale detection
   uses `ENGINE_KEY_FIELDS`/`MIMO_KEY_FIELDS`; job exclusion uses `JOB_GROUPS`;
   scalar edits apply `SCALAR_EDITS` (clamp) or answer the recorded refusal
   envelope from `calibration_settings_edit.json`; a calibration answers the
   recorded `results`/`result_lines` entry whose `engine_key` matches the
   current settings, else the default recording with only `stale` changed;
   export answers `export.json`; updater transitions answer
   `update_transitions.json`. Structural edits (subs, groups, infra, XO map)
   stay as hand code. `screens/calibrate/fixtures.ts` builds its specimens
   from the recorded `results`/`result_lines` (synthetic curves stay);
   `CalibrateScreen.test.tsx:10, 1573` stop importing `rcEditRefusal` and
   `:1201-1210`'s synthetic numbers become the recorded notes' values. Delete
   the tests that pinned the copies (`mock.test.ts:687, 976, 1048, 1129` and
   the `mockWiring.test.ts` cases that compared the mock with itself; list
   each deleted test name). The gallery's review issue will show the specimens
   with recorded notes; mention that in the report.

### X-15 · ADR 0056 · one table of project replacement rules

Read `docs/adr/0056-project-replacement-rules-table.md` in full; it is the
specification. This is a deepening item: write the tests at the new module's
interface first, then move the behaviour behind it, then delete the hand-written
key lists the new tests cover.

1. Red: `apps/secs2r/ui/src/state/projectScope.test.ts` as ADR 0056's `## 시험`
   describes: after `replaceProject(session, "request")` and after
   `replaceProject(session, "external")`, every store named in
   `PROJECT_RESOURCES` is `null`, every field of `ProjectHostState` equals
   `PROJECT_HOST_DEFAULTS` (iterate the table; the literal defaults come from
   the ADR's list), `opened` rose by one, and the `AppHostState` fields
   (`current`, `tab`, `camillaDevices`, `extraRates`) kept their values. The
   external case also asserts `designName === ""` (the drift the ADR names).
2. Green: `state/projectScope.ts` with `ProjectHostState`, `AppHostState`,
   `HostState = ProjectHostState & AppHostState`, `PROJECT_HOST_DEFAULTS`,
   `PROJECT_RESOURCES`, `APP_RESOURCES` as the computed complement, and
   `replaceProject(session, source)`; the effect rule `project.replace`, the
   closure in `session.ts` and `refreshExternal` call it. `external` also
   invalidates the stores that have `invalidate`. What is reloaded afterwards
   stays with `effectsOf(...)` (unchanged).
3. Tidy and delete: `effects.test.ts` and `externalChanges.test.ts` use the
   table instead of their hand-written key lists; the `satisfies` type check
   makes a `HostState` field that belongs to neither type a `tsc` error.

Two commits: `refactor(audit): X-15 프로젝트 교체 규칙을 projectScope 표 하나로 모읍니다 (#186 A15)`
(step 1 and 2 together, since the Red test needs the module to compile) and
`refactor(audit): X-15 교체 시험이 표를 읽고 손으로 적은 키 목록을 지웁니다 (#186 A15)`.
Gate: `npm run typecheck && npm test`.

### B-5 · TD-175 · export screen effect depends on the results object

`src/screens/export/useExportScreen.ts:44-68` and `src/state/effects.ts:38`:
depend on scalars (the stale flag, the count of finished export jobs, the
service address) instead of `results?.calibration`, and run the 3 s
`apply_status` polling only while `host.current === "export"`. Gate: the
export screen tests keep passing; if a test counted `apply_status` calls per
edit, it should now count fewer, and that is a behaviour the test must be
updated to (say so; it is the finding's intent, not a reclassification).

Commit: `refactor(audit): B-5 내보내기 화면의 효과가 결과 객체 대신 스칼라에 의존합니다 (#186 TD-175)`.

### B-6 · TD-176 · `host.set` always allocates

`src/state/session.ts:42-67, 107` and `src/state/react.ts:21-24`: `host.set`
returns early when a shallow comparison finds no change (so `store.ts:10`'s
`Object.is` guard works), and the meter/monitor fields move to their own
store (or `useHost(selector)`), so the calibrate and export hooks do not
re-render on meter events. Gate: vitest counts preserved.

Commit: `refactor(audit): B-6 호스트 상태에 같은 패치가 들어오면 화면을 다시 표시하지 않습니다 (#186 TD-176)`.

### B-28 · TD-232 · helper duplication

Import `isActiveJob` from `state/jobRetention.ts` at the eight inlined sites;
one `MEASURING_KINDS`/monitor-kind list used by `useCalibrateScreen.ts:117`,
`useMeasureScreen.ts:50`, `mock.ts:106, 539` and the tests; one
`internalError()` in `ipc/client.ts` for the six `{ code: "internal", … }`
literals (`useProjectScreen.ts:22` among them); rename the second
`noticeText` (different return type) so one name means one function;
`useNumber` gains `plain`/`signed` and the ad-hoc formatters use it;
`TRAY_HIDDEN` is used or deleted; `retryable` is either rendered by
`ErrorNotice` as a retry action or removed from the screen types (choose:
remove; the service never sets it for screen methods today, say so in the
report if you find otherwise).

Commit: `refactor(audit): B-28 화면의 잡 판정, 오류 리터럴, 숫자 형식을 하나로 모읍니다 (#186 TD-232)`.

### B-29 · TD-237 · curves reloaded in every stage

`src/state/session.ts:138`, `effects.ts:32`, `useCalibrateScreen.ts:114`:
`measuredRoles` comes from `measurement_records` (the records already carry
role and position) instead of the curves; `measurement_curves` is re-read only
while the measure or import stage is visible. Gate: vitest counts preserved.

Commit: `refactor(audit): B-29 곡선은 측정 화면이 보일 때만 다시 읽습니다 (#186 TD-237)`.

### C-13 · TD-85 · stale "service gap" comments

`src/screens/measure/types.ts:93, 97, 99, 141` and `src/screens/connect/types.ts:113, 133, 139`:
replace each "service gap"/"no method yet" comment with the IPC method the
hook actually calls (`measure_configure { repetitions }`, `rew_info`,
`level_stop`, `levels_fill_defaults`; check `useMeasureScreen.ts:30` and
`useConnectScreen.ts:47, 133-137, 161`).

Commit: `chore(audit): C-13 측정과 연결 화면의 낡은 주석을 실제 IPC 메서드 이름으로 고칩니다 (#186 TD-85)`.

### C-21 · TD-233 · unused-locals flags and dead exports

`tsconfig.json`: add `"noUnusedLocals": true, "noUnusedParameters": true`;
fix the four dead locals/imports `tsc` then reports; drop `export` from the
~50 internal-only names (`mock.ts`'s `mockRcBandsNotes`, `measurementReference`
etc.; after X-11 many are gone already); delete the unused `.export-note`
CSS rule and any specimen no gallery page renders. No ESLint (maintainer
decision, audit question 14).

Commit: `chore(audit): C-21 쓰지 않는 지역 변수와 export를 컴파일러가 막습니다 (#186 TD-233)`.

### C-22 · TD-235 · specimens for the new notice states

After X-11 the mock no longer attaches `polarity_reversed`/`pair_phase` to
every calibration. Add one static specimen each in
`screens/calibrate/specimens.tsx` for the polarity warning (with its flip
button) and the dip-limit line, built from recorded notes, and one tour scene
(`src/gallery/tour/scenes.ts:528-537`) that shows a polarity warning only in
the scene that records it. Gate: the gallery build (`npm run build`) and the
specimen tests.

Commit: `chore(audit): C-22 극성 경고와 딥 보정 제한의 견본을 둡니다 (#186 TD-235)`.

### C-23 · TD-236 · `EffectContext.source` typed

`src/state/effects.ts:52-57, 75, 108` and `session.ts:258, 264`: type
`source` as a union keyed by method (`ResponseOf<M>`) so the rules and
`call()` stop casting (`as MeasureConfigResult`, `as JobStarted`,
`as CalibrationSettings`). Gate: `tsc` clean, no behaviour change.

Commit: `chore(audit): C-23 효과 규칙의 응답에 메서드별 타입을 붙입니다 (#186 TD-236)`.

### C-24 · TD-238 · comments, the sentence-style title, CSP reasons

`screens/calibrate/Settings.tsx:5` and `types.ts:55`: the comments say
`calibration_settings_edit` (what `session.ts:424` calls). `system.notices.title`
becomes a noun phrase in both catalogs: ko `"고칠 곳 {count}곳"`, en
`"{count} things to fix"` (labels and titles are noun phrases, ADR 0003; the
register test must keep passing). `apps/secs2r/tauri.conf.json` is not yours:
report the three CSP entries (`style-src 'unsafe-inline'` for React style
props and uPlot, `img-src blob:` for plot exports, `font-src data:` for the
subset fonts) so Claude writes the reasons there.

Commit: `chore(audit): C-24 보정 화면의 주석과 알림 제목을 바로잡습니다 (#186 TD-238)`.

## Verification (foreground; do not end the turn before they finish)

```
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
git log --oneline fix/audit-186..audit/186-ui
```

`npm test` runs 1,300+ tests in about 90 s; one or two tutorial tests time
out under load on this machine (the `quests.test.tsx` file passes alone); if
a file fails only in the full run, re-run that file alone and report both
results.

## Report

Per item: the commit hash; for A-47 the Red test and its failing assertion;
for X/B items the vitest counts before and after and the deleted test names;
for C items the gate outputs. Then: every new catalog key with its ko/en first
version (ASTRA polishes them), the CSP reasons for Claude, anything
reclassified or blocked with why, and the final `git log --oneline` of the
branch.
