# P12b (ASTRA): XO recommendation, XO and sub-combine previews

Worktree: `/home/user/wt/p12b` (branch `m2/p12b`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_TARGET_DIR=/home/user/wt/target-p12b` (yours alone).
Another worker (P12d, `secs-export`) runs at the same time in another
worktree: do not touch `crates/secs-export`, `crates/secs-mdat` or
`crates/secs-types`.

Read first: `docs/ENGINE.md`, `docs/adr/0004-parity-and-deviations.md`
(including "끝단 판정의 통계 규칙"), `oracle/README.md`, and what has landed,
all read-only for you except where listed below: `design.rs` (`design`,
`Design`, `production_fitters`), `chain.rs` (`stage_responses`, `term`,
`sub_sum`), `measurement.rs` (`long_spectra`), `scorecard.rs`, `decay.rs`
(`late_db_20_150`), `calibration.rs` (`engine_inputs`), the replayed-design
test and its fixtures (`src/tests/design.rs`, family `engine_design`).

## Goal

The screen's XO and BA tabs and the XO recommendation button, computed from
a `Design` and the measurements, never from engine internals.

Upstream `src/secs/calibration.py`:

- `XO_CANDIDATES` 61, `XO_GRID`, `XO_FRAC`, `XO_GD_W`, `XO_LATE_W` 631–636,
  `xo_groups` 639–644, `xo_scores` 662–688, `pick_xo` 698–706, `_xo_trial`
  709–715, `recommend_xo` 718–770 (candidates are independent: run them with
  `ctx.try_map`, keep the table in candidate order, report progress per
  finished candidate; no process pool), `xo_preview` 773–822 (its cache:
  build once per design, then a call with a new `fc` only multiplies HP/LP and
  smooths; upstream promises under 0.2 s, requirement R7), `_sub_out`
  833–840 (`chain::term` already is this), `sub_band` 843–846, `SUB_GRID`,
  `SUB_METRIC_FRAC`, `SUB_RAW` 828–830, `sub_combine` 849–909 (without its
  `layouts` text, which P12d owns; return the layout info values instead).

## Contract

```rust
// crates/secs-engine/src/xo.rs (the skeleton module exists; fill it)
pub const XO_CANDIDATES: [f64; 4] = [60.0, 80.0, 100.0, 120.0];
pub struct XoScores { pub flat: f64, pub gd: f64, pub late: f64 }
pub struct XoRow { pub fc: f64, pub scores: XoScores, pub score: f64 }
pub struct XoRecommendation {
    pub candidates: Vec<f64>,
    pub table: Vec<(String, Vec<XoRow>)>,   // group order, rows in candidate order
    pub best: Vec<(String, f64)>,
}
pub fn xo_groups(config: &EngineConfig) -> Vec<String>;
pub fn xo_scores(design: &Design, measurements: &MeasurementSet, config: &EngineConfig)
    -> Vec<(String, XoScores)>;
pub fn pick_xo(table: &mut [(String, Vec<XoRow>)]) -> Vec<(String, f64)>;
pub fn recommend_xo(view: &ProjectView<'_>, irs: &MeasurementSet, settings: &CalibrationSettings,
                    tuning: &Tuning, candidates: &[f64], ctx: &Context)
    -> Result<XoRecommendation, EngineError>;

// crates/secs-engine/src/preview.rs (new)
pub struct XoCurves { pub grid: Vec<f64>, pub main: Vec<f64>, pub sub: Vec<f64>,
                      pub sum: Vec<f64>, pub target: Vec<f64> }
pub struct XoPreview { /* the cache */ }
impl XoPreview {
    pub fn new(design: &Design) -> Self;
    pub fn curves(&self, design: &Design, group: &str, fc: f64, corrected: bool,
                  role: Option<&str>) -> XoCurves;
}
pub struct SubCombine { /* every key upstream returns except 'layouts', typed */ }
pub fn sub_combine(design: &Design) -> Option<SubCombine>;
```

Make `xo` and `preview` public modules of `secs-engine` (lib.rs module lines
only), with `#![…]`-free docs on every public item.

## Goldens

`oracle/export_engine_xo.py`, family `engine_xo`, budget 3 MB:

- `xo_scores(run)` for every `engine_design` case with subs (the replayed
  design is the input; reference the case by name).
- `pick_xo` on the table upstream's `recommend_xo` returns for `room` and
  `dolby` (record the table and best).
- `recommend_xo` end to end on `room` and `dolby`: the table and best from
  the reference environment plus the eight-run envelope of each table value
  (the same eight configurations as `engine_e2e`; reuse its runner; at most
  two subprocesses at a time).
- `xo_preview` on `dolby`, `room_dba`, `room_filters`: every group, fc in
  {50, 80, 115, 150}, corrected and not, the group average and one role.
- `sub_combine` on every case with two or more subs.

## Tests (`src/tests/xo.rs`, `src/tests/preview.rs`)

- `xo_scores`, `xo_preview`, `sub_combine` on the replayed designs: measured
  tolerance (`MEASURE`, 1.5 × measured, floor `PLATFORM_RTOL`) on the oracle
  libm; off it, the same except what a threshold can flip (say which).
- `pick_xo`: exact best and scores on the recorded tables.
- `recommend_xo` end to end (production fitters): the ADR 0004 statistical
  rule on the table values (per value `3 × max(w, q)`, per case the two
  binomial checks), and `best` must equal upstream's wherever the reference
  score gap between the best and the second candidate exceeds three times the
  envelope width of that gap; otherwise report it.
- `XoPreview::curves` after `new`: time one call in the test profile and
  assert it stays under 0.2 s (R7).

Register `engine.xo_recommendation` (`implemented` with these tests) and
`engine.chain`'s preview part: add `engine.xo_preview` and
`engine.sub_combine` (kind `"engine"`, milestone `"M2"`, upstream names) as
`implemented`.

## Files you may change

- `crates/secs-engine/src/{xo,preview}.rs`, `crates/secs-engine/src/lib.rs`
  (module lines), `crates/secs-engine/src/tests/{mod,xo,preview}.rs`
- `oracle/export_engine_xo.py`, `oracle/goldens/engine_xo/**`,
  `oracle/README.md` (family row), `features.toml` (the entries above)
- `Cargo.lock`, only as cargo rewrites it

If you need a private helper of another module, make it `pub(crate)` and
list it in the report; do not change its logic.

## Verification (foreground; finish before you report)

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

```sh
cd /home/user/wt/p12b
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p12b
<venv>/bin/python oracle/run.py engine_xo
du -sh --apparent-size oracle/goldens/engine_xo
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine xo -- --nocapture 2>&1 | grep -E "MEASURE|XO_|test result|FAILED|panicked"
cargo test -p secs-engine preview -- --nocapture 2>&1 | grep -E "MEASURE|test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine xo 2>&1 | grep -E "test result|FAILED"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine preview 2>&1 | grep -E "test result|FAILED"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

Files changed; golden size; verification output with `MEASURE` lines; the
recommendation tables (Rust, reference, envelope) for `room` and `dolby`;
the preview call time; anything not matched and why; anything upstream does
that looks like a defect (for the deviation ledger; do not fix it).
