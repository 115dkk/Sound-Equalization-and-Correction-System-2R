# P05 (ASTRA): engine case set, measurement, settings normalisation, levels

Worktree: `/home/user/wt/p05` (branch `m2/p05`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents; do the
work yourself.

Read first: `docs/ENGINE.md` (the porting method for the whole engine; this
packet builds the foundation every later engine packet uses),
`docs/adr/0004-parity-and-deviations.md`, `oracle/README.md`,
`oracle/common.py`, and the engine skeleton in `crates/secs-engine/src/`
(`grid.rs`, `input.rs` hold the shared types; do not change their meaning,
additions are fine and must be reported).

## Goal

1. **The shared case set.** One deterministic set of engine inputs that every
   engine golden family uses, frozen once.
2. **The first stages ported:** settings normalisation (`setup.rs`),
   measurement preparation (`measurement.rs`), position weights, band level,
   target sampling and Schroeder frequency (`levels.rs`), gated on the oracle.

## Part 1: case set (`oracle/engine_cases.py`, family `engine_inputs`)

`oracle/engine_cases.py` is a library other exporters import:
`cases() -> list[Case]`, where a `Case` has `name`, `project` (an upstream
`secs.model` project), `irs` (`{position: {role: (ir_float32, 48000, t0)}}`
as `calibration.engine_inputs` expects), `settings`
(`calibration.CalibrationSettings`), and `engine()` returning the engine
`calibration.make_engine` builds for it (construct it exactly the way
`calibration.compute` does, `calibration.py:564–592`, without the GUI: the
oracle venv has no PyQt, so do not import `secs.wizard` modules that need it;
replicate what they do with `secs.model` and `secs.calibration`).

Required cases (name them as below; keep the set small, it is shared):

| Case | Data | Settings | Exercises |
|---|---|---|---|
| `dolby` | the tutorial demo room: `src/secs/rewapi/sample_dolby_714.npz` (14 real IRs, one position, `t0`), with the project the tutorial builds for it (see `src/secs/rewapi/fake.py`, `tools/make_sample_calibration.py`, the wizard's `load_sample`) | defaults (mode `mixed`) | the real demo |
| `dolby_zero`, `dolby_linear`, `dolby_ultra` | same | modes `zero`, `linear`, `ultra` (default `ultra_ms`) | RC branches (PEQ only, phase fraction 24, long FIR) |
| `dolby_infra` | same | the tutorial's infra quest state (SW3 as LP-only infra sub, see `tutorial_quests.py`) | two-way infra level match |
| `room` | synthetic, from `tests/synth.py` / `tests/room_model.py`: 4 positions, roles L, R, C, SW1, SW2; nonzero `t0` on some IRs; room volume set | defaults | multi-position, BA with DE, Schroeder |
| `room_dba` | synthetic, 3+ positions, 4 subs (2 front, 2 rear) in bundle A, layout `dba` | DBA rows, a rear trim | DBA layout |
| `room_multi` | as `room_dba` | layout `multi` | multi-sub layout |
| `room_manual` | as `room` plus SW3 | SW1 fixed, SW2/SW3 bundle `values`, `sub_main_delay_ms` > 0 | manual BA |
| `room_filters` | as `room` | fixed HP/LP/PEQ on the subs (different band types), XO PEQ band set by hand, boost limit lowered, `infra_boost_max_db` lowered | fixed sub filters, XO PEQ, limits |

Synthetic IRs: 1.0 s at 48 kHz, cast to float32 before the oracle sees them
(production measurements are float32). Reuse the same synthetic room for
cases that differ only in settings (content addressing then stores the IRs
once). Seeds fixed.

`oracle/export_engine_inputs.py` writes one fixture per case:

- `inputs.measurements`: positions ascending, roles in the dictionary order
  the engine iterates, each `{"sample_rate", "t0", "samples": blob}` with the
  samples as little-endian float32 bytes (`Family.blob(..., "f32")`).
- `inputs.config`: the arguments `make_engine` passes to
  `CorrectionEngine.__init__` and the attributes it writes after
  construction (`max_boost_db`, `infra_boost_max_db`, `xo_peq_on`,
  `xo_peq_lo`, `xo_peq_hi`), recorded by interposing on `__init__` inside a
  real `make_engine` call, so they are exactly what upstream used. Name the
  keys after `secs_engine::EngineConfig` fields.
- `inputs.settings`: `dataclasses.asdict(settings)` and the project summary
  later packets need (groups, roles, project XO), for the orchestrator port.

Budget: `engine_inputs` at most 9 MB.

## Part 2: stages (`oracle/export_engine_prep.py`, family `engine_prep`)

Per case, record (by calling the real methods on the real engine):

- after `__init__`: `sub_manual` (normalised), `sub_filters`, `layouts`,
  `lift_ms`, `lift_note` (present or not), `max_latency_ms`, `rc_lo`,
  `rc_hi_manual`, `rc_hi`, `peq_bands`;
- `onset_index` and the `windowed` start index of every IR;
- after `_prepare`: `arrival`, `align_delay_ms`, the spectral origin `base`,
  and `H` / `H0` sampled (below);
- `long_spectra(2000)` and `long_spectra(500)` sampled;
- `_weights()`, `_freq_weights()` sampled, `_band_level(role, 300, 3000)` for
  every main, the reference level (their minimum);
- `schroeder_hz()`;
- the target sampled on the design grid (sampled) and on a log grid.

Sampling full-grid arrays: all bins in 15–300 Hz plus every 257th bin of the
whole grid, as complex values; record the bin indices. `H0` only where it
differs from `H` (subs with fixed filters). Budget: `engine_prep` at most
3 MB.

## Part 3: Rust

Upstream: `src/secs/correction_engine.py` lines 105–162 (`onset_index`,
`windowed`, `spectrum`, `freqs` already exists as `grid::design_freqs`),
418–489 (`__init__` normalisation), 491–561 (`_prepare`,
`sub_filter_response`, `_apply_sub_filters`, `long_spectra`), 589–608
(`_weights`, `_freq_weights`, `_delay`, `_band_level`), 1318–1339
(`schroeder_hz`); `src/secs/calibration.py` 505–518 (`_TargetFn`).
Use `secs_dsp` primitives (`fft::rfft`, `response::band_filter`,
`response::peq_response`, `iir::butter`, `iir::sosfilt`) and
`secs_types::target::TargetCurve::sample`; do not re-implement them.

These items are the contract later packets build on. Keep the names; the
bodies, private helpers and extra fields are yours.

```rust
// setup.rs
pub struct SubFilter { pub hp_hz: f64, pub hp_type: BandType, pub lp_hz: f64,
                       pub lp_type: BandType, pub peq: Vec<Peq> }
pub struct Layout { pub kind: LayoutKind, pub rear_trim_ms: f64, pub members: Vec<String> }
pub struct Setup {
    pub sub_manual: Vec<(String, SubManual)>,  // upstream dict order
    pub sub_filters: Vec<(String, SubFilter)>, // upstream dict order
    pub layouts: Vec<(String, Layout)>,
    pub lift_ms: f64,                 // after the FIR-length cap
    pub lift_capped_from_ms: Option<f64>,
    pub max_latency_ms: f64,
    pub rc_lo: f64, pub rc_hi_manual: Option<f64>, pub rc_hi: f64,
    pub peq_bands: usize,
}
impl Setup { pub fn new(config: &EngineConfig) -> Setup }

// measurement.rs
pub const WINDOW_MS: f64 = 500.0;
pub fn onset_index(ir: &Samples) -> usize;
pub fn windowed(ir: &Samples, start: usize) -> (Vec<f64>, usize);
pub fn spectrum(x: &[f64]) -> Vec<C>;               // rfft, zero-padded to N_FFT
pub fn delay_response(ms: f64) -> Vec<C>;          // `_delay` on the design grid
pub fn sub_filter_response(filter: &SubFilter) -> Vec<C>;
/// Spectra by (position, role); cells shared with `Arc` like upstream's aliases.
pub struct Spectra { /* yours */ }
impl Spectra {
    pub fn positions(&self) -> &[u32];
    pub fn get(&self, position: u32, role: &str) -> Option<&Arc<[C]>>;
    pub fn roles_at(&self, position: u32) -> impl Iterator<Item = &str>; // upstream order
    pub fn set(&mut self, position: u32, role: &str, value: Arc<[C]>);
}
pub struct Prepared {
    pub arrival_s: Vec<(String, f64)>,      // main-position roles, upstream order
    pub align_delay_ms: Vec<(String, f64)>,
    pub base_s: f64,
    pub h: Spectra,
    pub h0: Spectra,
}
pub fn prepare(set: &MeasurementSet, config: &EngineConfig, setup: &Setup)
    -> Result<Prepared, EngineError>;
pub fn long_spectra(set: &MeasurementSet, prepared: &Prepared,
                    sub_filters: &[(String, SubFilter)], window_ms: f64)
    -> Result<Spectra, EngineError>;

// levels.rs
pub const MAIN_WEIGHT: f64 = 5.0;
pub fn weights(positions: &[u32], main_position: u32) -> Vec<f64>;  // aligned with positions
pub fn freq_weights(positions: &[u32], main_position: u32, main_weight_lo: f64)
    -> Vec<Arc<[f64]>>;                                              // aligned with positions
pub fn target_db(target: &TargetCurve, f: &[f64]) -> Vec<f64>;
pub fn band_level(h: &Spectra, role: &str, weights: &[f64], target: &TargetCurve,
                  lo: f64, hi: f64) -> f64;
pub fn schroeder_hz(set: &MeasurementSet, config: &EngineConfig) -> Option<f64>;
```

`long_spectra` takes the sub filters explicitly because the layout stage (P10)
appends bundle PEQ bands to them after `prepare`. Errors: an IR at any
position whose sample rate is not 48 kHz is `EngineError::Input` (upstream
checks only the main position: that is deviation D2; record it in
`docs/DEVIATIONS.md` as applied, with your test). A missing main position or
no measured main is `EngineError::Input`. Never panic on input data.

Memory: a full spectrum is 1 MiB. Do not copy spectra that upstream aliases
(`H0` shares `H` cells except subs with fixed filters).

## Part 4: tests

In `crates/secs-engine/src/tests/`: `cases.rs` (the loader every later family
uses: `pub(crate) fn load(case: &str) -> Case` with the `MeasurementSet`, the
`EngineConfig`, `Tuning::default()` and the raw settings JSON; plus
`pub(crate) fn names() -> Vec<String>`), and `prep.rs` with the stage tests.
Register `mod prep;` in `src/tests/mod.rs`.

Budgets: integers exact (onset, window start, bin indices); `arrival`,
`align_delay_ms`, levels, weights, Schroeder, target with `assert_close` at
`PLATFORM_RTOL`; spectra with a measured bound printed as a `MEASURE` line
(ADR 0004: 1.5 × the largest error you measure, with a floor of 1e-12
relative to the largest magnitude of that spectrum).

## Files you may change

- `oracle/engine_cases.py`, `oracle/export_engine_inputs.py`,
  `oracle/export_engine_prep.py` (new), `oracle/goldens/engine_inputs/**`,
  `oracle/goldens/engine_prep/**` (generated), `oracle/README.md` (family
  rows; document the case set in a short table)
- `crates/secs-engine/src/{setup,measurement,levels}.rs`,
  `crates/secs-engine/src/tests/{mod,cases,prep}.rs`,
  `crates/secs-engine/src/input.rs` (additions only), `crates/secs-engine/Cargo.toml`
- `features.toml`: set `engine.levels_delays` to `implemented` with your tests
- `docs/DEVIATIONS.md`: D2 applied
- `Cargo.lock`, only as cargo rewrites it

Other workers hold `crates/secs-compute/**` (P06). Do not touch
`crates/secs-dsp/**`; if a primitive is missing or wrong, report it.

## Verification (foreground; finish before you report)

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

```sh
cd /home/user/wt/p05
export CARGO_INCREMENTAL=0
<venv>/bin/python oracle/run.py engine_inputs engine_prep
du -sh oracle/goldens/engine_inputs oracle/goldens/engine_prep
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine -p secs-oracle -p secs-policy -- --nocapture 2>&1 | grep -E "MEASURE|test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

Files changed; the case table as built (data, positions, roles, settings);
golden sizes; verification output including the `MEASURE` lines; any upstream
behaviour you found that later packets must know (ordering, aliasing, edge
cases); anything not matched and why.
