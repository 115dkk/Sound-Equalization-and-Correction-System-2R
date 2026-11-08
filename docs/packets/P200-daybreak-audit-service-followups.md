# P200 (Daybreak): audit #186, follow-ups to the service group (P183) after review

Worktree `E:\wt\audit-svc`, branch `audit/186-svc-fix` (already created from
`fix/audit-186` at 89005e76; do not rebase, do not push). Every Cargo command
takes `CARGO_TARGET_DIR=C:\cargo-target\audit-svc`; check once that
`E:\wt\audit-svc\target` does not appear. Never run workspace-wide `cargo test`
or `cargo clippy`; run them per package (`-p`). One commit per item, titled
`fix(audit): A-n <what> (#186 TD-xxx)` with the A-n of the item below and the
trailer `Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>`.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/ARCHITECTURE.md`,
`docs/packets/P183-daybreak-audit-service-a.md` (the original items), the TD
rows in `TECH_DEBT_AUDIT.md`. The items below come from a second review of
P183's commits; each names the defect, the design, the test and the files.
Each item follows the TDD shape: write the failing test first, run it, make it
pass with the smallest change, run the package's tests, commit.

Forbidden: the upstream Python tree, `secs-types` IPC types (adding a pure
function to `secs-types` is allowed where an item says so), `apps/secs2r/ui`,
`apps/secs2r/src/update.rs`, `apps/secs2r/Cargo.toml`, `.github`, ADRs.
Register every new test in `features.toml` under the feature it proves, with
its real name; `python3 .github/scripts/check_feature_tests.py` must list no
name other than the two known Windows blind spots
(`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`). A new test must compile on
every OS (no `#[cfg(windows)]` on a registered test).

## A-2 · TD-149 · the side mdat job: one worker, per-destination, survives shutdown

Current state (`crates/secs-service/src/session.rs:244-280, 764-860`,
`crates/secs-service/src/jobs.rs:175-205`): every `ProjectSave` starts its own
`JobGroup::SideMdat` job; the group skips the "one job per group" check, so N
saves run N full WAV reads and mdat builds at once; one service-wide
`side_mdat_revision` makes a later save to path B skip the pending write for
path A (A's mdat is never written); `Service::shutdown` cancels the job, so the
app's "save, then close" flow can leave no mdat.

Design:

- Replace `side_mdat_revision` with a queue on the service:
  `Arc<Mutex<SideMdatQueue>>` with
  `struct SideMdatQueue { pending: BTreeMap<PathBuf, (u64, SideMdatWork)>, next_generation: u64, running: bool }`
  (`SideMdatWork` loses its `revisions`/`revision` fields).
- `project_save`: build the work under the session lock as today; then, under
  the queue lock, insert or replace `pending[destination]` with the next
  generation; if `running` is false, set it and start one job
  (`start_side_mdat`), otherwise return (the running job picks the entry up).
  The save reply is unchanged (`ProjectInfo`).
- The job body loops: take the first pending entry (any destination) under the
  queue lock; if none, clear `running` and finish; else load, build, then
  under the queue lock check that no newer generation for that destination is
  pending (if one is, skip the write and continue); `job.begin_commit()?`
  before the write as today; `job.cancel()` is checked between entries. A
  guard (`Drop`) clears `running` on every exit, so an error or cancel never
  strands later saves (the next save starts a fresh job and redoes the
  pending entries).
- `Service::shutdown(timeout)` (`lib.rs:770`): `cancel_all` no longer cancels
  the `SideMdat` group, and the wait covers both `Device` and `SideMdat` jobs
  (rename `wait_for_device` or add `wait_for_groups`). Document in a comment
  that a side mdat still running at the deadline is abandoned by the process
  and that `write_atomic_bytes` keeps the file whole either way.
- Keep the CLI behaviour from 89005e76 (`apps/secs2r-cli/src/runner.rs`): it
  waits for the job's `Finished`; with coalescing, a CLI save never finds a
  running job, so nothing changes there. Mention in the commit body that the
  app's close flow relies on `shutdown` waiting.

Tests (`crates/secs-service/tests/service.rs`; the uuid closure in `Deps` is
called once per input inside the job, so a closure that blocks on a channel
for its first call holds the job at a known point):

- `successive_saves_to_two_paths_write_both_side_mdats_td149`: open, save as
  A, save as B (same session, `ProjectSave { path }`), wait for both jobs'
  `Finished`, both `A.mdat` and `B.mdat` exist.
- `saves_while_a_side_mdat_job_runs_coalesce_into_one_job_td149`: block the
  first job in the uuid closure, call `ProjectSave` twice more, release; exactly
  one `JobEvent::Started` with `kind == Export` appears per *distinct
  destination run* (here: one), the final mdat reflects the last save
  (change something observable between saves, e.g. a measurement note), and
  the service's job list shows one SideMdat job at a time.
- `shutdown_waits_for_a_running_side_mdat_job_td149`: block the job, call
  `shutdown(Duration::from_secs(5))` from another thread, release after 200 ms;
  `shutdown` returns `true` and the mdat exists.

Files: `crates/secs-service/src/session.rs`, `crates/secs-service/src/jobs.rs`,
`crates/secs-service/src/lib.rs`, `crates/secs-service/tests/service.rs`,
`features.toml`.

## A-29 · TD-196 · the CamillaDSP export prepares once and installs what it prepared

Current state (`crates/secs-service/src/export.rs:531-543`): the job calls
`secs_camilla::make_config` for the cancel check, discards the result, then
`write_config` recomputes everything (FIR resampling included) after
`begin_commit`, so the slow part still runs past the point of no cancel.

Design (`crates/secs-camilla/src/lib.rs`, a published crate; adding
functions is fine, keep the existing ones working):

- `pub struct PreparedConfig` (fields private) holding everything
  `write_config_observed` computes before its first write: the bundle
  (`Vec<(PathBuf, Vec<u8>)>`), the entry path, and whether `in.raw` is to be
  written.
- `pub fn prepare_config(chain, out, name, options, mimo) -> Result<PreparedConfig, CamillaError>`:
  the first half of today's `write_config_observed` (validation, `make_config`,
  the bypass config, the bundle).
- `pub fn install_prepared(out, prepared, after_stage_write, before_install) -> Result<Vec<PathBuf>, CamillaError>`:
  the second half (`write_bundle_observed`), returning what
  `write_config_observed` returns today.
- `write_config_observed` becomes `install_prepared(out, prepare_config(..)?, ..)`;
  `write_config` keeps its signature.
- Service: `let prepared = prepare_config(..)?; job.begin_commit()?; install_prepared(folder, prepared, &noop, &noop)?`.

Tests: in `crates/secs-camilla`, `prepared_config_installs_the_bytes_write_config_writes`
(install via the two-step path and via `write_config` into two folders from the
same chain and options; every file byte-identical). The existing service test
`camilla_export_cancelled_before_commit_writes_nothing_td196` stays green.

Files: `crates/secs-camilla/src/lib.rs`, `crates/secs-camilla/tests/*.rs`,
`crates/secs-service/src/export.rs`, `features.toml`.

## A-27 · TD-262 · one validity rule for sidecar calibration curves

Current state: `crates/secs-service/src/calibration.rs:417-422, 652-656`
drops the whole `(position, role)` fingerprint when a curve fails
`MicCalibration::from_points`, and the calibrate signature is taken from the
stripped project; `crates/secs-project/src/record.rs:41-64, 104-120, 222-234`
re-attaches a fingerprint on open for any curve whose arrays merely have
equal length ≥ 2. `results.rs:454` compares the two signatures, so a project
with one unsorted meterCal is "stale" after every reopen although nothing
changed.

Design:

- `secs-types`: `pub fn calibration_curve_is_valid(freq: &[f64], gain: &[f64]) -> bool`
  in `crates/secs-types/src/calibration.rs` (or the module that already holds
  calibration helpers), the exact rule `secs_audio::MicCalibration::from_points`
  enforces (at least two points, finite values, strictly increasing
  frequencies; copy the rule from `crates/secs-audio/src/calibration.rs:210`).
  `from_points` calls it first so the two cannot drift; a `secs-audio` unit
  test checks a table of valid and invalid curves against both.
- `secs-project::attached_calibration` uses it in place of `has_curve_arrays`,
  so an invalid curve is not attached and not fingerprinted at open, record and
  status alike; the service's fingerprint maps therefore agree with the
  calibrate signature without stripping keys.
- Service: keep the `calibration_curve_invalid` notice (role, position); delete
  the key stripping (`invalid_calibration_keys`) and the commit's `remove`,
  since the fingerprint already excludes the curve. Where the runtime
  calibration signature is built, use the same attached set.

Test (`crates/secs-service/tests/service.rs`):
`reopening_after_calibrating_with_an_invalid_curve_keeps_results_fresh_td262`:
project with one unsorted meterCal sidecar, calibrate, `Results` fresh, save,
reopen, `Results`/status not stale; the notice still appears once in the
calibration summary.

Files: `crates/secs-types/src/calibration.rs`, `crates/secs-audio/src/calibration.rs`,
`crates/secs-project/src/record.rs`, `crates/secs-service/src/calibration.rs`,
`crates/secs-service/src/mic_calibration.rs` (if it computes the attached set),
`crates/secs-service/tests/service.rs`, `features.toml`.

## A-30 · TD-198 · the per-file bundle install reports its backup too

Current state (`crates/secs-graph/src/files.rs:128-300`): after a successful
install, `let _ = fs::remove_dir_all(&backup)` hides a leftover backup; when
install and restore both fail the error does not say where the previous
files are. The service's folder-replace path (A-30's first half) already
reports both.

Design (`secs-graph` is published; a return-type change is fine, the api
version bump is the release gate's):

- `pub struct BundleInstall { pub leftover_backup: Option<(PathBuf, String)> }`;
  `write_bundle_observed` returns `Result<BundleInstall, std::io::Error>`;
  when restoring fails the `io::Error` text reads
  `install failed (<install error>); restoring the previous files failed (<restore error>); they are kept in <backup path>`.
- `secs-camilla` and `secs-filters` observed writers return
  `Installed { files: Vec<PathBuf>, leftover_backup: Option<(PathBuf, String)> }`
  (one struct per crate, or one shared in `secs-graph::files`); their plain
  variants keep returning `Vec<PathBuf>` and drop the leftover (say so in
  their doc comment).
- Service: every export and apply path that installs a bundle logs
  `(LogLevel::Warn, "export", "backup folder <path> could not be removed after install: <error>")`
  through `Deps.log` when `leftover_backup` is `Some`, and keeps the job result.

Tests: `crates/secs-graph` unit test
`bundle_install_names_the_backup_when_restore_fails_td198` (make the restore
fail deterministically: e.g. after staging, `before_install` replaces the
destination entry's parent with a file so the rename back cannot succeed; use
the hooks, not sleeps); on Windows only (unregistered, `#[cfg(windows)]`)
`leftover_backup_is_reported_when_it_cannot_be_removed_td198` holding a file
open inside the backup during install. Service: extend
`backup_removal_failure_is_a_warning_not_a_job_error_td198` or add a sibling
for the camilla export path if the fake can produce a leftover.

Files: `crates/secs-graph/src/files.rs`, `crates/secs-camilla/src/lib.rs`,
`crates/secs-filters/src/lib.rs`, `crates/secs-service/src/{export.rs,design_export.rs,apply.rs}`,
tests, `features.toml`.

## A-17 · TD-172 · late progress callbacks of the previous role are ignored

Current state (`crates/secs-service/src/measure/rew.rs:269-285`): the
per-role progress callback captures the role name and keeps reporting;
`crates/secs-rew/src/client.rs:947-960, 1017, 1059` clears the current role
so a callback body that arrives after the role finished is dropped, and the
service's copy of the loop lacks that guard.

Design: a shared `current_role: Arc<Mutex<Option<String>>>` set before each
role's subscribe and cleared when the role's IR is in; the callback compares
its captured role with the current one and returns early on mismatch. Test
(`crates/secs-service/tests/measure_ipc.rs`, with the fake/recorded
transport): deliver a progress callback for role L after role R has started;
the job's `Progress`/`MeasureStatus` events after R started never name L.

Files: `crates/secs-service/src/measure/rew.rs`, tests, `features.toml`.

## A-20 · TD-183 · every unreadable sidecar warns, and later loads keep the warnings

Current state (`crates/secs-project/src/record.rs:741-745, 794-799`): a
sidecar that exists but fails `is_file()` or the confinement check is skipped
silently (only a parse failure or the budget warns), and the public
`load_project_sources` drops the warnings, so `session.rs:693` and the export
paths cannot report them.

Design:

- In `load_project_sources_within`: a missing sidecar (NotFound) stays silent
  (F23 allows absence); a sidecar whose metadata cannot be read, that is not a
  regular file, or that fails confinement yields `measurement.source.skipped`
  for that sidecar path.
- `pub struct LoadedProjectSources { pub values, pub warnings }`;
  `load_project_sources` returns it (callers in `secs-service` adjust; the
  side-mdat job and the original-measurement export forward each warning as
  `JobEvent::Log { code: warning.code, params: { "entry": warning.entry } }`;
  callers with no event channel ignore the warnings with a comment saying
  which earlier path reported them).

Tests (`crates/secs-project`, registered under the sidecar feature):
`sidecar_that_is_a_directory_is_reported_as_skipped_td183`;
`crates/secs-service/tests/service.rs`:
`side_mdat_job_reports_sidecar_warnings_td183` (break a sidecar after open,
save, the job's events carry `measurement.source.skipped` with the entry).

Files: `crates/secs-project/src/record.rs`, `crates/secs-project/src/lib.rs`
(export), `crates/secs-service/src/{session.rs,export.rs,...}`, tests,
`features.toml`.

## A-41 · TD-215 · a subscribe failure is not lost when the role then fails

Current state (`crates/secs-service/src/measure/rew.rs:269-285, 390-393, 513-525`):
the subscribe error is kept and reported only when the role succeeds; if the
sweep or the IR then fails, the job error says nothing about the subscribe.

Design: when `record_role` returns an error after a subscribe failure, add
`subscribe_error = <detail>` to the error's params before returning. Test
(`tests/measure_ipc.rs`): subscribe fails and the sweep fails; the `Finished`
error carries `subscribe_error`. The direct `measure_position` path
(`rew.rs:597-602`, `RewClient::run_position`) is out of scope; leave a
one-line comment there naming TD-215.

Files: `crates/secs-service/src/measure/rew.rs`, tests, `features.toml`.

## A-38 · TD-210 · the tutorial test proves the refusal it is named after

Current state: `tutorial_folder_lives_under_app_data_and_refuses_an_existing_path_td210`
creates the folder twice but never pre-creates a colliding folder.

Design: pre-create the first candidate folder under `app_data/tutorial` with
a marker file; start the tutorial; the session uses another folder, the
marker file is intact, and after the session ends (drop/replace) the
pre-existing folder still exists. No production change expected; if the
candidate name is not predictable, expose the naming rule to the test through
an existing seam rather than a new public function.

Files: `crates/secs-service/tests/service.rs`.

## Verification (foreground, per package)

```
CARGO_TARGET_DIR=C:\cargo-target\audit-svc cargo fmt --all -- --check
CARGO_TARGET_DIR=C:\cargo-target\audit-svc cargo clippy -p secs-types -p secs-audio -p secs-project -p secs-graph -p secs-camilla -p secs-filters -p secs-service -p secs2r-cli --all-targets --locked -- -D warnings
CARGO_TARGET_DIR=C:\cargo-target\audit-svc cargo test -p secs-types -p secs-audio -p secs-project -p secs-graph -p secs-camilla -p secs-filters -p secs-service -p secs2r-cli --locked
CARGO_TARGET_DIR=C:\cargo-target\audit-svc python3 .github/scripts/check_feature_tests.py
CARGO_TARGET_DIR=C:\cargo-target\audit-svc cargo +nightly-2026-10-01 miri test --locked -p secs-project --lib
```

Goldens of `secs-camilla` and `secs-filters` must stay byte-identical (A-29,
A-30 touch writers). If a test fails under load only, rerun it alone and say
so.

## Report

Per item: the Red test name and its first failure line, the Green commit
hash, files, what was deleted, and any deviation from this packet with the
reason. Then the verification commands with their last lines, and the list
of new notice or warning codes and params for the catalogue packet. Finish
with `git log --oneline fix/audit-186..audit/186-svc-fix`.
