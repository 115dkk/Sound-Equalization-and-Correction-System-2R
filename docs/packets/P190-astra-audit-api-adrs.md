# P190 (ASTRA): audit #186, the architecture ADRs on the wire types, the public API and the bindings

Tree: `E:\wt\audit-api`, a git worktree that Claude re-points to branch
`audit/186-api-adr`, created from the then-current `fix/audit-186` (after the
Phase 1 packets P183..P187 and the engine/service ADR packets P188/P189 have
been rebased into it). Check `git log -1` and `git status` before the first
edit; if the branch is not `audit/186-api-adr` or the tree is dirty, stop and
report. Every cargo command runs with `CARGO_TARGET_DIR=C:\cargo-target\audit-api`.
Commit on that branch, one commit per step below, in order. Do not push, do
not rebase, do not spawn sub-agents, do not touch `audit-state.md`.

This packet assumes the earlier commits of P184 exist on the base: `WireError`
(A-8, TD-160), `ServerEvent::Lagged` and the server frame (A-9, TD-161), the
Python `events(timeout)`/`close()` (A-1, TD-150), `secs_sys::clock::local_clock`
(A-4, TD-152), the `RESULTS` cap (A-35), `Drop for Engine` (A-36), the
`IpcError` constructors (A-37), `ServerHandle::stop` with `shutdown_timeout`
(A-40). Read those commits first (`git log --oneline fix/audit-186 -- crates/secs-api
crates/secs-local-api bindings`) and build on them; do not redo them. If one is
missing, do that item first with its P184 text and say so in the report.

Allowed files:

- `crates/secs-types/**` (run `cargo test -p secs-types --locked` after every
  change: it regenerates `apps/secs2r/ui/src/ipc/bindings/*.ts` and
  `bindings/rules.ts`; commit the regenerated files whose `git diff` has
  content changes, not the line-ending-only ones);
- `crates/secs-api/**`, `crates/secs-local-api/**`;
- `crates/secs-sys/src/{embed.rs,clock.rs,lib.rs}` and `crates/secs-sys/Cargo.toml`
  (the `secs-service` dependency ADR 0053 adds), `crates/secs-policy/tests/gates.rs`
  only for the dependency allow-list line that gate needs;
- `bindings/python/**`, `bindings/dotnet/**`;
- `apps/secs2r-cli/**`, `apps/secs2r-host-shell/src/main.rs` (ADR 0053's
  `HostPlatform` narrowing), `apps/secs2r/src/{main.rs,local_api.rs,host.rs,apply.rs}`
  and `apps/secs2r-cli/src/apply.rs` (deleted by ADR 0053);
- `crates/secs-service/src/lib.rs` only for `Deps.on_app_settings` (ADR 0059)
  and `crates/secs-service/src/project_edit.rs` only for the one call after a
  successful `app_settings_set`; `crates/secs-service/tests/recorded.rs` and
  `apps/secs2r/ui/src/ipc/recorded/*.json` (ADR 0052 recordings);
- `docs/adr/0016-public-rust-api-and-local-api.md` (the sentences ADR 0054 and
  0059 add), `docs/adr/0014-*.md` (the paragraph ADR 0052 changes),
  `docs/ARCHITECTURE.md` (the dependency arrow ADR 0053 adds), `docs/adr/0002-*.md`
  (one sentence, ADR 0053), `bindings/python/README.md`, `bindings/dotnet/README.md`,
  `docs/UNSAFE_BOUNDARIES.md`;
- `features.toml`: register every new test in the `[[feature]]` entry that
  covers the behaviour (`ipc.*`, `api.*`, `bindings.*`, `local_api.*`) or add one
  in the same style; `python3 .github/scripts/check_feature_tests.py` must accept it.

Forbidden: `crates/secs-engine/**`, `crates/secs-service/**` beyond the lines
named above, `apps/secs2r/ui/**` beyond the regenerated bindings and recorded
JSON (P192 consumes them), `CHANGELOG.md`, `docs/DEVIATIONS.md`.

Read first: `CLAUDE.md`, `CONTEXT.md`, the ADRs this packet implements
(`docs/adr/0042-engine-note-typed-value.md` stages 4 and 5,
`0054-one-public-api-for-three-bindings.md`, `0053-one-native-embedder.md`,
`0059-local-api-runtime-owned-by-crate.md`, `0052-mock-rules-as-recorded-answers.md`),
ADR 0003, 0014, 0016, 0032, `crates/secs-api/src/lib.rs` (the `Secs` trait at
150, `Observer` at 196, `wait_job` at 421, `Client::events` at 626, `Events`
and `EventsCloser` at 713-791), `crates/secs-local-api/src/lib.rs` (`ServerConfig`
78-89, `ServerHandle::stop` 147, the 1013 close at 481-487),
`crates/secs-types/src/local_api.rs` (`ServerEvent` 69), `crates/secs-types/src/rules.rs`
(the `typescript` module at 121-230 that writes `rules.ts`), `crates/secs-service/tests/recorded.rs`
(`Fresh`, `check`, the four recorders at 145-205), `apps/secs2r/src/local_api.rs`
(`Manager`, `lifecycle` 17, `apply_revision` 96, `local_error` 204),
`apps/secs2r/src/main.rs:68-110, 176-204` (the `Deps` block and the
`AppSettingsSet` special case at 74-80), `apps/secs2r-cli/src/main.rs:587-627`
(`deps()`), `apps/secs2r/src/apply.rs` and `apps/secs2r-cli/src/apply.rs`
(the two `NativeApply`/`NativeManagedHost` copies), `bindings/python/src/lib.rs`,
`bindings/dotnet/native/src/{core.rs,ffi.rs}`.

## Why

The monthly audit (issue #186) reviewed the architecture and the maintainer
decided (2026-11-06) to implement every candidate, each behind an ADR. These
are the candidates whose deepened module is the wire, the public API or an
adapter the apps share. Each step is a Category A "deepening" item of
`/115dkk-skills:resolve-audit`: the tests are written at the deepened
interface first, the behaviour moves behind it until they pass, and the
shallow tests the new ones cover are deleted in the same commit. Expected
values come from literals, the ADR text or recorded files, never from the
code under test.

## The protocol for every step

1. Announce the step in the report (`── X-n step k: … ──`).
2. **Red.** Write the tests named below at the deepened interface. Stub the
   new items so the tests compile, run them, and quote the failing assertion.
3. **Green.** Move the behaviour behind the interface until they pass. Delete
   the shallow tests the ADR names as covered. Do not leave both.
4. **Tidy.** Only the lines this step wrote.
5. Gate: `cargo fmt --all -- --check`,
   `cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`,
   `cargo test -p <touched crates> --locked`, `cargo test -p secs-types --locked`
   when a wire type changed (bindings), `(cd apps/secs2r/ui && npm run typecheck)`
   when `bindings/*.ts` changed (Node 24; `npm ci` once if `node_modules` is
   missing). Every fifth commit and at the end `cargo test --workspace --locked`
   in the foreground.
6. Commit with `git add <the files you touched>` (never `git add -A`) and:

   ```
   refactor(audit): X-n <한국어 한 문장> (#186 A<n>, ADR 00NN)

   <깊어진 interface에서 어떤 시험을 먼저 썼고 무엇이 그 뒤로 옮겨 갔는지 한두 문장.>

   Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>
   ```

   Use `fix(audit): A-n …` for the two steps that are also TD findings with a
   behaviour change the P184 commits did not cover (named below). Korean
   sentences: 하십시오체, no em-dash, no program-as-subject verbs
   (묻다/적다/넘기다 → 표시합니다/기록합니다/전달합니다), "추가하다" not "더하다".
7. If a step needs a file you may not edit, stop that step, keep its Red
   tests `#[ignore]`d with a comment naming the dependency, commit nothing for
   it, and report it.

## Steps

### X-13 · ADR 0054 · one public API carries the three bindings

Step 1 (`Secs` trait): add `wait_job` and `subscribe` to `pub trait Secs`
(`crates/secs-api/src/lib.rs:150`) exactly as the ADR writes them:

```rust
pub trait Secs {
    fn call(&self, request: Request) -> Result<Response>;
    fn wait_job(&self, job: JobId, timeout: Duration) -> Result<JobEvent>;
    fn subscribe(&self) -> Result<Subscription>;
}
pub struct Subscription { pub receiver: mpsc::Receiver<Event>, pub dropped: DroppedEventCount, pub closer: SubscriptionCloser }
pub enum Event { Job(JobEvent), Changed { method: String }, Lagged { missed: u64 }, Closed { reason: String } }
```

`Engine::subscribe` is today's `subscribe_with_drop_count` (queue 1024, drop
on full, never drop terminal events, `Lagged`, `Closed`); `Client::subscribe`
moves the pump thread from `bindings/dotnet/native/src/core.rs:22-63, 254-336`
into the crate, maps the server's `lagged` frame to `Event::Lagged` and the
1013 close to `Event::Closed`. `Error` gains `Timeout { job: JobId }` and
`JobNotFound { job: JobId }`; `Display` prints the snake_case code
(`timeout`, `job_not_found`, `transport_error`, `protocol_error`, and the
`IpcError` code for `Api`), never the Debug name. `wait_job` on `Client`
subscribes and waits; a job absent from `/v1/status`'s running list answers
`JobNotFound`; a timeout answers `Timeout`, not `Protocol`.

Red tests in `crates/secs-api/tests/public_api.rs`, parameterised over
`Engine` and a `Client` connected to a local API server started in the test
(the crate's tests already start one; reuse that helper):
`subscribe_delivers_job_events_in_order_on_both_adapters`,
`slow_engine_subscriber_counts_dropped_events_and_slow_client_receives_lagged`
(engine: `dropped.get() >= 1` after producing 1024 + 5 events without reading;
client: the next event after the stall is `Event::Lagged { missed }` with
`missed >= 5`), `wait_job_answers_finished_unknown_and_timeout_the_same_way`
(finished job → its terminal `JobEvent`; unknown id → `Error::JobNotFound`;
a running job with a 100 ms timeout → `Error::Timeout`),
`closer_ends_a_blocked_recv_within_one_second`, and the string golden
`error_display_prints_snake_case_codes` with the literals
`"project_not_open: …"`, `"timeout: job 7"`, `"job_not_found: job 7"`.
Delete the dotnet pump (`SubscriptionReceiver::{Engine, Client}`,
`Dropped::{Engine, Client}`) and make `secs_events_next` one arm over
`Subscription`; delete the Python `ClientEvents` iterator in favour of one
`_Events` over `Subscription` (keeping P184's `timeout`, `close()`,
`check_signals`; `events(timeout=…)` raises `TimeoutError` per the ADR, change
P184's `StopIteration` if it chose that and say so). .NET `EventStream` passes
`Lagged` through as an event of kind `"lagged"` (`PackageTests.cs`
`EventStreamPassesLaggedThrough`); Python `test_client_events_report_lagged_td161`.

Commit: `refactor(audit): X-13 Secs trait가 구독과 잡 대기를 가지고 두 바인딩은 직렬화만 합니다 (#186 A13, ADR 0054)`.

Step 2 (`EngineBuilder` setters and the CLI): add setters `process`, `memory`,
`apply`, `managed_host`, `eqapo_install`, `clock` (taking the clock closure
and the offset closure together), `unix_ms`, `uuid`, `acceleration`, `log_dir`,
and `EngineBuilder::from_deps(Deps)`. The CLI (`apps/secs2r-cli/src/main.rs`)
uses `secs_api::Engine` for every command, deletes its own `Events` and the
`Condvar` wait (`:417-455, 632`), and prints events through `observer`.
Red: the existing CLI tests (recommend-xo, calibrate, export) keep passing
unchanged (they are the gate; name them in the report), plus
`engine_builder_from_deps_keeps_every_field` in `crates/secs-api/tests/public_api.rs`
(a `Deps` with a fixed clock and offset +32400 → the engine's `/v1/status`
or an export's `created_utc` shows the UTC instant; literal
`2024-01-02T03:04:05Z`). ADR 0016 gets the compatibility sentence from ADR
0054 ('판 호환을 약속하는 표면은 …'; `Engine::service()` and the builder's
internal-crate traits marked `#[doc = "불안정한 탈출구 …"]` or with a
rustdoc `# Stability` section).

Commit: `refactor(audit): X-13 EngineBuilder가 빠진 seam을 받고 CLI가 공개 API를 씁니다 (#186 A13, ADR 0054)`.

### X-12 · ADR 0053 · one native embedder

Step 3: `crates/secs-sys/src/embed.rs` with `OsApply`, `OsManagedHost::new(host_executable, shell: Option<ShellAutostart>)`
implementing `secs_service::{ApplyAdapter, ManagedHostAdapter}`, the
`host_executable`/`shell_executable`/`application_executable` finders moved
in, and `pub fn native_deps(NativeEmbedding { app_version, app_data_path, shell }) -> secs_service::Deps`
filling `clock`, `clock_offset_seconds` (both from `secs_sys::clock`), `unix_ms`,
`audio_backend`, `rew_transport`, `process`, `memory`, `eqapo_install`, `apply`,
`managed_host`, leaving the app-owned fields at `..Deps::default()`.
`secs-sys/Cargo.toml` gains `secs-service.workspace = true`; `secs-policy`'s
dependency gate (`crates/secs-policy/tests/gates.rs`) gets `secs-service` on
`secs-sys`'s allow-list and keeps forbidding `secs-service → secs-sys`.
Red tests in `crates/secs-sys/src/embed.rs` tests:
`native_deps_keeps_the_clock_and_its_offset_consistent` (`clock()` minus
`clock_offset_seconds()` equals `unix_ms()`'s UTC within 1 s) and
`managed_host_install_touches_the_shell_only_when_asked` (a `HostCommands`
test seam records calls; `shell: None` → no autostart/spawn calls; `Some` →
they follow the service registration). Then move the CLI (`deps()` becomes
`native_deps(..)` plus `acceleration`/`compute_override`, and
`deterministic_clock(&mut deps)` sets `clock`, `clock_offset_seconds = 0`,
`unix_ms` together), the Tauri app (`main.rs:180-204` becomes `native_deps`
plus `acceleration`, `host`, `observer`, `updates`, `app_settings_path`,
`log_dir`, `log`), and the host shell (`HostPlatform` keeps only
`set_shell_autostart`, `spawn_detached`, `main_app_running`). Delete both
`apply.rs` files. `docs/ARCHITECTURE.md` gets the `secs-sys → secs-service`
arrow; ADR 0002's sentence changes as ADR 0053 says.

Commit: `refactor(audit): X-12 운영체제 adapter와 Deps 조립을 secs-sys 한 곳에 둡니다 (#186 A12, ADR 0053)`.

### X-18 · ADR 0059 · the local API runtime is the crate's

Step 4: `crates/secs-local-api/src/runtime.rs` with `pub struct Runtime` and
`new(config_dir, service, hub, on_change, log)`, `startup`, `apply_settings`,
`status`, `regenerate_token`, `stop` (the ADR's signatures), moving
`lifecycle`, `ensure_token`, `start_locked`, `stop_locked`, `status_locked`
out of `apps/secs2r/src/local_api.rs`. `LocalApiError::Token { path, source }`
is added; the app maps `PermissionDenied` → `ErrorCode::PermissionDenied`,
other I/O → `ProjectIo`, through P184's `IpcError::io`. `Runtime::stop` and
`apply_settings` use the 2 s `shutdown_timeout` P184 gave `ServerHandle::stop`.
Red tests in `crates/secs-local-api/tests/local_api.rs` (real loopback
sockets): `runtime_startup_enabled_serves_and_disabled_only_makes_a_token`,
`runtime_apply_settings_moves_port_stops_and_keeps_an_unchanged_server`
(the four `lifecycle` cases of `apps/secs2r/src/local_api.rs:223-245` move
here and the app's test is deleted), `runtime_regenerate_token_invalidates_the_old_token`
(old token 401, new 200), `runtime_stop_returns_within_three_seconds_while_a_request_runs`
(reuse P184's slow-request double), `read_only_token_folder_is_a_token_error`
(→ `LocalApiError::Token`; the app-side mapping test
`denied_token_folder_is_a_permission_error_td208` from P184 keeps passing).
The app's `local_api.rs` shrinks to the `Runtime` construction, `on_change =
app.emit("secs://external-change", method)` and `log = AppLog::line`;
`TauriHost::{local_api_status, local_api_regenerate_token}` delegate.

Commit: `refactor(audit): X-18 로컬 API의 수명을 secs-local-api의 Runtime이 소유합니다 (#186 A18, ADR 0059)`.

Step 5 (`Deps.on_app_settings`): `pub on_app_settings: Arc<dyn Fn(RevisedAppSettings) + Send + Sync>`
on `Deps` (default no-op); `project_edit.rs`'s `app_settings_set` calls it
once after a successful save with `revised_app_settings()`. The app registers
one observer that runs `AppEffects::accept`, `icon::show` and
`Runtime::apply_settings`; delete `apps/secs2r/src/main.rs:74-80` and the
re-apply thread at `local_api.rs:145-155`. If ADR 0051's packet (P189) already
added this observer, use it and say so.
Red test in `crates/secs-service/tests/service.rs` (allowed only for this
test, appended at the end): `on_app_settings_runs_once_after_a_successful_set_and_never_after_a_refusal`
(a counter closure; one accepted `app_settings_set` → 1; a refused one → still 1).

Commit: `refactor(audit): X-18 앱 설정 변경을 Deps의 관찰자 하나가 받습니다 (#186 A18, ADR 0059)`.

### X-1 · ADR 0042 stages 4 and 5 · the engine Note on the wire

Prerequisite: stages 1–3 (P188, engine packet) are on the base: `Note::record()`
exists, `secs_types::notes` holds `Note`, `MimoNote`, `GuardCause` with named
structs. If not, stop and report; do not start stage 4 on the old types.

Step 6 (stage 4): change `CalibrationSummary.notes`/`rc` (`crates/secs-types/src/ipc.rs:666-668`),
`CalibrationSummary.notes` (`:1066`), `MimoSummary.notes` (`:1170`) and
`MimoApplied.notes` (`:1441`) from `Vec<Notice>` to `Vec<Note>`; service-made
notices stay `Notice`. Regenerate bindings (`cargo test -p secs-types`);
`apps/secs2r/ui/src/ipc/bindings/Note.ts` must appear. Update the recorded
responses (`SECS_RECORD=1 cargo test -p secs-service --test recorded`) so
`results.json` carries typed notes. Red tests: in `secs-types`
`every_note_variant_round_trips_json_with_its_snake_case_code` (per variant a
JSON literal; `code` tag matches the stage-1 record code; `dip_limit` sample
`{"code":"dip_limit","role":"L","dips":[{"hz":80,"depth_db":6.1,"fill_db":3}]}`),
in `secs-local-api` tests `calibration_summary_notes_are_typed_on_the_wire`
(`notes[0].code` and a typed field), Python
`test_calibration_notes_are_typed_td162` and .NET `CalibrationNotesAreTyped`
on the mock-room flow (`project_new sample`, `measure_configure mock`,
`measure_position`, `calibrate`). The UI side (`text.ts`, fixtures, mock) is
P192's; here only the regenerated bindings and recorded JSON are committed,
and `npm run typecheck` is allowed to fail until P192 lands: say so in the
report with the exact errors.

Commit: `refactor(audit): X-1 엔진 메모가 wire에서 typed Note가 됩니다 (#186 A1, ADR 0042)`.

Step 7 (stage 5): README of the Python and .NET packages and ADR 0016's
response examples show the typed note; the `Lagged` event, queue sizes (1024
engine, 256 server) and the 1013 rule are written in ADR 0016's local-API
section (ADR 0054). Gate: the doc examples are checked by the existing
README doctest or example test if one exists, else by reading.

Commit: `docs(audit): X-1 공개 API 문서가 typed Note와 구독 규칙을 보여 줍니다 (#186 A1, ADR 0042)`.

### X-11 · ADR 0052 · the exported tables and the recordings

Step 8: `crates/secs-types/src/rules.rs` gains `ENGINE_KEY_FIELDS`,
`MANUAL_KEY_FIELDS`, `MIMO_KEY_FIELDS` (the field-name lists `engine_key`,
`manual_key`, `mimo_key` read; the service's key functions must read these
lists so a copy cannot drift: a test `engine_key_reads_exactly_the_exported_fields`
in `crates/secs-types/src/calibration.rs` tests compares the list with the
implementation's field set), `JOB_GROUPS` (the `jobs.rs` exclusion groups;
`secs-service` imports it instead of its own table, with the test
`job_groups_match_the_exported_table`), `SCALAR_EDITS` (kind, field, limit
key, policy `clamp`/`refuse`; from ADR 0055's declaration when P189 has
landed it, else from `settings_edit.rs:18-139` and say so),
`EXPORT_FOLDER_TAGS`, `MIMO_MIN_POSITIONS`, `DESIGN_RATE`, `OUTPUT_RATES`,
`SUB_LIMITS`, `INFRA_DEFAULT_HZ`, `ALLPASS_Q_DEFAULT`, `TRANSITION_FACTORS`,
`AGREEMENT_MIN_POSITIONS`, `REPETITIONS`, `NEW_PROJECT_PRESETS`, `INPUT_HP_START`
(each the single source its TD-174 copy named; the `typescript` module's
`constant` writer handles the new shapes). `cargo test -p secs-types`
regenerates `bindings/rules.ts`. Red: the rules tests above fail before the
service reads the tables.

Commit: `refactor(audit): X-11 모의 서비스가 읽을 규칙 표를 secs-types가 내보냅니다 (#186 A11, ADR 0052)`.

Step 9: `crates/secs-service/tests/recorded.rs` records
`calibration_settings_edit.json` (one representative edit and one out-of-range
edit per `SettingEdit` kind on a new project: the accepted settings or the
refusal envelope), `results.json` and `result_lines.json` (the sample room
`project_new sample` + mock measurements at 9 positions; the default settings,
the three `rc_hi_basis` values, `rc_hi_transition` 1/2/4, a manual ceiling,
no volume, low protect, input HP, two extra bands), `export.json` (one per
`ExportKind` on the same room; folder name, file list, output rate, notes).
Rename `no_input_responses_match_the_recorded_copies` to
`responses_match_the_recorded_copies` and extend it; keep the "no path, no
time" check. Commit the JSON. (`update_transitions.json` is recorded by the
app crate's updater test and belongs to P191.) Change ADR 0014's fourth
result item as ADR 0052 writes it.

Commit: `test(audit): X-11 편집 거절, 결과, 내보내기 응답을 서비스에서 기록합니다 (#186 A11, ADR 0052)`.

## Verification (foreground; do not end the turn before they finish)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p secs-types --locked
git status --short apps/secs2r/ui/src/ipc/bindings apps/secs2r/ui/src/ipc/recorded
(cd apps/secs2r/ui && npm run typecheck)       # may fail only on the Note/rules consumers P192 owns; quote the errors
python3 .github/scripts/check_feature_tests.py
python -m pytest bindings/python/tests -q      # in the venv P184 made; if maturin develop fails, say so
dotnet test bindings/dotnet/Secs2R.Tests       # if the SDK is here
git log --oneline fix/audit-186..audit/186-api-adr
```

`check_feature_tests.py` on Windows reports two names it cannot list here
(`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`); anything else is yours.

## Report

Per step: the commit hash, the Red tests and the assertion each failed on,
what moved behind the interface, which shallow tests were deleted, gate tails,
what could not be run locally. Then: steps blocked or left for another
worker (with the exact dependency), seams or signatures you changed from the
ADR and why, the `npm run typecheck` errors P192 must fix, and the final
`git log --oneline` of the branch.
