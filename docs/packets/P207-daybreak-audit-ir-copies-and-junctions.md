# P207 (Daybreak): audit #186, B-10 and B-17's junction question as Category A items

Worktree `E:\wt\audit-svc`, branch `audit/186-svc-a` (Claude created it
from the current `fix/audit-186`, which carries P205; `git status` is clean
when you start). Every cargo command runs with
`CARGO_TARGET_DIR=C:\cargo-target\audit-api`. Never run `cargo clean`,
never delete anything under `C:\cargo-target`, check after the first
command that no build output appears under `E:\wt\audit-svc\target`. No
workspace-wide `cargo test`; per-package (`-p`) only, plus one
`cargo clippy --workspace --all-targets --locked -- -D warnings` at the end.
One commit per item, trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push, do
not rebase, do not touch ADRs, `docs/**`, `CHANGELOG.md`, `audit-state.md`
or the UI. Two other workers run at the same time: P196 (doc comments and
documents, in `E:\wt\audit-api`) and later P197 (tests, fuzz targets, in
`E:\wt\audit-host`); keep your diff to the files named below.

Both items change an interface or a behaviour that existing tests pin, so
they are Category A: the protocol is Red (the test that shows the new
contract fails on the current code), Green (the smallest change), then the
old tests that pinned the old contract are updated and listed in the
report.

## 1. B-10 · TD-187 · one copy of an IR instead of four

Finding (audit #186): a 256 MiB IR has four copies (`read_bounded`'s buffer,
`payload.to_vec()`, the `Vec<f32>`, `IrCache::insert`'s `to_vec`), and every
cache hit clones the whole sample vector. P205 found that the fix changes
the public return type of `secs-project` (a published 0.x crate; a minor
version may break its API, ADR 0016) and that four tests assert an owned
`Vec<f32>`.

Decision: the cache value and the read path carry `Arc<[f32]>`.

- `crates/secs-project/src/irs.rs`: the WAV chunk walk returns the payload
  as a `Range<usize>` into the read buffer (callers slice `&data[range]`),
  so the payload is not copied before decoding.
- `crates/secs-project/src/record/cache.rs`: `CacheValue.samples:
  Arc<[f32]>`; `IrCache::load(&mut self, path) -> Result<(Arc<[f32]>, u32), ProjectError>`;
  `insert` takes the `Arc` it decoded (no `to_vec`); a hit returns
  `Arc::clone`. The byte accounting uses `samples.len()` as before.
- `crates/secs-project/src/record/core.rs`: `MeasurementStore::ir_of(&mut self, role, pos) -> Result<(Arc<[f32]>, u32), ProjectError>`.
- `crates/secs-service`: the callers (`calibration.rs` `measured_ir`,
  `curves.rs`, `mic_calibration.rs`, and wherever else the build points)
  take the `Arc`; a caller that needs to modify the samples (the mic
  calibration subtraction, for example) converts with `.to_vec()` at that one
  site, and says so in a comment.
- Red: in `crates/secs-project/tests/record.rs` (or the cache's unit tests),
  `ir_cache_hit_returns_the_same_allocation_td187`: load the same IR twice
  through `MeasurementStore::ir_of` and assert `Arc::ptr_eq` on the two
  sample buffers. It cannot compile today (owned `Vec`); write it with a
  temporary `Arc::from(vec)` adapter first if you need it to compile and fail
  on the assertion, then remove the adapter in Green.
- Update the four tests P205 named
  (`test_saved_ir_matches_measured_ir_and_ir_saver_paths_and_round_trip`,
  `f5_absolute_parent_and_symlink_escapes_are_rejected_but_in_tree_reads`,
  `ir_cache_rereads_changed_file_and_evicts_to_cap`,
  `ir_cache_keeps_its_byte_cap_and_evicts_least_recently_used_td44`) to
  compare `&*samples` with the expected slice; their assertions stay the same
  otherwise.
- Gate: `cargo test -p secs-project -p secs-service --locked --no-fail-fast`,
  both Miri lines of the verification section (secs-project reads untrusted
  WAV files), and the service goldens unchanged.
- Commit: `fix(audit): B-10 IR 하나를 읽을 때 사본을 하나만 두고 캐시 적중은 Arc를 돌려줍니다 (#186 TD-187)`

## 2. B-17 · TD-197 · `deploy.rs` and `apply.rs` refuse a junction like a symlink

P205 moved the link check into `crates/secs-service/src/fs.rs`
(`is_link_or_junction(metadata, LinkKind)`); `design_export.rs` passes
`LinkKind::Any` (a Windows reparse point counts) and `deploy.rs`/`apply.rs`
pass `LinkKind::Symlink`. Decision: `deploy.rs` and `apply.rs` pass
`LinkKind::Any` too, because a junction can take a write or a delete outside
the folder exactly as a symlink can, and `secs-graph` already refuses
junction parents (TD-270). The refusal error and its code stay the ones each
site returns for a symlink today. If `LinkKind::Symlink` then has no user,
remove the enum and the parameter.

- Red: a Windows-only test (`#[cfg(windows)]`) in
  `crates/secs-service/tests/apply.rs` (or the test file that already covers
  `deploy.rs`'s symlink refusal; search for the symlink test of the target
  you change), `a_junction_target_is_refused_like_a_symlink_td197`: create a
  junction with `cmd /c mklink /J <link> <target>` (the same pattern
  `crates/secs-graph/tests/files.rs` uses; read it), point the apply or
  deploy destination at it, and assert the same error code and `path` param
  the symlink case gets. It fails today because the junction is followed.
- Green: switch the call sites.
- Register the new test in `features.toml` next to the existing symlink
  refusal test of that feature (search for its name).
- Gate: `cargo test -p secs-service --locked --no-fail-fast`.
- Commit: `fix(audit): B-17 내보내기 적용과 배치가 junction도 심볼릭 링크처럼 거부합니다 (#186 TD-197)`

## Verification (foreground, at the end)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-project -p secs-service -p secs-types -p secs2r -p secs2r-cli --locked --no-fail-fast
cargo test -p sound-equalization-and-correction-system-rust-api --all-features --locked
cargo +nightly-2026-10-01 miri test --locked -p secs-project --lib
cargo +nightly-2026-10-01 miri test --locked -p secs-project --test golden -- wav_reader truncated_wav_data
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-svc-a
```

Known load flakes (`cached_resample_replay_releases_read_memory_before_calibrating_td130`,
`countdown_reports_seconds_and_cancel_writes_nothing`) pass alone; rerun
before you treat one as yours.

## Report

Per item: the Red test and its first failure line, the commit hash, the
files, the tests whose expectations changed and how, the public signature
changes of `secs-project`, and the final `git log --oneline`. Finish the
report before ending your turn.
