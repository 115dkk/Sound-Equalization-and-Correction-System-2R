"""보정 설계 화면의 타겟 커브 카드·편집 창(2026-10-09 — 개정 9 에서 옛 6단계와 함께 빠졌던 것을 되살림).

'동작 → 변화' 로 본다: 프리셋을 누르면 `project.target` 이 바뀌고 결과가 낡았다고 보이지만 계산은 돌지 않는다(10-09 사용자
결정: [계산] 을 눌러야만 계산). 편집 창에서 제어점을 고치면 같은 일이 생기고 프리셋 표시가 꺼진다. 저장 → 불러오기는 같은 점을
되돌린다. 영어 화면에 한글이 남지 않는다.
"""
from __future__ import annotations

import re

from test_calibration_step import _compute, _page, _project, fast_engine, synth  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def _pick(page, key):
    """SPL 그래프 빈 곳 우클릭 메뉴의 프리셋을 누른다(10-09 — 타겟 커브 카드 대신)."""
    action = next(a for a in page.target_menu().actions() if a.data() == key)
    action.trigger()


def _checked(page):
    return [a.data() for a in page.target_menu().actions() if a.isCheckable() and a.isChecked()]


def test_target_preset_and_editor_mark_result_stale_without_computing(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    from secs import calibration as cal
    from secs.model import TargetCurve

    case, irs = synth
    project = _project(case, 3)
    page = _page(project, irs, tmp_path)
    try:
        _pick(page, 'harman')
        assert project.target.points == TargetCurve.presets()['harman'].points
        assert project.bc['target_preset'] == 'harman'
        assert _checked(page) == ['harman']
        assert len(project.target.points) >= 2
        assert page.compute_count == 0 and not page.running()          # 누르기만으로는 계산하지 않는다

        run = _compute(page, qapp)
        assert run.target_key == cal.target_key(project) and not page.stale()
        assert page.status.text() == '계산 완료'

        _pick(page, 'cinema')
        assert page.stale() and '설정이 바뀌었습니다' in page.status.text()
        assert not page.running() and page.compute_count == 1

        _compute(page, qapp)
        assert not page.stale()
        # SPL 그래프 위 손잡이로 고친다(10-09: 별도 편집 창 없앰)
        page.tab = 'spl'
        page.refresh()
        assert len(page._target_handles) == len(project.target.points)
        f1, _db = sorted(project.target.points)[1]
        page.set_target_point(1, f1, 5.5)
        assert sorted(project.target.points)[1][1] == 5.5
        assert page.stale() and not page.running() and page.compute_count == 2
        assert project.bc['target_preset'] == ''
        assert _checked(page) == ['custom']
        assert not hasattr(page, 'target_buttons')          # 카드는 없앴다(10-09)

        # 저장 → 다른 프리셋 → 불러오기 = 고친 점이 돌아온다
        saved = list(project.target.points)
        path = page.save_target(str(tmp_path / 'tgt.json'))
        _pick(page, 'flat')
        assert page.load_target(path) and [tuple(p) for p in project.target.points] == [tuple(p) for p in saved]
        before = len(project.target.points)
        page.remove_target_point(0)
        assert len(project.target.points) == before - 1
        page.add_target_point(1000.0, 1.0)
        assert len(project.target.points) == before
    finally:
        page.deleteLater()


def test_target_card_is_english(qapp, synth, tmp_path):  # noqa: F811
    from PyQt6 import QtWidgets
    from secs.i18n import language_manager

    case, irs = synth
    page = _page(_project(case, 1), irs, tmp_path)
    try:
        language_manager().set_language('en', persist=False)
        qapp.processEvents()
        hangul = re.compile('[가-힣]')
        left = [w.text() for w in page.findChildren(QtWidgets.QWidget)
                if isinstance(w, (QtWidgets.QLabel, QtWidgets.QAbstractButton)) and w.isVisible() and hangul.search(w.text())]
        assert not left, left
    finally:
        language_manager().set_language('ko', persist=False)
        page.deleteLater()
