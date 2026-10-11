"""보정 설계 '서브 정렬'(BA) 보기(2026-10-04 — 요구 대장 R9, 사용자 "서브가 여러 개면 서브끼리 합치는 것도 보여 줘").

'동작 → 변화' 로 본다(곡선이 있다류는 근거가 아니다):
- `calibration.sub_combine` 의 숫자 = 엔진 값: 정렬 값은 `CorrectionResult.subs` 그대로, 정렬 후 서브 합은 엔진의 합
  (`CorrectionEngine._sub_sum` + 최종 매개변수)을 같은 스무딩으로 편 것과 같고, 서브마다 출력의 합 = 엔진의 합.
- 화면: 서브가 2개 이상이면 [서브 정렬] 탭이 보이고, 누르면 그래프 곡선이 그 보기의 배열 그대로 바뀌고, 숫자 줄 ·
  정렬 값 표가 엔진 값과 같은 글자다. 다른 탭으로 가면 표가 숨는다. 서브가 1개면 탭이 없다.
서브 정렬 DE(수십 초)는 결정적인 고정 정렬로 바꿔 끼운다 — 엔진 수학이 아니라 보기가 엔진 값을 그대로 옮기는지를 본다.
"""
from __future__ import annotations

import copy

import numpy as np
import pytest

from test_calibration_step import _compute, _page, fast_engine  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

SUBS = ('SW1', 'SW2')


def _fixed_ba(self, hi_hz):
    """`CorrectionEngine._align_subs` 대역 — 도착 정렬 + 서브마다 다른 게인·미세 지연·극성·올패스(DE 없이)."""
    base = {r: self.align_delay_ms[r] for r in self.subs}
    return [(-1.5 * k, base[r] + 0.4 * k, -1 if k == 1 else 1, [(60.0, 0.8)] if k == 1 else [])
            for k, r in enumerate(self.subs)]


@pytest.fixture(scope='module')
def fast_ba(engine):  # noqa: F811
    from secs import correction_engine

    with pytest.MonkeyPatch.context() as mp:
        mp.setattr(correction_engine.CorrectionEngine, '_align_subs', _fixed_ba)
        yield


@pytest.fixture(scope='module')
def two_subs(engine):  # noqa: F811
    """합성 방 2.2(서브 2개), 위치 3곳."""
    import room_model_decay as rmd

    case = rmd.build_case('2.2', positions=3)
    project = copy.deepcopy(case.project)
    project.config.set_sub_count(2)
    roles = ('L', 'R') + SUBS
    project.measurements = [{'pos': p, 'ir': {r: f'{r}_P{p}.wav' for r in roles}} for p in range(1, 4)]
    irs = {pos: {r: v for r, v in by_role.items() if r in roles} for pos, by_role in case.irs.items()}
    return project, irs


def _engine_sum_db(run, pos=None):
    """엔진이 합에 쓰는 서브 합(최종 매개변수, 원 측정, 서브 버스 PEQ 까지 — 10-09) → 보기 격자 dB."""
    from secs import calibration as cal
    from secs.correction_engine import smooth_power

    eng = run.engine
    pos = eng.main_position if pos is None else pos
    saved, eng.H = eng.H, eng.H0
    try:
        total = eng._sub_sum(eng.sub_state['params'], np.arange(eng.f.size), [pos])[pos]
    finally:
        eng.H = saved
    if eng.sub_state['bus_P'] is not None:
        total = total * eng.sub_state['bus_P']
    return total, np.interp(cal.SUB_GRID, eng.f, 10 * np.log10(smooth_power(eng.f, np.abs(total) ** 2, cal.XO_FRAC) + 1e-30))


# ──────────────────────────── 계산 층 ────────────────────────────
def test_sub_combine_numbers_are_the_engine_values(fast_engine, fast_ba, two_subs):  # noqa: F811
    from secs import calibration as cal
    from secs.correction_engine import smooth_power

    project, irs = two_subs
    run = cal.compute(project, irs, cal.CalibrationSettings(mode='zero'))
    view = cal.sub_combine(run)
    assert view is not None and view['subs'] == list(SUBS) and view['positions'] == 3
    # 정렬 값 = CorrectionResult.subs(시트·내보내기 값) 그대로
    for value in view['values']:
        s = run.result.subs[value['role']]
        assert (value['gain_db'], value['delay_ms'], value['polarity']) == (s.gain_db, s.delay_ms, s.polarity)
        assert value['allpass'] == [tuple(a) for a in s.allpass] and value['mode'] == 'auto'
    assert view['values'][1]['polarity'] == -1 and view['values'][1]['allpass'] == [(60.0, 0.8)]
    # 정렬 후 서브 합 = 엔진의 합
    total, db = _engine_sum_db(run)
    assert np.allclose(view['sum_after'], db)
    eng = run.engine
    each = [cal._sub_out(eng, r, p, eng.H0[eng.main_position][r]) for r, p in zip(eng.subs, eng.sub_state['params'])]
    bus = eng.sub_state['bus_P']
    assert bus is not None                      # 이 합성 방은 XO 가 있어 서브 버스 PEQ 가 걸린다 — 보기의 정렬 후에 들어가야 한다
    assert np.allclose(sum(each) * bus, total)
    assert np.allclose(view['after'][eng.subs[0]], np.interp(cal.SUB_GRID, eng.f, 10 * np.log10(
        smooth_power(eng.f, np.abs(each[0] * bus) ** 2, cal.XO_FRAC) + 1e-30)))
    # 정렬 전 = 그대로 더한 것 — 고정 정렬(극성 반전·게인)이 들어가니 대역 안에서 달라진다
    lo, hi = view['band']
    assert (lo, hi) == (20.0, max(run.engine.xo.values()) * 1.5)
    band = (cal.SUB_GRID >= lo) & (cal.SUB_GRID <= hi)
    assert np.max(np.abs(view['sum_before'][band] - view['sum_after'][band])) > 3.0
    raw = sum(eng.H0[eng.main_position][r] for r in SUBS)
    assert np.allclose(view['sum_before'], np.interp(cal.SUB_GRID, eng.f, 10 * np.log10(
        smooth_power(eng.f, np.abs(raw) ** 2, cal.XO_FRAC) + 1e-30)))
    # 평탄도 = 1/6 oct 서브 합의 대역 안 표준편차, 좌석 편차 = 위치 사이 표준편차의 평균
    grid = np.geomspace(lo, hi, 200)

    def level(H):
        return np.interp(grid, eng.f, 10 * np.log10(smooth_power(eng.f, np.abs(H) ** 2, cal.SUB_METRIC_FRAC) + 1e-30))

    assert view['flat'][1] == pytest.approx(float(np.std(level(total))))
    assert view['flat'][0] == pytest.approx(float(np.std(level(raw))))
    sums = [_engine_sum_db(run, p)[0] for p in eng.positions]
    assert view['seat'][1] == pytest.approx(float(np.mean(np.std([level(s) for s in sums], axis=0))))
    assert cal.sub_combine(run) is view                       # 한 번 계산(탭을 오갈 때 다시 풀지 않는다)


def test_sub_bus_fix_limits_bus_peq_and_never_worsens_ba_cost(fast_engine, fast_ba, two_subs, monkeypatch):  # noqa: F811
    """10-09 `SUB_BUS_FIX`(기본 끔): 켜면 서브 버스 PEQ 밴드 중심이 1.25 × 가장 높은 XO 안이고, BA 2차는 버스 PEQ 를
    건 정확한 비용이 1차 답보다 커지지 않는다. 끄면 2차는 1차 답을 그대로 돌려준다."""
    from secs import calibration as cal
    from secs import correction_engine as de

    project, irs = two_subs
    monkeypatch.setattr(de, 'SUB_BUS_FIX', True)
    monkeypatch.setattr(de, 'BA_BUS_ITERS', 2)
    run = cal.compute(project, irs, cal.CalibrationSettings(mode='zero'))
    eng = run.engine
    xo_max = max(v for g, v in eng.xo.items() if g != 'sub' and v)
    assert eng.sub_bus_peq and all(f0 <= de.BUS_PEQ_XO_RATIO * xo_max + 1e-6 for f0, _q, _g in eng.sub_bus_peq)
    s = eng._ba_setup(xo_max * 1.5)
    base = [eng.align_delay_ms[r] for r in eng.subs]
    bounds = [(-8, 0), (-3, 3), (0, 1)] * len(eng.subs)

    def unpack(x):
        return [(x[3 * k], base[k] + x[3 * k + 1], 1 if x[3 * k + 2] < 0.5 else -1, []) for k in range(len(eng.subs))]

    x1 = np.array([0.0, 0.0, 0.2, -6.0, 2.5, 0.8])       # 일부러 나쁜 1차 답(반전 · 지연 2.5 ms)

    def exact(x):
        return eng._ba_cost(unpack(x), s, None, eng._bus_peq_of(unpack(x)))

    x2 = eng._ba_bus_pass(unpack, bounds, s, None, x1)
    assert exact(x2) <= exact(x1)
    monkeypatch.setattr(de, 'SUB_BUS_FIX', False)
    assert eng._ba_bus_pass(unpack, bounds, s, None, x1) is x1


def test_one_sub_has_no_sub_combine(fast_engine, two_subs):  # noqa: F811
    from secs import calibration as cal

    project, irs = two_subs
    one = copy.deepcopy(project)
    one.config.set_sub_count(1)
    one.measurements = [{'pos': m['pos'], 'ir': {r: v for r, v in m['ir'].items() if r != 'SW2'}} for m in one.measurements]
    run = cal.compute(one, {p: {r: v for r, v in d.items() if r != 'SW2'} for p, d in irs.items()},
                      cal.CalibrationSettings(mode='zero'))
    assert run.engine.subs == ['SW1'] and cal.sub_combine(run) is None


# ──────────────────────────── 화면 ────────────────────────────
def _cells(page) -> list[list[str]]:
    grid = page.sub_grid
    rows = []
    for r in range(grid.rowCount()):
        row = []
        for c in range(grid.columnCount()):
            item = grid.itemAtPosition(r, c)
            if item is not None and item.widget() is not None:
                row.append(item.widget().text())
        if row:
            rows.append(row)
    return rows


def test_sub_tab_shows_the_engine_values(qapp, fast_engine, fast_ba, two_subs, tmp_path):  # noqa: F811
    import pyqtgraph as pg
    from secs import calibration as cal
    from secs.model import role_code

    project, irs = two_subs
    page = _page(project, irs, tmp_path)
    try:
        assert page.tab_buttons['subs'].isVisibleTo(page)          # 구성에 서브 2개 → 계산 전에도 탭이 있다
        page.mode_buttons['zero'].click()
        run = _compute(page, qapp)
        page.tab_buttons['subs'].click()
        assert page.tab == 'subs' and page.sub_values.isVisibleTo(page) and not page.channel_combo.isEnabled()
        for key in ('sub_before', 'sub_after', 'sub_sum', 'target'):     # 타깃도 그린다(10-11 사용자)
            assert page.legend_items[key].isVisibleTo(page)
        for key in ('before', 'after', 'xo_sum'):
            assert not page.legend_items[key].isVisibleTo(page)
        view = cal.sub_combine(run)
        # 그래프 = 보기 배열 그대로(서브마다 전·후 + 합 전·후)
        drawn = [item.getData()[1] for item in page.plot.getPlotItem().items if isinstance(item, pg.PlotDataItem)]
        # 맨 앞은 타깃 커브(10-11 사용자: "서브정렬에도 타겟 커브") — 지금 프로젝트 타깃 그대로
        assert np.allclose(drawn[0], page.project.target.sample(view['grid']))
        drawn = drawn[1:]
        expected = [view[s][r] for r in view['subs'] for s in ('before', 'after')] + [view['sum_before'], view['sum_after']]
        assert len(drawn) == len(expected)
        for got, want in zip(drawn, expected):
            # 10-10: 서브 정렬 탭도 SPL 탭처럼 타깃 기준으로 옮겨 그린다(결정 1A · 3차 시험) — 모양은 엔진 값 그대로, 차이는 상수
            assert np.ptp(np.asarray(got) - np.asarray(want)) < 1e-6
        # 숫자 줄 · 정렬 값 표 = 엔진 값의 글자
        lo, hi = view['band']
        assert page.metrics_label.text() == (
            f'서브 합 평탄도({lo:.0f}–{hi:.0f} Hz) {view["flat"][0]:.1f} → {view["flat"][1]:.1f} dB · '
            f'좌석 편차 {view["seat"][0]:.1f} → {view["seat"][1]:.1f} dB')
        rows = _cells(page)
        assert rows[0] == ['서브', '방식', '게인', '딜레이', '극성', '올패스']
        for row, role in zip(rows[1:], SUBS):
            s = run.result.subs[role]
            ap = ', '.join(f'{f0:.0f} Hz Q {q:.1f}' for f0, q in s.allpass) or '없음'
            assert row == [f'━ {role}', '자동', f'{s.gain_db:+.1f} dB', f'{s.delay_ms:.2f} ms',
                           '정상' if s.polarity > 0 else '반전', ap]
        assert rows[2][4] == '반전' and rows[2][5] == '60 Hz Q 0.8'
        # 다른 탭으로 가면 표·서브 범례가 숨고 채널을 다시 고를 수 있다
        page.tab_buttons['spl'].click()
        assert not page.sub_values.isVisibleTo(page) and page.channel_combo.isEnabled()
        assert not page.legend_items['sub_sum'].isVisibleTo(page)
    finally:
        page.deleteLater()


def test_sub_tab_is_hidden_with_one_sub(qapp, two_subs, tmp_path):  # noqa: F811
    project, irs = two_subs
    one = copy.deepcopy(project)
    one.config.set_sub_count(1)
    page = _page(one, irs, tmp_path)
    try:
        assert not page.tab_buttons['subs'].isVisibleTo(page)
        page.set_tab('subs')
        assert page.tab == 'spl'
    finally:
        page.deleteLater()


def test_sub_tab_is_english_without_korean_left(qapp, fast_engine, fast_ba, two_subs, tmp_path):  # noqa: F811
    import re

    from PyQt6 import QtWidgets
    from secs.i18n import language_manager

    project, irs = two_subs
    page = _page(project, irs, tmp_path)
    try:
        page.mode_buttons['zero'].click()
        _compute(page, qapp)
        page.set_tab('subs')
        language_manager().set_language('en', persist=False)
        qapp.processEvents()
        page.refresh()
        hangul = re.compile('[가-힣]')
        left = [w.text() for w in page.findChildren(QtWidgets.QWidget)
                if isinstance(w, (QtWidgets.QLabel, QtWidgets.QAbstractButton)) and w.isVisibleTo(page)
                and hangul.search(w.text())]
        assert not left, left
        assert page.tab_buttons['subs'].text() == 'Sub alignment'
        assert 'Sub sum flatness' in page.metrics_label.text()
    finally:
        language_manager().set_language('ko', persist=False)
        page.deleteLater()
