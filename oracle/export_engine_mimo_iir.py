"""P16: observe upstream IIR seeds, dense derivatives, solves and guards."""
from __future__ import annotations

import ast
import contextlib
import inspect
import json
import os
import pickle
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import common
import numpy as np
from scipy import optimize
from engine_cases import cases
from secs import calibration as cal, correction_engine as ce, mimo, mimo_iir as mi

LABELS = ('reference', 'haswell', 'skylakex', 'simd', 'ulp_0', 'ulp_1')
SOLVE = mi._solve_problem


def decode(v, directory):
    if isinstance(v, dict) and 'file' in v:
        return np.fromfile(directory / v['file'], dtype='<f8')
    if isinstance(v, list):
        return np.asarray(v)
    return v


def perturb(values, rng):
    # Same one-ULP rule as export_engine_fit. Importing that exporter runs it.
    directions = np.where(rng.integers(0, 2, size=values.shape), np.inf, -np.inf)
    return np.where(values == 0., values, np.nextafter(values, directions))


def observe(pb, injected=None, boundary=False, seed_only=False):
    original_ls, original_fit = optimize.least_squares, mi._fit_one
    seeds, joint = [], {}
    cursor = [0]

    def ls(fun, x0, **kw):
        parent = sys._getframe(1)
        kind = parent.f_code.co_name
        if kind not in ('_fit_one', '_solve_problem'):
            return original_ls(fun, x0, **kw)
        if kind == '_solve_problem' and seed_only:
            from types import SimpleNamespace
            return SimpleNamespace(x=x0)
        start = time.perf_counter()
        rec = dict(x0=x0.copy(), lb=kw['bounds'][0], ub=kw['bounds'][1])
        if kind == '_solve_problem' and boundary:
            f = fun(x0)
            j = kw['jac'](x0)
            rec.update(residual=f, jtj=j.T @ j, jtf=j.T @ f)
        sol = original_ls(fun, x0, **kw)
        rec.update(x=sol.x, cost=sol.cost, nfev=sol.nfev, njev=sol.njev,
                   status=sol.status, seconds=time.perf_counter()-start)
        if kind == '_fit_one':
            rec['polarity'] = parent.f_locals['pol']
            seeds.append(rec)
        else:
            joint.update(rec)
        return sol

    def fit(*a, **kw):
        if injected is None:
            return original_fit(*a, **kw)
        rec = injected[cursor[0]]
        cursor[0] += 1
        return np.array(rec['x']), rec['polarity']

    optimize.least_squares, mi._fit_one = ls, fit
    try:
        result = SOLVE(pb)
    finally:
        optimize.least_squares, mi._fit_one = original_ls, original_fit
    return dict(seeds=seeds, joint=joint, result=result)


def postprocess(pb, x, pols):
    """Execute the unchanged post-processing AST from upstream's function."""
    tree = ast.parse(inspect.getsource(mi.design_mimo_iir))
    fn = tree.body[0]
    begin = next(i for i, node in enumerate(fn.body)
                 if isinstance(node, ast.Assign) and isinstance(node.targets[0], ast.Name)
                 and node.targets[0].id == 'out')
    fn.body = ast.parse("K=pb['K']; A=pb['A']; fl=pb['fl']; LP=pb['LP']; nf=pb['nf']; n=pb['n']; level_db=pb['level_db']; results=[(pb['input'],x,pols)]; problems=[pb]").body + fn.body[begin:]
    fn.args = ast.parse('def post(pb,x,pols): pass').body[0].args
    fn.name = 'observed_post'
    ast.fix_missing_locations(tree)
    namespace = dict(mi.__dict__)
    exec(compile(tree, '<upstream-iir-post>', 'exec'), namespace)
    record = dict(violations=[])

    def trace(frame, event, arg):
        if frame.f_code.co_filename != '<upstream-iir-post>':
            return None
        loc = frame.f_locals
        if event == 'return' and frame.f_code.co_name == 'violation':
            record['violations'].append(float(arg))
        if event == 'return' and frame.f_code.co_name == 'observed_post':
            record.update(level_cut=float(10*np.log10(loc['lvl']/loc['target'])) if loc['lvl'] > loc['target'] else 0.,
                          removed=loc['removed'], gain_steps=loc['step'],
                          final={j: vars(s).copy() for j,s in loc['cur'].items()})
        return trace

    sys.settrace(trace)
    try:
        with contextlib.redirect_stdout(sys.stderr):
            supports = namespace['observed_post'](pb,x,pols)[pb['input']]
    finally:
        sys.settrace(None)
    indices = [pb['roles'].index(j) for j in supports]
    ss = [s.response(pb['fl']) for s in supports.values()]
    ys = [pb['X'][p] + (np.fft.irfft(sum(pb['G'][p,r]*s for r,s in zip(indices,ss)),pb['nf'])[:pb['n']] if ss else 0.) for p in range(len(pb['X']))]
    record['band_db'] = mi._worst_band_db(dict(pb,roles=list(supports),G=pb['G'][:,indices]),supports,pb['fl'],pb['nf'],pb['n']) if ss else -99.
    record['t60_change'] = mi.worst_t60_change(list(pb['X']),ys)
    return record


def worker(label, path):
    if label != 'simd':
        common.check_libm()
    jobs = pickle.loads(Path(path).read_bytes())
    results = []
    for job in jobs:
        pb = dict(job['pb'])
        if label.startswith('ulp_'):
            pb['X'] = perturb(pb['X'], np.random.default_rng(int(label[-1])))
        result = observe(pb)
        result['fixed_joint'] = observe(pb, injected=job['reference']['seeds'])['joint']
        result['post'] = postprocess(pb, *result['result'][1:])
        results.append(result)
        print(label, job['case'], pb['input'], result['joint']['seconds'], file=sys.stderr, flush=True)
    sys.stdout.write(json.dumps(common.plain(results), separators=(',', ':')))


def seed_spread(path, label):
    common.check_libm()
    jobs = pickle.loads(path.read_bytes())
    results = []
    for job in jobs:
        pb = dict(job['pb'])
        rng = np.random.default_rng(int(label[-1]))
        pb['fir'] = {r: perturb(s,rng) for r,s in pb['fir'].items()}
        observed = observe(pb, seed_only=True)
        records = []
        for r,rec in zip(pb['roles'],observed['seeds']):
            # Evaluate perturbed solutions against the unperturbed seed target.
            lo,hi = pb['ranges'][r]
            fl = np.geomspace(max(lo,15.),min(hi,pb['fsiso']),160)
            fir = job['pb']['fir'][r]
            target = np.exp(-2j*np.pi*np.outer(fl,np.arange(fir.size))/mi.FS_LOW) @ fir
            h = mi._unpack(rec['x'],pb['K'],[rec['polarity']],pb['A'])[0].response(fl)
            residual = (h-target)/(np.abs(target).mean()+1e-12)
            rec['cost'] = .5*float(np.dot(residual.real,residual.real)+np.dot(residual.imag,residual.imag))
            records.append({k:rec[k] for k in ('cost','nfev','njev','status','seconds')})
        results.append(records)
        print(label,job['case'],pb['input'],flush=True)
    (path.parent/f'mimo_iir_{label}.json').write_text(json.dumps(common.plain(results)))


def replay_engine(case):
    """Replay engine_design's fit decisions, in the upstream call order."""
    outputs = []
    directory = common.GOLDENS / 'engine_design'
    for path in sorted(directory.glob(case.name+'_fits_*.json')):
        fixture = json.loads(path.read_text())
        for request, output in zip(fixture['inputs']['calls'],fixture['outputs']['calls']):
            if 'reference' not in request:
                outputs.append(output['result'])
                continue
            ref = request['reference']
            source = json.loads((common.GOLDENS / ref['family'] / (ref['fixture']+'.json')).read_text())
            if 'call' in ref:
                outputs.append(source['outputs']['calls'][ref['call']]['result'])
            else:
                outputs.append(source['outputs']['candidates'][ref['candidate']]['fits'][ref['fit']]['result'])
    index = [0]
    original = ce.fit_peq
    def fit(*a, **kw):
        result = outputs[index[0]]
        index[0] += 1
        return [tuple(row) for row in result]
    ce.fit_peq = fit
    try:
        engine = cal.compute(case.project,case.irs,case.settings).engine
    finally:
        ce.fit_peq = original
    assert index[0] == len(outputs), (case.name,index[0],len(outputs))
    return engine


def scratch_directory(argument=None):
    return Path(argument) if argument else Path(tempfile.gettempdir()) / 'secs-oracle-engine_mimo_iir'


def main(scratch):
    common.check_libm()
    start = time.perf_counter()
    fam = common.Family('engine_mimo_iir')
    jobs = []
    for case in cases():
        if case.name not in ('room', 'room_dba'):
            continue
        engine = replay_engine(case)
        ideal = mimo.design_mimo(engine, positions=engine.positions)
        # Pin the reference to the recorded design, not a new fit trajectory.
        directory = common.GOLDENS / 'engine_design'
        for role in engine.chain:
            recorded = json.loads((directory / f'{case.name}_{role}.json').read_text())['outputs']['F']
            indices = [i for i,f in enumerate(engine.f) if (15 <= f <= 600 and i%8 == 0) or i%257 == 0]
            expected = decode(recorded['re'],directory) + 1j*decode(recorded['im'],directory)
            actual = engine.chain[role]['F'][indices]
            error = float(np.max(np.abs(actual-expected))/max(np.max(np.abs(expected)),1e-30))
            print('REFERENCE_DESIGN',case.name,role,error,flush=True)
            assert error <= 1e-12, (case.name,role,error)
        records = {}
        original = mi._solve_problem

        def solve(pb):
            ref = observe(pb, boundary=True)
            row = dict(case=case.name, pb=pb, reference=ref, post=dict(violations=[]))
            jobs.append(row)
            records[pb['input']] = row
            print('reference', case.name, pb['input'], ref['joint']['seconds'], flush=True)
            return ref['result']

        def trace(frame, event, arg):
            if frame.f_code.co_filename != mi.__file__:
                return None
            name, loc = frame.f_code.co_name, frame.f_locals
            if name == 'violation' and event == 'return':
                records[loc['pb']['input']]['post']['violations'].append(float(arg))
            if name == 'design_mimo_iir' and event == 'line' and frame.f_lineno == 435:
                records[loc['i']]['post'].update(level_cut=float(10*np.log10(loc['lvl']/loc['target']))
                    if loc['lvl'] > loc['target'] else 0., removed=loc['removed'].copy(),
                    gain_steps=loc['step'], final={j: vars(s).copy() for j, s in loc['cur'].items()})
            return trace if name in ('design_mimo_iir', 'violation') else None

        mi._solve_problem = solve
        sys.settrace(trace)
        try:
            with contextlib.redirect_stdout(sys.stderr):
                mi.design_mimo_iir(engine, ideal, engine.positions, workers=1)
        finally:
            sys.settrace(None)
            mi._solve_problem = original
    path = scratch / 'mimo_iir_jobs.pkl'
    path.write_bytes(pickle.dumps(jobs))
    runs = {}
    labels = list(LABELS)
    for label in LABELS[1:]:
        if label == 'ulp_1' and time.perf_counter()-start > 3000:
            labels.remove(label)
            print('Dropped ulp_1 to keep the family below 60 minutes', flush=True)
            break
        env = os.environ.copy()
        env['OPENBLAS_VERBOSE'] = '2'
        if label in ('haswell', 'skylakex'):
            env['OPENBLAS_CORETYPE'] = {'haswell': 'Haswell', 'skylakex': 'SkylakeX'}[label]
        if label == 'simd':
            env.pop('NPY_DISABLE_CPU_FEATURES', None)
        proc = subprocess.run([sys.executable, __file__, '--worker', label, str(path)],
                              env=env, capture_output=True, text=True, check=True)
        print(proc.stderr, flush=True)
        runs[label] = json.loads(proc.stdout)
        (scratch / f'mimo_iir_{label}.json').write_text(proc.stdout)
    for label in tuple(f'seed_ulp_{i}' for i in range(8)):
        seed_spread(path,label)
    seed_runs = {l: json.loads((scratch/f'mimo_iir_{l}.json').read_text()) for l in tuple(f'seed_ulp_{i}' for i in range(8))}
    finish(fam, jobs, runs, labels, seed_runs)
    print('elapsed_seconds', time.perf_counter()-start, flush=True)


def finish(fam, jobs, runs, labels, seed_runs=None):
    for idx, job in enumerate(jobs):
        key = job['case'] + '_' + job['pb']['input']
        job['post'] = postprocess(job['pb'], *job['reference']['result'][1:])
        for label in labels[1:]:
            run = runs[label][idx]
            if 'post' not in run:
                pb_run = dict(job['pb'])
                if label.startswith('ulp_'):
                    pb_run['X'] = perturb(pb_run['X'], np.random.default_rng(int(label[-1])))
                run['post'] = postprocess(pb_run, *run['result'][1:])
        pb = dict(job['pb'])
        for name in ('fl', 'X', 'WL', 'WE'):
            pb[name] = fam.f64(key+'_'+name, pb[name])
        for name in ('LP', 'G'):
            pb[name] = fam.c128(key+'_'+name, pb[name])
        pb['fir'] = {j: fam.f64(key+'_fir_'+j, s) for j, s in pb['fir'].items()}
        ref = job['reference']
        for name in ('residual', 'jtj', 'jtf'):
            ref['joint'][name] = fam.f64(key+'_'+name, ref['joint'][name])
        compact = {}
        for label in labels[1:]:
            row = runs[label][idx]
            stats = lambda s: {k: s[k] for k in ('cost', 'nfev', 'njev', 'status', 'seconds')}
            compact[label] = dict(seeds=[stats(s) for s in row['seeds']], joint=stats(row['joint']),
                                  fixed_joint=stats(row['fixed_joint']), post=row['post'])
        outputs = dict(reference=ref, post=job['post'], runs=compact,
                       seed_runs={l: rows[idx] for l,rows in (seed_runs or {}).items()})
        fam.save(key, dict(case=job['case'], problem=pb, labels=labels), outputs)
    fam.finish()
    size = sum(fam.written.values())
    assert size < 16*1024*1024, size


if __name__ == '__main__':
    command = sys.argv[1] if len(sys.argv) > 1 else None
    if command == '--worker':
        worker(sys.argv[2], sys.argv[3])
    elif command == '--finish':
        scratch = Path(sys.argv[2])
        jobs = pickle.loads((scratch / 'mimo_iir_jobs.pkl').read_bytes())
        labels = [l for l in LABELS if l == 'reference' or (scratch / f'mimo_iir_{l}.json').exists()]
        runs = {l: json.loads((scratch / f'mimo_iir_{l}.json').read_text()) for l in labels[1:]}
        seed_runs = {l: json.loads((scratch/f'mimo_iir_{l}.json').read_text()) for l in tuple(f'seed_ulp_{i}' for i in range(8)) if (scratch/f'mimo_iir_{l}.json').exists()}
        finish(common.Family('engine_mimo_iir'), jobs, runs, labels, seed_runs)
    elif command == '--seed-spread':
        for label in tuple(f'seed_ulp_{i}' for i in range(8)):
            if not (Path(sys.argv[2])/f'mimo_iir_{label}.json').exists():
                seed_spread(Path(sys.argv[2])/'mimo_iir_jobs.pkl',label)
    else:
        scratch = scratch_directory(command)
        scratch.mkdir(parents=True, exist_ok=True)
        main(scratch)
