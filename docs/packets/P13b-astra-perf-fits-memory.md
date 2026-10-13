# P13b (ASTRA): fit timing over the input envelope, recommendation memory, CamillaDSP

Worktree: `/home/user/wt/p13b` (branch `m2/p13b`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p13`
(yours alone while you run). The machine has four cores. Nothing else runs
benchmarks while you work, but check `uptime` before each comparison and wait
until the one-minute load is below 1.0.

**Timebox: you have 110 minutes of wall time.** Start no new measurement after
95 minutes; then run the verification and report. A cell you could not fill
stays `not measured`.

Read first: `docs/packets/P13-astra-perf-audit.md` (the method, still in
force unless this packet changes it), `docs/perf/engine.md` (your previous
results; you will update it), `docs/ENGINE.md` §5, `docs/adr/0007-*`,
`docs/adr/0004-parity-and-deviations.md`, and the harness you wrote:
`oracle/bench_engine.py`, `crates/secs-engine/examples/perf.rs`,
`crates/secs-export/examples/perf.rs`.

Everything in P13's "Not allowed" list still holds: no change to the order of
a floating-point accumulation, no different algorithm, no `unsafe`, no
`faer` parallel mode, no relaxed test, no new dependency.

## A. Fit rows are compared over the input envelope

A trust-region fit's evaluation count is chaotic in its input. SECS 2's own
recorded runs of the room bus fit take 43 to 147 evaluations (nfev) across
three BLAS kernels and eight one-ULP perturbations of `corr_db`
(`oracle/goldens/engine_fit/fit_room_bus_00.json`, `outputs.calls[0]`:
`nfev`, `environments[*].nfev`, `perturbations[*].nfev`). One input's wall
time therefore measures one trajectory, not the implementation. From now on
every `fit_*` row (rc1, rc2, bus, xo; dolby, room, room_dba; both
configurations) is measured like this:

1. Inputs: the recorded input the row already uses, plus its eight one-ULP
   perturbations made exactly as `oracle/export_engine_fit.py` makes them:
   for seed `k` in `0..8`, `perturb(corr_db, np.random.default_rng(k))`
   with that file's `perturb` (copy the function into the bench; do not
   import the exporter, it runs on import). Only `corr_db` is perturbed.
2. `oracle/bench_engine.py` writes the nine `corr_db` arrays for a row to one
   little-endian f64 file (`SECS_PERF_FIT_INPUTS=<path>`, nine arrays of the
   recorded length, in seed order after the recorded one). The Rust example
   reads the same file, so both sides fit byte-identical inputs.
3. For each of the nine inputs, each side takes the median of five timed fits
   after one warm-up and records `nfev` and `njev`.
4. The row's value on each side is the median of the nine per-input medians.
   The ratio is Rust/Python of those two medians.
5. The report lists, per row, both sides' nine `nfev` values and SECS 2's
   recorded envelope (reference, three environments, eight perturbations)
   from the fixture. If Rust's median `nfev` over the nine inputs is above
   the largest `nfev` in SECS 2's recorded envelope, say so and show the
   first iteration where Rust's trace leaves the recorded one (the fixture
   has the reference trace); do not change the solver. That would be a
   porting question for Claude, not a performance fix.

Also give Rust's and Python's time per residual evaluation (total residual
calls from your existing `profile_fit` counters) for the room bus row.

## B. Recommendation memory, measured fairly, then reduced

1. **Measure aggregate memory.** Write `oracle/peak_memory.py`: it starts a
   command, then every 10 ms until it exits sums over the process and all
   its descendants (walk `/proc/<pid>/task/*/children` recursively) both
   `VmRSS` from `/proc/<pid>/status` and `Pss` from
   `/proc/<pid>/smaps_rollup`, and prints the peak of each sum and the peak
   process count. PSS is the honest figure for forked workers that share
   pages with their parent; report both. Use it for `design` and
   `recommend` on `dolby`, both implementations, both configurations
   (eight runs, one each; memory is not noisy enough to need medians).
   Keep the `/usr/bin/time -v` column too, labelled as single-process
   high-water.
2. **Share what does not depend on the candidate.** `recommend_xo`
   (`crates/secs-engine/src/xo.rs`) runs a full `design` per candidate
   crossover and each one rebuilds its inputs and measurement preparation.
   Find what every candidate computes identically (check each input of the
   preparation: if XO or anything derived from it reaches it, it is not
   shared), compute it once before the `try_map`, and share it by reference
   or `Arc`. This may need an internal entry point beside `design` that
   accepts the prepared values; keep `design`'s public signature. The
   candidate results must stay bit-identical: `xo_recommendation` tests,
   the XO goldens and the design/e2e tests must pass unchanged on both libm
   paths.
3. Re-measure (1) after (2). Record before and after.
4. Do **not** cap candidate concurrency, add a memory estimate, or change
   how `recommend_xo` uses `MemoryBudget`. If Rust's aggregate PSS is still
   above Python's in any configuration after (2), report the numbers and
   what holds the memory (which buffers, how many bytes each, per
   candidate); Claude decides the policy.

## C. CamillaDSP export

`crates/secs-camilla/src/**` is open to you for performance changes only.
Its golden tests (`cargo test -p secs-camilla`) must pass unchanged.

1. Operations shorter than 50 ms are measured with 31 timed samples after
   one warm-up, and Rust and Python batches are interleaved (Python 31, Rust
   31, Python 31, Rust 31; the row uses the median of all 62 on each side)
   so drift hits both. Apply this to every `camilla` row and to every row
   in `docs/perf/engine.md`'s "Unresolved measured rows" list that is
   shorter than 50 ms.
2. Profile the shipped `room_dba` CamillaDSP export (building the
   configuration, serializing, writing files) and remove the cause if Rust
   is slower. Find out why the shipped Rust time (2.7 ms) is above the
   one-thread Rust time (1.8 ms) for the same export and say so.

## D. Update the report

Update `docs/perf/engine.md`: the method changes above (fit envelope,
interleaved short rows, aggregate memory), the new rows replacing the old
ones, a memory table (single-process high-water, aggregate RSS, aggregate
PSS, process count), and the fixes with before/after. Leave every row you
did not re-measure as it is. `features.toml` `perf.engine` stays `partial`;
add the tests you wrote to its `tests` list (format `crate-name::test_fn`).

## Files you may change

- `crates/secs-engine/src/**`, `crates/secs-camilla/src/**`,
  `crates/secs-dsp/src/**`, `crates/secs-compute/src/**`,
  `crates/secs-export/src/**` (performance changes only), the `examples/`
  directories of those crates
- `oracle/bench_engine.py`, `oracle/peak_memory.py` (new),
  `docs/perf/engine.md`, `features.toml` (the `perf.engine` entry)
- `Cargo.lock`, only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p13b
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p13
cargo fmt --all -- --check
cargo clippy -p secs-engine -p secs-dsp -p secs-compute -p secs-export -p secs-camilla --all-targets --locked -- -D warnings
cargo test -p secs-engine -p secs-dsp -p secs-compute -p secs-export -p secs-camilla --locked 2>&1 | grep -E "test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine -p secs-export --locked 2>&1 | grep -E "test result|FAILED"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine --release --locked 2>&1 | grep -E "test result|FAILED"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine -p secs-dsp -p secs-compute -p secs-export -p secs-camilla
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

Per section: the numbers (old and new rows), each fix with before/after,
the nfev table for A, the memory table for B and what holds the memory if
Rust is still above Python, the CamillaDSP cause for C, and anything left
`not measured`.
