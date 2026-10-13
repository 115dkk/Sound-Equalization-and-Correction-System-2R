"""P15: observe SciPy's analytic-Jacobian bounded exact TRF.

Observation follows oracle/trf_trace.py; no solver arithmetic is replaced.
MIMO derivatives follow src/secs/mimo_iir.py:_solve_problem (the repository's
GPL-3.0 upstream); SciPy observers use the existing BSD-3-Clause attribution
in docs/THIRD_PARTY.md. Run with the oracle venv: oracle/run.py trf_normal.
"""
import importlib
import sys
import numpy as np
from scipy.optimize import least_squares
import common
from secs.correction_engine import peaking_response


def small():
    t = np.tile(np.linspace(0.0, 2.0, 50), 4)
    truth = np.array([1.4, .7, .9, 1.1, 1.8, .45, 1.1, 1.6])
    x0 = np.array([.8, 1.0, 1.2, .6, 1.0, .9, .7, 1.0])
    lb = np.tile([.1, .1], 4)
    ub = np.tile([2.0, 2.5], 4)
    def prediction(x):
        return np.concatenate([x[2*k] * np.exp(-x[2*k+1] * t[k*50:(k+1)*50]) for k in range(4)])
    target = prediction(truth) + .003 * np.sin(np.arange(200) * 1.37)
    def fun(x):
        return prediction(x) - target
    def jac(x):
        j = np.zeros((200, 8))
        for k in range(4):
            tt = t[k*50:(k+1)*50]
            e = np.exp(-x[2*k+1] * tt)
            j[k*50:(k+1)*50, 2*k] = e
            j[k*50:(k+1)*50, 2*k+1] = -tt * x[2*k] * e
        return j
    return dict(kind='small', t=t, target=target), fun, jac, x0, lb, ub, 150


def mimo():
    nf, n, sr = 512, 384, 600.0
    fl = np.fft.rfftfreq(nf, 1.0 / sr)
    # Different path/position impulse responses, deterministic and broadband.
    time = np.arange(nf)
    g = np.empty((2, 3, fl.size), complex)
    for p in range(2):
        for r in range(3):
            ir = np.zeros(nf)
            ir[7 + 11*r + 3*p] = .8 + .13*r + .07*p
            ir += .05*np.exp(-time/(35+12*r+7*p))*np.sin(time*(.27+.09*r+.05*p))
            g[p, r] = np.fft.rfft(ir)
    wl = np.ones((2, n))
    we = np.zeros((2, n))
    for p in range(2):
        wl[p, :40+5*p] = .3
        we[p, :40+5*p] = .7
    x0 = np.array([v for r in range(3) for v in
                   [-8.-r, 2.+r, np.log(42.+r*2), np.log(1.3), -3.,
                    np.log(83.+r*3), np.log(2.), 2., np.log(145.+r*4), np.log(1.6), -2.]])
    truth = x0.copy()
    truth[::11] -= 1.5
    truth[1::11] += .8
    truth[4::11] -= 1.
    truth[7::11] += 1.2
    truth[10::11] -= .7
    lb = np.tile([-25., 0., np.log(20.), np.log(.3), -12., np.log(55.), np.log(.3), -12., np.log(110.), np.log(.3), -12.], 3)
    ub = np.tile([0., 12., np.log(65.), np.log(6.), 8., np.log(115.), np.log(6.), 8., np.log(220.), np.log(6.), 8.], 3)
    effort, reg = .025, .003
    def parts(x, derivative=False):
        result = []
        for r in range(3):
            v = x[r*11:(r+1)*11]
            peqs = [peaking_response(fl, np.exp(v[2+3*k]), np.exp(v[3+3*k]), v[4+3*k]) for k in range(3)]
            s = 10**(v[0]/20) * np.exp(-2j*np.pi*fl*v[1]/1000.)
            for q in peqs:
                s = s*q
            if not derivative:
                result.append(s)
                continue
            ds = np.empty((11, fl.size), complex)
            ds[0] = s*np.log(10)/20
            ds[1] = s*(-2j*np.pi*fl/1000.)
            h = 1e-4
            for k in range(3):
                vv = v[2+3*k:5+3*k]
                for c in range(3):
                    up, dn = vv.copy(), vv.copy()
                    up[c] += h
                    dn[c] -= h
                    a = peaking_response(fl, np.exp(up[0]), np.exp(up[1]), up[2])
                    b = peaking_response(fl, np.exp(dn[0]), np.exp(dn[1]), dn[2])
                    ds[2+3*k+c] = s*(a-b)/(2*h)/peqs[k]
            result.append(ds)
        return result
    ss = parts(truth)
    target = np.array([np.fft.irfft(sum(g[p,r]*ss[r] for r in range(3)), nf)[:n] for p in range(2)])
    target += .0001*np.sin(np.arange(2*n).reshape(2,n)*.73)
    def fun(x):
        ss = parts(x)
        out = []
        for p in range(2):
            add = np.fft.irfft(sum(g[p,r]*ss[r] for r in range(3)), nf)[:n]
            out.extend([wl[p]*(add-target[p]), we[p]*(add-target[p])])
        for s in ss:
            out.extend([effort*s.real, effort*s.imag])
        out.append(np.array([reg*x[r*11+4+3*k] for r in range(3) for k in range(3)]))
        return np.concatenate(out)
    def jac(x):
        dd = parts(x, True)
        j = np.zeros((4*n+6*fl.size+9, 33))
        for r in range(3):
            cols = slice(r*11,(r+1)*11)
            for p in range(2):
                c = np.fft.irfft(g[p,r][None,:]*dd[r], nf, axis=1)[:,:n]
                j[2*p*n:(2*p+1)*n,cols] = (wl[p]*c).T
                j[(2*p+1)*n:(2*p+2)*n,cols] = (we[p]*c).T
            row = 4*n+2*r*fl.size
            j[row:row+fl.size,cols] = effort*dd[r].real.T
            j[row+fl.size:row+2*fl.size,cols] = effort*dd[r].imag.T
            for k in range(3):
                j[4*n+6*fl.size+r*3+k,r*11+4+3*k] = reg
        return j
    return dict(kind='mimo', nf=nf, n=n, fl=fl, G=g, wl=wl, we=we, target=target, effort=effort, reg=reg), fun, jac, x0, lb, ub, 150


def rank_deficient():
    a = np.zeros((200, 4))
    a[:100, :2] = 1.0
    a[100:, 2] = 1.0
    target = np.r_[np.full(100, 10.), np.full(100, -.1)]
    return (dict(kind='rank', matrix=a, target=target), lambda x: a@x-target, lambda x: a.copy(),
            np.zeros(4), np.array([-np.inf, -np.inf, -1., -1.]),
            np.array([np.inf, np.inf, 1., 1.]), 4)


def observe(fun, jac, x0, lb, ub, cap, scale):
    mod = importlib.import_module('scipy.optimize._lsq.trf')
    solve, update = mod.solve_lsq_trust_region, mod.update_tr_radius
    bases, trials = [], []
    previous = None
    def trust(*args, **kwargs):
        nonlocal previous
        state = sys._getframe(1).f_locals
        if previous is not state['J']:
            previous = state['J']
            j = state['J']
            bases.append(dict(x=state['x'].copy(), cost=state['cost'],
                              normal=j.T@j, gradient=state['g'].copy(),
                              scale_inv=np.broadcast_to(state['scale_inv'], x0.shape).copy(),
                              v=state['v'].copy(), m=j.shape[0]))
        result = solve(*args, **kwargs)
        trials.append(dict(base=len(bases)-1, Delta=state['Delta'], alpha=state['alpha'],
                           alpha_out=result[1], full_rank=bool(state['s'][-1] > np.finfo(float).eps*state['m']*state['s'][0])))
        return result
    def radius(*args, **kwargs):
        state = sys._getframe(1).f_locals
        result = update(*args, **kwargs)
        trials[-1].update({key: np.copy(state[key]) for key in ('step','predicted_reduction','actual_reduction','x_new')})
        trials[-1].update(Delta_new=result[0], ratio=result[1], accepted=bool(state['actual_reduction'] > 0))
        return result
    mod.solve_lsq_trust_region, mod.update_tr_radius = trust, radius
    try:
        sol = least_squares(fun, x0, jac=jac, bounds=(lb,ub), max_nfev=cap, x_scale=scale)
    finally:
        mod.solve_lsq_trust_region, mod.update_tr_radius = solve, update
    assert len(trials) == sol.nfev-1
    return sol, bases, trials


def main():
    family = common.Family('trf_normal')
    for make in (small, mimo, rank_deficient):
        inputs, fun, jac, x0, lb, ub, cap = make()
        for scale in (('jac', 1.0) if make is not rank_deficient else ('jac',)):
            name = inputs['kind'] + ('_jac' if scale == 'jac' else '_unit')
            sol, bases, trials = observe(fun, jac, x0, lb, ub, cap, scale)
            ins = dict(inputs, x0=x0, lb=lb, ub=ub, cap=cap, x_scale=scale)
            for key, value in list(ins.items()):
                if isinstance(value, np.ndarray):
                    ins[key] = family.c128(name+'_'+key, value) if np.iscomplexobj(value) else family.f64(name+'_'+key, value)
            encoded = []
            for index, base in enumerate(bases):
                encoded.append({key: family.f64(f'{name}_base{index}_{key}', value)
                                if isinstance(value, np.ndarray) else value for key,value in base.items()})
            steps = {key: family.f64(name+'_trials_'+key, [t[key] for t in trials]) for key in trials[0]}
            family.save(name, ins, dict(x=sol.x, cost=sol.cost, nfev=sol.nfev, njev=sol.njev, status=sol.status,
                                       bases=encoded, trials=steps),
                        dict(rows=fun(x0).size, columns=x0.size, observer='Original SciPy trust/radius call boundaries; every trial and every distinct Jacobian base.'))
            print(f'{name}: shape={fun(x0).size}x{x0.size} nfev={sol.nfev} njev={sol.njev} status={sol.status} cost={sol.cost:.17g}', flush=True)
    family.finish()
    assert sum(family.written.values()) < 4*1024*1024


if __name__ == '__main__':
    main()
