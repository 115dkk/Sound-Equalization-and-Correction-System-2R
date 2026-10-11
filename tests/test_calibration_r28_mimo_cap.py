"""R28(2026-10-05): 보정 설계 MIMO 카드의 '입력당 서포트 최대 n' — 테시라 DSP 예산(DSP-2 × 3, 2×8 딜레이 매트릭스를 입력 둘이
나눠 써 입력당 4갈래가 기준, `보고서/01 안내서/2026-10-05 테시라 오브젝트 배치도/`).

'동작 → 변화' 로 본다:
- 기본 0 = 제한 없음: 엔진(`mimo.design_mimo`)에 상한 없이(None) — 전과 같은 길.
- n = 4: 입력마다 서포트(갈래) ≤ 4, 결과 노트·MIMO 줄에 '입력당 서포트 ≤ 4', MIMO 결과가 낡는다(설정이 바뀌면).
- 저장·불러오기(옛 .secs 는 0), 화면 칸 → 설정, 영어에 한글 없음.
합성 방 5.1.2(메인 7 + 서브 2, 위치 3)에서 입력 L 하나만 실제 `design_mimo` 로 푼다(입력당 후보 8 → 상한이 효과를 낸다).
IIR 맞춤(수 분)은 건너뛴다(빈 결과) — 상한은 FIR 서포트 고르기 단계에서 정해진다.
"""
from __future__ import annotations

import copy

import pytest

from test_calibration_step import _compute, _page, fast_engine  # noqa: F401
from test_calibration_sub_combine import fast_ba, two_subs  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


@pytest.fixture(scope='module')
def five(engine):  # noqa: F811
    """합성 방 5.1.2(L·R·C·Ls·Rs·Tfl·Tfr + SW1·SW2), 위치 3곳."""
    import room_model_decay as rmd

    case = rmd.build_case('5.1.2', positions=3)
    project = copy.deepcopy(case.project)
    roles = tuple(case.roles)
    project.measurements = [{'pos': p, 'ir': {r: f'{r}_P{p}.wav' for r in roles}} for p in range(1, 4)]
    return project, case.irs


def _mimo_on_l_only(monkeypatch, seen):
    """`design_mimo` 를 입력 L 하나로 · IIR 맞춤은 빈 결과로(무겁다) — 받은 상한을 적어 둔다."""
    from secs import mimo, mimo_iir

    original = mimo.design_mimo

    def design_mimo(engine, **kw):
        seen.append(kw.get('max_supports'))
        return original(engine, inputs=['L'], **kw)

    monkeypatch.setattr(mimo, 'design_mimo', design_mimo)
    monkeypatch.setattr(mimo_iir, 'design_mimo_iir', lambda engine, design, positions, **kw: {})


def test_old_secs_has_no_cap_and_cap_survives_save(tmp_path, two_subs):  # noqa: F811
    from secs import calibration as cal
    from secs import model

    project, _irs = two_subs
    old = copy.deepcopy(project)
    old.extra['calibration'] = {'mode': 'mixed', 'mimo_on': True}                 # R28 전 .secs 모양
    path = tmp_path / 'old.secs'
    old.save(path)
    s = cal.CalibrationSettings.from_project(model.Project.load(path))
    assert s.mimo_max_supports == 0
    s.mimo_max_supports = 4
    s.store(old)
    old.save(path)
    back = cal.CalibrationSettings.from_project(model.Project.load(path))
    assert back.mimo_max_supports == 4 and back.mimo_key() != cal.CalibrationSettings().mimo_key()
    for raw, want in ((99, 12), (-3, 0), ('x', 0), (3.6, 4)):
        p = copy.deepcopy(project)
        p.extra['calibration'] = {'mimo_max_supports': raw}
        assert cal.CalibrationSettings.from_project(p).mimo_max_supports == want


def test_cap_zero_is_unlimited_and_cap_four_limits_each_input(fast_engine, fast_ba, five, monkeypatch):  # noqa: F811
    from secs import calibration as cal

    project, irs = five
    run = cal.compute(project, irs, cal.CalibrationSettings(mode='zero'))
    seen = []
    _mimo_on_l_only(monkeypatch, seen)
    free = cal.compute_mimo(run)
    capped = cal.compute_mimo(run, max_supports=4)
    assert seen == [None, 4]                                    # 0 = 엔진에 상한 없이(전과 같은 길)
    n_free = len(free.design.supports['L'])
    assert n_free > 4, free.design.supports['L'].keys()            # 상한이 효과를 내는 구성인지
    assert len(capped.design.supports['L']) == 4
    assert set(capped.design.supports['L']) <= set(free.design.supports['L'])
    assert free.max_supports == 0 and capped.max_supports == 4 and free.key() != capped.key()
    assert '입력당 서포트 ≤ 4' in capped.notes and not any('입력당 서포트' in n for n in free.notes)
    assert cal.mimo_cap_text(capped) == ' · 입력당 서포트 ≤ 4' and cal.mimo_cap_text(free) == ''


def test_mimo_card_cap_field(qapp, fast_engine, fast_ba, two_subs, tmp_path, monkeypatch):  # noqa: F811
    """칸 → 설정 저장 → [MIMO 계산]이 상한을 넘기고 → MIMO 줄에 '입력당 서포트 ≤ 2'. 칸을 바꾸면 MIMO 결과가 낡는다."""
    from secs import mimo, mimo_iir

    calls = []

    def design_mimo(engine, fsiso=150.0, positions=None, f_low=None, exclude=(), max_supports=None, **_):
        calls.append(max_supports)
        out = mimo.MimoResult()
        out.ranges = {'SW1': (20.0, 120.0), 'SW2': (20.0, 120.0)}
        out.supports = {'L': {'SW1': [0.0] * 8, 'SW2': [0.0] * 8}}
        return out

    monkeypatch.setattr(mimo, 'design_mimo', design_mimo)
    monkeypatch.setattr(mimo_iir, 'design_mimo_iir',
                        lambda engine, design, positions, **kw: {'L': {'SW1': mimo_iir.IirSupport(-6.0, 3.0, 1, [], [])}})
    project, irs = two_subs
    page = _page(copy.deepcopy(project), irs, tmp_path)
    try:
        spin = page.mimo_max_spin
        assert spin.value() == 0 and spin.specialValueText() == '제한 없음' and not spin.isEnabled()   # MIMO 끔
        page.mode_buttons['zero'].click()
        _compute(page, qapp)
        page.mimo_check.click()
        assert spin.isEnabled()
        page.mimo_button.click()
        assert page.wait(120000) and page.mimo_run is not None
        assert calls == [None] and '입력당 서포트' not in page.mimo_status.text()
        spin.setValue(2)
        assert page.project.extra['calibration']['mimo_max_supports'] == 2 and page.mimo_stale()
        assert '[MIMO 계산]' in page.mimo_status.text()
        page.mimo_button.click()
        assert page.wait(120000)
        assert calls == [None, 2] and page.mimo_run.max_supports == 2 and not page.mimo_stale()
        assert '입력당 서포트 ≤ 2' in page.mimo_status.text()
    finally:
        page.deleteLater()


def test_mimo_card_cap_is_english(qapp, two_subs, tmp_path):  # noqa: F811
    import re

    from PyQt6 import QtWidgets
    from secs.i18n import language_manager

    project, irs = two_subs
    page = _page(copy.deepcopy(project), irs, tmp_path)
    try:
        language_manager().set_language('en', persist=False)
        qapp.processEvents()
        page.refresh()
        assert page.mimo_max_spin.specialValueText() == 'No limit' and page.mimo_max_spin.suffix() == ' supports'   # 10-10 현장 말: 경로 → 서포트
        labels = [w.text() for w in page.findChildren(QtWidgets.QLabel) if w.isVisibleTo(page)]
        assert 'Max per input' in labels
        assert not [x for x in labels if re.search('[가-힣]', x)]
    finally:
        language_manager().set_language('ko', persist=False)
        page.deleteLater()
