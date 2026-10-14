# P18 (ASTRA): MIMO performance and memory audit against SECS 2

Worktree: `/home/user/wt/p18` (branch `m3/p18`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p16`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.
The machine has four cores and other work may run: take every timing as the
median of at least five runs (three for a call over a minute), check
`uptime` before each pair, and repeat a comparison that is within 20 % once
the machine is quiet.

Read first: `docs/MIMO.md` (§3 the computing decisions, §6 the exit
conditions this packet measures), `docs/perf/engine.md` (P13–P13c: the
method, the tools and how the table is written), `docs/adr/0007-*`
(parallelism: map only, floating-point accumulation in the original order,
`faer` sequential), and the MIMO code: `crates/secs-engine/src/mimo/`
(`design.rs`, `solve.rs`, `iir.rs`, `run.rs`), `crates/secs-dsp` (the
normal-equation TRF), `crates/secs-camilla`, `crates/secs-export`. The
tools to reuse: `oracle/bench_engine.py`, `oracle/peak_memory.py`,
`crates/secs-engine/examples/engine_perf.rs`.

## Goal

M3 ends when, on one large case, MIMO is at least **five times faster per
input** than SECS 2 on the same machine and the whole `mimo::compute` stays
under **1 GB of process-tree PSS** and below SECS 2's (MIMO.md §6). Measure
it, write `docs/perf/mimo.md`, and make Rust faster or smaller where it
misses, without changing any result a golden test checks.

## The large case

Add `big_case()` to `oracle/engine_cases.py`: six positions, a 7.1.4 layout
with two subs, built the way `_room()` builds the four-position room (the
modal room, the upstream synth for excitation, deterministic offsets). Use
the role names upstream's 7.1.4 preset gives. Keep it **out of `cases()`**:
existing exporters iterate `cases()` and their goldens must not change.
Record nothing for it under `oracle/goldens/` except what the Rust bench
needs to load the case (the measurements as the existing `engine_*`
families store them, under `oracle/goldens/perf_mimo/`, below 8 MiB; if the
IRs do not fit, generate them in the Rust bench from the same recipe and
say so). Both sides design the calibration first (not timed), then MIMO.

## Operations

On `big_case` and, for reference, `room_dba`:

1. the ideal FIR design per input (`mimo.design_mimo` restricted to one
   input against Rust's per-input design step) and for all inputs;
2. the IIR fit per input (`mimo_iir` for one input: seeds plus the joint
   fit) and for all inputs;
3. the whole run, IIR and FIR modes (`calibration.compute_mimo` against
   `mimo::compute`);
4. the MIMO exports: CamillaDSP with paths, staged mdat and before/after
   mdat with MIMO items.

Two configurations each, as P13 defines them: **as shipped** (Rust release,
default threads; Python with SIMD dispatch and OpenBLAS default threads and
upstream's own MIMO process pool `MIMO_WORKERS` as it ships) and **one
thread** (Rust one-thread `Context`; Python `OPENBLAS_NUM_THREADS=1`,
`workers=1`). Python timing: `time.perf_counter` around the upstream call
after one warm-up, a fresh process per configuration (extend
`oracle/bench_engine.py`). Rust: extend `engine_perf.rs` with `mimo_*`
operations, run as `cargo run --release --example engine_perf -- <op> <case>`.
No new dependencies.

Memory: `oracle/peak_memory.py` around one fresh-process call of the whole
run (IIR mode), both sides, both configurations, aggregate PSS of the
process tree (SECS 2's pool workers included).

## Fixes

Where Rust misses 5× per input or the memory bound, find the cause with a
profile (`perf` is installed if `perf record` works; otherwise timers inside
the bench) and fix it. Allowed: anything that keeps every golden test
passing bit for bit where it asserts bits and within its gate where it
asserts a gate. Not allowed: changing a tolerance or a gate, parallel
reductions, `faer` parallelism, a different optimizer path, raising the
memory budget to hide a peak. Each fix gets a test that proves it kept the
result (as P13b's `*_preserves_*` tests do). If a miss cannot be fixed
within those rules, stop and report it with the profile; do not weaken the
target.

## Output

- `docs/perf/mimo.md`: method, machine, the per-input and whole-run tables
  (Python seconds, Rust seconds, ratio, both configurations), the memory
  table, each fix with before and after, and a verdict against MIMO.md §6.
- `features.toml`: add `perf.mimo` (kind `perf`, M3) pointing at the tests
  that prove the fixes kept the results (and the existing MIMO golden tests
  if no fix was needed); `implemented` only if every row meets the target.

## Files you may change

- `oracle/engine_cases.py` (add `big_case()` only), `oracle/bench_engine.py`,
  `oracle/peak_memory.py`, `oracle/goldens/perf_mimo/**`
- `crates/secs-engine/examples/engine_perf.rs`, `crates/secs-engine/src/mimo/**`,
  `crates/secs-engine/src/tests/**`, `crates/secs-dsp/src/**` and its tests
  (fixes only), `crates/secs-camilla/src/**`, `crates/secs-export/src/**`
  (fixes only)
- `docs/perf/mimo.md` (new), `features.toml` (the one entry)

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p18
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p16
cargo fmt --all -- --check
cargo clippy -p secs-engine -p secs-dsp -p secs-camilla -p secs-export --all-targets --locked -- -D warnings
cargo test -p secs-engine -p secs-dsp -p secs-camilla -p secs-export --locked 2>&1 | grep -E "test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine --release --locked 2>&1 | grep -E "test result|FAILED"
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

The tables, each fix and its proof, anything you could not measure or fix
and why, and the verdict.
