#!/usr/bin/env python3
"""P27: screen display data, with Qt-free extraction of the actual screen methods."""
from __future__ import annotations

import ast
import copy
from dataclasses import asdict
from types import SimpleNamespace

import common
import numpy as np
from scipy.ndimage import gaussian_filter1d
from engine_cases import cases
from export_engine_design import DesignFamily
from secs import calibration as cal


def extract(path, names, namespace, class_methods=False):
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    candidates = [child for node in tree.body if isinstance(node, ast.ClassDef) for child in node.body] if class_methods else tree.body
    body = [node for node in candidates if isinstance(node, ast.FunctionDef) and node.name in names]
    assert {node.name for node in body} == set(names)
    exec(compile(ast.Module(body=body, type_ignores=[]), str(path), "exec"), namespace)
    return [namespace[name] for name in names]


sigma, = extract(common.SRC / "secs/wizard/plot.py", ("octave_smoothing_sigma",), {"np": np})
FREQS = np.logspace(np.log10(5.0), np.log10(22000.0), 280)
magnitude, = extract(common.SRC / "secs/wizard/step5_measure.py", ("magnitude_db",), dict(np=np, FREQS=FREQS, _octave_smoothing_sigma=sigma, SMOOTHING_OCTAVE_FRACTION=12.0, gaussian_filter1d=gaussian_filter1d))
level_offset, = extract(common.SRC / "secs/wizard/step_calibration.py", ("_level_offset",), dict(np=np, cal=cal), True)


def main():
    fam = DesignFamily("service_curves")
    for case in cases():
        if case.name not in ("dolby", "room_dba"):
            continue
        print("service_curves:", case.name, flush=True)
        measurements = []
        for position, roles in case.irs.items():
            for role, value in roles.items():
                measurements.append(dict(position=position, role=role, values=fam.f64(f"{case.name}_measurement_{position}_{role}", magnitude(value[0], value[1]))))
        run = cal.compute(case.project, case.irs, case.settings)
        offset = level_offset(SimpleNamespace(project=case.project), run)
        spl, gd, lines = [], [], []
        for role in run.roles:
            curves = run.curves[role]
            spl.append(dict(role=role, before=fam.f64(f"{case.name}_{role}_spl_before", curves['spl_before'] - offset), after=fam.f64(f"{case.name}_{role}_spl_after", curves['spl_after'] - offset)))
            both = np.concatenate((curves['gd_before'], curves['gd_after']))
            both = both[np.isfinite(both)]
            bounds = [float(np.clip(np.percentile(both, 1) - 5, -50, -5)), float(np.clip(np.percentile(both, 99) + 10, 30, 400))] if both.size else [-10., 100.]
            gd.append(dict(role=role, before=fam.f64(f"{case.name}_{role}_gd_before", curves['gd_before']), after=fam.f64(f"{case.name}_{role}_gd_after", curves['gd_after']), bounds=bounds))
            metrics = run.metrics[role]
            lines.append(dict(role=role, within_1db_pct=[metrics[s]['in1_20'] for s in ('before', 'after')], gd_spread_ms=[metrics[s]['gd_20'] for s in ('before', 'after')], filters=run.filters(role)))
        xo = []
        for group, roles in run.engine.groups.items():
            if group == 'sub':
                continue
            configured = float(run.engine.xo.get(group, 80.))
            for hz in (configured, 55., 135.):
                for corrected in (False, True):
                    view = cal.xo_preview(run, group, hz, corrected, roles[0])
                    key = f"{case.name}_xo_{len(xo)}"
                    mid = float(np.median(view['target'])) - offset
                    xo.append(dict(group=group, hz=hz, corrected=corrected, role=roles[0], curves={k: fam.f64(key + '_' + k, v if k == 'grid' else v - offset) for k, v in view.items()}, bounds=[mid - 30., mid + 12.]))
        ba = cal.sub_combine(run)
        band = (ba['grid'] >= ba['band'][0]) & (ba['grid'] <= ba['band'][1])
        top = float(max(np.max(ba['sum_before'][band]), np.max(ba['sum_after'][band]))) - offset
        ba_curves = {key: fam.f64(f"{case.name}_ba_{key}", ba[key] - offset) for key in ('sum_before', 'sum_after')}
        for state in ('before', 'after'):
            ba_curves[state] = [(role, fam.f64(f"{case.name}_ba_{state}_{role}", values - offset)) for role, values in ba[state].items()]
        taken = copy.deepcopy(case.settings)
        before = copy.deepcopy(asdict(taken))
        taken.take_sub_values(run)
        target = np.asarray(case.project.target.sample(cal.SPL_GRID), float)
        mid = float(np.median(run.target[(cal.SPL_GRID >= 20) & (cal.SPL_GRID <= 20000)])) - offset
        fam.save(case.name, dict(case=case.name), dict(
            measurement_grid=fam.f64('measurement_grid_' + case.name, FREQS), measurements=measurements,
            spl_grid=fam.f64('spl_grid_' + case.name, cal.SPL_GRID), spl=spl, target=fam.f64('target_' + case.name, target), spl_bounds=[mid - 30., mid + 15.],
            gd_grid=fam.f64('gd_grid_' + case.name, cal.GD_GRID), gd=gd, xo=xo, offset=offset,
            ba=dict(grid=fam.f64('ba_grid_' + case.name, ba['grid']), curves=ba_curves, bounds=[top - 40., top + 6.], band=ba['band'], flat=ba['flat'], seat=ba['seat'], positions=ba['positions'], values=ba['values'], layouts=list(run.engine.layout_info.items()), warnings=cal.sub_filter_warnings(run.settings, ba['subs']) + cal.sub_gain_warnings(run)),
            lines=lines, delay_ms=run.latency_ms, rc_pieces=cal.rc_pieces(run), take_before=before, take_after=asdict(taken)))
    fam.finish()
    assert sum(fam.written.values()) < 4 * 1024 * 1024


if __name__ == '__main__':
    main()
