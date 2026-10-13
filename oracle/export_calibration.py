"""Freeze calibration settings normalization, storage, keys and adapter inputs."""
from __future__ import annotations

import dataclasses
import json
from types import SimpleNamespace

import common
from engine_cases import cases
from secs import calibration as cal
from secs.model import Project


def _project(raw=None):
    project = Project.from_preset('2.0')
    project.extra['keep'] = {'unknown': 1}
    if raw is not ...:
        project.extra['calibration'] = raw
    return project


def _settings_cases():
    yield 'missing', ...
    yield 'none', None
    yield 'wrong', []
    yield 'scalars', {
        'mode': 'bogus', 'rc_lo': -1, 'rc_hi': 99999, 'room_volume_m3': '75.5',
        'room_dims_m': [6, 4, 2.5], 'mimo_on': 'false', 'fsiso': 0,
        'mimo_low': float('nan'), 'mimo_max_supports': 2.5, 'mimo_fir': [1],
        'ultra_ms': float('inf'), 'sub_main_delay_ms': -1, 'infra_boost_max_db': 999,
        'xo_peq_on': 0, 'boost_max_db': None, 'xo_peq_lo': '35', 'xo_peq_hi': -3,
        'unknown': 'drop',
    }
    yield 'nested', {
        'mode': 'ultra', 'room_dims_m': [0.49, 4, 2.5],
        'xo': {'front': 20, 7: 300, 'bad': 'x', 'nan': float('nan')},
        'xo_rec': {'top': 120},
        'subs_manual': {
            'SW1': {'mode': 'fixed', 'bundle': 'Z', 'gain_db': -80, 'delay_ms': 999,
                    'polarity': 0, 'allpass': [[0, 1], [5, 0], [60, 2], ['x']],
                    'mimo_support': 0, 'hp_hz': 1, 'hp_type': 'bad', 'lp_hz': 999,
                    'lp_type': 'BW24', 'peq': [[5, .01, -99], [60, 2, 0], [80, 3, 20], []],
                    'dba_row': 'bad'},
            'junk': 3,
        },
        'bundle_layouts': {'A': {'kind': 'dba', 'rear_trim_ms': 99},
                           'B': {'kind': 'bad', 'rear_trim_ms': float('nan')},
                           'C': {'kind': 'multi'}, 'bad': []},
    }


def _adapter_cases():
    measurement = ('samples omitted', 48000, 0.0)
    project = SimpleNamespace(
        config=SimpleNamespace(groups=lambda: {
            'front': [SimpleNamespace(role='L'), SimpleNamespace(role='R')],
            'sub': [SimpleNamespace(role='SW1')],
        }),
        xo={},
    )
    yield 'biamp', project, {
        1: {'L': measurement, 'L:LF': measurement},
    }
    yield 'non_reference_only', project, {
        1: {'L': measurement},
        2: {'L': measurement, 'R': measurement, 'SW1': measurement},
    }


def _key_cases():
    base = cal.CalibrationSettings()
    excluded = cal.CalibrationSettings(room_dims_m=[3, 4, 2.5], mimo_on=True,
                                       xo_rec={'front': 120})
    auto_a = cal.CalibrationSettings(subs_manual={'SW1': cal.sub_entry({'gain_db': 4})})
    auto_b = cal.CalibrationSettings(subs_manual={'SW1': cal.sub_entry({'gain_db': -8})})
    filtered = cal.CalibrationSettings(subs_manual={'SW1': cal.sub_entry({'hp_hz': 20})})
    ultra_a = cal.CalibrationSettings(mode='ultra', ultra_ms=120)
    ultra_b = cal.CalibrationSettings(mode='ultra', ultra_ms=121)
    mimo = cal.CalibrationSettings(subs_manual={'SW1': cal.sub_entry({'mimo_support': False})})
    return [('base', base), ('excluded', excluded), ('auto_a', auto_a), ('auto_b', auto_b),
            ('filtered', filtered), ('ultra_a', ultra_a), ('ultra_b', ultra_b), ('mimo', mimo)]


def main():
    fam = common.Family('calibration')
    for name, raw in _settings_cases():
        project = _project(raw)
        settings = cal.CalibrationSettings.from_project(project)
        settings.store(project)
        fam.save(f'project_{name}', inputs={'calibration': None if raw is ... else raw},
                 outputs={'settings': dataclasses.asdict(settings),
                          'saved': json.dumps(project.to_dict(), ensure_ascii=False, indent=2) + '\n',
                          'extra_keep': project.extra['keep']})

    keys = _key_cases()
    fam.save('keys', inputs={'settings': {name: dataclasses.asdict(value) for name, value in keys}},
             outputs={'engine': {name: repr(value.engine_key()) for name, value in keys},
                      'manual': {name: repr(value.manual_key()) for name, value in keys},
                      'mimo': {name: repr(value.mimo_key()) for name, value in keys}})
    rows = []
    for mode in ('zero', 'mixed', 'linear', 'ultra'):
        for ms in (0, 30, 31, 80, 120, 199, 200, 300):
            rows.append({'mode': mode, 'ultra_ms': ms, 'params': cal.mode_params(mode, ms)})
    fam.save('mode_params', inputs={}, outputs={'rows': rows})

    for case in cases():
        inputs = cal.engine_inputs(case.project, case.irs, case.settings.xo)
        fam.save(f'engine_{case.name}',
                 inputs={'case': case.name},
                 outputs={'notes': inputs['notes'], 'settings': dataclasses.asdict(case.settings)})
    for name, project, irs in _adapter_cases():
        inputs = cal.engine_inputs(project, irs)
        fam.save(f'adapter_{name}', inputs={'case': name}, outputs={
            'positions': [[pos, list(roles)] for pos, roles in inputs['irs'].items()],
            'mains': inputs['mains'], 'subs': inputs['subs'],
            'groups': list(inputs['groups'].items()), 'xo': list(inputs['xo'].items()),
            'main_position': inputs['main_position'], 'notes': inputs['notes'],
        })
    fam.finish()


if __name__ == '__main__':
    main()
