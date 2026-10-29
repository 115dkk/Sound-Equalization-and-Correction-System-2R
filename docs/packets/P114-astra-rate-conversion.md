# P114 (ASTRA): moving designed filters to an output rate (ADR 0017)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\output-rates`
(branch `claude/output-rates`). Do not commit, do not push. Work only in
this worktree. P115 (Daybreak) will later thread the output rate through
the project, service and exporters using what you build here; do not touch
those.

Read first: `CLAUDE.md`, `docs/adr/0017-output-sample-rate.md`,
`docs/adr/0007-compute-and-gpu.md` (determinism: parallel map only, float
accumulation in the original order), `crates/secs-dsp/src/fir.rs`
(`lowrate_len_48k`, `lowrate_response`, `lowrate_to_48k`), `response.rs`,
`resample.rs`, `crates/secs-engine/src/rc.rs` (`truncate`: the half-Hann
taper over the last 10%), `crates/secs-engine/src/grid.rs`,
`crates/secs-filters/src/render.rs` (all of it) and its tests,
`crates/secs-graph/src/plan.rs` (`Op`, `FirView`, `op_response`).

## What it is for

Design stays at 48 kHz. When a DSP runs at another rate (44.1, 88.2, 96,
176.4 or 192 kHz), exported filters must be rebuilt for that rate: IIR
sections re-made at the new rate from the same parameters, delays rounded
to the new rate's samples, FIRs rebuilt so the response below 20 kHz is the
48 kHz design's. This packet writes those conversions and lets the
convolution renderer run at any output rate. At 48 kHz everything must stay
byte-identical to today.

## API (fixed)

New module `crates/secs-dsp/src/rate.rs`, exported from `secs-dsp`:

```rust
pub const DESIGN_RATE: u32 = 48_000;
pub const OUTPUT_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];
pub fn is_output_rate(rate: u32) -> bool;
/// Taps at `rate` for a filter `taps_48k` long at 48 kHz:
/// round(taps_48k × rate / 48000), at least 1 when taps_48k > 0.
pub fn taps_at(taps_48k: usize, rate: u32) -> usize;
/// A delay in samples at `rate`, rounding ties to even (as exporters do today at 48 kHz).
pub fn delay_samples(ms: f64, rate: u32) -> i64;
/// A 48 kHz FIR rebuilt for `rate` (ADR 0017). At 48 kHz returns the input unchanged.
pub fn fir_to_rate(taps_48k: &[f64], rate: u32) -> Result<Vec<f64>, DspError>;
/// A low-rate FIR (MIMO support) expanded straight to `rate`; equals
/// `lowrate_to_48k` bit for bit at 48 kHz.
pub fn lowrate_to_rate(taps_low: &[f64], fs_low: f64, rate: u32) -> Result<Vec<f64>, DspError>;
/// Exact DTFT of `taps` sampled at `rate`, at each frequency in Hz.
pub fn fir_response(taps: &[f64], rate: u32, frequencies: &[f64]) -> Vec<C>;
```

`fir_to_rate` rule: evaluate the 48 kHz FIR's response on the output rate's
FFT grid; up to 20 kHz use that response; above 20 kHz keep the magnitude
at 20 kHz and continue the phase linearly with the group delay at 20 kHz
(estimate it robustly, for example from the unwrapped phase slope over the
last few hundred Hz below 20 kHz); inverse FFT; keep the first
`taps_at(len, rate)` samples; apply the same half-Hann taper as
`rc::truncate` to the last 10%. Choose the FFT size so time aliasing is
negligible (state the choice). It must be exact enough for the tolerance
below, deterministic (same bits on every run and thread count) and fast
enough: a 16384-tap FIR to 192 kHz within 2 s in a release build on one
core; measure and report.

In `crates/secs-filters/src/render.rs` add

```rust
pub fn render_paths_at(plan: &FilterPlan, max_len_48k: usize, rate: u32) -> Result<RenderedPaths, FiltersError>;
```

and make `render_paths(plan, max_len)` call it with 48 kHz. At `rate` the
biquads are made with RBJ formulas at that rate (give `Biquad::peaking`,
`band`, `all_pass` a rate argument), delays use `delay_samples`, `Op::Fir`
goes through `fir_to_rate`, `Op::FirLowRate` through `lowrate_to_rate`, and
the length limit becomes `taps_at(max_len_48k, rate)`. `RenderedPaths` (or
each path) records its rate.

## Tolerance (the ADR's promise)

For filters as the engine designs them, the response at `rate` and the 48
kHz response agree within 0.1 dB and 1 degree from 20 Hz to 20 kHz, after
removing nothing (the time reference is the same). Prove it:

- `secs-dsp` tests: `fir_to_rate` for a linear-phase FIR, a minimum-phase
  FIR and a mixed-phase FIR of 2048 and 16384 taps, to every rate in
  `OUTPUT_RATES`; `lowrate_to_rate` for the MIMO low-rate recipe (70 taps at
  600 Hz plus its 31-tap low-pass, see `secs-engine/src/mimo`) to every rate;
  identity at 48 kHz (bit-equal); `taps_at` and `delay_samples` tables;
  determinism across two runs.
- `secs-filters` tests: render a real design's paths (use the plans the
  existing convolution and Equalizer APO tests build from oracle fixtures)
  at every output rate and compare each path's response with the 48 kHz
  render within the tolerance; `render_paths` stays byte-identical (the
  existing convolution tests must pass unchanged).
- A digital biquad at 48 kHz cramps near Nyquist, so a high-frequency PEQ
  may differ between 48 kHz and 96 kHz by more than 0.1 dB above about
  10 kHz. If a real design hits that, report where and by how much, and
  compare such sections against their analog prototype instead; do not
  loosen the tolerance silently.

Report the worst deviation per rate (dB, degrees, frequency) in a short
table.

## Files

You may edit: `crates/secs-dsp/src/**`, `crates/secs-dsp/tests/**`,
`crates/secs-filters/src/render.rs`, `crates/secs-filters/src/lib.rs` only
to re-export `render_paths_at`, `crates/secs-filters/tests/**`. Do not touch
exporters, `secs-types`, `secs-project`, `secs-service`, the screen, or
Cargo manifests unless a dev-dependency is needed (say which).

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-dsp -p secs-filters --all-targets --locked -- -D warnings
cargo test -p secs-dsp --locked
cargo test -p secs-filters --locked
cargo test -p secs-filters --release --locked rate -- --nocapture
```

## Report

The FFT size and group-delay estimate you chose, the timing, the deviation
table, files changed, and the commands' last lines.
