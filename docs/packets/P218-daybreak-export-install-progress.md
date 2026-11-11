# P218 (Daybreak Blue): the rest of an export's silence

Worktree `E:\wt\export-progress`, branch `feat/export-progress` (P215's work
is committed there, and the calibration stages of P217 are merged in). Build
with `CARGO_TARGET_DIR=C:\cargo-target\calib-progress` for every cargo
command; the worktree must not get a `target` folder. Do not commit, do not
push, do not run `git add`, `git stash`, `git checkout` or `git reset`. Do
not touch `apps\secs2r\ui`, `CHANGELOG.md`, `docs\`, or the upstream Python
tree. `bindings\dotnet\Secs2R\NativeMethods.g.cs` shows as modified after a
build (line endings only): leave it alone. Run every command in the
foreground and end your turn only after the report.

## What a trace of the real thing showed

The release CLI on a 7.1.4 project with 2 subs, 9 positions and MIMO:

```
convolution, three rates (878 files, 123 MB)
   filters.convolution  0..936/936      38.9 s
   export.write         0..123244064    2.6 s
   quiet 6.4 s  after export.write 123244064/123244064 until finished
camilla_dsp (147 files)
   quiet 1.2 s  after export.write 4546713/4546713 until finished
before_after_mdat (design recomputed first)
   design 0/1, read_measurements 0..117/117, read_measurements 1/1,
   engine.ba 0/0, engine.top, engine.design, design 1/1,
   read_measurements 1..117/117,
   quiet 2.4 s  until export.prepare 0/139
staged_mdat
   quiet 3.6 s  after design 1/1 until export.prepare 0/66
```

Three things are left:

1. After the bytes are staged, `write_bundle_observed`
   (`crates\secs-graph\src\files.rs`) moves every file into its place with
   one rename each. On Windows that is several milliseconds per file, so a
   large bundle spends longer there than writing, with the bar full.
2. The recomputed-design path of `crates\secs-service\src\export.rs` still
   reports placeholders: `("design", 0, 1)`, `("read_measurements", 1, 1)`,
   `("design", 1, 1)`. The engine's own stages now say what happens there.
3. The two mdat exports compute their responses (`secs_export::rew_mdat_items`,
   `stage_items`, `stage_items_mimo`) before `export.prepare` starts, and
   say nothing meanwhile.

## What to change

### 1. A stage for moving the files into place: `export.install`, in files

`crates\secs-graph\src\files.rs`: `write_bundle_reported` gets one more
callback (the function is not released yet, so change its signature):

```rust
pub fn write_bundle_reported(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
    entry: &Path,
    written: &dyn Fn(u64, u64),
    installed: &dyn Fn(u64, u64),
) -> Result<BundleInstall, std::io::Error>
```

`installed(done, total)` counts files: `(0, total)` when the first file is
about to be moved (the existing `before_install` hook of
`write_bundle_observed` is called with the index of the file about to be
installed, so index `i` means `i` files are in place), then after each
file, and `(total, total)` when the last one is in place. `total` is
`files.len()`; an empty bundle reports nothing. `written` keeps its meaning
and still ends at `(total, total)` before `installed` starts.

Thread the callback through every reported function that ends in a bundle
write, as the last parameter:

- `crates\secs-filters\src\lib.rs`: `export_eqapo_reported`,
  `export_filter_text_reported`, `PreparedConvolution::install_reported`
  and the internal helpers between them and `write_bundle_reported`;
- `crates\secs-camilla\src\lib.rs`: `install_prepared_reported`.

`crates\secs-service\src\export.rs`: every bundle writer passes
`&|done, total| job.report_progress("export.install", done, total)`. The
two mdat writers write one file with a rename inside `write_atomic` and
report no `export.install`.

### 2. No placeholders in the recomputed-design path

Remove the three `job.report_progress` calls named above from
`crates\secs-service\src\export.rs`. `load_measurements` already reports
`read_measurements` item by item and the engine reports its own stages.
Tests that use a `design` progress event as a point in the middle of the
job (`crates\secs-service\tests\cache.rs` around lines 328, 396 and 739,
and any other you find with `grep -rn '"design"' crates\secs-service\tests`)
must keep proving what they prove: hook them on a stage that still exists
at the same point of the job (`read_measurements`, or `engine.design`),
and say in the report which test moved to which stage.

### 3. The mdat responses are announced

Right before the service calls `rew_mdat_items`, `stage_items` or
`stage_items_mimo` for an export that has a design (not the
measurements-only export, which computes nothing), report
`job.report_progress("export.responses", 0, 0)` once. A stage with total 0
is an announcement: the amount of work is not counted, the screen names the
stage and draws no bar (the bass alignment `engine.ba` is announced the
same way). Do not count inside `secs-export`.

## The contract after this packet

A job started by `Service::start_export` emits, after the stages of a
recomputed design (`read_measurements`, the engine's stages; no `design`):

1. for an mdat with a design: `export.responses` once, as `(0, 0)`;
2. the contents: `export.prepare` or `filters.convolution` (unchanged);
3. `export.write` in bytes (unchanged);
4. for the four bundle kinds: `export.install` in files, `(0, N)` ..
   `(N, N)`.

Except for the announced stages (`export.responses`, `engine.ba`,
`engine.ba_probe`), every stage starts at `(0, total)`, ends at
`(total, total)`, never goes back and has a constant total above 0.

## Tests

- `crates\secs-graph\tests\files.rs`: the three-file bundle also checks
  `installed` as `(0,3) (1,3) (2,3) (3,3)`, reported after the last
  `written`.
- `crates\secs-service\tests\export.rs`
  (`export_jobs_report_real_prepare_and_write_progress` and the IIR-only
  test): the order above for each kind; `export.install` only for
  `camilla_dsp`, `equalizer_apo`, `convolution`, `filter_text` with total
  equal to the number of files the bundle installs; `export.responses`
  once with total 0 for the two mdat kinds with a design and absent for a
  measurements-only mdat; no `design` stage anywhere in the job.
- Every other test that calls a function whose signature changed.

## Checks to run (foreground, in this order) and report

```
set CARGO_TARGET_DIR=C:\cargo-target\calib-progress
cargo fmt --all -- --check
cargo clippy --workspace --exclude secs2r --exclude secs2r-host-shell --all-targets --locked -- -D warnings
cargo test -p secs-graph -p secs-camilla -p secs-filters -p secs-mdat -p secs-export --locked
cargo test -p secs-service --locked
set RUSTDOCFLAGS=-D warnings
cargo doc -p secs-graph -p secs-camilla -p secs-filters -p secs-service --no-deps --document-private-items --locked
cargo build --release -p secs2r-cli --locked
```

If an API is not what this packet says, read the code and use what is
there; do not guess. The exporters' bytes, file names, staging, commit
point and cancellation must stay exactly as they are; the golden tests
pass unchanged.

## Report

- Each file changed, with one line on what changed.
- The stage sequence one export of each kind produced in the service test
  (stage, first report, last report, number of reports).
- Which tests moved from the `design` stage to which stage.
- The output tail of every check, and anything you could not do.
