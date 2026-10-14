# P18b (ASTRA): one factorization for the λ sweep, and the per-input MIMO rows

Worktree: `/home/user/wt/p18` (branch `m3/p18`, P18's first pass is
committed). Work only there. Do not commit; Claude reviews the diff and
commits. Do not spawn sub-agents. Build with
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p16`.
The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.
Timing rules as in P18: median of five (three over a minute), `uptime`
before each pair, nothing else running.

Read first: `docs/perf/mimo.md` (your first pass), `docs/MIMO.md` §3,
`docs/adr/0007-*`, `crates/secs-engine/src/mimo/design.rs`
(`design_selected`, `normal_equations`) and `solve.rs`.

## The finding

`design_selected` solves `(M + λI) u = b` for 19 values of λ (one at
`log = 4`, then 18 bisection midpoints), with `M` and `b` fixed and
`λ = normalization · 10^log`. On the large case `M` is 2,160 × 2,160 and the
19 sequential Cholesky factorizations take 2.95 s of the 3.54 s input
design (one thread). Only `λ` changes between the solves.

## Part A: factor once (the fix)

Replace the 19 factorizations with one decomposition of `M` that serves
every λ, keeping the bisection, its midpoints, its comparisons and its
result exactly as now. First benchmark the candidates on the large case's
actual `M` (sequential `faer`, `Par::Seq`, release) and put the numbers in
the report:

1. **Spectral.** `M = Q Λ Qᵀ` once (faer's self-adjoint eigendecomposition),
   `c = Qᵀ b` once, then `u(λ) = Q (c ⊘ (Λ + λ))` per λ.
2. **Tridiagonal.** `faer::linalg::evd::tridiag::tridiag_in_place` once
   (`M = Q T Qᵀ`, `Q` as a Householder sequence; see how faer's own
   self-adjoint EVD applies it), `c = Qᵀ b` once, then per λ a tridiagonal
   LDLᵀ solve of `(T + λI) y = c` and `u = Q y`.
3. **Cholesky reuse.** Keep a fresh Cholesky for the first midpoints; once
   the new midpoint is within `log10(1.5)` of a factored λ, solve by
   conjugate gradients preconditioned with that factor, to relative residual
   `1e-14`, at most 30 iterations, else factor afresh.

Use the fastest whose whole sweep is at least twice as fast as today's;
prefer 1 or 2 (no iteration count to reason about) when they are within
20 % of 3. Whichever you use:

- `M` is only positive semi-definite. If the decomposition shows
  `Λᵢ + λ ≤ 0` (or a non-positive pivot in `T + λI`), or anything comes out
  non-finite, that λ falls back to today's `solve` (Cholesky, then LU). The
  fallback stays exercised by a test.
- No parallel reduction and no faer parallelism (ADR 0007). The result must
  not depend on the thread count (`mimo_deterministic_threads` keeps
  passing).
- Put it behind one function in `solve.rs` (for example
  `RidgeSweep::new(ata, atb) -> RidgeSweep`, `sweep.solve(lambda)`), so
  `design_selected` changes in one line and the guard steps are untouched.

Proof that it kept the result (new tests in `src/tests/mimo.rs`):

- On every recorded system the MIMO goldens hold (the golden system of
  `mimo_cholesky_matches_lu_on_golden_system`, and the systems the stage
  tests rebuild): at each λ of the recorded trajectory, the sweep's `u` has
  backward error `‖(M+λI)u − b‖ / (‖M+λI‖·‖u‖ + ‖b‖) ≤ 16·N·ε` and differs
  from the Cholesky solution by at most `16·N·κ·ε` relative (the same gate
  as the existing Cholesky/LU test).
- On every golden case, the bisection trajectory (each midpoint's accepted
  flag and the final `log_lambda`) with the sweep equals the trajectory with
  per-λ Cholesky, computed both ways in the test.
- Every existing MIMO test passes unchanged; do not touch a tolerance.

## Part B: the rest of the input design

Profile the input design after Part A on the large case, one thread and
shipped, and report the breakdown (normal equations, sweep, ranking,
path building, guard steps, other). If the normal equations are still a
fifth or more of the time, build them as a map over block pairs with
`ctx` (each block computed exactly as now, then placed), proven
bit-identical to the sequential build by a test. Do not parallelize
anything that reduces across threads.

## Part C: the rows

Re-measure, both configurations, with your P18 harness, and update
`docs/perf/mimo.md` (keep the first-pass table as "before"):

1. `mimo_design_one` (input L) and `mimo_design_all` on `big` and `room_dba`;
2. `mimo_iir_one` (input L) on `big`: Rust five runs; Python as many as fit
   in 40 minutes (at least one; say how many);
3. the whole run (`compute_mimo` against `mimo::compute`, IIR mode) on
   `room_dba`, both configurations.

Leave the big-case whole run, its memory and the exports for a later pass
(they need a quiet machine for longer than this packet runs), and write
them as "not measured" rows. Write no verdict on the 5× target; report
the numbers.

## Files you may change

- `crates/secs-engine/src/mimo/**`, `crates/secs-engine/src/tests/**`,
  `crates/secs-engine/examples/engine_perf.rs`
- `oracle/bench_engine.py`, `docs/perf/mimo.md`
- `features.toml` (the `perf.mimo` entry's tests and note only)

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p18
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p16
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine --locked 2>&1 | grep -E "test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine --release --locked 2>&1 | grep -E "test result|FAILED"
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

The candidate benchmark, the choice and why, the proofs, the profile, the
new rows next to the old ones, anything you could not do.
