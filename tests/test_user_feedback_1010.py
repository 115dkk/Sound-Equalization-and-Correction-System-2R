"""사용자 지적(2026-10-10 저녁, 4.13.0 exe) — 다시 깨지지 않게.

- F1 도움말 검색 칸에서 Enter 를 누르면 도움말 창이 닫혔다.
- 튜토리얼 안내 카드의 [이전으로] · [다음으로] 위에서 카드 끌기용 '이동' 커서가 떴다.
"""
from __future__ import annotations

from test_wizard_common import fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def test_enter_in_help_search_keeps_the_dialog_open(qapp):  # noqa: F811
    from PyQt6 import QtCore, QtTest
    from secs.wizard.help_dialog import HelpDialog

    dlg = HelpDialog()
    try:
        dlg.show()
        qapp.processEvents()
        dlg.search_edit.setFocus()
        dlg.search_edit.setText('측정')          # 한글을 QTest.keyClicks 로 치면 Windows 에서 프로세스가 죽는다(10-10)
        QtTest.QTest.keyClick(dlg.search_edit, QtCore.Qt.Key.Key_Return)
        qapp.processEvents()
        assert dlg.isVisible()
        QtTest.QTest.keyClick(dlg, QtCore.Qt.Key.Key_Escape)        # Esc 는 그대로 닫는다
        qapp.processEvents()
        assert not dlg.isVisible()
    finally:
        dlg.close()


def test_coach_buttons_use_a_pointing_cursor(qapp, fake):  # noqa: F811
    from PyQt6 import QtCore
    from secs.wizard import new_project

    win = make_window(new_project(True), fake, None)
    try:
        win.show()
        win.set_trial(True)
        qapp.processEvents()
        coach = win.tutorial.coach
        for button in (coach.prev_button, coach.next_button, coach.why_button):
            assert button.cursor().shape() == QtCore.Qt.CursorShape.PointingHandCursor
    finally:
        win.tutorial.stop()
        win.close()


def test_right_card_quest_cycles_tabs_and_restores(qapp, fake):  # noqa: F811
    """10-10 사용자: '오른쪽 카드는 탭에 맞춰 바뀝니다' 에서 탭이 저절로 SPL → GD → XO 로 넘어가게, 떠나면 SPL 로.
    3차 시험(10-10): 한 바퀴만 돌고 SPL 에서 멈춘다 · 도는 동안 안내 카드 자리는 그대로 · 강조는 탭 줄과 오른쪽 카드를
    따로 · 글은 실제로 나오는 카드 이름(RC 범위 · XO)."""
    from secs.wizard import new_project

    win = make_window(new_project(True), fake, None)
    try:
        win.show()
        win.set_trial(True)
        qapp.processEvents()                     # 튜토리얼은 다음 이벤트 차례에 시작한다
        tut = win.tutorial
        index = next(i for i, q in enumerate(tut.quests) if q.animate is not None)
        tut.index = index
        tut._show()
        tut._place()
        spot = tut.coach.pos()
        seen = [win.calibration_page.tab]
        for _ in range(5):
            tut._animate()
            tut._place()
            seen.append(win.calibration_page.tab)
            assert tut.coach.pos() == spot                 # 탭이 바뀌어도 카드는 그 자리(3차 시험: y 600 ↔ 428)
        # 데모 방은 서브 3(10-11) — 서브 정렬 탭까지 돌고, [계속] 을 누를 때까지 반복(10-11 사용자), 간격 3.6 s
        assert seen[:6] == ['spl', 'gd', 'xo', 'subs', 'spl', 'gd'] and tut._anim_timer.isActive()
        assert tut._anim_timer.interval() == 3600
        assert len(tut.overlay.rings()) == 2                 # 탭 줄 · 오른쪽 카드 따로
        tut.completed.add(tut.index)
        tut.next()
        assert win.calibration_page.tab == 'spl'
        q = tut.quests[index]
        # 3차 시험: 강조는 RC 범위 카드인데 글은 탭 이야기였다 — 이제 글이 나오는 카드를 이름으로 말한다
        assert 'RC 범위' in q.text and 'XO 카드' in q.text and '서브 정렬 설정 카드' in q.text and '부스트' not in q.text
    finally:
        win.tutorial.stop()
        win.close()


def test_blocked_next_shows_the_reason(qapp, fake):  # noqa: F811
    """10-10 사용자: 출력 번호가 겹쳐 [다음 →] 이 막혔는데 튜토리얼은 넘어가라고 했다 — 이유를 카드에 보인다."""
    from secs.wizard import new_project

    win = make_window(new_project(True), fake, None)
    try:
        win.show()
        win.set_trial(True)
        qapp.processEvents()                     # 튜토리얼은 다음 이벤트 차례에 시작한다
        tut = win.tutorial
        index = next(i for i, q in enumerate(tut.quests) if q.screen == 2 and q.blocked is not None)
        tut.index = index
        tut._show()
        speakers = win.project.config.speakers
        speakers[1].bands[0].out = speakers[0].bands[0].out       # 출력 번호 중복
        win.config_page._touch()
        tut._tick()
        assert tut._blocked_on and '자동 정렬' in tut.coach.text.text()
        tut.coach.next_button.click()
        tut.coach.next_button.click()                              # 두 번이면 고치고 넘어간다
        assert win.current_step != 2 and win.can_advance_step(2)[0]
    finally:
        win.tutorial.stop()
        win.close()


def test_config_table_keeps_its_scroll_when_outputs_change(qapp, fake):  # noqa: F811
    """10-10 사용자: 구성에서 출력 채널을 바꾸거나 중복이 생겼다 없어지면 스크롤이 움직였다 — 표 스크롤도 그대로."""
    from secs.wizard import new_project

    win = make_window(new_project(True), fake, None)
    try:
        win.resize(1280, 800)
        win.show()
        win.go(2)
        qapp.processEvents()
        page = win.config_page
        bar = page.table.verticalScrollBar()
        if bar.maximum() == 0:
            import pytest
            pytest.skip('이 크기에서는 표에 스크롤이 없다')
        bar.setValue(bar.maximum())
        before = bar.value()
        speakers = win.project.config.speakers
        speakers[-1].bands[0].out = speakers[0].bands[0].out      # 중복을 만들고
        page._touch()
        qapp.processEvents()
        assert bar.value() == before
        page._renumber()                                          # 다시 없앤다
        qapp.processEvents()
        assert bar.value() == before
    finally:
        win.close()
