"""Shared helpers of the oracle exporters (oracle/README.md, docs/adr/0004).

An exporter is ``oracle/export_<family>.py``. It imports this module, calls
upstream SECS 2 (``src/secs``, the oracle) on fixed inputs, and writes one
JSON fixture per case into ``oracle/goldens/<family>/`` through ``Family``:

    import common
    fam = common.Family("dsp")
    fam.save("peaking_0", inputs={...}, outputs={...})
    fam.finish()          # refuses stale files, prints the totals

Adapted from Impulcifer-py313 tests/migration/export_goldens_secs2.py
(MIT, Copyright (c) 2024- 115dkk); see docs/THIRD_PARTY.md.
"""
from __future__ import annotations

import hashlib
import json
import math
import os
import platform
import sys
import tomllib
from pathlib import Path
from typing import Any, Callable

import numpy as np
import scipy

ROOT = Path(__file__).resolve().parents[1]
GOLDENS = Path(__file__).resolve().parent / "goldens"
SRC = ROOT / "src"

#: Every fixture file stays below this many bytes; longer data goes to sidecars.
LIMIT = 200_000
#: Values per float64 sidecar (192 kB); longer arrays are a list of sidecars.
CHUNK = 24_000
#: Arrays up to this many values stay inline in the JSON.
INLINE = 64

sys.path.insert(0, str(SRC))


def _upstream() -> str:
    with open(ROOT / "UPSTREAM.toml", "rb") as fh:
        up = tomllib.load(fh)["upstream"]
    return f"SECS 2 {up['version']} ({up['commit'][:7]})"


def check_libm() -> None:
    """NumPy's float64 transcendental functions must be the C library's.

    oracle/run.py sets NPY_DISABLE_CPU_FEATURES; running an exporter directly
    on a CPU with AVX2/AVX-512 would give SIMD log10/exp that differ from the
    C library in the last bit, and a capped trust-region fit amplifies that.
    """
    x = np.linspace(1e-3, 1e3, 20001)
    if not all(float(a) == math.log10(float(b)) for a, b in zip(np.log10(x), x)):
        raise SystemExit("NumPy dispatches SIMD log10; run the exporter through oracle/run.py")
    y = np.linspace(-50.0, 50.0, 20001)
    if not all(float(a) == math.exp(float(b)) for a, b in zip(np.exp(y), y)):
        raise SystemExit("NumPy dispatches SIMD exp; run the exporter through oracle/run.py")
    if os.environ.get("OPENBLAS_NUM_THREADS") != "1":
        raise SystemExit("BLAS must run single-threaded; run the exporter through oracle/run.py")


META = {
    "oracle": _upstream(),
    "python": platform.python_version(),
    "numpy": np.__version__,
    "scipy": scipy.__version__,
    "numpy_dispatch": "baseline (C library transcendental functions)",
}


def plain(value: Any) -> Any:
    """JSON-safe form: float64 at repr precision, complex as [re, im], nonfinite as strings."""
    if isinstance(value, np.ndarray):
        return plain(value.tolist())
    if isinstance(value, dict):
        return {str(k): plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [plain(v) for v in value]
    if isinstance(value, (complex, np.complexfloating)):
        return [plain(float(value.real)), plain(float(value.imag))]
    if isinstance(value, (bool, np.bool_)):
        return bool(value)
    if isinstance(value, (int, np.integer)):
        return int(value)
    if isinstance(value, (float, np.floating)):
        value = float(value)
        if math.isnan(value):
            return "nan"
        if math.isinf(value):
            return "+inf" if value > 0 else "-inf"
        return value
    return value


class Family:
    """The fixtures of one family, written under oracle/goldens/<name>/."""

    def __init__(self, name: str) -> None:
        check_libm()
        self.name = name
        self.dir = GOLDENS / name
        self.dir.mkdir(parents=True, exist_ok=True)
        self.written: dict[str, int] = {}

    # -- files ------------------------------------------------------------
    def _write(self, filename: str, data: bytes, limit: int | None = LIMIT) -> None:
        if filename in self.written:
            raise AssertionError(f"{self.name}/{filename} written twice")
        if limit is not None and len(data) >= limit:
            raise AssertionError(f"{self.name}/{filename} is {len(data)} bytes (limit {limit})")
        path = self.dir / filename
        if not path.exists() or path.read_bytes() != data:
            path.write_bytes(data)
        self.written[filename] = len(data)

    def save(self, case: str, inputs: Any, outputs: Any, meta: dict | None = None) -> None:
        """One fixture: {"inputs", "outputs", "meta"} as compact JSON."""
        payload = plain({"inputs": inputs, "outputs": outputs, "meta": {**META, **(meta or {})}})
        text = json.dumps(payload, allow_nan=False, ensure_ascii=False, separators=(",", ":")) + "\n"
        self._write(f"{case}.json", text.encode("utf-8"))

    # -- sidecars ---------------------------------------------------------
    def f64(self, key: str, values: Any) -> Any:
        """A float64 array: inline when short, else a sidecar descriptor (or a list of them)."""
        arr = np.ascontiguousarray(np.asarray(values, dtype="<f8").ravel())
        if arr.size <= INLINE:
            return arr
        if arr.size > CHUNK:
            return [self.f64(f"{key}_{i}", arr[i * CHUNK:(i + 1) * CHUNK])
                    for i in range(-(-arr.size // CHUNK))]
        data = arr.tobytes()
        self._write(f"{key}.f64", data)
        return {"file": f"{key}.f64", "length": int(arr.size),
                "sha256": hashlib.sha256(data).hexdigest(), "first": arr[:32], "last": arr[-32:]}

    def c128(self, key: str, values: Any) -> Any:
        """A complex array as {"re", "im"} float64 arrays."""
        arr = np.asarray(values, dtype=np.complex128).ravel()
        return {"re": self.f64(f"{key}_re", arr.real), "im": self.f64(f"{key}_im", arr.imag)}

    def blob(self, key: str, data: bytes, suffix: str = "bin") -> dict:
        """Raw bytes (a file the port must reproduce byte for byte)."""
        filename = f"{key}.{suffix}"
        self._write(filename, bytes(data), limit=None)
        return {"blob": filename, "length": len(data), "sha256": hashlib.sha256(data).hexdigest()}

    # -- end --------------------------------------------------------------
    def finish(self) -> None:
        stale = sorted(p.name for p in self.dir.iterdir() if p.name not in self.written)
        if stale:
            raise SystemExit(f"stale files in oracle/goldens/{self.name}: {stale}; delete them")
        total = sum(self.written.values())
        print(f"{self.name}: {len(self.written)} files, {total} bytes")


# -- interposition (observing intermediate values without copying the oracle)
class Recorder:
    """Calls observed while a session is open; nested calls of a recorded function are not recorded."""

    def __init__(self) -> None:
        self.calls: list[dict] | None = None
        self.depth = 0

    def open(self) -> None:
        self.calls = []

    def close(self) -> list[dict]:
        calls, self.calls = self.calls or [], None
        return calls


REC = Recorder()


def interpose(module: Any, name: str, label: str | None = None) -> Callable:
    """Wrap ``module.name`` so calls are recorded in ``REC`` while a session is open."""
    original = getattr(module, name)

    def wrapper(*args, **kwargs):
        if REC.calls is None or REC.depth:
            return original(*args, **kwargs)
        REC.depth += 1
        try:
            out = original(*args, **kwargs)
        finally:
            REC.depth -= 1
        REC.calls.append({"label": label or name, "args": args, "kwargs": kwargs, "out": out})
        return out

    wrapper.__wrapped__ = original
    setattr(module, name, wrapper)
    return original
