# P204 (Opus 5.5): audit #186, the screen side of ADR 0052, the settings notice and the REW path field

Worktree `E:\wt\audit-ui`, branch `audit/186-ui-x11` (Claude creates it from
`fix/audit-186` after P202 has landed there, so the generated inputs below are
in the tree; `git status` is clean when you start). Work only in
`apps/secs2r/ui`. `node_modules` is installed there (Node 24, npm 11); run
`npm ci` only if `npm run typecheck` fails to find a package. No cargo
commands: the bindings and recorded files are inputs, never regenerate or
edit them. One commit per step with the title given, trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push, do
not rebase, do not touch ADRs, `docs/**`, `CHANGELOG.md`, `audit-state.md`,
Rust files, `apps/secs2r/ui/src/ipc/bindings/**` or
`apps/secs2r/ui/src/ipc/recorded/**`.

Read first: `CLAUDE.md` (the screen rules paragraph), `docs/adr/0052-mock-rules-as-recorded-answers.md`
(the specification of steps 1 to 3 here), `docs/adr/0014-generated-service-rules-for-the-screen.md`,
`docs/adr/0051-one-app-settings-store.md` (the settings notice), and the
make-interfaces-feel-better rules the repository follows (sentences in
하십시오체, labels as noun phrases, one filled default action per screen,
14 px minimum).

## Inputs that P202 produced (read them, do not retype their values)

- `apps/secs2r/ui/src/ipc/bindings/rules.ts`: the generated tables ADR 0052
  names (`ENGINE_KEY_FIELDS`, `MANUAL_KEY_FIELDS`, `MIMO_KEY_FIELDS`,
  `JOB_GROUPS`, `SCALAR_EDITS`, `EXPORT_FOLDER_TAGS`, `MIMO_MIN_POSITIONS`,
  `DESIGN_RATE`, `OUTPUT_RATES`, `SUB_LIMITS`, `INFRA_DEFAULT_HZ`,
  `ALLPASS_Q_DEFAULT`, `TRANSITION_FACTORS`, `AGREEMENT_MIN_POSITIONS`,
  `REPETITIONS`, `NEW_PROJECT_PRESETS`, `INPUT_HP_START`). If a name differs
  in the file, the file wins; list the difference in the report.
- `apps/secs2r/ui/src/ipc/recorded/`: `calibration_settings_edit.json`,
  `results.json`, `result_lines.json`, `export.json` (new) next to the
  existing `mode_spec.json`, `config_apply_preset.json`,
  `target_apply_preset.json`, `calibration_settings_get.json`,
  `update_transitions.json`. Each is a list of `{ params, data }`;
  `src/ipc/recorded.ts` already loads such files and answers by `params`.
- `apps/secs2r/ui/src/ipc/bindings/BootstrapInfo.ts`: `settings_notice?: Notice`.
- `apps/secs2r/ui/src/ipc/bindings/MeasureConfigureParams.ts` and
  `MeasureConfigResult.ts`: `rew_executable?: string`.

## Steps, in this order

### 1. The mock answers from records and tables (ADR 0052 step 3)

`src/ipc/mock.ts`. Keep what ADR 0014 calls the contract: envelopes, the job
scheduler and progress events, the exclusive job groups (now decided by
`JOB_GROUPS`), failure injection, undo, the open project's state, the dialog
queue, and the lookup that answers from a recording. Replace the copies ADR
0052 lists with lookups:

- `calibration_settings_edit`: scalar edits apply through `SCALAR_EDITS`
  (clamp or refuse per the table); a refusal returns the recorded refusal
  envelope for that edit from `calibration_settings_edit.json`. Structural
  edits (sub entries, bundle arrangement, infra, XO map) stay hand-written:
  they move state, they do not compute rules.
- `results` and `result_lines`: answer the recorded entry whose `engine_key`
  matches the current settings; a combination that was not recorded answers
  the default-settings record with only `stale` changed, as the ADR says.
- `export`: folder name, file list, output rate and notes from `export.json`
  by `kind`, `design_name`, `extra_rates`.
- Staleness: `correctionSettings` and `MIMO_ONLY_EDITS` become comparisons
  over `ENGINE_KEY_FIELDS` / `MANUAL_KEY_FIELDS` / `MIMO_KEY_FIELDS`.
- Updater transitions: `update_transitions.json` (already present) replaces
  the copied table.
- Delete: `rcLimitNote`, `mockRcBandsNotes`, `mockLowProtectNotes`,
  `mockInputHpNotes`, `rcEditRefusal`, `correctionSettings`, `mimoExport`,
  the folder-name and file-list generation, the output-rate resolution, the
  MIMO precondition check, the DSP edit rules and the updater transition
  table. Nothing in `src/screens` may import a deleted name afterwards.

Commit: `refactor(audit): X-11 모의 서비스가 규칙 사본 대신 기록된 응답과 생성 표를 씁니다 (#186 A11 TD-173 TD-174)`

### 2. Specimens from the records (ADR 0052 step 3, TD-235)

`src/screens/calibrate/fixtures.ts` builds its result specimens from the
recorded `results` and `result_lines` (the default record and the recorded
variants: the three `rc_hi_basis`, the `rc_hi_transition` values, manual
limit, low protect, input high-pass, extra bands) and keeps only the
synthetic curves it draws itself. `specimens.tsx` and the gallery pick from
those. The literal frequencies `CalibrateScreen.test.tsx` asserts for RC
notes come from the recorded notes. The polarity warning appears only in the
specimens whose record carries it.

Commit: `refactor(audit): X-11 보정 견본이 기록된 결과에서 나옵니다 (#186 A11 TD-235)`

### 3. The tests that fixed the copies go (ADR 0052 step 4)

Delete the tests in `src/ipc/mock.test.ts` and `src/ipc/mockWiring.test.ts`
that asserted a copied rule against itself (ADR 0052 names the lines as of
2026-11-06: `mock.test.ts` around 687, 976, 1048, 1129 and most of
`mockWiring.test.ts`); keep the tests of the contract (envelopes, job groups
from `JOB_GROUPS`, failure injection, undo, project state, recorded lookup).
Screen tests assert only what the mock answers from a record. Update the
`ui.screens` note in `features.toml` with one sentence saying the mock
answers from records and generated tables (ADR 0052); nothing else in that
file.

Commit: `test(audit): X-11 사본을 정답으로 삼던 화면 시험을 지웁니다 (#186 A11)`

### 4. The settings notice from the bootstrap (ADR 0051 step 4)

`src/app/useFrameModel.ts` reads `session.stores.bootstrap` already (see
`src/app/appOptions.ts` for the subscription). Carry
`bootstrap.settings_notice` into the frame model next to
`project.warnings`, and show it in `src/app/ProjectStatus.tsx` in the same
place and shape as `ProjectWarnings` (`ui-error ui-error--warning`, a
title, one sentence, 닫기 that hides it for the session). It is not tied to a
project: it shows with or without one. Text keys:
`notice.app_settings_unreadable` (params `path`, `error`) and
`notice.app_settings_newer` (params `version`), plus a title key you name
under `frame.status.settings_notice.*`. Add the keys to `ko.json` and
`en.json` with provisional sentences (ASTRA rewrites them in the next
packet; say which keys you added in the report). `data-ui`:
`frame.status.settings_notice`, `frame.status.settings_notice.close`.
A vitest test renders the frame with a bootstrap that carries each notice and
asserts the sentence and the close.

Commit: `feat(audit): X-10 시작 응답의 설정 알림을 프로젝트 경고 자리에 표시합니다 (#186 A10 TD-169)`

### 5. The REW executable field (A-42, TD-221)

The connect screen (`src/screens/connect/ConnectScreen.tsx`,
`useConnectScreen.ts`, `types.ts`) configures the measurement method and
devices through `measure_configure`. When the method is `rew`, add one text
field "REW 실행 파일" (a path; empty means the installed REW) below the REW
device fields, filled from `MeasureConfigResult.rew_executable`, written
with `measure_configure({ method, rew_executable })` on commit (blur or
Enter), with the service's refusal (`field = rew_executable`, a relative
path) shown inline like the other field errors. No file dialog: the host's
open-file kinds do not include an executable, and a pasted path is enough.
Keys under `connect.rew_executable.*` (label, help, the relative-path
error), provisional sentences as in step 4. `data-ui`:
`connect.rew_executable`. Update `src/screens/connect/fixtures.ts` and
`specimens.tsx` so one specimen shows the field filled. Tests: the hook test
pins the IPC params (`rew_executable` present only when the field changed,
empty string when cleared); the screen test shows the inline error.

Commit: `feat(audit): A-42 연결 화면에 REW 실행 파일 경로 입력란을 둡니다 (#186 TD-221)`

## Design direction

- Tokens and components: use the existing kit (`src/components`, the
  `ui-error` family, `IconButton`, the field components the connect screen
  already uses). No new colors, no new spacing values.
- Neighbours to copy: `ProjectWarnings` in `src/app/ProjectStatus.tsx` for
  the notice; the REW device fields in `ConnectScreen.tsx` for the path
  field.
- Interface ids (`data-ui`) that exist must not change; the two new ones are
  named above.
- Sentences in 하십시오체, labels as noun phrases, 14 px minimum, no emoji.

## Verification (foreground, in `apps/secs2r/ui`)

```sh
npm run typecheck
npm test
npm run build
```

Run the touched test files with `npx vitest run <files>` first, then the
whole suite. The i18n test checks that `ko.json` and `en.json` have the same
keys and the same placeholders.

## Report

Per step: the commit hash, files, what was deleted, the new catalog keys
with their provisional sentences, the interface ids, anything in the
generated inputs that differed from this packet, and
`git log --oneline fix/audit-186..audit/186-ui-x11`. Finish the report before
ending your turn.
