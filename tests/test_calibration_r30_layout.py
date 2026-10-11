"""요구 대장 R30(2026-10-09): 서브 묶음 레이아웃 — DBA · 위치기반 멀티서브, 2단 합성.

사용자: "서브우퍼 레이아웃에 DBA와 위치기반 멀티서브우퍼 2개의 레이아웃 선택지를 만들어주고 여기에 구현해줘",
"같이 묶이되 DBA끼리 먼저 필터를 줘서 합친 다음에 LFE로 합쳐지길". 결정: 묶음 안 필터 완전히 동일 + 레벨만 서브별 자동,
DBA 뒤 서브의 반전·딜레이 자동, 묶음은 A·B 둘. '동작 → 변화' 로 본다:
- 옛 .secs(레이아웃 칸 없음)의 묶음은 R27 상대값 고정 그대로, 새 칸은 저장·불러오기를 거쳐 남는다.
- 레이아웃 묶음은 고정 출력 필터가 묶음 전부에 같다(화면에서 한 서브를 고치면 묶음 전부).
- 멀티: 필터(HP·LP·PEQ·올패스)가 서브마다 정확히 같고, 지연·극성도 같다 — 게인만 다르다.
- DBA: 필터 같음, 뒤 서브 극성 = 앞의 반대, 지연 차 = 전파 지연(≥ 도착 차) + 사람 보정.
- 돌비룸 사본(`SECS_DOLBY` 환경 변수에 .secs 경로를 주면 — 무겁다): 둘 다 계산이 끝나고 같은 성질.
"""
from __future__ import annotations

import copy
import json
import os

import numpy as np
import pytest

from test_calibration_step import _page, fast_engine  # noqa: F401
from test_calibration_sub_combine import fast_ba, two_subs  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def _layout_settings(kind: str, trim: float = 0.0, lp: float = 0.0):
    from secs import calibration as cal

    s = cal.CalibrationSettings(mode='zero')
    s.set_sub('SW1', mode='bundle', bundle='A')
    s.set_sub('SW2', mode='bundle', bundle='A', dba_row='rear')
    s.set_layout('A', kind=kind, rear_trim_ms=trim)
    if lp:
        s.set_sub('SW1', lp_hz=lp, lp_type='LR24')
    return s


def _filters(a):
    return (a.hp_hz, a.hp_type, a.lp_hz, a.lp_type, [tuple(b) for b in a.peq], [tuple(b) for b in a.allpass])


def test_old_bundle_stays_relative_values_and_layout_survives_save(tmp_path, two_subs):  # noqa: F811
    from secs import calibration as cal
    from secs import model

    project = copy.deepcopy(two_subs[0])                       # 모듈 공용 픽스처를 더럽히지 않게
    project.extra['calibration'] = {'mode': 'mixed', 'subs_manual': {'SW1': {'mode': 'bundle', 'bundle': 'A'}}}
    s = cal.CalibrationSettings.from_project(project)
    assert s.layout('A') == 'values' and s.sub('SW1')['dba_row'] == 'front'
    s = _layout_settings('dba', trim=1.25, lp=60.0)
    assert s.sub('SW2')['lp_hz'] == 60.0                       # 필터는 묶음 전부에
    s.store(project)
    path = tmp_path / 'p.secs'
    project.save(path)
    assert 'bundle_layouts' in json.dumps(json.loads(path.read_text(encoding='utf-8')))
    back = cal.CalibrationSettings.from_project(model.Project.load(path))
    assert back.bundle_layouts == {'A': {'kind': 'dba', 'rear_trim_ms': 1.25}}
    assert back.sub('SW2')['dba_row'] == 'rear' and back.engine_key() == s.engine_key()
    assert cal.bundle_layout_entry({'kind': 'magic', 'rear_trim_ms': 99})['kind'] == 'values'


def test_multi_bundle_shares_filters_and_differs_only_in_gain(fast_engine, fast_ba, two_subs):  # noqa: F811
    from secs import calibration as cal

    project, irs = two_subs
    run = cal.compute(project, irs, _layout_settings('multi', lp=70.0))
    a, b = run.result.subs['SW1'], run.result.subs['SW2']
    assert a.mode == b.mode == 'bundle:A:multi'
    assert _filters(a) == _filters(b) and a.lp_hz == 70.0
    assert len(a.peq) >= 1                                     # 묶음 합에 한 번 설계한 PEQ
    assert a.delay_ms == pytest.approx(b.delay_ms) and a.polarity == b.polarity
    info = run.engine.layout_info['A']
    assert b.gain_db - a.gain_db == pytest.approx(info['gains']['SW2'] - info['gains']['SW1'])
    assert max(info['gains'].values()) == 0.0                  # 가장 작은 서브에 맞춰 깎기만
    assert 'Group A 멀티서브' in cal.sub_values_text(run) and cal.sub_mode_text(a.mode) == 'Group A 멀티'


def test_dba_rear_is_inverted_and_delayed_by_travel_time(fast_engine, fast_ba, two_subs):  # noqa: F811
    from secs import calibration as cal

    project, irs = two_subs
    run = cal.compute(project, irs, _layout_settings('dba'))
    a, b = run.result.subs['SW1'], run.result.subs['SW2']
    assert (a.mode, b.mode) == ('bundle:A:dba-front', 'bundle:A:dba-rear')
    assert _filters(a) == _filters(b)
    assert a.polarity * b.polarity == -1
    info = run.engine.layout_info['A']
    assert b.delay_ms - a.delay_ms == pytest.approx(info['rear_ms'])
    assert info['rear_ms'] >= info['min_ms'] - 0.25            # 전파 지연 ≥ 도착 차(앞→뒤 거리 ≥ 거리 차)
    # 사람 보정은 자동 값에 더해진다
    run2 = cal.compute(project, irs, _layout_settings('dba', trim=1.5))
    i2 = run2.engine.layout_info['A']
    assert i2['auto_ms'] == pytest.approx(info['auto_ms']) and i2['rear_ms'] == pytest.approx(info['auto_ms'] + 1.5)
    assert run2.result.subs['SW2'].delay_ms - run2.result.subs['SW1'].delay_ms == pytest.approx(i2['rear_ms'])
    assert 'DBA' in cal.layout_lines(run)[0]


def test_sub_card_layout_rows(qapp, two_subs, tmp_path):  # noqa: F811
    project, irs = two_subs
    page = _page(project, irs, tmp_path)
    page.refresh()
    page.set_sub_role('SW1')
    assert page.sub_layout_combo.isHidden()
    page.set_sub_mode('SW1', 'bundle:A')
    page.refresh()
    assert not page.sub_layout_combo.isHidden() and page.sub_row_combo.isHidden()
    page.sub_layout_combo.setCurrentIndex(page.sub_layout_combo.findData('dba'))
    assert page.settings.layout('A') == 'dba'
    assert not page.sub_row_combo.isHidden() and not page.sub_gain_spin.isEnabled()
    page.sub_row_combo.setCurrentIndex(1)
    assert page.settings.sub('SW1')['dba_row'] == 'rear'


@pytest.mark.skipif(not os.environ.get('SECS_DOLBY'), reason='돌비룸 사본 경로(SECS_DOLBY) 없음 — 무거운 실측 계산')
@pytest.mark.parametrize('kind', ['multi', 'dba'])
def test_dolby_copy(kind):
    from secs import calibration as cal
    from secs.rewapi.session import load_project_irs
    from secs.model import Project

    path = os.environ['SECS_DOLBY']
    project = Project.load(path)
    irs = load_project_irs(project, path)
    s = cal.CalibrationSettings(mode='zero')
    for r in ('SW1', 'SW2', 'SW3'):
        s.set_sub(r, mode='bundle', bundle='A')
    s.set_sub('SW3', dba_row='rear')
    s.set_layout('A', kind=kind)
    run = cal.compute(project, irs, s)
    subs = run.result.subs
    assert len({repr(_filters(a)) for a in subs.values()}) == 1
    if kind == 'dba':
        info = run.engine.layout_info['A']
        assert subs['SW3'].polarity == -subs['SW1'].polarity == -subs['SW2'].polarity
        assert subs['SW3'].delay_ms - subs['SW1'].delay_ms == pytest.approx(info['rear_ms'])
        assert info['rear_ms'] >= info['min_ms'] - 0.25
    else:
        assert len({round(a.delay_ms, 9) for a in subs.values()}) == 1
    view = cal.sub_combine(run)
    assert np.isfinite(view['flat'][1])


@pytest.mark.parametrize('lang', ['ko', 'en'])
def test_layout_card_fits_1024x640(qapp, fast_engine, fast_ba, two_subs, tmp_path, lang):  # noqa: F811
    """1024×640 에서 DBA 묶음 카드·서브 정렬 탭이 잘리지 않는다(실제 글꼴 — `QT_QPA_PLATFORM=windows`).
    `SECS_CAPTURE_DIR` 를 주면 창·카드 캡처를 남긴다(눈 확인용)."""
    if qapp.platformName() == 'offscreen':
        pytest.skip('실제 글꼴 폭 검사: QT_QPA_PLATFORM=windows 로 실행')
    import copy

    import secs.rewapi as rewapi
    from PyQt6.QtCore import Qt
    from secs.i18n import language_manager
    from secs.wizard import WizardWindow
    from secs.wizard.strings import CAL_STEP
    from test_calibration_r5_1024 import SIZE, _capture_tool
    from test_calibration_step import _compute

    tool = _capture_tool()
    project, irs = two_subs
    rew = rewapi.FakeREW(delay_s=0.0)
    win = WizardWindow(copy.deepcopy(project), rew, rew, rew, rew, project_path=tmp_path / 'r30.secs')
    win.ask_before_close = False
    win.auto_calibrate = False
    win.setAttribute(Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    win.show()
    page = win.calibration_page
    out = os.environ.get('SECS_CAPTURE_DIR')
    try:
        with pytest.MonkeyPatch.context() as mp:
            page.set_context(tmp_path / 'r30.secs', lambda: irs)
            mp.setattr(win, '_set_calibration_context', lambda: None)
            page.set_sub_mode('SW1', 'bundle:A')
            page.set_sub_mode('SW2', 'bundle:A')
            page.set_sub_role('SW2')
            page.sub_layout_combo.setCurrentIndex(page.sub_layout_combo.findData('dba'))
            page.sub_row_combo.setCurrentIndex(1)
            page.mode_buttons['zero'].click()
            _compute(page, qapp)
            language_manager().set_language(lang, persist=False)
            qapp.processEvents()
            bad = []
            for tab in ('subs', 'spl'):
                page.set_tab(tab)
                row = tool.capture(win, qapp, CAL_STEP, SIZE, tmp_path, f'r30_{lang}_{tab}')
                if (win.width(), win.height()) != SIZE or row['hscroll'] or row['clipped']:
                    bad.append((tab, row['hscroll'], row['clipped']))
                if out:
                    win.grab().save(os.path.join(out, f'r30_{lang}_{tab}_1024.png'))
            if out:
                page.sub_card.grab().save(os.path.join(out, f'r30_{lang}_card.png'))
            assert not page.sub_row_combo.isHidden() and not page.sub_trim_spin.isHidden()
            assert not bad, bad
    finally:
        language_manager().set_language('ko', persist=False)
        win.close()
        qapp.processEvents()
        rew.close()
