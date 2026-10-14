# P17a (ASTRA): MIMO run, path values and the applied curve

Worktree: `/home/user/wt/p17a` (branch `m3/p17a`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p16`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

Read first: `docs/MIMO.md` (this is half of its P17), `docs/ENGINE.md` §3,
the MIMO module `crates/secs-engine/src/mimo/` (P14 `design.rs`, P16
`iir.rs`), `crates/secs-types/src/chain.rs`, and upstream
`src/secs/calibration.py` (`MimoRun`, `compute_mimo`, `mimo_availability`,
`mimo_kind`, `mimo_fir_budget_text`, `mimo_cap_text`, `estimate_mimo_seconds`,
the MIMO settings parsing), `src/secs/mimo.py` (`FirSupport`),
`src/secs/export_camilladsp.py` (`fir_to_48k`, `mimo_paths`) and the tests
`tests/test_calibration_mimo_fir_1012.py`, `tests/test_calibration_r28_mimo_cap.py`.

## Goal

The one engine entry point of MIMO and the value the exporters read.
P17b (Daybreak) ports the CamillaDSP and mdat sides against these types in
parallel; keep the signatures below exactly.

## Interface (fixed)

The path types are already in `crates/secs-types/src/chain.rs` (`MimoPaths`,
`MimoPath`, `MimoFilter`); do not change them. FIR paths carry the 48 kHz
taps (`fir_to_48k` is applied here, once, not by the exporter).

In `crates/secs-engine/src/mimo/run.rs`:

```rust
pub struct MimoKey { pub fsiso: f64, pub f_low: Option<f64>, pub exclude: Vec<String>,
                     pub max_supports: usize, pub fir: bool }       // upstream MimoRun.key()
pub struct MimoRun {
    pub key: MimoKey,
    pub ideal: MimoDesign,                     // design_mimo's result
    pub paths: MimoPaths,                      // what the exporters read
    pub final_resp: Vec<(String, Vec<C>)>,     // per chain role: final response at the main position (MIMO off)
    pub applied: Vec<(String, Vec<C>)>,        // the same plus every support path (upstream `applied`)
    pub spl_mimo: Vec<(String, Vec<f64>)>,     // spl_db(applied) per role, upstream `curves[role]['spl_mimo']`
    pub notes: Vec<Note>,
    pub fir: bool,
}
pub fn compute(design: &Design, measurements: &MeasurementSet, config: &EngineConfig,
               settings: &CalibrationSettings, tuning: &Tuning, ctx: &Context)
    -> Result<MimoRun, EngineError>;
pub fn availability(positions: usize) -> (bool, String);   // upstream mimo_availability
pub fn kind_text(fir: bool) -> &'static str;               // mimo_kind
pub fn fir_budget_text(...) -> String;                     // mimo_fir_budget_text (read its arguments upstream)
pub fn cap_text(max_supports: usize) -> String;            // mimo_cap_text
pub fn estimate_seconds(...) -> f64;                       // estimate_mimo_seconds
pub fn fir_to_48k(taps_low: &[f64], fs_low: f64) -> Vec<f64>; // export_camilladsp.fir_to_48k
```

Roles in `final_resp`, `applied` and `spl_mimo` follow the chain order
(upstream iterates `engine.chain`); a role with no supports has
`applied == final_resp`. Upstream's `FirSupport.response` (DTFT below
`fs_low/2` with the 10 % cosine edge, zero above) is a private helper; the IIR
path's response is P16's `IirSupport::response`.

`compute` reads the settings the way upstream does (`mimo_on`, `fsiso`,
`mimo_low`, `mimo_max_supports` with 0 meaning no cap, `mimo_fir`,
`excluded_supports`), runs `design_mimo` and then either `design_mimo_iir`
or the FIR wrap, builds `MimoPaths` exactly as upstream's `mimo_paths`
(order, fallback 50 Hz, delay), and the applied curve as upstream's
`compute_mimo` (final response at the main position plus every path's
`support_path · LR4 LP(fsiso) · response`, `lp = 1` in FIR mode, then
`spl_db`). Progress: `ctx.stage("engine.mimo", …)` around the
two phases. Cancellation as in P14/P16.

Split the tail into a crate-private step the tests can drive with recorded
inputs: `assemble(design, measurements, config, &MimoDesign, supports, key,
ctx) -> Result<MimoRun, EngineError>`, where `supports` is the per-input IIR
or FIR supports. `compute` is `design_mimo`, then the fit or the FIR wrap,
then `assemble`. The IIR fit's agreement with upstream is P16's gate; this
packet asserts what follows it given the same supports.

## Oracle family `engine_mimo_run`

`oracle/export_engine_mimo_run.py`: on `room_dba` and `room_filters` (BA
matches on the oracle libm; `room` does not, ENGINE.md §3), IIR and FIR
modes, run upstream `compute_mimo` from the recorded design (replay
`engine_design`'s fits, as P16's exporter does) on the reference settings
only: the IIR fit runs once per case, no perturbed runs. Record the ideal
design, the fitted `IirSupport` values (so the Rust test can inject them),
`mimo_paths` with `fir_to_48k` of each FIR support, `final` and `applied`
per role, `spl_mimo`, the notes, `key()`, and the text helpers for a grid of
arguments (availability for 0–4 positions, cap text for 0, 1, 4, 12, budget
text for a few tap budgets, estimate for a few sizes). Under 8 MiB.

## Tests (`crates/secs-engine/src/tests/mimo_run.rs`)

- `assemble` with the recorded ideal design and supports injected:
  `MimoPaths` equal the recorded paths (names, order, low edge, delay to
  1e-12; IIR parameters exactly, they are the injected values; FIR taps to
  relative 1e-12).
- `compute` end to end: FIR mode equals the recorded run (paths, applied,
  spl) within P14's ideal-design tolerances; IIR mode yields the same
  inputs, supports and order as upstream and finite curves (the fit values
  themselves are P16's gate).
- `fir_to_48k` equals upstream to relative 1e-12; `FirSupport.response`
  agrees with the rfft of `fir_to_48k` within 1 % below 250 Hz and is zero
  above 300 Hz; `taps_48k == 8000` for a solved support (70 taps through the 31-tap Fsiso
  low-pass, 100 low-rate taps; upstream
  `test_calibration_mimo_fir_1012.py`).
- With injected supports, `final_resp` and `applied` equal upstream to
  relative 1e-9 and `spl_mimo` to 1e-9 dB.
- Text helpers equal upstream exactly; the cap setting clamps to 0..12 and 0
  means no cap (`test_calibration_r28_mimo_cap.py`); FIR mode skips the IIR
  fit (no IIR call, progress jumps to done).
- Off the oracle libm, values downstream of BA are reported (ENGINE.md §3).

`features.toml`: add `engine.mimo_run` (M3, `implemented`, your tests) and
set `engine.mimo_fir` to `implemented` with the FIR tests.

## Files you may change

- `crates/secs-engine/src/mimo/**`, `crates/secs-engine/src/lib.rs`,
  `crates/secs-engine/src/notes.rs`, `crates/secs-engine/src/tests/**`,
  `crates/secs-engine/examples/engine_perf.rs` (keep it compiling)
- `oracle/export_engine_mimo_run.py` (new), `oracle/goldens/engine_mimo_run/**`
- `features.toml` (the two entries), `Cargo.lock` only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p17a
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p16
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine --locked 2>&1 | grep -E "test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine --locked 2>&1 | grep -E "test result|FAILED"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

What you ported and where, the oracle family (cases,
size), each test's numbers, anything reported rather than asserted and why,
anything you could not do.
