# P69 (Daybreak Blue): Dirac Live projects through the mdat import flow

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\dirac-liveproject-support-2e310d`
(branch `claude/dirac-liveproject-support-2e310d`). Work only there. Do not
commit, do not push, do not spawn sub-agents. Claude reviews the diff and
commits.

Read first: `docs/adr/0013-dirac-liveproject-import.md`,
`docs/DIRAC_LIVEPROJECT.md` (section '가져오기 규약'), `CLAUDE.md`,
`crates/secs-dirac/src` (P68, already built: `parse`, `Limits`,
`LiveProject::{measured, response, channels, positions, sample_rate}`,
`ChannelKind`, `NOMINAL_ARRIVAL_S`, and the `test_support` module behind the
`test-support` feature with `SyntheticProject` and `write`),
`crates/secs-service/src/mdat_import.rs`, `crates/secs-service/src/measurement.rs`
(`mdat_scan`, `start_mdat`), `crates/secs-service/tests/measure_ipc.rs`
(`mdat_scan_and_import_match_direct_oracle_bytes` and its helpers) and
`crates/secs-types/src/role.rs` (`ROLES`, `SUB_ROLES`).

## Goal

`MdatScan` and `MdatImport` accept a `.liveproject` path as well as `.mdat`.
The IPC types, the bindings and the job kind do not change. The mdat path's
behaviour and bytes do not change (its oracle tests must pass untouched).

## Design (fixed)

### `crates/secs-service/src/dirac_import.rs` (new, `pub mod dirac_import;` in `lib.rs`)

Module doc: Dirac Live `.liveproject` measurements through the mdat import
flow; new in SECS 2R (not in SECS 2); format in `docs/DIRAC_LIVEPROJECT.md`.

```rust
pub fn is_liveproject(path: &Path) -> bool; // extension, ASCII case-insensitive, "liveproject"

#[derive(Debug, Clone, PartialEq)]
pub struct DiracRow {
    pub title: String,               // "{channel title} P{position}"
    pub position: i64,               // Dirac position index + 1
    pub channel: usize,              // index into LiveProject::channels
    pub role: Option<String>,        // suggestion, see below
    pub looks_like_sub: Option<bool>,// mdat_import::looks_like_sub(ir, rate)
}
#[derive(Debug, Clone, PartialEq)]
pub struct DiracScan { pub rows: Vec<DiracRow>, pub sample_rate: u32 }

pub fn scan(path: &Path, roles: &[String]) -> Result<DiracScan, MeasureError>;
pub fn import(
    store: &mut MeasurementStore,
    path: &Path,
    assignments: &[mdat_import::Assignment], // one per scan row, same order
) -> Result<usize, MeasureError>;
/// Base role (an entry of ROLES) for a Dirac channel; subwoofers return None
/// here and get their SW number from their order in `scan`.
pub fn base_role(name: &str) -> Option<&'static str>;
```

- Loading: read the file with the same 256 MiB budget as
  `mdat_import::load` (`take(LIMIT + 1)`), then
  `secs_dirac::parse(&bytes, &Limits::default())`. Every error becomes
  `MeasureError::Input(message)`; `NoMeasurements` reads
  "Dirac project holds no measured impulse responses".
- Rows: `LiveProject::measured()` order (position by position, channels in
  list order). Unmeasured slots produce no row.
- Channel title: the Dirac name; when the same name occurs more than once in
  the channel list, append " 1", " 2" … by occurrence (`Subwoofer 1`,
  `Subwoofer 2`). Row title: `"{channel title} P{position}"`.
- Subwoofer channels: `ChannelKind::Subwoofer`, or a name whose words include
  `sub`, `subwoofer` or `lfe`. The k-th subwoofer channel in list order
  (k = 1..=8) has base role `SUB_ROLES[k-1]`; a ninth has none.
- `base_role(name)`: split into lowercase words at non-alphanumerics and at
  lower→upper case boundaries (`LeftSurroundBack` → left surround back,
  `TopFrontLeft` → top front left). Side from `left`/`right`
  (`center`/`centre` for C); both left and right, or none, gives None.
  Then, first match wins:
  - `center`/`centre` with no other word except `front`: `C`;
  - `top` with `front`: Tfl/Tfr; `top` with `middle` or `mid`: Tml/Tmr;
    `top` with `rear` or `back`: Tbl/Tbr; `top` alone: None;
  - `height` with `rear` or `back`: Hbl/Hbr; `height` otherwise: Hfl/Hfr;
  - `wide`: Lw/Rw;
  - `back` or `rear` (including `surround back`, `rear surround`): Lb/Rb;
  - `surround` or `side`: Ls/Rs;
  - only the side word, optionally with `front`: L/R;
  - anything else: None.
- Role suggestion per position, given `roles` (plan roles, possibly with a
  band suffix like `L:LF`): the first role in `roles` whose part before ':'
  equals the base role and which is not yet used at that position; None if
  there is none.
- `import`: re-parse the file, rebuild the rows the same way, and for each row
  i with `assignments[i].role = Some(role)` and `assignments[i].position =
  Some(pos)`: skip unless `role` is in the plan roles of
  `plan_for(store.project(), &MeasurementSettings::default())`, `1 <= pos <=
  position_count(store.project())` and `(pos, role)` is new in this import
  (the same rules as `mdat_import::import`). Record
  `MeasurementResult { role, pos, ir: samples as f32, sample_rate:
  positive(rate), t0_s: -secs_dirac::NOMINAL_ARRIVAL_S, spl_db: None,
  spl_offset_db: None, snr_db: None, uuid: None, source: Some(file name),
  source_info: None }` with `Some(&plan.roles)`, then write the per-role
  source name into the measurement entry exactly as `mdat_import::import`
  does, then `store.save()`. Return the number recorded. An assignment slice
  whose length differs from the rows is `MeasureError::Input`.
- Move the source-name block of `mdat_import::import` into
  `pub(crate) fn record_source(store: &mut MeasurementStore, pos: i64, role: &str, name: &str)`
  in `mdat_import.rs` and call it from both importers. This is the only
  change allowed in `mdat_import.rs`; it must not change any byte the mdat
  path writes.

### `crates/secs-service/src/measurement.rs`

- `mdat_scan`: when `dirac_import::is_liveproject`, call
  `dirac_import::scan(path, &ROLES as Vec<String>)` (the same role list
  `mdat_import::scan` uses) and build `MdatScanResult` from the rows:
  `measurements[i] = { index: i, title, role, position, position_tagged: true,
  looks_like_sub }`, suggestions for rows with a role
  (`index: Some(i)`, `position: Some(position)`), problems from
  `mdat_import::problem_notices`. Errors map through the same
  `measure_error(e, "path", …)` call.
- `start_mdat`: inside the job, take the row titles from
  `dirac_import::scan(path, &plan.roles)` instead of `mdat_import::scan` when
  the path is a liveproject; keep the existing assignment matching and
  validation loop working on that title list (factor it so both formats share
  it); then call `dirac_import::import(&mut open.store, path, &assignments)`
  instead of `mdat_import::import`. Everything else (identity check,
  `begin_commit`, progress, `JobResult::MdatImport`) stays.

### Elsewhere

- `crates/secs-service/Cargo.toml`: `secs-dirac.workspace = true`; under
  `[dev-dependencies]`, `secs-dirac = { workspace = true, features = ["test-support"] }`.
- `apps/secs2r/src/host.rs` `dialog_pick_mdat`: filters
  `("Measurements", ["mdat", "liveproject"])` first, then
  `("REW measurement", ["mdat"])` and `("Dirac Live project", ["liveproject"])`.
- `apps/secs2r-cli/src/main.rs`: the `ImportMdat` doc comment becomes
  "Import suggested assignments from a REW mdat or a Dirac Live project."
  Behaviour comes through the service; check that the command works with a
  liveproject (the test below may drive it through the service only).

## Tests (names fixed; `features.toml` will cite them)

Build inputs with `secs_dirac::test_support::{SyntheticProject, write}` into
a temporary directory under the test's own root (as the existing tests do).
Use responses that the sub classifier sorts clearly: a short decaying
broadband pulse for mains, a 40 Hz decaying sine for subwoofers.

In `crates/secs-service` (unit tests in `dirac_import.rs` or a new
`tests/dirac.rs`):

1. `dirac_channel_names_suggest_roles`: `base_role` for Left, Right, Center,
   Centre, LeftFront, LeftSurround, RightSide, LeftSurroundBack, RightBack,
   LeftRear, LeftWide, LeftTopFront, TopFrontRight, LeftTopMiddle,
   LeftTopRear, LeftHeight, LeftFrontHeight, RightRearHeight, "Left Surround"
   → the expected roles; Top Left, LeftRight, Unknown, "" → None.
2. `dirac_scan_lists_positions_channels_and_sub_flags`: a 2-position project
   with Left, Right, Subwoofer, Subwoofer and one unmeasured slot, scanned
   through `Request::MdatScan`: row count, titles (`Subwoofer 2 P1` …),
   positions, `position_tagged`, roles L/R/SW1/SW2, `looks_like_sub`.
3. `dirac_import_records_irs_t0_and_source`: a project whose plan has L, R,
   SW1, SW2 and at least two positions; `Request::MdatImport` with all
   suggestions; the job succeeds; for each imported (pos, role) the stored IR
   equals the parsed samples as f32, sample rate matches, `t0` is -0.25, the
   measurement entry's `source` for that role is the file name, and no
   `.src.json` exists for it.
4. `dirac_import_skips_unplanned_and_outside_rows_and_rejects_bad_files`:
   rows assigned to a position beyond the layout or a role outside the plan
   are skipped (direct `dirac_import::import`); a truncated file, a file with
   every slot unmeasured, and a missing path give `ErrorCode` input errors
   through `MdatScan` and leave the measurement store unchanged.
5. Every existing mdat test passes unchanged.

## Gates (foreground; all must pass, then report)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-service --locked
cargo test -p secs-dirac --all-features --locked
cargo test -p secs-policy --locked
cargo build -p secs2r --locked
cargo build -p secs2r-cli --locked
```

Then drive the real samples once (read only, never copy them):
`SECS_DIRAC_SAMPLES=<samples>`, a local folder outside the repository whose
path Claude gives when handing out the packet.
Create a throwaway project (9 positions, roles L, R, SW1, SW2) in your own
scratch directory with the CLI (`cargo run -p secs2r-cli -- …`; read its
`--help`), import `1.liveproject` with `import-mdat`, and report the command
lines and output. Delete the throwaway project afterwards.

## Allowed files

`crates/secs-service/src/dirac_import.rs` (new),
`crates/secs-service/src/lib.rs` (the `pub mod` line),
`crates/secs-service/src/mdat_import.rs` (the `record_source` move only),
`crates/secs-service/src/measurement.rs`, `crates/secs-service/Cargo.toml`,
`crates/secs-service/tests/dirac.rs` (new), `Cargo.lock`,
`apps/secs2r/src/host.rs`, `apps/secs2r-cli/src/main.rs`. Nothing else.

## Report

Files touched; any deviation from the design and why; each gate with its
result; the CLI session on the real sample (commands and output); the scan of
`1.liveproject` (titles, positions, roles, sub flags) as the service returns it.
