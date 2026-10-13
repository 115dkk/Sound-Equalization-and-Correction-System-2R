# P10 (ASTRA): bass alignment

Worktree: `/home/user/wt/p10` (branch `m2/p10`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents.

Read first: `docs/ENGINE.md`, `docs/adr/0004-parity-and-deviations.md`
(D1 is a planned deviation of `_bus_delay`: port the upstream objective
here, unchanged), `oracle/README.md`, M1's DE gate
(`crates/secs-dsp/tests/golden_de.rs`, `oracle/export_dsp_de.py`), and what
has landed: case set and loader, `measurement.rs`, `levels.rs`, `setup.rs`
(P05), `fit.rs` with `xo_target_peq` and `replay_fitter` (P07), `decay.rs`
with `BaLateGrid` / `ba_late_db` (P08), `chain.rs` with `term` / `sub_sum`
and `BaRow`, `secs-compute` (`CancelToken`). All read-only for you.

**Base and build.** Your worktree starts from `m2/p07`: P07's `fit.rs`
(`fit_peq`, `xo_target_peq`, `Fitter`, `replay_fitter`, the recorded-fit
reader in `src/tests/fit.rs`) is there; its own oracle gate is being
finalised in P07c, but the API will not change. Use the shared build
directory to save disk: `export CARGO_TARGET_DIR=/home/user/wt/target-shared`
in every shell (another worker builds there too; cargo waits for its lock,
which is fine). Do not delete anything outside your worktree.

## Goal

Port the whole BA stage of `compute()` (lines 1371–1428 except the
mains' acoustic-XO PEQ, which P12 sequences between `align` and
`bus_delay`): bundle layouts,
automatic and manual DE searches, the two-way infra level match, sub sums,
bus level and gain, bus PEQ (and the optional bus-aware second pass), bus
delay and polarity, the fold of bus values into the rows, and the sub mode
labels.

Upstream, `src/secs/correction_engine.py`: `two_way_split` 624–634,
`_match_two_way` 636–670, `_ba_setup` 672–695, `_ba_cost` 697–726,
`_align_subs` 728–755, `_bus_peq_of` 758–775, `_bus_peq_fit` 777–783,
`_ba_bus_pass` 785–808, `manual_value` 810–814, `manual_plan` 816–833,
`sub_mode` 835–849, `_layout_stage` 852–958 (with its own `late_db`
closure, which is not P08's), `_align_subs_manual` 960–1060, `_bus_delay`
1063–1095, `_term` 358–363; the BA part of `compute()` 1371–1428.
Constants 49–93.

**Approved contract changes (Claude, after your first run).** Add
`pub measurements: &'a MeasurementSet` to `BaInput`: the DBA search bound
uses every position's raw onsets and `t0` (`correction_engine.py:885–910`),
which `Prepared` does not keep. You found that `room_dba`'s bound is 2.75 ms
from all positions against 1.50 ms from the reference only; test that
case. For any further *additive* change to the `ba.rs` contract that
upstream parity needs, make it and list it in your report; do not stop to
ask. Stop and ask only if a landed module (`measurement.rs`, `fit.rs`,
`decay.rs`, `chain.rs`) would have to change.

## Contract (`crates/secs-engine/src/ba.rs`)

```rust
pub struct BaInput<'a> {
    pub prepared: &'a Prepared,      // H (subs), alignment delays
    pub setup: &'a Setup,
    pub config: &'a EngineConfig,
    pub tuning: &'a Tuning,
    pub xo_of: &'a [(String, f64)],  // per main role, compute 1364–1370
    pub ref_level: f64,              // compute 1373
    pub cancel: &'a CancelToken,
}
/// Steps up to the bus PEQ: everything before the acoustic-XO PEQ of the mains.
pub struct SubAlignment {
    pub rows: Vec<BaRow>,                     // self.subs order, before the bus fold
    pub sub_filters: Vec<(String, SubFilter)>,// with generated bundle PEQ appended
    pub h_subs: Vec<(u32, String, Arc<[C]>)>, // sub spectra the layout stage replaced
    pub layout_rel: Vec<(String, Vec<(String, BaRow)>)>,
    pub layout_info: Vec<(String, LayoutInfo)>,
    pub sub_level_db: Option<f64>,
    pub bus_gain: f64,
    pub bus_peq: Vec<Peq>,
    pub sub_sum_by_pos: Vec<(u32, Arc<[C]>)>, // after bus gain and bus PEQ
    pub plan: Option<ManualPlan>,
    pub notes: Vec<Note>,
}
pub fn align(input: &BaInput<'_>, fitter: &mut Fitter<'_>) -> Result<SubAlignment, EngineError>;
/// `_bus_delay`: `mains` holds each main's spectra (with its acoustic-XO PEQ) by position.
pub fn bus_delay(alignment: &SubAlignment, mains: &[(String, Vec<(u32, Arc<[C]>)>)],
                 align_delay_ms: &[(String, f64)], xo_of: &[(String, f64)], lo_ms: f64) -> (f64, i8);
/// Folds bus gain, delay and polarity into the rows (compute 1426).
pub fn fold(rows: &[BaRow], bus_gain: f64, bus_delay_ms: f64, bus_polarity: i8) -> Vec<BaRow>;
pub fn sub_mode_label(role: &str, setup: &Setup, layout_info: &[(String, LayoutInfo)]) -> String;
```

`LayoutInfo`, `ManualPlan` are yours (mirror upstream's dict fields). Return
`EngineError::Cancelled` when the token is cancelled; check it in the DE cost
closure (return a large finite cost once cancelled, so DE ends quickly) and
between stages.

## The DE gate

`secs_dsp::optimize::differential_evolution` reproduces SciPy's DE bit for
bit when the cost is bit-identical (M1). The BA cost involves `np.polyfit`
(degree 2: scaled Vandermonde and LAPACK `lstsq`), unwrap, gradient, median,
variance; a Rust port can differ in the last bits (SVD), and a DE comparison
of two costs within that noise can flip. So:

1. Gate `ba_cost` itself at recorded parameter vectors (every evaluation
   of the first two generations and of the last, from the oracle's trace):
   relative error ≤ a measured bound (`MEASURE`).
2. Replay the DE on the oracle path: each generation's trial vectors from the
   oracle's population; the Rust cost of each trial; the selection decision
   must equal the oracle's wherever the two costs differ by more than the
   measured bound; count and print near-ties.
3. End to end (Rust DE from the start): on the oracle libm, report whether the
   whole evaluation sequence and the result equal the oracle's; if they do,
   assert it, otherwise print the first diverging evaluation and the final
   rows' difference (report-only, like capped TRF fits).

Implement `np.polyfit(x, y, 2)` faithfully (NumPy 2.5.3: `vander`, column
scaling, `lstsq` with `rcond = len(x) * eps`); the closer the arithmetic, the
fewer near-ties.

## Goldens (`oracle/export_engine_ba.py`, family `engine_ba`)

Cases: `room` (2 auto subs), `dolby` (3 auto subs), `dolby_infra` (two-way),
`room_dba`, `room_multi`, `room_manual` (fixed and value bundles,
`sub_main_delay_ms`), `room_filters` (fixed filters), and `room` with
`sub_allpass='full'`, with `bundle_gain='fixed'`, and with `SUB_BUS_FIX` on
(module attribute) for the second pass. Run `calibration.compute` with
`subs_only=False` and interpose on the methods above. Record: the layout
stage (member levels, the DBA search's chosen delay and cost per candidate
as a digest plus the winner, `layout_rel`, `layout_info`, appended PEQ via
the recorded-fit form), `_ba_setup` (grid indices, positions, `lin`,
`raw_late`), the DE trace (every evaluated vector and cost in order; as a
digest above 4096 values with the first two and last generations in full),
DE results, the manual plan and unpacked rows, `_match_two_way` iterations,
sub level, bus gain, bus PEQ fits (recorded-fit form), `_bus_delay` inputs
(recipe) and its candidate costs (digest) and answer, folded rows, labels.
Full-grid sums as recipes, never in full.

Budget `engine_ba` at most 6 MB.

## Tests (`crates/secs-engine/src/tests/ba.rs`)

Exact: grid indices, positions, plan, labels, layout membership and DBA
winner index, bus polarity, bus delay (a grid value), two-way iteration
count. Measured tolerances (`MEASURE`): costs, levels, gains, rows (when
the DE path matched), sums (sampled). Report-only off the oracle libm where
a decision could flip; say which.

## Files you may change

- `crates/secs-engine/src/ba.rs`, `crates/secs-engine/src/notes.rs`
  (create it if missing; additions), `crates/secs-engine/src/lib.rs` (module
  lines), `crates/secs-engine/Cargo.toml` (add `secs-compute` for
  `CancelToken`), `crates/secs-engine/src/tests/{mod,ba}.rs`
- `oracle/export_engine_ba.py` (new), `oracle/goldens/engine_ba/**`,
  `oracle/README.md` (family row)
- `features.toml`: set `engine.ba_auto`, `engine.ba_manual_bundles`,
  `engine.ba_bus` to `implemented` with your tests (keep `engine.ba_bus`'s
  D1 note: the deviation comes later)
- `Cargo.lock`, only as cargo rewrites it

Others hold `rc.rs` (P09), `secs-camilla` (P11). If `chain::sub_sum` or a
landed module is wrong, report it with the failing case instead of editing it.

## Verification (foreground; finish before you report)

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

```sh
cd /home/user/wt/p10
export CARGO_INCREMENTAL=0
<venv>/bin/python oracle/run.py engine_ba
du -sh oracle/goldens/engine_ba
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine -- --nocapture 2>&1 | grep -E "MEASURE|PLATFORM|test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

Files changed; golden size; verification output with `MEASURE` lines; per
case: whether the DE path matched end to end, near-ties counted, final rows
vs oracle; anything not matched and why; upstream behaviour that looks like a
defect (for the ledger, do not fix it).
