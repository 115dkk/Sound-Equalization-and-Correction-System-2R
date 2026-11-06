# P189 (Daybreak): audit #186, the service ADRs 0044, 0048, 0049, 0047 (service part), 0043, 0051 and 0060

Runs AFTER the Phase 1 packets P183..P187 have been rebased into
`fix/audit-186`. Claude re-creates the worktree branch from the then-current
`fix/audit-186`; do not start before Claude says the branch exists. P183's
items have already landed there: A-2 moved the side mdat into an `Export`
job and deleted `original_measurements`; A-5, A-29 and A-39 changed
`export.rs` (no design index for apply, `begin_commit` after resampling,
relative folders rejected); A-20 added the sidecar warnings
`measurement.source.skipped`/`budget`; A-14 reports a damaged settings file
through the log sink. Build on those commits; do not undo them.

Tree: `E:\wt\audit-svc`, a git worktree on branch `audit/186-svc-adr`.
Work only there. Every cargo command runs with
`CARGO_TARGET_DIR=C:\cargo-target\audit-svc`. Commit on that branch, one
commit per ADR step, in the order below. Do not push, do not rebase, do not
spawn sub-agents, do not touch `audit-state.md`, do not edit `docs/**`,
`CHANGELOG.md` or the screens (`apps/secs2r/ui/src/**` other than the
regenerated bindings).

Allowed files:

- `crates/secs-service/**` (source and tests);
- `crates/secs-project/**` (ADR 0060);
- `crates/secs-types/src/ipc.rs`: only the one optional field
  `BootstrapInfo.settings_notice` (ADR 0051 step 2) and the regenerated
  `apps/secs2r/ui/src/ipc/bindings/*.ts`;
- `apps/secs2r/src/main.rs`, `apps/secs2r/src/local_api.rs` and any
  `apps/secs2r/src/settings*.rs`: only the `AppEffects` removal and the
  effect adapters of ADR 0051 step 3;
- `apps/secs2r-cli/src/main.rs` and `crates/secs-api/src/lib.rs`: only the
  export call sites ADR 0043 step 5 changes;
- `features.toml`: register every new test name in the `[[feature]]` entry
  that covers the behaviour or add one in the same style;
  `python3 .github/scripts/check_feature_tests.py` must accept it.

Forbidden: everything else, in particular `crates/secs-engine/**`,
`crates/secs-filters/**`, `crates/secs-camilla/**`, the screens.

Read first: `CLAUDE.md`, `CONTEXT.md`, the seven ADRs in full
(`docs/adr/0044-open-project-edit-interface.md`, `0048-one-commit-and-snapshot-set.md`,
`0049-job-registry-owns-cancel-and-kind.md`, `0047-cancel-progress-context.md`
section '진행은 ProgressSink 하나로 보고합니다', `0043-export-plan-writer-adapters.md`,
`0051-one-app-settings-store.md`, `0060-measurement-store-keeps-source-info.md`),
`docs/adr/0009`, `0012`, `0013`, `0014`, `0015`, `0018`, `0021`, `0024`,
`crates/secs-service/src/session.rs`, `jobs.rs`, `monitor.rs`, `watch.rs`,
`ipc.rs:40-60`, `calibration.rs:24-60, 300-330, 1120-1240`, `export.rs`,
`export_run.rs`, `export_folder.rs`, `design_export.rs:177-618`, `apply.rs:100-340`,
`results.rs`, `project_edit.rs:760-880`, `lib.rs:370-640`,
`crates/secs-project/src/record.rs:180-240, 520-560, 660-860`,
`crates/secs-service/tests/{service,config,monitor}.rs` helpers, and the
P183 commits on `fix/audit-186` (`git log --oneline --grep "fix(audit): A-"`).

## Why

The architecture review of issue #186 found that the service repeats the
same rules in several places (dirty marking, lock order, commit policy,
snapshots, job cancellation, export assembly, app-settings effects, sidecar
parsing). Each has an ADR now. This packet implements them in the
resolve-audit skill's 'deepening' shape: tests at the deepened interface
first, behaviour moved behind it, the shallow tests the new ones cover
deleted. Observable behaviour, the wire (except the one optional field of
ADR 0051), the project and cache file formats and the exported bytes do not
change.

## The protocol for every step

Identical to P188: announce; tests first (fail on the assertion, stub so
they compile; literal expectations copied from existing tests or fixtures);
the smallest move; delete what the step lists; gate
(`cargo fmt --all -- --check`, `cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`,
`cargo test -p secs-service -p secs-project --locked`); after each ADR and
at the end `cargo test --workspace --locked` in the foreground; commit with
`git add <files>` and

```
refactor(audit): X-n <한국어 한 문장> (#186 A<n>)

ADR 00NN의 <k>단계입니다. <무엇을 옮겼고 무엇이 그대로인지 한두 문장.>

Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>
```

(`feat(audit):` where marked; 하십시오체, no em-dash, no program-as-subject
verbs 묻다/적다/넘기다, "추가하다" not "더하다"). A step that needs a file
you may not edit stops with its tests `#[ignore]`d and a comment, no commit,
and a line in the report.

## ADR 0044 (X-3): the open project's edit interface

### Step 1 · `OpenProject` methods and their tests (fields still `pub`)

Tests first, `crates/secs-service/src/session.rs` `#[cfg(test)]`:
`edit_marks_dirty_and_edit_if_marks_only_when_told` (fresh `OpenProject`
from the sample project: `edit(|p| …)` → `is_dirty()` true; after
`mark_saved()` false; `edit_if(|p| ((), false))` → still false),
`store_mut_marks_dirty_only_when_a_write_method_ran` (a `store_mut()`
borrow that calls no write method and drops → not dirty; one that calls
`record(..)` → dirty; this is the ADR's '위험' decided: `StoreEdit` counts
write calls), `record_undo_hands_the_payload_back_once_and_caps_at_sixteen`
(tokens have the `<identity>-<n>` shape; `take_undo` answers `Some` once
then `None`; the 17th entry evicts the oldest).

Move: add `project()`, `store()`, `edit`, `edit_if`, `store_mut() -> StoreEdit<'_>`,
`record_undo`, `take_undo`, `is_dirty`, `mark_saved` to `impl OpenProject`
over the existing fields; `StoreEdit` wraps the `MeasurementStore` write
methods (`record`, `replace_source`, `remove_position`, `restore_position`,
and any other `&mut self` method the handlers call: list them in the
report) and sets dirty on drop only if one ran.
Commit: `refactor(audit): X-3 OpenProject에 편집과 undo 메서드를 둡니다 (#186 A3)`.

### Steps 2a-2d · handlers move to the methods (four commits)

In this order, one commit each: `project_edit.rs` (12 `open.dirty = true`
sites and its 40 `project_mut()` calls), `measurement.rs` (2 sites, 22
calls, the undo handler at 673-717 becomes `take_undo(token)`),
`mic_calibration.rs` (20 calls), then `curves.rs`, `settings_edit.rs`,
`export_folder.rs`, `monitor.rs`, `session.rs` (the two in `run_change`).
Rule: a site that was unconditionally `dirty = true` becomes `edit`; a
conditional one becomes `edit_if`. `cargo test -p secs-service --locked`
passes unchanged after each commit (no test edits).
Commits: `refactor(audit): X-3 project_edit.rs가 OpenProject의 edit 메서드를 씁니다 (#186 A3)`
and the same shape for `measurement.rs`, `mic_calibration.rs`, 나머지 다섯 파일.

### Step 3 · `dirty`, `undo`, `store` become private

Lower the three fields to private (`path`, `tutorial`, `tutorial_folder`,
`results` stay `pub(crate)`); the compiler names leftovers; fix them with
the methods only.
Commit: `refactor(audit): X-3 OpenProject의 dirty, undo, store를 비공개로 바꿉니다 (#186 A3)`.

### Step 4 · `with_open`, `with_open_ticket`, `read_open` and the lock order (TD-213)

Tests first: `with_open_may_call_acceleration_again_without_deadlock`
(inside `with_open` call `self.acceleration()`; the test finishes within
the harness timeout; this replaces the lock-order regression tests at
`tests/service.rs:4589-4736`: delete the ones this covers and name them in
the report) and `result_lines_summary_and_lines_come_from_one_lock_td213`
(reuse the pattern of `tests/service.rs:4008` that interleaves a commit
between two locks; after the change, summary `stale` and the lines agree).

Move: the seven `ComputeIdentity::from(&self.acceleration())`-then-lock
sites (`apply.rs:419`, `calibration.rs:261`, `curves.rs:259, 498`,
`export.rs:106`, `lib.rs:689` and the one in `calibration.rs:55/215` if
still present) into the accessors; `export_folder.rs:11`'s relock becomes a
path-only function called inside `with_open`, removing the manual
`drop(session)` pairs in `export.rs`; `result_lines` builds summary and
lines inside `read_open`.
Commit: `refactor(audit): X-3 잠금 순서와 result_lines를 with_open 하나에 모읍니다 (#186 A3 TD-213)`.

## ADR 0048 (X-7): one commit, one snapshot

### Step 1 · `Finality` and one `commit`

Tests first (`session.rs` tests; rewrite the six commit tests at
`session.rs:1073-1274` into these): `final_commit_refuses_a_stale_ticket_before_begin_commit`,
`final_commit_reports_cancelled_before_running_the_change`,
`final_commit_runs_the_change_after_begin_commit_and_dirty_follows_the_return_value`,
`phase_commit_after_cancel_is_cancelled_and_leaves_the_job_cancellable`,
`phase_commit_survives_a_later_cancel_and_the_next_phase_fails`,
`streaming_commit_checks_only_the_ticket_even_when_cancelled` (replaces
`apply_checks_the_ticket_without_begin_commit`, which is deleted).

Move: `commit(session, ticket, job, finality, change)`; delete
`commit_phase` and `apply`; replace the callers (`calibration.rs:127-131`
`run_mimo`, `measurement.rs:606-612` `CommitBoundary`, `monitor.rs`,
`export.rs`): former `apply` sites are `Finality::Streaming`, former
`commit_phase` sites `Finality::Phase`. The review checklist: `begin_commit`
is called only under `Final`.
Commit: `refactor(audit): X-7 커밋 함수 셋을 Finality를 가진 commit 하나로 합칩니다 (#186 A7)`.

### Step 2 · `commit_results` takes over `persist_cache`

Tests first: `commit_results_saves_the_cache_outside_the_lock_and_reports_a_failed_save_once`
(a cache path made unwritable → the owner's notices hold one
`cache_save_failed`; a following successful save clears it; a project
switched between commit and save leaves the in-memory copy alone, i.e.
the `open.path != path` branch). Move `persist_cache`'s body
(`calibration.rs:1126-1166`) into `commit_results`; the callers in
calibration, MIMO and the subs-only calibration drop their
`(open.path.clone(), open.results.cache_snapshot())` ritual. Delete
`persist_cache`.
Commit: `refactor(audit): X-7 결과 커밋이 캐시 저장까지 맡습니다 (#186 A7)`.

### Step 3 · one `ProjectSnapshot`

Tests first: `project_snapshot_load_originals_checks_cancel_between_files_and_keeps_plan_order`
(move the existing `OriginalSnapshot` test if there is one; otherwise
assert with a cancel after the first file that the second file is not
read, and that roles come back in the plan's order). Move:
`ProjectSnapshot { ticket, path, project }`, `Session::snapshot()`,
`load_measurements`, `load_originals`; delete `OriginalSnapshot`,
`MeasurementSnapshot`, `original_snapshot()`, `measurement_snapshot()`;
shrink calibration's `Snapshot` to `ProjectSnapshot` plus the two settings;
the measurement and monitor `(project, ticket)` tuples become `snapshot()`.
(A-2 already deleted `original_measurements`; the side-mdat job now calls
`load_originals`.)
Commit: `refactor(audit): X-7 스냅샷 셋을 ProjectSnapshot 하나로 합칩니다 (#186 A7)`.

### Step 4 · the job knows its memory

Tests first (`jobs.rs` tests): `job_context_reports_the_memory_available_at_start`
(a `FixedMemory(123)` probe → `job.available_memory() == 123`). Move:
`JobRegistry::start` takes `Deps.memory`; `Service::start_job(kind, events, body)`;
`compute_context(job, background)` reads it; the five duplicated two-line
sites go. Commit: `refactor(audit): X-7 잡이 시작 시점의 가용 메모리를 보관합니다 (#186 A7)`.

### Step 5 · `MimoRun`

No new test. `compute_and_commit_mimo(session, ticket, job, run: MimoRun)`;
the `#[allow(clippy::too_many_arguments)]` at `calibration.rs:309` goes.
Commit: `refactor(audit): X-7 MIMO 호출 인자 아홉을 MimoRun 하나로 묶습니다 (#186 A7 TD-213)`.

## ADR 0049 (X-8): the job registry owns the secondary cancel token

### Step 1 · `running(kind)` and `attach_cancel`

Tests first (`jobs.rs` tests): `running_answers_the_live_job_of_a_kind_and_none_after_it_ends`,
`attach_cancel_is_cancelled_with_the_job_and_only_the_last_attached_token_counts`.
Move: the record's `attached: Option<crate::measure::CancelToken>`,
`JobContext::attach_cancel`, `JobRegistry::running`, `cancel` cancels all
three tokens.
Commit: `refactor(audit): X-8 잡 레지스트리가 보조 취소 토큰과 실행 중인 종류를 가집니다 (#186 A8)`.

### Step 2 · the level monitor

Tests first (`tests/monitor.rs`): `job_cancel_alone_stops_a_playing_level_monitor`
(one `JobCancel` → `level_play_end` with `stopped`, then the job is
terminal; replace the two-step case near `tests/monitor.rs:838` if it
assumed `cancel_level_monitor_job`) and `level_stop_stops_only_the_play_and_keeps_the_monitor`
(pins today's behaviour). Move: `level_play` calls `job.attach_cancel(token.clone())`;
delete `LevelState.job`, `cancel_level_monitor_job`, the `ipc.rs:44-47`
special case; `stop_level_monitor` becomes `running(JobKind::LevelMonitor)`
plus `jobs.cancel`. `PlayRequest.cancel` stays.
Commit: `refactor(audit): X-8 레벨 모니터가 잡 레지스트리의 취소를 씁니다 (#186 A8)`.

### Step 3 · the status watch

Tests first: `stop_status_watch_cancels_the_running_watch_and_answers_none_otherwise`.
Delete `WatchState.job` (`watch.rs:10, 22, 44`), use `running(JobKind::StatusWatch)`.
Commit: `refactor(audit): X-8 상태 감시가 거울 필드 대신 running을 씁니다 (#186 A8)`.

## ADR 0047 (X-6), service part: one progress sink

Tests first: `mock_measure_reports_in_role_stages_td171` already exists
after P183's A-16; keep it and add nothing unless A-16 was blocked (then
write it as P183 §A-16 describes). Move: delete `measure::ProgressSink`
(`crates/secs-service/src/measure/mod.rs:83-90`); the measurers receive
`JobContext::role_progress` directly; `report_progress`'s per-call
`Arc<dyn ProgressSink>` allocation goes (TD-207's progress part: one
private `fn progress(&self, stage, done, total)`; the `unwrap` on the
condvar at `jobs.rs:304` becomes `unwrap_or_else(|p| p.into_inner())`).
Commit: `refactor(audit): X-6 측정 방식이 잡의 role_progress 하나로 진행을 보고합니다 (#186 A6 TD-207)`.

## ADR 0043 (X-2): one export plan and per-kind writer adapters

### Step 1 · `ProjectKeys` and `inputs_unchanged_since`

Tests first (`results_tests.rs`): `project_keys_strings_are_pinned`
(`ProjectKeys::of(&sample)` equals the literal strings the current
`config_key`/`signature_key`/`target_key_string` produce for the sample
project: compute them once by hand from the current functions, paste the
literals, never call the old functions from the test) and
`inputs_unchanged_since_is_false_after_a_config_settings_or_target_change`.
Move: the four key functions at `calibration.rs:1168-1240` become
`ProjectKeys` methods; `export.rs:308-316, 576-592` stop recomputing
`run_key`. Key strings unchanged.
Commit: `refactor(audit): X-2 결과 장부가 프로젝트 키를 만들어 가집니다 (#186 A2)`.

### Step 2 · `ExportResult.main_config` and `design_id`

Tests first (`tests/apply.rs`): `camilla_apply_reads_the_main_config_from_the_export_result`
(the writer answers `main_config: Some(<folder>/<name>.yml)` and
`design_id: Some(..)` for filter kinds; apply no longer scans `.yml`
names). Move: the two fields; delete the `.yml` lookup at `apply.rs:113-129`
and the `ok_or_else(Internal)` at `:114, :268`.
Commit: `refactor(audit): X-2 적용이 파일 목록 대신 ExportResult의 주 설정을 읽습니다 (#186 A2)`.

### Step 3 · `ExportPlan` and `plan_export`

Tests first (`export_plan_tests.rs`, new): `plan_export_rejects_the_four_invalid_requests_with_reasons`
(relative folder → `reason=relative_folder` (A-39's code path moves here),
`design_name` on an mdat kind, `camilla_devices` on a non-CamillaDSP kind,
tutorial session) and `plan_export_marks_apply_plans_without_index_or_record`
(`ExportPurpose::Apply` → `record == None`, `destination.index_root == None`;
this absorbs A-5's flag). Move: `export_plan.rs` with the ADR's structs;
`plan_export` takes the first ~300 lines of `start_export_with` under the
lock; `resolve_output_rate` reads only `dsp.output_rate` under a short lock
and enumerates devices outside; sidecar paths are captured, files read in
the job (TD-155); FIR-bearing values wrapped in `Arc`.
Commit: `refactor(audit): X-2 내보내기 계획을 잠금 아래에서 하나로 만듭니다 (#186 A2 TD-155)`.

### Steps 4a-4f · the six writer adapters (six commits)

Order: `CamillaWriter`, `StagedMdatWriter`, `BeforeAfterMdatWriter`,
`FilterTextWriter`, `EqapoWriter`, `ConvolutionWriter`. Tests first per
adapter (`export_plan_tests.rs`): `<kind>_writer_stores_the_same_files_main_config_and_design_id`
(the file list, `main_config` and `design_id` literals come from the
existing export tests at `tests/service.rs:1031-1500, 2355-2535, 3901-4007`;
bytes stay pinned by those goldens) and, once for `CamillaWriter`,
`a_job_cancelled_during_prepare_leaves_the_staging_folder_empty` (reuse
A-29's test if it exists; otherwise this is it). Move the kind's branch of
the job closure into `prepare` (resampling, rendering, mdat bytes) and
`write` (files under `destination.staging`); the job body is
`writer.prepare(&plan, &job)?; job.begin_commit()?; writer.write(plan, &job)`.
After each: `cargo test -p secs-service -p secs-camilla -p secs-filters -p secs-export --locked`.
Commits: `refactor(audit): X-2 CamillaDSP 작성기를 adapter로 뽑습니다 (#186 A2)` and the
same shape for the other five (`곁 mdat`, `보정 전후 mdat`, `필터 텍스트`,
`Equalizer APO`, `컨볼루션`).

### Step 5 · `start_export(plan, …)` only

Delete `start_export_with` and the job closure; `install_bundle(&plan, result)`
holds install, manifest and index; the CLI (`apps/secs2r-cli/src/main.rs`)
and `secs-api` export entry points call `plan_export` + `start_export`.
Move the long export tests of `tests/service.rs` into `tests/export.rs`
split at the first `reopen` (TD-167's export part); the test count does
not drop. `begin_commit` appears exactly once in `export.rs`.
Commit: `refactor(audit): X-2 내보내기 입구 셋이 같은 계획을 거칩니다 (#186 A2 TD-195)`.

## ADR 0051 (X-10): one app-settings store

### Step 1 · `app_settings.rs` owns the settings

Tests first (`crates/secs-service/src/app_settings.rs` tests):
`open_without_a_file_starts_from_defaults_without_a_notice`,
`set_increments_the_revision_and_writes_atomically_and_a_rejected_value_changes_nothing`
(window size 0 rejected; file bytes and revision unchanged),
`effects_run_once_per_revision_in_order_even_when_two_threads_interleave`
(two fake `SettingsEffect`s; replaces `tests/config.rs:39-62 ReorderedUpdates`
and `lib.rs:612 effects_compare_with_the_last_applied_settings_td115`, both
deleted), `typed_readers_return_the_stored_fields_and_apply_settings_set_touches_nothing_else`.
Move: `AppSettingsStore`, `SettingsEffect`, `Opened` as the ADR writes
them; `Service` owns the store; the raw `lock(&self.app_settings)` reads
(`apply.rs:19, 72, 240, 360`, `compute.rs:64`, `project_edit.rs:770, 775,
811`) become typed readers; `apply_settings_set` is one `set` call; the
update-policy effect registers as a `SettingsEffect`. `AppEffects` becomes
private to the store; `RevisedAppSettings` stays.
Commit: `refactor(audit): X-10 앱 설정 저장소 하나가 읽기, 저장, 효과를 맡습니다 (#186 A10)`.

### Step 2 · the notice and the newer-version rule (behaviour)

Tests first: `open_reports_a_damaged_or_oversized_file_as_app_settings_unreadable`
(`{not json` and a 1 MiB + 1 file → defaults and `Opened.notice.code == "app_settings_unreadable"`
with `params.path`), `open_keeps_a_newer_file_read_only_and_set_refuses_it`
(`version: 2` → `app_settings_newer`, `set` answers
`ErrorCode::Unsupported` with `params.reason == "app_settings_newer"`, the
file bytes unchanged), and in `tests/service.rs`
`bootstrap_carries_the_settings_notice` (the `BootstrapInfo.settings_notice`
of a service opened on a damaged file is the same notice). Move: the two
notice codes, the read-only state, `BootstrapInfo.settings_notice: Option<Notice>`
with `#[ts(optional)]` in `crates/secs-types/src/ipc.rs`; run
`cargo test -p secs-types --locked` and commit the changed bindings. A-14's
log-sink line stays as well (the ADR says both). Record the two new notice
codes in the report: the catalog text is ASTRA's and the screen is Opus's.
Commit: `feat(audit): X-10 읽지 못한 설정 파일을 알리고 새 판 파일은 덮어쓰지 않습니다 (#186 A10 TD-169)`.

### Step 3 · the app registers effect adapters

Delete `Mutex<AppEffects>` (`apps/secs2r/src/main.rs:236-246`),
`apply_revision` (`local_api.rs:96-106`) and the `on_change` thread
(`local_api.rs:143-162`); register the icon and local-API adapters as
`SettingsEffect`s when the service is built; the local API server keeps
only the `secs://external-change` event. Gate: `cargo test -p secs2r --locked`
(the app crate's own tests) and `cargo clippy -p secs2r`.
Commit: `refactor(audit): X-10 앱이 설정 효과 adapter만 등록합니다 (#186 A10)`.

ADR 0051 step 4 (screen notice + catalog) is Opus/ASTRA's; not here.

## ADR 0056 (X-15): not in this packet

ADR 0056 exists but it changes the screen's state module
(`apps/secs2r/ui/src/state/projectScope.ts`), which Daybreak must not
touch. Skip it; Claude dispatches it to the UI worker.

## ADR 0060 (X-19): the measurement store keeps the source info

### Step 1 · the map, `sources_by_position`, `source_of(&self)`

Tests first (`record.rs` tests, using the `SOURCE_PARSE_COUNT` hook at
`record.rs:15`): `open_parses_each_sidecar_once_and_source_of_never_parses_again`
(three sidecars → count 3 after `open`; five `source_of` calls → still 3)
and `sources_by_position_matches_the_old_loader` (equal to what
`load_project_sources` answered for the same project; call the old
function in this one test since it is being retired, and delete the test in
step 5). Move: `sources: BTreeMap<(i64, String), Arc<SourceInfo>>`,
`SourceBudget`, `load_project_sources_within` becomes `pub(crate)` and
answers `LoadedSources { by_entry, exhausted, skipped }`; `open` fills the
map; `source_of(&self, role, pos) -> Option<Arc<SourceInfo>>`. The old
`load_project_sources` stays for now.
Commit: `refactor(audit): X-19 측정 저장소가 열 때 읽은 사이드카를 보관합니다 (#186 A19 TD-184)`.

### Step 2 · the writes maintain the map

Tests first: `record_replace_remove_and_restore_keep_the_source_map_in_step_with_the_files`
(`record` with a sidecar → `source_of` answers it and equals the file;
`replace_source(.., None)` → gone from map and disk; `remove_position` →
`None`; `restore_position` → back). Move: the four write methods update the
map where they update the fingerprint.
Commit: `refactor(audit): X-19 저장소의 쓰기 넷이 출처 맵을 함께 갱신합니다 (#186 A19)`.

### Step 3 · the service reads the map

Tests first (`tests/service.rs` or the mic-calibration tests):
`mic_calibration_attach_parses_no_sidecar_under_the_session_lock`
(`SOURCE_PARSE_COUNT` unchanged across `attach`; the store exposes a
`#[doc(hidden)] pub fn source_parse_count()` for tests if the static is not
reachable) and `the_side_mdat_job_reads_sources_from_the_snapshot_not_the_disk`.
Move: `calibration.rs:554`, `mic_calibration.rs:126, 375, 386`,
`session.rs:655`, `export.rs:336` use `source_of`/`sources_by_position`;
the `ProjectSnapshot` (ADR 0048) captures `sources_by_position()` under the
lock for `load_originals`.
Commit: `refactor(audit): X-19 서비스가 사이드카를 다시 읽지 않고 저장소의 맵을 씁니다 (#186 A19)`.

### Step 4 · the skip warnings (verify A-20)

A-20 (P183) already added `measurement.source.skipped`/`budget`. Make the
warnings come from `LoadedSources.skipped` (`SourceSkip { pos, role, reason }`)
if A-20 built them elsewhere; keep A-20's tests green; no new test unless
A-20 was blocked (then write `unreadable_sidecar_is_reported_as_a_migration_warning_td183`
as P183 describes). Commit only if code moved:
`refactor(audit): X-19 건너뛴 사이드카 경고가 로더의 결과에서 나옵니다 (#186 A19 TD-183)`.

### Step 5 · delete the old loaders

Delete `load_project_sources`, `load_project_irs`, `LoadedIr`, `load_source`
and the step-1 comparison test; `secs-project` is a published crate but
0.x minor and no outside callers (ADR 0016): say so in the commit body so
Claude writes the CHANGELOG line.
Commit: `refactor(audit): X-19 옛 사이드카 로더와 LoadedIr을 삭제합니다 (#186 A19 TD-219)`.

## Verification (foreground; do not end the turn before they finish)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p secs-types --locked && git status --short apps/secs2r/ui/src/ipc/bindings
(cd apps/secs2r/ui && npm run typecheck)
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-svc-adr
```

The bindings folder shows every file as modified after regeneration because
the generator writes LF: commit only the files whose `git diff` has content
changes. `check_feature_tests.py` on Windows reports two names it cannot
list here (`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`); anything else is yours.

## Report

Per step: the commit hash, the tests written first and the assertion they
failed on, what moved, what was deleted (name the shallow tests), the gate
tails. Then: steps blocked or narrowed and why, the write-method list
`StoreEdit` wraps, the two notice codes of ADR 0051 for the catalog, and
the final `git log --oneline` of the branch.
