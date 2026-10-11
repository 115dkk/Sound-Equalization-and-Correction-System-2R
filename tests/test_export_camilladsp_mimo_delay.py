# -*- coding: utf-8 -*-
"""카밀라 내보내기의 MIMO 갈래 딜레이 = 갈래 지연 + 받는 스피커 정렬 지연 + 메인 추가 지연(10-05 버그 수정).

엔진은 서브를 앞당기느라 메인·서브 딜레이 모두에 메인 추가 지연(`main_extra_ms`)을 더한다. 갈래만 빠지면 도움 신호가
그만큼 일찍 도착해 엔진이 설계한 시간 관계가 깨진다(맥미니 카밀라로 바로 적용할 때 실제로 틀린 값이 들어간다)."""
from __future__ import annotations

import json

import numpy as np

from test_calibration_step import fast_engine  # noqa: F401
from test_calibration_sub_combine import _fixed_ba, fast_ba, two_subs  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def _mimo_delays(text: str) -> dict:
    """설정 YAML 의 mimo_<입력>_<스피커>_delay 필터 → 샘플 수."""
    out = {}
    for line in text.splitlines():
        line = line.strip()
        if line.startswith('mimo_') and '_delay:' in line:
            name, spec = line.split(':', 1)
            out[name] = json.loads(spec)['parameters']['delay']
    return out


def test_mimo_branch_delay_includes_main_extra(fast_engine, fast_ba, two_subs, tmp_path):  # noqa: F811
    from secs import calibration as cal
    from secs import mimo, mimo_iir
    from secs.export_camilladsp import SAMPLE_RATE, make_config

    project, irs = two_subs
    run = cal.compute(project, irs, cal.CalibrationSettings(mode='zero'))
    eng = run.engine
    design = mimo.MimoResult()
    design.ranges = {'SW1': (20.0, 120.0), 'SW2': (20.0, 120.0)}
    iir = {'L': {'SW1': mimo_iir.IirSupport(-6.0, 3.0, 1, [], [])}}
    for extra in (0.0, 1.7):
        eng.main_extra_ms = extra
        text = make_config(eng, run.result, tmp_path / f'x{extra}', {}, {}, mimo=design, iir=iir)
        got = _mimo_delays(text)['mimo_L_SW1_delay']
        want = int(round((3.0 + eng.align_delay_ms['SW1'] + extra) * SAMPLE_RATE / 1000))
        assert got == want, (extra, got, want)
    # 메인 추가 지연이 없는 엔진(옛 결과)도 그대로 열린다
    del eng.main_extra_ms
    text = make_config(eng, run.result, tmp_path / 'old', {}, {}, mimo=design, iir=iir)
    assert _mimo_delays(text)['mimo_L_SW1_delay'] == int(round((3.0 + eng.align_delay_ms['SW1']) * SAMPLE_RATE / 1000))
    assert np.isfinite(eng.align_delay_ms['SW1'])
