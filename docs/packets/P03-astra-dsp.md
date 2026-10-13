# P03 (ASTRA): secs-dsp primitives, first round

Worktree: `/home/user/wt/p03` (branch `m1/p03`). Work only there. Do not
commit; Claude reviews the diff and commits.

## Goal

Port the numeric primitives upstream SECS 2 builds its engine from, with
oracle goldens per primitive, so M2's engine modules can be ported on top of
verified pieces. Engine logic (`fit_peq` and its ring caps, onset/windowing,
decay, BA, XO, RC, MIMO design) is **not** in this packet: it belongs to
`secs-engine` (M2, `docs/ARCHITECTURE.md` §5).

No clean room: read upstream freely; this is an official port (GPL-3.0).

## Scope (secs-dsp)

Upstream sources are module-level functions of `src/secs/correction_engine.py`
(lines 105–320), `src/secs/mimo.py`, `src/secs/calibration.py:652`, and the
SciPy/NumPy routines they call. Port each with the same semantics, including
edge cases (empty input, nonfinite values, Nyquist bins, ties).

| Module | Functions |
|---|---|
| `grid` | `freqs(n, sr)`, `frac_for(f)` |
| `smooth` | `smooth_power(f, power, frac)` (scalar or per-bin `frac`), `smooth_complex(f, h, frac)` |
| `minphase` | `minimum_phase(mag)` (cepstral, any even length) |
| `response` | `lr4(f, fc, kind)`, `band_filter(f, fc, kind, typ)` with `BAND_TYPES`, `allpass2(f, f0, q)`, `peaking_response(f, f0, q, gain_db, sr)` and its z-domain variant, `peq_response(f, bands)`, `gd_curve(h, f, g, frac)`, `hp2(f, fc)` (mimo.py) |
| `iir` | `scipy.signal.butter` (digital, `output='sos'`, the orders and band types upstream uses), `sosfilt`, `sosfreqz` |
| `fir` | `scipy.signal.firwin` as `mimo.lowpass_fir` calls it |
| `peaks` | `scipy.signal.find_peaks` with the arguments upstream passes (plateau midpoint rule) |
| `lowrate` | `mimo.to_low`, `mimo.conv_matrix` |
| `optimize` | `scipy.optimize.least_squares`: method `trf`, bounds, `max_nfev`, default `x_scale`/`ftol`/`xtol`/`gtol`, two-point Jacobian, `tr_solver='exact'`; `max_nfev` counts residual calls only. Return the result and an optional trace (every evaluated point and cost, every Jacobian point). |

`secs_dsp::fft` already exists (`rfft`, `irfft`, `fft`, `ifft`,
`Complex64`); keep its signatures (secs-mdat uses them). Add helpers there
only if needed.

Prior art you may copy (MIT, Impulcifer-py313, written against SECS 2 Beta
1.1.0 and SciPy 1.18.1, gated against oracle fixtures there):
`/home/user/Impulcifer-pip313/crates/impulcifer-dsp/src/optimize.rs`,
`src/filters.rs`, `src/peaks.rs`, `src/stages/room_tuning2/{primitives,peq}.rs`,
and the trace-recording approach of
`/home/user/Impulcifer-pip313/tests/migration/export_goldens_secs2.py`
(interposing on SciPy's TRF internals). Copied code keeps a header naming the
source file and license, and gets a row in `docs/THIRD_PARTY.md`. Re-gate
everything against this repository's oracle (Beta 1.2.0): do not trust the
old fixtures.

## Files you may change

- `crates/secs-dsp/**` (keep `src/fft.rs` signatures)
- `oracle/export_dsp.py` (new), `oracle/goldens/dsp/**` (generated)
- `features.toml`: the `dsp.*` entries (`dsp.fft` exists; add
  `dsp.smoothing`, `dsp.minimum_phase`, `dsp.responses`, `dsp.iir`,
  `dsp.firwin`, `dsp.find_peaks`, `dsp.lowrate`, `dsp.least_squares_trf`,
  kind `"dsp"`, milestone `"M1"`)
- `docs/THIRD_PARTY.md` (rows for code you copy)
- `oracle/README.md`: add rows for the `dsp` family to the Families table

Do not touch any other file. Other workers own `crates/secs-jos`,
`crates/secs-mdat`, `crates/secs-types`, `crates/secs-project`,
`oracle/export_mdat.py`, `oracle/export_project.py`. Never edit the upstream
tree (`src/`, `tests/`, `tools/`, …).

Available workspace deps (`x.workspace = true`): `rustfft`, `realfft`,
`rayon`, `faer` (no `std`; keep it that way), `pulp`, `gemm`, `faer-traits`,
`thiserror`, `serde`, `serde_json`; tests: `secs-oracle = { path =
"../secs-oracle" }`.

## Contracts

Read first: `CLAUDE.md`, `docs/ARCHITECTURE.md` §5–6, `docs/adr/0004`,
`docs/adr/0007`, `oracle/README.md`, `crates/secs-oracle/src/lib.rs`.

- `#![forbid(unsafe_code)]`; no `std::arch`; SIMD only through `pulp` and only
  where it keeps results identical to the scalar order NumPy uses.
- f64 throughout. Round half to even (`round_ties_even`) where NumPy rounds.
  Sum in NumPy's pairwise order where NumPy sums (`np.sum`, `np.mean`,
  `np.cumsum` is sequential).
- Deterministic: parallel code maps only; no reductions whose order depends
  on threads. faer with `Par::Seq`.
- Public functions take slices and return owned `Vec`s; document panics
  (programmer errors only) and return `Result` for data-dependent failures.

## Goldens (`oracle/export_dsp.py`, family `dsp`)

Use `oracle/common.py`. Run with `<venv>/bin/python oracle/run.py dsp`,
`<venv>` =
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.
Call the upstream functions (`from secs import correction_engine as ce`,
`from secs import mimo`) and SciPy directly where upstream calls SciPy. Use
inputs that look like upstream's: the 131072-point design grid at 48 kHz, real
room spectra from `src/secs/rewapi/sample_dolby_714.npz`, and synthetic
edge cases. Spectra on the design grid may be stored at a bin subset (as
Impulcifer did) to keep the family small; say which. For `least_squares`, at
least: the PEQ residual shape upstream fits (gain/Q/f0 bands, bounds, caps
150 and 400), a problem that stops at `max_nfev`, one that converges by each
tolerance, one with active bounds; record the full trace.

## Tests and budgets

Per `oracle/README.md`: exact where the computation has no FFT or
order-dependent sum (`assert_same`), otherwise `assert_close` with a budget
of 1.5× the measured error and a MEASURE line. Integer outputs (peak
indices, `nfev`, `njev`, `status`) exactly. TRF traces match up to the point
where the oracle's own trace becomes sensitive (report where that is per
case). Register the tests in `features.toml`; mark a feature `implemented`
only when its golden test passes.

## Verification (run in the foreground; finish before you report)

```sh
cd /home/user/wt/p03
<venv>/bin/python oracle/run.py dsp
cargo fmt --all -- --check
cargo clippy -p secs-dsp --all-targets -- -D warnings
cargo test -p secs-dsp -p secs-oracle -p secs-policy -- --nocapture 2>&1 | grep -E "MEASURE|test result|FAILED|panicked"
python3 .github/scripts/upstream.py check-tree
```

Do not end your turn while a command is still running.

## Report

Files changed; the exact output of the verification commands (test counts and
every MEASURE line); dependencies added; code copied and from where; anything
you could not match and why; upstream behaviours you found questionable (do
not "fix" them, list them).
