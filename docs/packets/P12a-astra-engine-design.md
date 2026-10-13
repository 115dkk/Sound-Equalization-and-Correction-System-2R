# P12a (ASTRA): the design orchestrator, the chain simulator, and the end-to-end gate

Worktree: `/home/user/wt/p12a` (branch `m2/p12a`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Use the
shared build directory: `export CARGO_TARGET_DIR=/home/user/wt/target-shared`
in every shell (another worker may build there; cargo waits for its lock).
Remove nothing outside your worktree.

Read first: `docs/ENGINE.md` (all of it: the two time axes in §2 and the
golden rules in §3 decide most questions here), `docs/adr/0004-parity-and-deviations.md`,
`oracle/README.md`, and what has landed, all read-only for you:
`setup.rs`, `measurement.rs`, `levels.rs` (P05), `secs-compute` (P06),
`fit.rs` with `replay_fitter` (P07), `decay.rs`, `scorecard.rs` (P08),
`rc.rs` (P09), `ba.rs` and `notes.rs` (P10, P09), `calibration.rs` (P12c),
`chain.rs` (`BaRow`, `term`, `sub_sum`), `secs-types::chain`.

## Goal

One call from measurements and settings to an immutable design:
`calibration.compute` around `CorrectionEngine.compute`, with the stages the
other packets ported. Plus the response simulator the screens and exports use
instead of reading engine internals, and the end-to-end gate that closes M2:
the demo room's result lies within SECS 2's own spread.

Upstream:

- `src/secs/correction_engine.py`: `compute` 1350–1499 (the whole method:
  sub-filter note, RC upper limit from `schroeder_hz`, `xo_of`, levels and
  `ref_level`, `manual_plan`, `_bus_ctx`, BA, sub sums, sub level and bus
  gain, bus PEQ, acoustic-XO top PEQ with `xo_peq_band`, bus delay and its
  lower bound, the fold, the per-main loop that `rc::channel` already ports,
  the shift / `main_extra_ms` / latency, `SubAlign` assembly with fixed subs'
  `manual_value` and sub filters, the target, the notes); `xo_peq_band`
  1291–1299; `_step` 1341–1348 (progress = 1 + mains);
  `stage_responses` 563–584, `input_response` 586–587; the result types
  368–408.
- `src/secs/calibration.py`: `compute` 584–611 (metrics and curves per role,
  `rc_auto`, `schroeder_hz`, `target` on `SPL_GRID`, `rc_note`),
  `spl_db` 522–526, `gd_ms` 528–531, `SPL_GRID`, `GD_GRID`, `DISPLAY_FRAC`
  (64–66), `CalibrationRun.filters` 556–561.

## Contract

```rust
// crates/secs-engine/src/design.rs (private module like the other stages;
// `pub` items, `#[allow(dead_code)]` on the module line until the service uses it)

/// Which stage a `fit_peq` call belongs to, so tests can replay per stage and
/// production can run channels in parallel, each with its own fitter.
pub enum FitStage<'a> { Ba, Bus, Top(&'a str), Rc(&'a str) }
pub type FitterFactory<'f> = dyn Fn(FitStage<'_>) -> Box<Fitter<'f>> + Sync + 'f;
pub fn production_fitters() -> Box<FitterFactory<'static>>;   // fit_peq everywhere

pub struct DesignRequest<'a> {
    pub measurements: &'a MeasurementSet,
    pub config: &'a EngineConfig,
    pub tuning: &'a Tuning,
    pub subs_only: bool,
    pub adapter_notes: &'a [String],      // EngineInputs::notes (P12c)
}
pub fn design(req: &DesignRequest<'_>, ctx: &Context, fitters: &FitterFactory<'_>)
    -> Result<Design, EngineError>;

/// Everything `CalibrationRun` and `CorrectionResult` carried that a screen,
/// an export or a later stage reads. No engine object survives the call.
pub struct Design {
    pub chain: SignalChain,           // output axis: what DSP exports read (secs-types)
    pub mains: Vec<MainDesign>,       // role order; mains measured at the reference position
    pub subs: Option<SubDesign>,      // None without subs
    pub ref_level: f64,
    pub rc_hi: f64,
    pub rc_auto: bool,
    pub schroeder_hz: Option<f64>,
    pub target_db: Arc<[f64]>,        // design grid, `result.target_db`
    pub target_display: Vec<f64>,     // SPL_GRID, `CalibrationRun.target`
    pub spectra: DesignSpectra,       // what the simulator needs, shared, not copied
    pub notes: Vec<Note>,             // engine notes in upstream order, then the two summary notes
    pub adapter_notes: Vec<String>,
    pub scores: Vec<RoleScores>,      // role order
}
pub struct DesignSpectra {
    pub raw: Arc<Spectra>,            // `engine.H0`
    pub used: Arc<Spectra>,           // `engine.H` after the layout stage and the top PEQ
    pub main_position: u32,
    pub positions: Vec<u32>,
}
pub struct MainDesign {
    pub role: String,
    pub gain_db: f64,
    pub xo_hz: Option<f64>,
    pub align_delay_ms: f64,          // internal axis
    pub top_response: Option<Arc<[C]>>, // `chain['top_P']`
    pub response: Arc<[C]>,           // `chain['F']`, with GD cuts
    pub phase_strength: f64,
    pub phase_band: Option<f64>,
    pub phase_frac_used: Option<f64>,
    pub before: Arc<[C]>,             // `result.before`
    pub combined: Arc<[C]>,           // `result.combined`
    pub after: Arc<[C]>,              // `result.after`
}
pub struct SubDesign {
    pub rows: Vec<(String, BaRow)>,   // `sub_state['params']`: folded, internal axis
    pub bus_gain: f64,
    pub bus_delay_ms: f64,
    pub bus_polarity: i8,
    pub bus_response: Option<Arc<[C]>>, // `sub_state['bus_P']`
    pub sum_at_reference: Arc<[C]>,   // `result.sub_sum`
    pub sub_level_db: Option<f64>,
    pub layout_info: Vec<(String, LayoutInfo)>,
    pub plan: Option<ManualPlan>,
}
pub struct RoleScores {
    pub role: String,
    pub before: Metrics,
    pub after: Metrics,
    pub spl_before: Vec<f64>, pub spl_after: Vec<f64>,   // SPL_GRID
    pub gd_before: Vec<f64>, pub gd_after: Vec<f64>,     // GD_GRID
}

// crates/secs-engine/src/chain.rs (additions)
pub struct StageResponses { pub top_xo: Vec<C>, pub sub_xo: Option<Vec<C>>,
                            pub combined: Vec<C>, pub output: Vec<C> }  // upstream 'final'
pub fn stage_responses(design: &Design, role: &str, spectra: &Spectra, position: u32)
    -> Option<StageResponses>;
pub fn input_response(design: &Design, role: &str, spectra: &Spectra, position: u32)
    -> Option<Vec<C>>;
```

`SignalChain` holds what `CorrectionResult.channels`, `.subs`, `.sub_bus_*`,
`.lift_ms` and `latency_ms` / `main_extra_ms` hold, on the output axis, with
the sub filters filled in exactly as `compute` 1493–1495 does. The per-main
RC runs through `ctx.try_map` over roles (order-preserving), each channel
with `fitters(FitStage::Rc(role))`; gather notes in role order afterwards so
the note list equals upstream's serial order. Report progress through
`StageProgress` with upstream's count (1 + mains). Check cancellation between
stages and pass the token to BA; a cancelled design returns
`EngineError::Cancelled` within a second (test it on `dolby`).

Metrics, curves and the display target follow `calibration.compute`
exactly (`scorecard::metrics` with the normalisation upstream passes;
`spl_db` and `gd_ms` on their grids).

Put any helper the other packets left private behind `pub(crate)` only if
you must, and list it in your report; do not change their logic.

## Goldens

### `engine_design` (`oracle/export_engine_design.py`): the orchestrator on recorded decisions

For every case in `engine_cases` (all ten), run `calibration.compute` with
interposition:

- Tag each `fit_peq` call with its stage by wrapping the stage entry points
  (`_layout_stage`, `_align_subs`, `_align_subs_manual`, `_match_two_way`
  → `Ba`; `_bus_peq_fit` → `Bus`; `_xo_target_peq` → `Top(role)`, the role
  from the per-role loop; `_correction` → `Rc(role)`) and record each
  stage's calls in the recorded-fit form (P07), in call order.
- Record the result: every `ChannelOut` field (FIR digest plus samples),
  every `SubAlign` field, bus delay and polarity, lift, latency,
  `main_extra_ms`, `ref_level`, `sub_level_db`, the notes as strings and as
  their numbers, `before` / `combined` / `after` / `sub_sum` / `target_db`
  sampled (all bins 15–600 Hz plus every 257th), `chain[role]['F']` and
  `top_P` sampled the same way, `phase_frac_used`, `rc_hi`, `rc_auto`,
  `schroeder_hz`, `rc_note`; per role the 17 metrics before and after and
  the four curves in full.
- For `stage_responses`: on `dolby` and `room_dba`, call
  `engine.stage_responses(role, engine.long_spectra(window_ms=500.0), p)` for
  two roles and every position and record the four responses sampled.

Budget: 6 MB; bundle with `Fixture::calls` / `all_calls`.

Test (`src/tests/design.rs`): run `design` with a factory that hands each
stage its recorded calls through `replay_fitter`. Exact: roles, XO, counts,
polarities, modes, the chosen phase fraction, note kinds and their discrete
fields, `rc_auto`. Within measured tolerances (`MEASURE`, 1.5 × measured,
floor `PLATFORM_RTOL`) on the oracle libm: everything continuous. Off the
oracle libm, assert what does not depend on a threshold that last bits can
flip; report the rest and say which.

BA inside `design` runs for real. P10 found the DE evaluation sequence and
result equal to the oracle's on every engine case except `room`, where it
diverges at evaluation 3713 after 41 near-ties (cost differences within the
measured 1e-13 objective budget). So on `room` assert only what precedes the
BA result and report the rest; that case's end result is gated by
`engine_e2e`. Do not inject BA.

### `engine_e2e` (`oracle/export_engine_e2e.py`): SECS 2's own spread

The M2 exit gate (ADR 0004: an end result lies within 1.25 × SECS 2's own
spread). Cases `dolby` (the demo room), `room`, `room_dba`. For each, run
upstream `calibration.compute` in eight configurations, each a fresh
subprocess of the oracle venv, one subprocess per configuration running all
three cases:

1. the reference environment (already the oracle's),
2. `OPENBLAS_CORETYPE=Haswell`,
3. `OPENBLAS_CORETYPE=SkylakeX` (or the highest supported; record the core),
4. NumPy SIMD dispatch on (skip `common.check_libm` there, as P07b did),
5.–8. the reference environment with every IR sample moved one float32 ulp
   (`np.nextafter` in float32 towards ±inf by a seeded random sign, seeds
   0–3; exact zeros untouched).

Run at most two subprocesses at a time (other workers share four cores).
Record per run: per role the 17 metrics before and after, gain, output
delay, XO, PEQ band count, FIR length; per sub gain, delay, polarity, mode;
bus delay, bus polarity, latency, `main_extra_ms`, `ref_level`, `rc_hi`;
elapsed seconds.

Test (`src/tests/e2e.rs`): run `design` with `production_fitters()` from the
raw IRs. Per value, with `c0` the reference and `w = cmax − cmin` the range
of the eight runs (SECS 2's own spread; P07 settled this reading of ADR 0004:
Rust is one more draw from the same distribution, so its distance from the
reference is bounded by the spread, not by how far the other runs happen to
sit above the reference):

- metrics where higher is better (`in*`): Rust ≥ `c0 − 1.25·w`;
  where lower is better (`turns`, `gd_*`, `gd24_abs`, `gd24_max`,
  `gd24_span`, `pre_db`): Rust ≤ `c0 + 1.25·w`; neutral descriptors
  (`level`, `gd24_abs_hz`) and chain scalars: `|Rust − c0| ≤ 1.25·w`. Read
  upstream `scorecard.py` to confirm each direction before you fix it; if one
  is not obvious, make it two-sided and say so.
- Print, per case, on how many values Rust lies outside `[cmin, cmax]`
  (exchangeable draws: about 2/9 of them).
- Every bound has an absolute floor `1e-9·max(1, |c0|)` so a zero spread
  does not demand bits.
- Discrete values (polarities, modes, band counts, XO) must equal one of the
  eight runs' values.
- `before` metrics do not involve a fit: assert them within the measured
  tolerance on the oracle libm.

Print one table per case (value, Rust, c0, envelope, which run set the
bound, pass) and the Rust and upstream elapsed seconds. Register
`engine.e2e_demo` (kind `"engine"`, milestone `"M2"`, `upstream =
"calibration.compute on the demo room"`) as `implemented` only if every
value passes. If some fail, do not widen anything: report them with all
eight values. Budget: 1 MB.

These runs are slow in Python; mark the Rust e2e test `#[ignore]` only if
it takes more than 60 s in debug, and then add it to the `release` profile
test run instead (say so; Claude wires CI).

## Simulator test

`stage_responses` against the recorded responses (on the replayed design):
exact on structure, measured tolerance on values.

## Files you may change

- `crates/secs-engine/src/{design,chain}.rs`, `crates/secs-engine/src/lib.rs`
  (module lines), `crates/secs-engine/src/notes.rs` (new variants only),
  `crates/secs-engine/src/tests/{mod,design,e2e}.rs`
- `oracle/export_engine_design.py`, `oracle/export_engine_e2e.py`,
  `oracle/goldens/engine_{design,e2e}/**`, `oracle/README.md` (family rows)
- `features.toml`: `engine.design` (`implemented` with the design tests),
  `engine.e2e_demo` (see above), `engine.simulator`
- `Cargo.lock`, only as cargo rewrites it

## Verification (foreground; finish before you report)

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

```sh
cd /home/user/wt/p12a
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-shared
<venv>/bin/python oracle/run.py engine_design engine_e2e
du -sh --apparent-size oracle/goldens/engine_design oracle/goldens/engine_e2e
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine -- --nocapture 2>&1 | grep -E "MEASURE|E2E|test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine design e2e 2>&1 | grep -E "test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

Files changed; golden sizes; verification output with `MEASURE` lines; the
e2e tables; Rust and Python elapsed time per case; anything not matched and
why; anything upstream does that looks like a defect (for the deviation
ledger; do not fix it).
