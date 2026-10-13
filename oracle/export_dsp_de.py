"""P04: observe NumPy legacy RandomState and SciPy's serial best1bin path.

References (BSD-3-Clause): NumPy v2.5.3 numpy/random/mtrand.pyx,
_mt19937.pyx, src/mt19937, src/distributions; SciPy 1.18.1
scipy/optimize/_differentialevolution.py. Costs below are synthetic BA-shaped
objectives; no optimizer or RNG formulas are reproduced in this exporter.
"""
import warnings

import numpy as np
import scipy
from scipy.optimize import differential_evolution

import common

assert np.__version__ == "2.5.3"
assert scipy.__version__ == "1.18.1"
fam = common.Family("dsp_de")


def trace(key, values):
    """A DE trace: full values up to 4096 (for reading diffs), else a digest.

    DE is exact on the oracle's libm, so the SHA-256 of the whole evaluation
    sequence is as strong a gate as the values; long traces keep only that.
    """
    arr = np.asarray(values, dtype="<f8").ravel()
    return fam.f64(key, arr) if arr.size <= 4096 else fam.f64_digest(arr)


for seed in [0, 1, 2, 12345, 2**32 - 1]:
    name = f"rs_{seed}"
    raw = np.random.RandomState(seed).randint(0, 2**32, size=5000, dtype=np.uint32)
    samples = np.random.RandomState(seed).random_sample(5000)
    rng = np.random.RandomState(seed)
    operations = []

    def record(kind, args, values):
        key = f"{name}_op{len(operations)}"
        operations.append(dict(kind=kind, args=args, values=fam.f64(key, values)))

    # Scalar randint is the fill-point draw. Include the no-draw high=1 case,
    # powers of two and rejecting bounds, then the 64-bit path as an API edge.
    for high in [1, *range(2, 13), 16, 17, 256, 257, 2**32, 2**32 + 1]:
        record("randint", [high], [rng.randint(high, dtype=np.int64) for _ in range(257)])
    for low, high in [(0., 1.), (.5, 1.), (-3., 7.), (2., 2.)]:
        record("uniform", [low, high], rng.uniform(low, high, 500))
    for n in [0, 1, 5, *range(24, 145, 12)]:
        record("permutation", [n], rng.permutation(range(n)))
        indices = np.arange(n)
        for _ in range(3):
            rng.shuffle(indices)
            record("shuffle", [n], indices)
    record("random_sample", [], rng.random_sample(1000))
    fam.save(name, dict(seed=seed), dict(
        raw=fam.blob(name + "_raw", raw.astype("<u4").tobytes()),
        samples=fam.f64(name + "_samples", samples), operations=operations))


def objective(kind, x):
    # Explicit scalar accumulation is shared by both tests, isolating the solver
    # from differences in vectorized objective reductions / math libraries.
    total = 0.
    if kind == "ba":
        for j, value in enumerate(x):
            target = .125 * (j + 1) * (1 if j % 2 == 0 else -1)
            error = float(value) - target
            total += (1. + .25 * j) * error * error
            total += .03125 * abs(error)
        for j in range(1, len(x)):
            delta = float(x[j]) - float(x[j - 1])
            total += .0625 * abs(delta)
        return total
    if kind == "rosen":
        for j in range(len(x) - 1):
            a = float(x[j + 1]) - float(x[j]) * float(x[j])
            b = 1. - float(x[j])
            total += 100. * a * a + b * b
        return total
    if kind == "plateau":
        for value in x:
            total += float(np.floor(abs(float(value)) * 2.))
        return total
    if kind == "constant":
        return 7.
    if kind == "offset":
        return 10. + objective("ba", x)
    if kind == "nan":
        return np.nan
    if kind == "inf":
        return np.inf
    raise AssertionError(kind)


def export(name, kind, bounds, seed, x0=None, maxiter=12, popsize=12, tol=1e-6):
    evaluations, costs = [], []

    def cost(x):
        evaluations.append(x.copy())
        value = objective(kind, x)
        costs.append(value)
        return value

    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        result = differential_evolution(cost, bounds, seed=seed, x0=x0,
                                        maxiter=maxiter, popsize=popsize,
                                        tol=tol, polish=False)
    assert len(evaluations) == result.nfev
    fam.save("de_" + name, dict(kind=kind, bounds=bounds, seed=seed, x0=x0,
                              maxiter=maxiter, popsize=popsize, tol=tol), dict(
        x=result.x, fun=result.fun, nit=result.nit, nfev=result.nfev,
        success=result.success, message=result.message,
        evaluations=trace("de_" + name + "_evaluations", evaluations),
        costs=trace("de_" + name + "_costs", costs)), dict(
            trace="every cost call, including initial population and every trial; row-major"))
    return result


for n in range(2, 13):
    bounds = [(-3. - .125 * j, 4. + .25 * j) for j in range(n)]
    for seed in [1, 2]:
        for initial in [False, True]:
            x0 = [b[0] if j % 2 == 0 else b[1] for j, b in enumerate(bounds)] if initial else None
            export(f"ba{n}_s{seed}_x{int(initial)}", "ba", bounds, seed, x0)
for kind in ["rosen", "plateau"]:
    for n in [2, 7, 12]:
        for seed in [1, 2]:
            export(f"{kind}{n}_s{seed}", kind, [(-2., 3.)] * n, seed,
                   [0.] * n if seed == 2 else None, maxiter=60)
converging = export("converging", "offset", [(-3., 4.)] * 4, 1, maxiter=500)
assert converging.success and 1 < converging.nit < 500
capped = export("capped", "ba", [(-3., 4.)] * 7, 2, maxiter=60)
assert not capped.success and capped.nit == 60
export("zero_iterations", "ba", [(-3., 4.)] * 2, 1, maxiter=0)
export("constant", "constant", [(-3., 4.)] * 2, 2, maxiter=1)
export("equal_bounds", "ba", [(1., 1.), (-2., 3.), (0., 0.)], 1)
export("all_equal", "ba", [(1., 1.), (2., 2.)], 2)
export("reversed", "ba", [(3., -2.), (-2., 3.)], 2, [1., -2.])
export("minimum_population", "ba", [(-3., 4.)] * 2, 1, popsize=0)
export("one_dimension", "ba", [(-3., 4.)], 2)
# Keep the NaN cost array inline: the shared descriptor audit uses numeric
# equality for sidecar preview values (NaN != NaN). All calls remain recorded.
export("nan", "nan", [(-3., 4.)] * 2, 1, maxiter=2, popsize=5)
export("inf", "inf", [(-3., 4.)] * 2, 2, maxiter=2)
fam.finish()
