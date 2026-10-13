# P07c (ASTRA): a last-bit sensitivity envelope, and why seven steps differ by 1e-8

Same worktree and rules as P07b (`/home/user/wt/p07`, branch `m2/p07`, your
P07b work uncommitted on top). Do not commit. Do not spawn sub-agents. Do not
change `secs-dsp`.

## Findings from your P07b report

1. On some failing calls all four oracle environments return bit-identical
   fits (zero spread), yet Rust ends 0.04–0.06 % higher. OpenBLAS kernel
   choice does not change LAPACK's SVD algorithm for these 700×48 problems, so
   the environment spread misses the variation a *different* SVD
   implementation (Rust's, or macOS Accelerate's for SECS 2 itself) brings.
   The envelope must measure how far last-bit noise moves the result in
   general, not only across BLAS kernels.
2. The worst one-step errors (up to 7e-8) sit on calls whose scaled condition
   number is only 400–1100. Conditioning explains ~κ·ε ≈ 1e-13, not 1e-8, so
   something inside the step amplifies a last-bit difference: a likely
   candidate is the iteration count of the trust-region subproblem (SciPy's
   `solve_lsq_trust_region` Newton iteration on α stops at `rtol=0.01`; a
   different number of iterations gives a different α). That would be
   inherent to SciPy's algorithm (SECS 2 on another machine would do the
   same), but it must be shown, not assumed.

## Work

1. **Diagnose the seven calls whose one-step error exceeds 1e-9.** For the
   worst trial of each, compare Rust and the oracle quantity by quantity in
   the order the step computes them: the scaled Jacobian, its singular
   values and right vectors (sign-normalised), `uf`, the initial α bounds,
   each α iterate and the iteration count, the step `p`, the reflection or
   bound handling, `x_new`. Record the oracle's intermediates by
   interposing on `scipy.optimize._lsq.common.solve_lsq_trust_region` (and
   whatever else the step calls) for just those trials. Report the first
   quantity that differs beyond 1e-12 relative and how the difference grows.
   If it is anything other than an iteration-count or stopping-test flip
   driven by a last-bit difference, stop and report it as a possible defect.

2. **Sensitivity envelope.** For every recorded `fit_peq` call (and the GD
   inner fits), re-run upstream `fit_peq` in the reference environment 8 more
   times with `corr_db` perturbed by at most one ulp per element
   (`np.nextafter` towards ±inf, a seeded random sign per element, seeds
   0–7; leave elements that are exactly 0.0 alone). Record each run's final
   cost, counts and bands like the environment runs. The envelope of a call
   is the union of the reference, the three environments and the eight
   perturbed runs: `cmax` is the largest cost among them.

3. **Gate.** Keep `peq_results_within_secs2_spread` with the same rule
   (`Rust cost ≤ c0 + 1.25·(cmax − c0) + 1e-12·c0`; a lower cost always
   passes), now over the wider envelope. Keep the bit-exact assertion only
   where every one of the twelve runs agrees bit for bit and the one-step
   replay is exact. Print per call: Rust / c0, envelope `(cmax − cmin)/c0`,
   which run set `cmax`.

4. If every call passes, set `engine.peq_fit` to `implemented`. If some still
   fail, do not widen anything: report them with the numbers (Rust cost,
   the twelve costs, counts) so the gate can be decided.

Budget: keep `engine_fit` within 7 MB (bundle the extra runs into the
existing call fixtures; store only cost, counts, status and bands per run).

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p07
export CARGO_INCREMENTAL=0
<venv>/bin/python oracle/run.py engine_fit dsp && git status --short oracle/goldens/dsp
ls oracle/goldens/engine_fit | wc -l; du -sh --apparent-size oracle/goldens/engine_fit
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine fit -- --nocapture 2>&1 | grep -E "MEASURE|SPREAD|test result|FAILED|panicked" | tail -40
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine fit 2>&1 | grep -E "test result|FAILED|panicked"
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

## Report

The diagnosis of the seven calls (first differing quantity, its size, the
growth to the step); the per-call gate table (Rust / c0, envelope, who set
`cmax`, pass); golden size and file count; verification output.
