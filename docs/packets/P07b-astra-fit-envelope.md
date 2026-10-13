# P07b (ASTRA): gate the PEQ fitter on SECS 2's own spread

Worktree: `/home/user/wt/p07` (branch `m2/p07`), where your P07 work is
committed as `wip` and the dev branch is already merged in (so
`Fixture::calls` / `Fixture::all_calls` exist). Continue there. Do not commit; Claude reviews and commits. Do
not spawn sub-agents.

## Why

P07's report: the greedy seed, `x0`, bounds, and the residual and Jacobian at
every oracle path point match exactly; the one-step replay along the oracle
path is within 1e-14…1e-10 on 90 of 97 calls and up to 7e-8 on seven
ill-conditioned Dolby RC calls; but free-running fits end elsewhere (counts
and status differ on every converging fit, bands typically 1e-4…1e-3 apart,
and on `fit_dolby_rc_000` the Rust fit hits the cap with a 40 % different
cost while the oracle converged). That is the expected behaviour of a capped,
non-convex 48-parameter TRF fit whose steps differ in the last bits: SECS 2
itself does the same across machines (ADR 0004, "배경"). Exact end-to-end
results are therefore the wrong gate. ADR 0004 already says what the right
one is: continuous budgets are 1.5 × the measured error, and an end result
must lie within 1.25 × SECS 2's own spread between environments. This packet
applies that rule to every recorded fit.

## Changes

1. **One-step replay budget.** Keep the replay as it is, but set its budget by
   the ADR rule: 1.5 × the largest measured relative error on the oracle
   libm (print it as a `MEASURE` line), floored at 1e-9. Also print, per
   call, the condition number of the Jacobian at the worst trial (ratio of
   extreme singular values, from the same SVD the step uses) so the review
   can see the outliers are the ill-conditioned calls. Off the oracle libm the
   replay stays report-only.

2. **SECS 2's spread per call (`oracle/export_engine_fit.py`).** For every
   recorded `fit_peq` call, re-run upstream `fit_peq` on the recorded
   arguments in three more environments, each a fresh subprocess of the
   oracle venv:
   - `OPENBLAS_CORETYPE=Haswell`,
   - `OPENBLAS_CORETYPE=SkylakeX` (or the highest your CPU supports; check
     `OPENBLAS_VERBOSE=2` output and record the core actually used),
   - NumPy's SIMD dispatch enabled (unset `NPY_DISABLE_CPU_FEATURES`; that
     subprocess must skip `common.check_libm`, which refuses exactly this).
   Record per environment the returned bands, the final cost (½‖r‖² of the
   fitted bands evaluated by the same residual), `nfev`, `njev`, `status`.
   The reference run stays the one already recorded. Keep it fast: one
   subprocess per environment, all calls in it.

3. **End-result gate (`peq_converging_and_capped_results`, rename it
   `peq_results_within_secs2_spread`).** For each call, with `c0` the
   reference cost and `cmax` the largest cost across the reference and the
   three environments: assert Rust's final cost `≤ c0 + 1.25·(cmax − c0) +
   1e-12·c0`, i.e. Rust is no worse than SECS 2 is on some environment, with
   the ADR's 1.25 margin (a lower cost always passes). Print per call: Rust
   cost / `c0`, the spread `(cmax − cmin)/c0`, band differences to the
   nearest environment's bands, counts. Where every environment returns the
   same bands bit for bit (the spread is zero) and the one-step replay is
   exact, also assert Rust equals them bit for bit on the oracle libm.
   Off the oracle libm keep the cost assertion (it does not depend on bits).

4. **GD cuts.** `fit_gd_cuts` inputs are full spectra that only P09/P10 can
   rebuild. Keep what you have (flat-response test, recorded calls with
   sampled inputs and digests); add the same spread measurement for its
   inner fits; mark the end-to-end GD check as P09's (write that in
   `oracle/README.md` and in a comment at the test).

5. **Register.** If the gates above pass, set `engine.peq_fit` to
   `implemented`; list the tests. Keep the budget of `engine_fit` at 6 MB
   (bundle calls per kind and case with `Fixture::calls` /
   `Fixture::all_calls` from `secs-oracle` if the file count is high; see
   `oracle/README.md`).

Do not change `secs-dsp`. If you find a real defect in the TRF port while
doing this (a step that differs by much more than the conditioning explains,
or a residual mismatch), stop and report it with the call id and the numbers
instead of widening a budget.

## Verification (foreground; finish before you report)

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

```sh
cd /home/user/wt/p07
export CARGO_INCREMENTAL=0
<venv>/bin/python oracle/run.py engine_fit dsp && git status --short oracle/goldens/dsp
ls oracle/goldens/engine_fit | wc -l; du -sh --apparent-size oracle/goldens/engine_fit
cargo fmt --all -- --check
cargo clippy -p secs-engine -p secs-oracle --all-targets --locked -- -D warnings
cargo test -p secs-engine -p secs-oracle -- --nocapture 2>&1 | grep -E "MEASURE|PLATFORM|SPREAD|test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine fit 2>&1 | grep -E "test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

The disk is shared and was full earlier: before building, remove nothing
outside your worktree; if space runs out, `cargo clean -p secs-engine` in your
worktree and retry once, then report.

## Report

Files changed; the per-call table (stage, case, Rust cost / c0, spread,
counts, pass); the worst one-step calls with their condition numbers; golden
size and file count; verification output; anything that looks like a real
defect.
