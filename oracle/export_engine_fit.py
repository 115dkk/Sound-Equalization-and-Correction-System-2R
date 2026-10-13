"""P07: observe the real calibration fitter and its upstream TRF boundaries."""
import inspect
import sys
from collections import Counter

import common
import numpy as np
from scipy import optimize
from secs import calibration as cal, correction_engine as ce
from engine_cases import cases
from trf_trace import lsmod, trfmod

fam = common.Family('engine_fit')
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
            gd_solutions.append(dict(x0=x0.copy(), lb=kwargs['bounds'][0].copy(),
                                     ub=kwargs['bounds'][1].copy(), x=sol.x.copy(), cost=sol.cost,
                                     nfev=sol.nfev, njev=sol.njev, status=sol.status))
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
    out = dict(result=result)
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
    fam.save(name, inputs, out, meta)


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
    fam.save(f'xo_{case_name}_{index:03}',
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
    fam.save(name, dict(lo=lo, hi=hi, n_max=n_max, depth=depth, indices=indices.tolist(),
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
print('engine_fit summary:', sorted(summary.items()), flush=True)
fam.finish()
assert sum(fam.written.values()) <= 6_000_000, 'engine_fit exceeds 6 MB'
