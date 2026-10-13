"""Shared SciPy TRF observation, extracted without changing DSP fixtures.

TRF interposition follows Impulcifer-py313
 tests/migration/export_goldens_secs2.py (MIT, Copyright (c) 2024- 115dkk).
See docs/THIRD_PARTY.md. Observers never replace residual or solver arithmetic.
"""
import importlib
import sys
import numpy as np
from scipy import optimize

# Capture all actual residual calls plus Jacobian base points. This instruments
# SciPy without altering the residual, solver, numerical differentiation or SVD.
lsmod=importlib.import_module('scipy.optimize._lsq.least_squares')
trfmod=importlib.import_module('scipy.optimize._lsq.trf')
original_trf=lsmod.trf
original_svd=trfmod.svd

def replay(fun, x0, lb, ub, cap, tols):
    """Observe original SciPy call boundaries; never recompute a solver quantity."""
    trials = []
    steps = []
    original = lsmod.trf
    def observed_trf(fun, jac, *args, **kwargs):
        def observed_jac(*args, **kwargs):
            result = jac(*args, **kwargs)
            steps[-1]['jacobian_recomputed'] = True
            return result
        return original(fun, observed_jac, *args, **kwargs)
    solve = trfmod.solve_lsq_trust_region
    update = trfmod.update_tr_radius
    def trust(*args, **kwargs):
        parent = sys._getframe(1).f_locals
        result = solve(*args, **kwargs)
        steps.append(dict(x=parent['x'].copy(), Delta=parent['Delta'],
                          alpha=parent['alpha'], jacobian_recomputed=False))
        if len(trials) < 3:
            trial = {key: np.copy(parent[key]) for key in
                     ('x', 'f', 'J', 'g', 'g_norm', 'v', 'dv', 'd', 'J_h',
                      'diag_h', 'g_h', 'theta', 'Delta', 's', 'uf', 'V')}
            trial.update(alpha_in=parent['alpha'], alpha_out=result[1],
                         n_iter=result[2], p_h=result[0].copy())
            trials.append(trial)
        return result
    def radius(*args, **kwargs):
        parent = sys._getframe(1).f_locals
        result = update(*args, **kwargs)
        steps[-1].update({key: np.copy(parent[key]) for key in
                          ('step', 'x_new', 'actual_reduction', 'predicted_reduction')})
        steps[-1].update(Delta_new=result[0], ratio=result[1],
                         accepted=bool(parent['actual_reduction'] > 0))
        if trials and 'Delta_new' not in trials[-1]:
            trials[-1].update({key: np.copy(parent[key]) for key in
                              ('step', 'step_h', 'x_new', 'actual_reduction',
                               'predicted_reduction')})
            trials[-1].update(Delta_new=result[0], ratio=result[1])
        return result
    lsmod.trf = observed_trf
    trfmod.solve_lsq_trust_region = trust
    trfmod.update_tr_radius = radius
    try:
        optimize.least_squares(fun, x0, bounds=(lb, ub), max_nfev=cap, **tols)
    finally:
        lsmod.trf = original
        trfmod.solve_lsq_trust_region = solve
        trfmod.update_tr_radius = update
    assert all('Delta_new' in t for t in steps)
    return trials, steps

def traced(fun,x0,lb,ub,cap,tols,driver=None):
    trace=dict(evaluations=[],costs=[],jac_points=[],main=[],main_cost=[],jac=[],J0=[])
    def residual(x):
        y=fun(x)
        trace['evaluations'].append(x.copy());trace['costs'].append(.5*float(np.dot(y,y)))
        return y
    def trf(fun,jac,x0,f0,J0,*a,**kw):
        trace['jac_points'].append(x0.copy());trace['J0']=J0.copy()
        if driver == 'fortran': J0=np.asfortranarray(J0)
        def main(x):
            y=fun(x);trace['main'].append(x.copy());trace['main_cost'].append(.5*float(np.dot(y,y)));return y
        def j(x,*a):
            trace['jac_points'].append(x.copy());trace['jac'].append(x.copy());j=jac(x,*a);return np.asfortranarray(j) if driver=='fortran' else j
        return original_trf(main,j,x0,f0,J0,*a,**kw)
    lsmod.trf=trf
    if driver and driver != 'fortran':
        trfmod.svd=lambda *a,**kw:original_svd(*a,**kw,lapack_driver=driver)
    try: sol=optimize.least_squares(residual,x0,bounds=(lb,ub),max_nfev=cap,**tols)
    finally: lsmod.trf=original_trf;trfmod.svd=original_svd
    return sol,trace

