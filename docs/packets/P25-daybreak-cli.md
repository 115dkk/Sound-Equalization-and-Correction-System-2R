# P25 (Daybreak): the CLI, the sample project and demo data

Worktree: `/home/user/wt/p25` (branch `m4/p25`, which has P23a, P23b, P23c and
P24 merged). Work only there. Do not commit; Claude reviews the diff and
commits. Do not spawn sub-agents. Build with
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p25`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

Read first: `docs/M4.md` (§2 the CLI row, §7 exit conditions, §8),
`docs/adr/0003-*` (the CLI prints events on standard output),
`docs/adr/0007-*`, `apps/secs2r-cli/src/main.rs`,
`crates/secs-service/src/lib.rs` (`Service::new(Deps)`, `Service::call`,
`EventSink`, `Deps` with its UUID source), `crates/secs-service/tests/` (how
the tests drive the service and how `service.rs` writes an oracle case into a
project), `crates/secs-service/src/measure/sample.rs`, and upstream
`src/secs/__main__.py` (`--check`, `--version`), `src/secs/wizard/__init__.py`
(`new_project`, `apply_auto_defaults`), `src/secs/wizard/step5_measure.py`
(`load_sample`), `src/secs/rewapi/fake.py` (`FakeREW` with
`use_sample_dolby`, `_fallback_ir`, `_position_variation`), and the tests
`tests/test_entrypoint.py`, `tests/test_sample_dolby.py`,
`tests/test_wizard_measure.py` (sample cases).

## 1. Service: the sample project and demo data (upstream features)

- `project_new` ports upstream `new_project(fake)`: add `sample: bool` to its
  params. Not sample: `Project.from_preset(preset)` with the sub count as
  now, plus what `new_project` adds (DSP profile `camilla`,
  `apply_auto_defaults`). Sample: 7.1.4 + three subs placed after the first
  three mains and renumbered, preset name `7.1.4`, `extra['array']` as
  upstream's `SAMPLE_ARRAY`. Proof: a new oracle family `service_new`
  (`oracle/export_service_new.py`, under 200 KB) records the `.secs` text
  upstream writes for `new_project(False)`, `new_project(True)` and a few
  presets with sub counts, saved to the same file name; the service writes
  the same bytes.
- `sample_load { positions: Option<u32> }` (device-group job): upstream
  `load_sample`. `None` keeps only position-1 entries and fills position 1
  for every planned role straight from the sample room (no sweep: the IR,
  rate and t0 as stored), roles the sample lacks from the synthetic IR, and
  records through the session's `MeasurementStore`. `Some(n)` with n ≥ 2 is
  upstream's test and development path: synthetic IRs for every role and
  position with `_position_variation`. Port `_fallback_ir` and
  `_position_variation`; upstream seeds the synthetic IR with Python's
  `hash(role)`, which changes per process, so seed it with a fixed hash of
  the role name (FNV-1a 64 of the UTF-8 bytes, modulo 100000, plus
  20260916 as upstream) and a small PCG-style generator of your choice;
  write this as deviation **R5** in `docs/DEVIATIONS.md` (REW section). The
  synthetic IR cannot equal upstream's bytes; test its structure instead
  (direct sound at 3 ms, length, subs low-passed) and that two calls give
  the same bytes. Result: the project info. Register `ipc.sample_load`.
- Keep the request and response types documented line by line in
  `secs-types` and regenerate the bindings (`cargo test -p secs-types`).

## 2. The CLI (`apps/secs2r-cli`)

Every command is one or more service requests through `Service::call`; the
CLI never reaches past the service. Output on standard output is JSON Lines:
each job event as `{"event": JobEvent}` while a job runs, then one
`{"reply": Envelope}` per request (the job's `Finished` result for jobs).
`--quiet` drops the event lines. Exit code 0 only when every reply is ok and
every job finished `succeeded`. Standard error carries nothing a script
must parse.

Global options: `--quiet`, `--deterministic-uuids` (the `Deps` UUID source
becomes a counter starting at 1, so mdat bytes repeat; for tests and CI
only). `Deps` otherwise come from `secs-sys` (`default_backend`,
`OsProcessProbe`, `OsMemoryProbe`) and the HTTP REW transport.

Commands:

| Command | Requests |
|---|---|
| `bootstrap` | `bootstrap` |
| `check` | Port of upstream `--check`: one JSON object with the version, the upstream base, the platform, the available parallelism, Rayon's thread count, and `parallel: "ok"` when a small parallel map over `secs-compute` equals the same map run sequentially (`"wrong"` or `"error: …"` otherwise), `status: "ok"`. |
| `project new <path> [--preset P] [--subs N] [--sample]` | `project_new` |
| `project info <path>` | `project_open`, `project_info` |
| `sample <project> [--positions N]` | open, `sample_load`, `project_save` |
| `measure <project> --position N [--method mock\|native\|rew]` | open, `measure_configure` (only when `--method` is given), `measure_position`, save |
| `import-mdat <project> <file> [--position N]` | open, `mdat_scan`, `mdat_import` with the suggested assignment, save |
| `calibrate <project>` | open, `calibrate` |
| `recommend-xo <project>` | open, `recommend_xo` |
| `mimo <project>` | open, `mimo` |
| `export <project> --kind camilla-dsp\|staged-mdat\|before-after-mdat --out DIR` | open, `export` (uses the run cache; recomputes as the service does) |
| `run --out DIR [--project PATH] [--fill sample\|mock] [--mimo]` | Without `--project`: `project_new { sample }` at `DIR/project/sample.secs`, then fill position 1 (`sample_load` or, with `--fill mock`, `measure_configure { mock }` and `measure_position 1`). With `--project`: open it as it is. Then `calibrate`, `mimo` when `--mimo`, and the three exports into `DIR/export/camilla-dsp`, `DIR/export/staged-mdat`, `DIR/export/before-after-mdat`. |

`calibrate`, `recommend-xo` and `mimo` on their own exist so a script can run
a step; they save the run cache as the service does.

## 3. A project from an oracle case (dev only)

`crates/secs-service/examples/case_project.rs`: `case_project <case>
<path.secs>` writes the measurements of an `oracle/goldens/engine_inputs`
case into a new project through `MeasurementStore`, the way
`crates/secs-service/tests/service.rs` does (share that code; do not copy
it). CI uses it with `room_dba` (four positions) so the CLI can run MIMO on
real multi-position data. It is an example, so it never ships.

## Tests

- `apps/secs2r-cli/tests/cli.rs` runs the built binary
  (`env!("CARGO_BIN_EXE_secs2r-cli")`): `bootstrap` and `check` print one
  reply each and exit 0 (`test_check_imports_every_source_module` stands for
  upstream's); `run --out tmp --deterministic-uuids` writes the three
  exports and a second run into another folder gives identical bytes;
  `project new --sample` then `sample` then `calibrate` exit 0; a bad
  project path exits 1 with an error reply; `--quiet` prints only replies.
- Service tests (`crates/secs-service/tests/`): `service_new` bytes;
  `sample_load` fills position 1 with the sample's exact IRs for all 14
  roles (`test_sample_project_measures_every_sub_from_the_real_data`),
  drops other positions, and `Some(3)` fills three positions of synthetic
  IRs deterministically (`test_roles_outside_714_or_biamp_bands_stay_synthetic`
  for the role rule).
- Keep debug-build test time in check: the CLI tests may take at most about
  a minute together in debug on this machine; say how long they took.

`features.toml`: `cli.commands` (M4) `implemented` with the CLI tests;
`measure.sample` (M4) `implemented` with the sample tests; `ipc.sample_load`.

## Files you may change

- `apps/secs2r-cli/**`
- `crates/secs-service/src/**` (the new-project port, `sample_load`, the
  synthetic IR), `crates/secs-service/tests/**`,
  `crates/secs-service/examples/case_project.rs`
- `crates/secs-types/src/ipc.rs`, `apps/secs2r/ui/src/ipc/bindings/**`
  (regenerated)
- `oracle/export_service_new.py`, `oracle/goldens/service_new/**`
- `features.toml`, `docs/DEVIATIONS.md` (R5), `Cargo.toml`/`Cargo.lock` as
  cargo rewrites them

Not `.github/` (Claude writes the CI job), not the screen.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p25
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p25
cargo fmt --all -- --check
cargo clippy -p secs2r-cli -p secs-service -p secs-types --all-targets --locked -- -D warnings
cargo clippy -p secs2r-cli -p secs-service --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs2r-cli -p secs-service -p secs-types --locked 2>&1 | grep -E "test result|FAILED|panicked"
cargo test -p secs-policy --locked 2>&1 | grep -E "test result|FAILED"
cargo build --release -p secs2r-cli -p secs-service --example case_project --locked
target=/home/user/wt/target-p25/release
$target/examples/case_project room_dba /tmp/p25-case/project.secs
for t in 1 default 7; do
  if [ $t = default ]; then unset RAYON_NUM_THREADS; else export RAYON_NUM_THREADS=$t; fi
  $target/secs2r-cli --quiet --deterministic-uuids run --out /tmp/p25-run-$t --fill mock
  cp -r /tmp/p25-case /tmp/p25-case-$t
  $target/secs2r-cli --quiet --deterministic-uuids run --project /tmp/p25-case-$t/project.secs --out /tmp/p25-case-$t --mimo
done
unset RAYON_NUM_THREADS
(cd apps/secs2r/ui && npm ci --no-audit --no-fund && npm run typecheck && npm test)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs2r-cli -p secs-service -p secs-types
python3 .github/scripts/upstream.py check-tree
```

Report the wall time of each `run`, and whether the `export` folders of the
three thread counts are byte-identical (`sha256sum` them; Claude's CI job
will compare them the same way).

## Report

The service methods and CLI commands as built, the output format with one
example of each line kind, the oracle family, each test with its numbers and
the upstream test it stands for, the run times, the thread-count comparison,
and anything you could not do.
