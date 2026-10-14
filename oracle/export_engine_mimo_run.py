"""P17a: compute_mimo, exporter paths and applied curves at recorded fit decisions.

Only reference runs: each case fits IIR once. Full FIR taps and ideal designs;
complex design-grid outputs use the engine_design low/high sampling rule.
"""
from __future__ import annotations

from types import SimpleNamespace

import common
import numpy as np
from engine_cases import cases
from export_engine_mimo_iir import replay_engine, decode
from secs import calibration as cal, mimo
from secs.export_camilladsp import fir_to_48k, mimo_paths
import json


def main():
    family = common.Family('engine_mimo_run')
    for case in cases():
        if case.name not in ('room_dba', 'room_filters'):
            continue
        engine = replay_engine(case)
        indices = np.array([i for i, f in enumerate(engine.f)
                            if (15 <= f <= 600 and i % 8 == 0) or i % 257 == 0])
        directory = common.GOLDENS / 'engine_design'
        for role in engine.chain:
            expected = json.loads((directory / f'{case.name}_{role}.json').read_text())['outputs']['F']
            expected = decode(expected['re'], directory) + 1j * decode(expected['im'], directory)
            error = np.max(np.abs(engine.chain[role]['F'][indices] - expected)) / max(np.max(np.abs(expected)), 1e-30)
            assert error <= 1e-12, (case.name, role, error)
        for fir in (False, True):
            name = case.name + ('_fir' if fir else '_iir')
            print('RUN', name, flush=True)
            run = cal.compute_mimo(SimpleNamespace(engine=engine), workers=1, fir=fir)
            ideal = run.design
            paths = []
            for input_role, support, sp, low, delay in mimo_paths(engine, ideal, run.iir):
                filt = ({'Fir': {'taps_48k': family.f64(name + '_' + input_role + '_' + support + '_48k', fir_to_48k(sp.taps, sp.fs_low))}}
                        if fir else {'Iir': vars(sp)})
                paths.append(dict(input=input_role, support=support, filter=filt, low_hz=low, delay_ms=delay))
            outputs = dict(
                ideal=dict(fs_low=ideal.fs_low, ranges=list(ideal.ranges.items()),
                           supports=[(i, [(r, family.f64(name+'_'+i+'_'+r+'_ideal', taps)) for r, taps in sups.items()]) for i, sups in ideal.supports.items()],
                           level_db=list(ideal.level_db.items()), notes=ideal.notes),
                supports=[] if fir else [(i, [(r, vars(sp)) for r, sp in sups.items()]) for i, sups in run.iir.items()],
                paths=dict(fsiso=run.fsiso, paths=paths),
                final=[(r, family.c128(name+'_'+r+'_final', h[indices])) for r, h in run.final.items()],
                applied=[(r, family.c128(name+'_'+r+'_applied', h[indices])) for r, h in run.applied.items()],
                spl_mimo=[(r, family.f64(name+'_'+r+'_spl', curve['spl_mimo'])) for r, curve in run.curves.items()],
                notes=run.notes, key=run.key(),
                budgets=[(b, cal.mimo_fir_budget_text(run, b)) for b in (0, 1, 8000, 48000, 1000000000)])
            family.save(name, dict(case=case.name, fir=fir, indices=family.f64(name+'_indices', indices)), outputs)
    # Independent FIR conversion / response specimen: upstream's own test input.
    rng = np.random.default_rng(12)
    taps = np.convolve(mimo.lowpass_fir(150.), rng.standard_normal(mimo.FIR_TAPS_LOW) * np.exp(-np.arange(mimo.FIR_TAPS_LOW) / 15))
    f = np.fft.rfftfreq(1 << 15, 1 / 48000.)
    family.save('fir', dict(taps=family.f64('fir_low', taps), fs_low=600.),
                dict(taps_48k=family.f64('fir_48k', fir_to_48k(taps, 600.)),
                     response=family.c128('fir_response', mimo.FirSupport(taps).response(f))))
    estimates = []
    old = cal.MIMO_WORKERS
    try:
        for inputs, positions, workers in ((0, 0, 0), (2, 3, 1), (7, 9, 2), (11, 17, 4)):
            cal.MIMO_WORKERS = workers
            project = SimpleNamespace(config=SimpleNamespace(speakers=[SimpleNamespace(role='L')] * inputs))
            for fir in (False, True):
                estimates.append([inputs, positions, workers, fir, cal.estimate_mimo_seconds(project, positions, fir)])
    finally:
        cal.MIMO_WORKERS = old
    family.save('helpers', {}, dict(availability=[cal.mimo_availability(n) for n in range(5)],
        caps=[(n, cal.mimo_cap_text(SimpleNamespace(max_supports=n))) for n in (0, 1, 4, 12)],
        kinds=[cal.mimo_kind(SimpleNamespace(fir=f)) for f in (False, True)], estimates=estimates))
    family.finish()
    size = sum(p.stat().st_size for p in family.dir.iterdir())
    assert size < 8 * 1024 * 1024, size
    print('BYTES', size, flush=True)


if __name__ == '__main__':
    main()
