# P16 (ASTRA): MIMO IIR realization

Worktree: `/home/user/wt/p16` (branch `m3/p16`, based on P14 and P15). Work
only there. Do not commit; Claude reviews the diff and commits. Do not spawn
sub-agents. Build with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p16`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

Read first: `docs/MIMO.md` (this is its P16), `docs/ENGINE.md` §3,
`docs/adr/0004-parity-and-deviations.md`, `docs/adr/0007-compute-and-gpu.md`,
the P14 module `crates/secs-engine/src/mimo/` (ideal FIR supports; reuse its
helpers), the P15 solver `crates/secs-dsp/src/optimize/normal.rs`
(`NormalEquations`, `least_squares_normal`, `XScale`), the PEQ fitter
`crates/secs-engine/src/fit.rs`, and upstream `src/secs/mimo_iir.py` (all of
it) with its call site in `src/secs/calibration.py` `compute_mimo`.

## Goal

Port `IirSupport`, `_band_of`, `_bounds`, `_unpack`, `late_modes`,
`_fit_one`, `_solve_problem`, `_worst_band_db` (the IIR one, which differs
from `mimo._worst_band_db`) and `design_mimo_iir` to
`crates/secs-engine/src/mimo/iir.rs`, with an oracle family and tests.
Upstream's process pool, low priority and `abort_workers` are replaced by
`ctx.try_map` over inputs and cooperative cancellation (D17, D36).

## Interface (fixed; private helpers are yours)

```rust
pub struct IirSupport { pub gain_db: f64, pub delay_ms: f64, pub polarity: i8,
                        pub peq: Vec<(f64, f64, f64)>, pub allpass: Vec<(f64, f64)> }
impl IirSupport { pub fn response(&self, f: &[f64]) -> Vec<C>; }
pub struct MimoIir { pub supports: Vec<(String, Vec<(String, IirSupport)>)>, // input -> paths
                     pub removed: Vec<(String, Vec<String>)>,               // greedy removals
                     pub notes: Vec<Note> }
pub fn design_mimo_iir(design: &Design, measurements: &MeasurementSet, config: &EngineConfig,
                       ideal: &MimoDesign, req: &MimoRequest, tuning: &Tuning,
                       ctx: &Context) -> Result<MimoIir, EngineError>;
```

Upstream's environment knobs become `Tuning` fields with upstream's
defaults (`crates/secs-engine/src/input.rs`): `mimo_band_db` 15,
`mimo_q_max` 10, `mimo_in_band` true, `mimo_band_reg` 0.05, `mimo_effort`
0.05, `mimo_worse_db` 1.0, `mimo_gain_max_db` 20, `mimo_k_bands` 9,
`mimo_a_bands` 3, `mimo_max_nfev` 150. Never read the environment.

## Arithmetic decisions (docs/MIMO.md §3; do not deviate)

- **Seeds (`_fit_one`)**: `fit_peq` through the engine's production fitter
  with `boost = mimo_band_db`, padded to K; then the existing dense bounded
  TRF (`secs_dsp::optimize`, two-point Jacobian, `max_nfev = 300`) on the
  320-row complex residual. An `Err` from the solver keeps `x0`, as upstream
  does on `LinAlgError`.
- **Joint fit (`_solve_problem`)**: implement `NormalEquations` for the MIMO
  objective and call `least_squares_normal` with `XScale::Jac` and
  `max_nfev`. On `Err` retry with `XScale::Unit`, then keep `x0` (upstream's
  two `LinAlgError` fallbacks). `normal()` never builds the Jacobian: for
  position p and path r the time-domain columns are
  `c_{p,r} = irfft(G[p,r]·dS_r, nf)[:n]` (per × n); the late and early rows
  contribute `c_{p,r}·diag(WL_p² + μ·WE_p²)·c_{p,r'}ᵀ` to block `(r, r')` of
  `JᵀJ`, the effort rows and the band-gain penalty rows only to diagonal
  blocks, and the level row (when `over > 0`) its outer product. `Jᵀf` the
  same way. Sum positions in position order. You may use faer matrix
  products (`Par::Seq`) for the blocks. Keep upstream's derivative formulas
  exactly (analytic gain and delay, central differences with `h = 1e-4` for
  PEQ and all-pass parameters, divided by the factor as upstream does) and
  its single-entry cache of the path responses.
- **Parallel over inputs only**, order preserved; accumulation inside an
  input sequential. Take a memory permit per input for what you allocate
  (the problem arrays plus `JᵀJ`), documented in a comment. Report progress
  through `ctx.stage("engine.mimo_iir", inputs)`. Check cancellation once per
  residual evaluation; the solver must stop at its next check, as
  `fit_peq_until` does.
- **Post-processing** exactly as upstream: the level cut, the violation
  `max(band − worse_db, t60 − 0.15·worse_db)` (D12 is ported as is), greedy
  path removal (at most `max(1, R/2)`, keep at least one, first minimum in
  path order), then up to four −3 dB steps, then off.

## Oracle family `engine_mimo_iir`

Write `oracle/export_engine_mimo_iir.py`. **Start its slow part early in the
background** (`nohup … > <scratch>/mimo_iir_oracle.log 2>&1 &`, keeping the
PID) and implement while it runs; check the log, never start a second copy.

- Cases: `room` and `room_dba` with default MIMO settings (IIR mode).
- Record per input: the problem (`X`, `WL`, `WE`, `G`, `LP`, ideal FIR,
  ranges, modes, `direct`, `level_db`), each path's seed (`x0`, polarity,
  cost, `nfev`, status), the joint solve (`x`, cost, `nfev`, `njev`,
  status), and at the joint start point the residual and the dense `JᵀJ`
  and `Jᵀf` (from upstream's own `jac`), the post-processing (level cut dB,
  violation after each step, removed paths in order, gain steps, final
  parameters) and the late-energy outcome (`_worst_band_db`, T60 change).
- Spread: rerun the seeds and the joint solve of every input under the
  labels reference, two BLAS core types and NumPy SIMD, plus two one-ULP
  perturbations of `X` (seeds 0 and 1, `oracle/export_engine_fit.py`'s
  `perturb`). Record cost, `nfev` and status of each. If the whole family
  would take more than 60 minutes, drop the second perturbation first and
  say so.
- Keep the family under 16 MiB (blobs for arrays, as the other families).

## Tests (`crates/secs-engine/src/tests/mimo_iir.rs`)

- **Functions at the start point**: the Rust residual equals upstream's to
  relative 1e-10 of its norm; `JᵀJ` and `Jᵀf` equal the oracle's dense
  products to relative 1e-10 of their largest entry. On every libm.
- **Seeds**: the P07c rule per path. Rust cost ≤ reference cost +
  1.25 · max(spread, 1e-6 · reference); capped seeds asserted only on the
  oracle libm; the number of seeds where Rust is worse than every run within
  the 99.9% binomial bound (reuse the helper in `tests/fit.rs`).
- **Joint fit**: the same rule from the recorded seeds (inject them).
- **Post-processing**: from the recorded final `x`, the level cut, the
  removal order, the steps and the final parameters equal the oracle's on
  the oracle libm (numbers to relative 1e-9); reported elsewhere.
- **Whole path**: `design_mimo_iir` on the Rust replayed design: per input
  the late-energy outcome and the number of kept paths judged by ADR 0004's
  statistical rule against the runs (reported off the oracle libm, as
  `e2e.rs` does).
- **Cancellation**: cancelling during the joint fit returns
  `EngineError::Cancelled` within one second.
- **Determinism**: one-thread and four-thread contexts give bit-identical
  output.

`features.toml`: set `engine.mimo_iir` to `implemented` with your tests.

## Files you may change

- `crates/secs-engine/src/mimo/**`, `crates/secs-engine/src/input.rs`
  (the `Tuning` fields only), `crates/secs-engine/src/notes.rs` (notes),
  `crates/secs-engine/src/lib.rs` (re-exports), `crates/secs-engine/src/tests/**`,
  `crates/secs-engine/examples/perf.rs` (keep it compiling: it re-includes
  the engine sources and mirrors the root re-exports)
- `oracle/export_engine_mimo_iir.py` (new), `oracle/goldens/engine_mimo_iir/**`
- `features.toml` (the entry above), `Cargo.lock` only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p16
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p16
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine --locked 2>&1 | grep -E "test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine --locked 2>&1 | grep -E "test result|FAILED"
cargo test -p secs-engine --release --locked 2>&1 | grep -E "test result|FAILED"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

What you ported and where, the oracle family (cases, labels, size, how long
it took), each gate's numbers (largest relative errors, seed and joint cost
margins, binomial counts), the Rust and Python time per input for the joint
fit (one input, one thread, informational), anything reported rather than
asserted and why, and anything you could not do.
