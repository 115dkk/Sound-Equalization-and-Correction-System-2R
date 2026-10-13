"""P03 primitive oracle: actual upstream calls, not reimplemented formulas.

TRF interposition follows Impulcifer-py313
 tests/migration/export_goldens_secs2.py (MIT, Copyright (c) 2024- 115dkk).
See docs/THIRD_PARTY.md. Full grids are used; selection only reduces outputs.
"""
import importlib
import sys
import warnings
import numpy as np
from scipy import signal, optimize
import common
from secs import correction_engine as ce, mimo

fam = common.Family("dsp")

def save(name, inputs, fun):
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            out = fun()
        if isinstance(out, np.ndarray):
            out = fam.c128(name + "_out", out) if np.iscomplexobj(out) else fam.f64(name + "_out", out)
        fam.save(name, inputs, {"value": out})
    except (ValueError, IndexError, ZeroDivisionError) as e:
        fam.save(name, inputs, {"error": type(e).__name__, "message": str(e)})

N, SR = 131072, 48000
f = ce.freqs(N, SR)
indices = np.unique(np.r_[np.arange(0, 1800), np.arange(1800, len(f), 127), len(f)-1]).astype(int)
room = np.load(common.ROOT / "src/secs/rewapi/sample_dolby_714.npz")
h = np.fft.rfft(room['ir_L'].astype(float), N)
fdesc = fam.f64("full_f", f)
hdesc = fam.c128("room_h", h)
power = np.abs(h)**2
pdesc = fam.f64("room_power", power)
for n, sr in [(N,SR),(14,44100),(7,600),(0,48000),(8,0)]:
    save(f"grid_{n}_{sr}",dict(n=n,sr=sr),lambda n=n,sr=sr:ce.freqs(n,sr))
for name, grid in [('full',f),('edge',np.array([-np.inf,-1.,0.,1e-3,199.,200.,2000.,np.inf,np.nan]))]:
    save('frac_'+name,dict(f=fam.f64('frac_'+name+'_f',grid)),lambda grid=grid:ce.frac_for(grid))
for name, fraction in [('scalar',24.),('variable',ce.frac_for(f))]:
    fd=fam.f64('fraction_'+name,np.atleast_1d(fraction))
    for complex_ in [False,True]:
        case='smooth_'+name+('_complex' if complex_ else '_power')
        out=ce.smooth_complex(f,h,fraction) if complex_ else ce.smooth_power(f,power,fraction)
        desc=fam.c128(case+'_out',out[indices]) if complex_ else fam.f64(case+'_out',out[indices])
        fam.save(case,dict(f=fdesc,h=hdesc,power=pdesc,frac=fd,complex=complex_,indices=fam.f64(case+'_indices',indices)),dict(value=desc),dict(selection='all bins below 659 Hz, every 127th higher bin, Nyquist; operation ran full N=131072'))
for i,(ff,pp,frac) in enumerate([([],[],24),([0],[1],24),([0,1,2],[1,2,3],np.inf),([0,1,2],[1,np.nan,3],24),([0,1,2],[1,2,3],0),([0,1,2],[1,2,3],np.nan),([0,1,2],[1,2,3],-24)]):
    ff,pp=np.array(ff,float),np.array(pp,float)
    save('smooth_edge_'+str(i),dict(f=ff,power=pp,frac=[frac],complex=False),lambda:ce.smooth_power(ff,pp,frac))
for i,imag in enumerate([np.inf,np.nan,-0.]):
    ff=np.array([0.,1.,2.]);hh=np.array([complex(1.,imag),complex(2.,0.),complex(3.,0.)])
    save('smooth_complex_edge_'+str(i),dict(f=ff,h=fam.c128('smooth_ce_'+str(i),hh),frac=[24.],complex=True),lambda:ce.smooth_complex(ff,hh,24.))
for name,mag in [('room',np.abs(h)),('nonpower',np.array([1.,2.,.1,0.,-1.,5.,1.,2.])),('two',np.array([1.,2.])),('empty',np.array([])),('one',np.ones(1)),('nan',np.array([1.,np.nan,1.]))]:
    save('minphase_'+name,dict(mag=fam.f64('mp_'+name+'_mag',mag)),lambda mag=mag:ce.minimum_phase(mag))
rf=np.array([0.,1e-9,20.,79.9,80.,80.1,150.,1000.,23999.,24000.])
for typ in ce.BAND_TYPES:
    for kind in ['LP','HP','other']:
        save('response_band_'+typ+'_'+kind,dict(fn='band',f=rf,fc=80.,kind=kind,typ=typ),lambda typ=typ,kind=kind:ce.band_filter(rf,80.,kind,typ))
for k,ff in enumerate([np.array([]),np.array([np.nan,np.inf,-np.inf,-0.])]):
    save('response_edge_'+str(k),dict(fn='peaking',f=ff,f0=80.,q=2.,gain=-6.,sr=SR),lambda:ce.peaking_response(ff,80.,2.,-6.))
for fn in ['allpass','hp2','peaking','peaking_z','peq']:
    save('response_'+fn,dict(fn=fn,f=rf,f0=80.,q=2.,gain=-6.,sr=SR,bands=[[80.,2.,-6.],[170.,.7,3.]]),lambda fn=fn: {'allpass':lambda:ce.allpass2(rf,80.,2.),'hp2':lambda:mimo.hp2(rf,80.),'peaking':lambda:ce.peaking_response(rf,80.,2.,-6.),'peaking_z':lambda:ce._peaking_response_z(np.exp(-1j*2*np.pi*rf/SR),80.,2.,-6.),'peq':lambda:ce.peq_response(rf,[(80.,2.,-6.),(170.,.7,3.)])}[fn]())
g=np.geomspace(20.,20000.,400)
save('gd_room',dict(h=hdesc,f=fdesc,g=fam.f64('gd_g',g),frac=24.),lambda:ce.gd_curve(h,f,g))
for i,(order,cutoff,kind,sr) in enumerate([(2,[48.5,51.5],'bandpass',600),(2,[1,4],'bandpass',600),(4,[20,150],'bandpass',48000),(4,[80/np.sqrt(2),80*np.sqrt(2)],'bandpass',48000),(1,[80],'lowpass',48000),(3,[80],'highpass',48000),(4,[80],'lowpass',48000),(0,[80],'lowpass',48000)]):
    sos=signal.butter(order,cutoff if len(cutoff)==2 else cutoff[0],btype=kind,fs=sr,output='sos')
    x=np.r_[1.,np.zeros(719)]
    ff=np.r_[0.,np.geomspace(1,sr/2,100),sr/2]
    name='iir_'+str(i)
    fam.save(name,dict(order=order,cutoff=cutoff,kind=kind,sr=sr,x=fam.f64(name+'_x',x),f=fam.f64(name+'_f',ff)),dict(sos=fam.f64(name+'_sos',sos),filtered=fam.f64(name+'_filtered',signal.sosfilt(sos,x)),response=fam.c128(name+'_response',signal.sosfreqz(sos,worN=ff,fs=sr)[1])))
for k,(ss,xx) in enumerate([(np.empty((0,6)),[1.]),(np.array([[1.,0,0,1,0,0]]),[]),(np.array([[1.,0,0,2,0,0]]),[1.]),(np.array([[np.nan,0,0,1,0,0]]),[1.,0.,0.]),(np.array([[1.,0,0,1,0,0]]),[np.inf,0.,np.nan])]):
    save('sos_edge_'+str(k),dict(sos=ss.ravel(),x=xx),lambda:signal.sosfilt(ss,xx))
for i,(taps,fc,fs) in enumerate([(31,150,600),(31,50,600),(30,120,600),(1,150,600),(0,150,600),(31,0,600),(31,300,600),(31,np.nan,600)]):
    save('fir_'+str(i),dict(taps=taps,fc=fc,fs=fs),lambda:signal.firwin(taps,fc,fs=fs))
for i,x in enumerate([[],[1],[0,1,0],[0,2,2,0],[0,2,2,2,0],[2,2,1,0,3],[0,np.nan,3,0],[0,np.inf,np.inf,0],[0,2,0,2,0],[-np.inf,0,-np.inf]]):
    for j,height in enumerate([None,2.,np.nan]):
        save(f'peaks_{i}_{j}',dict(x=x,height=height),lambda x=x,height=height:signal.find_peaks(x,height=height)[0])
save('lowrate_room',dict(h=hdesc,f=fdesc,n=720),lambda:mimo.to_low(h,f,720))
for k,hh in enumerate([np.array([1.+0j,complex(1,np.inf),2.+0j]),np.array([1.+0j,complex(np.nan,0),2.+0j])]):
    save('lowrate_edge_'+str(k),dict(h=fam.c128('lowedge_'+str(k),hh),f=[0.,150.,300.],n=3),lambda:mimo.to_low(hh,np.array([0.,150.,300.]),3))
for i,(gg,taps,n) in enumerate([([1,2,3,4],3,4),([1,2,3,4],5,4),([1,2,3,4],6,4),([1],2,4),([],0,0),([],1,0),([1,2],1,4)]):
    save('conv_'+str(i),dict(g=gg,taps=taps,n=n),lambda:mimo.conv_matrix(np.array(gg),taps,n).ravel())

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

cases=[]
for cap in [150,400]:
    # Real PEQ residual shape, bounded log(f),log(Q),gain, not engine fitting.
    gg=np.geomspace(20.,2000.,96)
    target=20*np.log10(np.abs(ce.peq_response(gg,[(90.,2.,-7.)]))+1e-12)
    x0=np.array([np.log(75.),np.log(1.5),-5.]);lb=np.array([np.log(15),np.log(.3),-25.]);ub=np.array([np.log(2000),np.log(20),6.])
    cases.append((f'peq{cap}','peq',x0,lb,ub,cap,{},dict(g=gg,target=target)))
for cap in [150,400]:
    gg=np.geomspace(20.,2000.,240)
    target=np.interp(gg,f,20*np.log10(np.abs(h)+1e-12))
    target=target-np.mean(target)
    x0=np.array([v for fc,q,gain in [(60.,3.,-3.),(62.,3.,-3.),(65.,3.,-3.)] for v in (np.log(fc),np.log(q),gain)])
    lb=np.tile([np.log(15),np.log(.3),-25.],3);ub=np.tile([np.log(2000),np.log(20),6.],3)
    cases.append((f'room{cap}','peq',x0,lb,ub,cap,{},dict(g=gg,target=target)))
cases += [('exhaust','rosen',np.array([-1.,1.]),np.array([-2.,-1.]),np.array([2.,3.]),2,{},{}),('active','linear',np.array([0.]),np.array([-1.]),np.array([1.]),100,{},dict(target=[2.])),('gtol','linear',np.array([.25]),np.array([-1.]),np.array([1.]),100,{},dict(target=[.25])),('ftol','inconsistent',np.array([.5]),np.array([-2.]),np.array([2.]),100,dict(ftol=1e-8,xtol=None,gtol=None),{}),('xtol','linear',np.array([.5]),np.array([-1.]),np.array([1.]),100,dict(ftol=None,xtol=1e-8,gtol=None),dict(target=[.25]))]
for name,kind,x0,lb,ub,cap,tols,extra in cases:
    def fun(x):
        if kind=='peq':
            response=np.ones(len(extra['g']),complex)
            z=np.exp(-1j*2*np.pi*extra['g']/SR)
            for k in range(0,len(x),3):
                response=response*ce._peaking_response_z(z,np.exp(x[k]),np.exp(x[k+1]),x[k+2])
            w=np.where((extra['g']>=20)&(extra['g']<=500),1.,.15)
            return w*(20*np.log10(np.abs(response)+1e-12)-extra['target'])
        if kind=='linear': return x-np.array(extra['target'])
        if kind=='rosen': return np.array([10*(x[1]-x[0]**2),1-x[0]])
        return np.array([x[0]-1.,x[0]+1.])
    sol,tr=traced(fun,x0,lb,ub,cap,tols)
    trials, steps = replay(fun, x0, lb, ub, cap, tols)
    assert len(steps) == sol.nfev - 1
    for index, step in enumerate(steps):
        assert np.array_equal(step['x_new'], tr['main'][index])
    assert sum(t['jacobian_recomputed'] for t in steps) == sol.njev - 1
    case='trf_'+name
    outputs=dict(x=sol.x,cost=sol.cost,nfev=sol.nfev,njev=sol.njev,status=sol.status)
    for key,values in tr.items(): outputs[key]=fam.f64(case+'_'+key,np.asarray(values))
    outputs['one_step'] = {
        key: fam.f64(case+'_one_step_'+key, [step[key] for step in steps])
        for key in ('x', 'Delta', 'alpha', 'step', 'x_new', 'actual_reduction',
                    'predicted_reduction', 'ratio', 'Delta_new', 'accepted',
                    'jacobian_recomputed')
    }
    ins=dict(kind=kind,x0=x0,lb=lb,ub=ub,cap=cap,tols=tols)
    for key,value in extra.items(): ins[key]=fam.f64(case+'_'+key,value)
    fam.save(case,ins,outputs,dict(trace='all residual calls including finite-difference probes; all Jacobian base points; flattened row-major',one_step='each trust-region trial at the oracle x, Delta and carried alpha; accepted and post-trial Jacobian recomputation encoded as 0/1; no residual or Jacobian stored here'))
    if name in ('peq150', 'room150'):
        replay_case = 'trf_replay_' + name
        assert len(trials) == 3
        for index, trial in enumerate(trials):
            assert np.array_equal(trial['x_new'], tr['main'][index])
        encoded = []
        for index, trial in enumerate(trials):
            encoded.append({key: fam.f64(f'{replay_case}_{index}_{key}', value)
                            if np.ndim(value) else np.asarray(value).item()
                            for key, value in trial.items()})
        fam.save(replay_case, ins, dict(trials=encoded),
                 dict(trace='first three trials observed at original SciPy trust solve and radius update boundaries; matrices row-major; V columns carry uf signs'))
fam.finish()
