"""P12a: complete designs with stage-local recorded PEQ decisions."""
from __future__ import annotations

import hashlib
import inspect
import json
import re
import sys
from contextlib import ExitStack
from unittest.mock import patch

import common
import numpy as np
from engine_cases import cases
from secs import calibration as cal, correction_engine as ce


def array(folder, value):
    if isinstance(value, dict):
        data = (folder / value['file']).read_bytes()
        assert hashlib.sha256(data).hexdigest() == value['sha256']
        return np.frombuffer(data, dtype='<f8')
    if value and isinstance(value[0], dict):
        return np.concatenate([array(folder, v) for v in value])
    return np.asarray(value, dtype='<f8')


def request_key(inputs):
    digests = {k: hashlib.sha256(np.asarray(inputs[k], dtype='<f8').tobytes()).hexdigest()
               for k in ('g', 'corr_db')}
    scalars = tuple(inputs[k] for k in ('n', 'lo', 'hi', 'boost', 'ring_ms', 'f_max'))
    return (*digests.values(), *scalars), digests


def recorded_fits():
    bank = {}
    for family in ('engine_fit', 'engine_rc'):
        folder = common.GOLDENS / family
        for path in sorted(folder.glob('*.json')):
            fixture = json.loads(path.read_text())
            calls = []
            if family == 'engine_fit' and path.stem.startswith('fit_'):
                calls = [(i, o['result'], dict(call=k)) for k, (i, o) in enumerate(
                    zip(fixture['inputs']['calls'], fixture['outputs']['calls']))]
            elif family == 'engine_rc':
                calls = [(fit['inputs'], fit['result'], dict(candidate=c, fit=k))
                         for c, candidate in enumerate(fixture['outputs'].get('candidates', []))
                         for k, fit in enumerate(candidate['fits'])]
            for inputs, result, location in calls:
                request = dict(inputs)
                request.update({k: array(folder, inputs[k]) for k in ('g', 'corr_db')})
                key, digests = request_key(request)
                bank.setdefault(key, (dict(family=family, fixture=path.stem,
                                           **location, request_sha256=digests), result))
    return bank


class DesignFamily(common.Family):
    def f64(self, key, values):
        descriptor = super().f64(key, values)
        if isinstance(descriptor, dict):
            # The full sidecar and its hash remain unchanged; these are only
            # redundant diff previews, not the values consumed by tests.
            descriptor['first'] = descriptor['first'][:8]
            descriptor['last'] = descriptor['last'][-8:]
        return descriptor


class Notes(list):
    """Capture unrounded note arguments from their actual append site."""
    def __init__(self, values):
        super().__init__(values)
        assert not values, 'add capture for constructor lift note if a case needs it'
        self.typed = []

    def append(self, text):
        v = sys._getframe(1).f_locals
        e = v['self']
        if text.startswith('서브 고정 필터'):
            note = dict(SubFilters=[(r, dict(hp_hz=s['hp'], hp_type=s['hp_type'],
                lp_hz=s['lp'], lp_type=s['lp_type'], peq=s['peq'])) for r, s in e.sub_filters.items()])
        elif text.startswith('RC 상한'):
            note = dict(RcLimit=dict(hz=e.rc_hi, auto=e.rc_hi_manual is None, schroeder_hz=v.get('fs')))
        elif text.startswith('BA 비용'):
            note = dict(BaCost=v['res'].fun)
        elif text.startswith('BA 버스 PEQ'):
            note = dict(BaBusPass=dict(first=v['c1'], second=v['c2'], kept_first=v['c2'] >= v['c1']))
        elif text.startswith('BA 수동'):
            note = 'BaManual'
        elif text.startswith('묶음'):
            note = {('LayoutFallback' if '비어' in text else 'Layout'): v['name']}
        elif text.startswith('2-way'):
            note = dict(TwoWay=dict(roles=v['movable'], gain_db=v['total'], xo_hz=v['xo']))
        elif text.startswith('서브 버스 게인'):
            note = dict(BusGainAnchored=e.sub_level_db)
        elif text.startswith('긴 모드'):
            note = dict(LongModeBoost=dict(cap_db=ce.LONG_MODE_BOOST_DB,
                modes=[(p, t) for p, t in sorted(v['t60'].items())
                       if t >= max(ce.LONG_MODE_MIN_S, ce.LONG_MODE_RATIO * v['ref'])]))
        elif ' GD 컷' in text:
            note = dict(GdCuts=dict(role=v['role'], cuts=v['cuts']))
        elif text.startswith('전체 지연'):
            note = dict(Latency=dict(latency_ms=e.latency_ms, lift_ms=e.lift_ms, main_extra_ms=v['shift']))
        else:
            raise AssertionError(text)
        self.typed.append(common.plain(note))
        super().append(text)


def main():
    fam = DesignFamily('engine_design')
    bank = recorded_fits()
    f = ce.freqs()
    bins = np.arange(len(f))
    idx = np.flatnonzero(((f >= 15) & (f <= 600) & (bins % 8 == 0)) | (bins % 257 == 0))
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
            key, digests = request_key(inputs)
            if key in bank:
                reference, expected = bank[key]
                assert common.plain(result) == common.plain(expected), reference
                inputs = dict(reference=reference)
                output = {}
            else:
                key = f'{case.name}_fit{len(state["calls"])}'
                for field in ('g', 'corr_db'):
                    inputs[field] = fam.f64(key + '_' + field, inputs[field])
                inputs['request_sha256'] = digests
                output = dict(result=result)
            inputs['stage'] = state['stage']
            assert inputs['stage'] is not None
            state['calls'].append((inputs, output))
            return result
        original_compute = ce.CorrectionEngine.compute
        def compute(engine, *args, **kwargs):
            engine.notes = Notes(engine.notes)
            return original_compute(engine, *args, **kwargs)
        with ExitStack() as stack:
            stack.enter_context(patch.object(ce.CorrectionEngine, 'compute', compute))
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
            align_delay_ms=e.align_delay_ms, sub_rows=e.sub_state['params'],
            bus_gain=e.sub_state['bus_gain'], layout_info=list(e.layout_info.items()),
            plan=e.manual_plan() if e.subs else None,
            sub_bus_peq=e.sub_bus_peq, bus_delay_ms=r.sub_bus_delay_ms,
            bus_polarity=r.sub_bus_polarity, lift_ms=r.lift_ms, latency_ms=e.latency_ms,
            main_extra_ms=e.main_extra_ms, ref_level=e.ref_level, sub_level_db=e.sub_level_db,
            notes=r.notes, typed_notes=e.notes.typed + [dict(PhaseStrength=list(e.phase_strength.items())),
                dict(PhaseBand=list(e.phase_band.items()))],
            note_numbers=[[float(x) for x in re.findall(r'[-+]?\d+(?:\.\d+)?', n)] for n in r.notes],
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
