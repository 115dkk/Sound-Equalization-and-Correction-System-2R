# P23b (ASTRA): the measurement seam and mdat import in `secs-service`

Worktree: `/home/user/wt/p23b` (branch `m4/p23b`, on `m4/plan` with P20–P22).
Work only there. Do not commit; Claude reviews the diff and commits. Do not
spawn sub-agents. Build with
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p23b`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.
P23a works in parallel on the rest of `secs-service` and on the IPC; create
only `crates/secs-service/src/measure/**` and
`crates/secs-service/src/mdat_import.rs` (declare the two modules in
`lib.rs` with one line each) and no IPC types.

Read first: `docs/M4.md` (§3 the measurement seam, §8), `docs/adr/0005-*`,
`crates/secs-project` (`plan.rs`, `record.rs`: `MeasurementPlan`,
`MeasurementStore`, `MeasurementResult`), `crates/secs-audio`
(`measure_position`, `SimRoom`, `level_check`, `recommend_sweep_level`,
`MicCalibration`), `crates/secs-rew` (`RewClient`, `FakeRew`, its
`MeasurementPlan`, `PARITY.md`), `crates/secs-mdat` (`load_mdat_project`,
`extract_ir`), `secs_types::role::role_from_title`, and upstream
`src/secs/wizard/step5_measure.py` (`plan`, `measure_position`, `cancel`,
`redo_last`, `load_sample`, `load_mdat`, `clear_position`, `mdat_sources`,
`match_role`, `split_position`, `looks_like_sub`, `auto_assign`,
`MdatAssignDialog.problems`), `src/secs/mdat_loader.py`,
`src/secs/wizard/step1_devices.py`, `src/secs/wizard/step3_level.py`,
`src/secs/rewapi/fake.py` (what the tutorial's fake measurement does with
`sample_dolby_714.npz`), and the tests `tests/test_wizard_measure.py`,
`test_p2_flow.py`, `test_sample_dolby.py`, `test_measure_sweep_options.py`,
`test_qa_layout_b.py` (mdat cases), `test_loading_overlay.py` (mdat cases).

## Goal

One measurement seam with three adapters that all end in the same record:
native (`secs-audio` on an injected backend), REW (`secs-rew`), and mock
(native on a `SimRoom` built from SECS 2's sample room). And the mdat import:
scan a REW `.mdat`, propose the role and position of each measurement as
upstream does, and record the chosen ones into the project.

## Interface (crate-internal; P23c wires it to IPC)

- `measure::Measurer` trait: `devices`, `status`, `level_check`,
  `measure_position(plan, pos, progress, cancel) -> Vec<MeasurementResult>`
  (`secs_project::MeasurementResult`), implemented by `NativeMeasurer`
  (`Box<dyn secs_audio::AudioBackend>` from a factory), `RewMeasurer`
  (`RewClient<T>`), `MockMeasurer` (`NativeMeasurer` over `SimRoom`).
- `measure::plan_for(&Project, settings) -> secs_project::MeasurementPlan`
  and its translations to `secs_audio::MeasurementPlan` (role output
  channels, loopback, sweep length and level, window) and to
  `secs_rew::MeasurementPlan`. The measurement settings live where upstream
  keeps them (`project.extra['devices']`, `config.device`); read and write
  them there.
- `measure::sample`: SECS 2's sample room. Convert
  `src/secs/rewapi/sample_dolby_714.npz` once with a script
  (`oracle/export_sample_room.py`) into a compact data file under
  `crates/secs-service/data/` (float32, with a small JSON index; say its
  size) that the crate embeds, and build the `SimRoom` paths from it the way
  upstream's fake measurement uses the sample (positions, roles, and its
  limits on which roles and positions exist).
- `mdat_import`: `scan(path) -> MdatScan` (measurements with title, role and
  position guess, `looks_like_sub`, problems as upstream's dialog reports
  them) and `import(store, path, assignment, position)` writing through
  `MeasurementStore` with `source[role]` set as upstream does.

## Oracle family `service_mdat`

`oracle/export_service_mdat.py`: run upstream `split_position`,
`match_role`, `looks_like_sub`, `auto_assign`, `suggest_assignment` and
`MdatAssignDialog.problems` (extract the pure functions without Qt, as P20
did for `step4_layout.py`) over the `.mdat` files the existing families
already use and a grid of titles; and upstream `load_mdat` into a temp
project, recording the `.secs` text after import. Under 2 MiB.

## Tests

- Mock measurement end to end: plan from a project built for the sample
  room, two positions measured through `MockMeasurer`, IRs recorded through
  `MeasurementStore`, and the recovered IRs equal the sample IRs within
  P21's bound; relative arrivals within one sample.
- REW measurement through `RewMeasurer` with `secs-rew`'s `RecordedTransport`
  (or `FakeRew` with an `IrSource` over the sample room) records the same
  shape.
- Level check through the mock and the recommended level (U1).
- Every `service_mdat` value equals upstream; the imported `.secs` equals
  upstream's byte for byte.
- The upstream tests listed above that are not about Qt, as Rust tests named
  after them.

`features.toml`: set `measure.mdat_import` to `implemented`, add
`measure.mock` (M4) as `implemented`, and add your tests to
`measure.rew_api` and `measure.native` (which stays `partial`).

## Files you may change

- `crates/secs-service/src/measure/**`, `crates/secs-service/src/mdat_import.rs`,
  the two `mod` lines in `crates/secs-service/src/lib.rs`,
  `crates/secs-service/data/**`, `crates/secs-service/tests/measure*.rs`,
  `crates/secs-service/tests/mdat*.rs`, `crates/secs-service/Cargo.toml`
  (dependencies on `secs-audio`, `secs-rew`, `secs-project`, `secs-mdat`)
- `oracle/export_sample_room.py`, `oracle/export_service_mdat.py`,
  `oracle/goldens/service_mdat/**`
- `features.toml` (the entries above), `Cargo.lock` only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p23b
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p23b
cargo fmt --all -- --check
cargo clippy -p secs-service --all-targets --locked -- -D warnings
cargo test -p secs-service --locked 2>&1 | grep -E "test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-service
python3 .github/scripts/upstream.py check-tree
```

Do not run `check_feature_tests.py` (Claude runs it in the gate).

## Report

The seam and adapters as built, the sample room data (size, positions,
roles), the oracle family, each test with its numbers and the upstream test
it stands for, and anything you could not do.
