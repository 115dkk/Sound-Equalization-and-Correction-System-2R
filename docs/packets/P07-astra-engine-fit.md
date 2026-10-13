# P07 (ASTRA): the PEQ fitter, ringing caps, GD cuts, the acoustic-XO target PEQ

Worktree: `/home/user/wt/p07` (branch `m2/p07`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents.

Read first: `docs/ENGINE.md` (method; §3 "the fitter is cut by replay"),
`docs/adr/0004-parity-and-deviations.md`, `oracle/README.md` (TRF paragraph),
and how M1 gated SciPy's TRF: `oracle/export_dsp.py` (`traced`, `replay`),
`crates/secs-dsp/tests/trf.rs`, `crates/secs-dsp/tests/trf_replay.rs`,
`secs_dsp::optimize::{least_squares_with_trace, trf_one_step}`. P05 has
landed: the case set (`oracle/engine_cases.py`, family `engine_inputs`), the
loader `crates/secs-engine/src/tests/cases.rs`, and the measurement stage.

## Goal

Port `fit_peq` and its relatives so every later stage (RC, BA bus and bundle
PEQ, XO PEQ, M3's MIMO IIR seeds) calls one fitter, and gate it the way M1
gated TRF: a capped least-squares fit amplifies last-bit differences into
different answers, so trajectories are checked one step at a time on the
oracle's path, and only converging fits are checked end to end.

Upstream, `src/secs/correction_engine.py`:

- `fit_peq(g, corr_db, n, lo=20, hi=500, boost=10, ring_ms=None, f_max=None)`
  lines 221–285: greedy placement (largest residual, half-height width Q,
  clips), optional ringing caps, TRF refinement in `(ln f, ln Q, gain)` with
  bounds, region weights 1 / 0.15, the band-response LRU cache (a pure
  speed device: reproduce the values, not the cache), `max_nfev` 400 or 150,
  final Q re-cap.
- `_ring_q_cap` 288–290, `_ring_boxes` 293–306.
- `fit_gd_cuts(f, H, lo, hi, n_max=6, depth=3)` 317–355 and `gd_curve`
  (already in `secs_dsp::response`).
- `CorrectionEngine._xo_target_peq(by_pos, level_db, lo, hi, bands, boost, f_max)`
  1301–1316 (weighted power average, frequency-dependent smoothing, narrow-dip
  limit, clip, 500-point log interpolation, soft edges, `fit_peq` without a
  ring cap).

## Contract (`crates/secs-engine/src/fit.rs`)

```rust
/// One `fit_peq` call's arguments.
#[derive(Debug, Clone)]
pub struct FitRequest<'a> {
    pub g: &'a [f64], pub corr_db: &'a [f64], pub n: usize,
    pub lo: f64, pub hi: f64, pub boost: f64,
    pub ring_ms: Option<f64>, pub f_max: Option<f64>,
}
/// The fitter a stage is given: production passes `fit_peq`; oracle tests
/// pass a replay of the oracle's answers.
pub type Fitter<'f> = dyn FnMut(&FitRequest<'_>) -> Vec<Peq> + 'f;

pub fn fit_peq(req: &FitRequest<'_>) -> Vec<Peq>;
pub fn ring_q_cap(f0: f64, gain_db: f64, ring_ms: f64) -> f64;
pub fn fit_gd_cuts(f: &[f64], h: &[C], lo: f64, hi: f64, n_max: usize, depth_db: f64) -> Vec<Peq>;

/// `_xo_target_peq`: `by_pos` holds (position weight, response) pairs in
/// position order; `target` is the engine target.
pub fn xo_target_peq(by_pos: &[(f64, &[C])], target: &TargetCurve, level_db: f64,
                     lo: f64, hi: f64, bands: usize, boost: f64, f_max: Option<f64>,
                     fitter: &mut Fitter<'_>) -> Vec<Peq>;

/// For tests of later stages: a fitter that answers each call with the
/// oracle's recorded result, after checking the request matches the recorded
/// arguments within `rtol` (panics with the call index and the worst
/// mismatch otherwise).
#[cfg(test)]
pub(crate) fn replay_fitter(calls: Vec<RecordedFit>, rtol: f64) -> impl FnMut(&FitRequest<'_>) -> Vec<Peq>;
#[cfg(test)]
pub(crate) struct RecordedFit { /* arguments and result as recorded */ }
```

Put `RecordedFit` and its fixture reader where later families can use it
(`src/tests/fit.rs` exports a `pub(crate)` reader; `replay_fitter` stays in
`fit.rs` under `#[cfg(test)]`). The fixture form of one recorded call (keys,
arrays) is yours; document it in `oracle/README.md`, because P09, P10 and
P12 will record calls in the same form.

Least squares: call `secs_dsp::optimize` (the TRF port) with the exact
options upstream uses. Do not change `secs-dsp`; if it lacks something,
report it.

## Goldens (`oracle/export_engine_fit.py`, family `engine_fit`)

Interpose on `fit_peq`, `fit_gd_cuts` and `scipy.optimize.least_squares`
(imported inside `fit_peq` at call time, so module-attribute interposition
works) while running the real engine on the cases. Record, per `fit_peq`
call: the arguments, the greedy seed (bands before TRF, and the TRF `x0`,
bounds), the TRF trace in the form `trf_replay.rs` consumes (move the shared
tracing helpers of `export_dsp.py` into `oracle/trf_trace.py` if you need
them, and check `dsp` goldens regenerate byte-identically), the TRF result
(`x`, `nfev`, `njev`, `status`, cost) and the returned bands. Which call this
was (stage, role, case) goes in `meta` (find it from the interposed caller's
stack or by recording around the stage methods).

- `fit_peq`: every call from cases `dolby`, `dolby_zero`, `dolby_linear`,
  `room`, `room_filters` running `calibration.compute` (RC, XO PEQ, bus PEQ,
  bundle PEQ via the layout cases `room_dba`, `room_multi`). If that is too
  large, keep every call of `dolby` and `room` and the first three per stage
  of the others.
- `fit_gd_cuts`: production leaves `gd_cuts=False`; construct the engine with
  `gd_cuts=True` for `dolby` and `room` and record those calls (`H` is a full
  spectrum: record it as the reconstruction recipe — role, position, the
  combined response's ingredients — or, if that is impractical, sampled plus
  a digest, and say which).
- `_xo_target_peq`: every call for main roles (the `xo_stage` path of
  `compute`), with its scalar arguments and the identities of `by_pos`
  (positions, role, the gain applied) so the Rust test rebuilds `by_pos` from
  P05's `prepare` output.

Long traces as digests (as `dsp_de` does); budget `engine_fit` at most 6 MB.

## Tests (`crates/secs-engine/src/tests/fit.rs`)

- Greedy placement, `x0` and bounds: exact on the oracle libm, `rtol(…)`
  elsewhere.
- TRF: one-step replay along the oracle trace (residual and Jacobian at the
  oracle's points, the step taken, the cost reduction with the `x_new`
  correction M1 uses); converging fits (status other than the evaluation
  cap): exact `nfev`, `njev`, `status` and the returned bands within
  tolerance; capped fits: report-only (print the trace agreement), as in
  `crates/secs-dsp/tests/trf.rs`. Off the oracle libm (macOS, Windows) the
  replay is report-only.
- `ring_q_cap`, `_ring_boxes`: exact formula checks on recorded values.
- `fit_gd_cuts`, `xo_target_peq`: rebuilt inputs, recorded results; same
  converge/cap rule for their inner fits.
- `replay_fitter` itself: a mismatching request panics with a useful message.

Print `MEASURE` lines for every tolerance you set (ADR 0004: 1.5 × measured,
floored).

## Files you may change

- `crates/secs-engine/src/fit.rs`, `crates/secs-engine/src/tests/{mod,fit}.rs`
  (add `mod fit;` to `mod.rs`)
- `oracle/export_engine_fit.py` (new), `oracle/trf_trace.py` (new, optional),
  `oracle/export_dsp.py` (only to import moved helpers; its goldens must not
  change), `oracle/goldens/engine_fit/**`, `oracle/README.md` (family row,
  recorded-fit form)
- `features.toml`: add `engine.peq_fit` (kind `"engine"`, milestone `"M2"`,
  `upstream = "correction_engine.py fit_peq, _ring_q_cap, _ring_boxes, fit_gd_cuts, _xo_target_peq"`)
  as `implemented` with your tests
- `Cargo.lock`, only as cargo rewrites it

Others hold `setup.rs`, `measurement.rs`, `levels.rs` (landed, read-only for
you), `decay.rs`, `scorecard.rs` (P08), `ba.rs` (P10), `secs-camilla` (P11).

## Verification (foreground; finish before you report)

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

```sh
cd /home/user/wt/p07
export CARGO_INCREMENTAL=0
<venv>/bin/python oracle/run.py engine_fit dsp && git status --short oracle/goldens/dsp
du -sh oracle/goldens/engine_fit
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine -- --nocapture 2>&1 | grep -E "MEASURE|PLATFORM|test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

Files changed; how many calls were recorded per stage and how many converged
vs hit the cap; golden size; verification output with `MEASURE` lines; the
recorded-fit fixture form; anything not matched and why.
