# P14 (ASTRA): MIMO support ranges, ideal FIR supports and the do-no-harm guard

Worktree: `/home/user/wt/p14` (branch `m3/p14`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p14`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

Read first: `docs/MIMO.md` (the plan; this packet is its P14), `docs/ENGINE.md`
§3 (stage-boundary goldens, the off-oracle-libm rule), `docs/adr/0004-parity-and-deviations.md`
("끝단 판정의 통계 규칙"), `docs/adr/0007-compute-and-gpu.md`, and upstream
`src/secs/mimo.py` (all of it) with the call site `src/secs/calibration.py`
`compute_mimo` (around line 934). The upstream tree is GPL-3.0 like this
repository; read it freely, but never edit it.

## Goal

Port `support_ranges`, `support_path`, `design_mimo` (ideal FIR supports,
λ bisection, guard), `_rank_supports`, `_worst_band_db`, `worst_t60_change`
and `lowpass_fir` to `crates/secs-engine/src/mimo/design.rs` (new module
`mimo` with `mod.rs` re-exporting the public items), with an oracle family
that measures SECS 2's own spread and a golden test that judges Rust against
it. IIR realization (`mimo_iir.py`) is P16, not here.

## Interface (fixed; add private helpers freely)

```rust
pub struct MimoRequest {
    pub fsiso: f64,                 // upstream fsiso, default 150
    pub level_db: f64,              // upstream LEVEL_DB, -12
    pub f_low: Option<f64>,         // user low edge (None = automatic)
    pub max_supports: Option<usize>,// None = unlimited (upstream 0 -> None)
    pub exclude: Vec<String>,       // subs with mimo_support = false
    pub groups: Option<Vec<String>>,// None = every group
    pub taps_low: usize,            // 180 (TAPS_LOW) or 70 (FIR_TAPS_LOW)
}
pub struct MimoDesign {
    pub fs_low: f64,                                        // 600
    pub ranges: Vec<(String, Option<(f64, f64)>)>,          // speaker order as upstream
    pub supports: Vec<(String, Vec<(String, Vec<f64>)>)>,   // input -> [(support role, low-rate FIR)]
    pub level_db: Vec<(String, f64)>,                       // input -> support level dB
    pub notes: Vec<Note>,
}
pub fn design_mimo(
    design: &Design, measurements: &MeasurementSet, config: &EngineConfig,
    req: &MimoRequest, ctx: &Context,
) -> Result<MimoDesign, EngineError>;
```

Everything upstream reads from the engine object comes from `Design`, the
measurement preparation and the chain simulator (`long_spectra` with the sub
fixed filters, `input_response`, `align_delay_ms`, the input's electrical `F`,
`sub_passband`, groups, positions). Find each one; if something upstream reads
is not reachable without changing an existing public signature, add a
`pub(crate)` accessor rather than changing the signature, and say so in the
report. Add the MIMO notes to `Note` (`crates/secs-engine/src/notes.rs`) with
the upstream Korean text, as the other notes do.

## Arithmetic decisions (from docs/MIMO.md §3; do not deviate)

- **AᵀA without the design matrix.** Never build `A`. For each position the
  candidate column block is the Toeplitz `conv_matrix(g_j, taps, n)` and the
  row weight is `c_t = 10` before `e_end = onset + 36` and `1` from `e_end`
  (that is `wl² + MU_EARLY·we²`). Compute block `(j, j')` as
  `M[k, l] = Σ_t c_t g_j[t-k] g_j'[t-l]`: the first row and first column by
  direct sums in increasing `t`, then every other entry by the diagonal
  recurrence `M[k+1, l+1] = M[k, l] - c_{n-1} g_j[n-1-k] g_j'[n-1-l] + (c_e - c_{e-1}) g_j[e-1-k] g_j'[e-1-l]`
  (the last term only when `e-1 >= max(k, l)`; check the boundary cases
  yourself against the definition). Sum the blocks over positions in position
  order. `Aᵀb` is `-Σ_t wl_t² x_t g_j[t-k]` by direct sums. A unit test builds
  `A` densely for a small random case (2 positions, 2 candidates, taps 12,
  n 40, onsets inside the early window and at 0) and checks `AᵀA` and `Aᵀb`
  to relative 1e-12 of the largest entry.
- **Solve by Cholesky.** `AᵀA + λI` with faer's Cholesky (`Par::Seq`); if the
  factorization fails, partial-pivot LU. Same λ bisection as upstream: the
  first solve at `hi = 4`, then 18 halvings of `[lo, hi]` in log10 units of
  `mean(diag(AᵀA))`; the level is `Σ_k Σ (lp ⊛ u_k)²` over `Σ to_low(F_i)²`.
- **`_rank_supports`** uses the same recurrence with late weights only
  (`c_t = 0` before `e_end`, `1` after) and `1e-2·mean(diag)` regularization.
- **Parallel over inputs only** (`ctx.try_map`, order preserved; accumulation
  inside an input is sequential). Check cancellation once per λ step and per
  guard step. Before an input's work, take a memory permit from
  `ctx.budget()` for the bytes you allocate (AᵀA plus the per-position path
  signals); document the estimate in a comment.
- Keep upstream's arithmetic everywhere else (`np.convolve` = direct
  convolution in index order; `np.sum` = NumPy pairwise sum via
  `secs_dsp::numeric::pairwise_sum` where upstream sums an array; Python
  `sum` over a list = sequential). `to_low`, `conv_matrix`, `firwin`, `hp2`,
  `smooth_power`, `butter`, `sosfilt`, `find_peaks` and `late_peaks` exist in
  `secs-dsp` and `secs-engine::decay`; use them, do not re-implement them.

## Oracle family `engine_mimo`

Write `oracle/export_engine_mimo.py` (model it on
`oracle/export_engine_e2e.py`, which already runs a case under eight labels:
reference, two BLAS core types, NumPy SIMD, four one-ULP IR perturbations).

- Cases: `room`, `room_dba`, `room_filters`, `room_multi` from
  `oracle/engine_cases.py` with default MIMO settings, plus on `room`: support
  cap 2, FIR taps (`taps_low=70`), user low edge 40 Hz, and `SW2` excluded
  (pick whatever sub names the case has). Also add a positions-below-three
  case (the note only).
- Record per case: `ranges`; per input in upstream order: the candidate list
  (after ranking), the ranking scores when ranked, the λ trajectory (`mid`,
  ratio, accepted) and the final log10 λ, `ratio`, the guard steps (scale,
  `_worst_band_db`, `worst_t60_change`), the final supports (taps) and
  `level_db`; the notes. Interpose on the upstream functions; do not copy
  their bodies.
- Spread: run every case under the eight labels and store, per recorded
  number, the values of all eight runs. Keep the family under 8 MiB (store
  support taps as f64 blobs, as the other families do).
- Run the exporter yourself and commit nothing; Claude commits the goldens.

## Tests (in `crates/secs-engine/src/tests/mimo.rs`)

- Ranges: equal to the reference after rounding to 0.1 Hz (upstream rounds);
  `None` where upstream is `None`.
- Candidates and rank order: equal to the reference on the oracle libm;
  reported (`MIMO_OFFLIBM`) elsewhere.
- Support taps, `level_db`, final λ, guard values: ADR 0004's statistical
  end-stage rule against the eight runs (reuse the helpers in
  `crates/secs-engine/src/tests/e2e.rs`; move them to a shared test module
  if needed). Off the oracle libm the per-value caps are reported and the
  binomial checks asserted, as in `e2e.rs`.
- Guard decisions (number of −3 dB steps, off) and notes: equal where all
  eight runs agree; reported otherwise.
- The upstream behaviours: fewer than three positions gives only the note;
  cap 4 on the case with the most candidates gives exactly 4 supports, a
  subset of the uncapped set (if no case has more than 4 candidates, use cap
  2 and say so); excluded subs are never candidates and produce the
  "서포트 제외(사용자)" note; a sub low-pass fixed filter lowers that sub's
  upper range edge (upstream `test_calibration_r27_sub_manual.py:264-351`).
- The AᵀA unit test above, and a Cholesky-versus-LU test on one golden
  input's final system (relative 1e-9 on the solution).
- Determinism: the same `design_mimo` with a one-thread and a four-thread
  `Context` gives bit-identical output.

`features.toml`: set `engine.mimo_design` to `implemented` with your tests
(format `secs-engine::test_fn`).

## Files you may change

- `crates/secs-engine/src/mimo/**` (new), `crates/secs-engine/src/lib.rs`
  (module and re-exports), `crates/secs-engine/src/notes.rs` (MIMO notes),
  `crates/secs-engine/src/tests/**`, `pub(crate)` accessors in other
  `crates/secs-engine/src/*.rs` files (no behaviour change)
- `oracle/export_engine_mimo.py` (new), `oracle/engine_cases.py` (only if a
  case needs a MIMO setting it lacks), `oracle/goldens/engine_mimo/**`
- `features.toml` (the entry above), `Cargo.lock` only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p14
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p14
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine --locked 2>&1 | grep -E "test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine --locked 2>&1 | grep -E "test result|FAILED"
cargo test -p secs-engine --release --locked 2>&1 | grep -E "test result|FAILED"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

What you ported and where, every accessor you added, the oracle family
(cases, size), the test results with the largest per-value deviations and the
binomial counts per case, anything reported rather than asserted and why,
and anything you could not do.

## Amendment (Claude, 2026-10-13, answering the worker's question)

`crates/secs-engine/Cargo.toml` may take `faer.workspace = true` under
`[dependencies]`. This is the existing workspace dependency with its pinned
version and features, not a new package, so it does not break the "no new
dependencies" rule. Use it for the Cholesky and LU solves only, always with
`Par::Seq`. Do not touch `secs-dsp` (P15 is editing it). Add
`crates/secs-engine/Cargo.toml` to your allowed files for this one line.

## Amendment 2 (Claude, 2026-10-13): the Cholesky-versus-LU test

The fixed 1e-9 relative tolerance in that test was a wrong specification:
two backward-stable solvers of the same system agree only to about
`κ(M)·ε`, and the final MIMO systems sit at a small λ where `κ(M)` is large
(the worker measured 3.98e-9 on room/L). Do not add iterative refinement or
change the solver; upstream solves once with LU and the parity gates already
judge the outcome. Replace the test's assertion with what "same mathematics"
means for a linear solve, on the same golden input's final system
`M = AᵀA + λI`:

- each solution's normwise backward error
  `‖M x − b‖ / (‖M‖·‖x‖ + ‖b‖)` (infinity norms, residual computed in f64 in
  index order) is at most `16·N·ε`, `N` the system size; and
- the relative difference `‖x_chol − x_lu‖ / ‖x_lu‖` is at most
  `16·N·κ·ε`, with `κ` the 2-norm condition number of `M` from its
  eigenvalues (compute it in the test with faer's self-adjoint
  eigendecomposition).

Print both measured values and the bound. If either assertion fails, report
the numbers; do not loosen the constants.
