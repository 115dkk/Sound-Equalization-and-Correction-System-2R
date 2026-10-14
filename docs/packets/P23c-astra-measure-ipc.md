# P23c (ASTRA): the measurement methods on the service

Worktree: `/home/user/wt/p23c` (branch `m4/p23c`, which already has P23a, P23b
and P24 merged). Work only there. Do not commit; Claude reviews the diff and
commits. Do not spawn sub-agents. Build with
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p23c`
(yours alone).

Read first: `docs/M4.md` (§3 the measurement seam, §8 the method table, the
job model and its groups), `docs/adr/0003-*` (pushed job events, codes not
sentences), `docs/adr/0005-*`, `docs/DEVIATIONS.md` (U1, U5, D17),
`crates/secs-types/src/ipc.rs` (the P23c request and response types are stubs
P23a left for you), `crates/secs-service/src/{lib.rs,ipc.rs,jobs.rs,session.rs}`
(how P23a runs jobs, holds the open project and sends events), and P23b's
seam: `crates/secs-service/src/measure/**` (`Measurer`, `NativeMeasurer`,
`RewMeasurer`, `MockMeasurer`, `MeasurementSettings`, `plan_for`,
`store_devices`, `apply_level`) and `crates/secs-service/src/mdat_import.rs`
(`scan`, `import`, `problems`). Upstream: `src/secs/wizard/step1_devices.py`,
`step3_level.py`, `step5_measure.py` (`measure_position`, `cancel`,
`redo_last`, `clear_position`, `load_mdat`, `MdatAssignDialog`),
`src/secs/proc.py` and `src/secs/rewapi/rew.py` (`launch`, status).

## Goal

The ten measurement methods of `docs/M4.md` §8 work through `Service::call`,
long ones as jobs in the device group with pushed events, all ending in the
same `.secs` record P23b writes. After this packet the screen (M5) and the
CLI (P25) can measure, import and clear without touching the crates below.

## Methods

Replace the `not_implemented` handlers and finish the stub types in
`secs_types::ipc` (you own these types; keep every field documented with one
`///` line, as the rest of the file is, and regenerate the bindings with
`cargo test -p secs-types`).

| Method | Behaviour |
|---|---|
| `measure_methods` | Which methods can run now: native (the devices the injected backend enumerates, with channel counts), REW (`secs-rew` status through the injected transport and process probe: running, reachable, Pro), mock (always, with the sample room's positions and roles). No job. |
| `measure_configure` | Sets the method (`native`, `rew`, `mock`), devices, channels, sweep length and level, timing reference, loopback channel, share mode, and stores them where P23b reads them (`project.extra['devices']`, `config.device`) through `store_devices`; marks the project dirty. Returns the effective settings (the stub `MeasureConfigResult` grows to carry them). |
| `level_check` | Job (device group): `Measurer::level_check` for one role; `Progress` while it runs; result `LevelResult` (RMS, peak, SPL when calibrated, recommended sweep level). |
| `level_apply` | U1: writes the recommended level into the settings through `apply_level`; returns the settings. |
| `measure_position` | Job (device group): plan from the project and settings, `Measurer::measure_position`, then record each role through the session's `MeasurementStore` as it arrives (F2: channel by channel), `Progress` per role (`stage` = role code, `done`/`total` = roles), `Log` for warnings (code + params, e.g. low SNR). Cancel: the job ends `cancelled`; roles already recorded stay (REW) and a native position that did not finish records nothing, as P23b's seam does. Recording a measurement marks calibration and MIMO results stale (the signatures already do this; check it). Result: the project info. |
| `position_clear` | U5: removes the position's entry as upstream `clear_position` does (`MeasurementStore::clear_position`, IR files stay on disk), saves, and returns an undo token with the project info. |
| `undo` | Restores the entry a token removed at its original index and saves; a token is single use; keep the last 16 tokens per open project and drop them when another project opens. Unknown or used token: `invalid_request`. |
| `rew_launch` | Starts REW through the host seam when it is not running (the host is a stub until M5; the stub reports `not_implemented` in its own error, which the method returns), then reports the REW status. |
| `mdat_scan` | `mdat_import::scan`: each measurement with its title, guessed role and position, `looks_like_sub`, and the problems. Problems go out as codes with params, not the upstream sentence: add a structured twin of `problems` (`mdat_duplicate_roles { roles }`, `mdat_nothing_selected`) and keep the sentence function for its oracle test. |
| `mdat_import` | Job (device group): `mdat_import::import` with the chosen assignment and position, through the session's store; result the project info. |

Errors use the existing codes plus, where none fits, new ones you add with
their params and a test each: at least `device_unavailable` (the backend or
endpoint is gone), `share_mode_refused` (a pinned WASAPI mode was refused,
ADR 0005), `rew_unreachable`, `measurement_input` (an invalid plan or
setting, with the field). Map `MeasureError` and `secs_audio::AudioError`
variants to them in one function with a test per variant.

## Tests (`crates/secs-service/tests/measure_ipc.rs`)

All through `Service::call` with an event collector:

- Mock flow: new project for the sample room, `measure_configure { method:
  mock }`, `level_check` and `level_apply`, `measure_position { 1 }`; events in
  order (`Started`, one `Progress` per role, `Finished`); the `.secs` and the
  `.irs` files written; calibration then runs on the result (P23a's job) and
  reports no stale flag.
- Device group exclusion: a second `measure_position` while one runs gets
  `job_busy`; a `calibrate` at the same time is allowed (other group).
- Cancel during `measure_position` (mock, slowed by a sim room delay) ends
  `cancelled` within one second and leaves the project as before the job.
- REW through `RecordedTransport`: `measure_methods` shows REW reachable and
  Pro; `measure_position` records the recorded IRs.
- `position_clear` then `undo` restores the `.secs` bytes exactly; a second
  `undo` with the same token is `invalid_request`.
- `mdat_scan` and `mdat_import` on a `.mdat` from the `service_mdat` oracle
  give the same `.secs` bytes as P23b's direct test.
- Stale after measurement: calibrate, then measure position 2, then `results`
  shows calibration stale.
- The error mapping test.

`features.toml`: every P23c `ipc.<method>` → `implemented` with its test;
`measure.native` stays `partial`. Write U1 and U5 in `docs/DEVIATIONS.md` as
**적용** with their tests.

## Files you may change

- `crates/secs-service/src/**` (new files for the handlers; small changes in
  `measure/**` and `mdat_import.rs` for the structured problems and error
  mapping), `crates/secs-service/tests/measure_ipc.rs`
- `crates/secs-types/src/ipc.rs` (the P23c types and new error codes only),
  `apps/secs2r/ui/src/ipc/bindings/**` (regenerated, never by hand)
- `features.toml`, `docs/DEVIATIONS.md` (U1, U5), `Cargo.lock` only as cargo
  rewrites it

Not the screen (`apps/secs2r/ui/src/**` other than the generated bindings),
not the CLI, not other crates.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p23c
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p23c
cargo fmt --all -- --check
cargo clippy -p secs-service -p secs-types --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-service --locked 2>&1 | grep -E "test result|FAILED|panicked"
cargo test -p secs-policy --locked 2>&1 | grep -E "test result|FAILED"
(cd apps/secs2r/ui && npm ci --no-audit --no-fund && npm run typecheck && npm test)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-service -p secs-types
python3 .github/scripts/upstream.py check-tree
```

Do not run `check_feature_tests.py` (Claude runs it in the gate).

## Report

The methods as built (params, results, errors), the event sequences, each
test with its numbers and the upstream behaviour it stands for, and anything
you could not do.
