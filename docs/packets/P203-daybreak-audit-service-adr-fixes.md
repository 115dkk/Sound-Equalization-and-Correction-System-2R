# P203 (Daybreak): audit #186, defects found in the P189 service ADR work

Worktree `E:\wt\audit-svc`, branch `audit/186-svc-fix` (Claude created it from
the current `fix/audit-186`, which already carries P189's 34 commits plus
Claude's merge fixes; `git status` is clean when you start). Every cargo
command and every script that starts cargo runs with
`CARGO_TARGET_DIR=E:\cargo-target\audit-engine` (a warm target shared with
Claude's own verification runs: you may see "Blocking waiting for file lock on
build directory", just wait). Never run `cargo clean`, never delete anything
under `E:\cargo-target`, and after the first command check that
`E:\wt\audit-svc\target` does not exist. No workspace-wide `cargo test` or
`cargo clippy`; per-package (`-p`) only. The public API crate's package name
is `sound-equalization-and-correction-system-rust-api` (directory
`crates/secs-api`). One commit per step with the title given, trailer
`Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>`. Do not push, do
not rebase, do not touch ADRs, `docs/**`, `CHANGELOG.md`, `audit-state.md`,
`Cargo.toml`, `Cargo.lock` or the UI.

## Why

A read-only audit of P189 (`d1dda207..audit/186-svc-adr`) found the defects
below. Each step names the ADR whose contract is not met, the defect, the
decision, and the test that must exist first (Red) before the fix (Green).
Where a step restores assertions that P189 deleted, the restored test is part
of that step. Read the ADR named in each step before you start it: 0043,
0044, 0047, 0048, 0049, 0051, 0060 under `docs/adr/`.

## Steps, in this order

### 1. ADR 0051: effects run outside every store lock and a re-entrant set does not deadlock

`crates/secs-service/src/app_settings.rs`, `AppSettingsStore::set`. Today the
`effects` mutex is held while `SettingsEffect::changed` runs (lines around
159–174). An effect that calls `set` again (through the service's
`AppSettingsSet`) blocks on that mutex forever.

Decision: one drainer. `set` pushes `(revision, settings)` onto a pending
list under the `effects` lock. If no thread is delivering, this thread
becomes the drainer: in a loop it takes the pending entry whose revision is
`effects.revision + 1` (waiting on the condvar when that revision has not
arrived yet, which happens when a later `set` reached the queue first),
releases the lock, calls every registered effect with
`(previous, next)`, re-locks, advances `effects.revision`/`settings`,
notifies, and stops when nothing is pending. If a drainer is already running,
`set` returns `Ok` right after enqueueing: the drainer delivers that revision
in order. No lock is held while an effect runs.

Red tests in the same file:
- `an_effect_that_sets_again_is_delivered_after_the_current_revision_without_deadlock`:
  an effect holding a `Weak<AppSettingsStore>` (or the `Arc` through a
  `OnceLock`) calls `set` once from its first `changed`; run `set` from the
  test thread with a 5 s watchdog thread that panics on timeout; assert both
  effects saw `(ko→en)` then `(en→fr)` and the store ends on revision 3.
- keep `effects_run_once_per_revision_in_order_even_when_two_threads_interleave`
  green.
- `update_policy_effect_receives_the_last_policy_in_revision_order`: a fake
  `UpdateAdapter` recording `set_policy` calls; two interleaved `set`s (first
  effect blocked as the existing interleave test does) end with the recorded
  calls in revision order and the last one `Off` (this restores the assertion
  `tests/config.rs` lost with `newer_app_settings_win_when_older_effects_finish_late_td115`).

Commit: `fix(audit): X-10 설정 효과를 잠금 밖에서 판 순서대로 전달하고 효과 안의 재저장을 허용합니다 (#186 A10)`

### 2. ADR 0051 / TD-169: a newer settings file written while running is still never overwritten

`AppSettingsStore::set` only looks at `read_only_reason` decided at `open`.
The service used to re-read the file's `version` before every save
(`refuse_newer_app_settings`, fix/audit-186 320987e0) and P189 dropped it.

Decision: inside `set`'s critical section, before `atomic_write`, read the
file's version header (reuse `read_bounded` + `AppSettingsVersion`); if it is
newer than `APP_SETTINGS_VERSION`, set `read_only_reason = newer_version` and
refuse with `Unsupported`, `reason = app_settings_newer`, file untouched. An
unreadable file at that point is not a refusal (that case is decided at
open, as today).

Red test in `crates/secs-service/tests/service.rs`:
`newer_settings_file_written_while_running_is_never_overwritten_td169`:
create the service on an empty path, write a `version: 2` file, call
`AppSettingsSet` → `Envelope::Err` with code `Unsupported` and
`reason = app_settings_newer`, the file bytes unchanged, and a registered
`SettingsEffect` not called. Register it in `features.toml` next to the other
TD-169 tests (find the feature that lists `newer_settings_file_is_never_overwritten_td169`).

Commit: `fix(audit): X-10 실행 중 새 판이 된 설정 파일도 덮어쓰지 않습니다 (#186 A10 TD-169)`

### 3. ADR 0051: the local API effect never stops the server from inside its own request

`apps/secs2r/src/local_api.rs`. `LocalApiSettingsEffect` (in `main.rs`) runs
synchronously inside `set`, which runs inside the HTTP handler of the local
API when the request came from the local API. `Runtime::apply_settings` then
calls `stop()` on the server that is serving that request: the server waits
for the in-flight request, hits the shutdown bound, and the response is lost.

Decision: `Manager` owns one worker thread fed by an `mpsc::Sender<(LocalApiSettings, LocalApiSettings)>`;
`Manager::apply_settings(old, new)` only sends, the thread calls
`Runtime::apply_settings` in order. `startup` stays synchronous. The thread
is named `secs-local-api-settings`.

Red test (unit test in `local_api.rs`, built with the `tauri` `test`
feature the app already has in `[dev-dependencies]`, see `update.rs` for
`tauri::test::mock_builder`): build a `Manager` over a real loopback
`Runtime` (disabled), call `apply_settings(disabled, enabled port 0)` and
assert the call returns in under 100 ms; then poll `status()` up to 3 s until
`running` is true. If `Manager::new` cannot be built in a unit test because
of its Tauri `AppHandle`, split the thread-and-channel part into a
`SettingsApplier` struct that takes `Arc<Runtime>` and a closure, test that
struct, and have `Manager` wrap it. List what you chose in the report.

Commit: `fix(audit): X-10 로컬 API 설정 효과가 별도 스레드에서 순서대로 적용됩니다 (#186 A10)`

### 4. ADR 0049: a token attached after the cancel request is cancelled at once

`crates/secs-service/src/jobs.rs`, `JobContext::attach_cancel`. It attaches
to any non-terminal job, including one whose status is already
`CancelRequested`; `cancel` only cancels tokens attached at that moment. The
level monitor (`monitor.rs`) attaches a fresh play token after checking the
loop condition, so a `JobCancel` between the two leaves the burst playing and
`level_play_end` says `done`.

Decision: `attach_cancel` cancels the token immediately when the job's status
is `CancelRequested` (and still attaches it so the monitor's own `level_stop`
path is unchanged).

Red test in `jobs.rs`:
`a_token_attached_after_the_cancel_request_is_cancelled_immediately`: start a
job, cancel it, then `attach_cancel(token)` → `token.is_cancelled()` (or
whatever the `CancelToken` query is called) is true without any further call.

Commit: `fix(audit): X-8 취소 요청 뒤에 붙인 보조 토큰도 바로 취소됩니다 (#186 A8)`

### 5. ADR 0060: Save As keeps the source map

`crates/secs-service/src/session.rs`, `prepare_save_as` builds
`MeasurementStore::new(destination, project)`, whose `sources` map is empty
(`crates/secs-project/src/record.rs`, `new`). After Save As every
`source_of` is `None` until the project is reopened, so calibration runs
without the mic correction and the side mdat loses its sources.

Decision: add `pub fn MeasurementStore::relocated(&self, path: PathBuf, project: Project) -> MeasurementStore`
in secs-project that copies the source map (and the parse counter, see
step 8) from `self`; `prepare_save_as` uses it.

Red test in `crates/secs-service/tests/service.rs` (or `src/session.rs` if
the fixture is easier there): open a project whose positions have
`.src.json` sidecars (the oracle sample project or the existing fixture that
`the_side_mdat_job_reads_sources_from_the_snapshot_not_the_disk` uses), take
`sources_by_position()`, Save As to another folder, and without reopening
assert `sources_by_position()` equals the earlier map and the next side-mdat
snapshot carries the same sources.

Commit: `fix(audit): X-19 다른 이름으로 저장해도 출처 맵이 남습니다 (#186 A19)`

### 6. ADR 0060: the map holds values only for measurements that currently have an IR

`record.rs`: `start_position` replaces the position's measurement entry but
leaves `sources`; `clear_position` likewise; `project_edit.rs`
`clear_measurements` empties the measurement JSON through `edit` and the
map keeps stale entries. The old `source_of` checked the current entry first.

Decision: (a) `start_position`, `clear_position` and `remove_position` drop
the position's map entries (`remove_position` keeps them inside
`RemovedPosition` for `restore_position`, as it already keeps the files);
(b) `source_of` and `sources_by_position` return an entry only when the
project's measurement entry for `(pos, role)` currently records an IR file,
so a `clear_measurements` through `edit` cannot leak stale sources either.

Red tests in `crates/secs-project/tests/record.rs`:
`start_position_and_clear_position_forget_the_sources_of_that_position` and
`sources_follow_the_current_measurement_entries` (remove a position →
`sources_by_position` lacks it; restore → it is back; clear all measurement
entries on the project → `sources_by_position` is empty).

Commit: `fix(audit): X-19 출처 맵은 현재 측정 항목이 있는 자리에서만 값을 반환합니다 (#186 A19)`

### 7. ADR 0060: the source budget holds for the whole open project

`SourceBudget` lives only in the loader. `record` and `replace_source` push
`Arc<SourceInfo>` without any size check, so an open project's map can grow
past the budget the loader enforces, and a reopen then drops sources the
live session had.

Decision: `MeasurementStore` owns the budget state (bytes in the map, same
per-item cap and total as the loader uses). `record` and `replace_source`
write the sidecar file regardless, but keep the map entry only when it fits;
when it does not, they push the same `measurement.source.budget` warning the
loader emits (`warnings()`), and `source_of` returns `None` for it.
`remove_position`, `start_position`, `clear_position` and `replace_source`
release the bytes of what they drop.

Red test in `crates/secs-project/tests/record.rs`:
`recorded_sources_past_the_budget_are_written_but_not_kept_and_release_on_remove`
(use sources sized against the real constants; if that makes the test slow,
add a `#[doc(hidden)] pub fn with_source_budget(self, cap_bytes: usize) -> Self`
builder and say so in the report).

Commit: `fix(audit): X-19 출처 예산이 열린 프로젝트의 쓰기에도 적용됩니다 (#186 A19)`

### 8. ADR 0060: no process-global parse counter in the shipped API

`secs_project::source_parse_count` (global `AtomicUsize`, re-exported from
`crates/secs-project/src/lib.rs`) is test instrumentation in the public API,
and the service tests that read it (`tests/service.rs` around the side-mdat
test, `src/mic_calibration.rs` unit test) are racy under parallel tests.

Decision: delete the global counter, the thread-local and the public
re-export. `MeasurementStore` keeps a per-instance counter exposed as
`pub fn source_parse_count(&self) -> usize` (documented as a diagnostic:
sidecar parses since `open`/`relocated`). secs-project's own tests use it.
In the service: the side-mdat test keeps only its behavioural assertion (the
snapshot value survives a changed sidecar on disk) and drops the counter
assertion; the mic-calibration unit test reads the open store's counter
through the session (`open.store().source_parse_count()`), which is in-crate.

Commit: `fix(audit): X-19 사이드카 파싱 횟수가 저장소 인스턴스의 진단값이 됩니다 (#186 A19 TD-219)`

### 9. ADR 0043: a cached mdat replay rejects inputs that changed after planning

`crates/secs-service/src/export.rs` (the replay branch that calls
`inputs_unchanged_since`, around line 1140) compares the current result book
with the current project. The old code kept the plan-time input key and
rejected a change made after planning. Compute and Export are different job
groups, so a calibration of changed inputs can finish between planning and
the replay's read.

Decision: `ExportPlan` carries the plan-time `ProjectKeys` (or the input key
it derives); the replay compares the plan's keys with the current project's
keys as well as calling `inputs_unchanged_since`; a mismatch is
`StaleInputs`.

Red test in `tests/service.rs`: start a cached mdat export with an
`EventSink` that blocks on the job's first progress event (the test sinks in
that file already do this); while it is blocked, change an input setting and
run a calibration to completion; release the sink; the export job ends with
`StaleInputs`. Name: `a_cached_mdat_export_rejects_inputs_changed_after_planning_td155`.

Commit: `fix(audit): X-2 캐시 재생 내보내기가 계획 뒤 바뀐 입력을 거절합니다 (#186 A2 TD-155)`

### 10. ADR 0043: the writer tests drive the writers

The six `*_writer_stores_the_same_files_main_config_and_design_id` tests in
`export.rs` (around lines 1502–1579) check type names or byte copies and
never run a writer. ADR 0043's test section asks for the file list,
`main_config` and `design_id` of each kind against the literals the long
integration tests assert.

Decision: rewrite them (keep the names) to build the plan with `plan_export`
on the oracle sample project the way `export_plan_tests.rs` does, run the
export job to completion with the offline adapters, and assert the folder's
file list, `main_config` (Some for CamillaDSP, None for the others today;
assert what the code guarantees) and `design_id` (`Some` for the three
filter kinds, `None` for the mdat kinds). Move them into
`export_plan_tests.rs` if that keeps `export.rs` shorter.

Commit: `test(audit): X-2 작성기 시험이 실제 계획과 작성기를 실행합니다 (#186 A2)`

### 11. ADR 0044: `status()` reads one session snapshot

`crates/secs-service/src/lib.rs`, `Service::status` reads results under
`read_open`, releases, then locks the session again for `info()` and
`last_export()`. Two locks can mix project A's results with project B's
path.

Decision: one `lock(&self.session)` (compute identity read before it, as
`with_open` does) yields `info`, `results` and `last_export`.

Red test: in `src/lib.rs` or `tests/service.rs`, swapping the open project
between the two reads is not reachable from outside, so the test asserts the
shape instead: with a project open, `status().project.path` and the
`results` summary belong to the same project after an `open` of a second
project racing on another thread (loop 50 times, assert `project` and
`results` are both `Some` or both `None` and the path matches the results'
project whenever both are `Some`). If no observable field ties the summary
to a project, say so in the report and keep the test to the both-or-neither
assertion.

Commit: `fix(audit): X-3 상태 응답이 세션 잠금 하나에서 나옵니다 (#186 A3)`

### 12. ADR 0044: a failed record that changed memory leaves the project dirty

`MeasurementStore::record` mutates the entry, fingerprint and map, then
`save()`s; a failing save returns `Err` and `StoreEdit::record` records no
write; `measurement.rs` then calls `mark_saved()` on a project that was
clean before. Memory changed, the flag says clean.

Decision: in secs-project, `record` returns
`Result<MeasurementRecord, RecordError>` with
`RecordError::Invalid(ProjectError)` (nothing changed) and
`RecordError::SaveFailed(ProjectError)` (memory changed, file not saved);
`StoreEdit::record` marks dirty for `SaveFailed`; the `mark_saved()`
rollbacks in `measurement.rs` (around lines 461–474, 606–623, 693–714, 734)
go: after a failed record the flag is whatever `StoreEdit` set. The IPC
error mapping stays as it is for both variants.

Red test in `tests/service.rs` (or `src/measurement.rs`): IR folder
writable, the `.secs` destination unwritable (a directory in its place, or
read-only on Windows via a locked file), record a mock measurement → the
call fails, the measurement entry is in memory, `ProjectInfo.dirty` is true.
Name: `a_failed_project_save_after_a_recorded_measurement_leaves_the_project_dirty`.

Commit: `fix(audit): X-3 저장이 실패한 측정 기록은 프로젝트를 dirty로 남깁니다 (#186 A3)`

### 13. ADR 0048: the assertions P189 deleted come back

Restore, in `src/session.rs` unless noted:
- stale ticket and cancel together → the canonical `Cancelled` error for a
  `Finality::Final` commit (old `a_cancelled_stale_ticket_finishes_with_the_canonical_cancelled_error`);
- a change closure returning `ProjectIo` leaves `dirty` unchanged (old
  `a_failing_change_leaves_dirty_alone`), for `Final` and `Phase`;
- `Streaming`: the normal return value comes back, `changed = false` keeps
  the project clean, and the job stays cancellable afterwards;
- `tests/measure.rs` `test_cancel_before_start_measures_nothing`: the last
  progress stage reported is `cancelled` (through `JobContext::role_progress`,
  observed on the events the test already collects);
- `crates/secs-project/tests/record.rs`: opening a project with no
  measurements yields an empty `sources_by_position()` (old
  `test_load_project_irs_without_measurements`), and
  `test_reload_project_restores_irs_by_role` asserts the reopened sample
  rate (48 kHz) again.

Commit: `test(audit): X-7 P189가 지운 커밋·스트리밍·측정 취소 단언을 되살립니다 (#186 A7)`

### 14. ADR 0047: the measurer trait is not public with a private context type

`crates/secs-service/src/measure/mod.rs` `pub trait Measurer` takes
`Option<&JobContext>` from the private `jobs` module, so nothing outside the
crate can implement it; nothing outside does (secs-rew implements its own
`Measurer`), and `grep -rn "secs_service::measure::" crates apps` finds no
user.

Decision: `mod measure` becomes private (drop `pub`); keep a `pub use` in
`lib.rs` only for items another workspace crate or app actually needs (the
build tells you; the CLI and the public API crate are the candidates). The
trait itself stays `pub(crate)` in effect.

Commit: `refactor(audit): X-6 측정 모듈이 비공개가 되고 필요한 항목만 재수출합니다 (#186 A6)`

### 15. Visibility tidy

`AppSettingsStore` and `Opened` in `app_settings.rs` are `pub` inside a
private module: make them `pub(crate)` (keep `SettingsEffect` and
`Service::register_settings_effect` public, the app uses them).

Commit: `refactor(audit): X-10 설정 저장소 타입을 크레이트 안으로 들입니다 (#186 A10)`

## Allowed files

`crates/secs-service/src/{app_settings.rs,session.rs,jobs.rs,export.rs,export_plan.rs,export_plan_tests.rs,results.rs,measurement.rs,mic_calibration.rs,lib.rs,project_edit.rs,monitor.rs}`,
`crates/secs-service/src/measure/mod.rs`, `crates/secs-service/tests/**`,
`crates/secs-project/src/{record.rs,lib.rs}`, `crates/secs-project/tests/**`,
`apps/secs2r/src/local_api.rs`, `apps/secs2r/src/main.rs` (only the effect
adapter), `features.toml` (registrations for tests you add, rename or
delete). Nothing else. If a fix needs another file, stop that step and
report it.

## Verification (foreground, in this order, all with the target dir above)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-service -p secs-project -p secs-types -p secs2r -p secs2r-cli -p sound-equalization-and-correction-system-rust-api --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-project -p secs-types -p secs2r -p secs2r-cli --locked
cargo test -p sound-equalization-and-correction-system-rust-api --all-features --locked
cargo +nightly-2026-10-01 miri test --locked -p secs-project --lib
cargo +nightly-2026-10-01 miri test --locked -p secs-project --test golden -- wav_reader truncated_wav_data
python3 .github/scripts/check_feature_tests.py
```

The feature check may list only the two Windows blind spots
(`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`). Run the two tests that
exercise the effect drainer and the settings-written-while-running case
twenty times each (`--test-threads=1` is not required; use a shell loop) and
report any flake.

## Report

Per step: the Red test name and its first failure line, the commit hash,
files, what was deleted. Then the verification output tails, the public
interface changes (secs-project: `relocated`, `RecordError`,
`source_parse_count` instance method, removed global; secs-service: the
measure module visibility), anything blocked with the reason, and
`git log --oneline fix/audit-186..audit/186-svc-fix`. Finish the report
before ending your turn.
