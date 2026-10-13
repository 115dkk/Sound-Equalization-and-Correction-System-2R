# P13c (Daybreak): fill the remaining performance rows and decide `perf.engine`

Worktree: `/home/user/wt/p13c` (branch `m2/p13c`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p13`
(yours alone while you run). Nothing else runs benchmarks while you work;
check `uptime` before each comparison and wait until the one-minute load is
below 1.0.

**Timebox: 110 minutes of wall time.** Start no new measurement after 95
minutes; then run the verification and report. A cell you could not fill
stays `not measured`.

This packet only measures. It changes no engine code.

Read first: `docs/packets/P13-astra-perf-audit.md` and
`docs/packets/P13b-astra-perf-fits-memory.md` (the method; P13b's rules for
fit rows, short rows and memory replace P13's where they differ),
`docs/perf/engine.md` (the tables you fill), and the harness:
`oracle/bench_engine.py`, `oracle/peak_memory.py`,
`crates/secs-engine/examples/perf.rs`, `crates/secs-export/examples/perf.rs`.
Use the harness as it is; extend it only where a row below needs it.

## Rows to fill

Both configurations (shipped and one thread) for each, unless noted:

1. `design` for `dolby_zero`, `dolby_linear`, `dolby_ultra`.
2. `recommend` for `room` (both) and `dolby` (one thread). Also re-measure
   `recommend` `dolby` shipped if P13b changed `recommend_xo`.
3. `gd` for `room` and `room_dba` (both) and `dolby` (one thread), with the
   recorded-input mechanism the harness already has
   (`SECS_PERF_GD_INPUT`).
4. Memory (`oracle/peak_memory.py`, aggregate RSS and PSS) for `recommend`
   `dolby` one thread if P13b left it empty, and `recommend` `room` both.
5. Project open and save, as a user does them in SECS 2:
   - Build the demo project once, outside any timer: in Python, a
     `secs.model.Project` with the `dolby` case's speaker configuration and
     its impulse responses written by `secs.rewapi.session.IrSaver` into
     `<scratch>/demo.irs/`, saved as `<scratch>/demo.secs` (look at how
     `oracle/engine_cases.py` builds the case's project and how `IrSaver`
     names files; read `src/secs/rewapi/session.py`).
   - **open** = what `MainWindow.load_project` does before the GUI refresh:
     `secs.wizard.open_project(path)` then
     `secs.rewapi.session.load_project_irs(project, path)` (no cache).
     Rust: read the file, `secs_project::Project::load_text`, the same
     single-amplifier conversion if `secs-project` has it, and every IR the
     records name through `secs_project` (`load_ir` or what the crate
     offers).
   - **save** = what `MainWindow.save_project` does for a project that
     already has its `.irs/` folder: `project.save(target)` and
     `_write_side_mdat(target)` (read it in `src/secs/wizard/window.py`;
     reproduce its non-GUI body: `load_project_irs`, the
     `MeasurementInput` list, `build_mdat`). Rust: `save_text` and write,
     the IRs read back, `secs_mdat::build` with the same items, the file
     written.
   - Add a release example to `crates/secs-project/examples/perf.rs` for
     the Rust side (dev-dependencies already in the workspace only; if
     `secs-project` would need a new dependency for this, stop and report).
   - If a step has no Rust counterpart yet, time the parts that exist,
     mark the row `partial`, and name the missing part.
6. Quiet repeats: every row in `docs/perf/engine.md` whose ratio is between
   0.80 and 1.00 and that P13b did not already re-measure, with P13b's
   interleaved method for rows shorter than 50 ms and five timed calls
   otherwise.

## The verdict

`perf.engine` becomes `implemented` only if, in both tables, every row is
measured and its ratio is below 1.00, the fit rows by P13b's envelope rule,
and Rust's aggregate PSS is at most Python's for `design` and `recommend`
in both configurations. Otherwise it stays `partial` and the report lists
each failing or missing row. Do not change the rule.

## Files you may change

- `oracle/bench_engine.py`, `oracle/peak_memory.py`
- `crates/secs-engine/examples/**`, `crates/secs-export/examples/**`,
  `crates/secs-project/examples/**` (new)
- `docs/perf/engine.md`, `features.toml` (the `perf.engine` entry only)
- `Cargo.lock`, only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p13c
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p13
cargo fmt --all -- --check
cargo clippy -p secs-engine -p secs-export -p secs-project --all-targets --locked -- -D warnings
cargo build --release --locked -p secs-engine -p secs-export -p secs-project --examples
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

The filled rows (old cell → new cell), the project open/save rows and how
each side was timed, the memory rows, the verdict with the list of failing
or missing rows if any.
