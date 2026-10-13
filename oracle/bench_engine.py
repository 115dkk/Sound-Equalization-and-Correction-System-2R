"""SECS 2 performance audit, not a golden exporter.

Run a fresh process for each op/case/configuration. SIMD stays enabled.
Configuration is applied before NumPy imports, including spawned XO workers.
fit_* requires SECS_PERF_FIT_INPUTS and writes the nine recorded/ULP inputs
for the Rust example. Short rows use --runs 31 in interleaved language batches.
"""
from __future__ import annotations

import argparse
import json
import os
import statistics
import time


def perturb(values, rng):
    """One independently signed ULP per nonzero target, preserving signed zeros.

    Copied from export_engine_fit.py; importing it would run the exporter.
    """
    import numpy as np
    directions = np.where(rng.integers(0, 2, size=values.shape), np.inf, -np.inf)
    return np.where(values == 0., values, np.nextafter(values, directions))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('op', choices=[
        'design', 'stages', 'prepare', 'recommend', 'preview', 'cache',
        'sub_combine', 'fit_rc1', 'fit_rc2', 'fit_bus', 'fit_xo', 'profile_fit',
        'camilla', 'mdat_staged', 'mdat_before_after', 'gd', 'record_gd',
        'project_setup', 'project_open', 'project_save',
    ])
    parser.add_argument('case')
    parser.add_argument('--config', choices=['shipped', 'one'], default='shipped')
    parser.add_argument('--runs', type=int, choices=[3, 5, 31], default=None,
                        help='override samples; 31 for interleaved short batches')
    parser.add_argument('--memory-once', action='store_true',
                        help='one call, no warm-up, for process-tree memory')
    args = parser.parse_args()
    os.environ.pop('NPY_DISABLE_CPU_FEATURES', None)
    os.environ.pop('OPENBLAS_CORETYPE', None)
    if args.config == 'one':
        os.environ['OPENBLAS_NUM_THREADS'] = '1'
    else:
        os.environ.pop('OPENBLAS_NUM_THREADS', None)

    from engine_cases import cases
    from secs import calibration as cal

    case = next(c for c in cases() if c.name == args.case)
    design = lambda: cal.compute(case.project, case.irs, case.settings)
    before_each = lambda: None
    if args.op.startswith('project_'):
        project_bench(args, case)
        return
    if args.op.startswith('fit_') or args.op == 'profile_fit':
        import common
        import numpy as np
        from secs import correction_engine as ce
        kind = {'fit_rc1': 'rc', 'fit_rc2': 'rc', 'profile_fit': 'bus'}.get(args.op, args.op[4:])
        calls = []
        folder = common.GOLDENS / 'engine_fit'
        for path in sorted(folder.glob(f'fit_{args.case}_{kind}_*.json')):
            calls.extend(json.loads(path.read_text())['inputs']['calls'])
        index = int(args.op == 'fit_rc2')
        if index < len(calls):
            inputs = calls[index]
        else:
            stage = {'bus': 'Ba', 'xo': 'Top:L', 'rc': 'Rc:L'}[kind]
            folder = common.GOLDENS / 'engine_design'
            calls = [call for path in sorted(folder.glob(f'{args.case}_fits_*.json'))
                     for call in json.loads(path.read_text())['inputs']['calls']
                     if call['stage'] == stage]
            inputs = calls[index]
            if 'reference' in inputs:
                ref = inputs['reference']
                folder = common.GOLDENS / ref['family']
                source = json.loads((folder / (ref['fixture'] + '.json')).read_text())
                inputs = (source['inputs']['calls'][ref['call']] if 'call' in ref else
                          source['outputs']['candidates'][ref['candidate']]['fits'][ref['fit']]['inputs'])
        def array(value):
            if isinstance(value, list):
                return np.asarray(value, dtype=np.float64)
            return np.fromfile(folder / value['file'], dtype='<f8')
        kw = {key: inputs[key] for key in ('n', 'lo', 'hi', 'boost', 'ring_ms', 'f_max')}
        kw.update(g=array(inputs['g']), corr_db=array(inputs['corr_db']))
        call = lambda: ce.fit_peq(**kw)
        if args.op.startswith('fit_'):
            import hashlib
            from pathlib import Path
            import scipy.optimize
            corr = kw['corr_db']
            envelope = [corr] + [perturb(corr, np.random.default_rng(k)) for k in range(8)]
            data = np.asarray(envelope, dtype='<f8').tobytes()
            Path(os.environ['SECS_PERF_FIT_INPUTS']).write_bytes(data)
            original = scipy.optimize.least_squares
            stats = []
            def counted(fun, *a, **kwargs):
                sol = original(fun, *a, **kwargs)
                stats.append((sol.nfev, sol.njev))
                return sol
            scipy.optimize.least_squares = counted
            rows = []
            try:
                for index, corr in enumerate(envelope):
                    kw['corr_db'] = corr
                    call()
                    samples = []
                    counts = []
                    for _ in range(5):
                        stats.clear()
                        started = time.perf_counter()
                        call()
                        samples.append(time.perf_counter() - started)
                        counts.append(stats[0] if stats else (0, 0))
                    assert all(c == counts[0] for c in counts)
                    rows.append(dict(input=index, samples_s=samples,
                                     median_s=statistics.median(samples),
                                     nfev=counts[0][0], njev=counts[0][1]))
            finally:
                scipy.optimize.least_squares = original
            print(json.dumps(dict(implementation='python', op=args.op, case=args.case,
                                  config=args.config, input_count=9, runs_per_input=5, inputs=rows,
                                  input_sha256=hashlib.sha256(data).hexdigest(),
                                  median_s=statistics.median(r['median_s'] for r in rows))))
            return
        if args.op == 'profile_fit':
            import scipy.optimize
            original = scipy.optimize.least_squares
            def profiled(fun, *a, **kw):
                calls = 0
                residual_s = 0.0
                def counted(x):
                    nonlocal calls, residual_s
                    started = time.perf_counter()
                    out = fun(x)
                    residual_s += time.perf_counter() - started
                    calls += 1
                    return out
                started = time.perf_counter()
                sol = original(counted, *a, **kw)
                print(json.dumps(dict(op='profile_fit', case=args.case,
                                      total_s=time.perf_counter()-started,
                                      residual_s=residual_s, calls=calls,
                                      nfev=sol.nfev, njev=sol.njev)))
                return sol
            scipy.optimize.least_squares = profiled
    elif args.op == 'record_gd':
        import hashlib
        import numpy as np
        from pathlib import Path
        from secs import correction_engine as ce
        path = Path(os.environ['SECS_PERF_GD_INPUT'])
        engine = case.engine()
        engine.gd_cuts = True
        class Recorded(Exception):
            pass
        def capture(f, h, *unused, **kw):
            data = np.asarray(h, dtype='<c16').tobytes()
            path.write_bytes(data)
            print(json.dumps(dict(case=args.case, role=engine.mains[0],
                                  bytes=len(data), sha256=hashlib.sha256(data).hexdigest())))
            raise Recorded
        original = ce.fit_gd_cuts
        ce.fit_gd_cuts = capture
        try:
            engine.compute()
        except Recorded:
            return
        finally:
            ce.fit_gd_cuts = original
        raise RuntimeError('no GD input captured')
    elif args.op == 'gd':
        import numpy as np
        from secs import correction_engine as ce
        path = os.environ['SECS_PERF_GD_INPUT']
        spectrum = np.fromfile(path, dtype='<c16')
        engine = case.engine()
        if spectrum.size != engine.f.size:
            raise ValueError('recorded GD spectrum does not match the design grid')
        call = lambda: ce.fit_gd_cuts(engine.f, spectrum, 20.0, 500.0, 6, 3.0)
    elif args.op == 'stages':
        stages(case, args.config, args.runs or 5)
        return
    elif args.op == 'design':
        call = design
    elif args.op == 'prepare':
        engine = case.engine()
        call = engine._prepare
    elif args.op == 'recommend':
        call = lambda: cal.recommend_xo(case.project, case.irs, case.settings,
                                       workers=1 if args.config == 'one' else None)
    else:
        run = design()
        if args.op in ('camilla', 'mdat_staged', 'mdat_before_after'):
            folder = os.environ['SECS_PERF_OUTPUT']
            export = {'camilla': cal.export_camilladsp, 'mdat_staged': cal.export_mdat,
                      'mdat_before_after': cal.export_before_after_mdat}[args.op]
            call = lambda: export(run, folder)
        elif args.op == 'sub_combine':
            before_each = lambda: run.__dict__.pop('_sub_combine', None)
            call = lambda: cal.sub_combine(run)
        else:
            def preview():
                return cal.xo_preview(run, 'front', 80.0)
            if args.op == 'cache':
                import inspect
                source = inspect.getsource(cal.xo_preview)
                source = source[:source.index('    roles = [r for r in engine.groups')]
                source += "    return cache\n"
                namespace = dict(cal.__dict__)
                exec(compile(source, '<upstream cache construction>', 'exec'), namespace)
                build_cache = namespace['xo_preview']
                before_each = lambda: run.__dict__.pop('_xo_cache', None)
                call = lambda: build_cache(run, 'front', 80.0)
            else:
                call = preview
    if args.memory_once:
        before_each()
        result = call()
        print(json.dumps(dict(implementation='python', op=args.op, case=args.case,
                              config=args.config, memory_once=True)))
        del result
        return
    before_each()
    warm_started = time.perf_counter()
    call()
    warm_seconds = time.perf_counter() - warm_started
    runs = args.runs or (3 if args.op in ('design', 'recommend') and warm_seconds > 10 else 5)
    samples = []
    for _ in range(runs):
        before_each()
        start = time.perf_counter()
        result = call()
        samples.append(time.perf_counter() - start)
        del result
    print(json.dumps(dict(implementation='python', op=args.op, case=args.case,
                          config=args.config, samples_s=samples,
                          median_s=statistics.median(samples))))


def project_bench(args, case):
    """Build once, then time the non-GUI open/save bodies used by SECS 2."""
    import copy
    from pathlib import Path

    import numpy as np
    from secs.rew_mdat.builder import MeasurementInput, build_mdat
    from secs.rewapi.session import IrSaver, load_project_irs
    from secs.wizard import open_project

    root = Path(os.environ['SECS_PERF_PROJECT'])
    path = root / 'demo.secs'
    if args.op == 'project_setup':
        root.mkdir(parents=True, exist_ok=True)
        project = copy.deepcopy(case.project)
        saver = IrSaver(path, use_soundfile=False)
        records = []
        for pos in sorted(case.irs):
            record = dict(pos=int(pos), label=f'Position {pos}', uuid={}, mdat=None,
                          ir={}, spl_db={}, t0={}, done=True, sample_rate=None)
            for role, stored in case.irs[pos].items():
                samples, rate, *rest = stored
                wav = saver.save(role, pos, np.asarray(samples, dtype=np.float32), int(rate))
                record['ir'][role] = str(wav.relative_to(path.parent)).replace('\\', '/')
                record['t0'][role] = float(rest[0]) if rest else 0.0
                record['spl_offset'] = record.get('spl_offset', {})
                record['sample_rate'] = int(rate)
            records.append(record)
        project.measurements = records
        project.save(path)
        print(json.dumps(dict(op=args.op, path=str(path), records=len(records),
                              irs=sum(len(r['ir']) for r in records))))
        return

    def loaded():
        project = open_project(path)
        irs = load_project_irs(project, path)
        return project, irs

    if args.op == 'project_open':
        call = loaded
    else:
        project, _ = loaded()
        target = root / 'saved.secs'
        order = [speaker.role for speaker in project.config.speakers]

        def call():
            project.save(target)
            irs = load_project_irs(project, target)
            items = []
            for pos in sorted(irs):
                roles = sorted(irs[pos], key=lambda role: order.index(role)
                               if role in order else len(order))
                for role in roles:
                    samples, rate, *rest = irs[pos][role]
                    items.append(MeasurementInput(
                        f'{role}_P{pos}', f'SECS · 위치 {pos} · 원 측정',
                        int(rate), np.asarray(samples, dtype=np.float32),
                        float(rest[0]) if rest else 0.0, None))
            return build_mdat(items, target.with_suffix('.mdat'))

    call()
    samples = []
    for _ in range(args.runs or 5):
        started = time.perf_counter()
        result = call()
        samples.append(time.perf_counter() - started)
        del result
    print(json.dumps(dict(implementation='python', op=args.op, case=args.case,
                          config=args.config, samples_s=samples,
                          median_s=statistics.median(samples))))


def stages(case, config, runs=5):
    """Insert boundary timers into an in-memory copy; upstream files stay intact.

    Unlike sys.settrace this does not add callbacks to optimizer loops.
    """
    import inspect
    import textwrap
    from collections import defaultdict
    from secs import calibration as cal, correction_engine as ce
    samples = defaultdict(list)
    elapsed = defaultdict(float)
    active = {}
    def start(key):
        active[key] = time.perf_counter()
    def stop(key):
        elapsed[key] += time.perf_counter() - active.pop(key)
    def instrument(function, replacements, namespace):
        source = textwrap.dedent(inspect.getsource(function))
        for old, new in replacements:
            assert source.count(old) == 1, old
            source = source.replace(old, new)
        scope = dict(namespace, _perf_start=start, _perf_stop=stop)
        exec(compile(source, '<perf boundary timers>', 'exec'), scope)
        return scope[function.__name__]
    original_compute = ce.CorrectionEngine.compute
    original_prepare = ce.CorrectionEngine._prepare
    ce.CorrectionEngine.compute = instrument(original_compute, [
        ("    if self.subs:\n        hi = max", "    _perf_start('ba')\n    if self.subs:\n        hi = max"),
        ("        earliest = min", "        _perf_stop('ba')\n        earliest = min"),
        ("        if self.xo_stage:\n", "        _perf_start('top')\n        if self.xo_stage:\n"),
        ("        if manual is None or not manual['fixed']:", "        _perf_stop('top')\n        _perf_start('ba')\n        if manual is None or not manual['fixed']:"),
        ("    self.sub_state =", "    _perf_stop('ba')\n    self.sub_state ="),
        ("    for k_role, role in enumerate", "    _perf_start('rc_all')\n    for k_role, role in enumerate"),
        ("        gain_db = ref_level", "        _perf_start('rc:' + role)\n        gain_db = ref_level"),
        ("        after[role] = comb[self.main_position] * F", "        after[role] = comb[self.main_position] * F\n        _perf_stop('rc:' + role)"),
        ("    if manual is not None and manual['fixed']:", "    _perf_stop('rc_all')\n    if manual is not None and manual['fixed']:"),
    ], ce.__dict__)
    compute = instrument(cal.compute, [
        ("    metrics, curves = {}, {}", "    _perf_start('scores')\n    metrics, curves = {}, {}"),
        ("    auto = settings.rc_hi is None", "    _perf_stop('scores')\n    auto = settings.rc_hi is None"),
    ], cal.__dict__)
    def prepare(engine):
        start('prepare')
        result = original_prepare(engine)
        stop('prepare')
        return result
    ce.CorrectionEngine._prepare = prepare
    try:
        for iteration in range(runs + 1):
            elapsed.clear()
            started = time.perf_counter()
            result = compute(case.project, case.irs, case.settings)
            elapsed['design'] = time.perf_counter() - started
            del result
            if iteration:
                for stage, value in elapsed.items():
                    samples[stage].append(value)
    finally:
        ce.CorrectionEngine.compute = original_compute
        ce.CorrectionEngine._prepare = original_prepare
    for stage, values in samples.items():
        print(json.dumps(dict(implementation='python', op=stage, case=case.name,
                              config=config, samples_s=values,
                              median_s=statistics.median(values))))


if __name__ == '__main__':
    main()
