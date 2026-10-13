# Oracle goldens

SECS 2R ports upstream SECS 2 by matching it, stage by stage, on fixed inputs
(docs/adr/0004). The oracle is the upstream Python tree in this repository
(`src/secs`), which CI keeps byte-identical to `UPSTREAM.toml`'s commit. The
exporters here run it and freeze what it returns as fixtures; the Rust tests
read them through `crates/secs-oracle`.

## Regenerating

```sh
python3.13 -m venv ~/.venvs/secs-oracle
~/.venvs/secs-oracle/bin/pip install -r oracle/requirements.txt
~/.venvs/secs-oracle/bin/python oracle/run.py libm mdat project dsp dsp_de
```

`run.py` runs `export_<family>.py` for each family named, with NumPy's SIMD
dispatch above its baseline turned off (`NPY_DISABLE_CPU_FEATURES` set to the
features this CPU has), BLAS and OpenMP on one thread and `PYTHONHASHSEED=0`.
With AVX2 or AVX-512, NumPy's float64 `log10` and `exp` come from SIMD kernels
that differ from the C library in the last bit, and a PEQ fit that stops at its
evaluation cap amplifies that into a different result; with dispatch off they
are the C library's, which is also what Rust's `f64` methods call on Linux.
Every exporter checks this (`common.check_libm`) and refuses to run otherwise.

An exporter rewrites only files whose bytes changed and refuses to finish when
its folder holds a file it did not write (delete stale files by hand). Commit
the goldens with the change that needs them.

## Layout and encoding

```
oracle/
  run.py, common.py           launcher and shared helpers
  export_<family>.py          one exporter per family
  goldens/<family>/<case>.json
  goldens/<family>/<key>.f64  float64 sidecars
  goldens/<family>/<key>.bin  byte blobs (files the port reproduces exactly)
```

A fixture is `{"inputs": …, "outputs": …, "meta": …}` as compact UTF-8 JSON
below 200 kB. `meta` names the oracle commit and the Python, NumPy and SciPy
versions.

- float64 values are written at `repr` precision (shortest round trip); the
  Rust side parses them exactly (`serde_json` with `float_roundtrip`).
- Nonfinite values are the strings `"nan"`, `"+inf"`, `"-inf"`.
- A complex number is `[re, im]`; a complex array is `{"re": …, "im": …}`.
- An array of at most 64 values is inline. A longer one is a sidecar
  descriptor `{"file", "length", "sha256", "first", "last"}` (first and last
  32 values, for reading diffs) pointing at headerless little-endian float64;
  above 24000 values it is a list of such descriptors in order.
  Sidecars are content-addressed within a family: an array whose bytes
  equal an earlier sidecar's points at that file.
- Raw bytes are `{"blob", "length", "sha256"}`.
- An array whose values another fixture already pins (a model adapted from
  a blob) may be a digest `{"digest", "length", "first", "last"}`: the
  SHA-256 of its little-endian float64 bytes, without a sidecar
  (`Family.f64_digest`, `Fixture::assert_digest`).
- Matrices are row-major.

`secs-oracle` checks every sidecar's length and SHA-256 when it reads it, and
its test `every_sidecar_and_blob_matches_its_descriptor` checks all of them.

## Budgets

Integer decisions (indices, counts, chosen branches, rounded sample delays,
evaluation counts) match exactly. A continuous value's budget is 1.5× the
largest error measured when the port first matched (with a floor); tests print
it as `MEASURE <name> n=… max_abs=… max_rel=…`. Stages that are exact on the
oracle's libm compare bit for bit there (`assert_same`) and within `1e-9`
relative elsewhere (`oracle_libm()` decides from `goldens/libm/probe.json`).

TRF is gated by one-step replay at every trial of the oracle's own path:
Rust recomputes the residual and bounded two-point Jacobian at the recorded
`x`, using the recorded trust radius and carried `alpha`. Steps, `x_new`,
the predicted reduction and the updated radius must agree within `1e-9`
relative to `max(1, |expected|)`. The actual reduction is `cost(x) -
cost(x_new)`, so it inherits the (gated) difference between our `x_new` and
the oracle's, amplified by the residual's slope; since the residual is
bit-exact at a given point, the test subtracts the cost difference between
the two `x_new` values and holds only the remainder to `1e-9`; acceptance and post-trial Jacobian recomputation must
match exactly, except reported oracle ratios within `1e-9` of 0, 0.25 or
0.75. Only converging fits gate global x/cost (1.5× the measured error,
`1e-10` relative floor) and exact nfev/njev/status; capped fits gate nfev
and status, and report njev and trajectory differences. P03b established
that a one-ulp step change amplifies into a 5e-4 Jacobian change even in
SciPy itself, while OpenBLAS Haswell/SandyBridge/Nehalem/Prescott runs agree
to 1e-16: a variant-derived global trace horizon cannot gate this port.
The full trace and the first three intermediate trials remain reports.
The replay's premise, a bit-exact residual at the oracle's `x`, holds only
on the oracle's libm (`oracle_libm()`); elsewhere the replay reports its
mismatches without failing, and the converging-fit results remain the gate.
On the room fits, trial 81's actual reduction differs by 8.579e-9, of which
8.578e-9 is the cost difference of a one-ulp `x_new` difference; the largest
unexplained remainder over all trials is 2.7e-12.

## Shared engine cases (P05)

`engine_cases.cases()` returns fresh copies of this frozen set. All engine
families use these names and load IRs/config through `engine_inputs`; do not
make a new synthetic room in each exporter. Construction calls the real
`calibration.make_engine`, without importing Qt wizard modules.

| Case | Data / positions / measured roles | Settings |
|---|---|---|
| `dolby` | Real tutorial NPZ; 1 position; L R C Ls Rs Lb Rb Tfl Tfr Tbl Tbr SW1 SW2 SW3 | Default mixed |
| `dolby_zero` | Same Dolby IRs | zero (PEQ only) |
| `dolby_linear` | Same Dolby IRs | linear (phase fraction 24) |
| `dolby_ultra` | Same Dolby IRs | ultra, default 200 ms |
| `dolby_infra` | Same Dolby IRs | Tutorial infra quest: SW3 LP 25 Hz LR24 only, SW1/SW2 HP 25 Hz LR24 |
| `room` | 4 positions; L R C SW1 SW2 | Default mixed; volume 6.2 × 4.4 × 2.7 m³ |
| `room_dba` | Same room; 4 positions; L R C SW1 SW2 SW3 SW4 | Bundle A, DBA; SW1/SW2 front, SW3/SW4 rear; rear trim 1.25 ms |
| `room_multi` | Same IRs as `room_dba` | Bundle A, multi; rear trim retained but unused |
| `room_manual` | Same room plus SW3; 4 positions | SW1 fixed −2.5 dB / 6 ms / inverted / AP 65 Hz Q0.8; SW2/SW3 bundle A `values`; main delay 8 ms |
| `room_filters` | Same IRs as `room` | SW1 BW12 HP18 / LR48 LP160 / two PEQs; SW2 LR12 HP22 / BW24 LP180 / one PEQ; XO PEQ 35–270 Hz; boost 4 dB, infra boost 8 dB |

Synthetic IRs convolve upstream `tests/synth.py` excitation (seed 20260915 +
role index) with `tests/room_model.py` modal transfers, 1 second at 48 kHz,
then cast to float32. All variants are subsets of one four-position,
seven-role room. `t0 = (position−1)×0.00013 + role_index×0.00107 − 0.0002`
seconds includes nonzero/negative offsets and keeps the earliest main arrival
common to all subsets. The tutorial project mirrors `wizard.new_project(True)`;
measurement order mirrors `step5_measure.plan` (`ordered_speakers`, subs last),
not NPZ order or output-channel order.

`engine_inputs` uses ordered pairs for positions, roles and configuration
maps; float32 blobs are content-addressed within this exporter. `config`
records the real constructor arguments (including defaults) through an
interposed `__init__`, and the five attributes set afterward. Constructor
knobs owned by `Tuning` retain `Tuning::default()`. `settings` is the raw
settings dataclass and `project` holds groups, roles, XO and target.

`engine_prep` samples 1,032 bins: every bin in 15–300 Hz plus every 257th
full-grid bin. Spectrum rows are `[position, role, spectrum_index]` into
`spectra.json`'s packed complex128 bank (each index occupies 1,032 values).
The bank deduplicates identical cells across windows and cases before using
normal float64 sidecars. It is data storage only, not an additional case.
`H0` differences are empty: upstream copies dictionaries after fixed filters,
so **all** H0 cells initially alias H, including filtered subs. Long 500 ms
spectra differ from preparation: their taper is 10%, not 20%.

Budgets: `engine_inputs` ≤9,000,000 bytes; `engine_prep` ≤3,000,000 bytes,
asserted by the exporters. Spectrum parity measured max error / sampled peak
5.368509846704912e-16; 1.5× is below the 1e-12 floor. Levels, arrivals,
weights, target and Schroeder use `PLATFORM_RTOL`; indices are exact.

## Recorded engine fits (P07)

`engine_fit/fit_<case>_<stage>_<call>.json` is the shared recorded-call form.
Stages are `rc`, `xo` (main acoustic XO), `bus` and `bundle`. `meta` carries
`case`, `stage`, `role`, and the zero-based per-case/per-stage `call`. The
selection keeps every Dolby/room call, the first three per stage elsewhere,
and every main-role XO call (97 PEQ calls total).

A later family's recorded call uses the same `inputs` object:
`{g, corr_db, n, lo, hi, boost, ring_ms, f_max}`. Both arrays use normal
Family float64 descriptors relative to that family's folder; the two optional
scalars are JSON null when absent. The result is an array of `[Hz, Q, dB]`
triples, in `outputs.result`. The shared Rust reader is
`tests::fit::read_recorded_fit(fixture, inputs, result)`. `replay_fitter`
compares n and optional presence exactly and all continuous arguments using
`abs(actual-expected)/max(1,abs(expected))`; its panic identifies the call and
worst field/index. Callers must also check their expected call count.

Additional P07 outputs are `seed` (greedy triples), `x0`, `lb`, `ub`, `cap`,
TRF `x`, `cost`, `nfev`, `njev`, `status`; and `trace` digests of all residual
call points/costs, Jacobian base points, and the initial residual/Jacobian.
The matrices are row-major. Empty fits have only an empty seed/result.

`one_step` preserves **every** trial, losslessly: `initial_x`, the per-trial
`step`, `Delta`, carried `alpha`, reductions, ratio, `Delta_new`, accepted and
Jacobian-recomputed flags. Reconstruct `x_new = x + step`, move any value on
a bound one ULP inward, and set the next x to x_new only when accepted.
The exporter proves this against SciPy's actual arrays, and the Rust test
checks the complete reconstructed `x_new_digest`. This removes two redundant
matrix columns to keep the family below 6,000,000 bytes. Long diagnostic
traces remain digests, not truncated optimizer paths.

`xo_*` names the shared input case, role, ordered positions, applied gain and
scalar arguments, and the corresponding `fit_*` record. Its test rebuilds
responses from P05 prepare and cuts the fitter with recorded decisions.
`gd_*` records **sampled corrected combined spectra plus full real/imag
SHA-256 digests**, not reconstructable full spectra. It also keeps every
inner least-squares x0/bounds/result/count/status. These inputs depend on
RC/BA decisions: a full GD result gate is pending their reconstruction;
interpolating the samples would not reproduce the original GD computation.

Current P07 blocker: seeds, boxes, initial residuals and Jacobians are exact
on oracle libm, but the existing secs-dsp TRF diverges globally on these
6–16-band converging engine fits (including evaluation counts). The local
1e-9 replay also fails some ill-conditioned RC trials. These are retained
as failing tests, not waived or turned into capped-fit reports. The feature
remains partial until the solver limitation is resolved. `secs-dsp` is not
changed by P07. XO request tolerance is 1e-9 (measured errors below the floor);
1e-10 final-fit and 1e-9 local-replay floors remain diagnostic failing gates,
not claimed measured parity budgets.

## Families

| Family | Exporter | What is frozen |
|---|---|---|
| `dsp` | `export_dsp.py` | P03 grid, smoothing, cepstral minimum phase, analog/RBJ responses, GD, digital Butterworth SOS, FIR, peaks, low-rate projection; N=131072 sr=48000 room L spectrum from sample_dolby_714.npz. Smoothing outputs keep bins 0–1799, every 127th higher bin and Nyquist (computed on the full grid); all other outputs are complete. TRF includes caps150/400, real-room 3-band fits that exhaust both caps, all residual/probe calls and Jacobian bases. Every oracle trial records x/Delta/carried alpha, step, x_new, reductions, ratio, updated radius and accept/Jacobian-update decisions for local replay. Trials 0–2 of peq150/room150 keep full intermediates; global traces are reports. |
| `dsp_de` (`rs_*`) | `export_dsp_de.py` | NumPy 2.5.3 legacy RandomState seeds 0, 1, 2, 12345, 2**32−1: 5000 raw uint32 draws as little-endian byte blobs, 5000 53-bit samples; mixed scalar randint (DE dimensions and rejection/width edges), uniform, permutation and repeated in-place shuffle. Random streams always compare bit-for-bit, including on foreign libm. |
| `dsp_de` (`de_*`) | `export_dsp_de.py` | SciPy 1.18.1 unpolished immediate best1bin: BA-shaped nonsmooth costs at every dimension 2–12, seeds 1/2, with/without x0; Rosenbrock and plateaus at dimensions 2/7/12; tol convergence and maxiter exhaustion, zero iterations, constant/equal/reversed bounds, minimum population, one dimension, NaN/infinity. Every cost-call vector and cost (full values up to 4096, longer traces as digests), final x/fun/nit/nfev/success/message; traces are bit-exact on the oracle libm (digests checked whole), elsewhere `rtol(0)` on the values and the digests' first/last 32. Counts and messages are always exact. |
| `engine_fit` | `export_engine_fit.py` | Actual calibration PEQ calls, exact greedy seeds and ring boxes, lossless per-trial replay, long trace digests; main XO reconstruction identities; GD sampled inputs and inner solver results. ≤6 MB; solver/GD gates still partial (see above). |
| `engine_inputs` | `export_engine_inputs.py` | Ten shared engine cases, float32 IRs stored once, observed make_engine constructor/configuration values, raw settings and project summary |
| `engine_prep` | `export_engine_prep.py` | Constructor normalisation, exact onsets/window starts, arrival/alignment/base, sampled H/H0 and long 2000/500 ms spectra, position/frequency weights, main levels/reference, Schroeder and target samples |
| `libm` | `export_libm.py` | float64 `ln`, `log10`, `log2`, `exp`, `sin`, `cos`, `tan`, `atan`, `tanh`, `atan2`, `pow` at fixed points |
| `mdat` | `export_mdat.py` | bounded Java serialization parsing, Modified UTF-8, full REW model adaptation, byte-identical round trips, and deterministic 1- and 3-measurement builds (one from three real Dolby-room IRs); built models are frozen as digests |
| `project` | `export_project.py` | role vocabulary, `.secs` presets and migrations, Python JSON bytes, tap allocation, target curves, float32 WAV bytes and readers |
