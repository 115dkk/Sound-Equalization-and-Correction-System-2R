# P18c (ASTRA): compute MIMO's shared work once, and a blocked tridiagonal reduction

Worktree: `/home/user/wt/p18` (branch `m3/p18`, at main after 0.10.1). Work
only there. Do not commit; Claude reviews the diff and commits. Do not spawn
sub-agents. Build with
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p16`.
The machine is yours alone while you run: no other build or benchmark runs.
Time-box: finish Parts A and C first; Part B only if time remains. Run the
verification and write the report before you stop, even if Part B is not done.

Read first: `docs/perf/mimo.md` (P18, P18b), `docs/MIMO.md` §3 and §6,
`docs/adr/0007-*`, `crates/secs-engine/src/mimo/` (`design.rs`
`design_mimo_inputs`/`design_input`, `iir.rs`, `run.rs` `compute`/`assemble`,
`solve.rs` `RidgeSweep`).

## Where the time is (Claude's quiet-machine runs, big case, 2026-10-14)

| Operation | Config | SECS 2 s | Rust s | Ratio |
|---|---|---:|---:|---:|
| whole MIMO, IIR | shipped | > 5,280 (not finished in 88 min) | 81.8 | > 64× |
| whole MIMO, IIR | one | 3,168.8 | 286.5 | 11.1× |
| whole MIMO, FIR mode | shipped | 13.47 | 2.98 | 4.5× |
| whole MIMO, FIR mode | one | 14.36 | 5.94 | 2.4× |
| `mimo_design_one` (L, 180 taps) | shipped | 3.13 | 2.22 | 1.4× |
| `mimo_design_one` (L, 180 taps) | one | 5.18 | 2.46 | 2.1× |
| `mimo_iir_one` (L) | one | 275.9 | 23.25 | 11.9× |
| peak PSS, whole IIR | shipped | ≥ 1,901,543 KiB (25 min cap) | 808,083 KiB | |
| peak PSS, whole IIR | one | ≥ 1,152,868 KiB (10 min cap) | 483,028 KiB | |

Record this table in `docs/perf/mimo.md` as given (the method lines are in
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/measure_{a,b,c,d}.sh`: Rust `engine_perf` defaults or
`SECS_PERF_RUNS=3`; Python one `--time-budget 1` sample for the whole IIR
runs, `--runs 3` for FIR mode; memory by `oracle/peak_memory.py`, with
`timeout` caps where noted, so those Python peaks are lower bounds).

The rows below 5× are the ideal FIR design and FIR mode, which is all design.
Two kinds of repeated work are visible in the code:

- `crate::export_view(...)` and `measurement::prepare(...)` run inside
  `design_mimo_inputs`, again in the IIR fit, and again in `run::assemble`.
- `support_path(out, r, &spectra, p, align)` and its `to_low` projection
  depend on the candidate `r` and position `p`, not on the input, yet are
  rebuilt for every input.

## Part A: shared work once (bit-identical)

1. Compute the spectra and the measurement preparation once per
   `mimo::compute` (and once per `design_mimo` when called alone) and pass
   them down to the design, the IIR fit and `assemble` through one
   crate-private struct. Public signatures stay as they are.
2. Build each `(position, candidate)` low-rate support path once per design
   (a map over pairs is fine; no reduction across threads) and let every
   input take its candidates from that table. Count its memory in the
   budget permit honestly (it is shared, so acquire it once).
3. Profile `mimo_design_one`, `mimo_design_all` and FIR-mode `mimo::compute`
   on `big` before and after (one thread and shipped), with section timers
   as in P18b, and report where the remaining time is.

Proof: every MIMO test passes unchanged; add a test that the whole
`mimo::compute` (IIR and FIR, `room_dba`) and `design_mimo_traced` on every
stage-boundary case give **bit-identical** results to the code before this
packet (compute the "before" path in the test by calling the per-input
preparation directly, or compare against values you record from the current
code in the test itself; do not add goldens for this).

## Part B: blocked tridiagonal reduction (only if time remains)

`faer::linalg::evd::tridiag::tridiag_in_place` under `Par::Seq` is the
remaining large cost of the λ sweep (about 1.0 s of 1.25 s at n = 2,160).
If, and only if, a blocked reduction (LAPACK `dsytrd`/`dlatrd` style: panel
of `nb` reflectors, trailing update as a symmetric rank-2k product through
faer's sequential `matmul`) is at least 1.5× faster on the big case's actual
matrix, use it in `RidgeSweep`:

- It must produce `T` and a Householder representation that the existing
  `RidgeSweep::solve` applies (or adapt the application to it), with the
  same fallbacks.
- The P18b proofs (`mimo_ridge_sweep_recorded_systems_and_trajectories`,
  `mimo_ridge_sweep_falls_back`) must pass unchanged, plus a test of the
  reduction itself: `‖Qᵀ M Q − T‖ / ‖M‖ ≤ 16·n·ε` and `‖QᵀQ − I‖ ≤ 16·n·ε`
  on two recorded systems.
- Sequential only, no parallel reduction (ADR 0007).

If it is not 1.5× faster, keep faer's and report the numbers.

## Part C: the rows

Re-measure Rust only (the SECS 2 side is above): `mimo_design_one`,
`mimo_design_all`, `mimo_fir`, `mimo_iir` on `big`, both configurations,
with `engine_perf` (default sampling; `SECS_PERF_RUNS=3` for `mimo_iir`).
Add them to `docs/perf/mimo.md` next to the table above with the ratios.
Do not write a verdict; Claude writes it.

## Files you may change

- `crates/secs-engine/src/mimo/**`, `crates/secs-engine/src/tests/**`,
  `crates/secs-engine/examples/engine_perf.rs`
- `docs/perf/mimo.md`
- `features.toml` (the `perf.mimo` entry's `tests` only)

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

What you shared and how, the proofs, the profiles before and after, Part B's
benchmark and decision, the new rows, anything you could not do.
