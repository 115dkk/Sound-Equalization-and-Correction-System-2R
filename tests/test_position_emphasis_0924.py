"""다점 측정에서 위치 1(청취 위치)을 한눈에 — 굵은 선·위치 칩(2026-09-24 사용자 추가 요청).

"다점 측정 시 어느 게 정축(청취 위치 1번) 응답인지 확인하기 어렵다" → 5·7단계 위치별 곡선은 위치 1 = 굵고
진한 채널 색, 나머지 = 가늘고 옅은 같은 색. 위치 칩으로 켜고 끄고, 칩에 커서를 대면 그 위치만 강조.
"""
from __future__ import annotations

import pytest

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def test_position_pen_marks_listening_seat(qapp, engine):
    from secs.wizard.legend import MAIN_WIDTH, OTHER_WIDTH, position_pen

    main, other = position_pen('#a0258a', 1), position_pen('#a0258a', 2)
    assert main.widthF() == pytest.approx(MAIN_WIDTH) and other.widthF() == pytest.approx(OTHER_WIDTH)
    assert main.color().alpha() == 255 and other.color().alpha() < 128
    assert main.color().rgb() == other.color().rgb()                 # 같은 채널 색 계열
    hovered = position_pen('#a0258a', 3, hover=3)
    dimmed = position_pen('#a0258a', 1, hover=3)
    assert hovered.color().alpha() == 255 and hovered.widthF() > MAIN_WIDTH
    assert dimmed.color().alpha() < other.color().alpha()           # 강조 중 나머지는 더 흐림
    assert position_pen((127, 138, 31), 1).color().alpha() == 255   # 튜플 색도 받는다


def test_position_chips_toggle_and_hover(qapp, engine):
    from PyQt6 import QtCore, QtGui
    from secs.wizard.legend import PositionChips

    chips = PositionChips()
    events = []
    chips.toggled.connect(lambda p, on: events.append((p, on)))
    chips.hovered.connect(lambda p: events.append(('h', p)))
    try:
        chips.set_count(1)
        assert chips.isHidden()                                      # 1점이면 줄이 없다
        chips.set_count(9)
        assert list(chips.chips) == list(range(1, 10)) and not chips.isHidden()
        assert chips.chips[1].property('main') == 'true'
        chips.chips[4].click()
        assert not chips.visible(4) and events[-1] == (4, False)
        chips.set_count(9)                                           # 같은 수면 끈 상태 유지
        assert not chips.chips[4].isChecked()
        point = QtCore.QPointF(1, 1)
        QtCore.QCoreApplication.sendEvent(chips.chips[3], QtGui.QEnterEvent(point, point, point))
        assert chips.hover == 3 and events[-1] == ('h', 3)
        QtCore.QCoreApplication.sendEvent(chips.chips[3], QtCore.QEvent(QtCore.QEvent.Type.Leave))
        assert chips.hover == 0
    finally:
        chips.close()


def test_step5_draws_listening_seat_bold_and_hides_chipped_positions(qapp, fake, tmp_path, engine):
    import secs.model as model

    project = model.Project(name='다점', config=model.SpeakerConfig.from_preset('2.1'))
    win = make_window(project, fake, tmp_path / 'multi.secs')
    try:
        win.step5.load_sample(3)
        win.go(5)
        page = win.step5
        page.refresh()
        assert list(page.position_chips.chips) == [1, 2, 3]
        items = page.plot.getPlotItem().listDataItems()
        # 서브 곡선은 늘 더 굵다(10-10 3차 시험: 서브가 메인과 같은 색 · 굵기라 안 보였다) — 메인 색 곡선만 본다
        from secs.wizard.style import SUB_LINE_COLORS
        mains = [i for i in items if i.opts['pen'].color().name() not in SUB_LINE_COLORS]
        widths = sorted({round(i.opts['pen'].widthF(), 1) for i in mains})
        assert widths[-1] == pytest.approx(2.0) and widths[0] < 2.0     # 위치 1 만 굵다(5단계 2.0 px)
        count = len(items)
        page.position_chips.chips[2].click()
        assert len(page.plot.getPlotItem().listDataItems()) < count
    finally:
        win.close()
        qapp.processEvents()

