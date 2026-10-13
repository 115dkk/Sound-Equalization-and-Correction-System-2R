"""P10: observe complete calibration BA stages; never substitute oracle arithmetic."""
from __future__ import annotations

import copy
import inspect
import sys
from contextlib import ExitStack
from unittest.mock import patch

import common
import numpy as np
import scipy.optimize as opt
from engine_cases import cases
from secs import calibration as cal, correction_engine as ce


def closure(fn):
    return dict(zip(fn.__code__.co_freevars, (c.cell_contents for c in fn.__closure__)))


def main():
    fam = common.Family('engine_ba')
    originals = {n: getattr(ce.CorrectionEngine, n) for n in
                 ('_layout_stage', '_ba_setup', '_align_subs', '_align_subs_manual',
                  '_match_two_way', '_bus_peq_fit', '_bus_delay', '_ba_cost')}
    original_de, original_fit = opt.differential_evolution, ce.fit_peq
    samples = np.unique(np.r_[np.arange(0, 550, 3), np.arange(550, 65537, 997)])
    selected = [c for c in cases() if c.name in ('room', 'dolby', 'dolby_infra',
                'room_dba', 'room_multi', 'room_manual', 'room_filters')]
    room = next(c for c in selected if c.name == 'room')
    runs = [(c.name, c, {}) for c in selected]
    runs += [('room_full', copy.deepcopy(room), {'sub_allpass': 'full'}),
             ('room_fixed_gain', copy.deepcopy(room), {'bundle_gain': 'fixed'}),
             ('room_bus_fix', copy.deepcopy(room), {'sub_bus_fix': True})]
    # A pure-auto room does not exercise bundle_gain, so also pin its effective path.
    manual = next(c for c in selected if c.name == 'room_manual')
    runs += [('room_manual_fixed_gain', copy.deepcopy(manual), {'bundle_gain': 'fixed'})]

    for name, case, knobs in runs:
        print(f'calibration.compute BA: {name}', flush=True)
        result = dict(fits=[], searches=[], setups=[], layouts=[], two_way=[])
        state = dict(stage=None, engine=None)
        def array(key, v):
            return fam.f64(f'{name}_{key}', v)
        def spectrum(key, by_pos):
            return [[p, fam.c128(f'{name}_{key}_{p}', h[samples])] for p, h in by_pos.items()]
        def fit(*args, **kwargs):
            value = original_fit(*args, **kwargs)
            if state['stage'] is not None:
                b = inspect.signature(original_fit).bind(*args, **kwargs)
                b.apply_defaults()
                a = b.arguments
                k = len(result['fits'])
                result['fits'].append(dict(stage=state['stage'], inputs=dict(
                    g=array(f'fit{k}_g', a['g']), corr_db=array(f'fit{k}_corr', a['corr_db']),
                    **{key: a[key] for key in ('n', 'lo', 'hi', 'boost', 'ring_ms', 'f_max')}), result=value))
            return value
        def setup(e, hi):
            s = originals['_ba_setup'](e, hi)
            result['setups'].append(dict(hi=hi, grid=s['grid'], positions=s['positions'],
                lin=s['lin'], raw_late=list(s['raw_late'].items()), base=list(s['base'].items())))
            return s
        def de(fun, bounds, *args, **kwargs):
            if state['stage'] != 'align':
                return original_de(fun, bounds, *args, **kwargs)
            xs, costs = [], []
            def cost(x):
                v = fun(x); xs.append(x.copy()); costs.append(v); return v
            r = original_de(cost, bounds, *args, **kwargs)
            n = len(result['searches']); pop = max(5, kwargs.get('popsize', 15)*len(bounds))
            keep = np.unique(np.r_[np.arange(min(len(xs),pop*3)), np.arange(max(0,len(xs)-pop),len(xs))])
            fc = closure(fun)
            bus = fc.get('P1')
            # All costs reconstruct the exact oracle population: Rust's solver
            # receives these energies while independently measuring its objective.
            result['searches'].append(dict(bounds=bounds, options={key: kwargs[key] for key in
                ('maxiter','popsize','tol','seed')}, x0=kwargs.get('x0'),
                x=r.x, fun=r.fun, nit=r.nit, nfev=r.nfev,
                costs=array(f'de{n}_costs',costs),
                evaluations=fam.f64_digest(xs), keep=keep,
                vectors=array(f'de{n}_vectors',np.asarray(xs)[keep]),
                bus_peq=(getattr(state['engine'], '_ba_record_bus_peq', None) if bus is not None else None)))
            return r
        def layout(e, hi):
            state['engine'] = e; state['stage'] = 'bundle'
            observed = []
            member_levels = {}
            target_file = originals['_layout_stage'].__code__.co_filename
            def tracer(frame, event, arg):
                if frame.f_code is originals['_layout_stage'].__code__ and event == 'line':
                    if frame.f_lineno == 928:
                        observed.append((frame.f_locals['name'], float(frame.f_locals['P']), float(frame.f_locals['cost'])))
                    if frame.f_lineno == 897:
                        member_levels[frame.f_locals['name']] = dict(frame.f_locals['levels'])
                return tracer if frame.f_code.co_filename == target_file else None
            old = sys.gettrace(); sys.settrace(tracer)
            try: value = originals['_layout_stage'](e, hi)
            finally: sys.settrace(old); state['stage'] = None
            for bundle, info in e.layout_info.items():
                trials = [v[1:] for v in observed if v[0] == bundle]
                result['layouts'].append(dict(name=bundle, info=info, levels=member_levels[bundle],
                    candidates=fam.f64_digest(trials),
                    winner_index=(next((i for i,v in enumerate(trials) if v[0] == info['auto_ms']),None)),
                    candidate_values=array('layout_'+bundle+'_candidates',trials)))
            result['layout_rel'] = list(e.layout_rel.items())
            result['sub_filters'] = list(e.sub_filters.items())
            return value
        def align(e, hi, *args):
            state['stage'] = 'align'
            method = '_align_subs_manual' if args else '_align_subs'
            try: value = originals[method](e, hi, *args)
            finally: state['stage'] = None
            result['plan'] = e.manual_plan(); result['aligned_rows'] = value
            return value
        def match(e, params, manual):
            records = []
            def tracer(frame,event,arg):
                if frame.f_code is originals['_match_two_way'].__code__ and event=='line' and frame.f_lineno==662:
                    records.append({k: frame.f_locals[k] for k in ('diff','step','total')})
                return tracer
            old=sys.gettrace();sys.settrace(tracer)
            try: value=originals['_match_two_way'](e,params,manual)
            finally: sys.settrace(old)
            result['two_way']=records;result['rows']=value
            result['raw_sums']=spectrum('raw_sums',e._sub_sum(value,np.arange(e.f.size),
                [p for p in e.positions if all(r in e.H[p] for r in e.subs)]))
            return value
        def bus_fit(e, by_pos):
            stage=state['stage'];state['stage']='bus'
            try: value=originals['_bus_peq_fit'](e,by_pos)
            finally: state['stage']=stage
            e._ba_record_bus_peq=value
            return value
        def bus_delay(e, by_pos, xo_of, lo=-15.0):
            observed=[]
            def tracer(frame,event,arg):
                if frame.f_code is originals['_bus_delay'].__code__ and event=='line' and frame.f_lineno==1093:
                    observed.append((frame.f_locals['pol'],frame.f_locals['d'],frame.f_locals['c']))
                return tracer
            old=sys.gettrace();sys.settrace(tracer)
            try: value=originals['_bus_delay'](e,by_pos,xo_of,lo)
            finally:sys.settrace(old)
            result['bus_delay']=dict(lo=lo,answer=value,candidates=fam.f64_digest(observed),
                candidate_values=array('bus_delay_candidates',observed),
                top_peq=list(e.top_peq.items()),xo_of=list(xo_of.items()),
                sums=spectrum('bus_sums',by_pos))
            return value
        original_make=cal.make_engine
        def make(*args,**kwargs):
            value=original_make(*args,**kwargs)
            for k,v in knobs.items():
                if k!='sub_bus_fix':setattr(value[0],k,v)
            return value
        with ExitStack() as stack:
            for method, fn in dict(_layout_stage=layout,_ba_setup=setup,_align_subs=align,
                                  _align_subs_manual=align,_match_two_way=match,
                                  _bus_peq_fit=bus_fit,_bus_delay=bus_delay).items():
                stack.enter_context(patch.object(ce.CorrectionEngine,method,fn))
            stack.enter_context(patch.object(ce,'fit_peq',fit))
            stack.enter_context(patch.object(opt,'differential_evolution',de))
            stack.enter_context(patch.object(cal,'make_engine',make))
            stack.enter_context(patch.object(ce,'SUB_BUS_FIX',knobs.get('sub_bus_fix',False)))
            run=cal.compute(case.project,case.irs,case.settings,subs_only=False)
        e=run.engine
        result.update(ref_level=e.ref_level,sub_level_db=e.sub_level_db,
                      bus_gain=e.sub_state['bus_gain'],bus_peq=e.sub_bus_peq,
                      bus_d=e.sub_state['bus_d'],bus_pol=e.sub_state['bus_pol'],
                      folded_rows=e.sub_state['params'],labels=[e.sub_mode(r) for r in e.subs])
        # Main-role XO order is compute's order, not group order.
        xo_of={r:e.xo[g] for g,rs in e.groups.items() for r in rs if r in e.mains and g in e.xo and e.xo[g]>0}
        result['xo_of']=list(xo_of.items())
        # Large integer vectors also use the standard lossless float64 descriptors.
        for i,s in enumerate(result['setups']):
            s['grid']=array(f'setup{i}_grid',s['grid']);s['lin']=array(f'setup{i}_lin',s['lin'])
        for i,s in enumerate(result['searches']):s['keep']=array(f'de{i}_keep',s['keep'])
        fam.save(name,dict(case=case.name,knobs=knobs,samples=samples),result)
    assert sum(fam.written.values())<=6_000_000, sum(fam.written.values())
    fam.finish()

if __name__=='__main__':main()
