"""MIMO 서포트 PEQ 밴드 제한 기본값(2026-10-12) — 외부 의견: 서포트 PEQ 가 ±25–30 dB, 큰 +/− 밴드가 겹친 상쇄 쌍.

mmio님 14채널 맥미니 비교(`보고서/02 새 엔진/2026-10-11 MIMO 서포트 PEQ 극단값/`)에서 V2(±15 dB · Q≤10 · 서포트 대역 안 ·
밴드 게인 벌점 0.05)를 기본값으로 골랐다. 여기서는 그 기본값과, 맞춤 시작값이 한도 안에 머무는지를 본다(전체 맞춤은 수 분이라 빼고).
"""
from __future__ import annotations

import numpy as np
import pytest

from conftest import load_engine

load_engine()                                       # src 를 sys.path 에

KNOBS = ('SECS_MIMO_BAND_DB', 'SECS_MIMO_Q_MAX', 'SECS_MIMO_IN_BAND', 'SECS_MIMO_BAND_REG')


@pytest.fixture
def clean_env(monkeypatch):
    for k in KNOBS:
        monkeypatch.delenv(k, raising=False)
    return monkeypatch


def test_default_is_v2(clean_env):
    from secs import mimo_iir

    assert mimo_iir._knobs() == {'band_db': 15.0, 'q_max': 10.0, 'in_band': True, 'reg': 0.05}


def test_old_values_come_back_by_env(clean_env):
    from secs import mimo_iir

    for k, v in zip(KNOBS, ('30', '40', '0', '0')):
        clean_env.setenv(k, v)
    assert mimo_iir._knobs() == {'band_db': 30.0, 'q_max': 40.0, 'in_band': False, 'reg': 0.0}
    assert mimo_iir._band_of(40.0, 120.0, 150.0) == (15.0, 300.0)


def test_bounds_follow_band_and_q(clean_env):
    from secs import mimo_iir

    K, A = 3, 2
    f_lo, f_hi = mimo_iir._band_of(40.0, 200.0, 150.0)
    assert (f_lo, f_hi) == pytest.approx((36.0, 165.0))           # 하한 × 0.9 · Fsiso × 1.1
    lb, ub = mimo_iir._bounds(K, A, f_lo, f_hi)
    for k in range(K):
        i = 2 + 3 * k
        assert np.exp(lb[i]) == pytest.approx(f_lo) and np.exp(ub[i]) == pytest.approx(f_hi)
        assert np.exp(ub[i + 1]) == pytest.approx(10.0)
        assert (lb[i + 2], ub[i + 2]) == (-15.0, 15.0)


def test_fit_start_stays_inside_limits(clean_env):
    """깊은 골 · 높은 봉우리가 있는 서포트 FIR 이어도 시작값의 밴드는 ±15 dB · Q ≤ 10 · 대역 안."""
    from scipy.signal import lfilter

    from secs import mimo_iir

    rng = np.random.default_rng(1012)
    n = 180
    x = np.zeros(n)
    x[3] = 1.0
    for f0, r in ((28.0, 0.995), (47.0, 0.99), (71.0, 0.985)):        # 저속(FS_LOW) 공진 셋
        w = 2 * np.pi * f0 / mimo_iir.FS_LOW
        x = lfilter([1.0], [1.0, -2 * r * np.cos(w), r * r], x)
    x += 0.01 * rng.standard_normal(n)
    lo, hi = 30.0, 150.0
    band = mimo_iir._band_of(lo, hi, 150.0)
    K, A = mimo_iir.K_BANDS, mimo_iir.A_BANDS
    v, pol = mimo_iir._fit_one(x, lo, hi, K, [47.0], A, band)
    sup = mimo_iir._unpack(v, K, [pol], A)[0]
    for f0, q, g in sup.peq:
        assert abs(g) <= 15.0 + 1e-6
        assert q <= 10.0 + 1e-6
        assert band[0] - 1e-6 <= f0 <= band[1] + 1e-6
