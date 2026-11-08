# P210 (Daybreak): audit #186, the four test items P197 stopped (C-12, C-14, C-18, C-19)

Worktree `E:\wt\audit-host`, branch `audit/186-tests-c2` (Claude created it
from the current `fix/audit-186`, which carries P197's eight commits;
`git status` is clean when you start). Every cargo command, including the
one `python3 .github/scripts/check_feature_tests.py` starts, runs with
`CARGO_TARGET_DIR=E:\cargo-target\audit-engine`: export it once per shell
and check it with `echo $CARGO_TARGET_DIR` before the first build and before
the feature check. You are the only one building in that target (P208 builds
in `C:\cargo-target\audit-api` from `E:\wt\audit-svc`). Never run
`cargo clean`, never delete anything under `E:\cargo-target`. No
workspace-wide `cargo test`; per-package (`-p`) only, plus one
`cargo clippy --workspace --all-targets --locked -- -D warnings` at the end.
One commit per item with the titles below, trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push, do
not rebase, do not touch ADRs, `docs/**` (except `docs/HARDWARE.md` for
C-18), `CHANGELOG.md`, `audit-state.md` or the UI.

The item texts are in
`E:\Sound-Equalization-and-Correction-System-2R\docs\packets\P197-daybreak-audit-tests-c.md`
(sections C-12, C-14, C-18, C-19); read them first. P197 stopped each one at
a condition this packet now decides. Everything not changed below stands.

## 1. C-19 · TD-227 (Category C)

P197 found that a `sample_rate = 44_100` chain answers
`FiltersError::Graph(GraphError::Invalid("chain.sample_rate 44100 (expected 48000)"))`,
not the `FiltersError::Invalid("unsupported output sample rate")` the packet
expected. The packet's expectation was out of date: assert the variant the
code returns today (`FiltersError::Graph(GraphError::Invalid(message))`
with `message.starts_with("chain.sample_rate ")`), and match each of the
other bare `is_err()` sites the same way, on what they return. The golden
literal is the one P197 measured: the `computed_dolby` L→L path's first
nonzero sample is index 93 with f64 bits `4562469665216348470`. The `luac`
compile check fails instead of printing `SKIP` when `CI` is set. Commit:
`chore(audit): C-19 컨볼루션 골든이 오라클 리터럴과 비교하고 오류 단언이 종류를 봅니다 (#186 TD-227)`

## 2. C-12 · TD-252 (Category C)

P197 stopped because the `room_filters` replay with `dip_limit` off is not
bit-identical to its recording on Windows (first difference at `L`, sample
0, bin 0). That is the known libm difference, not a defect: the engine's
replay tests compare bit for bit only where the oracle's libm is in use and
with the platform tolerance elsewhere. Use exactly that rule: find how the
existing replay tests decide it (`FixtureStore::oracle_libm()` /
`platform_rtol()` on the `secs_engine::testkit` store, or the helper the
neighbouring replay tests call) and apply it in the three rewritten tests
and in the dip-limit "only reported bins differ" check (on a non-oracle libm,
"differs" means "differs by more than the platform tolerance"). The rest of
P197's C-12 text stands (names kept, fixtures as named, the ADR 0034 literal
table for the note test). Commit: the C-12 title from P197.

## 3. C-14 · TD-202 (now Category A: one behaviour fix, then the tests)

P197 found that on Windows `deploy.rs`'s `remove_link` tries `remove_file`
first; for a directory that fails with an access-denied error, and after
X-25 (`fs::io_failure`) a non-empty directory answers `PermissionDenied`
instead of `ApplyFailed`. The cause is the order, not the permission.
Decision: `remove_link` reads `symlink_metadata` first and calls
`remove_dir` for a directory (link or real) and `remove_file` otherwise, so
a non-empty directory reports the directory error (`ApplyFailed` through
`io_failure`, because "directory not empty" is not a permission failure),
and a file link is removed as today.

- Red: P197's C-14 test `remove_link_removes_a_file_or_an_empty_directory_td202`
  (file removed; empty directory removed; non-empty directory answers
  `ApplyFailed` with `params.path` naming it). It fails today on Windows on
  the non-empty case (`PermissionDenied`).
- Green: the order change in `remove_link`.
- Also P197's `install_refuses_a_prefix_that_is_not_one_file_name_td202`.
- Allowed: `crates/secs-service/src/deploy.rs` (its `remove_link` and the
  new `#[cfg(test)] mod tests`). P208 works in other service files; do not
  touch `fs.rs`.
- Commit: `fix(audit): C-14 배치 링크 제거가 종류를 먼저 보고 deploy.rs에 단위 시험을 둡니다 (#186 TD-202)`

## 4. C-18 · TD-220 (Category C)

P197 found that the callback server accepts the 17th connection when 16
workers are busy and drops it at once (`Ok(_) => {}`), which is not the
"waits until a worker finishes" the packet expected, and that `SimOptions`
does not exist. Decisions:

- Pin today's bound instead of changing it, the same way the packet already
  pins `wait_for_new_measurement`: name the test
  `callback_server_drops_a_connection_beyond_16_workers_td220` and assert
  that with 16 held connections a 17th is closed without a response, and
  that after one worker finishes a new connection is served.
- The discontinuous-read option is part of the test double: add it to
  `SimRoom` the way `SimRoom` is configured today (a builder method or a
  constructor argument on the simulator; read `crates/secs-audio/src/sim_room.rs`),
  default off, and write `measure_position_survives_capture_discontinuities_td220`
  as the packet says.
- The rest of P197's C-18 text stands (the Pro-gate service test, the three
  `is_ok()` replacements, `docs/HARDWARE.md`, the 128 MiB body limit test,
  the `wait_for_new_measurement` test).
- Commit: the C-18 title from P197.

## Verification (foreground, at the end)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-filters -p secs-engine -p secs-service -p secs-rew -p secs-audio --locked --no-fail-fast
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-tests-c2
```

`secs-engine`'s goldens take about ten minutes; wait for them. On Windows the
feature check always reports two known names
(`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`); anything else is yours.

## Report

Per item: the commit hash, the tests added or rewritten (with the Red
failure line for C-14), the gate tails, anything still blocked and why, and
the final `git log --oneline`. Finish the report before ending your turn.
