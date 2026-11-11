# P215 (Daybreak Blue): an export reports the work it has really done

Worktree `E:\wt\export-progress`, branch `feat/export-progress`. Build with
`CARGO_TARGET_DIR=C:\cargo-target\dirac-limit` set for every cargo command
(the worktree must not get a `target` folder of its own). Do not commit, do
not push. Run every command in the foreground and end your turn only after
the report. Do not touch anything under `apps\secs2r\ui` (the main session
is changing the screen there at the same time), the upstream Python tree, or
`CHANGELOG.md`.

## Why

The maintainer asked (2026-11-11) whether the bar in the job tray during an
export is the real progress of the save. It is not. `crates\secs-service\src\export.rs`
reports nothing while files are computed and written, except:

- `job.report_progress("export.camilla.prepare", 0, 1)`: a placeholder that
  stays at 0 of 1 until the job ends;
- the convolution export's `ctx.stage("filters.convolution", rates)` in
  `crates\secs-filters\src\lib.rs` (`prepare_convolution_named_with_options`),
  one step per output rate, so one rate is again 0 of 1;
- `design` and `read_measurements` when the design is recomputed first.

With no report the screen animates an indeterminate bar. The job must report
what it has done in real units, so the screen can stop pretending.

## The contract (what the screen and the tests rely on)

A job started by `Service::start_export` (kinds `export` and `apply`) emits
`JobEvent::Progress` in this order, after the stages of a recomputed design
(`design`, `read_measurements`, the engine's own stages), which stay as they
are:

1. **The contents**, counted in items. Stage id `export.prepare`, except the
   convolution export, whose stage id stays `filters.convolution` (it comes
   from the compute context inside `secs-filters`).
   - CamillaDSP, Equalizer APO, filter text: one item per FIR coefficient
     file materialized (`plan.firs()` of the exported plan). A design with no
     FIR emits no `export.prepare` at all.
   - Convolution: per output rate `2 * P` items, `P = plan.inputs.len() *
     plan.outputs.len()`: one item when a pair has been rendered (first pass
     of the length loop in `render_paths_at_cancellable` only; a repeated
     pass adds nothing), one item when a pair has been encoded (WAV and CSV
     bytes made) or was found to carry no response. The total is
     `rates * 2 * P`, fixed before the first report.
   - REW mdat (both kinds): one item per measurement serialized by
     `secs_mdat::build`.
2. **The files**, counted in bytes. Stage id `export.write` for every kind.
   `total` is the sum of the lengths of all files this export writes; `done`
   is the bytes written and synced so far.

Rules for every stage above:

- The first report of a stage is `(0, total)` and the last is
  `(total, total)`; `done` never decreases and never exceeds `total`;
  `total` is the same in every report of the stage and is never 0. A stage
  whose total would be 0 is not reported.
- `export.prepare` / `filters.convolution` ends before `export.write`
  starts. Nothing else is reported between them or after `export.write`.
- `export.camilla.prepare` is removed. No `(x, 1)` placeholder is added
  anywhere.
- Reports must not change what is written: the bytes, names and order of
  the files, the atomic staging, the commit point (`commit_write` /
  `begin_commit`) and cancellation stay exactly as they are. The golden
  tests of the exporters must pass unchanged.

## Signatures (settled; do not redesign)

Progress leaves the library crates through plain callbacks, because
`secs-graph`, `secs-camilla`, `secs-export` and `secs-mdat` do not depend on
`secs-compute` and must not start to. Every callback is
`&dyn Fn(u64, u64)` called with `(done, total)` under the rules above. Keep
every existing public function and its behaviour; each delegates to its new
sibling with a callback that does nothing.

`crates\secs-graph\src\files.rs`

```rust
/// Stages and installs a bundle like [`write_bundle_observed`], calling
/// `written(done, total)` in bytes: `(0, total)` before the first file and
/// after each staged file has been written and synced.
pub fn write_bundle_reported(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
    entry: &Path,
    written: &dyn Fn(u64, u64),
) -> Result<BundleInstall, std::io::Error>
```

Implement it on top of `write_bundle_observed` (its `after_stage_write`
hook gives the index of the file just staged). An empty total (no bytes at
all) calls nothing.

`crates\secs-graph\src\emit\fir.rs` (re-export it where `fir_files` is)

```rust
/// [`fir_files`], calling `made(done, total)` in files: `(0, total)` before
/// the first FIR and after each one is materialized and encoded.
pub fn fir_files_reported(
    plan: &FilterPlan,
    prefix: &str,
    rate: u32,
    made: &dyn Fn(u64, u64),
) -> Result<Vec<(String, Vec<u8>)>, GraphError>
```

`crates\secs-camilla\src\lib.rs`

```rust
pub fn prepare_config_reported(
    chain: &SignalChain,
    out: &Path,
    name: &str,
    options: &Options,
    mimo: Option<&MimoPaths>,
    made: &dyn Fn(u64, u64),
) -> Result<PreparedConfig, CamillaError>

pub fn install_prepared_reported(
    out: &Path,
    prepared: PreparedConfig,
    written: &dyn Fn(u64, u64),
) -> Result<Installed, CamillaError>
```

`made` counts the FIR files of the main configuration (the bypass
configuration has none). Thread it down to the one `fir_files` call in
`make_config_observed` however is least invasive (an internal variant is
fine); do not change `make_config`'s public signature.

`crates\secs-filters\src\lib.rs`

```rust
pub fn export_eqapo_reported(
    run: &FilterRun<'_>,
    folder: &Path,
    options: &Options,
    outputs: Option<&[(String, usize)]>,
    made: &dyn Fn(u64, u64),
    written: &dyn Fn(u64, u64),
) -> Result<Installed, FiltersError>

pub fn export_filter_text_reported(
    run: &FilterRun<'_>,
    folder: &Path,
    options: &Options,
    made: &dyn Fn(u64, u64),
    written: &dyn Fn(u64, u64),
) -> Result<Installed, FiltersError>

impl PreparedConvolution {
    pub fn install_reported(self, folder: &Path, written: &dyn Fn(u64, u64)) -> Result<Installed, FiltersError>
}
```

These do what `export_eqapo_observed_with_channels`,
`export_filter_text_observed_with_options` and `install_observed` do today.
`prepare_convolution_named_with_options` keeps its signature; only the
units of its `filters.convolution` stage change as the contract says. For
the 'rendered' half add an internal sibling of `render_paths_at_cancellable`
in `render.rs` that takes a per-pair callback; the public function keeps
its signature.

`crates\secs-mdat\src\builder.rs` (re-export beside `build`)

```rust
/// [`build`], calling `built(done, total)` in measurements.
pub fn build_reported(
    measurements: &[MeasurementInput],
    template: &[u8],
    spl_offset_db: Option<f64>,
    now_ms: i64,
    uuid: &mut dyn FnMut() -> u128,
    built: &dyn Fn(u64, u64),
) -> Result<Vec<u8>, MdatError>
```

`crates\secs-service\src\export.rs`

- Every writer calls the reported functions with closures over the job:
  `&|done, total| job.report_progress("export.prepare", done, total)` and
  the same with `"export.write"`. `JobContext::report_progress` takes a
  `&'static str` stage; both ids are literals.
- The two mdat writers serialize with `build_reported` and write their one
  file through `secs_types::fs::write_atomic` (already public) in chunks of
  1 MiB, reporting `export.write` after each chunk. Do not add a function
  to `secs-types`.
- The Equalizer APO and filter text writers write their bytes into the
  hidden prepared folder during `prepare` (that is where the bytes go to
  disk) and rename it in `write`; report `export.write` during that first
  write. Nothing is reported for the rename.
- Remove `job.report_progress("export.camilla.prepare", 0, 1)`.

## Tests

- `secs-graph`: `write_bundle_reported` reports `(0, total)`, then the
  running byte count after each file, ending at `(total, total)`, for a
  bundle of three files of different sizes; an empty-bytes bundle reports
  nothing. `fir_files_reported` likewise for a plan with FIRs and one
  without.
- `secs-mdat`: `build_reported` returns the same bytes as `build` and
  reports once per measurement.
- `secs-filters`: update the tests that read the `filters.convolution`
  stage (`tests\golden.rs` near the `RecordingProgress` use) to the new
  units and assert the rules of the contract (first, last, monotonic,
  constant total `rates * 2 * P`) for one rate and for two rates.
- `secs-service` (`crates\secs-service\tests\export.rs`, with the event
  recording the neighbouring tests already use): for each `ExportKind`
  (`camilla_dsp`, `equalizer_apo`, `convolution`, `filter_text`,
  `before_after_mdat`, `staged_mdat`) export a fixture project that has FIR
  filters where the kind can carry them, collect the job's
  `JobEvent::Progress` and assert the contract: the stage order, the rules
  for each stage, no `export.camilla.prepare`, and for the two mdat kinds
  that the `export.write` total equals the length of the written file. One
  test that an IIR-only design (no FIR) emits no `export.prepare` and still
  emits `export.write`.
- `crates\secs-service\tests\export_folder.rs` line ~248 waits for
  `filters.convolution` with `done: 1` as a point in the middle of an
  export; keep what that test proves and make it hold with the new units.
- Register the feature in `features.toml` after `export.output_rate`:
  `id = "export.progress"`, implemented, pointing at the service test that
  asserts the contract. Follow the fields of the neighbouring entries.

## Checks to run (foreground, in this order) and report

```
set CARGO_TARGET_DIR=C:\cargo-target\dirac-limit
cargo fmt --all -- --check
cargo clippy -p secs-graph -p secs-camilla -p secs-filters -p secs-mdat -p secs-export -p secs-service --all-targets --locked -- -D warnings
cargo test -p secs-graph -p secs-camilla -p secs-filters -p secs-mdat -p secs-export --locked
cargo test -p secs-service --locked
set RUSTDOCFLAGS=-D warnings
cargo doc -p secs-graph -p secs-camilla -p secs-filters -p secs-mdat -p secs-service --no-deps --document-private-items --locked
python .github\scripts\check_feature_tests.py
```

(`check_feature_tests.py` starts a cargo build: keep `CARGO_TARGET_DIR` set
when you run it, and check `git status` afterwards; it must not leave
generated files such as `bindings\dotnet\Secs2R\NativeMethods.g.cs`
changed.)

If an API you want to call is not what this packet says, read the code and
use what is there; do not guess. If a rule of the contract cannot be met
without changing what is written to disk, stop and say so in the report
instead of bending either.

## Report

- Each file changed, with one line on what changed.
- The exact stage sequence one export of each kind produced in the service
  test (stage, first report, last report, number of reports).
- The output tail of every check above, and anything you could not do.
- A list of what the screen can now rely on, if it differs from the
  contract above in any detail.

## After review (main session, 2026-11-11)

The two mdat writers do not sync after every 1 MiB chunk, as the first
implementation did after reading 'written and synced' in the contract
literally. `secs_types::fs::write_atomic` syncs once after the last chunk and
renames, as it did before; `export.write` counts the bytes handed to the
file. A sync per chunk would only make a large mdat slower to save.
