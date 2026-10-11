"""서브 정렬 탭 정리(2026-10-09 사용자): 인프라 XO 줄 · [서브만 계산] · 보정 설계에서 내보내기 카드 빠짐 · 묶음 → Group.

'동작 → 변화' 로 본다."""
from __future__ import annotations

import copy

from test_calibration_step import _compute, _page, fast_engine  # noqa: F401
from test_calibration_sub_combine import fast_ba, two_subs  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def test_infra_row_sets_lp_on_infra_and_hp_on_the_rest(qapp, two_subs, tmp_path):  # noqa: F811
    project, irs = two_subs
    page = _page(copy.deepcopy(project), irs, tmp_path)
    try:
        page.set_tab('subs')
        assert page.infra_combo.currentData() == '' and not page.infra_hz_spin.isVisibleTo(page)
        page.infra_combo.setCurrentIndex(page.infra_combo.findData('SW2'))
        sw1, sw2 = page.settings.sub('SW1'), page.settings.sub('SW2')
        assert sw2['lp_hz'] > 0 and sw2['hp_hz'] == 0 and sw1['hp_hz'] == sw2['lp_hz'] and sw1['lp_hz'] == 0
        assert page.infra_hz_spin.isVisibleTo(page)
        page.infra_hz_spin.setValue(30.0)
        page.infra_type_combo.setCurrentIndex(page.infra_type_combo.findData('LR48'))
        sw1, sw2 = page.settings.sub('SW1'), page.settings.sub('SW2')
        assert (sw2['lp_hz'], sw2['lp_type'], sw1['hp_hz'], sw1['hp_type']) == (30.0, 'LR48', 30.0, 'LR48')
        assert page.infra_state() == ('SW2', 30.0, 'LR48') and 'SW2' in page.infra_note.text()
        page.infra_combo.setCurrentIndex(page.infra_combo.findData(''))     # 끄면 넣었던 LP·HP 가 빠진다
        assert page.settings.sub('SW2')['lp_hz'] == 0 and page.settings.sub('SW1')['hp_hz'] == 0
    finally:
        page.deleteLater()


def test_subs_only_compute_redraws_sub_tab_and_keeps_full_result(qapp, fast_engine, fast_ba, two_subs, tmp_path):  # noqa: F811
    project, irs = two_subs
    page = _page(copy.deepcopy(project), irs, tmp_path)
    try:
        page.mode_buttons['zero'].click()
        run = _compute(page, qapp)
        page.set_tab('subs')
        page.infra_combo.setCurrentIndex(page.infra_combo.findData('SW2'))
        assert page.sub_compute_button.isEnabled()
        page.sub_compute_button.click()
        assert page.wait(180000)
        qapp.processEvents()
        assert page.run is run and page.sub_run is not None          # 메인 결과는 그대로
        assert not page.sub_run.result.channels and page.sub_run.result.subs['SW2'].lp_hz > 0
        _compute(page, qapp)
        assert page.sub_run is None                                   # 전체 계산이 서브 결과를 대신한다
    finally:
        page.deleteLater()


def test_no_export_card_on_calibration_page_and_group_wording(qapp, two_subs, tmp_path):  # noqa: F811
    project, irs = two_subs
    page = _page(copy.deepcopy(project), irs, tmp_path)
    try:
        assert not page.export_card.isVisibleTo(page)
        texts = [page.sub_mode_combo.itemText(i) for i in range(page.sub_mode_combo.count())]
        assert 'Group A' in texts and not any('묶음' in x for x in texts)
    finally:
        page.deleteLater()


def test_two_way_lifts_infra_up_to_the_limit(qapp, fast_engine, fast_ba, two_subs, tmp_path):  # noqa: F811
    """2-way 서브 대역 레벨 맞춤 — 저역 서브 게인이 올라가고, 상한 칸이 그 양을 묶는다(0 이면 올리지 않는다)."""
    from secs import calibration as cal

    project, irs = two_subs
    page = _page(copy.deepcopy(project), irs, tmp_path)
    try:
        page.set_infra('SW2', 40.0, 'LR24')
        gains = {}
        for cap in (24.0, 0.0):
            page.infra_boost_spin.setValue(cap)
            run = cal.compute(page.project, irs, page.settings, subs_only=True)
            gains[cap] = run.result.subs['SW2'].gain_db - run.result.subs['SW1'].gain_db
            assert run.engine.two_way_split() == (['SW2'], ['SW1'], 40.0)
        assert gains[24.0] >= gains[0.0] - 1e-6
        assert cal.CalibrationSettings.from_project(page.project).infra_boost_max_db == 0.0   # 저장
    finally:
        page.deleteLater()
