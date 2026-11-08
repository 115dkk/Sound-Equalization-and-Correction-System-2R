# P208 (Daybreak): audit #186, X-20, X-21, X-27 and the rest of B-22

Worktree `E:\wt\audit-svc`, branch `audit/186-svc-c` (Claude creates it
from `fix/audit-186` after P207 has landed; `git status` is clean when you
start). Every cargo command runs with
`CARGO_TARGET_DIR=C:\cargo-target\audit-api`; you are the only one building
there. Never run `cargo clean`, never delete anything under
`C:\cargo-target`. No workspace-wide `cargo test`; per-package (`-p`) only,
plus one `cargo clippy --workspace --all-targets --locked -- -D warnings` at
the end. One commit per item, trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push, do
not rebase, do not touch ADRs, `docs/**`, `CHANGELOG.md`, `audit-state.md`
or the UI. If the classifier refuses a file removal an item needs, leave the
file with a one-line note and list it in the report.

Allowed files: `crates/secs-service/src/**`, `crates/secs-service/tests/**`,
`features.toml` (registrations for tests you add or rename).

## 1. X-20 · A20 · the `host.rs` forwarders go (Category B)

`crates/secs-service/src/host.rs` holds seven methods
(`dialog_open_project`, `dialog_save_project`, `dialog_pick_mdat`,
`dialog_pick_folder`, `dialog_save_file`, `dialog_open_file`, `open_folder`)
that only forward to `self.deps.host.*`. `ipc.rs` calls `deps.host` directly
instead, keeping each forwarder's parameter mapping inline at the call site,
and `host.rs` and its `mod` line go. Behaviour and wire answers stay the
same; the existing tests are the gate (no new tests).

Commit: `refactor(audit): X-20 대화상자와 폴더 열기의 서비스 전달 메서드를 지우고 ipc.rs가 호스트를 직접 부릅니다 (#186 A20)`

## 2. X-21 · A21 · `curves::dispatch` goes (Category B)

`crates/secs-service/src/curves.rs` `dispatch` is a second dispatcher with an
`unreachable!` arm, called from `ipc.rs` for four requests
(`MeasurementCurves`, `ResultCurves`, `ResultLines`, `SubTakeValues`).
`ipc.rs` matches those four requests itself and calls the service methods;
`dispatch` goes. Existing tests are the gate.

Commit: `refactor(audit): X-21 곡선 요청의 두 번째 dispatcher를 지우고 ipc.rs가 직접 고릅니다 (#186 A21)`

## 3. X-27 · A27 · one public import entry point, and Dirac rows keep a source (Category A)

Audit #186 A27: the shared entry point (`measurement_file::{open, import}`)
is private while the two format-specific ones are `pub` and only tests use
them; Dirac rows carry no sidecar (`dirac_import.rs`, `source_info: None`).

- `crates/secs-service/src/lib.rs`: `pub mod measurement_file;` with
  `open` and `import` (and the types their signatures need) public and
  documented in one line each. `mdat_import::import` and
  `dirac_import::import` are already `#[deprecated]` (B-23); change their
  deprecation note to name `measurement_file::import` as the replacement.
- Dirac rows get a `SourceInfo`. Read `secs_mdat::SourceInfo`
  (`crates/secs-mdat/src/source_info.rs`) and how the side mdat writer uses
  it, and fill only what a Dirac project really carries: the measurement's
  title in the form the mdat import uses for its rows, and `notes` naming
  the Dirac file. Leave the other fields empty. `measurement_file::import`
  then saves the sidecar like it does for mdat rows.
- Red: `dirac_rows_keep_a_source_with_the_file_name_td219` in
  `crates/secs-service/tests/dirac.rs` (or the file that already imports a
  Dirac fixture): after an import, `.src.json` exists for every recorded IR
  and the store's `source_of(role, pos)` has the title and the file name.
  It fails today (no sidecar).
- Gate: `cargo test -p secs-service --locked --no-fail-fast` (the mdat import
  goldens and the Dirac tests unchanged apart from the new assertion).
- Commit: `fix(audit): X-27 가져오기의 공개 진입점이 하나로 합쳐지고 Dirac 측정도 출처를 남깁니다 (#186 A27)`

## 4. B-22 rest · TD-218 · `MeasurementSettings` derives `Clone` (Category B)

`secs_mdat::CalFile` already derives `Clone`, so the hand-written
`impl Clone for MeasurementSettings` in
`crates/secs-service/src/measure/settings.rs` becomes `#[derive(Clone)]` if
every field is `Clone`; if a field is not, say which and keep the impl.
Existing tests are the gate.

Commit: `refactor(audit): B-22 측정 설정이 손으로 쓴 Clone 대신 derive를 씁니다 (#186 TD-218)`

## Verification (foreground, at the end)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-types -p secs2r -p secs2r-cli --locked --no-fail-fast
cargo test -p sound-equalization-and-correction-system-rust-api --all-features --locked
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-svc-c
```

Known load flakes pass alone; rerun before you treat one as yours.

## Report

Per item: the before/after counts (B) or the Red test and its first failure
line (A), the commit hash, the files, what was removed, and the final
`git log --oneline`. Finish the report before ending your turn.
