"""요구 대장 R27(2026-10-05): BA(BA) 수동 설정 — 서브마다 자동 / 고정 / 묶음, MIMO 서포트 제외.

사용자(10-04): "DBA 가 있는데 단순 BA 를 돌리면 DBA 세팅이 안 될 거잖아 … BA 에 수동으로 값을 설정할 수 있으면".
설계 `문서/설계·계약/R27 BA 수동 설정 2026-10-05.md`. '동작 → 변화' 로 본다:
- 고정: 넣은 게인·딜레이·극성·올패스가 결과(`CorrectionResult.subs`)·카밀라·mdat 노트에 그대로 나오고, 엔진은 도착 정렬·BA·
  버스 게인·버스 지연을 건너뛴다. 메인은 도착 정렬 + 메인 추가 지연(설정 값).
- 자동 결과 값을 [결과 값 가져오기]로 옮겨 전부 고정으로 다시 계산하면 자동 결과와 같다(고정 경로 검증) — 메인 추가 지연이 0 이
  아닐 때도.
- 묶음: 안의 상대값(게인·딜레이 차·극성 곱)은 그대로, 묶음 전체가 같이 움직인다. 자동 서브와 섞이면 BA 하나로 푼다.
- 모두 자동이면 지금 경로 그대로(같은 답). 칸이 없는 옛 .secs 는 그대로 열린다.
- MIMO: 체크를 끈 서브는 서포트 후보에서 빠진다.
- 고정 출력 필터(10-05 추가 — 서브마다 LR4 HP·LP · PEQ, 예: INFRA 로우패스): 엔진의 그 서브 응답(설계·긴 창) = 원 응답 × 필터,
  MIMO 서포트 대역도 그 응답으로, 카밀라는 서브 출력 맨 끝, 결과·mdat 노트에 필터.
- 화면은 `test_calibration_r27_sub_card.py`.
BA DE·입력 PEQ 맞춤은 빠른 대역으로 바꿔 끼운다(엔진 수학이 아니라 수동 설정이 엔진에 어떻게 들어가는지를 본다).
"""
from __future__ import annotations

import copy
import json

import numpy as np
import pytest

from test_calibration_step import fast_engine  # noqa: F401
from test_calibration_sub_combine import _fixed_ba, fast_ba, two_subs  # noqa: F401
from test_wizard_common import engine  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

SUBS = ('SW1', 'SW2')


def _settings(**subs):
    from secs import calibration as cal

    s = cal.CalibrationSettings(mode='zero')
    for role, values in subs.items():
        s.set_sub(role, **values)
    return s


@pytest.fixture
def spies(monkeypatch):
    """자동 경로(`_align_subs`)·버스 지연 탐색(`_bus_delay`)이 불렸는지 센다(빠른 고정 BA 위에서)."""
    from secs import correction_engine

    calls = {'align': 0, 'bus': 0}
    original_bus = correction_engine.CorrectionEngine._bus_delay

    def align(self, hi_hz):
        calls['align'] += 1
        return _fixed_ba(self, hi_hz)

    def bus(self, *a, **k):
        calls['bus'] += 1
        return original_bus(self, *a, **k)

    monkeypatch.setattr(correction_engine.CorrectionEngine, '_align_subs', align)
    monkeypatch.setattr(correction_engine.CorrectionEngine, '_bus_delay', bus)
    return calls


# ──────────────────────────── 저장 ────────────────────────────
def test_old_secs_without_r27_fields_opens_as_all_auto_and_new_fields_survive(tmp_path, two_subs):  # noqa: F811
    from secs import calibration as cal
    from secs import model

    project, _irs = two_subs
    old = copy.deepcopy(project)
    old.extra['calibration'] = {'mode': 'mixed', 'rc_lo': 20.0, 'xo': {'프론트': 90.0}}     # R27 전 .secs 모양
    path = tmp_path / 'old.secs'
    old.save(path)
    raw = json.loads(path.read_text(encoding='utf-8'))
    assert 'subs_manual' not in json.dumps(raw)
    s = cal.CalibrationSettings.from_project(model.Project.load(path))
    assert s.subs_manual == {} and s.sub_main_delay_ms == 0.0 and s.mode == 'mixed'
    assert s.manual_key() == ((), None) and s.excluded_supports() == ()
    assert s.sub('SW1')['mode'] == 'auto' and s.sub('SW1')['mimo_support'] is True
    # 새 칸: 고정 · 묶음 · MIMO 제외 · 메인 추가 지연이 저장·불러오기를 거쳐 그대로
    s.set_sub('SW1', mode='fixed', gain_db=-3.25, delay_ms=12.5, polarity=-1, allpass=[[45.0, 0.8], [0.0, 1.0]])
    s.set_sub('SW2', mode='bundle', bundle='B', mimo_support=False, lp_hz=40.0, lp_type='BW12',
              peq=[[30.0, 2.0, -3.0], [0.0, 1.0, -2.0]])
    s.sub_main_delay_ms = 1.75
    new = copy.deepcopy(project)
    s.store(new)
    new.save(path)
    back = cal.CalibrationSettings.from_project(model.Project.load(path))
    assert back.subs_manual == s.subs_manual
    assert back.sub('SW1') == {'mode': 'fixed', 'bundle': 'A', 'gain_db': -3.25, 'delay_ms': 12.5, 'polarity': -1,
                               'allpass': [[45.0, 0.8]], 'mimo_support': True,          # f 0 올패스 = 없음
                               'hp_hz': 0.0, 'lp_hz': 0.0, 'peq': [], 'hp_type': 'LR24', 'lp_type': 'LR24',
                                   'dba_row': 'front'}
    assert back.sub('SW2')['mode'] == 'bundle' and back.sub('SW2')['bundle'] == 'B'
    assert back.sub('SW2')['lp_hz'] == 40.0 and back.sub('SW2')['peq'] == [[30.0, 2.0, -3.0]]   # f 0 밴드 = 없음
    assert back.sub('SW2')['lp_type'] == 'BW12' and back.sub('SW2')['hp_type'] == 'LR24'
    assert back.sub_main_delay_ms == 1.75 and back.excluded_supports() == ('SW2',)
    assert back.engine_key() != cal.CalibrationSettings(mode='mixed').engine_key()
    # 이상한 값은 범위로 자르고 모르는 방식은 자동
    weird = cal.sub_entry({'mode': 'magic', 'gain_db': 99, 'delay_ms': -5, 'polarity': 0.3, 'allpass': [[9999, 0]]})
    assert weird == {'mode': 'auto', 'bundle': 'A', 'gain_db': 20.0, 'delay_ms': 0.0, 'polarity': 1,
                     'allpass': [[500.0, 0.1]], 'mimo_support': True, 'hp_hz': 0.0, 'lp_hz': 0.0, 'peq': [],
                     'hp_type': 'LR24', 'lp_type': 'LR24', 'dba_row': 'front'}
    assert cal.sub_entry({'lp_type': 'magic'})['lp_type'] == 'LR24'
    assert cal.sub_entry({'hp_hz': 3, 'lp_hz': 999})['hp_hz'] == 10.0 and cal.sub_entry({'lp_hz': 999})['lp_hz'] == 300.0
    # 자동 서브라도 고정 출력 필터는 결과를 바꾼다(낡음 판정)
    auto_lp = cal.CalibrationSettings(mode='mixed')
    auto_lp.set_sub('SW3', lp_hz=35.0)
    assert auto_lp.manual_key() != ((), None) and auto_lp.engine_key() != cal.CalibrationSettings(mode='mixed').engine_key()


# ──────────────────────────── 엔진 ────────────────────────────
def test_all_auto_takes_the_unchanged_path(fast_engine, spies, two_subs):  # noqa: F811
    """모든 서브가 자동(칸 값·MIMO 체크만 바꿈)이면 지금 경로(`_align_subs`) 그대로 — 결과도 같다."""
    from secs import calibration as cal

    project, irs = two_subs
    plain = cal.compute(project, irs, cal.CalibrationSettings(mode='zero'))
    assert spies['align'] == 1
    touched = _settings(SW1={'gain_db': -7.0, 'delay_ms': 3.0}, SW2={'mimo_support': False})
    assert touched.manual_key() == plain.settings.manual_key()
    run = cal.compute(project, irs, touched)
    assert spies['align'] == 2 and run.engine.manual_plan() is None
    for role in run.result.channels:
        assert np.array_equal(run.result.after[role], plain.result.after[role])
    assert [vars(s) for s in run.result.subs.values()] == [vars(s) for s in plain.result.subs.values()]
    assert all(s.mode == 'auto' for s in run.result.subs.values())


def test_fixed_subs_are_used_exactly_as_entered(fast_engine, spies, two_subs, tmp_path):  # noqa: F811
    from secs import calibration as cal
    from secs import rew_mdat

    project, irs = two_subs
    s = _settings(SW1={'mode': 'fixed', 'gain_db': -3.0, 'delay_ms': 12.5, 'polarity': -1},
                  SW2={'mode': 'fixed', 'gain_db': -1.25, 'delay_ms': 4.0, 'allpass': [[55.0, 0.7]]})
    s.sub_main_delay_ms = 1.25
    run = cal.compute(project, irs, s)
    eng, res = run.engine, run.result
    assert spies['align'] == 0 and spies['bus'] == 0          # 도착 정렬·BA·버스 지연 탐색 모두 건너뜀
    a, b = res.subs['SW1'], res.subs['SW2']
    assert (a.gain_db, a.delay_ms, a.polarity, a.allpass, a.mode) == (-3.0, 12.5, -1, [], 'fixed')
    assert (b.gain_db, b.delay_ms, b.polarity, b.allpass, b.mode) == (-1.25, 4.0, 1, [(55.0, 0.7)], 'fixed')
    assert eng.sub_state['bus_gain'] == 1.0 and res.sub_bus_delay_ms == 0.0 and res.sub_bus_polarity == 1
    assert any('서브 버스 게인 안 걸음' in n for n in eng.notes)
    # 메인 = 도착 정렬 + 메인 추가 지연(설정), 엔진 합의 서브 매개변수 = 출력 값 − 메인 추가 지연
    for role, ch in res.channels.items():
        assert ch.delay_ms == pytest.approx(eng.align_delay_ms[role] + 1.25)
    assert run.latency_ms == pytest.approx(eng.lift_ms + 1.25)
    p = dict(zip(eng.subs, eng.sub_state['params']))
    assert p['SW1'][:3] == pytest.approx((-3.0, 12.5 - 1.25, -1)) and p['SW2'][3] == [(55.0, 0.7)]
    # 결과 표·카밀라·mdat 노트에 넣은 값이 그대로
    view = cal.sub_combine(run)
    assert [(v['mode'], v['gain_db'], v['delay_ms'], v['polarity']) for v in view['values']] == [
        ('fixed', -3.0, 12.5, -1), ('fixed', -1.25, 4.0, 1)]
    yml = cal.export_camilladsp(run, tmp_path / 'cam')[0].read_text(encoding='ascii')
    assert '# sub SW1: fixed gain -3.000 dB, delay 12.500 ms, polarity inverted' in yml
    assert '# sub SW2: fixed gain -1.250 dB, delay 4.000 ms, polarity normal, allpass 55.0 Hz Q 0.700' in yml
    assert 'sub_SW1_delay: {"type": "Delay", "parameters": {"delay": 600,' in yml
    notes = {m.name: m.notes for m in rew_mdat.load(cal.export_before_after_mdat(run, tmp_path / 'm')).measurements}
    sw1 = next(v for k, v in notes.items() if k.endswith('SW1_1 보정 전(원 측정)'))
    assert 'DSP 값: SW1 고정 -3.00 dB 12.50 ms 반전' in sw1
    assert any('서브: SW1 고정 -3.00 dB 12.50 ms 반전 · SW2 고정 -1.25 dB 4.00 ms 정상 AP 55 Hz Q 0.70' in v
               for k, v in notes.items() if '보정 후' in k)


def test_fixed_copy_of_the_auto_result_reproduces_it(fast_engine, monkeypatch, two_subs):  # noqa: F811
    """(c) 고정 경로 검증: 자동 결과 → [결과 값 가져오기] → 전부 고정 → 같은 결과. 버스 지연을 일부러 당겨 메인 추가 지연이
    0 이 아니게 한다(그래도 같아야 한다 — 가져오기가 메인 추가 지연도 옮긴다)."""
    from secs import calibration as cal
    from secs import correction_engine

    monkeypatch.setattr(correction_engine.CorrectionEngine, '_align_subs', _fixed_ba)
    monkeypatch.setattr(correction_engine.CorrectionEngine, '_bus_delay', lambda self, *a, **k: (-6.0, -1))
    project, irs = two_subs
    auto = cal.compute(project, irs, cal.CalibrationSettings(mode='mixed'))
    assert auto.engine.main_extra_ms > 2.0
    s = cal.CalibrationSettings(mode='mixed')
    s.take_sub_values(auto)
    for role in SUBS:
        s.set_sub(role, mode='fixed')
    assert s.sub_main_delay_ms == auto.engine.main_extra_ms
    fixed = cal.compute(project, irs, s)
    assert fixed.engine.manual_plan()['fixed'] == list(SUBS)
    for role in SUBS:
        x, y = auto.result.subs[role], fixed.result.subs[role]
        assert (y.gain_db, y.delay_ms, y.polarity, y.allpass) == (x.gain_db, x.delay_ms, x.polarity, x.allpass)
    assert fixed.latency_ms == pytest.approx(auto.latency_ms, abs=1e-9)
    for role in auto.result.channels:
        assert fixed.result.channels[role].delay_ms == pytest.approx(auto.result.channels[role].delay_ms, abs=1e-9)
        assert np.allclose(fixed.result.after[role], auto.result.after[role], rtol=1e-6, atol=1e-9)
        assert np.allclose(fixed.result.combined[role], auto.result.combined[role], rtol=1e-9, atol=1e-12)


def test_bundle_keeps_relative_values_and_moves_as_one(fast_engine, spies, two_subs):  # noqa: F811
    """서브 전부 한 묶음: DE 없이 기준값, 버스 게인·지연·극성은 묶음 전체에 — 안의 상대값(게인 차·딜레이 차·극성 곱)은 그대로
    (10-05 사용자 결정: 묶음 = 공통 게인·지연·극성 자동). 글자대로(`bundle_gain='fixed'`)는 엔진 손잡이로만 — 넣은 게인 그대로."""
    from secs import calibration as cal

    project, irs = two_subs
    s = _settings(SW1={'mode': 'bundle', 'gain_db': 0.0, 'delay_ms': 0.0, 'polarity': 1},
                  SW2={'mode': 'bundle', 'gain_db': -2.0, 'delay_ms': 1.5, 'polarity': -1})
    run = cal.compute(project, irs, s)
    eng, res = run.engine, run.result
    assert spies['align'] == 0 and spies['bus'] == 1 and eng.bundle_gain == 'auto'
    a, b = res.subs['SW1'], res.subs['SW2']
    assert a.mode == b.mode == 'bundle:A'
    bus_db = 20 * np.log10(eng.sub_state['bus_gain'])
    assert abs(bus_db) > 1.0 and a.gain_db == pytest.approx(bus_db)            # 묶음 공통 게인 = 서브 버스 게인
    assert b.gain_db - a.gain_db == pytest.approx(-2.0)
    assert b.delay_ms - a.delay_ms == pytest.approx(1.5) and a.polarity * b.polarity == -1
    # 묶음 기준 지연(첫 음향 도착 = 도착 정렬 시점) + 버스 지연 + 메인 추가 지연
    center = max(eng.align_delay_ms['SW1'], eng.align_delay_ms['SW2'] - 1.5)
    assert a.delay_ms == pytest.approx(center + res.sub_bus_delay_ms + eng.main_extra_ms)
    assert a.polarity == res.sub_bus_polarity
    assert not any('서브 버스 게인 안 걸음' in n for n in eng.notes)
    # 글자대로(엔진 손잡이): 묶음 게인은 넣은 값 그대로, 버스 게인 없음
    literal, _ = cal.make_engine(project, irs, s)
    literal.bundle_gain = 'fixed'
    lit = literal.compute()
    assert (lit.subs['SW1'].gain_db, lit.subs['SW2'].gain_db) == (0.0, -2.0)
    assert any('서브 버스 게인 안 걸음' in n for n in literal.notes) and literal.sub_level_db is not None


def test_bundle_and_auto_are_solved_in_one_ba(fast_engine, monkeypatch, two_subs):  # noqa: F811
    """묶음(SW1) + 자동(SW2): BA 변수 = 자동 7 + 묶음 3(공통 게인·지연·극성), 게인은 상대(−8…0 dB — 묶음은 평균 게인 기준) 뒤 버스 게인.
    고정(SW1) + 자동(SW2): 자동 서브 게인 범위 = 고정 서브 게인 평균 ±8 dB(절대값), 버스 게인 없음."""
    import scipy.optimize
    from secs import calibration as cal

    seen = {}

    def fake_de(cost, bounds, **kw):
        x = np.array([(lo + hi) / 2 for lo, hi in bounds])
        x[1], x[2] = 0.5, 0.9                       # 자동 미세 지연 · 반전
        if len(bounds) == 10:
            x[8], x[9] = -0.25, 0.9                 # 묶음 공통 지연 · 반전(7 = 묶음 공통 게인)
        seen.update(bounds=list(bounds), cost=cost(x), kw=kw)
        return scipy.optimize.OptimizeResult(x=x, fun=seen['cost'])

    monkeypatch.setattr(scipy.optimize, 'differential_evolution', fake_de)
    project, irs = two_subs
    s = _settings(SW1={'mode': 'bundle', 'gain_db': -4.0, 'delay_ms': 2.0, 'polarity': 1})
    run = cal.compute(project, irs, s)
    eng, res = run.engine, run.result
    assert len(seen['bounds']) == 10 and np.isfinite(seen['cost'])
    assert seen['kw']['seed'] == 1 and seen['kw']['maxiter'] == 60
    # 상대 게인: 자동 −8…0, 묶음 공통은 묶음 평균 게인(넣은 −4 + 공통)이 같은 −8…0 에 오게 −4…+4
    assert seen['bounds'][0] == (-8.0, 0.0) and seen['bounds'][7] == (-4.0, 4.0)
    assert res.subs['SW1'].mode == 'bundle:A' and res.subs['SW2'].mode == 'auto'
    # 게인: 묶음 = 넣은 −4 + 공통 0(가운데) + 버스, 자동 = −4(가운데) + 버스 → 같다
    assert res.subs['SW1'].gain_db - res.subs['SW2'].gain_db == pytest.approx(0.0)
    # 묶음 멤버 하나: 공통 지연 −0.25 · 반전이 버스 단계 뒤에도 자동 서브와의 관계로 남는다
    d_rel = (res.subs['SW1'].delay_ms - res.subs['SW2'].delay_ms)
    want = (max(eng.align_delay_ms['SW1'], 0) - 0.25) - (eng.align_delay_ms['SW2'] + 0.5)
    assert d_rel == pytest.approx(want)
    assert res.subs['SW1'].polarity * res.subs['SW2'].polarity == 1   # 둘 다 반전 → 서로는 같은 극성
    # 고정 + 자동: 자동 게인 범위는 고정 게인 기준 절대값, 버스 게인 없음
    s2 = _settings(SW1={'mode': 'fixed', 'gain_db': -4.0, 'delay_ms': 2.0, 'polarity': 1})
    run2 = cal.compute(project, irs, s2)
    assert len(seen['bounds']) == 7 and seen['bounds'][0] == (-12.0, 4.0)
    assert run2.result.subs['SW1'].gain_db == -4.0 and run2.result.subs['SW2'].gain_db == pytest.approx(-4.0)


def test_mimo_skips_unchecked_subs(fast_engine, fast_ba, two_subs, monkeypatch):  # noqa: F811
    """MIMO 서포트 체크를 끈 서브는 서포트 후보에서 빠진다 — `design_mimo` 가 받는 값과 실제 후보 둘 다."""
    from secs import calibration as cal
    from secs import mimo, mimo_iir

    project, irs = two_subs
    s = _settings(SW2={'mimo_support': False})
    run = cal.compute(project, irs, s)
    original = mimo.design_mimo
    got = {}

    def design_mimo(engine, **kw):
        got.update(kw)
        return original(engine, inputs=['L'], **kw)

    monkeypatch.setattr(mimo, 'design_mimo', design_mimo)
    monkeypatch.setattr(mimo_iir, 'design_mimo_iir', lambda engine, design, positions, **kw: {})
    mimo_run = cal.compute_mimo(run, exclude=s.excluded_supports())
    assert got['exclude'] == ('SW2',) and mimo_run.exclude == ('SW2',)
    assert mimo_run.key() == s.mimo_key() and mimo_run.key() != cal.CalibrationSettings().mimo_key()
    assert 'SW2' not in mimo_run.design.supports['L'] and 'SW1' in mimo_run.design.supports['L']
    assert any('서포트 제외(사용자): SW2' in n for n in mimo_run.notes)



def test_fixed_output_filters_shape_the_sub_everywhere(fast_engine, spies, two_subs, tmp_path):  # noqa: F811
    """SW2(자동)에 고정 HP 15 Hz BW 12 · LP 40 Hz LR 24 · PEQ 30 Hz −3 dB: 엔진의 SW2 응답(설계 창·MIMO 긴 창) = 원 응답 × 필터,
    SW1 은 그대로, BA 는 자동 경로 그대로, MIMO 서포트 대역은 통과 대역 기준(LP 25 Hz 면 상한 < 40 Hz), 결과·카밀라(버스 PEQ →
    고정 대역·PEQ → 올패스·딜레이, 그 서브로 가는 MIMO 갈래에도 같은 필터)·mdat 노트에 필터."""
    from secs import calibration as cal
    from secs import mimo, rew_mdat
    from secs.correction_engine import band_filter, lr4, peq_response

    project, irs = two_subs
    s = _settings(SW2={'hp_hz': 15.0, 'hp_type': 'BW12', 'lp_hz': 40.0, 'peq': [[30.0, 2.0, -3.0]]})
    run = cal.compute(project, irs, s)
    eng, res = run.engine, run.result
    assert spies['align'] == 1 and eng.manual_plan() is None           # 방식은 전부 자동 그대로
    raw, _ = cal.make_engine(project, irs, cal.CalibrationSettings(mode='zero'))
    raw._prepare()
    F = band_filter(eng.f, 15.0, 'HP', 'BW12') * lr4(eng.f, 40.0, 'LP') * peq_response(eng.f, [(30.0, 2.0, -3.0)])
    long_f, long_r = eng.long_spectra(), raw.long_spectra()
    for pos in eng.positions:
        assert np.allclose(eng.H0[pos]['SW2'], raw.H0[pos]['SW2'] * F)
        assert np.array_equal(eng.H0[pos]['SW1'], raw.H0[pos]['SW1'])
        assert np.allclose(long_f[pos]['SW2'], long_r[pos]['SW2'] * F)
    b = res.subs['SW2']
    assert (b.mode, b.hp_hz, b.hp_type, b.lp_hz, b.lp_type, b.peq) == ('auto', 15.0, 'BW12', 40.0, 'LR24', [(30.0, 2.0, -3.0)])
    assert res.subs['SW1'].lp_hz == 0.0 and any('서브 고정 필터: SW2 HP 15 BW12 LP 40 LR24 PEQ 1' in n for n in eng.notes)
    # MIMO 서포트 대역: 통과 대역 기준 → SW2 상한이 내려간다. INFRA 처럼 LP 25 Hz LR 24 면 상한 < 40 Hz(자기 LP 위를 돕지 않게)
    pos = list(eng.positions)
    with_f = mimo.support_ranges(eng, long_f, 150.0, pos)['SW2']
    without = mimo.support_ranges(raw, long_r, 150.0, pos)['SW2']
    assert without is not None and (with_f is None or with_f[1] < without[1])
    infra = cal.compute(project, irs, _settings(SW2={'lp_hz': 25.0}))
    rng = mimo.support_ranges(infra.engine, infra.engine.long_spectra(), 150.0, pos)['SW2']
    assert rng is None or rng[1] < 40.0, rng
    # 카밀라: 서브 체인 = 버스 PEQ → 고정 대역·PEQ → 딜레이(게인·극성은 버스 믹서), 종류는 BiquadCombo 로
    yml = cal.export_camilladsp(run, tmp_path / 'cam')[0].read_text(encoding='ascii')
    assert 'sub_SW2_fixed_hp: {"type": "BiquadCombo", "parameters": {"type": "ButterworthHighpass", "freq": 15.0, "order": 2}}' in yml
    assert 'sub_SW2_fixed_lp: {"type": "BiquadCombo", "parameters": {"type": "LinkwitzRileyLowpass", "freq": 40.0, "order": 4}}' in yml
    assert 'sub_SW2_fixed_peq0: {"type": "Biquad", "parameters": {"type": "Peaking", "freq": 30.0, "q": 2.0, "gain": -3.0}}' in yml
    chain = next(json.loads(line.strip()[2:])['names'] for line in yml.split('pipeline:')[1].splitlines()
                 if '"sub_SW2_delay"' in line)
    fixed_at = chain.index('sub_SW2_fixed_hp')
    assert chain[fixed_at:fixed_at + 3] == ['sub_SW2_fixed_hp', 'sub_SW2_fixed_lp', 'sub_SW2_fixed_peq0']
    assert all(n.startswith('sub_SW2_peq') for n in chain[:fixed_at]) and fixed_at > 0 and chain[-1] == 'sub_SW2_delay'
    assert 'fixed HP 15 Hz BW12, fixed LP 40 Hz LR24, fixed PEQ 30.0 Hz Q 2.000 -3.00 dB' in yml and 'sub_SW1_fixed' not in yml
    notes = {m.name: m.notes for m in rew_mdat.load(cal.export_before_after_mdat(run, tmp_path / 'm')).measurements}
    sw2 = next(v for k, v in notes.items() if k.endswith('SW2_1 보정 전(원 측정)'))
    assert 'HP 15 Hz BW 12 LP 40 Hz LR 24 PEQ 30 Hz Q 2.00 -3.0 dB' in sw2


def test_mimo_branches_to_a_filtered_sub_carry_its_filters(fast_engine, fast_ba, two_subs, tmp_path):  # noqa: F811
    """카밀라 MIMO 갈래가 고정 필터가 걸린 서브로 가면 갈래에도 같은 필터(엔진은 그 서브 응답에 필터를 곱해 갈래를 설계했다)."""
    from secs import calibration as cal
    from secs import mimo, mimo_iir
    from secs.export_camilladsp import make_config

    project, irs = two_subs
    run = cal.compute(project, irs, _settings(SW2={'lp_hz': 25.0}))
    design = mimo.MimoResult()
    design.ranges = {'SW2': (20.0, 32.0), 'SW1': (20.0, 120.0)}
    iir = {'L': {'SW2': mimo_iir.IirSupport(-6.0, 3.0, 1, [], []), 'SW1': mimo_iir.IirSupport(-6.0, 3.0, 1, [], [])}}
    text = make_config(run.engine, run.result, tmp_path, {}, {}, mimo=design, iir=iir)
    lines = {json.loads(line.strip()[2:])['names'][0]: json.loads(line.strip()[2:])['names']
             for line in text.split('pipeline:')[1].splitlines() if '"mimo_L_' in line}
    assert lines['mimo_L_SW2_gain'][-1] == 'sub_SW2_fixed_lp' and 'sub_SW2_fixed_lp' not in lines['mimo_L_SW1_gain']
