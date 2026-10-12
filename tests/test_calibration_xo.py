"""보정 설계의 XO(2026-09-27 — 요구 대장 R7·R8·R13, 사용자 "XO 에 따른 차이가 있으면 추천값 + XO 바").

'동작 → 변화' 로 본다:
- 그룹 XO 를 바꾸면 설정이 저장되고(.secs) 결과가 낡으며, 다시 계산하면 엔진이 그 XO 를 쓴다.
- [XO 추천 계산] = 후보마다 보정을 끝까지 계산해 그룹별 최고 점수 → 추천값이 설정에 들어간다. 서브가 없으면 못 한다.
- XO 보기: XO 를 옮기면 0.2 s 안에 메인·서브·합 곡선이 바뀐다(마지막 계산의 BA 를 그대로 쓴다).
"""
from __future__ import annotations

import copy
import time

import numpy as np
import pytest

from test_calibration_step import _compute, _page, _project, fast_engine, synth  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def _groups(project):
    return [g for g in project.config.groups() if g != 'sub']


def test_xo_setting_is_saved_and_makes_the_result_stale(synth):  # noqa: F811
    from secs import calibration as cal

    case, _irs = synth
    project = _project(case, 3)
    s = cal.CalibrationSettings.from_project(project)
    key = s.engine_key()
    group = _groups(project)[0]
    s.xo = {group: 110.0}
    s.store(project)
    back = cal.CalibrationSettings.from_project(project)
    assert back.xo == {group: 110.0}
    assert back.engine_key() != key
    project.extra['calibration']['xo'] = {group: 5000.0, 'x': 'abc'}          # 범위 밖·숫자 아님
    assert cal.CalibrationSettings.from_project(project).xo == {group: cal.XO_RANGE[1]}


def test_engine_uses_the_chosen_group_xo(fast_engine, synth):  # noqa: F811
    from secs import calibration as cal

    case, irs = synth
    project = _project(case, 3)
    group = _groups(project)[0]
    s = cal.CalibrationSettings(mode='zero', xo={group: 120.0})
    run = cal.compute(project, irs, s)
    assert run.engine.xo[group] == 120.0
    assert all(run.engine.chain[r]['xo'] == 120.0 for r in run.engine.groups[group])


def test_recommendation_scores_every_candidate_and_picks_the_best(fast_engine, synth):  # noqa: F811
    from secs import calibration as cal

    case, irs = synth
    project = _project(case, 3)
    seen = []
    rec = cal.recommend_xo(project, irs, cal.CalibrationSettings(mode='zero'), candidates=(60.0, 120.0), workers=1,
                           progress=lambda k, n: seen.append((k, n)))
    assert seen == [(1, 2), (2, 2)]
    for group, rows in rec.table.items():
        assert set(rows) == {60.0, 120.0}
        assert rec.best[group] == max(rows, key=lambda fc: rows[fc]['score'])
        assert all(np.isfinite(r['flat']) and np.isfinite(r['gd']) and np.isfinite(r['late']) for r in rows.values())
    # 두 후보는 실제로 다른 결과를 낸다(후보마다 엔진을 돌렸다)
    g = next(iter(rec.table))
    assert rec.table[g][60.0] != rec.table[g][120.0]


def test_score_prefers_flat_then_group_delay_and_decay():
    from secs import calibration as cal

    table = {'front': {80.0: {'flat': 90.0, 'gd': 10.0, 'late': -26.0}, 120.0: {'flat': 75.0, 'gd': 6.0, 'late': -27.0}},
             'surr': {80.0: {'flat': 60.0, 'gd': 10.0, 'late': -25.0}, 120.0: {'flat': 61.0, 'gd': 4.0, 'late': -27.0}}}
    best = cal.pick_xo(table)
    assert best == {'front': 80.0, 'surr': 120.0}            # 평탄이 크게 다르면 평탄, 비슷하면 GD·울림


def test_no_subwoofer_means_no_xo(fast_engine, synth):  # noqa: F811
    from secs import calibration as cal

    case, irs = synth
    project = _project(case, 3)
    no_sub = {p: {r: v for r, v in roles.items() if not r.startswith('SW')} for p, roles in irs.items()}
    assert cal.xo_groups(project, no_sub) == []
    with pytest.raises(cal.CalibrationError):
        cal.recommend_xo(project, no_sub, cal.CalibrationSettings(mode='zero'), candidates=(80.0,))


def test_xo_view_curves_follow_the_xo_quickly(fast_engine, synth):  # noqa: F811
    from secs import calibration as cal

    case, irs = synth
    project = _project(case, 3)
    run = cal.compute(project, irs, cal.CalibrationSettings(mode='zero'))
    group = _groups(project)[0]
    cal.xo_preview(run, group, 80.0)                          # 첫 호출은 서브 합을 캐시
    started = time.perf_counter()
    lo, hi = cal.xo_preview(run, group, 60.0), cal.xo_preview(run, group, 120.0)
    assert (time.perf_counter() - started) / 2 < 0.2
    band = (lo['grid'] >= 70) & (lo['grid'] <= 110)
    assert np.mean(hi['main'][band]) < np.mean(lo['main'][band]) - 3.0      # XO 를 올리면 메인이 그 대역을 덜 맡고
    assert np.mean(hi['sub'][band]) > np.mean(lo['sub'][band]) + 3.0        # 서브가 더 맡는다
    assert np.all(np.isfinite(lo['sum'])) and np.all(np.isfinite(lo['target']))


# ──────────────────────────── 화면 ────────────────────────────
def test_xo_tab_curves_follow_the_spin_and_the_line(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    case, irs = synth
    project = _project(case, 3)
    page = _page(project, irs, tmp_path)
    try:
        page.set_mode('zero')
        _compute(page, qapp)
        page.set_tab('xo')
        group = page.xo_group
        assert group and page.xo_group_combo.isEnabled() and page.xo_spin.value() == 80.0
        assert set(page._xo_curves) == {'target', 'main', 'sub', 'sum'}
        main80 = np.array(page._xo_curves['main'].getData()[1])
        started = time.perf_counter()
        page.xo_spin.setValue(120.0)                            # Hz 칸 → 설정·곡선
        assert time.perf_counter() - started < 0.2 * 3            # 화면 전체 다시 그리기 포함(곡선만은 아래 선 끌기)
        assert project.extra['calibration']['xo'][group] == 120.0
        assert page.stale() and '설정이 바뀌었습니다' in page.status.text()
        main120 = np.array(page._xo_curves['main'].getData()[1])
        assert np.max(np.abs(main120 - main80)) > 3.0
        # 선을 끄는 중: 곡선·칸만(설정은 놓을 때)
        started = time.perf_counter()
        page.xo_line.setValue(np.log10(100.0))
        assert time.perf_counter() - started < 0.2
        assert page.xo_spin.value() == 100.0 and project.extra['calibration']['xo'][group] == 120.0
        assert np.max(np.abs(np.array(page._xo_curves['main'].getData()[1]) - main120)) > 1.0
        page.xo_line.sigPositionChangeFinished.emit(page.xo_line)
        assert project.extra['calibration']['xo'][group] == 100.0
        assert page.legend_items['xo_sum'].isVisibleTo(page) and not page.legend_items['before'].isVisibleTo(page)
    finally:
        page.deleteLater()


def test_recommend_button_puts_the_best_xo_and_recalculates(qapp, fast_engine, synth, tmp_path, monkeypatch):  # noqa: F811
    from secs import calibration as cal

    original = cal.recommend_xo
    monkeypatch.setattr(cal, 'recommend_xo', lambda *a, **k: original(*a, candidates=(60.0, 120.0), workers=1, **k))
    case, irs = synth
    project = _project(case, 3)
    page = _page(project, irs, tmp_path)
    try:
        page.set_mode('zero')
        page.settings.xo_rec = {}                               # 추천 전으로(_page 는 '이미 추천함' 으로 둔다)
        page.settings.store(project)
        page.refresh()
        assert page.xo_rec_label.text() == '추천 전'
        count = page.compute_count
        page.xo_button.click()
        assert page.xo_running() and 'XO 추천 계산 중' in page.xo_status.text()
        assert page.wait(240000), 'XO 추천이 끝나지 않았다'
        assert page.xo_recommendation is not None, page.xo_status.text()
        best = page.xo_recommendation.best
        assert best and all(v in (60.0, 120.0) for v in best.values()), (best, page.xo_recommendation.table)
        assert page.wait(180000)                                # wait 는 추천 뒤 이어지는 재계산까지 기다린다
        qapp.processEvents()
        assert page.compute_count == count + 1, '추천 뒤 그 XO 로 보정을 다시 계산해야 한다'
        assert all(page.run.engine.xo[g] == hz for g, hz in best.items())
        assert project.extra['calibration']['xo_rec'] == best
        assert page.xo_rec_label.text() == f'추천 {best[page.xo_group]:g} Hz'
        assert 'XO 추천 완료' in page.xo_status.text() and not page.stale()
    finally:
        page.deleteLater()


def test_without_subwoofer_the_xo_controls_are_off(qapp, synth, tmp_path):  # noqa: F811
    case, irs = synth
    project = _project(case, 3)
    project.config.set_sub_count(0)
    page = _page(project, irs, tmp_path)
    try:
        assert page.xo_group_names() == []
        assert not page.xo_group_combo.isEnabled() and not page.xo_spin.isEnabled() and not page.xo_button.isEnabled()
        page.set_tab('xo')                                      # XO 카드는 XO 탭에서만 보인다(10-09)
        assert page.xo_reason.isVisibleTo(page) and '서브가 없어' in page.xo_reason.text()
        assert page.recommend_xo() is False
    finally:
        page.deleteLater()
