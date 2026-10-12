"""보정 설계 화면 정리(2026-10-09 사용자 요청 5가지).

'동작 → 변화' 로 본다:
- 탭을 바꾸면 오른쪽 설정 카드가 탭에 맞춰 바뀐다(SPL·GD = RC, XO = XO, BA = BA 설정). 값은 그대로.
- SPL 그래프 빈 곳 우클릭 메뉴의 프리셋을 누르면 타겟이 바뀌고 체크 표시가 따라온다.
- XO 추천을 한 번도 안 한 프로젝트에서 [계산] 을 누르면 XO 추천부터 돌고(취소 가능), 끝나면 추천 XO 로 보정이 계산된다.
- SPL·GD 탭에 RC 하한·상한 세로선이 계산이 쓴 값(자동 상한 포함) 자리에 선다.
- XO 탭 곡선은 보정 필터가 걸린 응답이다(보정 전과 다르고, XO 를 옮겨도 0.2 s 안).
"""
from __future__ import annotations

import time

import numpy as np

from test_calibration_step import _compute, _page, _project, fast_engine, synth  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def test_side_cards_follow_the_tab_and_keep_values(qapp, synth, tmp_path):  # noqa: F811
    case, irs = synth
    project = _project(case, 1)
    project.config.set_sub_count(2)
    page = _page(project, irs, tmp_path)
    try:
        page.set_room(volume_m3=120.0)
        shown = {}
        for tab in ('spl', 'gd', 'xo', 'subs'):
            page.tab_buttons[tab].click()
            shown[tab] = (page.rc_card.isVisibleTo(page), page.xo_card.isVisibleTo(page), page.sub_card.isVisibleTo(page))
        assert shown == {'spl': (True, False, False), 'gd': (True, False, False),
                         'xo': (False, True, False), 'subs': (False, False, True)}
        page.tab_buttons['spl'].click()
        assert page.volume_spin.value() == 120.0 and page.settings.room_volume_m3 == 120.0
        assert not hasattr(page, 'target_buttons')
    finally:
        page.deleteLater()


def test_right_click_menu_applies_preset(qapp, synth, tmp_path):  # noqa: F811
    from secs.model import TargetCurve

    case, irs = synth
    project = _project(case, 1)
    page = _page(project, irs, tmp_path)
    try:
        menu = page.target_menu()
        next(a for a in menu.actions() if a.data() == 'cinema').trigger()
        assert project.target.points == TargetCurve.presets()['cinema'].points
        checked = [a.data() for a in page.target_menu().actions() if a.isCheckable() and a.isChecked()]
        assert checked == ['cinema'] and page.compute_count == 0
    finally:
        page.deleteLater()


def test_first_compute_runs_xo_recommendation_then_calibration(qapp, fast_engine, synth, tmp_path, monkeypatch):  # noqa: F811
    from secs import calibration as cal
    from secs.wizard.step_calibration import StepCalibration

    case, irs = synth
    original = cal.recommend_xo
    monkeypatch.setattr(cal, 'recommend_xo', lambda *a, **k: original(*a, candidates=(60.0, 120.0), workers=1, **k))
    project = _project(case, 1)
    page = StepCalibration(project)
    page.set_context(tmp_path / 'c.secs', lambda: irs)
    try:
        assert page.needs_first_xo()
        page.compute_button.click()
        assert page.xo_running() and 'XO' in page.status.text()
        page.cancel()                                          # 취소하면 아무것도 안 남는다
        assert not page.working() and page.run is None and page.needs_first_xo()
        page.compute_button.click()
        assert page.xo_running()
        assert page.wait(180000)
        qapp.processEvents()
        assert page.run is not None and page.compute_count == 1
        assert not page.needs_first_xo()
        for group, hz in page.settings.xo_rec.items():
            assert hz in (60.0, 120.0) and page.run.engine.xo[group] == hz
        page.mode_buttons['mixed'].click()
        page.compute_button.click()                            # 두 번째부터는 보정만
        assert page.running()
        assert page.wait(180000)
    finally:
        page.deleteLater()


def test_rc_lines_sit_at_the_limits_used(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    case, irs = synth
    page = _page(_project(case, 1), irs, tmp_path)
    try:
        page.rc_lo_spin.setValue(30.0)
        page.tab_buttons['spl'].click()
        assert page.rc_lines['lo'][0] == 30.0 and 'hi' not in page.rc_lines      # 자동 상한은 계산 전엔 모른다
        run = _compute(page, qapp)
        for tab in ('spl', 'gd'):
            page.tab_buttons[tab].click()
            (lo, lo_line), (hi, hi_line) = page.rc_lines['lo'], page.rc_lines['hi']
            assert lo == 30.0 and hi == run.rc_hi
            assert abs(lo_line.value() - np.log10(30.0)) < 1e-9 and abs(hi_line.value() - np.log10(run.rc_hi)) < 1e-9
            assert f'{run.rc_hi:.0f} Hz' in hi_line.label.toPlainText()
        page.tab_buttons['xo'].click()
        assert page.rc_lines == {}
    finally:
        page.deleteLater()


def test_xo_view_shows_corrected_response_and_stays_fast(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    from secs import calibration as cal

    case, irs = synth
    page = _page(_project(case, 1), irs, tmp_path)
    try:
        run = _compute(page, qapp)
        group = page.xo_group
        hz = page.xo_of(group)
        raw = cal.xo_preview(run, group, hz, corrected=False)
        corr = cal.xo_preview(run, group, hz, role=page.channel)
        assert float(np.max(np.abs(corr['main'] - raw['main']))) > 0.5          # 보정 필터가 걸렸다
        # 계산 때의 XO 에서 합 곡선 = 엔진의 보정 후 응답(그룹 평균, 같은 스무딩) — 다른 길로 그린 게 아니다
        f = run.result.f
        roles = [r for r in run.engine.groups[group] if r in run.result.after]
        power = np.mean([np.abs(run.result.after[r]) ** 2 for r in roles], axis=0)
        ref = np.interp(cal.XO_GRID, f, 10 * np.log10(cal.smooth_power(f, power, cal.XO_FRAC) + 1e-30))
        band = (cal.XO_GRID >= 30) & (cal.XO_GRID <= 400)
        assert float(np.max(np.abs(corr['sum'][band] - ref[band]))) < 1.0, float(np.max(np.abs(corr['sum'][band] - ref[band])))
        started = time.perf_counter()
        cal.xo_preview(run, group, hz * 1.3)
        assert time.perf_counter() - started < 0.2
        page.tab_buttons['xo'].click()
        # 화면은 SPL 탭과 같은 타겟 기준으로 옮겨 그린다(10-10 3차 시험) — 곡선 모양은 그대로, 높이만 offset 만큼
        assert np.allclose(page._xo_curves['main'].yData, corr['main'] - page._level_offset(run))
    finally:
        page.deleteLater()
