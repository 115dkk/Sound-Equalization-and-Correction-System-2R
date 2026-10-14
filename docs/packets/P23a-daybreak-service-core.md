# P23a (Daybreak): the service core — session, jobs, calibration, exports, IPC

Worktree: `/home/user/wt/p23a` (branch `m4/p23a`, on `m4/plan` with P20–P22).
Work only there. Do not commit; Claude reviews the diff and commits. Do not
spawn sub-agents. Build with
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p23a`
(yours alone). P23b works in parallel in `crates/secs-service/src/measure/`
and `crates/secs-service/src/mdat_import.rs`; do not create or edit those.

Read first: `docs/M4.md` (§3 seams, §8 the service interface: the method
table, the job model and the job groups are fixed), `docs/adr/0003-*` (IPC,
pushed job events), `docs/ARCHITECTURE.md` §7, `docs/DEVIATIONS.md` (D17,
S1), `crates/secs-types/src/ipc.rs`, `crates/secs-service/src/lib.rs`,
`crates/secs-project` (`MeasurementStore`, `signature.rs`,
`calibration_from_project`), `crates/secs-engine` public surface (`design`,
`recommend_xo`, `mimo::compute`, `Metrics`), `crates/secs-export`,
`crates/secs-camilla`, `crates/secs-compute`, and upstream
`src/secs/wizard/step_calibration.py` (`_start`, `cancel`, `stale`,
`mimo_stale`, the verdict card and scorecard it shows, `export(kind)`),
`src/secs/wizard/step8_mdat.py`, `src/secs/wizard/window.py`
(`save_project`, `_write_side_mdat`, `_carry_irs`, `_spl_offsets`),
`src/secs/calibration.py` (`save_run_cache`, `load_run_cache`,
`run_cache_path`, `export_*`), `src/secs/wizard/__init__.py`
(`new_project`, `open_project`), and the tests
`tests/test_calibration_cache_1009.py`, `test_calibration_xo_parallel.py`,
`test_busy_drop.py`, `test_step8_mdat.py`, `test_export_camilladsp_gui.py`,
`test_channel_names.py::test_close_asks_to_save_when_dirty`.

For the job registry, read Impulcifer's `crates/impulcifer-jobs/src/registry.rs`
and `crates/impulcifer-types/src/job.rs` (`/home/user/Impulcifer-pip313`,
MIT, commit `cac2217`); port what fits with the source note and a
`docs/THIRD_PARTY.md` row, and adapt it to pushed events (no polling).

## Goal

`secs-service` becomes the deep module of `docs/ARCHITECTURE.md` §7: open and
save projects, run calibration, XO recommendation and MIMO as cancellable
jobs with pushed events, keep results with staleness, cache them safely,
and assemble the three exports. The Tauri app and the CLI stay thin.

## Interface

- `secs-types::ipc`: the request and response variants for every method in
  `docs/M4.md` §8 marked P23a, the job types (`JobId`, `JobKind`,
  `JobStatus`, `JobSnapshot`, `JobEvent`, `JobResult`), and the new
  `ErrorCode`s with their params (at least `project_not_open`,
  `project_io`, `job_busy`, `job_not_found`, `cancelled`, `engine_input`,
  `export_failed`, `stale_inputs`). Also add the P23c method variants
  (`measure_methods` … `mdat_import`) with their params and results as
  types only: their handlers return `ErrorCode::NotImplemented` until P23c.
  Regenerate the TypeScript bindings (`cargo test -p secs-types`) and keep
  `apps/secs2r/ui` type-checking (`npm run typecheck` there).
- `secs-service`: `Service::new(Deps)`, `Service::call(request, events:
  Arc<dyn EventSink>) -> Envelope` (keep `call_value`), `EventSink`,
  `Deps` with the injected factories of `docs/M4.md` §8 (audio backend,
  REW transport, process and memory probes; host stays a stub until M5).
  Modules: `session` (the open project, `MeasurementStore`, dirty flag),
  `jobs` (registry, groups, cancellation via `secs_compute::CancelToken`),
  `calibration` (design, XO, MIMO jobs, results, staleness), `cache` (S1),
  `export`, `ipc` (request dispatch).
- **Run cache (S1).** A versioned, data-only file in the project's `.irs`
  folder (name and format yours; serde with a version field, no code
  execution, bounded size on read). It stores what lets `results` answer
  without recomputing: the signal chain, the summary the verdict card and
  scorecard show, the MIMO paths and summary, and the keys they were made
  from. A key mismatch or an unknown version means "recompute"; an old
  `calibration_cache.pkl` is never read.
- **Results summary.** What upstream's calibration step shows after a run:
  the verdict card (20–200 Hz within ±1 dB before and after, low-band time
  spread, added delay, position spread), the per-channel scorecard
  (`secs-engine` `Metrics`), the latency, warnings and notes. Curves for the
  graphs come later (M5); keep the result type extensible.
- **Exports.** `export { kind, folder }` writes CamillaDSP (with MIMO when a
  fresh MIMO result exists), staged mdat and before/after mdat, as upstream
  (`calibration.export_*`, step 8), plus the side `.mdat` on project save
  (`_write_side_mdat`). Results that are stale refuse with `stale_inputs`.

## Tests (`crates/secs-service/tests/`)

- A project from `oracle/goldens` (`dolby` case measurements written into a
  temp project through `MeasurementStore`) opened, calibrated, XO
  recommended, MIMO computed (`room_dba` case for MIMO), exported; the
  exported files equal what `secs-export`/`secs-camilla` produce for the
  same inputs directly.
- Jobs: events in order (`Started`, `Progress`…, `Finished`), one job per
  group (`job_busy`), cancel during calibration returns `Cancelled` within
  one second and leaves the previous results (marked stale only if inputs
  changed), results stale after a measurement change, MIMO stale after a
  recalibration.
- Cache: round trip; key mismatch recomputes; a `calibration_cache.pkl` next
  to it is ignored; a corrupted or oversized cache file is rejected without
  panic.
- IPC: every method parses, every P23c method returns `not_implemented`,
  envelopes never panic (malformed params).
- The upstream tests listed above that are not about Qt.

`features.toml`: `service.jobs` → `implemented`; add `service.session`,
`service.run_cache`, `service.exports` (M4) and `ipc.<method>` for every
method you add (P23c methods as `planned`). Write D17 and S1 into
`docs/DEVIATIONS.md` as **적용** with their tests.

## Files you may change

- `crates/secs-service/**` except `src/measure/**` and `src/mdat_import.rs`
- `crates/secs-types/src/ipc.rs` (and new modules it needs),
  `apps/secs2r/ui/src/ipc/bindings/**` (regenerated, not hand-edited)
- `apps/secs2r/src/**` and `apps/secs2r-cli/src/**` only to keep them
  compiling against the new `Service::new`/`call` (no new commands)
- `docs/THIRD_PARTY.md` (rows), `docs/DEVIATIONS.md` (D17, S1),
  `features.toml`, `Cargo.toml`, `Cargo.lock` only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p23a
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p23a
cargo fmt --all -- --check
cargo clippy -p secs-service -p secs-types -p secs2r-cli --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-service --locked 2>&1 | grep -E "test result|FAILED|panicked"
cargo test -p secs-policy --locked 2>&1 | grep -E "test result|FAILED"
(cd apps/secs2r/ui && npm ci --no-audit --no-fund && npm run typecheck)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-service -p secs-types
python3 .github/scripts/upstream.py check-tree
```

Do not run `check_feature_tests.py` (Claude runs it in the gate).

## Report

The IPC surface as built (methods, types, error codes), the job model, the
cache format, each test and the upstream test it stands for, and anything
you could not do.
