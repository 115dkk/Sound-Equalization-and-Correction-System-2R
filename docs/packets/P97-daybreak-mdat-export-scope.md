# P97 (Daybreak): what the REW mdat export carries, and file names that say it

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\dirac-liveproject-support-2e310d`
(branch `claude/mdat-export-scope`). Work only there. Do not commit, do not
push, do not spawn sub-agents. Claude reviews the diff and commits. ASTRA
edits files under `apps/secs2r/ui/` in the same tree at the same time; do not
touch anything there.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0015-mic-calibration-in-calculation.md`,
`crates/secs-export/src/lib.rs` (`before_after_items_with_mimo`,
`measured_item`, `export_items_using`), `crates/secs-service/src/export.rs`,
`crates/secs-service/src/export_folder.rs`, `crates/secs-service/src/session.rs`
(`write_side_mdat`), `crates/secs-mdat/src/builder.rs` (how `computed`,
`spl_offset_db` and `source_info` are written), `crates/secs-types/src/ipc.rs`
(`MdatScope`, `ExportParams::mdat_scope`; Claude already added them and
regenerated the bindings, do not change them).

## Why

The export screen's REW mdat (`ExportKind::BeforeAfterMdat`) gets a choice:
everything (default), the original measurements only, or the correction
results only. "Measurements only" must work before any correction exists, so
a user can measure with SECS 2R and open every position's measurements in REW.
File names must say what the file holds.

There is also a defect from 0.29.0 (D45) to fix here. The before/after file's
measured items (`NN_ROLE_1 보정 전(원 측정)`) take the engine's IR, which now
has the attached mic curves already removed (`calibration::measured_ir`), and
then attach the same curves (`source_info` with `meterCal`/`scCal`). REW
removes them a second time. The computed items (`_2`, `_3`) have the same
problem: they are computed from curve-free IRs but carry the curves. With a
resampling policy the measured items are also the resampled IR, not the
stored one.

## Rules (fixed)

| scope | items | file name |
| --- | --- | --- |
| `All` (default) | upstream before/after items at the main position, measured items replaced by the stored originals (below), computed items without curves; then the stored originals of every other position | `SECS 원 측정+보정 결과 {label}.mdat` |
| `Measurements` | the stored originals of every position, nothing else | `SECS 원 측정.mdat` |
| `Results` | the computed items only (`_2`, `_3`), without curves; no measured items, no subs | `SECS 보정 결과 {label}.mdat` |

`{label}` is `run.settings.label()` (for example `저지연 모드`,
`초고지연 모드 120 ms`). A name holds `원 측정` exactly when the file holds
measurements and `보정 결과` exactly when it holds computed responses.

"Stored original" = exactly what `write_side_mdat` writes today for one
record and role: title `{role}_P{pos}`, notes `SECS · 위치 {pos} · 원 측정`,
the WAV's samples and rate as stored (`store.ir_of`, never `measured_ir`),
`t0` of the record, the record's REW SPL offset, the sidecar `source_info`
with its curves, `computed: false`. Order: positions ascending, roles in plan
order (as `write_side_mdat`).

In `All`, a measured item at the main position keeps its upstream title and
notes (`NN_ROLE_1 보정 전(원 측정)`, `... 원 측정 그대로(필터 없음)...`) but
takes samples, sample rate, start time, SPL offset and `source_info` from the
stored original of the same `(position, role)`. If no original exists for it
(should not happen), keep the engine item and clear its curves. Subs are
measured items too. "Without curves" = the item's `source_info.cal` emptied;
every other source field stays.

Default folders (only when `params.folder` is empty): `All` and `Results` keep
`export_folder::mdat_folder` (`SECS_mdat_{RC_BA|RC_BA_MIMO}_{mode}_{stamp}`);
`Measurements` uses a new `measurement_mdat_folder(base, now)` =
`SECS_mdat_measurements_{stamp}`.

The upstream functions `before_after_items`, `before_after_items_mimo`,
`export_before_after_mdat*` must keep their exact behaviour and the goldens
in `crates/secs-export/tests/golden.rs` must pass unchanged. The service stops
calling `export_before_after_mdat_with_mimo_using` and calls the new function.
`StagedMdat` is unchanged.

## secs-export API (add; keep existing signatures)

```rust
pub use secs_types::ipc::MdatScope;

/// One stored measurement, as the project's side mdat writes it.
#[derive(Clone, Debug)]
pub struct Original {
    pub position: u32,
    pub role: String,
    pub item: MeasurementInput,
}

/// `SECS 원 측정.mdat`, `SECS 보정 결과 {label}.mdat`, `SECS 원 측정+보정 결과 {label}.mdat`.
pub fn rew_mdat_filename(scope: MdatScope, label: &str) -> String; // label unused for Measurements

pub fn rew_mdat_items(
    run: Option<&ExportRun<'_>>,
    scope: MdatScope,
    originals: &[Original],
    mimo: Option<&MimoResponses>,
    sources: Option<&Sources>,
) -> Result<Vec<MeasurementInput>, ExportError>;

pub fn export_rew_mdat_using(
    run: Option<&ExportRun<'_>>,
    scope: MdatScope,
    originals: &[Original],
    folder: &Path,
    mimo: Option<&MimoResponses>,
    sources: Option<&Sources>,
    spl_offset_db: Option<f64>,
    now_ms: i64,
    uuid: &mut dyn FnMut() -> Result<u128, ExportError>,
) -> Result<PathBuf, ExportError>;
```

New `ExportError` variants: `NoMeasurements` (scope `Measurements` and
`originals` is empty; `All` and `Results` never need originals) and
`NoResult` (`run` is `None` for `All` or `Results`). Implement by giving the private
`before_after_items_with_mimo` an `originals: Option<&[Original]>` argument;
`None` is today's behaviour byte for byte. Never panic.

## secs-service

`session.rs`: move the per-record loop of `write_side_mdat` into
`pub(crate) fn original_measurements(open: &mut OpenProject) -> Result<Vec<secs_export::Original>, ServiceError>`
and build the side mdat from it. The side mdat's bytes must not change (its
test in `session.rs` stays as is).

`export.rs` `start_export`:

- `params.mdat_scope` is `Some(_)` with a kind other than `BeforeAfterMdat`:
  `ErrorCode::InvalidRequest`, param `reason` = `mdat_scope`.
- `scope = params.mdat_scope.unwrap_or_default()`.
- Tutorial stays refused first.
- `BeforeAfterMdat` + `Measurements`: no calibration needed, no
  `export_block` (a stale cache does not block it), no MIMO, no compute
  provenance (call `write_compute_provenance(folder, None)` so a reused folder
  loses old metadata, as other kinds do). Collect `original_measurements`
  while holding the session lock, before the job starts. No originals:
  `ErrorCode::EngineInput`, param `reason` = `no_measurements`, before the job
  starts. `ExportResult.stale` = false. Pass `spl_offset_db` the same way the
  side mdat does.
- `All` and `Results`: today's path (calibration required, `export_block`,
  design rebuild from the cache) but writing through `export_rew_mdat_using`.
  Collect originals for `All` only.
- Map `ExportError::NoMeasurements` to `EngineInput` + `reason=no_measurements`,
  `NoResult` to `EngineInput` (message "calibration must run before export").

`export_folder.rs`: `measurement_mdat_folder` (unit test next to the others).

`crates/secs-service/tests/service.rs`: add `mdat_scope: None` to every
existing `ExportParams` literal, then add:

- `measurement_mdat_exports_every_position_without_a_result`: a project with
  at least two positions (pick a fixture; say which), no calibration,
  `BeforeAfterMdat` + `Some(Measurements)` + empty folder: the job succeeds,
  the file is `SECS 원 측정.mdat` in a `SECS_mdat_measurements_*` folder, and
  reading it back (`secs_mdat::load_mdat_project`) gives one item per stored
  record and role with title `{role}_P{pos}` and the stored sample rate.
- `rew_mdat_scope_names_say_what_the_file_holds`: after calibration, `All`
  writes `SECS 원 측정+보정 결과 {label}.mdat` whose items are the main
  position's before/after items and then the other positions' originals;
  `Results` writes `SECS 보정 결과 {label}.mdat` with computed items only;
  `None` behaves as `All`.
- `exported_measurements_keep_their_curves_and_results_drop_them`: attach a
  mic curve to one measurement's sidecar (see
  `calibration.rs::attached_calibration_is_removed_before_the_engine` for how),
  calibrate, export `All`: that role's `_1` item holds the stored WAV samples
  (not the corrected IR) and its `meterCal`; its `_2` item has no `meterCal`.
- `mdat_scope_is_refused_for_other_kinds`.
- In `resampled_calibration_and_cached_mdat_export_preserve_original_ir`,
  after the stale-cache refusal, export `BeforeAfterMdat` +
  `Some(Measurements)`: it succeeds and `L_P1` keeps 96000 Hz.

`crates/secs-export`: tests in `tests/golden.rs` (new tests only) or a new
test file: `rew_mdat_scopes_choose_items_and_names`,
`all_scope_takes_stored_originals_and_strips_curves_from_results` (an
original at 96 kHz with a `meterCal` entry replaces the measured item; the
computed items carry no `cal`), `measurement_scope_needs_measurements_not_a_result`.

## CLI

`apps/secs2r-cli/src/main.rs`: `export` gains an optional
`--mdat-scope <all|measurements|results>` (clap `ValueEnum`), passed through;
other callers pass `None`.

## Allowed files

`crates/secs-export/src/lib.rs`, `crates/secs-export/tests/*.rs`,
`crates/secs-service/src/export.rs`, `crates/secs-service/src/export_folder.rs`,
`crates/secs-service/src/session.rs`, `crates/secs-service/tests/service.rs`,
`apps/secs2r-cli/src/main.rs`, and `crates/secs-export/examples/export_perf.rs`
only if it stops compiling. Nothing else: Claude edits `features.toml`, docs
and the changelog; ASTRA owns `apps/secs2r/ui/`.

## Verify (foreground; do not end your turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-export -p secs-service -p secs-types --locked
cargo test -p secs2r-cli --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-export -p secs-service --locked
```

## Report

Changed files, the tail of each command above, the fixture you used for two
positions, how the builder treats per-item vs global SPL offset (one line),
and anything you could not do.
