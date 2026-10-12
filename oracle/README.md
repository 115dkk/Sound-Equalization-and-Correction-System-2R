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

## Families

| Family | Exporter | What is frozen |
|---|---|---|
| `libm` | `export_libm.py` | float64 `ln`, `log10`, `log2`, `exp`, `sin`, `cos`, `tan`, `atan`, `tanh`, `atan2`, `pow` at fixed points |
| `mdat` | `export_mdat.py` | bounded Java serialization parsing, Modified UTF-8, full REW model adaptation, byte-identical round trips, and deterministic 1- and 3-measurement builds (one from three real Dolby-room IRs); built models are frozen as digests |
