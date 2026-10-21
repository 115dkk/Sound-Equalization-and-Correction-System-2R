"""Regenerate the finite, full-tail IR oracle with SciPy 1.18.1.

This is test-data generation, not an implementation dependency.
"""
import json
import math
from pathlib import Path

import numpy as np
import scipy
from scipy.signal import firwin, upfirdn

assert scipy.__version__ == "1.18.1"
signal = np.array([1.0, -0.25, 0.0, 0.125, -0.5, 0.0, 0.0, 0.75,
                   0.0, -1.0, 0.25, 0.0, 0.5, 0.0, -0.125, 0.0, -0.75])
cases = []
for source in [44100, 96000]:
    target = 48000
    common = math.gcd(source, target)
    up, down = target // common, source // common
    half = 96 * max(up, down)
    kernel = firwin(2 * half + 1, 0.96 / max(up, down), window="blackmanharris")
    prepad = (-half) % down
    result = upfirdn(np.pad(kernel, (prepad, 0)) * down, signal, up=up, down=down)
    cases.append({
        "from_hz": source, "to_hz": target, "leading": (half + prepad) // down,
        "input": signal.tolist(), "output": result.tolist(),
    })
Path(__file__).with_name("resample_ir_scipy.json").write_text(
    json.dumps({"scipy": scipy.__version__, "numpy": np.__version__, "cases": cases}, indent=2) + "\n",
    encoding="utf-8",
)
