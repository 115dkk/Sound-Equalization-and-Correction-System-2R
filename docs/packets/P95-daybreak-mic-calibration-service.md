# P95 (Daybreak Blue): attached calibration curves in the store, the engine inputs and IPC

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\dirac-liveproject-support-2e310d`
(branch `claude/mic-calibration`). Work only there. Do not commit, do not
push, do not spawn sub-agents. Claude reviews the diff and commits. Nobody
else edits this tree while you work. Do not touch `apps/secs2r/ui/src/`
except the generated `apps/secs2r/ui/src/ipc/bindings/` that
`cargo test -p secs-types` rewrites.

Read first: `docs/adr/0015-mic-calibration-in-calculation.md` (the decision;
it is the specification of behaviour), `CLAUDE.md`, P94
(`docs/packets/P94-astra-mic-calibration-curve.md`, already merged into this
tree: `MicCalibration::{parse_checked, from_points, serial, analog_gain_db,
notes, has_phase}`, `CalibrationError`, `CalibrationIssue`,
`ResponseCorrection::new(meter: Option<MicCalibration>, others: Vec<MicCalibration>, c_weighted: bool)`), `crates/secs-mdat/src/source_info.rs`
(`SourceInfo`, `CalInfo`, `CalFile`, `from_rew_measurement`),
`crates/secs-project/src/record.rs` (`MeasurementStore::{open, record}`,
`save_source`, `load_source`, `load_project_sources`, `confined_ir_path`),
`crates/secs-service/src/calibration.rs` (`load_measurements_cancellable`,
`signature_key`), `crates/secs-service/src/curves.rs`,
`crates/secs-service/src/measure/{native.rs,settings.rs,rew.rs}`,
`crates/secs-service/src/measurement.rs` (`measure_configure`,
`store_devices`, `refuse_while_measuring`), `crates/secs-types/src/ipc.rs`.

## What exists

REW measurements already save a sidecar `SourceInfo` whose `cal` list holds
`("meterCal", CalInfo { freq, gain, phase, inverse_c_selected, .. })` built
by `secs_mdat::from_rew_measurement` from REW's cal file. mdat imports keep
the sidecar of the file. Native sweeps save no sidecar. Nothing applies the
curves: `load_measurements_cancellable` and the curve readers hand raw IRs to
the engine. `MeasurementSettings::from_project` already parses
`devices.mic_calibration` (text) but only the level check uses it, and no
writer sets it.

## 1. `secs-project`: which curves a sidecar attaches, and their fingerprints

In `record.rs` (export from `lib.rs`):

```rust
/// The calibration a measurement's sidecar attaches, as ADR 0015 applies it:
/// `meterCal` then `scCal` entries whose `freq` and `gain` arrays are both
/// present with equal length >= 2, and C weighting when `meterCal` has
/// `inverse_c_selected`.
pub struct AttachedCalibration<'a> {
    /// The `meterCal` entry when it has curve arrays.
    pub meter: Option<&'a CalInfo>,
    /// `scCal` entries with curve arrays.
    pub others: Vec<&'a CalInfo>,
    /// `meterCal.inverse_c_selected` (a C-weighted SPL meter).
    pub c_weighted: bool,
}
impl AttachedCalibration<'_> { pub fn is_empty(&self) -> bool; }
pub fn attached_calibration(info: &SourceInfo) -> AttachedCalibration<'_>;
/// FNV-1a 64 over each attached curve's name, the C flag, and the f32 bits
/// of freq, gain and phase (with length prefixes and a marker for a missing
/// phase). None when nothing is attached.
pub fn calibration_fingerprint(info: &SourceInfo) -> Option<u64>;
```

`Project` gains one field, not saved:

```rust
/// Fingerprints of the calibration attached to each measurement's sidecar
/// (`calibration_fingerprint`), by position and role. Not saved: the store
/// reads them from the sidecars when it opens a project and updates them
/// when it records a measurement or replaces a sidecar (ADR 0015).
pub measurement_calibration: BTreeMap<(i64, String), u64>,
```

Empty by default and from `from_json`. `MeasurementStore::open` fills it from
`load_project_sources` (same confinement). `MeasurementStore::record` sets
or removes the entry for `(pos, role)` from `result.source_info`. Fix every
`Project { .. }` literal the compiler finds.

New store methods:

```rust
/// The sidecar of a recorded IR (None when the IR or its sidecar is absent,
/// unreadable, or outside the project's `.irs`, as `load_project_sources`).
pub fn source_of(&mut self, role: &str, pos: i64) -> Option<SourceInfo>;
/// Rewrites the sidecar of a recorded IR and its fingerprint entry. The IR
/// file and the project's measurement entries are not touched.
pub fn replace_source(&mut self, role: &str, pos: i64, info: Option<&SourceInfo>) -> Result<(), ProjectError>;
```

## 2. `secs-service`: curves removed before the engine and the graphs

- One helper, e.g. in `calibration.rs`:
  `pub(crate) fn measured_ir(store: &mut MeasurementStore, role: &str, pos: i64) -> Result<(Vec<f32>, u32), ServiceError>`:
  `ir_of`, then `source_of`, then `attached_calibration` turned into
  `secs_audio::ResponseCorrection::new(meter, others, c_weighted)` (each
  curve through `MicCalibration::from_points`, phase only when the phase
  array is present with the same length; a curve that fails validation is
  left out, the rest still apply; C weighting applies outside the meter
  curve's range, which `ResponseCorrection` already handles), then `apply`.
  An empty correction returns the IR untouched.
- Use it in `load_measurements_cancellable` and in both IR reads of
  `curves.rs` (measurement curves and the SPL/target curve). Do NOT use it in
  `session.rs`'s mdat export of the original measurements (raw IR plus the
  sidecar, so REW removes the curve itself) or anywhere that writes files.
- `signature_key(project)`: when `project.measurement_calibration` has
  entries for `(pos, role)` pairs that the current measurement entries hold
  (an `ir` path for that role), append `:cal` and the sorted
  `Vec<(i64, &str, u64)>` debug string. With no such entry the string must be
  exactly what it is today, so caches of projects without curves stay valid,
  and the cache of a project whose REW measurements carry a curve reads as
  stale ("measurements").

## 3. Native sweeps carry the project's file

`MeasurementSettings` gains `pub calibration_file: Option<secs_mdat::CalFile>`
from `devices.mic_calibration` (text bytes) and `devices.mic_calibration_path`
(falling back to `devices.mic_calibration_name`, then `"mic calibration"`).
`NativeMeasurer` gains a clock (`Arc<dyn Fn() -> i64 + Send + Sync>`, the
type of `Deps::unix_ms`): `from_backend` keeps its signature and uses the
system clock, `pub fn with_clock(self, clock) -> Self`, and `measurer()` in
`measurement.rs` passes `deps.unix_ms`. `measure_position` sets each result's
`source_info` to `from_rew_measurement(Some(file), "", "", "", now)` when a
file is set (None otherwise, as today). Mock sweeps do not change.

## 4. IPC (secs-types, service, host)

Types in `crates/secs-types/src/ipc.rs`, documented like their neighbours:

```rust
pub struct MicCalibrationFile {
    pub name: String,            // file name as chosen
    pub path: String,            // full path as chosen (the project keeps the text)
    pub serial: Option<String>,  // miniDSP SERNO
    pub sens_factor_db: Option<f64>,
    pub analog_gain_db: Option<f64>,
    pub sensitivity_dbfs: Option<f64>, // analog "Sensitivity ... dBFS"
    pub notes: Vec<String>,
    pub points: u32,
    pub min_hz: f64,
    pub max_hz: f64,
    pub has_phase: bool,
}
pub struct MicCalibrationStatus {
    pub file: Option<MicCalibrationFile>,
    /// Measurements (position x role) whose sidecar attaches a `meterCal` curve.
    pub attached: u32,
    /// Sweep measurements (no `source` value for the role: native, REW and
    /// mock sweeps) without a `meterCal` curve; `mic_calibration_attach` gives them the project's file.
    pub attachable: u32,
}
pub struct MicCalibrationSetParams { pub path: String }
```

Methods, all answering `MicCalibrationStatus`, all needing an open project:

- `mic_calibration_get`.
- `mic_calibration_set { path }`: refuse while measuring. Read at most
  4 MiB (more is an error). Decode: UTF-8 (strip a BOM); a UTF-16 LE/BE BOM
  decodes as UTF-16; a NUL byte or invalid UTF-8 is "not text". Parse with
  `MicCalibration::parse_checked`. Store `mic_calibration` (the decoded
  text), `mic_calibration_name` (file name) and `mic_calibration_path` with
  `store_devices`, mark the project dirty, leave the measurements alone.
- `mic_calibration_clear`: refuse while measuring; remove the three keys;
  dirty.
- `mic_calibration_attach`: refuse while measuring; error when no file is
  set. For every recorded `(pos, role)` that is a sweep (as `attachable`)
  and has no `meterCal` curve: take its sidecar (or an empty `SourceInfo`
  with no date), put the `meterCal` entry that `from_rew_measurement` builds
  for the project's file in place of any existing `meterCal`, set boxed
  `usesEmbeddedCalData` true (replace, not duplicate), and
  `replace_source`. Measurements with a `source` value (mdat, Dirac
  imports) and those that already carry a `meterCal` curve stay as they are.
  The project file does not change (not dirty); the signature does.

Errors: `ErrorCode::MeasurementInput` with params `reason = "mic_calibration"`,
`issue` one of `unreadable`, `too_large`, `not_text`, `header`, `row`,
`order`, `range`, `too_few_points`, `both_conventions`, `no_file` (attach
without a file), plus `line` (1-based) when the parser gives one and `file`
(the file name). `detail` carries the raw error.

Register the four methods like the others (Request/Response variants, the
dispatcher in `crates/secs-service/src/ipc.rs`, method lists and the tests
that enumerate them, `rules.rs` if it lists methods). Add
`OpenFileKind::MicCalibration`; in `apps/secs2r/src/host.rs` its dialog
filters are "Mic calibration" `["txt", "cal", "frd"]` then "All files"
`["*"]`. Fix any other exhaustive match the compiler finds (CLI, test
hosts). Regenerate the bindings with `cargo test -p secs-types`.

## Tests (names cited by `features.toml`; put service tests where similar ones live)

- `secs-project::store_reads_and_updates_calibration_fingerprints`: open
  reads fingerprints from sidecars; `record` with and without a curve sets
  and removes the entry; `replace_source` updates it; a sidecar outside the
  `.irs` is ignored.
- `secs-service::attached_calibration_is_removed_before_the_engine`: a flat
  +6 dB `meterCal` scales the loaded IR by `10^(-6/20)` (1e-6 relative); a
  measurement without a sidecar is bit-exact; an invalid embedded curve is
  ignored without an error.
- `secs-service::measurement_curves_use_the_corrected_ir`.
- `secs-service::calibration_signature_keeps_old_caches_without_curves`:
  `signature_key` equals `format!("{:?}:{identities:?}", measurement_signature(..))`
  for a project with no curves; it changes when a curve is attached; a
  cached result written before the curve reads as stale "measurements".
- `secs-service::native_sweeps_carry_the_project_mic_calibration` (use the
  fake audio backend the native tests already use): the recorded sidecar has
  the file's `meterCal` arrays, `sourceName`, and the clock's date.
- `secs-service::mic_calibration_set_get_clear_and_errors`: a synthetic
  UMIK-1 90° file (CRLF, quoted description) gives the expected
  `MicCalibrationFile`; dirty after set and clear; each error `issue`
  (`unreadable`, `too_large`, `not_text`, a `row` with its `line`); UTF-16 LE
  decodes.
- `secs-service::mic_calibration_attach_gives_the_file_to_sweeps_only`: a
  sweep without a curve gets it; a REW-like sweep with its own `meterCal`
  keeps it; an imported measurement (with `source`) is untouched; results
  become stale; `attachable` drops to 0; `no_file` without a file.
- Keep `secs-types::every_method_name_is_listed_and_parses` passing with the
  new names.

## Allowed files

`crates/secs-project/src/{record.rs,project.rs,lib.rs,signature.rs}` and its
tests; `crates/secs-service/src/**` and `crates/secs-service/tests/**`,
`crates/secs-service/examples/**` only to fix compilation;
`crates/secs-types/src/**` and the generated
`apps/secs2r/ui/src/ipc/bindings/**`; `apps/secs2r/src/host.rs`;
`apps/secs2r-cli/**` only to fix compilation; `Cargo.lock` as cargo changes
it. Not `crates/secs-audio` (P94 owns it; report if you need a change there),
not `features.toml`, docs or the changelog (Claude).

## Verify (foreground; do not end your turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --workspace --locked
python .github/scripts/upstream.py check-tree
```

All existing golden and service tests must pass unchanged. If one changes
because of the curves, stop and report which and why instead of editing the
expectation.

## Report

Changed files, the output tails of the commands, the exact IPC method and
type names as they landed, and anything unfinished.
