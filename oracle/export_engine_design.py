"""P12a: complete designs with stage-local recorded PEQ decisions."""
from __future__ import annotations

import inspect
import re
import sys
from contextlib import ExitStack
from unittest.mock import patch

import common
import numpy as np
from engine_cases import cases
from secs import calibration as cal, correction_engine as ce


def main():
    fam = common.Family('engine_design')
    f = ce.freqs()
    idx = np.flatnonzero(((f >= 15) & (f <= 600)) | (np.arange(len(f)) % 257 == 0))
    original_fit = ce.fit_peq
    methods = ('_layout_stage', '_align_subs', '_align_subs_manual', '_match_two_way',
               '_bus_peq_fit', '_ba_bus_pass', '_xo_target_peq', '_correction')
    originals = {n: getattr(ce.CorrectionEngine, n) for n in methods}
    for case in cases():
        print('engine_design:', case.name, flush=True)
        state = dict(stage=None, calls=[])
        def wrapper(name):
            def call(engine, *a, **kw):
                old = state['stage']
                if name == '_correction':
                    state['stage'] = 'Rc:' + a[0]
                elif name == '_xo_target_peq' and old != 'Ba':
                    state['stage'] = 'Top:' + sys._getframe(1).f_locals['role']
                elif name != '_xo_target_peq':
                    state['stage'] = 'Ba'
                try:
                    return originals[name](engine, *a, **kw)
                finally:
                    state['stage'] = old
            return call
        def fit(*a, **kw):
            result = original_fit(*a, **kw)
            args = inspect.signature(original_fit).bind(*a, **kw)
            args.apply_defaults()
            inputs = dict(args.arguments)
            key = f'{case.name}_fit{len(state["calls"])}'
            for field in ('g', 'corr_db'):
                inputs[field] = fam.f64(key + '_' + field, inputs[field])
            inputs['stage'] = state['stage']
            assert inputs['stage'] is not None
            state['calls'].append((inputs, dict(result=result)))
            return result
        with ExitStack() as stack:
            for name in methods:
                stack.enter_context(patch.object(ce.CorrectionEngine, name, wrapper(name)))
            stack.enter_context(patch.object(ce, 'fit_peq', fit))
            run = cal.compute(case.project, case.irs, case.settings)
        e, r = run.engine, run.result
        def real(key, v):
            return fam.f64(case.name + '_' + key, v)
        def spectrum(key, v):
            return None if v is None else fam.c128(case.name + '_' + key, v[idx])
        for i in range(0, len(state['calls']), 4):
            chunk = state['calls'][i:i+4]
            fam.save(f'{case.name}_fits_{i//4:03}', dict(calls=[v[0] for v in chunk]),
                     dict(calls=[v[1] for v in chunk]), dict(case=case.name, kind='fits'))
        for role, c in r.channels.items():
            fir = np.asarray(c.fir)
            peak = int(np.argmax(np.abs(fir)))
            taps = np.arange(len(fir))
            selected = taps[(np.abs(taps - peak) <= 512) | (taps % 64 == 0)]
            out = dict(channel=dict(role=c.role, gain_db=c.gain_db, delay_ms=c.delay_ms,
                xo_hz=c.xo_hz, peq=c.peq, fir=fam.f64_digest(fir),
                fir_samples=real(role+'_fir', fir[selected]), fir_peak=peak,
                top_peq=e.top_peq.get(role, [])),
                phase_strength=e.phase_strength[role], phase_band=e.phase_band[role],
                phase_frac_used=getattr(e, 'phase_frac_used', {}).get(role),
                metrics=run.metrics[role], curves={k: real(role+'_'+k, v) for k, v in run.curves[role].items()})
            for key in ('before', 'combined', 'after'):
                out[key] = spectrum(role+'_'+key, getattr(r, key)[role])
            for key in ('F', 'top_P'):
                out[key] = spectrum(role+'_'+key, e.chain[role][key])
            fam.save(case.name+'_'+role, dict(case=case.name, role=role), out, dict(kind='role'))
        fam.save(case.name, dict(case=case.name), dict(
            roles=list(r.channels), subs=[vars(s) for s in r.subs.values()],
            sub_bus_peq=e.sub_bus_peq, bus_delay_ms=r.sub_bus_delay_ms,
            bus_polarity=r.sub_bus_polarity, lift_ms=r.lift_ms, latency_ms=e.latency_ms,
            main_extra_ms=e.main_extra_ms, ref_level=e.ref_level, sub_level_db=e.sub_level_db,
            notes=r.notes, note_numbers=[[float(x) for x in re.findall(r'[-+]?\d+(?:\.\d+)?', n)] for n in r.notes],
            note_kinds=[next(kind for prefix, kind in (
                ('여유 지연', 'LiftCapped'), ('서브 고정 필터', 'SubFilters'), ('RC 상한', 'RcLimit'),
                ('BA 비용', 'BaCost'), ('BA 버스 PEQ', 'BaBusPass'), ('BA 수동', 'BaManual'),
                ('2-way', 'TwoWay'), ('서브 버스 게인', 'BusGainAnchored'),
                ('긴 모드', 'LongModeBoost'), ('전체 지연', 'Latency'),
                ('위상 보정 세기', 'PhaseStrength'), ('위상 보정 시작', 'PhaseBand'),
                ('묶음', 'LayoutFallback' if '비어' in n else 'Layout'),
                (n.split(' GD 컷')[0] + ' GD 컷', 'GdCuts')) if n.startswith(prefix)) for n in r.notes],
            target=real('target', r.target_db[idx]),
            target_display=real('target_display', run.target), sub_sum=spectrum('sub_sum', r.sub_sum),
            rc_hi=run.rc_hi, rc_auto=run.rc_auto, schroeder_hz=run.schroeder_hz, rc_note=run.rc_note,
            adapter_notes=run.notes), dict(kind='design'))
        if case.name in ('dolby', 'room_dba'):
            h = e.long_spectra(window_ms=500.0)
            inputs, outputs = [], []
            for role in list(r.channels)[:2]:
                for p in e.positions:
                    response = e.stage_responses(role, h, p)
                    inputs.append(dict(role=role, position=p))
                    outputs.append({k: spectrum(f'stage_{role}_{p}_{k}', v) for k, v in response.items()})
            fam.save(case.name+'_stages', dict(calls=inputs), dict(calls=outputs), dict(kind='stages', case=case.name))
    fam.finish()
    assert sum(fam.written.values()) <= 6_000_000, sum(fam.written.values())


if __name__ == '__main__':
    main()
