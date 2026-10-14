# P20 (Daybreak): measurement plan, record and IR store in `secs-project`

Worktree: `/home/user/wt/p20` (branch `m4/p20`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p20`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

Read first: `docs/M4.md`, `docs/DEVIATIONS.md` (F1, F2, F5), `CONTEXT.md`,
`crates/secs-project` (`project.rs`, `irs.rs`, `config.rs`), `oracle/README.md`,
and upstream `src/secs/rewapi/interfaces.py` (`MeasurementPlan`,
`plan_from_project`, `split_role`, `sweep_length_samples`, `SWEEP_LENGTHS`,
`MeasurementResult`), `src/secs/rewapi/session.py` (`MeasurementSession`,
`IrSaver`, `load_project_irs`, `_repair_entry`), `src/secs/wizard/timing.py`,
`src/secs/wizard/step4_layout.py` (`start_delay_seconds`, `sweep_seconds`,
`position_seconds`), `src/secs/calibration.py` (`measured_positions`,
`measurement_signature`, `target_key`, `estimate_seconds`,
`estimate_subs_seconds`, `estimate_xo_seconds`), and the tests
`tests/test_rewapi_session.py`, `tests/test_measure_sweep_options.py`,
`tests/test_timing_reference.py`, `tests/test_step4_estimate.py`,
`tests/test_load_project_irs.py`, `tests/test_single_point.py`,
`tests/test_position_pick.py`.

## Goal

Everything about *what* to measure and *what was measured*, with no I/O
device and no REW: the plan, the per-position record in `.secs`, the IR
files in `.irs`, the signatures that make results stale, and the time
estimates. P21 (native measurement) and P22 (REW) produce results; P23 (the
service) records them through this crate. Keep the API free of those crates.

## Interface (names fixed; shapes yours to propose in the report)

New modules in `crates/secs-project/src/`:

- `plan.rs`: `MeasurementPlan` (upstream fields; `timing_reference` as an
  enum `Acoustic | Loopback | None` with upstream's string forms on disk),
  `plan_from_project(&Project, positions) -> MeasurementPlan`,
  `MeasurementPlan::{name_for, output_for, validate, total_steps}`
  (`validate` returns error *codes* with parameters, not prose),
  `split_role`, `sweep_length_samples`, `SWEEP_LENGTHS`,
  `timing_settings`, `measurement_roles`, `default_ref_role`,
  `start_delay_seconds`, `sweep_seconds`, `position_seconds`.
- `record.rs`: `MeasurementResult` (role, pos, IR samples, sample rate, t0,
  SPL, SPL offset, SNR, uuid, source) and `MeasurementStore` over a project
  path: `records`, `entry_for`, `done_positions`, `pending_positions`,
  `label_for`, `start_position`, `record`, `clear_position`, `ir_path_of`,
  `ir_of`, `t0_of`, `t0_by_role`, `irs_by_role`, `save`, and
  `load_project_irs` with a cache keyed by (path, mtime, size) and a byte cap
  (upstream keeps up to 256 MB).
- `signature.rs`: `measured_positions`, `measurement_signature`,
  `target_key` (values that compare equal exactly when upstream's tuples
  do), `estimate_seconds`, `estimate_subs_seconds`, `estimate_xo_seconds`.

Deviations (write each into `docs/DEVIATIONS.md` as **적용** with the test
that guards it):

- **F1**: `save` writes a temporary file in the same folder and renames it
  over the `.secs`; the store is the only writer it knows (no lock files).
- **F2**: `record` writes each channel's IR and entry at once; a later error
  in the same position keeps the channels already measured. Re-measuring a
  position replaces that position's entry.
- **F5**: an IR path in `.secs` that resolves outside the project's `.irs`
  (absolute, `..`, symlink escape) is not followed; `ir_of` returns a
  `ProjectError` naming the entry, and the store reports it as a migration
  warning so the screen can offer to re-measure.

## Oracle family `service_plan`

`oracle/export_service_plan.py`: for the projects the existing families
already build (`oracle/engine_cases.py` cases, the `project` family's
`.secs` fixtures, one multi-amp project, one single-position project),
record upstream's `plan_from_project` (and `validate`, `name_for`,
`total_steps`), `timing_settings`, `measurement_roles`, `default_ref_role`,
`sweep_seconds`, `position_seconds`, `measured_positions`,
`measurement_signature`, `target_key`, the three estimates; and the pure
helpers over grids (`split_role`, `sweep_length_samples` for every length and
some invalid ones, `start_delay_seconds` for a list of texts). Under 2 MiB.

## Tests

- `crates/secs-project/tests/plan.rs`: every recorded value equals upstream
  (estimates to 1e-12 relative).
- `crates/secs-project/tests/record.rs`: the behaviors of
  `tests/test_rewapi_session.py`, `test_load_project_irs.py`,
  `test_single_point.py`, `test_position_pick.py` (port each upstream test
  that is not about Qt or REW; name the Rust test after it), plus F1
  (an interrupted save leaves the old `.secs` intact), F2, F5 (absolute,
  `..`, and symlink escapes rejected; in-tree paths read).
- The IR cache: a changed file (mtime or size) is re-read; the cap evicts.

`features.toml`: add `project.measurement_plan`, `project.measurement_store`
and `project.signatures` (M4) as `implemented` with your tests.

## Files you may change

- `crates/secs-project/**` (new modules, `lib.rs` exports, tests)
- `oracle/export_service_plan.py` (new), `oracle/goldens/service_plan/**`
- `docs/DEVIATIONS.md` (F1, F2, F5 rows only), `features.toml` (the three
  entries), `Cargo.lock` only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p20
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p20
cargo fmt --all -- --check
cargo clippy -p secs-project --all-targets --locked -- -D warnings
cargo test -p secs-project --locked 2>&1 | grep -E "test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-project
python3 .github/scripts/upstream.py check-tree
```

Do not run `check_feature_tests.py` (it builds the whole workspace; Claude
runs it in the gate).

## Report

What you built and where, the types as built, the oracle family (cases,
size), each test and the upstream test it stands for, the deviations, and
anything you could not do.
