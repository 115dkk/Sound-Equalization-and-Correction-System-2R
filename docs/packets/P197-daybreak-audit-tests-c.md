# P197 (Daybreak): audit #186, Category C test-only findings

Phase 3 of `/115dkk-skills:resolve-audit` for issue #186. It runs after
every Phase 1, Phase 2 and ADR packet has landed on `fix/audit-186`
(including P189/P203 on the service, P202 on the API, P193/P205 on the
service and project crates, P199 on the calibration settings). Claude
created the worktree `E:\wt\audit-host` on a fresh branch
`audit/186-tests-c` from the current `fix/audit-186` (`git status` is clean
when you start). Work only there. Every cargo command runs with
`CARGO_TARGET_DIR=E:\cargo-target\audit-engine` (warm, shared with
Claude's verification runs; a lock wait is normal). Never run `cargo clean`,
never delete anything under `E:\cargo-target`, and check after the first
command that `E:\wt\audit-host\target` does not exist. Commit on that
branch, one commit per finding, in the order below, with the trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push, do
not rebase, do not spawn sub-agents, do not touch `audit-state.md`.

Line numbers below are from the audit commit; the service integration
tests were split into `tests/{service,calibration,export,cache}.rs` with a
shared `tests/support/mod.rs` (B-3), and `crates/secs-project/src/record.rs`
may be a `record/` directory now (B-26). Search by the names given.

Category C means: no runtime behaviour changes. Only tests, fuzz targets,
test fixtures, CI seed lines and the policy gates change. The gate for each
item is the test runner (or the fuzz build) itself. If an item turns out to
need a change in non-test code to make its test honest, STOP that item,
report it as a reclassification (C → A or B) with the reason, and go on.

Allowed files:

- the test files and `#[cfg(test)]` modules named per item below, and new
  files under a crate's `tests/`;
- `fuzz/**` (targets, `Cargo.toml` target list, a committed `corpus/` file);
- `.github/workflows/ci.yml`: the fuzz seed `case` block (lines near
  `Run every target`), the Miri comment, and the Python wheel job's test
  inputs only;
- `bindings/python/tests/**`, `bindings/python/src/lib.rs` only to delete
  the dead `#[cfg(test)] mod tests` (C-1), `bindings/python/Cargo.toml`
  only if C-1 needs it;
- `crates/secs-policy/tests/gates.rs` (one new gate, C-9);
- `docs/HARDWARE.md` (C-18 note);
- `features.toml`: when a test is renamed or added, update the entry that
  lists it; `python3 .github/scripts/check_feature_tests.py` must accept the
  result (on Windows it reports two known names it cannot list,
  `secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`
  and `secs-sys::callback_native_sample_conversion`; anything else is yours).

Forbidden: every `src/` file except the `#[cfg(test)]` modules named below,
`CHANGELOG.md`, `docs/**` except `HARDWARE.md`, the screens.

Read first: `CLAUDE.md` (the 검사 section), `docs/adr/0004-golden-parity.md`
(or the ADR whose title mentions 골든), `.github/workflows/ci.yml` lines
1286-1385 (Miri and fuzz jobs), `fuzz/Cargo.toml`, the finding texts quoted
below.

## Why

The monthly audit (issue #186) found tests that do not test what their
names say, test paths that never run, parsers that have no fuzz target, and
one published crate whose test reads outside its package. Each item here
changes only what is checked, never what the program does.

## The protocol for every item

1. Announce the item in the report (`── C-n: … ──`).
2. Make the change.
3. Gate: `cargo fmt --all -- --check`, then the command named per item
   (foreground; wait for it). Every fifth commit and at the end:
   `cargo test --workspace --locked`.
4. Commit with `git add <the files you touched>` (never `git add -A`) and
   this message shape (Korean, 하십시오체, no em-dash, no program-as-subject
   verbs such as 묻다/적다/넘기다, "추가하다" not "더하다"):

   ```
   chore(audit): C-n <한국어 한 문장> (#186 TD-xxx)

   <무엇을 검사하게 되는지 한두 문장. 동작은 바뀌지 않는다는 문장.>

   Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
   ```

## Items

### C-1 · TD-158 · the TD-127 Python test never compiles

Finding: "`[lib] test = false`라 lib 시험 대상이 꺼져 있어
`dropped_event_count_does_not_lock_the_blocking_receiver_td127`은
어디에서도 컴파일되거나 실행되지 않습니다(CI는 pytest만 돌림,
`ci.yml:332-345`). 파이썬 바인딩의 TD-127 수정은 검증되지 않았습니다."

Site: `bindings/python/src/lib.rs:227-243` (the dead `#[cfg(test)] mod tests`),
`bindings/python/Cargo.toml:14` (`test = false`, kept: a cdylib with
`extension-module` cannot link a test binary).

Change: delete the dead module. Add the check where it runs:
`bindings/python/tests/test_secs2r.py::test_dropped_events_does_not_block_while_next_is_waiting_td127`
(a thread blocked in `next(engine.events())` on an idle engine; the main
thread calls `events.dropped_events()` and it returns `0` within 1 s; then
stop the thread through the `close()` that P184's A-1 added, or by dropping
the engine if `close()` is absent, and say which). If the Rust-side property
(the counter is an atomic, not behind the receiver mutex) is also worth a
Rust test, put it in `crates/secs-api/tests/public_api.rs` as
`dropped_event_count_is_readable_while_a_receiver_blocks_td127` against
`secs_api`'s own event type.

Gate: `cargo check -p secs-py` (the crate name in `bindings/python/Cargo.toml`)
and the pytest run described in P184's "Local tooling" (venv + `maturin
develop`); if the wheel cannot be built here, run `python -m pytest
bindings/python/tests -q --collect-only` to prove the test parses and
report that CI's wheel job runs it.

Commit subject: `chore(audit): C-1 TD-127 파이썬 검사를 실제로 실행되는 자리로 옮깁니다 (#186 TD-158)`.

### C-2 · TD-159 · the wheel's only end-to-end test always skips in CI

Finding: "휠의 유일한 엔진 끝단 시험 `test_oracle_project_round_trip_when_supplied`는
`SECS_ORACLE_PROJECT`가 없으면 건너뛰는데 CI는 이 변수를 주지 않아, 파이썬
휠은 `bootstrap`과 오류 경로만 CI에서 돕니다. .NET 시험은 모의 보정을
돌립니다(`PackageTests.cs:110-166`)."

Site: `bindings/python/tests/test_secs2r.py:65-80`, `.github/workflows/ci.yml:304-345`.

Change: replace the environment-dependent test with
`test_mock_room_calibrates_and_exports` that follows
`bindings/dotnet/Secs2R.Tests/PackageTests.cs:110-166` step by step:
`project_new` with the `sample` preset into `tmp_path`, `measure_configure`
with the mock method, `measure_position` for position 1 (wait on its job),
`calibrate` (wait, assert `status == "succeeded"`), `export` of the
CamillaDSP kind into `tmp_path / "out"` (wait; assert the YAML file the
.NET test asserts exists). Expected literals are the same ones
`PackageTests.cs` uses. Keep the oracle test only if it still adds something
the mock flow does not; otherwise delete it and remove `SECS_ORACLE_PROJECT`
mentions from `ci.yml` and the README if any.

Gate: the pytest run (or `--collect-only` plus a note that CI runs it).

Commit subject: `chore(audit): C-2 파이썬 휠 시험이 모의 방으로 보정과 내보내기를 돕니다 (#186 TD-159)`.

### C-8 · TD-185 · the sidecar codec has no fuzz target

Finding: "`.src.json` 코덱(`source_from_json`, F10의 관대한 변환)은 파일
경로로만 닿아 fuzz도 Miri도 없고, `project_load_text`는 기본 예산(4×len+16
MiB)만 쓰고 `loads_with_budget`은 부르지 않습니다."

Site: `crates/secs-project/src/record.rs:1116` (`fn source_from_json(value:
&PyJson) -> Option<SourceInfo>`, private), `fuzz/fuzz_targets/project_load_text.rs`,
`crates/secs-project/src/pyjson.rs:195` (`pub fn loads_with_budget`).

Change (test-only; the one `pub` it needs is a thin wrapper, which is
allowed because it adds no behaviour): expose
`pub fn source_info_from_text(text: &str) -> Option<SourceInfo>` in
`record.rs` that parses with `pyjson::loads_with_budget(text, 16 MiB)` and
calls `source_from_json` (document it as the fuzz entry; `load_source` keeps
its path-based reading). Add `fuzz/fuzz_targets/source_info_parse.rs`
(`fuzz_target!` over `&[u8]` → UTF-8 → `source_info_from_text`; when
`Some`, round-trip through `SourceInfo`'s serializer if one is public and
assert the second parse equals the first), register it in `fuzz/Cargo.toml`,
and add a seed case in `ci.yml` (`source_info_parse) cp ../oracle/goldens/project/*.src.json
"corpus/$target/" ;;` if such goldens exist, else the sidecars under
`crates/secs-project/tests/data/`; find real `.src.json` files with
`git ls-files '*.src.json'` and pick a folder that exists). In
`project_load_text.rs` add, before the existing `load_text`, a call
`let _ = secs_project::pyjson::loads_with_budget(text, 256);` (no panic is
the property; the result is ignored).

Gate: `cargo +<NIGHTLY from ci.yml> fuzz build --target x86_64-pc-windows-msvc`
if `cargo-fuzz` is installed (`cargo fuzz --version`), else
`cargo +<NIGHTLY> check --manifest-path fuzz/Cargo.toml` and say so; plus
`cargo test -p secs-project --locked`.

Commit subject: `chore(audit): C-8 사이드카 코덱에 fuzz 대상과 작은 예산 검사를 추가합니다 (#186 TD-185)`.

### C-9 · TD-186 · a published crate's test reads outside its package

Finding: "패키지에 들어가는 파일(`src/**`가 `include`에 있음)이
`include_bytes!("../../../src/secs/rew_mdat/template_1.mdat")`로 크레이트
밖을 읽어 ADR 0016과 어긋납니다. `cargo publish --dry-run`은 `#[cfg(test)]`를
컴파일하지 않아 게이트가 놓치고, 게시된 tarball에서 `cargo test`는 컴파일에
실패합니다."

Site: `crates/secs-jos/src/tests.rs:361` (`deterministic_3000_mutations`),
`crates/secs-jos/Cargo.toml:13` (`include = [... "src/**"]`),
`crates/secs-policy/tests/gates.rs`.

Change: move `deterministic_3000_mutations` to
`crates/secs-jos/tests/mutations.rs` (outside `include`), reading the
template through the dev-dependency `secs_oracle` (`secs_oracle::Fixture`
or the helper the other crates use for `template_1.mdat`; check
`crates/secs-mdat/tests/*.rs` for the idiom) so the test keeps running in
the workspace. Then add a policy gate
`published_crate_sources_read_nothing_outside_the_crate` in `gates.rs`,
next to `os_code_stays_in_secs_sys`: for every crate whose manifest
publishes (reuse the manifest helpers near `workspace_dependency_packages`),
scan `src/**/*.rs` for `include_bytes!(` and `include_str!(` whose literal
contains `../` and fail with the file and line. Keep `features.toml` in
step if it names the moved test.

Gate: `cargo test -p secs-jos --locked`, `cargo test -p secs-policy --locked`.

Commit subject: `chore(audit): C-9 게시 크레이트의 시험이 패키지 밖 파일을 읽지 않게 합니다 (#186 TD-186)`.

### C-10 · TD-192 · the TD-135 regression test skips silently

Finding: "TD-135 회귀 시험이 `C:\Documents and Settings`가 없거나 정션이
아니면, 그리고 `symlink_dir`이 허용되지 않으면 조용히 "skipping"으로
통과합니다."

Site: `apps/secs2r-host/src/rules.rs:1034-1082` (the two `#[cfg(target_os = "windows")]` tests).

Change: in both tests replace the silent `return` with: if
`std::env::var_os("GITHUB_ACTIONS").is_some()` then `panic!` with the same
message (CI runners have the junction and allow `symlink_dir` for the runner
user); otherwise keep the skip but `eprintln!` it as today. Add one test
`at_least_one_link_rejection_case_runs_td192` that is `#[cfg(target_os =
"windows")]` and asserts that creating a directory symlink in a temp folder
succeeds on this machine (so a developer box that cannot run either case is
visible as a failure rather than two skips); if the machine cannot create
symlinks (Developer Mode off), it panics with the instruction to enable
Developer Mode, which is the point of the finding.

Gate: `cargo test -p secs2r-host --locked` on this Windows machine.

Commit subject: `chore(audit): C-10 TD-135 회귀 시험이 CI에서 조용히 건너뛰지 않습니다 (#186 TD-192)`.

### C-12 · TD-252 · three feature tests compare the code to itself

Finding: "`dip_limit_off_and_golden_rooms_are_bit_identical`은 `dip_limit`이
이미 false인 `baseline`을 복제해 다시 false로 두고 비트 동일을 단언합니다
… `mimo_early_sixty_keeps_goldens_bit_identical`은 …
`normal_equations(..,36)`과 `normal_equations(.., early_samples(60.0))`를
비교하고, `mimo_seat_guard_off_keeps_goldens_bit_identical`은 같은 설정을
두 번 돌립니다. … 딥 보정 제한의 메모 시험은 `fill == (2 + (depth-2)/2).min(6)`을
단언해 `rc.rs:131-136`의 공식을 단언에서 다시 유도합니다."
Recommendation: replay golden rooms with `design::replay`; compare the
60 ms and guard-off runs with the recorded `engine_mimo` /
`engine_mimo_run` fixtures; put the default-value assertions in the
`secs-types` tests that already exist; the note test asserts the ADR 0034
table values (7.5 → 4.75, 10 → 6).

Sites: `crates/secs-engine/src/tests/dip_limit.rs:93-104` and `:140-142,168-171`,
`crates/secs-engine/src/tests/mimo.rs:54-85`, `crates/secs-engine/src/tests/mimo_run.rs:722-753`,
`features.toml` entries `calibration.dip_limit`, `calibration.mimo_seat_guard`,
`calibration.mimo_early`.

Change, one commit, three tests rewritten in place (names kept so the
registry stays valid):

- `dip_limit_off_and_golden_rooms_are_bit_identical`: iterate the golden
  rooms the file's neighbours use (`cases::load(..)` names such as
  `room_filters`, `room`; take the list from `mimo_run.rs`'s helpers), run
  `design::replay(name)` with the recorded config (dip_limit off) and
  assert the design equals the recorded one bit for bit (the fixture's
  recorded FIR/response arrays, compared with `assert_bits` or the exact
  comparison the replay tests already use); then set `dip_limit = true`,
  run again, and assert the result differs only in bins that
  `rc::limit_dip_fills` reports (collect the bins from the `Note::DipLimit`
  dips: every differing bin lies inside one of the reported dip bands; no
  other bin differs). If the golden room has no limited dips at all, assert
  the run is bit-identical and say so in the report.
- `mimo_early_sixty_keeps_goldens_bit_identical`: load
  `Fixture::load("engine_mimo", "room")` (and `engine_mimo_run` if it holds
  the same case) and compare the output of the 60 ms run with the recorded
  outputs in the fixture (the arrays the neighbouring
  `mimo_stage_boundary_goldens` reads), not with a second call of the same
  function. Move the `assert_eq!(req.mimo_early_ms, 60.0)` /
  `early_samples == 36` default assertions into
  `crates/secs-types` next to `mimo_early_defaults_sixty_and_preserves_legacy_mimo_key`
  only if they are not already there (they are: leave them).
- `mimo_seat_guard_off_keeps_goldens_bit_identical`: compare the guard-off
  `mimo::compute` result with the recorded `engine_mimo_run` fixture output
  (`fx.outputs()[...]`, the same arrays `mimo_run_fir_to_48k_and_response_match_oracle`
  compares) instead of with a second call; keep the `legacy` deserialization
  assertion.
- `dip_limit_notes_list_the_deepest_limited_dips`: replace the two
  formula loops with literal assertions from ADR 0034's table: a 7.5 dB dip
  fills 4.75 dB, a 10 dB dip fills 6 dB (`fill == 4.75` for the dip whose
  depth is 7.5 within 0.05 dB, `6.0` for 10 dB); build the synthetic
  spectrum so those two depths occur, or assert on the dips that do occur
  against a small literal table you compute by hand from the ADR formula
  and write into the test as numbers.

If any golden room's replay is NOT bit-identical to its recording with
`dip_limit` off, STOP: that is a real defect (reclassify to A), commit
nothing for C-12, and report the room and the first differing bin.

Gate: `cargo test -p secs-engine --locked` (long) and
`python3 .github/scripts/check_feature_tests.py`.

Commit subject: `chore(audit): C-12 자기 자신과 비교하던 기능 시험 셋이 골든과 비교합니다 (#186 TD-252)`.

### C-14 · TD-202 · `deploy.rs` has no unit tests

Finding: "단위 시험이 없습니다. `install`의 접두 검증(13-19)과
`remove_link`의 파일/폴더 대체(80-88)는 `tests/apply.rs:1917,2066`을 거쳐
간접으로만 닿습니다."

Site: `crates/secs-service/src/deploy.rs` (add a `#[cfg(test)] mod tests`
at the end).

Change: two unit tests. `install_refuses_a_prefix_that_is_not_one_file_name_td202`:
`install(root, "a/b", stage)` and `install(root, "..", stage)` and
`install(root, "", stage)` each answer `ErrorCode::InvalidRequest` with the
literal detail `generation prefix must be one file name`, and the stage
folder still exists. `remove_link_removes_a_file_or_an_empty_directory_td202`:
a file link target is removed; an empty directory is removed; a non-empty
directory answers `ErrorCode::ApplyFailed` with `params.path` naming it
(literal path from the temp folder).

Gate: `cargo test -p secs-service --locked deploy`.

Commit subject: `chore(audit): C-14 deploy.rs의 접두 검증과 링크 제거에 단위 시험을 둡니다 (#186 TD-202)`.

### C-18 · TD-220 · FakeRew and SimRoom never exercise the error paths

Finding: "`FakeRew::run_position`은 `configure`·`open_plan_devices`·
`set_naming`·`set_notes`를 건너뛰고 `ProRequired`를 낼 수 없으며 `spl_db`를
합성하므로 FakeRew 위의 서비스 시험은 Pro 게이트와 REW 장치 오류에 닿지
않습니다. `SimRoom`의 입력은 재생이 끝날 때까지 0 프레임이라 동시 채움·
불연속 경로를 돌지 않습니다. `is_ok()`만 보는 단언 셋, CI가 돌리지 않는
유일한 `#[ignore]`(`real_umik1_file`), 시험이 없는 16 작업자 accept 상한·
128 MiB 본문 상한·`wait_for_new_measurement` 중 일시 오류."

Sites: `crates/secs-rew/src/fake.rs:300-382`, `crates/secs-audio/src/sim_room.rs:244-280`,
`crates/secs-audio/tests/native.rs:162`, `crates/secs-audio/tests/mic_calibration.rs:194,487`,
`crates/secs-audio/tests/level_calibration.rs:53`, `crates/secs-rew/src/callback.rs:133-140`,
`crates/secs-rew/src/transport.rs:103-110`, `crates/secs-rew/src/client.rs:816`.

Change (test doubles and tests only; the fake and the simulator are test
code even though they live in `src/`):

- `FakeOptions` gains nothing new; instead a service test
  `rew_pro_gate_is_reported_through_the_measurer_td220` in
  `crates/secs-service/tests/measure_ipc.rs` builds the fake with
  `pro: false` and drives `RewMeasurer` through the real `configure` path
  (the fake's `configure` must already answer `ProRequired` when `pro` is
  false; if `run_position` skips `configure`, make the fake's
  `run_position` call its own `configure` first, which is a change inside
  the test double) and asserts the measure job ends with the error code the
  service maps `ProRequired` to (read it from `measure/rew.rs`, write it as
  a literal).
- `SimRoom`: add `SimOptions::discontinuous_reads: bool` (default false)
  that makes `read_into` answer `discontinuity: true` on every third read;
  a test in `crates/secs-audio/tests/native.rs`
  `measure_position_survives_capture_discontinuities_td220` asserts the
  position still records all roles and that the result's SNR field is
  still finite.
- Replace the three `is_ok()` assertions with assertions on the value:
  `native.rs:162` → the returned roles equal the plan's roles (literal
  list); `mic_calibration.rs:194` → the parsed calibration has 2 points
  with the literal frequencies 20 and 1000; `level_calibration.rs:53` →
  whatever the call returns, assert its fields (read the test and pick the
  two most meaningful).
- `real_umik1_file` stays `#[ignore]`; add a paragraph to
  `docs/HARDWARE.md` under `## 실행 방법` saying how to run it
  (`SECS_REAL_MIC_CAL=<path> cargo test -p secs-audio -- --ignored real_umik1_file`).
- Three bound tests: `callback_server_accepts_at_most_16_workers_td220`
  (17 concurrent connections: the 17th is not served until one finishes;
  use the existing callback test harness in `callback.rs` tests),
  `transport_body_limit_is_128_mib_td220` (a fake HTTP response one byte
  over the limit → `RewError::connection`), and
  `wait_for_new_measurement_ignores_a_transient_error_td220` only if the
  client already tolerates one; if the current code propagates the first
  transient error (the audit's looks-bad-but-fine list says upstream does
  the same), write the test to pin the current behaviour and name it
  `wait_for_new_measurement_propagates_the_first_transient_error`.

Gate: `cargo test -p secs-rew -p secs-audio -p secs-service --locked`.

Commit subject: `chore(audit): C-18 FakeRew와 SimRoom의 오류 경로와 상한에 시험을 둡니다 (#186 TD-220)`.

### C-19 · TD-227 · a self-referential golden, bare `is_err` asserts, a skipping compile check

Finding: "`convolution_bundle_contains_complete_paths_and_is_atomic`이 기대
WAV 샘플을 같은 크레이트의 `render_paths`에서 끌어내 … `assert!(… .is_err())`
넷은 variant도 메시지도 보지 않으며, `reaper_script_compiles_when_luac_is_available`은
`luac`가 없으면 `SKIP`을 찍고 통과합니다."

Sites: `crates/secs-filters/tests/golden.rs:1140, 2272-2302, 2386, 2712-2713`,
`crates/secs-filters/tests/reaper_lua.rs:79-92`.

Change: in the bundle test, keep the `render_paths` comparison but add,
for one route (the first `L`→`L` path), an assertion that its first
non-zero sample equals a literal taken from the oracle
`Fixture::load("filters", "computed_dolby")` outputs (find the FIR array the
fixture records for that path; write the index and the f32 bits as
literals in the test with a comment naming the fixture key). Replace the
four `is_err()` assertions with `matches!(…, Err(FiltersError::Invalid(msg)) if msg.contains("<literal>"))`
using the message each site really produces (run once to read it, then
pin it). In `reaper_lua.rs`, when `std::env::var_os("CI").is_some()` and
no compiler is found, `panic!` instead of returning.

Gate: `cargo test -p secs-filters --locked`.

Commit subject: `chore(audit): C-19 컨볼루션 골든 시험이 오라클 값과 오류 종류를 단언합니다 (#186 TD-227)`.

### C-20 · TD-230 · a fuzz corpus gap and a wrong Miri comment

Finding: "`mdat_load` seed는 템플릿과 `build_*`/`error_dangling_handle`
골든뿐이라 `TC_REFERENCE`로 보정 배열을 공유하는 사례는 메모리의
시험(`untrusted.rs:163`)에만 있습니다. ci.yml 주석은 템플릿 파싱 시험이
Miri에서 제외된다고 하지만 … 218 kB `TEMPLATE`을 일곱 시험이 Miri로
파싱합니다."

Sites: `crates/secs-mdat/tests/untrusted.rs` (`template_graph()` and the
seven tests that call it; some already carry `#[cfg_attr(miri, ignore = …)]`),
`.github/workflows/ci.yml:1310-1312` (the Miri comment) and the fuzz seed
block.

Change: in `shared_calibration_array_copies_obey_the_parse_budget_td18`,
serialize the stream it builds (`secs_jos::write(&s)`) once into
`fuzz/corpus/mdat_load/shared_calibration.mdat` (commit the file; keep it
under 2 MiB, shrink the array if needed and say so), and add
`cp ../fuzz/corpus/mdat_load/*.mdat "corpus/$target/" 2>/dev/null || true`
to the `jos_parse|mdat_load|mdat_build_template)` seed case. Then make the
comment true: every test in `untrusted.rs` that calls `template_graph()`
gets `#[cfg_attr(miri, ignore = "parses the 218 kB template; too slow under Miri, runs natively")]`
if it lacks one. Add a `.gitignore` exception only if `fuzz/corpus/` is
ignored today (check `git check-ignore -v fuzz/corpus/x`).

Gate: `cargo test -p secs-mdat --locked`; if the nightly is installed,
`cargo +<NIGHTLY> miri test --locked -p secs-mdat` (report the time).

Commit subject: `chore(audit): C-20 공유 보정 배열 사례를 fuzz 말뭉치에 넣고 Miri 주석을 맞춥니다 (#186 TD-230)`.

### C-32 · TD-269 · no fuzz for the two calibration parsers

Finding: "퍼징이 … 신뢰할 수 없는 글자를 읽는 `MicCalibration::parse_checked`
(제조사 파일)와 `from_rew_measurement`(REW 응답)는 덮지 않습니다."

Sites: `crates/secs-audio/src/calibration.rs:99` (`pub fn parse_checked(text: &str)`),
`crates/secs-mdat/src/source_info.rs:488` (`pub fn from_rew_measurement(cal, output_device, input_device, output_channel, …)`),
`fuzz/Cargo.toml`, `.github/workflows/ci.yml` seed block.

Change: add `fuzz/fuzz_targets/cal_parse.rs` feeding the bytes (as UTF-8)
to `MicCalibration::parse_checked` and, split at the first newline, to
`from_rew_measurement` (the second half as the REW measurement JSON parsed
with `serde_json::from_str` and the first half as device names; check the
function's real signature and build its arguments from the input), add
`secs-audio` (and `serde_json` if needed) to `fuzz/Cargo.toml`, register
the target, and a seed case `cal_parse)` copying the calibration files the
tests use (`git ls-files '*.cal' '*.txt' | grep -i cal` under crates/secs-audio
and the recorded REW measurement JSON under crates/secs-service/tests or
crates/secs-rew/tests). No runtime code changes.

Gate: the fuzz build (or `cargo check` of fuzz/ with the nightly) and
`cargo test -p secs-audio --locked`.

Commit subject: `chore(audit): C-32 마이크 보정 파일과 REW 측정 응답 파서에 fuzz 대상을 추가합니다 (#186 TD-269)`.

## Verification (foreground; do not end the turn before they finish)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p <every crate you touched> --locked --no-fail-fast
python3 .github/scripts/check_feature_tests.py
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml', encoding='utf-8'))"
git log --oneline fix/audit-186..audit/186-tests-c
```

## Report

Per item: the commit hash, the test names added or rewritten, the gate
output tail. Then: items reclassified (C-12 if a golden differs) or blocked
and why, what could not run locally (wheel, fuzz, Miri) and what CI
covers, and the final `git log --oneline` of the branch.
