# P13 (ASTRA): performance audit of the engine against SECS 2, and the fixes it needs

Worktree: `/home/user/wt/p13` (branch `m2/p13`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_TARGET_DIR=/home/user/wt/target-p13` (yours alone). The
machine has four cores and other work may run: take every timing as the
median of at least five runs, and repeat a comparison that is within 20 %
once the machine is quiet (check `uptime`).

Read first: `docs/ENGINE.md` §5 (performance and cancellation),
`docs/adr/0007-*` (parallelism: map only, floating-point accumulation in
the original order, `faer` sequential), `docs/adr/0004-parity-and-deviations.md`,
and the engine (`crates/secs-engine`, public surface in `lib.rs`),
`crates/secs-compute`, `crates/secs-dsp`, `crates/secs-export`,
`crates/secs-camilla`.

## Goal

M2 ends when the engine is faster than SECS 2 on every operation a user
waits for, on the same machine (ENGINE.md §5). Measure every such operation
in both, write the table, and make Rust faster wherever it is not, without
changing any result a golden test checks.

## Operations

For the cases `dolby` (default mode), `dolby_zero`, `dolby_linear`,
`dolby_ultra`, `room`, `room_dba` (from `oracle/engine_cases.py`), measure:

1. the whole calibration (`calibration.compute` against `design()`), and its
   stages: measurement preparation, BA (`ba::align` with bus delay and fold),
   acoustic-XO top PEQ (all roles), RC per channel (one channel and all),
   scores and curves;
2. one `fit_peq` call of each kind (RC first pass, RC second pass, bus, XO,
   GD cuts) on recorded inputs;
3. the XO recommendation (`recommend_xo`, four candidates) on `room` and
   `dolby`;
4. an XO preview call after the cache exists, and building the cache;
   `sub_combine`;
5. the exports: CamillaDSP (`secs-camilla::export`), staged mdat and
   before/after mdat (`secs-export`);
6. `.secs` load and save of the demo project (`secs-project`).

Both sides run the release build / the same oracle venv
(`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`),
in two configurations each:

- **as shipped**: Rust release with the default thread count; Python with
  NumPy's SIMD dispatch on and OpenBLAS's default threads (the way SECS 2
  runs for a user; do not set `NPY_DISABLE_CPU_FEATURES`; upstream's own
  process pool for `recommend_xo` as it ships);
- **one thread**: Rust with a one-thread `Context`; Python with
  `OPENBLAS_NUM_THREADS=1` and its process pool limited to one worker.

The Python timer is `time.perf_counter` around the upstream call, after one
warm-up call, in a fresh process per configuration
(`oracle/bench_engine.py`, new). The Rust timer is a release-mode bench
binary (`crates/secs-engine/examples/perf.rs`, and `crates/secs-export/
examples/perf.rs` if you need one), run with
`cargo run --release --example perf -- <op> <case>`; no new dependencies.

## Fixes

Where Rust is slower in either configuration, profile it (`perf record`
on the release example if available, otherwise timers around the hot
loops) and fix the cause. Allowed: caching values recomputed with the same
inputs, reusing FFT plans and buffers, avoiding clones of spectra (they are
`Arc`), hoisting invariant work out of loops, `ctx.map` over independent
items (channels, positions, candidates, groups), better data layout.
Not allowed: changing the order of a floating-point accumulation, a
different algorithm, `unsafe`, `faer`'s parallel mode, relaxing any test.
Every golden test must pass afterwards exactly as before, on both libm paths
(run the full engine suite and `SECS_ORACLE_FOREIGN_LIBM=1` again). If a
speed-up would need an arithmetic change, stop and report it with the
numbers instead.

Also check memory: peak resident set size of the `dolby` design and of
`recommend_xo` on `dolby` in both implementations (`/usr/bin/time -v` for
both). Rust must not use more.

## Deliverables

- `docs/perf/engine.md` (new): the method, the machine (`lscpu`, `free -g`),
  and one table per configuration: operation, case, Python median, Rust
  median, ratio, peak RSS where measured. Then the fixes you made, each with
  the before/after numbers.
- `oracle/bench_engine.py`, the Rust examples.
- `features.toml`: add `perf.engine` (kind `"perf"` if the registry accepts
  it, otherwise `"engine"`; milestone `"M2"`), `implemented` only when Rust
  is faster on every row of both tables. Timing is not a CI gate: list the
  existing design and e2e tests (they prove the fixes kept the results) and
  put `docs/perf/engine.md` in the entry's `note`.

## Continuation (Claude, after the first run hit the two-hour limit)

Your first run was stopped at the session's two-hour limit. Its work is
still uncommitted in this worktree (examples, `oracle/bench_engine.py`,
`docs/perf/engine.md` with the dolby rows, `smooth.rs`, `design.rs`,
`fit.rs`). `fit.rs` was edited at 15:08, after your last test run (14:38):
start by finishing or reverting that edit and re-running the full engine
suite on both libm paths and in release mode. Results must stay bit-equal
where the goldens require it.

This run also has a hard two-hour limit. Budget it: stop starting new
measurements at 90 minutes, then write the report.

1. **Comparable rows.** In the shipped configuration a single channel's RC
   time overlaps the other channels' tasks, so `rc:<role>` is compared in
   the one-thread table only; the shipped table compares `rc_all` and
   `design`. Say so in `docs/perf/engine.md`.
2. **Fix what is slower**, in this order: `fit_bus` (1.23), `sub_combine`
   (1.13), `camilla` (1.13), `fit_rc1` (1.03). For each, find where the time
   goes (timers around the hot loops) and fix it within the packet's rules.
   If a row cannot be made faster without changing arithmetic, write why
   and stop on that row.
3. **Fill the tables** for `dolby`, `room`, `room_dba` in both
   configurations (the other dolby modes only for `design`). Use three
   timed runs instead of five for operations over 10 s (`design`,
   `recommend`), five otherwise. Skip a cell rather than run past the
   budget, and mark it `not measured`.
4. Do not register `perf.engine` as implemented unless every measured row
   is faster; leave it `partial` with the table path in its note otherwise.

## Files you may change

- `crates/secs-engine/src/**` (performance changes only), `crates/secs-dsp/src/**`
  (performance changes only), `crates/secs-compute/src/**` (performance only),
  `crates/secs-export/src/**` (performance only), the `examples/` directories
  of those crates
- `oracle/bench_engine.py` (new), `docs/perf/engine.md` (new),
  `features.toml` (the entry above)
- `Cargo.lock`, only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p13
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p13
cargo fmt --all -- --check
cargo clippy -p secs-engine -p secs-dsp -p secs-compute -p secs-export --all-targets --locked -- -D warnings
cargo test -p secs-engine -p secs-dsp -p secs-compute -p secs-export --locked 2>&1 | grep -E "test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine -p secs-export --locked 2>&1 | grep -E "test result|FAILED"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine --release --locked 2>&1 | grep -E "test result|FAILED"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine -p secs-dsp -p secs-compute -p secs-export
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

The two tables; the fixes with before/after; peak memory; anything still
slower and why; any speed-up you did not make because it would change
arithmetic.
