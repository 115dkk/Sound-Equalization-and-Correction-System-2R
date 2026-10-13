# P09 (ASTRA): per-channel room correction

Worktree: `/home/user/wt/p09` (branch `m2/p09`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents.

Read first: `docs/ENGINE.md`, `docs/adr/0004-parity-and-deviations.md`,
`oracle/README.md`, and what has landed: the case set and loader (P05),
`measurement.rs`, `levels.rs`, `setup.rs` (P05), `fit.rs` with
`replay_fitter` and the recorded-fit fixture form (P07), `decay.rs` (P08),
`chain.rs` (`term`, `sub_sum`). All of those are read-only for you.

**Base and build.** Your worktree starts from `m2/p07`: P07's `fit.rs`
(`fit_peq`, `xo_target_peq`, `Fitter`, `replay_fitter`, the recorded-fit
reader in `src/tests/fit.rs`) is there; its own oracle gate is being
finalised in P07c, but the API will not change. Use the shared build
directory to save disk: `export CARGO_TARGET_DIR=/home/user/wt/target-shared`
in every shell (another worker builds there too; cargo waits for its lock,
which is fine). Do not delete anything outside your worktree.

## Goal

Port the RC stage: for one main channel, from its combined (main + sub)
responses at every position to the input correction (PEQ + FIR) and its
diagnostics, plus the per-main loop logic of `compute()` around it
(phase-resolution candidates, GD cuts).

Upstream, `src/secs/correction_engine.py`:

- `_no_boost_mask` 1098–1109 (reads `SECS_NOBOOST_MS` per call: here
  `Tuning::noboost_ms`), `_long_mode_mask` 1111–1132 (uses `to_low`,
  `late_peaks`, `mode_t60`; its note).
- `_correction` 1134–1289: weighted power with `_freq_weights`, smoothing and
  the narrow-dip limit, target + level, boost/cut caps (16 kHz cap), masks,
  half-octave range fades, `design_db`; the zero-tap branch (two fits split
  at 400/500 Hz, 900-point grid); the FIR branch: initial PEQ (700-point
  grid, `f_max = rc_hi`), FIR-band fade, minimum phase, common excess phase
  over positions (`Tuning::phase_positions`), the delay removal by IR peak,
  complex smoothing at `phase_frac`, unwrap, 3–6 kHz reference, gradient,
  fades, `realize` and the strength ladder (0.9, 0.7, 0.5, 0.3), the
  zero-lift branch, the second PEQ fit with the no-boost rule, the broad
  touch-up rounds (`FIR_TOUCHUP_ROUNDS = 2`).
- The per-main part of `compute()` 1431–1476: building `comb` per position
  (alignment delay, level trim, LR4 HP on the main plus the bus-delayed,
  polarity-flipped, LR4-LP sub sum), the two-candidate phase-fraction choice
  when `phase_frac > 6` (score: max of 1/24-octave GD minus its median on
  1200 log points in the RC range; ties keep the requested fraction; notes
  of the losing candidate stay), optional `fit_gd_cuts`, and the outputs.
- Constants 28–100 that RC reads.

Keep the arithmetic order of NumPy expressions where it can change the last
bit (complex scalar multiplication and division follow NumPy's formulas;
`secs_dsp::numeric` has the division; `measurement::delay_response` already
reproduces `_delay`).

## Contract (`crates/secs-engine/src/rc.rs`)

```rust
/// Everything one channel's RC needs. `comb` holds the combined response at
/// each position where the channel was measured, in position order.
pub struct RcInput<'a> {
    pub role: &'a str,
    pub comb: &'a [(u32, Arc<[C]>)],
    pub main_position: u32,
    pub level_db: f64,
    pub setup: &'a Setup,          // taps via config, lift, rc range, peq_bands
    pub config: &'a EngineConfig,  // taps, max_boost_db, phase_frac, target
    pub tuning: &'a Tuning,
    pub positions: &'a [u32],      // all engine positions (weights)
}
pub struct RcOutput {
    pub fir: Vec<f64>,             // `[1.0]` in the zero-tap branch
    pub response: Vec<C>,          // F × P_peq on the design grid (`chain.F` before GD cuts)
    pub peq: Vec<Peq>,
    pub phase_strength: f64,
    pub phase_band: Option<f64>,   // upstream stores the accepted fraction here
    pub design_db: Vec<f64>,
    pub notes: Vec<Note>,          // long-mode note
}
/// `_correction` at one phase fraction.
pub fn correction(input: &RcInput<'_>, phase_frac: f64, fitter: &mut Fitter<'_>) -> RcOutput;

/// The per-main step of `compute()`: candidates, choice, GD cuts.
pub struct ChannelRc { pub out: RcOutput, pub phase_frac_used: Option<f64>,
                       pub gd_cuts: Vec<Peq>, pub response: Vec<C> /* with cuts */ }
pub fn channel(input: &RcInput<'_>, fitter: &mut Fitter<'_>) -> ChannelRc;

/// `comb` for one main at one position (compute 1438–1446).
pub fn combine(main: &[C], align_delay_ms: f64, gain_db: f64, xo_hz: Option<f64>,
               sub_sum: Option<&[C]>, bus_polarity: i8, bus_delay_ms: f64) -> Vec<C>;
```

Add `Note` variants you need in a `notes` enum (create
`crates/secs-engine/src/notes.rs` if it does not exist yet: `pub enum Note`
with typed variants carrying the numbers upstream puts in its Korean note
text; register `mod notes;` in `lib.rs`).

## Goldens (`oracle/export_engine_rc.py`, family `engine_rc`)

For cases `dolby` (mixed), `dolby_zero`, `dolby_linear`, `dolby_ultra`,
`room`, `room_filters` (and `dolby`, `room` again with `gd_cuts=True`),
run `calibration.compute` and interpose on `_correction`,
`_no_boost_mask`, `_long_mode_mask`, `fit_peq`, `fit_gd_cuts` and the
per-main loop. Record per main and candidate:

- the recipe of `comb` (per position: the main's alignment delay, level trim,
  XO, the sub rows and bus gain, bus PEQ, bus delay and polarity as
  `compute` had them), so the Rust test rebuilds `comb` from P05's spectra
  with `combine` and `chain::sub_sum`; plus a sampled copy of `comb` at the
  main position to check the rebuild;
- the nested `fit_peq` / `fit_gd_cuts` calls in the recorded-fit form (P07),
  in call order;
- the masks as index ranges; `design_db`, `response` sampled (all bins in
  15–600 Hz plus every 257th bin); `fir` in full; `peq`; strength; band;
  the ladder's `(rms, max)` per tried fraction; the IR-peak index per phase
  position; the chosen candidate.

Budget `engine_rc` at most 5 MB.

## Tests (`crates/secs-engine/src/tests/rc.rs`)

Run `channel` with `replay_fitter` over the recorded calls. Exact: mask
index ranges, IR-peak indices, the ladder's accepted fraction, the chosen
phase fraction, band counts. Within measured tolerances (`MEASURE` lines,
1.5 × measured, floored at `PLATFORM_RTOL`): `design_db`, the ladder's
`(rms, max)`, `fir`, `response`, PEQ values (they come from the replay, so
they must match exactly what was injected). Report-only off the oracle libm
where a ladder threshold or argmax could flip; say which.

Also `combine` against the sampled `comb`.

## Files you may change

- `crates/secs-engine/src/rc.rs`, `crates/secs-engine/src/notes.rs` (new or
  additions), `crates/secs-engine/src/lib.rs` (module lines only),
  `crates/secs-engine/src/tests/{mod,rc}.rs`
- `oracle/export_engine_rc.py` (new), `oracle/goldens/engine_rc/**`,
  `oracle/README.md` (family row)
- `features.toml`: set `engine.rc` to `implemented` with your tests
- `Cargo.lock`, only as cargo rewrites it

Others hold `ba.rs` (P10) and `secs-camilla` (P11).

## Verification (foreground; finish before you report)

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

```sh
cd /home/user/wt/p09
export CARGO_INCREMENTAL=0
<venv>/bin/python oracle/run.py engine_rc
du -sh oracle/goldens/engine_rc
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine -- --nocapture 2>&1 | grep -E "MEASURE|PLATFORM|test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

Files changed; golden size; verification output with `MEASURE` lines; per
case and main: accepted ladder fraction, chosen phase fraction (linear),
band counts; anything not matched and why; anything upstream does that looks
like a defect (for the deviation ledger, do not fix it).
