# -*- coding: utf-8 -*-
"""카밀라 내보내기의 LFE 입력(2026-10-05, 사용자: 업믹스를 안 거친 돌비 신호가 그대로 들어올 수 있다).

서브가 있으면 입력 맨 뒤에 LFE 하나: +10 dB → LR 24 LP 120 Hz → 모든 서브 버스로(메인 저역 센드와 같은 서브 게인 · 극성).
메인 출력으로는 안 간다. `lfe_gain_db=None` 이면 예전처럼 LFE 없음."""
from __future__ import annotations

import json

from test_calibration_step import fast_engine  # noqa: F401
from test_calibration_sub_combine import _fixed_ba, fast_ba, two_subs  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def _parse(text: str) -> tuple[list[str], dict]:
    head, sect, cfg = [], None, {}
    for line in text.splitlines():
        if line.startswith('#'):
            head.append(line)
        elif not line.startswith(' '):
            sect = line.rstrip(':')
            cfg[sect] = [] if sect == 'pipeline' else {}
        elif sect == 'pipeline':
            cfg[sect].append(json.loads(line.strip()[2:]))
        elif sect:
            k, v = line.strip().split(':', 1)
            cfg[sect][k] = json.loads(v) if v.strip().startswith('{') else v.strip()
    return head, cfg


def test_lfe_input_goes_to_every_sub_only(fast_engine, fast_ba, two_subs, tmp_path):  # noqa: F811
    from secs import calibration as cal
    from secs.export_camilladsp import make_config

    project, irs = two_subs
    run = cal.compute(project, irs, cal.CalibrationSettings(mode='zero'))
    mains, subs = list(run.result.channels), list(run.result.subs)
    head, cfg = _parse(make_config(run.engine, run.result, tmp_path / 'lfe', {}, {}))
    n_main, n_in = len(mains), len(mains) + 1
    assert f'# inputs {n_in}: ' + ', '.join(mains + ['LFE']) in head[1]
    assert any(h.startswith('# LFE: gain +10 dB, LR24 lowpass 120 Hz') for h in head)
    assert cfg['filters']['lfe_gain']['parameters']['gain'] == 10.0
    assert cfg['filters']['lfe_lp']['parameters'] == {'type': 'LinkwitzRileyLowpass', 'freq': 120.0, 'order': 4}
    assert {'type': 'Filter', 'channels': [n_main], 'names': ['lfe_gain', 'lfe_lp']} in cfg['pipeline']
    split, bus = cfg['mixers']['split'], cfg['mixers']['bus']
    assert split['channels'] == {'in': n_in, 'out': 2 * n_in}
    lfe_send = n_in + n_main
    by_dest = {m['dest']: [s['channel'] for s in m['sources']] for m in bus['mapping']}
    for s, role in enumerate(subs):                                   # 서브마다 LFE 센드가 들어간다(서브 게인 · 극성과 같이)
        assert lfe_send in by_dest[n_main + s], role
        src = next(x for m in bus['mapping'] if m['dest'] == n_main + s for x in m['sources'] if x['channel'] == lfe_send)
        assert src['gain'] == round(float(run.result.subs[role].gain_db), 4)
        assert src['inverted'] == (run.result.subs[role].polarity < 0)
    for k in range(n_main):                                            # 메인 출력으로는 안 간다
        assert lfe_send not in by_dest.get(k, []) and n_main not in by_dest.get(k, [])


def test_lfe_off_keeps_old_layout(fast_engine, fast_ba, two_subs, tmp_path):  # noqa: F811
    from secs import calibration as cal
    from secs.export_camilladsp import make_config

    project, irs = two_subs
    run = cal.compute(project, irs, cal.CalibrationSettings(mode='zero'))
    mains = list(run.result.channels)
    head, cfg = _parse(make_config(run.engine, run.result, tmp_path / 'nolfe', {}, {}, lfe_gain_db=None))
    assert f'# inputs {len(mains)}: ' + ', '.join(mains) + ' /' in head[1]
    assert 'lfe_gain' not in cfg['filters'] and cfg['mixers']['split']['channels']['in'] == len(mains)
