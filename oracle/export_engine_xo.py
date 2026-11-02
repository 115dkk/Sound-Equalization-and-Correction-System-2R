"""P12b: XO scoring/recommendation and cached XO/BA preview boundaries."""
from __future__ import annotations

import concurrent.futures
import contextlib
import hashlib
import io
import json
import os
import subprocess
import sys
import tomllib
from pathlib import Path
from unittest.mock import patch

import common
import numpy as np
import scipy
from engine_cases import cases
from export_engine_design import DesignFamily
from export_engine_e2e import LABELS, worker as e2e_worker
from secs import calibration as cal


def recommendation(value):
    return dict(candidates=value.candidates,
                table=[(g, [(fc, row) for fc, row in rows.items()]) for g, rows in value.table.items()],
                best=list(value.best.items()))


def cache_fingerprint(label):
    with open(common.ROOT / 'UPSTREAM.toml', 'rb') as fh:
        upstream = tomllib.load(fh)['upstream']['commit']
    return {
        'upstream': upstream,
        'exporter_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'inputs_sha256': {
            name: hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest()
            for name in ('engine_cases.py', 'export_engine_e2e.py')
        },
        'case': label,
        'numpy': np.__version__,
        'scipy': scipy.__version__,
    }


def cached_run(path, label):
    if not path.exists():
        return None
    record = json.loads(path.read_text())
    if record.get('fingerprint') != cache_fingerprint(label):
        return None
    return record['results'], record['cores']


def worker(label):
    # Reuse the e2e worker's exact case construction and seeded float32-IR
    # perturbation loop. Its normal scalar serialization is redirected; a
    # returned reference run lets it finish without introducing another fit.
    result = []
    compute = cal.compute
    last = []
    def capture(project, irs, settings):
        def trial_compute(*args, **kwargs):
            run = compute(*args, **kwargs)
            last[:] = [run]
            return run
        if len(result) == 2:
            return last[0]  # e2e's third (DBA) case is outside this family.
        with patch.object(cal, 'compute', trial_compute):
            value = cal.recommend_xo(project, irs, settings, workers=1)
        result.append(recommendation(value))
        return last[0]
    with patch.object(cal, 'compute', capture), contextlib.redirect_stdout(io.StringIO()):
        e2e_worker(label)
    print(json.dumps(common.plain(result), separators=(',', ':')))


def main():
    fam = DesignFamily('engine_xo')
    for case in cases():
        print('engine_xo:', case.name, flush=True)
        run = cal.compute(case.project, case.irs, case.settings)
        # Pin the upstream design to the recorded family used by Rust replay.
        recorded = json.loads((common.GOLDENS / 'engine_design' / (case.name + '.json')).read_text())['outputs']
        assert common.plain(run.engine.sub_state['params']) == recorded['sub_rows']
        fam.save(case.name + '_scores', dict(case=case.name), dict(scores=list(cal.xo_scores(run).items())))
        if len(run.engine.subs) >= 2:
            out = dict(cal.sub_combine(run))
            del out['layouts']
            out['layout_info'] = list(run.engine.layout_info.items())
            for key in ('grid', 'sum_before', 'sum_after'):
                out[key] = fam.f64(case.name + '_sub_' + key, out[key])
            for state in ('before', 'after'):
                out[state] = [(r, fam.f64(case.name + '_sub_' + state + '_' + r, v)) for r, v in out[state].items()]
            fam.save(case.name + '_sub', dict(case=case.name), out)
        if case.name in ('dolby', 'room_dba', 'room_filters'):
            for group, roles in run.engine.groups.items():
                if group == 'sub':
                    continue
                inputs, outputs = [], []
                for fc in (50., 80., 115., 150.):
                    for corrected in (False, True):
                        for role in (None, roles[0]):
                            request = dict(case=case.name, group=group, fc=fc, corrected=corrected, role=role)
                            curves = cal.xo_preview(run, group, fc, corrected, role)
                            key = f'{case.name}_{group}_{len(inputs)}'
                            inputs.append(request)
                            outputs.append({k: fam.f64(key + '_' + k, v) for k, v in curves.items()})
                fam.save(case.name + '_preview_' + group, dict(calls=inputs), dict(calls=outputs))

    def launch(label):
        env = os.environ.copy()
        env['OPENBLAS_VERBOSE'] = '2'
        if label in ('haswell', 'skylakex'):
            env['OPENBLAS_CORETYPE'] = {'haswell': 'Haswell', 'skylakex': 'SkylakeX'}[label]
        if label == 'simd':
            env.pop('NPY_DISABLE_CPU_FEATURES', None)
        cache = os.environ.get('SECS_XO_RUN_CACHE')
        cached = Path(cache) / (label + '.json') if cache else None
        saved = cached_run(cached, label) if cached is not None else None
        if saved is not None:
            print('engine_xo: saved run', label, flush=True)
            return label, *saved
        p = subprocess.run([sys.executable, __file__, '--worker', label], env=env,
                           text=True, capture_output=True, check=True)
        print('engine_xo:', label, p.stderr.strip(), flush=True)
        results, cores = json.loads(p.stdout), p.stderr.strip()
        if cached is not None:
            cached.parent.mkdir(parents=True, exist_ok=True)
            cached.write_text(json.dumps(dict(fingerprint=cache_fingerprint(label),
                                              results=results, cores=cores)))
        return label, results, cores
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        runs = list(pool.map(launch, LABELS))
    for index, case in enumerate(('dolby', 'room')):
        fam.save(case + '_recommend', dict(calls=[dict(case=case, run=label) for label, _, _ in runs]),
                 dict(calls=[result[index] for _, result, _ in runs]),
                 dict(environments=[dict(run=label, cores=cores) for label, _, cores in runs]))
    fam.finish()
    assert sum(fam.written.values()) <= 3_000_000, sum(fam.written.values())


if __name__ == '__main__':
    if len(sys.argv) > 1:
        worker(sys.argv[2])
    else:
        main()
