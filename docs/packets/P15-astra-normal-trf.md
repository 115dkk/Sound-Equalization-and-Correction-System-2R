# P15 (ASTRA): bounded TRF from the normal equations (analytic Jacobian, `x_scale='jac'`)

Worktree: `/home/user/wt/p15` (branch `m3/p15`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p15`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`
(SciPy 1.18.1; its `scipy/optimize/_lsq/trf.py` and `common.py` are the
specification — read them there).

Read first: `docs/MIMO.md` §3 (third bullet: why and what), `docs/adr/0007-compute-and-gpu.md`,
the existing port `crates/secs-dsp/src/optimize.rs` (bounded TRF, exact SVD,
two-point differences, the trace and `trf_one_step` replay) with its tests
`crates/secs-dsp/tests/trf.rs` and `trf_replay.rs`, and the oracle observers
`oracle/trf_trace.py` and the `trf_*` part of `oracle/export_dsp.py`.

## Goal

MIMO's joint IIR fit (P16) calls SciPy
`least_squares(res, x0, jac=jac, bounds=(lb, ub), max_nfev=150, x_scale='jac')`
(method `trf`, `tr_solver='exact'`, linear loss) on a residual of about
31,000 rows and up to 455 parameters. Upstream forms the dense Jacobian and
takes its SVD every iteration. Add a variant of the bounded TRF that never
sees the Jacobian, only `JᵀJ` and `Jᵀf`, and reproduces SciPy's
`trf_bounds` step for step. P16 supplies the normal equations by block
accumulation.

## Interface (fixed)

```rust
/// Supplies the residual and the normal equations of a least-squares problem.
pub trait NormalEquations {
    /// Residual f(x) (length m). Counted in nfev like SciPy's `fun`.
    fn residual(&mut self, x: &[f64]) -> Vec<f64>;
    /// JᵀJ (n×n, both triangles filled) and Jᵀf at x, where f is the residual
    /// just returned for the same x. Counted in njev like SciPy's `jac`.
    fn normal(&mut self, x: &[f64], f: &[f64]) -> Result<(Mat<f64>, Vec<f64>), DspError>;
}
pub enum XScale { Unit, Jac }
pub struct NormalOptions { pub max_nfev: usize, pub x_scale: XScale, pub ftol: f64, pub xtol: f64, pub gtol: f64 }
pub fn least_squares_normal<P: NormalEquations>(
    problem: &mut P, x0: &[f64], lb: &[f64], ub: &[f64], options: &NormalOptions,
) -> Result<LeastSquaresResult, DspError>;
```

Defaults as SciPy (`1e-8` tolerances). Return `DspError` where SciPy would
raise `LinAlgError` (eigendecomposition failure), so that P16 can reproduce
upstream's fallback (retry with `XScale::Unit`, then keep `x0`). Keep the
existing dense API unchanged; share code with it where the algebra is the
same (`CL_scaling_vector`, `make_strictly_feasible`, `select_step`'s scalar
parts, termination), but do not change its arithmetic.

## How each SciPy quantity comes from the normal equations

`trf_bounds` uses `J` only through these; derive each and say in a comment
which SciPy line it replaces.

- `g = Jᵀf`; `x_scale='jac'`: `scale_inv = max(scale_inv, sqrt(diag(JᵀJ)))`
  with SciPy's zero handling (`compute_jac_scale`), initial and per
  iteration exactly as SciPy updates it.
- `J_h = J·diag(d)` enters as `diag(d)·JᵀJ·diag(d)`; the augmented SVD of
  `[J_h; diag(diag_h)^½]` is the symmetric eigendecomposition of
  `M = diag(d)·JᵀJ·diag(d) + diag(diag_h)`: `s = sqrt(max(λ, 0))` in
  descending order, `V` the eigenvectors, and `s·uf = Vᵀ·(diag(d)·Jᵀf)`
  (`f_augmented` is `f` padded with zeros). Use faer's self-adjoint
  eigendecomposition with `Par::Seq`. Feed `solve_lsq_trust_region` with
  `uf = (s·uf)/s` where `s > 0` and 0 where `s = 0`; check that its full-rank
  test and its use of `suf` then match SciPy's.
- Every `evaluate_quadratic`/`build_quadratic_1d` value (`‖J_h p‖²`) is
  `pᵀ·(diag(d)·JᵀJ·diag(d))·p`, plus the `diag_h` term where SciPy adds it.
- Cost and the actual reduction use the residual, as in SciPy.

## Oracle family `trf_normal`

Extend `oracle/export_dsp.py` (or add `oracle/export_trf_normal.py`; your
choice, say which) with SciPy runs of
`least_squares(..., jac=<analytic>, bounds, x_scale='jac')`, observed with
`oracle/trf_trace.py`'s technique (record every main iteration's x, f-cost,
`J` (as `JᵀJ` and `Jᵀf`), `Delta`, `alpha`, `scale_inv`, `v`, step, ratio,
acceptance). Problems:

1. Small and well conditioned (a few exponentials or PEQ biquad magnitudes,
   about 8 parameters, 200 rows), converging before the cap.
2. MIMO-shaped: a time-domain sum of 3 paths, each a gain, a delay and 3
   peaking biquads (analytic gain/delay derivatives, central differences for
   the biquads, as `mimo_iir._solve_problem` does), 2 positions, about 3,000
   rows and 33 parameters, `max_nfev=150` (it may hit the cap; record which).
3. Rank-deficient on purpose (two identical columns), to cover the full-rank
   test and the `s = 0` handling.

Also run problems 1 and 2 with `x_scale=1.0`. Keep the family under 4 MiB.
Run the exporter yourself; do not commit.

## Tests (`crates/secs-dsp/tests/trf_normal.rs`)

- **One-step replay.** From each recorded iteration's inputs, the Rust step
  (`step`, predicted reduction, new `Delta`, `alpha`, acceptance) matches the
  recorded one to relative 1e-9 (absolute 1e-12 near zero). This is the gate.
- **Whole runs.** Converged runs end within relative 1e-7 of SciPy's `x` and
  cost; capped runs are reported (`TRF_NORMAL_REPORT`), not asserted.
- **Dense equivalence.** On problem 1 with `x_scale=Unit`, the normal variant
  and the existing dense TRF (two-point Jacobian replaced by the analytic one
  through a closure) take the same accepted steps to relative 1e-9.
- The fallback contract: an eigendecomposition failure (simulate with a
  non-finite `JᵀJ`) returns `Err`, not a panic.

`features.toml`: add `dsp.trf_normal` (kind `dsp`, milestone `M3`,
`implemented`, your tests, upstream "scipy.optimize.least_squares trf_bounds
(exact) via normal equations").

## Files you may change

- `crates/secs-dsp/src/optimize.rs` and new files under
  `crates/secs-dsp/src/optimize/` if you split it, `crates/secs-dsp/src/lib.rs`
  (re-exports), `crates/secs-dsp/tests/trf_normal.rs` (new)
- the oracle exporter above and `oracle/goldens/dsp/trf_normal*` (or a new
  `oracle/goldens/trf_normal/`)
- `features.toml` (the entry above), `Cargo.lock` only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p15
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p15
cargo fmt --all -- --check
cargo clippy -p secs-dsp --all-targets --locked -- -D warnings
cargo test -p secs-dsp --locked 2>&1 | grep -E "test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-dsp --locked 2>&1 | grep -E "test result|FAILED"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-dsp
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

The interface as built, how each SciPy quantity is obtained, the oracle
problems and their sizes, the replay deviations (largest relative error per
quantity per problem), the whole-run comparison, anything reported rather
than asserted, and anything you could not do.
