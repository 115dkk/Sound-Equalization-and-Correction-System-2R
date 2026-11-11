# P217 (ASTRA): a calibration reports the work it has really done

Worktree `E:\wt\calib-progress`, branch `feat/calibration-progress`. Build
with `CARGO_TARGET_DIR=C:\cargo-target\calib-progress` set for every cargo
command (the worktree must not get a `target` folder of its own; this target
folder is new, so the first build is a full one). Another worker is building
in a different worktree with a different target folder; never touch
`C:\cargo-target\dirac-limit` or anything outside this worktree. Do not
commit, do not push. Run every command in the foreground and end your turn
only after the report. Do not touch `apps\secs2r\ui`, the upstream Python
tree, `CHANGELOG.md` or `features.toml` (the main session registers the
feature after your report).

## Why

The maintainer wants the bar in the job tray to show what the backend really
measured (2026-11-11), and asked for the same in calibration: 'it would be
good if the backend can measure the calibration's progress; if it gets
meaningfully slower, the calibration's progress may be left out'.

The screen now draws a bar only from a job's reports. What a calibration
reports today is too coarse to read as progress. Measured with the release
CLI on a 7.1.4 project with 2 subs, 9 positions, MIMO on, `--compute cpu`
(9.6 s in all; the trace tools are below):

```
engine.design  0/12 .. 12/12   6 reports
   quiet 1.4 s  after 0/12
   quiet 3.2 s  after 1/12      (then 2/12 .. 12/12 within about 0.4 s)
   quiet 1.5 s  after 12/12 until engine.mimo 0/2
engine.mimo    0/2 .. 2/2      3 reports
   quiet 1.1 s  after 0/2
   quiet 1.8 s  after 1/2
```

So the bar stands at 8 % for a third of the job and then jumps to the end,
and the MIMO part is two steps. The units count things that are not where
the time goes: `engine.design` is 1 for the bass alignment plus 1 per main
(`crates\secs-engine\src\design.rs`, `design_pass`; `progress.advance(1)` in
`ba_stage` and once per role in `rc_stage`, whose roles finish together
because they run through the parallel map), and `engine.mimo` is 2
(`crates\secs-engine\src\mimo\run.rs`).

## What to do

Make the stages of the jobs `calibrate`, `mimo` and `recommend_xo` count the
work that takes the time, in units that are known before the stage starts.
The mechanism exists and stays: `ctx.stage(id, total)` returns a
`StageProgress` (`crates\secs-compute\src\lib.rs`); `advance` and `set` are
thread-safe and already throttled to one report per 100 ms
(`PROGRESS_INTERVAL`), so calling them from inside the parallel map is
allowed. Do not add another reporting mechanism and do not change
`secs-compute`'s public API.

First measure, then choose the units:

1. Build the release CLI and record the reference trace (commands below).
   Use the engine's own observers and `#[cfg(test)]` timings (`observed`,
   `observe("...")` in `mimo\design.rs` and `mimo\run.rs`, `timings` in
   `design.rs`) to see which phase each quiet stretch is.
2. For each phase that takes a visible share of the time, find the loop
   whose iterations are the work (optimizer evaluations or candidates in
   `ba.rs`, the bands or fits of one main in `rc.rs`/`fit.rs`, the inputs
   and sweeps of the MIMO design in `mimo\design.rs` and `mimo\solve.rs`,
   the supports in `mimo\iir.rs`) and count those.
3. If a phase's count cannot be known when the enclosing stage starts
   without doing the work twice, give the phase a stage of its own with a
   new id under `engine.` (for example `engine.ba`), started when its total
   is known. List every stage id a job can emit in your report, with one
   line on what the phase does; the main session writes the screen's words
   for them.

## The contract

For every stage a calibration job emits (including the existing
`engine.resample`, `engine.mimo_iir`, `engine.xo_recommendation`, and the
stage the service reports while it reads measurements):

- The first report is `(0, total)` and the last is `(total, total)`; `done`
  never decreases; `total` is constant within the stage and never 0. A
  stage with nothing to count is not started (`ctx.stage("engine.mimo_iir", 0)`
  in `mimo\iir.rs` goes away).
- No stage is a placeholder: a total of 1 or 2 that stands for seconds of
  work is what this packet removes. A phase that really is one short step
  needs no stage.
- The automatic RC limit runs `design_pass` twice (probe, then final). The
  screen must not see the bar run to the end and start again under the same
  words: either one stage covers both passes with a total known up front,
  or the two passes are two stages with different ids.
- Stages follow each other without long silence: on the reference project
  no stretch between two consecutive events of the job is longer than 1 s
  or 10 % of the job's wall time, whichever is larger. That includes the
  stretch between the design and the MIMO part (`late_energy`, the commit
  and the MIMO preparation in `crates\secs-service\src\calibration.rs`).
- What is computed does not change. Every golden and oracle test passes
  unchanged, and the reference calibration's result is byte-identical
  before and after (see 'Checks'). Keep the repository's rules: parallel
  work only through the context's map, floating-point accumulation in the
  original order, no `unsafe`, faer with `Par::Seq`.
- It does not get slower: the median wall time of the reference calibration
  changes by no more than 2 % (see 'Checks'). Never put `advance` in an
  innermost numeric loop; count at the level of an iteration that itself
  takes milliseconds.

The maintainer allows leaving a phase out: if a phase cannot be counted
without slowing the calculation measurably or changing its numbers, leave
it as it is, say which phase and what you measured, and still meet the
rules for the others.

## Tests

- `secs-engine`: with a recording `ProgressSink` (see the one in
  `crates\secs-compute\src\lib.rs` tests and
  `crates\secs-filters\tests\golden.rs`), run a calibration, a MIMO run
  (FIR supports and IIR supports) and an XO recommendation on a testkit
  case that has subs and several positions, and assert the contract's rules
  for every stage seen, that no stage has a total below 3 unless the case
  really has fewer items, and that the design stage (or stages) reports
  more distinct `done` values than the number of mains.
- `secs-service` (`crates\secs-service\tests\calibration.rs`): a calibrate
  job with MIMO emits its stages in order with the rules above, and no
  `engine.mimo_iir` with total 0.
- Update any existing test that asserted the old totals so it proves the
  same thing with the new ones.

## Checks to run (foreground) and report

```
set CARGO_TARGET_DIR=C:\cargo-target\calib-progress
cargo fmt --all -- --check
cargo clippy -p secs-engine -p secs-service --all-targets --locked -- -D warnings
cargo test -p secs-compute -p secs-engine -p secs-engine-testkit --locked
cargo test -p secs-service --locked
set RUSTDOCFLAGS=-D warnings
cargo doc -p secs-engine -p secs-service --no-deps --document-private-items --locked
cargo build --release -p secs2r-cli --locked
```

The reference trace, before (build the CLI once from the untouched tree and
keep a copy of that exe) and after:

```
set T=C:\Users\32170336\AppData\Local\Temp\claude\8a2ed516e7e71310\9cfb4797-a42c-54d9-a68d-f128d938515e\scratchpad\calib
set CLI=C:\cargo-target\calib-progress\release\secs2r-cli.exe
%CLI% --quiet project new <dir>\room.secs --preset 7.1.4 --subs 2
%CLI% --quiet sample <dir>\room.secs --positions 9
%CLI% --quiet project set <dir>\room.secs --mimo on
python -I %T%\trace.py %CLI% <dir>\trace.jsonl --compute cpu calibrate <dir>\room.secs --mimo
python -I %T%\summary.py <dir>\trace.jsonl 0.5
```

`trace.py` timestamps every line the CLI prints; `summary.py` prints each
stage with its times and counts and every quiet stretch above the given
seconds. Use a fresh `<dir>` under your own scratch folder for each project
(a calibrated project caches its result). Also trace `--mimo` with IIR
supports (`project set --mimo-supports iir`), `recommend-xo`, and one
project where the RC limit is automatic if the default is not.

Identical results: run the reference calibration with
`--deterministic-uuids --quiet` before and after and compare the printed
`results` reply (drop fields that are wall-clock timings, if any, and say
which).

Speed: seven runs of the reference calibration with each exe, alternating
before and after, fresh project each time; report both medians. The other
worker's builds load this machine, so if the two medians differ by more
than the spread within one exe, repeat when the machine is quieter and
report both attempts.

If an API is not what this packet says, read the code and use what is
there; do not guess.

## Report

- Each file changed, with one line on what changed.
- Every stage id each job kind can emit, its unit, how its total is known
  up front, and one line on what the phase does.
- The `summary.py` output before and after for each traced case.
- The two medians and the result comparison.
- Any phase you left coarse, and why.
- The output tail of every check above, and anything you could not do.

## After review (main session, 2026-11-11)

- The bass alignment could not be counted (its optimizer stops on
  convergence). So that the screen does not show the previous stage's full
  bar while it runs, it is announced as a stage with no total:
  `engine.ba` (and `engine.ba_probe` in the probe pass) is reported once as
  `(0, 0)` when the project has subs and the pass is not a candidate of the
  XO recommendation. This is the one exception to 'a total is never 0': the
  screen names such a stage and draws no bar.
- Left coarse, as the report says: the IIR support fit (per input) and the
  factorization for IIR-sized systems.
- Re-measured by the main session with the final build: the reference
  calibration's printed result is identical to the baseline's (four FIR
  pairs, one IIR pair); medians 8.439 s before and 8.445 s after.
