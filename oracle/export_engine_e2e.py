"""P12a: eight independent end-to-end draws, at most two interpreters at once."""
from __future__ import annotations

import concurrent.futures
import json
import os
import subprocess
import sys
import time

import common
import numpy as np
from engine_cases import cases
from secs import calibration as cal

LABELS = ('reference', 'haswell', 'skylakex', 'simd', 'ulp_0', 'ulp_1', 'ulp_2', 'ulp_3')


def worker(label):
    if label != 'simd':
        common.check_libm()
    results = []
    for case in cases():
        if case.name not in ('dolby', 'room', 'room_dba'):
            continue
        if label.startswith('ulp_'):
            rng = np.random.default_rng(int(label[-1]))
            for roles in case.irs.values():
                for role, (ir, sr, t0) in list(roles.items()):
                    x = np.asarray(ir, dtype=np.float32)
                    direction = np.where(rng.integers(0, 2, size=x.shape), np.float32(np.inf), np.float32(-np.inf))
                    perturbed = np.nextafter(x, direction, dtype=np.float32)
                    perturbed[x == 0] = x[x == 0]
                    roles[role] = (perturbed, sr, t0)
        started = time.perf_counter()
        run = cal.compute(case.project, case.irs, case.settings)
        e, r = run.engine, run.result
        results.append(dict(case=case.name, roles=[dict(role=role, metrics=run.metrics[role],
            **run.filters(role)) for role in run.roles],
            subs=[dict(role=s.role, gain_db=s.gain_db, delay_ms=s.delay_ms, polarity=s.polarity, mode=s.mode)
                  for s in r.subs.values()],
            bus_delay_ms=r.sub_bus_delay_ms, bus_polarity=r.sub_bus_polarity,
            latency_ms=e.latency_ms, main_extra_ms=e.main_extra_ms, ref_level=e.ref_level,
            rc_hi=e.rc_hi, elapsed_s=time.perf_counter()-started))
    print(json.dumps(common.plain(results), separators=(',', ':')))


def main():
    fam = common.Family('engine_e2e')
    def launch(label):
        env = os.environ.copy()
        env['OPENBLAS_VERBOSE'] = '2'
        if label in ('haswell', 'skylakex'):
            env['OPENBLAS_CORETYPE'] = {'haswell': 'Haswell', 'skylakex': 'SkylakeX'}[label]
        if label == 'simd':
            env.pop('NPY_DISABLE_CPU_FEATURES', None)
        p = subprocess.run([sys.executable, __file__, '--worker', label], env=env,
                           text=True, capture_output=True, check=True)
        print('engine_e2e:', label, p.stderr.strip(), flush=True)
        return label, json.loads(p.stdout), p.stderr.strip()
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        runs = list(pool.map(launch, LABELS))
    for case in ('dolby', 'room', 'room_dba'):
        fam.save(case, dict(calls=[dict(case=case, run=label) for label, _, _ in runs]),
            dict(calls=[next(r for r in results if r['case'] == case) for _, results, _ in runs]),
            dict(environments=[dict(run=label, cores=cores) for label, _, cores in runs]))
    fam.finish()
    assert sum(fam.written.values()) <= 1_000_000


if __name__ == '__main__':
    if len(sys.argv) > 1:
        worker(sys.argv[2])
    else:
        main()
