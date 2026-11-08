# P193 (Daybreak): audit #186, Category B refactors in the service and project crates

Phase 2 of `/115dkk-skills:resolve-audit` for issue #186. This packet runs
AFTER Phase 1 and every ADR implementation packet (P188..P192, P189, P203,
P202) have landed on `fix/audit-186`. Claude created the worktree
`E:\wt\audit-svc` on a fresh branch `audit/186-svc-b` from the current
`fix/audit-186` (`git status` is clean when you start). Work only there.
Every cargo command runs with `CARGO_TARGET_DIR=C:\cargo-target\audit-api`
(a warm target; another worker on the ADR 0055 packet may share it, so
"Blocking waiting for file lock" can appear: wait). Never run `cargo clean`,
never delete anything under `C:\cargo-target`, and check after the first
command that `E:\wt\audit-svc\target` does not exist. No workspace-wide
`cargo test`; per-package (`-p`) only, plus one `cargo clippy --workspace`
at the end. Commit on that branch, one commit per finding, in the order
below. Do not push, do not rebase, do not spawn sub-agents, do not touch
`audit-state.md`.

Since this packet was written, P189, P203 and P202 changed the service a
lot: `app_settings.rs` (the settings store), `export_plan.rs` and the six
concrete writers in `export.rs`, `ProjectSnapshot` and `commit(.., Finality)`
in `session.rs`, `JobContext::role_progress`, the source map and
`RecordError` in `secs-project`, a private `measure` module with root
re-exports, `secs_sys::native_deps` (X-12). Line numbers below are stale;
search by the names, and read the current code of each file before you
change it.

Allowed files:

- `crates/secs-service/**` (source and tests);
- `crates/secs-project/**`;
- `crates/secs-audio/**` and `crates/secs-rew/**`, for X-26 only;
- `features.toml`, only to rename a test that moved (never to add or
  remove a test name; `python3 .github/scripts/check_feature_tests.py` must
  still accept it).

Forbidden: everything else, in particular `crates/secs-types/**`,
`crates/secs-engine/**`, `crates/secs-graph/**`, the writers, the screens,
`docs/**`, `CHANGELOG.md`.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/ARCHITECTURE.md`,
`docs/adr/0044-open-project-edit-interface.md`,
`docs/adr/0048-one-commit-and-snapshot-set.md`,
`docs/adr/0049-job-registry-owns-cancel-and-kind.md`,
`docs/adr/0060-measurement-store-keeps-source-info.md` (what P189 already
changed; your items build on it), the finding texts quoted below, and the
current code of every file an item names (line numbers below are from the
audit commit `97a27818`; after Phase 1 and the ADR packets they have moved,
so search by the function names given).

## Why

Category B items are pure refactors: the observable behaviour of the
service (wire answers, files written, events, timing of commits) must be
identical before and after. The existing test suite is the only gate. You
write no new tests in this packet. If any existing test has to change for
an item, the item is not a refactor: undo it, leave the tree as it was, and
report the item for reclassification to Category A (Claude processes it
test-first later).

## The protocol for every item

1. Announce the item in the report (`── B-n: … ──`).
2. **Before.** Run the suite that covers the touched crates and record the
   counts (`cargo test -p <crates> --locked 2>&1 | grep "^test result"`;
   sum passed/failed/ignored). Before the first item and after every fifth
   commit run `cargo test --workspace --locked` and record the totals.
3. **Change.** Only the structural change the item names. No new tests, no
   behaviour change, no drive-by cleanup outside the item's files.
4. **After.** Run the same suite. Passed must be ≥ before, failed 0, and the
   set of test names must be the same (plus renames the item names). The
   goldens of `secs-service` (`oracle/goldens/service_*` through
   `tests/*.rs`) must pass unchanged.
5. **Stop rule.** A test that fails or has to be edited means the change
   altered behaviour or a test depends on internal structure. Undo the
   change (`git checkout -- <files>` for the files you touched; never
   `git checkout .`, the tree has generated files with line-ending churn),
   report the item as `RECLASSIFY → A` with the failing test name and the
   assertion, and move on.
6. Gate: `cargo fmt --all -- --check`,
   `cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`.
7. Commit with `git add <the files you touched>` and this message shape
   (Korean, 하십시오체, no em-dash, no program-as-subject verbs such as
   묻다/적다/넘기다, "추가하다" not "더하다"):

   ```
   refactor(audit): B-n <한국어 한 문장> (#186 TD-xxx)

   <무엇을 어디로 옮겼는지 한두 문장. 동작은 바뀌지 않았고 시험 N개가 전후 같이 통과했다는 문장.>

   Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
   ```

   Architecture deletions use the X id: `refactor(audit): X-n <한국어> (#186 A<n>)`.

## Covered elsewhere (verify only)

These rows of `audit-state.md` are closed by ADR packets. Do not redo them;
check in the report that the current tree matches and note any leftover:

- B-1 (TD-155) → P189 0ebbf415: `plan_export` reads the output-rate choice under the lock, resolves it with the device outside, and re-enters through `with_open_ticket`. Verify only; the one thing left for you is in the B-1 section below.
- B-9 (TD-184 sidecar reparse) → ADR 0060, P189.
- B-11 (TD-189 transitions) → ADR 0057, P191.
- B-13 (TD-251 testkit copy) → ADR 0050, P188.
- B-16's `export.rs` part (TD-195) → ADR 0043, P189. The other three parts of TD-195 are in P194.
- B-18's progress reporters (`report_progress`/`role_progress`, A24) and the
  `host.rs` pass-through and `curves::dispatch` deletions (A20, A21) → ADR
  0047 and 0049, P189. Only the remainder listed below is yours.
- B-19 (TD-213 relock and the nine-argument MIMO call) → ADR 0044 and 0048, P189.
- X-22 (A22 RecommendXo) → A-7, P184.
- B-4, B-5, B-6, B-28, B-29 (screen) → P192.

## Items

### B-1 · TD-155 · `start_export_with` holds the session lock while it enumerates devices and reads sidecars

Finding: "`start_export_with`가 107~426행 동안 세션 잠금을 쥔 채 운영체제
오디오 장치를 열거하고(`resolve_output_rate`, 195-199), 모든 `.src.json`
사이드카를 디스크에서 읽으며(336, 프로젝트 예산 64 MiB까지), `MimoPaths`(132×8,000탭
≈ 8.4M f64)를 최대 세 번, 사슬을 두 번 복제합니다. 장치 열거가 멈추면 모든
요청이 막힙니다. 잡이 생기기 전이라 취소도 닿지 않습니다." Recommendation:
read only `dsp.output_rate` under a short lock and resolve it outside; wrap
the FIR-carrying values in `Arc` so export, manifest and description share
one copy; capture sidecar paths under the lock and read them in the job
(like `original_snapshot`).

After ADR 0043 (P189) the function is `Service::plan_export` plus the
writer adapters, ADR 0060 keeps the sidecars in the store, and P189 moved
the device enumeration out of the lock (0ebbf415). What remains for this
item is only the copies:

- Before: in `crates/secs-service/src/export.rs`, `Arc::new(cached.calibration_chain.clone())`, `Arc::new(result.run.paths.clone())`, the `mimo.clone()` / `export_mimo.clone()` pair and the `paths.clone()` in the cached replay (search those strings).
- After: where the cloned value is a whole `SignalChain` or `MimoPaths` (not an `Arc`), the source keeps it in an `Arc` so the plan, manifest and description share one copy (`ResultBook` or the cached result may need an `Arc` field; that is inside `crates/secs-service`). Where the clone is already an `Arc` clone, leave it and say so.
- Deleted: the whole-value copies only.

Files: `crates/secs-service/src/export.rs` (and the writer adapter files P189 created under `crates/secs-service/src/export/` if that is where they live).
Gate: `cargo test -p secs-service --locked` (the export and device-fallback tests `output_rate_resolver_*` and `*export*` must pass unchanged).
Commit subject: `refactor(audit): B-1 내보내기 계획이 사슬과 MIMO 경로를 복제하지 않고 공유합니다 (#186 TD-155)`. If nothing whole-value is left, make no commit and say so in the report.
Stop rule: if `resolve_output_rate` outside the lock changes which rate an existing test observes (a test fakes a device change between calls), reclassify.

### B-2 · TD-156 · `measurement_curves` reads every WAV under the lock and smooths without cancel or progress

Finding: "`measurement_curves`가 측정 화면의 그래프를 새로 그릴 때마다 저장된
모든 WAV를 세션 잠금 안에서 읽고(`measured_ir`, 응답 보정 포함), 평활은 잠금
밖에서 새 `CancelToken::new()`와 `NoProgress`로 하므로 취소도 관찰도 되지
않습니다. TD-37은 보정 읽기만 잠금 밖으로 옮겼습니다." Recommendation: capture
the list under the lock and read the files after it (like
`MeasurementSnapshot`, now `ProjectSnapshot::load_measurements` after ADR
0048), use a memory budget, and keep a per-`(uuid, settings)` smoothed-curve
cache because the same IR is re-read on every position change.

- Before: `curves.rs` `fn measurement_curves` takes the lock, loops `open.store.records()`, calls `crate::calibration::measured_ir(&mut open.store, role, record.pos)?` per record, then smooths with `Arc::new(secs_compute::NoProgress)` and `secs_compute::CancelToken::new()`.
- After: under the lock take `ProjectSnapshot` (ADR 0048) plus the records list and the calibration fingerprints; drop the lock; load the IRs through `snapshot.load_measurements(..)` with the `compute_context` budget the export job uses; smooth; store the smoothed curves in a `curves_cache: Mutex<HashMap<(Uuid, CurveSettingsKey), Arc<Curve>>>` on `Service` keyed by the measurement uuid and the smoothing settings, invalidated where `session::commit` replaces records (the same hook ADR 0060 uses for `sources`). The cancel token stays `CancelToken::new()` (this request has no job; a cancellable curves job is not this item) but the progress sink and the budget come from `compute_context`.
- Deleted: the per-record `measured_ir` call under the lock and the duplicate smoothing of an unchanged IR on the next call.

Files: `crates/secs-service/src/curves.rs`, `crates/secs-service/src/lib.rs` (the cache field), `crates/secs-service/src/session.rs` (invalidation hook only).
Gate: `cargo test -p secs-service --locked` (`measurement_curves_match_oracle_*` and the curves golden must pass bit-identical: the cache returns the same `Arc` content; `curves_and_lines_*` tests too).
Commit subject: `refactor(audit): B-2 측정 곡선을 잠금 밖에서 읽고 평활 결과를 캐시합니다 (#186 TD-156)`.
Stop rule: any curve byte difference or a test that counts file reads → reclassify.

### B-3 · TD-167 · `tests/service.rs` is 4,876 lines with 62 tests and duplicated helpers

Finding: "4,876줄 한 파일에 시험 62개. 가장 긴 다섯(270, 182, 325, 270, 250줄)은
각각 생성→보정→MIMO→내보내기→다시 열기를 잇고 있어 한 단계의 실패가 나머지를
가리고, 내보내기 종류 하나를 바꾸면 다섯이 모두 다시 돕니다. `impl EventSink for
Events`와 `service()`·`temp_root` 같은 도우미가 시험 파일 아홉에 다시 선언됩니다."
Recommendation: a shared `tests/support/mod.rs` (the helpers of lines 1-200,
`write_case`, `start_calibration`), split into `tests/session.rs`,
`tests/calibration.rs`, `tests/export.rs`, `tests/cache.rs`, and cut the five
long tests at their first `reopen`.

This is test-file restructuring. The gate is that the set of test names and
their outcomes is identical before and after (a moved test keeps its name).

- Before: `crates/secs-service/tests/service.rs` (4,876 lines) and the helper copies in `tests/{apply,measure_ipc,monitor,config,mdat,measure,host,rew_info,settings_edit,tutorial,watch}.rs` (search each for `fn service(`, `fn temp_root(`, `impl EventSink for`).
- After: `tests/support/mod.rs` (`#[path]`-free: `mod support;` in each test crate file) holding `service()`, `deterministic_service()`, `Events` and its `EventSink` impl, `temp_root()`, `write_case`, `start_calibration`, `wait_for`, and whatever two or more files duplicate; `tests/service.rs` keeps only the tests that exercise `Service::call` dispatch and project lifecycle; new files `tests/session.rs` (open/save/replace/undo), `tests/calibration.rs` (calibrate, MIMO, results), `tests/export.rs` (every export kind, manifest, index), `tests/cache.rs` (cache versions and migrations). The five long tests are split at their first `reopen` into two tests each; the second half starts from the saved project the first half wrote (write it to the temp root under a fixed name so the second test can open it; if the two halves cannot be made independent without re-running the first, keep the test whole and say so).
- Deleted: the duplicated helper definitions; nothing else.
- `features.toml`: tests that moved keep their names, so the registry needs no change unless a name included the file (it does not: entries are `secs-service::<test name>`).

Gate: before and after, `cargo test -p secs-service --locked -- --list 2>/dev/null | grep ': test$' | sort` must be identical except for the five split tests (each becomes two names; list the new names in the report), and the counts match (+5).
Commit subject: `refactor(audit): B-3 서비스 통합 시험을 공용 도우미와 주제별 파일로 나눕니다 (#186 TD-167)`.
Stop rule: any test outcome change → undo the split that caused it.

### B-18 (remainder) · TD-207 · `monitor.rs` copies and the condvar `unwrap`

Finding (the parts not covered by ADR 0047/0049): "`wait_for_device`는
condvar 결과에 `.unwrap()`(304)을 쓰는데 크레이트의 다른 잠금은 poison을
견딥니다. `monitor.rs`의 `level_error`·`method`·`input`은 `measurement.rs`의
이름만 다른 사본입니다."

- Before: `crates/secs-service/src/jobs.rs` `wait_for_device` → `self.changed.wait_timeout(inner, remaining).unwrap()` (and the sibling `wait_timeout(..).unwrap()` / `wait(..).unwrap()` sites at the lines the grep below finds); `crates/secs-service/src/monitor.rs` `fn method(project)`, `fn input(field, detail)`, `fn level_error(..)` duplicate `crates/secs-service/src/measurement.rs` `fn method`, `fn input` (and its level error helper).
- After: the condvar results use `unwrap_or_else(|poisoned| poisoned.into_inner())` like `lock()` at the end of `lib.rs` (same comment: the job catches panics at the thread boundary, so the value is whole); `measurement.rs`'s `method`, `input` and the level error helper become `pub(crate)` and `monitor.rs` imports them. This is X-23 (A23) as well: one commit for TD-207's remainder, which closes A23 (say so in the body).
- Deleted: the three copies in `monitor.rs`.

Grep first: `grep -n "unwrap()" crates/secs-service/src/jobs.rs` and keep only the condvar ones in scope (the `unwrap()`s inside `#[cfg(test)]` stay).
Files: `crates/secs-service/src/jobs.rs`, `monitor.rs`, `measurement.rs`.
Gate: `cargo test -p secs-service --locked`.
Commit subject: `refactor(audit): B-18 모니터의 도우미 사본을 지우고 condvar 결과가 poison을 견딥니다 (#186 TD-207)`; the body cites `A23`.

### X-25 · A25 · one atomic-write helper that takes the error code

Finding: "`secs_types::fs::write_atomic_bytes`로 넘기는 도우미 넷과 자기
원자 교체 셋(TD-74는 둘만 고침)." Solution: "`ErrorCode`를 인자로 받는 도우미
하나." The service wrappers today (2026-11-08, `grep -rn write_atomic_bytes crates/secs-service/src`) are `app_settings.rs::atomic_write` and `project_edit.rs::atomic_write` (both map with `project_io`), `deploy.rs` and `apply.rs` (map with `io_error`), and `session.rs::atomic_replace` if it still exists; the direct calls in `design_export.rs` and `export.rs` stay direct unless the helper fits them without changing the error they return. The writer-crate copies (`secs-camilla::atomic_write`, `secs-export::atomic_replace`, `secs-project::record::atomic_write`) belong to ADR 0045/P194, not to you.

- Before: four private wrappers in four files, two error mappers.
- After: `crates/secs-service/src/fs.rs` with `pub(crate) fn write_atomic(path: &Path, bytes: &[u8], code: ErrorCode) -> Result<(), IpcError>` (calls `secs_types::fs::write_atomic_bytes`, maps the `io::Error` with the existing `io_error`/`project_io` shape: `path` param, `reason` from `io::ErrorKind` as those mappers already do; keep each call site's current `ErrorCode` exactly: `ProjectIo` for `project_edit.rs`/`session.rs`, the apply code for `deploy.rs`/`apply.rs`). The four call sites call it; `session.rs`'s `io::Result` caller maps at the call site as it does now.
- Deleted: the four wrappers.

Files: `crates/secs-service/src/{fs.rs (new), lib.rs (mod line), project_edit.rs, session.rs, deploy.rs, apply.rs}`.
Gate: `cargo test -p secs-service --locked` (the atomic-write tests of `tests/apply.rs` and `tests/service.rs` must keep their error codes and `params`).
Commit subject: `refactor(audit): X-25 원자적 쓰기 도우미 하나가 오류 코드를 인자로 받습니다 (#186 A25)`.
Stop rule: an error code or `params` change in any test → reclassify.

### X-26 · A26 · delete the `Transport` newtype and `CachedBackend`, share `rew_process_running`

Finding: "`Box<dyn Transport>` newtype(pass-through), 생성자가 한 번도 불리지
않는 `CachedBackend`, REW 프로세스 검사 두 벌." Solution: "newtype과
`CachedBackend` 삭제, `rew_process_running(&dyn ProcessProbe)` 공유."

- Before: `crates/secs-service/src/measure/rew.rs:9` `pub(crate) struct Transport(Box<dyn secs_rew::Transport>);` with a forwarding `impl secs_rew::Transport for Transport`; `crates/secs-audio/src/cached_backend.rs` `pub struct CachedBackend` whose constructor has no production caller (confirm with `grep -rn "CachedBackend::" crates apps bindings` outside its own file; if a caller exists, keep it and report); `crates/secs-rew/src/launch.rs:22` `pub fn rew_process_running(probe: &impl ProcessProbe) -> bool` and a second process check in `crates/secs-service/src/measurement.rs` (around the `rew_launch`/`rew_info` handlers, lines 169-177 at the audit commit).
- After: `measure/rew.rs` uses `Box<dyn secs_rew::Transport>` directly; `cached_backend.rs` is deleted together with its `mod` line, re-export and its own tests (they test only the deleted type; deleting a test file is allowed here because the type goes with it, and the report lists the deleted test names); `measurement.rs`'s copy calls `secs_rew::launch::rew_process_running` with the service's `ProcessProbe` (make the launch function `pub` if it is not reachable; `secs-rew` is a published crate, so a new `pub` item is additive).
- Deleted: the newtype, `cached_backend.rs`, the second process check.

Files: `crates/secs-service/src/measure/rew.rs`, `crates/secs-service/src/measurement.rs`, `crates/secs-audio/src/{lib.rs, cached_backend.rs}`, `crates/secs-rew/src/launch.rs` (visibility only).
Gate: `cargo test -p secs-service -p secs-audio -p secs-rew --locked`; `cargo publish --dry-run` constraints: no new file reads outside a crate.
Commit subject: `refactor(audit): X-26 쓰이지 않는 Transport newtype과 CachedBackend를 지우고 REW 프로세스 검사를 공유합니다 (#186 A26)`.
Stop rule: a production caller of `CachedBackend` → keep it, report.

### X-28 · A28 · `MockMeasurer` through the `audio_backend` seam

Finding: "`MockMeasurer`는 `NativeMeasurer` + `SimRoom` + 12줄인데 `Deps`에
`mock_measurer` 공장을 따로 가짐." Solution: "`"mock"`을
`NativeMeasurer::from_backend(sample::room(..))`로 돌리고 `Deps.mock_measurer`를
`audio_backend` seam에 합침."

- Before: `crates/secs-service/src/lib.rs:392` `pub mock_measurer: Arc<dyn Fn() -> measure::MockMeasurer + Send + Sync>` (default `MockMeasurer::new`), `crates/secs-service/src/measure/sample.rs:242-359` `MockMeasurer` with its own `Measurer` impl; the `"mock"` method branch in `measurement.rs` calls `deps.mock_measurer()`.
- After: the `"mock"` branch builds `NativeMeasurer::from_backend(Box::new(sample::room(..)))` (or whatever the native measurer's constructor over an `AudioBackend` is called; the SimRoom backend lives in `secs-audio`), keeping the mock's commit boundary behaviour (`MockMeasurer::new().commit_boundary()` at `sample.rs:369` shows what the tests rely on; if the boundary cannot be expressed through the backend seam, keep `MockMeasurer` as a thin `pub(crate)` wrapper that only adds the boundary and still delete the `Deps` field). `Deps.mock_measurer` is removed; the three places that construct `Deps` (`lib.rs` default, `apps/secs2r-cli`, `apps/secs2r`) are outside your files except `lib.rs`: if the apps set the field explicitly, stop and report instead of editing them.
- Deleted: the `Deps` field and the `Measurer` impl for `MockMeasurer` (or the whole type).

Files: `crates/secs-service/src/lib.rs`, `crates/secs-service/src/measure/{sample.rs, mod.rs}`, `crates/secs-service/src/measurement.rs`.
Gate: `cargo test -p secs-service --locked` (every `mock` measurement test in `tests/measure_ipc.rs` and `tests/measure.rs` unchanged).
Commit subject: `refactor(audit): X-28 모의 측정기가 audio_backend seam을 거칩니다 (#186 A28)`.
Stop rule: any changed mock measurement result → reclassify.

### B-20 · TD-216 · the whole mdat/Dirac import runs inside the commit closure

Finding: "mdat·Dirac 가져오기 전체가 `session::commit` 클로저 하나 안에서
돌아, 행마다 f64→f32 변환, WAV 쓰기, 사이드카 쓰기, `.secs` 저장이 모두 세션
잠금 아래에서 일어납니다." Recommendation: call `reader.take()` for every
assigned row before `commit` (the file is already read outside the lock
since TD-37) and keep only `store.record` inside.

- Before: `crates/secs-service/src/measurement.rs` (the `start_mdat`/Dirac job bodies, `session::commit(&session, ticket, &ctx, |open| { … measurement_file::import(.., reader, ..) .. })` at the audit's 947-954) and `crates/secs-service/src/measurement_file.rs::import` (108-157) which calls `reader.take(index)?` then `store.record(..)` per row inside the closure.
- After: `import` is split into `prepare(reader, assignments, ..) -> Result<Vec<PreparedRow>, MeasureError>` (takes every IR, converts to f32, builds the source info; no store access) and `record(store, prepared) -> Result<Count, MeasureError>` (only `store.record` per row). The job body calls `prepare` before `session::commit` and `record` inside it. The existing unit tests of `measurement_file.rs` (`import_skips_rows_outside_the_layout_the_plan_or_the_once_rule_f18`, `import_records_each_ir_with_its_source_and_saves_once_per_ir`, `import_refuses_an_assignment_count_that_does_not_match_the_rows`) must pass with `import` kept as a thin `prepare` + `record` wrapper so their call shape does not change.
- Deleted: nothing but the inline body.

Files: `crates/secs-service/src/measurement.rs`, `crates/secs-service/src/measurement_file.rs`, `crates/secs-service/src/{mdat_import.rs, dirac_import.rs}` (only if their `import` entry points wrap `measurement_file::import`).
Gate: `cargo test -p secs-service --locked` (import goldens `service_mdat_*` and the F2/F18 tests unchanged).
Commit subject: `refactor(audit): B-20 가져오기의 IR 변환을 세션 잠금 밖으로 옮깁니다 (#186 TD-216)`.
Stop rule: a test that observes partial recording after a mid-import failure (F2 semantics) must behave the same: if it changes, reclassify.

### B-15 · TD-264 · `mdat_scan` parses the whole file synchronously and twice, with a local FFT copy

Finding: "`mdat_scan`이 IPC 호출 위에서 동기로 256 MiB까지 읽어 mdat 전체를
파싱하고 행마다 분류기를 돌립니다(잡 없음, 취소 없음). `start_mdat`은 같은 파일을
다시 읽고 파싱합니다. … `scan_loaded`는 `get_ir`로 모든 IR을 복제해 손으로 쓴
radix-2 FFT를 두 번의 통과에서 모두 돌리는데, 그 FFT는 `secs_dsp::fft::rfft`의
사본입니다." Decision (open question 17): keep `mdat_scan` synchronous.

- Before: `crates/secs-service/src/measurement.rs::mdat_scan` (828) reads and parses; `start_mdat` (864) reads and parses again; `crates/secs-service/src/mdat_import.rs::scan_loaded` (285) copies every IR with `get_ir` and runs a local radix-2 FFT (the function above `scan_loaded`, lines 135-188 at the audit commit).
- After: `Service` keeps a `scanned_mdat: Mutex<Option<(PathBuf, u64 /*len*/, SystemTime /*mtime*/, Arc<LoadedMdat>)>>`; `mdat_scan` stores the parsed reader there and `start_mdat` reuses it when path, length and mtime match (otherwise it parses as today); the local FFT is replaced by `secs_dsp::fft::rfft` (same length rule: pad to the same power of two the local code used so the classifier's magnitudes are bit-identical; if `secs_dsp::fft::rfft` plans differently and the classification of any golden row changes, stop and reclassify); the classifier runs once per row in the scan and the import pass skips it (it only needs the assignments).
- Deleted: the local FFT (about 50 lines) and the second parse.

Files: `crates/secs-service/src/measurement.rs`, `crates/secs-service/src/mdat_import.rs`, `crates/secs-service/src/lib.rs` (the field).
Gate: `cargo test -p secs-service --locked` (the `service_mdat` goldens pin the scan rows and the classification).
Commit subject: `refactor(audit): B-15 mdat 검색이 파싱한 reader를 가져오기에 다시 쓰고 FFT는 secs-dsp 것을 씁니다 (#186 TD-264)`.
Stop rule: any golden row classification change → reclassify (the FFT replacement is the risk).

### B-10 · TD-187 · four copies of a 256 MiB IR

Finding: "256 MiB IR 하나에 사본이 넷입니다(`read_bounded` 버퍼,
`payload.to_vec()`, `Vec<f32>`, `IrCache::insert`의 `to_vec`). 캐시 적중마다
샘플 벡터 전체를 복제합니다." Recommendation: `wav_chunks` returns a
`Range<usize>` into `data` instead of `to_vec`, and the cache value is
`Arc<[f32]>` so a hit is a reference-count increment.

- Before: `crates/secs-project/src/irs.rs:152` `b"data" => payload = Some(data[header_end..end].to_vec())`; `record.rs::IrCache::insert(.., samples: &[f32], ..)` stores `samples.to_vec()` (955) and `get` clones the vector for the caller.
- After: `WavChunks.payload: Range<usize>` (callers slice `&data[range]`), `CacheValue.samples: Arc<[f32]>`, `IrCache::get` returns `Arc<[f32]>` (or a borrowed slice where the caller only reads), `MeasurementStore::ir(..)`/`measured_ir` callers in `secs-service` take the `Arc` (if a caller needs an owned `Vec<f32>`, it converts at that one site). The LRU accounting uses `samples.len()` as before.
- Deleted: the two `to_vec` copies.

Files: `crates/secs-project/src/{irs.rs, record.rs}`, the callers in `crates/secs-service/src/{calibration.rs, curves.rs, mic_calibration.rs}` (signature follow-through only).
Gate: `cargo test -p secs-project -p secs-service --locked` (IR goldens and the cache tests `ir_cache_*` unchanged).
Commit subject: `refactor(audit): B-10 IR을 읽을 때의 사본 넷을 하나로 줄입니다 (#186 TD-187)`.
Stop rule: a cache test asserting an owned type or copy count → reclassify.

### B-22 · TD-218 · JSON helper and Python-repr copies

Finding: "`JsonExt::{get,str,object,object_mut}`가 `secs-project`에서
`pub(crate)`인 `PyJson` 메서드를 다시 구현하고, `MeasurementSettings`의 손
`Clone`은 `secs_mdat::CalFile`이 `Clone`을 derive하지 않아서입니다. 상한 있는
읽기 `take(limit+1)` 관용구가 둘(`read_bounded`가 `pub(crate)`),
`python_repr`/`python_str`이 세 벌(이스케이프 규칙이 다름)에 파이썬 float
포매터가 둘, 토큰 파일은 unix 전용 임시 파일+rename을 따로 구현합니다."
Scope here (the rest is other crates): the `PyJson` accessors, `read_bounded`,
and the `python_repr`/`python_str` copies inside `secs-project` and
`secs-service`.

- Before: `crates/secs-project/src/pyjson.rs:26-69` `pub(crate) fn object/array/str/bool/get/object_mut/is_truthy`; `crates/secs-service/src/measure/json.rs::JsonExt` reimplementing four of them; `crates/secs-project/src/irs.rs:303` `pub(crate) fn read_bounded`; `crates/secs-service/src/measurement_file.rs:66` `pub(crate) fn read(path, what)` with the same `take(limit+1)` idiom; `record.rs:1246` `python_str` and `:1287` `python_repr` beside the `pyjson` formatter.
- After: the seven `PyJson` accessors are `pub` (additive API of the published `secs-project`; doc each in one line); `measure/json.rs` is deleted and its users import the methods; `irs::read_bounded` becomes `pub` and `measurement_file::read` calls it (keep `measurement_file::read`'s error type by mapping at the call); `record.rs`'s `python_str`/`python_repr` move into `pyjson.rs` next to the existing formatter if their escape rules are identical to it, otherwise they stay and the report says which rule differs (then the consolidation is a behaviour question for Claude, not this packet). `CalFile: Clone` is `secs-mdat` (not yours): skip and report.
- Deleted: `measure/json.rs`, the duplicate `read`.

Files: `crates/secs-project/src/{pyjson.rs, irs.rs, record.rs, lib.rs}`, `crates/secs-service/src/measure/{json.rs, mod.rs, settings.rs}`, `crates/secs-service/src/measurement_file.rs`.
Gate: `cargo test -p secs-project -p secs-service --locked` (pyjson oracle goldens `pyjson_*` unchanged).
Commit subject: `refactor(audit): B-22 PyJson 접근자와 상한 있는 읽기를 한 벌로 공유합니다 (#186 TD-218)`.

### B-26 · TD-229 · `record.rs` is 1,612 lines with five concerns

Finding: "한 파일에 저장소 CRUD(212-640), IR LRU 캐시(857-986), 사이드카 JSON
코덱(988-1292), 경로 가둠(1402-1460), PyJson 항목 도우미(1294-1400) 다섯 관심사."
Recommendation: move the sidecar codec (about 300 lines) to `source.rs` and
`IrCache` to `cache.rs`; `lib.rs` re-exports stay (no renames).

- Before: one file (functions listed by `grep -n "^fn \|^pub fn \|^pub struct \|^impl " crates/secs-project/src/record.rs`).
- After: `crates/secs-project/src/record/mod.rs` (the store CRUD, `MeasurementRecord`, `MigrationWarning`, `attached_calibration`, fingerprints), `record/source.rs` (`source_path`, `save_source`, `load_source*`, `source_to_json`, `source_from_json` and the `boxed_*`, `cal_*`, `float_array_*`, `python_*` helpers), `record/cache.rs` (`CacheKey`, `CacheValue`, `IrCache`, `cache_key`), `record/entries.rs` (`measurement_values*`, `pos_of*`, `new_entry`, `repair_entry`, `value_for*`, `set_value`, `setdefault_*`, `nested_object`, `set_nested`, `setdefault_nested`), `record/confine.rs` (`confined_ir_path` and the path checks). Visibility: items used across the new files become `pub(super)`; the crate's public API (`lib.rs` re-exports) is byte-for-byte the same, checked with `cargo doc -p secs-project --no-deps` before/after (compare the item list under `secs_project::record`). The `#[cfg(test)]` modules move with their functions (test names unchanged).
- Deleted: nothing.

Files: `crates/secs-project/src/record.rs` → `crates/secs-project/src/record/*.rs`.
Gate: `cargo test -p secs-project --locked -- --list` identical; `cargo test -p secs-project -p secs-service --locked`.
Commit subject: `refactor(audit): B-26 record.rs를 저장소, 사이드카 코덱, 캐시, 항목 도우미, 경로 가둠으로 나눕니다 (#186 TD-229)`.

### B-23 · TD-219 · public items without callers

Finding: "시험 밖 호출자가 없는 공개 항목: `RewClient::{hold_offline, probe,
pro_state/set_pro/is_pro, last_levels, process_result, callback_url, transport,
set_sequential_channels}`, `CallbackServer::history`, `launch::{clean_env,
rew_process_running}`, `secs_audio::{level_check, SplReference::from_usb}`,
서비스의 `mdat_import::{problems, import, mdat_sources}`·`dirac_import::import`·
`MockMeasurer::with_delay`, `secs-project`의 `load_project_irs`/`LoadedIr`·
`t0_by_role`·`irs_by_role`·`label_for`·`write_wav_float32`(오류를 빈 Vec으로
삼키는 옛 API)·`read_wav_float32`, 생성자가 한 번도 불리지 않는 `CachedBackend`,
`secs_camilla::atomic_write`, `FirView::materialize`(48 k 전용)." Recommendation:
published-crate items get one version of `#[deprecated]` and go in the next
minor; unpublished items are deleted now.

Scope here: `secs-rew`, `secs-audio`, `secs-service`, `secs-project`
(`secs-camilla`'s `atomic_write` and `FirView::materialize` are P194's;
`CachedBackend` is X-26 above; `load_project_irs`/`LoadedIr` are deleted by
ADR 0060/P189, verify).

- Before: the items above, confirmed by `grep -rn "<name>" crates apps bindings --include=*.rs | grep -v "^crates/<owner>/src"` giving only test hits (re-check every one; the audit was at `97a27818` and Phase 1 may have added callers, e.g. `rew_process_running` after X-26).
- After: `secs-rew`, `secs-audio`, `secs-project` are published (ADR 0016): mark each caller-less `pub` item `#[deprecated(since = "<next minor>", note = "no caller; removed in the next minor (audit #186 TD-219)")]` and add `#![allow(deprecated)]` only in the test modules that still use them; `secs-service` is also published: same rule. Items that are `pub(crate)` or private and unused are deleted outright. `mdat_import::{problems, import, mdat_sources}` and `dirac_import::import`: if only tests call them, they become `pub(crate)` (the crate's public surface shrinks by items nobody outside could reach before? No: they were `pub`; treat them as published and deprecate).
- Deleted: unused private/`pub(crate)` items only.

Files: the owning crates' source files.
Gate: `cargo test -p secs-rew -p secs-audio -p secs-service -p secs-project --locked`; `cargo clippy` with `-D warnings` must not trip on `deprecated` (allow it at the test modules).
Commit subject: `refactor(audit): B-23 호출자 없는 공개 항목을 deprecated로 표시하고 비공개 항목은 지웁니다 (#186 TD-219)`.

### B-17 · TD-197 · three stage→rename conventions and two link detectors

Finding: "stage→rename 규약이 셋(세대 폴더 통째, 백업 있는 폴더 통째, 파일마다
롤백)이고 링크 감지가 둘(`design_export.rs`는 `FILE_ATTRIBUTE_REPARSE_POINT`까지,
`deploy.rs`·`apply.rs`는 `is_symlink()`만)입니다. OneDrive 자리표시자 같은 새
reparse 종류를 색인 검색은 건너뛰고 정리·보존은 보통 폴더로 다룹니다."
Recommendation: lift `is_link_or_junction` to a `pub(crate)` helper used by
`deploy.rs` and `apply.rs`; document in `deploy.rs`'s header which replace
helper handles which case.

- Before: `crates/secs-service/src/design_export.rs:472` `fn is_link_or_junction(metadata: &fs::Metadata) -> bool` (nested, Windows reparse aware); `deploy.rs:49,69` and `apply.rs:556` use `metadata.file_type().is_symlink()` only.
- After: `crates/secs-service/src/fs.rs` (created by X-25) gains `pub(crate) fn is_link_or_junction(metadata: &fs::Metadata) -> bool`; the three sites call it; `deploy.rs`'s module doc lists the three conventions: generation folder install (`deploy::install`, whole folder, no backup), bundle replace with backup (`design_export::replace_folder`), per-file rollback (`secs_graph::files::install`). Note the behaviour question: treating a junction as a link in `deploy.rs`/`apply.rs` where only symlinks were refused before is a behaviour change on Windows for junctions. Keep the refusal set as today (symlink only) in this packet by passing a flag `LinkKind::Symlink` vs `LinkKind::Any`, and say in the report that widening it to junctions is a Category A follow-up (Claude decides).
- Deleted: the nested function (moved).

Files: `crates/secs-service/src/{fs.rs, design_export.rs, deploy.rs, apply.rs}`.
Gate: `cargo test -p secs-service --locked` (`tests/apply.rs` link tests unchanged).
Commit subject: `refactor(audit): B-17 링크 판정을 한 도우미로 모으고 교체 규약 셋을 문서화합니다 (#186 TD-197)`.

## Verification (foreground; do not end the turn before they finish)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-project -p secs-audio -p secs-rew -p secs-types -p secs2r -p secs2r-cli -p sound-equalization-and-correction-system-rust-api --locked --no-fail-fast
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-svc-b
```

The public API crate needs `--all-features` for its own tests
(`cargo test -p sound-equalization-and-correction-system-rust-api --all-features --locked`).
`cached_resample_replay_releases_read_memory_before_calibrating_td130` in
`tests/service.rs` and the tutorial quests vitest are known to fail under a
loaded parallel run on this machine and pass alone; rerun such a test alone
before you treat it as yours, and report it either way. Changing the public
items of `secs-project` (B-10, B-22, B-23) reaches crates outside your file
list: the workspace clippy tells you; if a dependent crate outside the list
breaks, stop that item and report it.

`check_feature_tests.py` on Windows reports two known names it cannot list
(`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`); anything else is yours.

## Report

Per item: the before counts, the after counts, the commit hash, the files,
what was deleted. Then: items reclassified (`RECLASSIFY → A`, with the test
and assertion), items skipped because a prerequisite packet did not land
as described (say what you found instead), anything you left for Claude
(the junction question of B-17, `CalFile: Clone`, app `Deps` constructors
of X-28), and the final `git log --oneline` of the branch.
