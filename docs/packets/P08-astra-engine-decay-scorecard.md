# P08 (ASTRA): decay measures and the scorecard

Worktree: `/home/user/wt/p08` (branch `m2/p08`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents.

Read first: `docs/ENGINE.md`, `docs/adr/0004-parity-and-deviations.md`,
`oracle/README.md`, the case set and loader P05 landed
(`oracle/engine_cases.py`, `crates/secs-engine/src/tests/cases.rs`) and
P05's `measurement.rs` / `levels.rs` (read-only for you).

## Goal

Port the late-energy and decay measures the BA cost, the RC long-mode mask,
the XO recommendation and the scorecard use, and the scorecard itself.
Upstream has five near-duplicate decay definitions; port each one exactly as
its caller uses it (unifying them is a later deviation, not this packet), but
share code where the arithmetic is genuinely identical.

Upstream:

| Function | Where | Used by |
|---|---|---|
| BA `late_db(spec)` closure and its grid (`lin`, `band`, `fs_eff`) | `correction_engine.py:672–695` (`_ba_setup`) | BA cost (P10) |
| `_late_db(H, f)` | `calibration.py:647–658` | XO recommendation (P12) |
| `mode_t60(ir, rate, center_hz, half_band=3.0)` | `mimo.py:271–289` | RC long-mode mask (P09) |
| `_late_peaks(xs, count)` | `mimo.py:322–335` | RC long-mode mask (P09), MIMO (M3) |
| `metrics(H, f, target_db, norm=(100, 400))` | `scorecard.py:18–61` | before/after scorecards, XO recommendation |

The layout stage's own `late_db` closure (`correction_engine.py:878` ff.) is
P10's; leave it.

## Contract

```rust
// decay.rs
/// The low-frequency linear grid BA measures late energy on (`_ba_setup`).
pub struct BaLateGrid { pub lin: usize, pub band: Vec<f64>, pub fs_eff: f64 }
impl BaLateGrid { pub fn new(hi_hz: f64) -> Self; }
/// The `late_db` closure: `spec` holds the first `lin` bins of a design-grid spectrum.
pub fn ba_late_db(grid: &BaLateGrid, spec: &[C]) -> f64;
/// `calibration._late_db`: 20–150 Hz late/early energy of a design-grid spectrum, dB.
pub fn late_db_20_150(h: &[C]) -> f64;
/// `mimo.mode_t60`.
pub fn mode_t60(ir: &[f64], rate: f64, center_hz: f64, half_band: f64) -> Option<f64>;
/// `mimo._late_peaks`.
pub fn late_peaks(xs: &[&[f64]], count: usize) -> Vec<f64>;

// scorecard.rs
/// `scorecard.metrics`: the 17 scalars, named as upstream's keys.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics { pub level: f64, pub in1_20: f64, pub in2_20: f64, pub in1_200: f64,
    pub in2_200: f64, pub in1_2000: f64, pub in2_2000: f64, pub turns: f64, pub gd_20: f64,
    pub gd_100: f64, pub gd_500: f64, pub gd24_abs: f64, pub gd24_abs_hz: f64,
    pub gd24_max: f64, pub gd24_span: f64, pub gd_30: f64, pub pre_db: f64 }
pub fn metrics(h: &[C], f: &[f64], target_db: &[f64], norm: (f64, f64)) -> Metrics;
```

Match NumPy where it matters: `np.percentile` (linear interpolation), median,
`np.gradient` (second-order interior, first-order edges), `np.unwrap`,
`np.interp`, `np.polyfit` for a degree-1 fit (least squares; check NumPy
2.5.3's `polyfit`: it scales and uses `lstsq`), `argmax` first index,
pairwise summation where NumPy reduces (`secs_dsp` has a pairwise sum helper;
expose it from `secs-dsp` only if needed and report it). Use
`secs_dsp::{fft, iir::butter, iir::sosfilt, iir::sosfreqz, smooth, response::gd_curve, peaks::find_peaks}`.

## Goldens (`oracle/export_engine_decay.py`, family `engine_decay`)

- `ba_late_db`: wrap `_ba_setup` while running `calibration.compute` on
  `room`, `room_manual`, `dolby`, `dolby_infra`; record `hi_hz`, the grid,
  and `late_db` evaluated on the raw sums `_ba_setup` itself builds (record
  the specs, they are short) and on a few random BA rows' sums
  (seeded; record the rows so the Rust test rebuilds the sums only if P10 has
  landed — otherwise record the specs).
- `late_db_20_150`: on P05's `long_spectra(2000)` of every role in `dolby`
  and `room` (Rust rebuilds those spectra with P05's code; record the role,
  position, and the result), plus a handful of synthetic spectra built from
  a recipe both sides evaluate (delay × peaking bands).
- `mode_t60`, `late_peaks`: every call during `calibration.compute` for all
  cases (interpose on `secs.mimo.mode_t60` / `_late_peaks` as the engine's
  `_long_mode_mask` calls them; check how it imports them), inputs recorded
  as they are (low-rate arrays are short), plus edge inputs (no decay,
  thresholds never reached, single-sample spike).
- `metrics`: `before` responses (`H0` at the main position for every main of
  `dolby`, `room`, `room_filters`; rebuilt by Rust from P05) with the target
  shape `calibration.compute` passes; synthetic recipe responses; record all
  17 values.

Budget `engine_decay` at most 3 MB.

## Tests (`crates/secs-engine/src/tests/decay.rs`, `.../scorecard.rs`)

Percentages are ratios of integer counts: exact. `gd24_abs_hz` exact
(a grid point). The rest `assert_close` with measured tolerances printed as
`MEASURE` lines (1.5 × measured, floored at `PLATFORM_RTOL`). Off the oracle
libm, report-only where an argmax over near-equal values could flip; say
which.

## Files you may change

- `crates/secs-engine/src/{decay,scorecard}.rs`,
  `crates/secs-engine/src/tests/{mod,decay,scorecard}.rs`
- `oracle/export_engine_decay.py` (new), `oracle/goldens/engine_decay/**`,
  `oracle/README.md` (family row)
- `features.toml`: set `engine.scorecard` to `implemented`; add
  `engine.decay` (kind `"engine"`, milestone `"M2"`,
  `upstream = "correction_engine._ba_setup late_db, calibration._late_db, mimo.mode_t60, mimo._late_peaks"`)
  as `implemented`
- `crates/secs-dsp/src/lib.rs` and `numeric.rs`: only to make an existing
  helper `pub` if you need it (report it)
- `Cargo.lock`, only as cargo rewrites it

Others hold `fit.rs` (P07), `ba.rs` (P10), `secs-camilla` (P11).

## Verification (foreground; finish before you report)

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

```sh
cd /home/user/wt/p08
export CARGO_INCREMENTAL=0
<venv>/bin/python oracle/run.py engine_decay
du -sh oracle/goldens/engine_decay
cargo fmt --all -- --check
cargo clippy -p secs-engine -p secs-dsp --all-targets --locked -- -D warnings
cargo test -p secs-engine -p secs-dsp -- --nocapture 2>&1 | grep -E "MEASURE|test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

Files changed; golden size; verification output with `MEASURE` lines; where
the five decay definitions differ (windows, thresholds, filters) in a short
table, for the later unification deviation; anything not matched and why.
