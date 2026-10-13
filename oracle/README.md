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
~/.venvs/secs-oracle/bin/python oracle/run.py libm mdat project dsp
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
On the room fits, trial 81's actual reduction differs by 8.579e-9, of which
8.578e-9 is the cost difference of a one-ulp `x_new` difference; the largest
unexplained remainder over all trials is 2.7e-12.

## Families

| Family | Exporter | What is frozen |
|---|---|---|
| `dsp` | `export_dsp.py` | P03 grid, smoothing, cepstral minimum phase, analog/RBJ responses, GD, digital Butterworth SOS, FIR, peaks, low-rate projection; N=131072 sr=48000 room L spectrum from sample_dolby_714.npz. Smoothing outputs keep bins 0–1799, every 127th higher bin and Nyquist (computed on the full grid); all other outputs are complete. TRF includes caps150/400, real-room 3-band fits that exhaust both caps, all residual/probe calls and Jacobian bases. Every oracle trial records x/Delta/carried alpha, step, x_new, reductions, ratio, updated radius and accept/Jacobian-update decisions for local replay. Trials 0–2 of peq150/room150 keep full intermediates; global traces are reports. |
| `libm` | `export_libm.py` | float64 `ln`, `log10`, `log2`, `exp`, `sin`, `cos`, `tan`, `atan`, `tanh`, `atan2`, `pow` at fixed points |
| `mdat` | `export_mdat.py` | bounded Java serialization parsing, Modified UTF-8, full REW model adaptation, byte-identical round trips, and deterministic 1- and 3-measurement builds (one from three real Dolby-room IRs); built models are frozen as digests |
