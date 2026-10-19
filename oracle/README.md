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

After an upstream merge, every family is exported again from the new release
(the porting PR says so). A fixture whose inputs and outputs are unchanged
keeps its file, so `meta.oracle` names the release that first produced its
values and a merge rewrites only what the new release changed.

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
- A fixture recording many calls of one function bundles them:
  `{"inputs": {"calls": [...]}, "outputs": {"calls": [...]}}`, one entry per
  call in call order, so a family does not grow one file per call
  (`Fixture::calls`, `Fixture::all_calls`).

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

The `filters` family also computes `dolby`, `dolby_infra`, and `dolby_ultra`, then calls the public `calibration.export_eqapo` wrapper unchanged. It derives sub-free `room` mixed and zero runs and calls both public filter wrappers unchanged. Text files are frozen byte-for-byte; WAV sidecars record ordered paths, byte lengths, and SHA-256 because their samples are already pinned by each case's `SignalChain`. The Dolby run additionally records that `export_filter_text` raises `CalibrationError` before creating its folder.

`calibration` freezes settings normalization and byte-identical project storage,
key equivalence classes, every mode's resolved parameters, the settings used by
each shared engine case, and biamp/non-reference-role adapter edges.

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

Budgets: `engine_inputs` ≤9,000,000 bytes; `engine_prep` ≤3,200,000 bytes,
asserted by the exporters. Spectrum parity measured max error / sampled peak
5.368509846704912e-16; 1.5× is below the 1e-12 floor. Levels, arrivals,
weights, target and Schroeder use `PLATFORM_RTOL`; indices are exact.

## Recorded engine fits (P07)

`engine_fit/fit_<case>_<stage>_<chunk>.json` bundles up to four calls in the
shared recorded-call form (each JSON remains below 200 kB).
Read calls with `Fixture::all_calls` / `Fixture::calls`; each input carries its
stable `id` (`fit_<case>_<stage>_<call>`), role and call index. Stages are `rc`,
`xo` (main acoustic XO), `bus` and `bundle`; `meta` carries case and stage. The
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
matrix columns to keep the family, including sensitivity runs, below 7,000,000 bytes. Long diagnostic
traces remain digests, not truncated optimizer paths.

`xo_*` names the shared input case, role, ordered positions, applied gain and
scalar arguments, and the corresponding `fit_*` record. Its test rebuilds
responses from P05 prepare and cuts the fitter with recorded decisions.
`gd_*` records **sampled corrected combined spectra plus full real/imag
SHA-256 digests**, not reconstructable full spectra. It also keeps every
inner least-squares x0/bounds/result/count/status. These inputs depend on
RC/BA decisions: **P09 owns the end-to-end GD result gate**, after P09/P10
rebuild full spectra; interpolating samples would not reproduce the original
GD computation. Each recorded inner fit is also rerun in the three spread
environments with its original closure inputs and upstream residual code,
not with inputs from a new, already-diverged GD trajectory.

P07b applies ADR 0004 to engine fits separately from the small P03 DSP fits
above. One-step replay uses 1.5× the frozen largest oracle-libm measurement,
floored at 1e-9, and reports each call's worst trial and the extreme-singular-
value ratio from the step's own SVD (the scaled, augmented Jacobian, not raw J).
Foreign-libm replay is report-only; exact seed/residual/Jacobian checks remain.

For every recorded PEQ call the exporter reuses the exact arguments in three
fresh oracle interpreters: OpenBLAS Haswell, OpenBLAS SkylakeX, and NumPy SIMD
dispatch enabled. One process per environment handles all calls, including GD
inner fits. Only the SIMD worker skips `common.check_libm`; it never creates a
Family. `meta.environments` records the actual NumPy/SciPy OpenBLAS cores from
`OPENBLAS_VERBOSE=2`, and dispatch mode. All runs remain single-threaded.
`outputs.environments` holds each run's returned bands, cost and nfev/njev/status.
`final_cost` is the reference returned bands' weighted residual cost, including
any final ring-Q clamp; the old `cost` remains the solver's diagnostic cost.

P07c adds eight reference-environment runs in `outputs.perturbations`, labelled
`ulp_0` through `ulp_7`. Each call resets `np.random.default_rng(seed)` (PCG64),
draws `integers(0, 2, size=corr_db.shape)` and applies `np.nextafter` toward
negative/positive infinity respectively. Exactly zero elements (including
signed zero) stay unchanged. Each run calls the unmodified upstream `fit_peq`,
including greedy placement. Only returned bands, cost, nfev/njev/status and
the run label are stored; no extra paths, spectra or matrices are retained.
All returned-band costs are evaluated against the original, unperturbed target
so the twelve runs compare the same objective, including final Q clamping.

GD inner fits have no `corr_db`. Their equivalent targets are the interpolated
GD baseline (`b`, in ms) and `mag0` (in dB). The eight runs perturb these two
arrays by the same one-ULP rule, drawing baseline signs first and magnitude
signs second from the per-call seeded generator. They reuse upstream's exact
residual code, with `g=gw` and `base=perturbed_b` so interpolation at the same
nodes supplies that baseline. Recorded `cur`, x0 and bounds stay fixed. Costs
are evaluated with the original residual closure. This is an inner-fit
sensitivity experiment, not a replacement for P09's full GD result gate.

The end-result gate, on every libm, is `Rust cost <= c0 +
1.25*max(cmax-cmin, 1e-6*c0)`: Rust may differ from the reference by up to 1.25
times SECS 2's own spread (ADR 0004), the range of the twelve runs (reference,
three environments, eight perturbed runs). Twelve runs cannot resolve a spread
far below the typical one (median 1.7e-5 of the cost, as small as 2e-8 on some
calls), so the spread is floored at 1e-6 of the cost; without the floor macOS's
libm failed `fit_dolby_rc_015` at 1.30 times a 1.1e-7 range. Converging fits
are gated on every libm. A fit that stops at its evaluation cap ends wherever
its trajectory is at the cap, and the twelve runs only sample the oracle's libm:
capped fits are gated on the oracle libm and reported elsewhere (Windows'
libm took `fit_dolby_rc_010` 3.6 ranges above the reference, a capped fit),
as docs/ENGINE.md §3 planned; the design's end result is gated by
`engine_e2e`. On every libm the number of calls where Rust exceeds all twelve
runs must stay within the binomial 99.9 % quantile for a thirteenth draw
(16 of 97; Linux 7, macOS 6, Windows 12). Lower cost always passes; evaluation counts are
reported, not required to follow a different trajectory. Each `SPREAD` line
prints Rust/c0, `(cmax-cmin)/c0`, the first run attaining cmax (reference wins
ties), nearest bands, counts and pass/fail. Failures also print the Rust cost
and all twelve costs. If all twelve costs, counts, statuses and bands are
bit-identical and replay is exact, the Rust bands must also be bit-identical
on oracle libm. `secs-dsp` is unchanged. XO request relative tolerance
remains 1e-9.

P07b first anchored the margin at `cmax-c0`. That fails whenever the reference
happens to be the worst of the twelve runs, and it did on seven calls
(`fit_dolby_rc_003` and XO calls 000/008 in `dolby`, `dolby_zero`,
`dolby_linear`; three distinct inputs), with Rust 6.6e-6, 5.6e-7 and 1.2e-6
above the reference against ranges of 2.5e-4, 6.4e-6 and 1.2e-6. The twelve
runs are draws from one last-bit distribution and Rust behaves as a
thirteenth: it exceeds all twelve on 7 of 97 calls, against 97/13 = 7.5
expected (`SPREAD_RANK`). P07b measured maximum local relative error
6.952128117632128e-8 (budget 1.0428192176448192e-7); all 97 replay calls pass.
All 97 calls pass the gate on the oracle libm and the foreign path.

### P07c: seven bound-sensitive trials

The worst trial of each of the seven calls above 1e-9 was captured by
interposing on SciPy's `solve_lsq_trust_region` and `select_step`, and compared
with Rust's one-step intermediates. The error metric is
`max_i |Rust_i-oracle_i| / max(1, |oracle_i|)`, as in `secs_oracle::errors`;
**stride relative error** below instead divides by the actual oracle stride.
Indices are zero-based.

All seven have identical x, residual, unscaled J, scaling d and scaled J_h.
Gradient/diagonal augmentation differences are at most 9.926e-14; singular
values differ by at most 1.288e-14, sign-normalised right vectors by
8.608e-13, and `uf` by 4.233e-14. Nothing before bound handling exceeds 1e-12
in the replay metric. Both solvers take the full-rank Gauss–Newton early
return: alpha_out=0 and Newton iteration count=0. Initial alpha bounds and
alpha iterates are **not reached**, rather than absent from the capture.

The first amplified difference is the bound stride `(bound-x)/p`: the hit
component is only 1–43 ULPs from its bound, with p between 4.78e-16 and
1.99e-14. Last-bit absolute differences in this tiny p become relative
stride differences of 2.81e-9–1.09e-7, which scale the whole selected step.
The selected branch agrees in every case; this is not an alpha iteration or
stopping-test flip.

| Call (`fit_` prefix) | Trial | Hit component | Branch | p_h error | Stride relative error | Selected-step error | x_new error |
|---|---:|---:|---|---:|---:|---:|---:|
| dolby_linear_rc_001 | 41 | 38 | p | 1.859624e-15 | 1.439674e-8 | 1.195853e-9 | 3.032810e-10 |
| dolby_rc_005 | 23 | 16 | p | 1.568362e-14 | 1.091265e-7 | 3.065053e-8 | 4.773356e-9 |
| dolby_rc_009 | 18 | 16 | p | 1.237899e-14 | 7.508427e-8 | 2.522914e-8 | 4.727989e-9 |
| dolby_rc_010 | 142 | 19 | p | 4.933554e-15 | 8.314178e-9 | 4.205900e-9 | 5.156329e-10 |
| dolby_rc_015 | 27 | 37 | p | 6.827872e-15 | 4.950037e-8 | 1.257912e-8 | 2.009701e-9 |
| dolby_rc_018 | 100 | 34 | p | 6.326103e-15 | 7.931662e-8 | 6.952128e-8 | 8.217231e-9 |
| dolby_rc_019 | 42 | 43 | reflected | 3.214443e-15 | 2.809758e-9 | 2.206856e-9 | 6.993929e-10 |

Countercheck: pass Rust's p_h to **unmodified SciPy `select_step`**, retaining
all other oracle quantities. The resulting selected step is bit-identical
to Rust's in **all seven trials**. Thus the amplification is reproduced by
SciPy's own TRF bound handling, not a mistranslated selection formula. This
mechanism was accepted for P07c; no optimizer code or replay budget changed.

## Families

| Family | Exporter | What is frozen |
|---|---|---|
| `dsp` | `export_dsp.py` | P03 grid, smoothing, cepstral minimum phase, analog/RBJ responses, GD, digital Butterworth SOS, FIR, peaks, low-rate projection; N=131072 sr=48000 room L spectrum from sample_dolby_714.npz. Smoothing outputs keep bins 0–1799, every 127th higher bin and Nyquist (computed on the full grid); all other outputs are complete. TRF includes caps150/400, real-room 3-band fits that exhaust both caps, all residual/probe calls and Jacobian bases. Every oracle trial records x/Delta/carried alpha, step, x_new, reductions, ratio, updated radius and accept/Jacobian-update decisions for local replay. Trials 0–2 of peq150/room150 keep full intermediates; global traces are reports. |
| `dsp_de` (`rs_*`) | `export_dsp_de.py` | NumPy 2.5.3 legacy RandomState seeds 0, 1, 2, 12345, 2**32−1: 5000 raw uint32 draws as little-endian byte blobs, 5000 53-bit samples; mixed scalar randint (DE dimensions and rejection/width edges), uniform, permutation and repeated in-place shuffle. Random streams always compare bit-for-bit, including on foreign libm. |
| `dsp_de` (`de_*`) | `export_dsp_de.py` | SciPy 1.18.1 unpolished immediate best1bin: BA-shaped nonsmooth costs at every dimension 2–12, seeds 1/2, with/without x0; Rosenbrock and plateaus at dimensions 2/7/12; tol convergence and maxiter exhaustion, zero iterations, constant/equal/reversed bounds, minimum population, one dimension, NaN/infinity. Every cost-call vector and cost (full values up to 4096, longer traces as digests), final x/fun/nit/nfev/success/message; traces are bit-exact on the oracle libm (digests checked whole), elsewhere `rtol(0)` on the values and the digests' first/last 32. Counts and messages are always exact. |
| `engine_design` | `export_engine_design.py` | P12a complete calibration, all ten shared cases; stage-local BA/Top(role)/RC(role) fit calls reference `engine_fit` or `engine_rc` by family, fixture and call/candidate index when the exact request exists there; request `g` and `corr_db` SHA-256 hashes pin NumPy float64 little-endian arrays, scalar arguments and returned bands are checked before export, and Rust checks hashes before replay. Only 36 new calls are stored; 230 reference existing families. Sampled before/combined/after/F/top_P/sub sum/target and simulator responses use the sorted union of **15–600 Hz inclusive bins whose zero-based index is divisible by 8, and every 257th zero-based bin** (Rust reconstructs indices). Full display curves and 17 metrics, FIR digests and peak-neighbourhood samples, all output bands and unrounded typed notes remain checked. Sidecar diff previews keep eight first/last values; complete arrays and SHA-256 remain unchanged. 5,233,953 bytes, ≤6 MB. Room post-BA is report-only; foreign-libm downstream DE/threshold/argmax choices are reported, while stable pre-BA values, configuration, top PEQ and axis identities are asserted. |
| `engine_xo` | `export_engine_xo.py` | P12b scores and typed sub-combine outputs for all ten `engine_design` cases; cached XO curves for every group in Dolby/DBA/filtered room, 50/80/115/150 Hz, corrected/raw and group/one-role. Room/Dolby recommendation tables and winners from eight fresh e2e-worker configurations (at most two processes); 2,188,553 bytes, ≤3 MB. `pick_xo` scores and winners are exact; stable replay values use 1.5× measured errors floored at PLATFORM_RTOL. Inherited room BA divergence is explicitly reported. Production tables use ADR 0004 caps and both binomial gates; best is asserted when the reference gap exceeds three gap-envelope widths. The cache-only call is timed against R7's 0.2 s. Optional `SECS_XO_RUN_CACHE` resumes completed environment draws from a caller-owned temporary directory. |
| `engine_e2e` | `export_engine_e2e.py` | P12a eight fresh subprocess configurations (reference, Haswell, SkylakeX, SIMD, four seeded float32-IR ULP perturbations), at most two concurrent. Dolby/room/DBA complete scalar results and 17 metrics; 161,839 bytes, ≤1 MB. ADR 0004 caps use `3*max(range,q)`; percentage q is 100 divided by the actual linear design-grid band count, turns is continuous, and other q is `1e-9*max(1,abs(c0))`. Neutral descriptors/chain scalars have two-sided caps and are excluded from directional binomial counts. Strict worse-than-all and worse-than-median counts use the exact binomial CDF's 99.9% quantile; the latter includes only nonzero ranges. Both oracle and forced-foreign libm paths pass; four `E2E_125` exceedances remain reports. Test-profile runtime is below 60 s, so the test is not ignored. Engine cancellation is independently incomplete (a single production fit can exceed one second). |
| `engine_rc` | `export_engine_rc.py` | P09: six shared cases plus Dolby/room GD-cut runs; comb rebuild recipes, both phase candidates, full recorded PEQ calls, mask ranges, IR peaks, realization ladders and the actual per-candidate GD comparison scores. All sampled spectra/design arrays use the sorted unique design-grid indices whose frequencies are in **15–600 Hz inclusive or whose zero-based index is divisible by 257**; indices are reconstructed, not stored. Only the chosen L candidate in `dolby`, `dolby_linear`, `dolby_ultra` keeps a full FIR; every other FIR keeps the sorted union of indices within **±512 inclusive of its first largest-magnitude tap and every 64th zero-based tap**, plus length, peak index, NumPy pairwise sum and sum of squares. Losing candidates retain fits, masks, peaks, ladder, GD score, notes and sparse FIR, but no sampled design/response. Continuous checks retain their measured budgets floored at PLATFORM_RTOL; integer decisions and injected bands match on oracle libm. IR/FIR argmax, ladder thresholds and GD candidate choice are reported off oracle libm. GD cuts use an accepted seam: P07 gates `fit_gd_cuts`' inner fits on SECS 2's spread; P09 checks its corrected input and application of recorded cuts, not the optimizer again. Budget 6 MB (5,959,155 bytes): everything left is evidence a test reads. |
| `engine_fit` | `export_engine_fit.py` | Actual calibration PEQ calls, exact greedy seeds and ring boxes, lossless per-trial replay, long trace digests; main XO reconstruction identities; GD sampled inputs and inner solver results. 6,601,389 bytes in 473 files (≤7 MB); twelve-run sensitivity envelopes for 97 PEQ / 59 GD-inner fits; 90/97 PEQ cost gates pass, feature/GD gates still partial (see above). |
| `engine_inputs` | `export_engine_inputs.py` | Ten shared engine cases, float32 IRs stored once, observed make_engine constructor/configuration values, raw settings and project summary |
| `export_mdat` | `export_export_mdat.py` | Staged and before/after REW projects for `dolby`, `room_dba`, `room_manual` and `room_filters`: exact labels, filenames, item order/text/timing/counts, sampled float32 IRs, full IR and `.mdat` SHA-256 reports, serialized lengths and parsed schema. NumPy pocketfft and Rust realfft differ in last-bit IR samples, so full hashes are reports while sampled values use a measured float32 roundoff gate. 225,047 bytes, ≤4 MB. |
| `calibration` | `export_calibration.py` | Settings normalization and storage bytes, key equivalence classes, mode parameters, shared-case settings/notes and adapter edge cases |
| `engine_prep` | `export_engine_prep.py` | Constructor normalisation, exact onsets/window starts, arrival/alignment/base, sampled H/H0 and long 2000/500 ms spectra, position/frequency weights, main levels/reference, Schroeder and target samples; `_delay` and `_sub_sum`/`_term` bit for bit on synthetic spectra, sampled on real ones |
| `engine_decay` | `export_engine_decay.py` | P08: all ten actual calibration runs, 742 mode-T60 calls (including 15 edges), 101 late-peak calls (including 9 edges); four BA setups with raw and three seeded random-row sums (40 short spectra); 34 rebuilt long spectra and four delay×PEQ recipes for XO late energy; 17 rebuilt H0 scorecards and eight recipe/norm variants, all 17 metrics. 1,969,077 bytes in 163 files, calls bundled per kind and case (≤3 MB). Percentages, peak frequencies and peak lists are exact on oracle libm. Continuous budgets are 1.5× measured, floored at PLATFORM_RTOL; pre_db below −250 dB has a separate 1.5×1.4484069100264113e−5 relative budget because FFT roundoff dominates its energy. Foreign-libm flat-response GD argmax and single-spike late-peak ties are report-only. |
| `engine_ba` | `export_engine_ba.py` | P10: eleven complete calibration runs (seven base cases, room full all-pass/fixed-gain/bus-fix and manual fixed-gain). Layout candidate costs and winner, plans, BA setup, all DE costs plus evaluation digests and first two/last generations, recorded bundle/bus fits, two-way iterations, sampled sums with recipes, bus candidate digest, folded rows and labels. ≤6 MB. Objective relative budget 1e-13 (measured 5.064e-14); replay restores the oracle population using every recorded energy, then checks Rust selection outside twice that budget. Global room DE diverges at evaluation 3713 and is report-only; the other first-pass paths match on oracle libm. Bus-fix second-pass matching is conditional on the recorded first-pass solution and PEQ. D1 is not applied. |
| `camilla` | `export_camilla.py` | CamillaDSP v4 YAML and float32 FIR WAV bytes from seven computed chains plus direct no-sub/LFE/device/rounding cases; Python float repr, half-even rounding, `g` and fixed formats on edge values |
| `filters` | `export_filters.py` | Complete filter plans, sampled complex responses, headroom and Windows channels; exact Korean BOM text, Equalizer APO ASCII configuration/body, BOM guide and float32 FIR WAV bytes for RC, BA, no-LFE, rounding and FIR/IIR MIMO cases. 921,218 bytes (≤6 MB). |
| `libm` | `export_libm.py` | float64 `ln`, `log10`, `log2`, `exp`, `sin`, `cos`, `tan`, `atan`, `tanh`, `atan2`, `pow` at fixed points |
| `mdat` | `export_mdat.py` | bounded Java serialization parsing, Modified UTF-8, full REW model adaptation, byte-identical round trips, and deterministic 1- and 3-measurement builds (one from three real Dolby-room IRs); built models are frozen as digests |
| `project` | `export_project.py` | role vocabulary, `.secs` presets and migrations, Python JSON bytes, tap allocation, target curves, float32 WAV bytes and readers |
