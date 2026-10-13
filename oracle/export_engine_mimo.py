"""P14: eight observed upstream MIMO designs; no copied oracle bodies."""
from __future__ import annotations

import concurrent.futures
import json
import os
import subprocess
import sys

import common
import numpy as np
from engine_cases import cases
from secs import calibration as cal, mimo

LABELS = ('reference', 'haswell', 'skylakex', 'simd', 'ulp_0', 'ulp_1', 'ulp_2', 'ulp_3')
CASES = ('room', 'room_dba', 'room_filters', 'room_multi', 'room_cap2',
         'room_fir', 'room_low40', 'room_exclude', 'room_two_positions')


def observe(engine, settings, boundary=False):
    inputs, ranked, guards, solves = {}, {}, {}, {}
    original_band = mimo._worst_band_db
    active = [None]

    def band(xs, gs, sup, fsiso, n, lp):
        value = original_band(xs, gs, sup, fsiso, n, lp)
        # Observe the second guard even when Python's `and` short-circuits it.
        ys = [x + sum(np.convolve(g[j], s)[:n] for j, s in sup.items())
              for x, g in zip(xs, gs)]
        tail = mimo.worst_t60_change(xs, ys)
        frame = sys._getframe(1)
        guards.setdefault(active[0], []).append(dict(scale=frame.f_locals['scale'],
                                                    band_db=value, t60_change=tail))
        return value

    def trace(frame, event, arg):
        if frame.f_code.co_filename != mimo.__file__:
            return None
        name = frame.f_code.co_name
        if name not in ('design_mimo', '_rank_supports', 'solve'):
            return None
        loc = frame.f_locals
        if name == 'design_mimo' and 'i' in loc:
            active[0] = loc['i']
        if event == 'return' and name == '_rank_supports':
            ranked[loc['i']] = dict(order=arg, scores=loc['scores'])
        if event == 'return' and name == 'solve':
            _, ratio = arg
            role = active[0]
            # Lambda's scale is still the normal-equation diagonal mean here.
            parent = frame.f_back.f_locals
            trajectory = solves.setdefault(role, [])
            trajectory.append(dict(mid=parent['mid'] if trajectory else parent['hi'],
                ratio=ratio, accepted=bool(ratio <= parent['target'])))
        if name == 'design_mimo' and event == 'line' and frame.f_lineno == 229:
            inputs[loc['i']] = dict(candidates=loc['cands'].copy(),
                ranking=ranked.get(loc['i']), trajectory=solves[loc['i']],
                log_lambda=loc['hi'], ratio=loc['ratio'], guards=guards[loc['i']],
                guard_scale=loc['scale'])
        return trace

    mimo._worst_band_db = band
    sys.settrace(trace)
    try:
        result = mimo.design_mimo(engine, **settings)
    finally:
        sys.settrace(None)
        mimo._worst_band_db = original_band
    output = dict(ranges=list(result.ranges.items()), inputs=inputs,
                  supports={i: dict(s) for i, s in result.supports.items()},
                  level_db=result.level_db, notes=result.notes)
    stage = None
    if boundary and result.ranges:
        h = engine.long_spectra()
        n = int(mimo.LEN_S * mimo.FS_LOW)
        stage = dict(positions=engine.positions,
            paths={j: [mimo.to_low(mimo.support_path(engine, result, j, h, p), engine.f, n)
                       for p in engine.positions] for j, r in result.ranges.items() if r is not None},
            inputs={i: dict(xs=[mimo.to_low(engine.input_response(i, h, p), engine.f, n)
                               for p in engine.positions],
                           direct=mimo.to_low(engine.chain[i]['F'], engine.f, n)) for i in inputs})
    return output, stage


def worker(label):
    if label != 'simd':
        common.check_libm()
    results = []
    for case in cases():
        if case.name not in CASES[:4]:
            continue
        if label.startswith('ulp_'):
            rng = np.random.default_rng(int(label[-1]))
            for roles in case.irs.values():
                for role, (ir, sr, t0) in list(roles.items()):
                    x = np.asarray(ir, dtype=np.float32)
                    direction = np.where(rng.integers(0, 2, size=x.shape),
                                         np.float32(np.inf), np.float32(-np.inf))
                    perturbed = np.nextafter(x, direction, dtype=np.float32)
                    perturbed[x == 0] = x[x == 0]
                    roles[role] = (perturbed, sr, t0)
        engine = cal.compute(case.project, case.irs, case.settings).engine
        variants = [(case.name, {})]
        if case.name == 'room':
            variants.extend([('room_cap2', dict(max_supports=2)),
                ('room_fir', dict(taps_low=70)), ('room_low40', dict(f_low=40.0)),
                ('room_exclude', dict(exclude=['SW2'])),
                ('room_two_positions', dict(positions=engine.positions[:2]))])
        for name, settings in variants:
            out, stage = observe(engine, settings, boundary=label == 'reference')
            results.append(dict(case=name, settings=settings, output=out, stage=stage))
            print(f'{label}: {name}', file=sys.stderr, flush=True)
    print(json.dumps(common.plain(results), separators=(',', ':')))


def main():
    fam = common.Family('engine_mimo')

    def launch(label):
        env = os.environ.copy()
        env['OPENBLAS_VERBOSE'] = '2'
        if label in ('haswell', 'skylakex'):
            env['OPENBLAS_CORETYPE'] = {'haswell': 'Haswell', 'skylakex': 'SkylakeX'}[label]
        if label == 'simd':
            env.pop('NPY_DISABLE_CPU_FEATURES', None)
        p = subprocess.run([sys.executable, __file__, '--worker', label], env=env,
                           text=True, capture_output=True, check=True)
        print('engine_mimo:', label, p.stderr.strip(), flush=True)
        return label, json.loads(p.stdout), p.stderr.strip()

    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        runs = list(pool.map(launch, LABELS))
    for case in CASES:
        outputs = []
        stage = None
        settings = None
        for label, results, _ in runs:
            row = next(r for r in results if r['case'] == case)
            out = row['output']
            settings = row['settings']
            for data in out['inputs'].values():
                data['trajectory'][0]['mid'] = 4.0
            for i, supports in out['supports'].items():
                for j, taps in supports.items():
                    supports[j] = fam.f64(f'{case}_{label}_{i}_{j}', taps)
            outputs.append(out)
            if row['stage'] is not None:
                stage = row['stage']
                for j, paths in stage['paths'].items():
                    stage['paths'][j] = [fam.f64(f'{case}_path_{j}_{p}', x) for p, x in enumerate(paths)]
                for i, data in stage['inputs'].items():
                    data['xs'] = [fam.f64(f'{case}_x_{i}_{p}', x) for p, x in enumerate(data['xs'])]
                    data['direct'] = fam.f64(f'{case}_direct_{i}', data['direct'])
        fam.save(case, dict(settings=settings, stage=stage, labels=LABELS), {},
                 dict(environments=[dict(run=l, cores=c) for l, _, c in runs]))
        for label, output in zip(LABELS, outputs):
            fam.save(f'{case}_{label}', dict(case=case, run=label), output)
    fam.finish()
    assert sum(fam.written.values()) < 8 * 1024 * 1024


if __name__ == '__main__':
    if len(sys.argv) > 1:
        worker(sys.argv[2])
    else:
        main()
