#!/usr/bin/env python3
"""Run oracle exporters with the arithmetic the goldens assume (docs/adr/0004).

    <oracle venv>/bin/python oracle/run.py mdat project dsp

For each family name, runs ``oracle/export_<family>.py`` in a fresh process
with:

- ``NPY_DISABLE_CPU_FEATURES`` set to every SIMD feature NumPy would dispatch
  above its baseline on this CPU, so float64 ``log10``/``exp`` and friends are
  the C library's, the same functions Rust's ``f64`` methods call on Linux;
- BLAS/LAPACK and OpenMP pinned to one thread;
- ``PYTHONHASHSEED=0`` so set iteration order cannot leak into a fixture.

The upstream tree in this repository (``src/``) is the oracle; CI keeps it
byte-identical to ``UPSTREAM.toml``'s commit. Exit status is the first failing
exporter's.
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
PROBE = ("import numpy._core._multiarray_umath as m; "
         "print(' '.join(f for f in m.__cpu_dispatch__ if m.__cpu_features__.get(f)))")


def environment() -> dict[str, str]:
    found = subprocess.run([sys.executable, "-c", PROBE], check=True, capture_output=True,
                           text=True).stdout.strip()
    env = dict(os.environ)
    env.update({
        "NPY_DISABLE_CPU_FEATURES": found,
        "OPENBLAS_NUM_THREADS": "1",
        "OMP_NUM_THREADS": "1",
        "MKL_NUM_THREADS": "1",
        "PYTHONHASHSEED": "0",
        "PYTHONDONTWRITEBYTECODE": "1",
    })
    return env


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__)
        return 2
    env = environment()
    print(f"NPY_DISABLE_CPU_FEATURES={env['NPY_DISABLE_CPU_FEATURES']!r}")
    for family in argv:
        script = HERE / f"export_{family}.py"
        if not script.is_file():
            print(f"no exporter {script.name}")
            return 2
        print(f"== {family}", flush=True)
        code = subprocess.run([sys.executable, str(script)], env=env).returncode
        if code:
            return code
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
