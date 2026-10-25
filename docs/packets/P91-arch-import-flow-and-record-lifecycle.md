# P91: audit #92 C6 (the store owns a record's lifecycle, scoped) and C3 (one measurement-file import flow, two readers)

Claude names the worktree and branch when it hands this over. Do not switch
branches, push, or spawn sub-agents. Commit once per item with the subject
given; Claude reviews, re-runs the gates and re-authors the commits.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0004-parity-and-deviations.md`,
`docs/adr/0013-dirac-liveproject-import.md`, `docs/DEVIATIONS.md` (F5),
`crates/secs-project/src/{record.rs,project.rs,lib.rs}`,
`crates/secs-service/src/{mdat_import.rs,dirac_import.rs,measurement.rs,session.rs,export.rs}`,
`crates/secs-service/tests/{mdat.rs,dirac.rs,service.rs}`, the upstream
`src/secs/session.py` (`record`) and `src/secs/wizard/step5.py`
(`load_mdat`) for what upstream writes and in what order.

Results stay byte-identical: every upstream `.secs` byte test and golden in
`secs-project` and `secs-service` must pass with unchanged assertions. Both
items are deepenings (resolve-audit category A): tests at the new interface
first, then move behaviour behind it, then delete the old tests the new ones
cover (name each).

## 1. C6, scoped to the record lifecycle

The user scoped C6 to the record's lifecycle; typed accessors for
`Project.extra` sections are out of scope.

- **Source with the record.** `MeasurementResult.source` exists but
  `MeasurementStore::record` ignores it; the importers then patch the
  measurement JSON with `mdat_import::record_source` and save a second time
  per IR. Make `record` write the `source` entry itself (same key position
  and value shape as `record_source` produces today, so the bytes are
  identical) and save once. Delete `record_source`.
- **Remove and restore a position.** `position_clear` and `undo` in
  `measurement.rs` splice raw JSON and lose the position's SNR values.
  Add to `MeasurementStore`:
  ```rust
  /// A removed position: its measurement entries (with their list indices)
  /// and the SNR values the store held for it.
  #[derive(Clone, Debug)]
  pub struct RemovedPosition { /* private fields */ }
  pub fn remove_position(&mut self, pos: i64) -> Option<RemovedPosition>;
  /// Puts a removed position back; fails if that position was recorded
  /// again meanwhile.
  pub fn restore_position(&mut self, removed: RemovedPosition) -> Result<(), ProjectError>;
  ```
  `clear_position` stays only if something else needs it. The service's
  `UndoPayload::Position` holds a `RemovedPosition`; the service keeps its
  error codes and messages and its save-and-roll-back behaviour.
- **Confined source paths.** P77 (TD-43) confined `load_project_sources` to
  `.irs`; check that every path the store resolves for a record (IR and
  sidecar) goes through the confined resolver and say so.
- Tests at the store's interface: `record` with a source writes it and the
  project file is saved once (count saves through a test hook or the file's
  modification sequence; say which); `remove_position` then
  `restore_position` gives back the identical project JSON and the SNR
  values; restore after re-recording fails.
- Commit: `refactor(audit): A-75 측정 기록의 출처, 지우기와 되돌리기를 MeasurementStore가 맡습니다 (#92 C6)`

## 2. C3 — one import flow, two readers

Today the mdat and Dirac paths each read the file with a 256 MiB limit,
build rows, validate assignments and loop over records
(`mdat_import.rs` `load`/`import`, `dirac_import.rs` `load`/`import`); the
service branches on the extension three times (`measurement.rs`, the
`mdat_scan` handler, the mdat job's title pass and its import call) and parses
the file up to three times per import; position checks differ (mdat
`pos > count`, Dirac `1..=count`).

Design (fixed):
```rust
// crates/secs-service/src/measurement_file.rs (new, private module)
/// One row of a measurement file as the assignment dialog shows it.
pub(crate) struct FileRow {
    pub title: String,
    pub position: i64,          // suggested, 1-based
    pub role: Option<String>,   // suggested
    pub looks_like_sub: Option<bool>,
}
/// A format reader: the rows of a file and the IR of each row.
pub(crate) trait MeasurementReader {
    fn rows(&self) -> &[FileRow];
    /// The IR, its sample rate, t0 and the record's uuid and source info.
    fn take(&self, row: usize) -> Result<ImportedIr, MeasureError>;
}
/// Reads the file once (256 MiB limit) and picks the reader by extension.
pub(crate) fn open(path: &Path, roles: &[String], position: Option<i64>, current: i64) -> Result<Box<dyn MeasurementReader>, MeasureError>;
/// Validates assignments (one rule: role in the plan, `1..=position_count`,
/// one record per (position, role)), records every IR with its source and
/// saves once per IR (C6), and returns the count.
pub(crate) fn import(store: &mut MeasurementStore, reader: &dyn MeasurementReader, file_name: &str, assignments: &[Assignment]) -> Result<usize, MeasureError>;
```
`ImportedIr` is a small struct you define. The mdat reader keeps the
upstream rules (`split_position`, `match_role`, `auto_assign`,
`suggest_assignment`, `problems`, `looks_like_sub`) as the pure functions
they are, with their goldens; the Dirac reader keeps `base_role` and its
naming. The service handlers call `open` once per request and `import`
once per job (the job reads the file once). `dirac_import` stops being a
`pub mod` if nothing outside the crate needs it.

- The unified position rule is `1..=position_count`. If upstream
  `step5.load_mdat` accepts positions the old mdat check accepted
  (`pos <= 0`), say so; a position below 1 cannot come from the dialog, so
  keep the stricter rule and add a `docs/DEVIATIONS.md` entry only if
  upstream really accepts it.
- Tests at the new interface with a fake reader (no file): assignment
  validation (out of range, duplicate, role not in plan), records carry the
  source, one save per IR; and one test per real reader that `open` on the
  existing sample files yields the rows the old scans did. Then remove the
  service tests that only re-checked the duplicated loops (name them); keep
  the end-to-end `tests/mdat.rs` and `tests/dirac.rs` behaviour tests.
- Commit: `refactor(audit): A-74 측정 파일 가져오기를 흐름 하나와 형식 reader 둘로 나눕니다 (#92 C3)`

## Allowed files

`crates/secs-project/src/**`, `crates/secs-project/tests/**`,
`crates/secs-service/src/**`, `crates/secs-service/tests/**`,
`docs/DEVIATIONS.md`, `Cargo.lock`. Nothing in `apps/`.

## Gates (foreground; do not end the turn before they finish)

Export `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only`.
After each item: `cargo fmt --all -- --check`;
`cargo clippy -p secs-project -p secs-service -p secs2r-cli --all-targets --locked -- -D warnings`;
`cargo test -p secs-project -p secs-service -p secs2r-cli --locked`, with
counts before item 1 and after each item. `secs-project` reads untrusted
files: after item 1 also run
`cargo +nightly-2026-10-01 miri test --locked -p secs-project` (a name
filter for the new tests is fine if the whole crate is too slow; say which).
At the end `python3 .github/scripts/check_feature_tests.py`.

## Report (final answer)

Per item: commit hash, files, tests added and removed, counts before/after,
the save-count method, the upstream finding on positions, Miri result, tail
of the last gate output, anything left undone.
