"""속도 개선 회귀: PEQ 복소 응답과 XO 지연 선택은 최적화 전 수식과 같아야 한다."""
import sys
from pathlib import Path

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'src'))

from secs.correction_engine import CorrectionEngine, SR, lr4, peaking_response, peq_response


def original_peaking(f, f0, q, gain_db, sr=SR):
    a = 10 ** (gain_db / 40)
    w0 = 2 * np.pi * f0 / sr
    al = np.sin(w0) / (2 * q)
    b = np.array([1 + al * a, -2 * np.cos(w0), 1 - al * a])
    d = np.array([1 + al / a, -2 * np.cos(w0), 1 - al / a])
    z = np.exp(-1j * 2 * np.pi * np.asarray(f, float) / sr)
    return (b[0] + b[1] * z + b[2] * z * z) / (d[0] + d[1] * z + d[2] * z * z)


@pytest.mark.parametrize('rate', [44100, 48000, 96000])
def test_peq_preserves_complex_response(rate):
    f = np.geomspace(15, 20000, 700)
    for f0, q, gain in [(31.25, 0.3, -25.), (147., 15., 10.), (8000., 3., -0.5)]:
        np.testing.assert_array_equal(peaking_response(f, f0, q, gain, rate),
                                      original_peaking(f, f0, q, gain, rate))
    bands = [(31.25, .3, -25.), (147., 15., 10.), (8000., 3., -.5)]
    for _ in range(2):
        expected = np.ones(f.shape, complex)
        for b in bands:
            expected = expected * original_peaking(f, *b)
        np.testing.assert_array_equal(peq_response(f, bands), expected)
        # 같은 배열을 바꿔 다시 호출해도 이전 격자가 남으면 안 된다.
        f *= 0.99
    np.testing.assert_array_equal(peq_response(f, []), np.ones(f.shape, complex))


def original_bus_delay(eng, sums, xo_of, lo):
    best = (1e18, 0.0, 1)
    masks = {r: np.nonzero((eng.f >= xo_of[r] / 2) & (eng.f <= xo_of[r] * 2))[0]
             for r in eng.mains if xo_of.get(r)}
    if not masks:
        return 0., 1
    for pol in (1, -1):
        for delay in np.arange(lo, 15.01, .25):
            cost = 0.
            for role, m in masks.items():
                for pos in eng.positions:
                    if role not in eng.H[pos]:
                        continue
                    main = eng.H[pos][role][m] * eng._delay(eng.align_delay_ms[role])[m] * lr4(eng.f[m], xo_of[role], 'HP')
                    sub = sums[pos][m] * lr4(eng.f[m], xo_of[role], 'LP') * pol * eng._delay(delay)[m]
                    total = np.abs(main + sub) ** 2
                    ref = np.abs(main) ** 2 + np.abs(sub) ** 2
                    cost += float(np.mean((10*np.log10(ref+1e-30)-10*np.log10(total+1e-30))**2))
            if cost < best[0]:
                best = (cost, float(delay), pol)
    return best[1], best[2]


@pytest.mark.parametrize('lo', [-2.25, 0., 3.75])
def test_bus_delay_preserves_choice_across_groups_positions(lo):
    rng = np.random.default_rng(7)
    eng = CorrectionEngine({}, ['L', 'R'], ['SW1'], {}, lambda f: np.zeros_like(f))
    eng.f = np.arange(1025, dtype=float)
    eng.positions = [1, 2]
    eng.align_delay_ms = {'L': 1.125, 'R': 2.75}
    eng.H = {p: {r: rng.normal(size=eng.f.size) + 1j*rng.normal(size=eng.f.size)
                 for r in eng.mains} for p in eng.positions}
    del eng.H[2]['R']
    sums = {p: rng.normal(size=eng.f.size) + 1j*rng.normal(size=eng.f.size) for p in eng.positions}
    xo = {'L': 80., 'R': 120.}
    assert eng._bus_delay(sums, xo, lo) == original_bus_delay(eng, sums, xo, lo)
    assert eng._bus_delay(sums, {}, lo) == (0., 1)


def test_fit_peq_ring_limit_keeps_every_band_short():
    """10-05 감쇠 악화: 울림 상한(ring_ms)을 주면 모든 밴드의 극 시간 상수 Q·A/(π f)가 그 안이다(돌비 C 94 Hz +9.5 dB·Q 15.8
    = 92 ms 가 93 Hz 늦은 에너지 +28 dB 를 만들었다). 상한 없이는 같은 곡선에 더 길게 우는 밴드가 나온다."""
    from secs.correction_engine import fit_peq

    g = np.geomspace(15, 20000, 700)
    corr = (20 * np.log10(np.abs(peq_response(g, [(94., 16., 9.), (60., 20., -6.), (40., 1.5, 4.)])))
            + np.random.default_rng(0).normal(0, 0.2, g.size))

    def ring_ms(bands):
        return [q * 10 ** (gain / 40) / (np.pi * f0) * 1000 for f0, q, gain in bands]

    capped = fit_peq(g, corr, 8, ring_ms=20.0)
    assert capped and max(ring_ms(capped)) <= 20.0 + 1e-6
    assert max(ring_ms(fit_peq(g, corr, 8))) > 20.0
