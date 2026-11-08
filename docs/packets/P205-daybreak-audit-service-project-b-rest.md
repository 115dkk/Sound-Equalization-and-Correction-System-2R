# P205 (Daybreak): audit #186, the rest of P193 (service and project Category B) plus X-25

Worktree `E:\wt\audit-svc`, branch `audit/186-svc-b2` (Claude created it
from the current `fix/audit-186`, which already carries P193's B-2, B-3,
B-18 and X-26; `git status` is clean when you start). Every cargo command
runs with `CARGO_TARGET_DIR=C:\cargo-target\audit-api` (warm). Never run
`cargo clean`, never delete anything under `C:\cargo-target`, check after
the first command that `E:\wt\audit-svc\target` does not exist. No
workspace-wide `cargo test`; per-package (`-p`) only, plus one
`cargo clippy --workspace --all-targets --locked -- -D warnings` at the
end. One commit per item, trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push, do
not rebase, do not touch ADRs, `docs/**`, `CHANGELOG.md`, `audit-state.md`
or the UI. Another worker (P199, ADR 0055) changes
`crates/secs-types/**`, `crates/secs-service/src/{settings_edit.rs,calibration.rs}`
(scalar arms and `snapshot` only), `crates/secs-engine/src/{design.rs,levels.rs}`,
the CLI and `crates/secs-project/src/calibration.rs` in another worktree at
the same time: do not edit those files except the signature follow-through
B-10 names in `calibration.rs`, and keep that edit minimal.

The item texts, the protocol (before/after counts, stop rule, gates,
commit message shape) and the allowed files are those of
`E:\Sound-Equalization-and-Correction-System-2R\docs\packets\P193-daybreak-audit-service-project-b.md`;
read it first. Its B-1, B-2, B-3, B-18 and X-26 are done (P193 first pass);
X-28 and X-25 change as described below. Do the items in this order.

## 1. X-25 · A25 · one atomic-write helper (Category A now, test first)

P193's first pass found that the wrappers map I/O errors differently on
purpose: `apply.rs::io_error` turns a permission failure into
`ErrorCode::PermissionDenied`, while `app_settings.rs`/`project_edit.rs`
(`project_io` → `ProjectIo`) and `deploy.rs` (`io_error` → `ApplyFailed`)
do not. TD-208 already made a denied project open answer
`PermissionDenied` (`project_open_permission_denied_maps_to_permission_denied_td208`
in `crates/secs-service/src/lib.rs`). Decision: one rule for every write,
the one `apply.rs` has: a permission failure is `PermissionDenied`, any
other failure is the caller's code. This changes the code a denied
settings, project-edit or deploy write answers (from `ProjectIo` /
`ApplyFailed` to `PermissionDenied`), so it is Category A.

- Red: create `crates/secs-service/src/fs.rs` (and `mod fs;` in `lib.rs`)
  with `pub(crate) fn io_failure(path: &Path, error: &std::io::Error, code: ErrorCode) -> IpcError`
  stubbed to always use `code`, and its unit test
  `a_denied_write_is_permission_denied_for_every_caller_td208`: for each of
  `ProjectIo` and `ApplyFailed`, an `io::Error` of kind `PermissionDenied`
  gives `ErrorCode::PermissionDenied` with `params["path"]` equal to the
  path; an `io::Error::other("Access is denied. (os error 5)")` also gives
  `PermissionDenied` (the text rule `apply.rs` has); a `NotFound` error gives
  the caller's code. Run it and see the first assertion fail.
- Green: implement `io_failure` with exactly `apply.rs::io_error`'s rule
  (kind, then the two lowercase texts), and
  `pub(crate) fn write_atomic(path: &Path, bytes: &[u8], code: ErrorCode) -> Result<(), IpcError>`
  that calls `secs_types::fs::write_atomic_bytes` and maps through
  `io_failure`. `app_settings.rs` and `project_edit.rs` call
  `write_atomic(.., ErrorCode::ProjectIo)` (keep `app_settings.rs`'s
  `create_dir_all` of the parent before it, mapped through `io_failure` too);
  `deploy.rs` calls `write_atomic(.., ErrorCode::ApplyFailed)`; `apply.rs`'s
  `io_error` becomes a call to `io_failure(.., ErrorCode::ApplyFailed)`
  (keep the name as a one-line wrapper only if that keeps the diff small).
  The direct `write_atomic_bytes` calls in `design_export.rs` and
  `export.rs` stay as they are.
- Deleted: the duplicated wrappers and the duplicated denied-text rule.
- Gate: `cargo test -p secs-service --locked` (the apply tests that assert
  `PermissionDenied` must still pass; a test that asserted `ProjectIo` or
  `ApplyFailed` for a permission failure is the behaviour change: update it
  and list it in the report).
- Commit: `fix(audit): X-25 원자적 쓰기 도우미 하나가 권한 거부를 늘 PermissionDenied로 돌려줍니다 (#186 A25 TD-208)`

## 2. B-17 · TD-197 (P193's text; `fs.rs` now exists)

## 3. B-20 · TD-216

## 4. B-15 · TD-264

## 5. B-10 · TD-187

## 6. B-22 · TD-218

## 7. B-26 · TD-229

## 8. B-23 · TD-219

P189 deleted `load_project_irs`/`LoadedIr`/`load_source` and P203 deleted
`secs_project::source_parse_count`; P203 made `secs_service::measure`
private, so the items in it are no longer reachable from outside: for
`secs-service` mark only items that are still `pub` and reachable from the
crate root.

## 9. X-28 · A28 · a reason at the `Deps.mock_measurer` field (Category C)

P193's first pass found that `crates/secs-service/tests/measure_ipc.rs`
injects `Deps.mock_measurer` twice to pin the mock's delay and room, so the
field is the test seam of the `mock` method, not a copy of the
`audio_backend` seam (that one is the real device; the mock method must work
without one). Decision: keep the field. Write one doc-comment sentence on
it saying so and citing `audit #186 A28`. No code change.

Commit: `chore(audit): X-28 모의 측정 공장이 모의 방법의 시험 seam인 이유를 적습니다 (#186 A28)`

## Verification (foreground, at the end)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-project -p secs-audio -p secs-rew -p secs-types -p secs2r -p secs2r-cli --locked --no-fail-fast
cargo test -p sound-equalization-and-correction-system-rust-api --all-features --locked
cargo +nightly-2026-10-01 miri test --locked -p secs-project --lib
cargo +nightly-2026-10-01 miri test --locked -p secs-project --test golden -- wav_reader truncated_wav_data
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-svc-b2
```

The Miri lines are required because B-10, B-22 and B-26 change
`secs-project`, which reads untrusted files (CLAUDE.md, 검사). Known load
flakes (`cached_resample_replay_releases_read_memory_before_calibrating_td130`
in `tests/cache.rs` or wherever B-3 moved it) pass alone; rerun before you
treat a failure as yours. If the classifier refuses a file deletion an item
needs, leave the file with a one-line note and list it in the report.

## Report

P193's report format per item, then the X-25 behaviour change list (which
tests changed their expected code), anything reclassified, anything left,
and the final `git log --oneline`. Finish the report before ending your
turn.
