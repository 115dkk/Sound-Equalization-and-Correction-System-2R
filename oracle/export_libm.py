"""libm family: the oracle's float64 transcendental functions at fixed points.

secs-oracle's `oracle_libm()` compares Rust's f64 functions with these; when
they agree bit for bit (glibc, as on the oracle machine) the exact parity
budgets apply, otherwise they widen (crates/secs-oracle).
"""
import numpy as np

import common

fam = common.Family("libm")
rng = np.random.default_rng(20261013)
positive = np.concatenate([np.geomspace(1e-300, 1e300, 120), rng.uniform(1e-3, 1e5, 200)])
signed = np.concatenate([np.linspace(-700.0, 700.0, 141), rng.uniform(-30.0, 30.0, 200)])
angles = np.concatenate([np.linspace(-1e4, 1e4, 101), rng.uniform(-2 * np.pi, 2 * np.pi, 200)])
cases = {
    "ln": [(x, np.log(x)) for x in positive],
    "log10": [(x, np.log10(x)) for x in positive],
    "log2": [(x, np.log2(x)) for x in positive],
    "exp": [(x, np.exp(x)) for x in signed],
    "sin": [(x, np.sin(x)) for x in angles],
    "cos": [(x, np.cos(x)) for x in angles],
    "tan": [(x, np.tan(x)) for x in angles],
    "atan": [(x, np.arctan(x)) for x in signed],
    "tanh": [(x, np.tanh(x)) for x in signed],
    "atan2": [(y, x, np.arctan2(y, x)) for y, x in zip(rng.uniform(-5, 5, 200), rng.uniform(-5, 5, 200))],
    "pow": [(b, e, np.power(b, e)) for b, e in zip(rng.uniform(1e-3, 50, 200), rng.uniform(-8, 8, 200))],
}
fam.save("probe", inputs={"seed": 20261013}, outputs={"cases": cases})
fam.finish()
