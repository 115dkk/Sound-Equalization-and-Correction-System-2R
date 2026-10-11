"""요구 대장 R27(2026-10-05) 화면: 보정 설계의 '서브 정렬 설정' 카드(서브 2개 이상)와 서브 정렬 탭의 '방식' 열.

'동작 → 변화' 로 본다: 방식 고르기·값 칸·MIMO 체크가 설정(`project.extra['calibration']['subs_manual']`)에 들어가고 결과가
낡는다, 자동이면 값 칸이 꺼진다, [결과 값 가져오기]가 결과 값을 반올림 없이 옮긴다, 다시 계산하면 표의 방식·값이 바뀐다,
서브 1개면 카드가 없다, 영어에 한글이 안 남는다. 엔진·저장·내보내기는 `test_calibration_r27_sub_manual.py`.
"""
from __future__ import annotations

import copy

import pytest

from test_calibration_step import _compute, _page, fast_engine  # noqa: F401
from test_calibration_sub_combine import _cells, fast_ba, two_subs  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

SUBS = ('SW1', 'SW2')


# ──────────────────────────── 화면 ────────────────────────────
def _combo_texts(combo):
    return [combo.itemText(i) for i in range(combo.count())]


def _card_shows(page, role):
    """카드의 칸 = 그 서브의 저장 값(스핀은 표시 자릿수만큼) — 10-05 리드: 캡처의 HP·LP 칸이 요약 줄과 다르게 보였다."""
    e = page.settings.sub(role)
    assert page.sub_pick.currentData() == role
    assert page.sub_mode_combo.currentData() == (f"bundle:{e['bundle']}" if e['mode'] == 'bundle' else e['mode'])
    assert page.sub_gain_spin.value() == pytest.approx(e['gain_db'], abs=0.006)
    assert page.sub_delay_spin.value() == pytest.approx(e['delay_ms'], abs=0.006)
    assert page.sub_pol_combo.currentData() == e['polarity']
    for k, (f_spin, q_spin) in enumerate(page.sub_ap_spins):
        assert f_spin.value() == pytest.approx(e['allpass'][k][0] if k < len(e['allpass']) else 0.0, abs=0.5)
    for kind in ('hp', 'lp'):
        hz = e[f'{kind}_hz']
        assert getattr(page, f'sub_{kind}_spin').value() == pytest.approx(hz, abs=0.5)
        assert getattr(page, f'sub_{kind}_spin').isEnabled() == (hz > 0)
        assert page.sub_band_combos[kind].currentData() == (e[f'{kind}_type'] if hz > 0 else 'off')
    for k, (f_spin, q_spin, g_spin) in enumerate(page.sub_peq_spins):
        band = e['peq'][k] if k < len(e['peq']) else (0.0, 1.0, 0.0)
        assert (f_spin.value(), q_spin.value(), g_spin.value()) == pytest.approx(tuple(band), abs=0.06)
    assert page.sub_mimo_check.isChecked() == e['mimo_support']


def test_sub_card_sets_modes_and_values_and_takes_result_values(qapp, fast_engine, fast_ba, two_subs, tmp_path):  # noqa: F811
    project, irs = two_subs
    page = _page(copy.deepcopy(project), irs, tmp_path)
    try:
        page.set_tab('subs')                                    # 서브 정렬 설정 카드는 서브 정렬 탭에서만(10-09)
        assert page.sub_card.isVisibleTo(page)
        assert [page.sub_pick.itemData(i) for i in range(page.sub_pick.count())] == list(SUBS)
        assert _combo_texts(page.sub_mode_combo) == ['자동', '고정', 'Group A', 'Group B']
        assert not page.sub_gain_spin.isEnabled() and not page.sub_main_spin.isVisibleTo(page)
        assert page.sub_summary.text() == 'SW1 자동 · SW2 자동'
        page.mode_buttons['zero'].click()
        run = _compute(page, qapp)
        # [결과 값 가져오기] → 모든 서브 칸에 결과 값(방식은 그대로)
        page.sub_take_button.click()
        stored = page.project.extra['calibration']['subs_manual']
        for role in SUBS:
            r = run.result.subs[role]
            assert stored[role]['mode'] == 'auto'
            assert (stored[role]['gain_db'], stored[role]['delay_ms'], stored[role]['polarity']) == (
                r.gain_db, r.delay_ms, r.polarity)
        assert not page.stale()                     # 자동 서브의 칸 값은 결과를 바꾸지 않는다
        # SW2 고정 → 저장, 값 칸 켜짐, 메인 추가 지연 줄 보임, 결과 낡음
        page.set_sub_role('SW2')
        page.sub_mode_combo.setCurrentIndex(page.sub_mode_combo.findData('fixed'))
        assert page.project.extra['calibration']['subs_manual']['SW2']['mode'] == 'fixed'
        assert page.sub_gain_spin.isEnabled() and page.sub_main_spin.isVisibleTo(page)
        assert page.sub_gain_spin.value() == pytest.approx(run.result.subs['SW2'].gain_db, abs=0.006)
        assert page.stale() and '설정이 바뀌었습니다' in page.status.text()
        page.sub_gain_spin.setValue(-6.5)
        page.sub_pol_combo.setCurrentIndex(1)
        page.sub_ap_spins[0][0].setValue(48.0)
        page.sub_ap_spins[0][1].setValue(1.2)
        e = page.settings.sub('SW2')
        assert (e['gain_db'], e['polarity'], e['allpass']) == (-6.5, -1, [[48.0, 1.2]])
        assert e['delay_ms'] == run.result.subs['SW2'].delay_ms         # 안 건드린 칸은 가져온 값 그대로(반올림 없음)
        page.sub_main_spin.setValue(0.5)
        assert page.settings.sub_main_delay_ms == 0.5
        page.sub_mimo_check.click()
        assert page.settings.excluded_supports() == ('SW2',)
        assert page.sub_summary.text() == 'SW1 자동 · SW2 고정 (MIMO 제외)'
        # 고정 대역 필터·PEQ 칸은 방식과 상관없이 켜져 있고 저장된다. 처음엔 HP·LP 종류 = 끔(주파수 칸 꺼짐), 주파수 칸은 비면 '— Hz'
        page.set_sub_role('SW1')
        lp = page.sub_band_combos['lp']
        assert _combo_texts(lp)[:2] == ['끔', 'LR 24'] and lp.currentData() == 'off' and not page.sub_lp_spin.isEnabled()
        assert page.sub_lp_spin.specialValueText() == '— Hz' and page.sub_peq_spins[0][0].specialValueText() == '— Hz'
        assert page.sub_ap_spins[0][0].specialValueText() == '— Hz'
        lp.setCurrentIndex(lp.findData('LR24'))                        # 켜면 기본 80 Hz · 주파수 칸 켜짐
        assert page.settings.sub('SW1')['lp_hz'] == 80.0 and page.sub_lp_spin.isEnabled() and page.sub_lp_spin.value() == 80.0
        page.sub_lp_spin.setValue(35.0)
        page.sub_peq_spins[0][0].setValue(28.0)
        page.sub_peq_spins[0][2].setValue(-4.0)
        e = page.settings.sub('SW1')
        assert (e['mode'], e['lp_hz'], e['lp_type'], e['peq']) == ('auto', 35.0, 'LR24', [[28.0, 1.0, -4.0]])
        assert page.project.extra['calibration']['subs_manual']['SW1']['lp_hz'] == 35.0
        assert page.sub_summary.text() == 'SW1 자동 [LP 35 PEQ 1] · SW2 고정 (MIMO 제외)'
        lp.setCurrentIndex(lp.findData('BW24'))
        assert page.settings.sub('SW1')['lp_type'] == 'BW24' and page.settings.sub('SW1')['lp_hz'] == 35.0
        assert page.sub_summary.text() == 'SW1 자동 [LP 35 BW 24 PEQ 1] · SW2 고정 (MIMO 제외)'
        # 서브를 바꿨다 돌아와도 카드 = 저장 값(10-05 리드 지적 — 캡처의 HP·LP 칸과 요약 줄)
        _card_shows(page, 'SW1')
        page.set_sub_role('SW2')
        _card_shows(page, 'SW2')
        page.set_sub_role('SW1')
        _card_shows(page, 'SW1')
        lp.setCurrentIndex(lp.findData('off'))                         # 끔 → 주파수 0, 칸 꺼짐
        page.sub_peq_spins[0][0].setValue(0.0)                         # 주파수 비움 = 밴드 없음
        assert page.settings.sub('SW1')['peq'] == [] and page.settings.sub('SW1')['lp_hz'] == 0.0
        assert not page.sub_lp_spin.isEnabled()
        _card_shows(page, 'SW1')
        # 고른 서브를 바꾸면 그 서브 값이 칸에 온다
        page.set_sub_role('SW1')
        assert page.sub_mode_combo.currentData() == 'auto' and not page.sub_gain_spin.isEnabled()
        assert page.sub_mimo_check.isChecked()
        # 다시 계산 → 표의 방식 열 · 고정 값
        run = _compute(page, qapp)
        assert run.result.subs['SW2'].gain_db == -6.5 and run.result.subs['SW2'].mode == 'fixed'
        page.tab_buttons['subs'].click()
        rows = _cells(page)
        assert rows[0][:2] == ['서브', '방식']
        assert rows[1][1] == '자동' and rows[2][1:4] == ['고정', '-6.5 dB', f'{run.result.subs["SW2"].delay_ms:.2f} ms']
        assert rows[2][4] == '반전' and rows[2][5] == '48 Hz Q 1.2'
        assert len(rows) == 3 and not page.sub_filters_label.isVisibleTo(page)     # 필터가 없으면 '고정 필터' 줄 없음
        # 대역 필터를 넣고 다시 계산하면 표 아래 '고정 필터' 줄에 그 값
        page.set_sub_role('SW2')
        page.sub_band_combos['lp'].setCurrentIndex(page.sub_band_combos['lp'].findData('LR24'))
        page.sub_lp_spin.setValue(25.0)
        run = _compute(page, qapp)
        page.set_tab('subs')
        assert page.sub_filters_label.isVisibleTo(page) and page.sub_filters_label.text() == '고정 필터: SW2 LP 25 Hz LR 24'
        page.set_tab('spl')
        assert not page.sub_filters_label.isVisibleTo(page)
    finally:
        page.deleteLater()


def test_sub_card_is_hidden_with_one_sub(qapp, two_subs, tmp_path):  # noqa: F811
    project, irs = two_subs
    one = copy.deepcopy(project)
    one.config.set_sub_count(1)
    page = _page(one, irs, tmp_path)
    try:
        assert not page.sub_card.isVisibleTo(page)
    finally:
        page.deleteLater()


def test_sub_card_is_english_without_korean_left(qapp, two_subs, tmp_path):  # noqa: F811
    import re

    from PyQt6 import QtWidgets
    from secs.i18n import language_manager

    project, irs = two_subs
    page = _page(copy.deepcopy(project), irs, tmp_path)
    try:
        page.set_sub_mode('SW1', 'fixed')
        page.set_sub_mode('SW2', 'bundle:B')
        page.set_sub_role('SW2')
        page.sub_mimo_check.click()
        language_manager().set_language('en', persist=False)
        qapp.processEvents()
        page.refresh()
        hangul = re.compile('[가-힣]')
        texts = [w.text() for w in page.sub_card.findChildren(QtWidgets.QWidget)
                 if isinstance(w, (QtWidgets.QLabel, QtWidgets.QAbstractButton)) and w.isVisibleTo(page)]
        # 번역 콤보의 itemText 는 원문(한국어)을 돌려준다 — 보이는 글자는 Qt 의 것으로
        texts += [QtWidgets.QComboBox.itemText(page.sub_mode_combo, i) for i in range(page.sub_mode_combo.count())]
        texts += [page.sub_mode_combo.displayText(), page.sub_pol_combo.displayText(),
                  page.sub_ap_spins[1][0].specialValueText(), page.sub_lp_spin.specialValueText(),
                  page.sub_peq_spins[0][0].specialValueText(), page.sub_band_combos['lp'].displayText()]
        assert page.sub_band_combos['lp'].displayText() == 'Off'
        left = [x for x in texts if hangul.search(x)]
        assert not left, left
        assert page.sub_summary.text() == 'SW1 Fixed · SW2 Group B (no MIMO)'
    finally:
        language_manager().set_language('ko', persist=False)
        page.deleteLater()
