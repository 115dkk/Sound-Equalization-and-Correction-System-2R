"""P07: observe the real calibration fitter and its upstream TRF boundaries."""
import inspect
import json
import os
import pickle
import subprocess
import sys
import types
from collections import Counter, defaultdict
from pathlib import Path

import common
import numpy as np
from scipy import optimize
from secs import calibration as cal, correction_engine as ce
from engine_cases import cases
from trf_trace import lsmod, trfmod

def bands_cost(args, bands):
    """The upstream weighted residual, evaluated at the returned (Q-clipped) bands."""
    g = args['g']
    w = np.where((g >= args['lo']) & (g <= args['hi']), 1., .15)
    r = w * (20 * np.log10(np.abs(ce.peq_response(g, bands)) + 1e-12) - args['corr_db'])
    return .5 * float(np.dot(r, r))


def perturb(values, rng):
    """One independently signed ULP per nonzero target, preserving signed zeros."""
    directions = np.where(rng.integers(0, 2, size=values.shape), np.inf, -np.inf)
    return np.where(values == 0., values, np.nextafter(values, directions))


def spread_worker():
    # Only this SIMD subprocess bypasses Family/check_libm; it never writes goldens.
    if sys.argv[2] != 'simd':
        common.check_libm()
    assert os.environ['OPENBLAS_NUM_THREADS'] == '1'
    jobs = pickle.loads(sys.stdin.buffer.read())
    results = []
    original = optimize.least_squares
    gd_code = next(c for c in ce.fit_gd_cuts.__code__.co_consts
                   if isinstance(c, types.CodeType) and c.co_name == 'res')
    for job in jobs:
        rng = (np.random.default_rng(int(sys.argv[2].removeprefix('ulp_')))
               if sys.argv[2].startswith('ulp_') else None)
        solutions = []
        def observed(fun, x0, **kw):
            sol = original(fun, x0, **kw)
            solutions.append(dict(nfev=sol.nfev, njev=sol.njev, status=sol.status))
            return sol
        optimize.least_squares = observed
        if job['kind'] == 'peq':
            args = dict(job['args'])
            if rng is not None:
                args['corr_db'] = perturb(args['corr_db'], rng)
            bands = ce.fit_peq(**args)
            stats = solutions[0] if solutions else dict(nfev=0, njev=0, status=0)
            # Compare every returned result on the same, unperturbed objective.
            results.append(dict(result=bands, cost=bands_cost(job['args'], bands), **stats))
        else:
            # Reuse upstream's exact residual code on each recorded inner-fit input,
            # not a new GD trajectory (whose later inputs would already differ).
            closure = job['closure']
            cells = tuple(types.CellType(closure[k]) for k in gd_code.co_freevars)
            fun = types.FunctionType(gd_code, ce.__dict__, closure=cells)
            objective = fun
            if rng is not None:
                # GD has no corr_db: its targets are the interpolated GD baseline
                # and mag0. Perturb those targets, not the complex input spectrum.
                baseline = np.interp(closure['gw'], closure['g'], closure['base'])
                base_ulp = perturb(baseline, rng)
                mag_ulp = perturb(closure['mag0'], rng)
                perturbed = dict(closure, g=closure['gw'], base=base_ulp, mag0=mag_ulp)
                cells = tuple(types.CellType(perturbed[k]) for k in gd_code.co_freevars)
                fun = types.FunctionType(gd_code, ce.__dict__, closure=cells)
            sol = original(fun, job['x0'], **job['kwargs'])
            bands = [(float(np.exp(sol.x[0])), float(np.exp(sol.x[1])), float(sol.x[2]))]
            residual = objective(sol.x)
            results.append(dict(result=bands, cost=.5 * float(np.dot(residual, residual)),
                                nfev=sol.nfev, njev=sol.njev, status=sol.status))
    sys.stdout.write(json.dumps(common.plain(results), allow_nan=False))


if len(sys.argv) > 1 and sys.argv[1] == '--spread':
    spread_worker()
    sys.exit(0)

fam = common.Family('engine_fit')
pending = defaultdict(list)
spread_jobs = []
spread_targets = []


def save_call(name, inputs, outputs, meta):
    inputs = dict(inputs, id=name, **meta)
    pending[(name.split('_')[0], meta['case'], meta['stage'])].append((inputs, outputs))


def add_spread(job, target):
    spread_jobs.append(job)
    spread_targets.append(target)
    target['environments'] = []
    target['perturbations'] = []
original_fit, original_gd = ce.fit_peq, ce.fit_gd_cuts
original_xo = ce.CorrectionEngine._xo_target_peq
original_ls = optimize.least_squares
active = None
gd_solutions = None
case_name = ''
counts = Counter()
summary = Counter()


def context():
    frames = inspect.stack()
    names = [f.function for f in frames]
    stage = ('bundle' if '_layout_stage' in names else 'bus' if '_bus_peq_fit' in names
             else 'xo' if '_xo_target_peq' in names else 'rc')
    role = next((str(f.frame.f_locals['role']) for f in frames
                 if f.function in ('compute', '_correction') and 'role' in f.frame.f_locals), '')
    return stage, role


def least_squares(fun, x0, **kwargs):
    if active is None:
        sol = original_ls(fun, x0, **kwargs)
        if gd_solutions is not None:
            record = dict(x0=x0.copy(), lb=kwargs['bounds'][0].copy(),
                          ub=kwargs['bounds'][1].copy(), x=sol.x.copy(), cost=sol.cost,
                          result=[(np.exp(sol.x[0]), np.exp(sol.x[1]), sol.x[2])],
                          nfev=sol.nfev, njev=sol.njev, status=sol.status)
            gd_solutions.append(record)
            add_spread(dict(kind='gd', x0=x0.copy(), kwargs=kwargs,
                            closure=inspect.getclosurevars(fun).nonlocals), record)
        return sol
    record = active
    parent = sys._getframe(1).f_locals
    trials, evaluations, costs, jac_points = [], [], [], []
    solve, update, trf = trfmod.solve_lsq_trust_region, trfmod.update_tr_radius, lsmod.trf
    def residual(x):
        y = fun(x)
        evaluations.append(x.copy())
        costs.append(.5 * float(np.dot(y, y)))
        return y
    def observed_trf(fun, jac, x, f, j, *a, **kw):
        jac_points.append(x.copy())
        record['initial_f'] = f.copy()
        record['initial_j'] = j.copy()
        def observed_jac(x, *a, **kw):
            out = jac(x, *a, **kw)
            jac_points.append(x.copy())
            trials[-1]['jacobian_recomputed'] = True
            return out
        return trf(fun, observed_jac, x, f, j, *a, **kw)
    def trust(*a, **kw):
        p = sys._getframe(1).f_locals
        out = solve(*a, **kw)
        trials.append(dict(x=p['x'].copy(), Delta=p['Delta'], alpha=p['alpha'],
                           jacobian_recomputed=False))
        return out
    def radius(*a, **kw):
        p = sys._getframe(1).f_locals
        out = update(*a, **kw)
        trials[-1].update({k: np.copy(p[k]) for k in
                          ('step', 'x_new', 'actual_reduction', 'predicted_reduction')})
        trials[-1].update(Delta_new=out[0], ratio=out[1], accepted=bool(p['actual_reduction'] > 0))
        return out
    lsmod.trf, trfmod.solve_lsq_trust_region, trfmod.update_tr_radius = observed_trf, trust, radius
    try:
        sol = original_ls(residual, x0, **kwargs)
    finally:
        lsmod.trf, trfmod.solve_lsq_trust_region, trfmod.update_tr_radius = trf, solve, update
    record.update(x0=x0.copy(), lb=kwargs['bounds'][0].copy(), ub=kwargs['bounds'][1].copy(),
                  cap=kwargs['max_nfev'], x=sol.x.copy(), cost=sol.cost, nfev=sol.nfev,
                  njev=sol.njev, status=sol.status, trials=trials,
                  evaluations=evaluations, costs=costs, jac_points=jac_points)
    if 'bands' in parent:
        record['seed'] = list(parent['bands'])
    return sol


def encode_fit(name, args, result, rec, meta):
    inputs = dict(args)
    for k in ('g', 'corr_db'):
        inputs[k] = fam.f64(name + '_' + k, inputs[k])
    out = dict(result=result, final_cost=bands_cost(args, result))
    add_spread(dict(kind='peq', args=args), out)
    if rec:
        for k in ('seed', 'x0', 'lb', 'ub', 'cap', 'x', 'cost', 'nfev', 'njev', 'status'):
            out[k] = rec[k]
        out['trace'] = {k: fam.f64_digest(rec[k]) for k in
                        ('evaluations', 'costs', 'jac_points', 'initial_f', 'initial_j')}
        for t in rec['trials']:
            rebuilt = t['x'] + t['step']
            rebuilt = np.where(rebuilt <= rec['lb'], np.nextafter(rec['lb'], rec['ub']), rebuilt)
            rebuilt = np.where(rebuilt >= rec['ub'], np.nextafter(rec['ub'], rec['lb']), rebuilt)
            assert np.array_equal(rebuilt, t['x_new'])
        out['one_step'] = {k: fam.f64(name + '_step_' + k, [t[k] for t in rec['trials']])
                           for k in ('Delta', 'alpha', 'step', 'actual_reduction',
                                     'predicted_reduction', 'ratio', 'Delta_new', 'accepted', 'jacobian_recomputed')}
        out['one_step']['initial_x'] = rec['trials'][0]['x'] if rec['trials'] else rec['x0']
        for prev, cur in zip(rec['trials'], rec['trials'][1:]):
            assert np.array_equal(cur['x'], prev['x_new'] if prev['accepted'] else prev['x'])
        out['one_step']['x_new_digest'] = fam.f64_digest([t['x_new'] for t in rec['trials']])
        summary[(meta['stage'], 'cap' if rec['status'] == 0 else 'converged')] += 1
    else:
        out['seed'] = []
        summary[(meta['stage'], 'empty')] += 1
    save_call(name, inputs, out, meta)


def fit(*a, **kw):
    global active
    bound = inspect.signature(original_fit).bind(*a, **kw)
    bound.apply_defaults()
    args = dict(bound.arguments)
    stage, role = context()
    idx = counts[(case_name, stage)]
    counts[(case_name, stage)] += 1
    keep = case_name in ('dolby', 'room') or idx < 3 or stage == 'xo'
    rec = {} if keep else None
    before, active = active, rec
    try:
        result = original_fit(*a, **kw)
    finally:
        active = before
    if keep:
        name = f'fit_{case_name}_{stage}_{idx:03}'
        encode_fit(name, args, result, rec, dict(case=case_name, stage=stage, role=role, call=idx))
    return result


def xo(self, by_pos, level_db, lo, hi, bands, boost, f_max=None):
    parent = sys._getframe(1).f_locals
    if 'ref_level' not in parent:
        return original_xo(self, by_pos, level_db, lo, hi, bands, boost, f_max)
    role = parent.get('role', '')
    gain_db = parent['ref_level'] - parent['levels'][role]
    index = counts[(case_name, 'xo')]
    result = original_xo(self, by_pos, level_db, lo, hi, bands, boost, f_max)
    save_call(f'xo_{case_name}_{index:03}',
             dict(case=case_name, role=role, positions=list(by_pos), gain_db=gain_db,
                  level_db=level_db, lo=lo, hi=hi, bands=bands, boost=boost, f_max=f_max,
                  fit=f'fit_{case_name}_xo_{index:03}'), dict(result=result),
             dict(case=case_name, stage='xo', role=role))
    return result


def gd(f, h, lo, hi, n_max=6, depth=3):
    # The combined corrected spectrum depends on not-yet-ported RC and BA.
    # Preserve diagnostic samples plus a full digest, never fake reconstruction.
    stage, role = context()
    indices = np.unique(np.r_[np.arange(0, 1800), np.arange(1800, len(f), 257), len(f)-1]).astype(int)
    global gd_solutions
    gd_solutions = []
    try:
        result = original_gd(f, h, lo, hi, n_max, depth)
        solutions = gd_solutions
    finally:
        gd_solutions = None
    name = f'gd_{case_name}_{role}'
    save_call(name, dict(lo=lo, hi=hi, n_max=n_max, depth=depth, indices=indices.tolist(),
                       h=fam.c128(name+'_h', h[indices]),
                       h_digest=dict(re=fam.f64_digest(h.real), im=fam.f64_digest(h.imag))),
             dict(result=result, solutions=solutions), dict(case=case_name, role=role, stage='gd',
                 input='sampled corrected combined response; full real/imag digests; RC/BA reconstruction deferred'))
    summary[('gd', 'calls')] += 1
    return result


ce.fit_peq, optimize.least_squares = fit, least_squares
ce.CorrectionEngine._xo_target_peq = xo
selected = {'dolby', 'dolby_zero', 'dolby_linear', 'room', 'room_filters', 'room_dba', 'room_multi'}
for case in cases():
    if case.name not in selected:
        continue
    case_name = case.name
    print('engine_fit:', case_name, flush=True)
    cal.compute(case.project, case.irs, case.settings)
# The production entry point disables GD cuts. Enable only that switch for the
# two additional real engine runs; other fit calls are not duplicated.
ce.fit_peq, ce.CorrectionEngine._xo_target_peq = original_fit, original_xo
ce.fit_gd_cuts = gd
for case in cases():
    if case.name in ('dolby', 'room'):
        case_name = case.name
        print('engine_fit: GD', case_name, flush=True)
        engine = case.engine()
        engine.gd_cuts = True
        engine.compute()
ce.fit_gd_cuts, optimize.least_squares = original_gd, original_ls
# Explicit scalar ring probes include positive, negative and zero gains.
probes = [(f, gain, ms) for f in (15., 80., 500.) for gain in (-25., -1., 0., 6., 10.) for ms in (1., 30.)]
fam.save('ring', dict(probes=probes), dict(caps=[ce._ring_q_cap(*p) for p in probes]))
# One fresh interpreter per environment, all PEQ and GD-inner calls in each.
environments = []
for label, core in [('haswell', 'Haswell'), ('skylakex', 'SkylakeX'), ('simd', None)]:
    env = dict(os.environ, OPENBLAS_VERBOSE='2')
    if core:
        env['OPENBLAS_CORETYPE'] = core
    else:
        env.pop('NPY_DISABLE_CPU_FEATURES', None)
    print('engine_fit spread:', label, flush=True)
    process = subprocess.run([sys.executable, str(Path(__file__).resolve()), '--spread', label],
                             input=pickle.dumps(spread_jobs), capture_output=True, env=env, check=True)
    cores = [line.removeprefix('Core: ').strip() for line in process.stderr.decode().splitlines()
             if line.startswith('Core: ')]
    assert cores, 'OPENBLAS_VERBOSE=2 did not report the actual core'
    if core:
        assert all(c.lower() == core.lower() for c in cores), (label, cores)
    environments.append(dict(name=label, requested_core=core, actual_cores=cores,
                             numpy_dispatch='enabled' if label == 'simd' else 'baseline'))
    results = json.loads(process.stdout)
    assert len(results) == len(spread_targets)
    for target, result in zip(spread_targets, results):
        target['environments'].append(dict(environment=label, **result))
    print('engine_fit spread cores:', label, cores, flush=True)
for seed in range(8):
    label = f'ulp_{seed}'
    print('engine_fit sensitivity:', label, flush=True)
    process = subprocess.run([sys.executable, str(Path(__file__).resolve()), '--spread', label],
                             input=pickle.dumps(spread_jobs), capture_output=True, check=True)
    results = json.loads(process.stdout)
    assert len(results) == len(spread_targets)
    for target, result in zip(spread_targets, results):
        target['perturbations'].append(dict(environment=label, **result))
for (kind, case, stage), calls in pending.items():
    # Bound each JSON below Family.LIMIT, retaining grouping by kind and case.
    for start in range(0, len(calls), 4):
        chunk = calls[start:start + 4]
        fam.save(f'{kind}_{case}_{stage}_{start // 4:02}', dict(calls=[i for i, _ in chunk]),
                 dict(calls=[o for _, o in chunk]),
                 dict(case=case, stage=stage, environments=environments))
print('engine_fit summary:', sorted(summary.items()), flush=True)
fam.finish()
assert sum(fam.written.values()) <= 7_000_000, 'engine_fit exceeds 7 MB'
