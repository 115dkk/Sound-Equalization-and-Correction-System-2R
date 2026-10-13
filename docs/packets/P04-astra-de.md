# P04 (ASTRA): differential evolution and NumPy's legacy RandomState

Worktree: `/home/user/wt/p04` (branch `m1/p04`, starts after P03 lands).
Work only there. Do not commit; Claude reviews the diff and commits.

## Goal

SECS 2's bass alignment (BA) calls `scipy.optimize.differential_evolution`
with an integer `seed`, so SciPy draws from NumPy's legacy `RandomState`
(MT19937). Reproducing BA therefore needs both, consuming random numbers in
exactly SciPy's order (docs/ARCHITECTURE.md §6). Port them into `secs-dsp`.

Upstream calls (`src/secs/correction_engine.py` lines 751, 799, 1057):

```python
differential_evolution(cost, bounds, maxiter=60, popsize=12, tol=1e-6, seed=1, polish=False)
differential_evolution(cost, bounds, maxiter=BA_BUS_ITERS, popsize=12, tol=1e-6, seed=2,
                       polish=False, x0=np.clip(x1, *np.array(bounds).T))
```

Everything else is SciPy 1.18.1's default: strategy `best1bin`, mutation
`(0.5, 1)` with dithering redrawn each generation, recombination 0.7,
`init='latinhypercube'`, `atol=0`, `updating='immediate'`, `workers=1`, no
constraints, no integrality, `vectorized=False`.

## Scope

- `secs_dsp::random::RandomState`: MT19937 with NumPy's legacy seeding for an
  integer seed (`init_genrand`), `random_sample` (53-bit doubles from two
  32-bit draws), and every other draw SciPy's DE makes through it (bounded
  integers with NumPy's legacy masked-rejection rule, `permutation`/`shuffle`,
  `choice`, `uniform` — whatever the code path uses). Confirm each against
  NumPy 2.5.3's sources (`numpy/random/mtrand.pyx`, `_mt19937.pyx`,
  `src/mt19937/`, `src/distributions/`, `src/legacy/`; fetch them from GitHub
  tag `v2.5.3` with WebFetch) and against oracle goldens.
- `secs_dsp::optimize::differential_evolution`: the configuration above,
  including `x0` (replaces the first population member), the LHS
  initialization, the convergence test, `maxiter` counting, `nfev`, and the
  returned `x`, `fun`, `nit`, `nfev`, `success`, `message`. The cost is a
  caller closure `FnMut(&[f64]) -> f64`. Read SciPy 1.18.1's
  `scipy/optimize/_differentialevolution.py` (in the oracle venv:
  `<venv>/lib/python3.13/site-packages/scipy/optimize/_differentialevolution.py`)
  and port the code path, not a textbook DE.

## Files you may change

- `crates/secs-dsp/src/random.rs` (new), `crates/secs-dsp/src/optimize/**`
  (DE only; do not change the TRF port's behaviour), `crates/secs-dsp/src/lib.rs`
  (module lines), `crates/secs-dsp/tests/**` (new test files)
- `oracle/export_dsp_de.py` (new), `oracle/goldens/dsp_de/**` (generated)
- `features.toml`: add `dsp.random_state` and `dsp.differential_evolution`
  (kind `"dsp"`, milestone `"M1"`)
- `oracle/README.md`: rows for the `dsp_de` family
- `Cargo.lock`, only as cargo rewrites it (never by hand); Claude regenerates
  it when merging worker branches

## Goldens (`oracle/export_dsp_de.py`, family `dsp_de`)

Run with `<venv>/bin/python oracle/run.py dsp_de`, `<venv>` =
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

1. `rs_*`: `RandomState(seed)` for seeds 0, 1, 2, 12345, 2**32-1: the first
   5000 raw 32-bit outputs, `random_sample`, and each other draw DE uses with
   the arguments DE passes.
2. `de_*`: DE on cost functions whose shape matches BA (sums of squared
   errors of delays/gains with a non-smooth term), on Rosenbrock and on a
   function with plateaus; dimensions 2–12; seeds 1 and 2; with and without
   `x0`; one run that converges by `tol` and one that hits `maxiter`. Record,
   by wrapping the cost, every evaluated vector in order, and the result.

## Tests and budgets

The random streams are exact (bits). DE is exact on the oracle's libm: the
same evaluation sequence (every vector bit-equal) and result; elsewhere
`rtol(…)` from secs-oracle. Integer outputs exactly.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p04
<venv>/bin/python oracle/run.py dsp_de
cargo fmt --all -- --check
cargo clippy -p secs-dsp --all-targets -- -D warnings
cargo test -p secs-dsp -p secs-oracle -p secs-policy -- --nocapture 2>&1 | grep -E "MEASURE|test result|FAILED|panicked"
python3 .github/scripts/upstream.py check-tree
```

## Report

Files changed; verification output; the NumPy/SciPy source lines you
followed for each random draw; anything not matched and why.
