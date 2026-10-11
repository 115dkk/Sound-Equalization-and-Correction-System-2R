"""성적표 1/24 GD 절대 봉우리(2026-10-09) — 보정 설계 화면의 GD 지표.

'봉우리 − 중앙값'(gd24_max)은 넓은 대역 GD(중앙값)가 달라지면 같은 봉우리도 달라 보였다(돌비룸 L 438 Hz: 절대 211 ms 그대로인데
168 → 203). 절대 봉우리는 봉우리 그 자체(IR 피크 지연을 뺀 1/24 oct GD 의 최댓값)와 그 주파수다.
"""
from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'src'))


def test_abs_peak_is_the_peak_itself_and_its_frequency():
    from secs.correction_engine import allpass2, freqs, gd_curve
    from secs.scorecard import metrics

    f = freqs()
    H = allpass2(f, 300.0, 6.0)                          # 300 Hz 좁은 GD 봉우리
    m = metrics(H, f, np.zeros(f.size))
    g = np.geomspace(20, 500, 1500)
    gd = gd_curve(H, f, g)
    assert m['gd24_abs'] == float(np.max(gd))
    assert abs(m['gd24_abs_hz'] - 300.0) < 15
    # 중앙값을 빼지 않는다 — 중앙값이 0 이 아니면 두 지표가 그만큼 다르다
    assert m['gd24_abs'] - m['gd24_max'] == float(np.median(gd)) or abs(m['gd24_abs'] - m['gd24_max'] - np.median(gd)) < 1e-9
