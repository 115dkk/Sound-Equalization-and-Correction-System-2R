"""튜토리얼 잠시 치우기(2026-10-11 사용자: "튜토리얼에서 잠시 튜토리얼 치우고 다른 기능들 막 만질 수 있게", "아이콘으로").

'동작 → 변화':
- 안내 카드의 아이콘(글자 없음 · 풍선 도움말 · 접근성 이름)을 누르면 막 · 카드 · 목록 · 알림이 사라지고, 막이 받던 자리(대상
  밖)의 위젯이 그대로 눌린다. 그동안 폴링 · 신호에 반응하지 않는다.
- [튜토리얼 이어 하기](진행바 줄 오른쪽 끝)를 누르면 멈춘 퀘스트의 화면으로 돌아가 같은 퀘스트를 보인다 — 자유 모드에서
  다른 단계로 갔다 와도. 자유 모드에서 그 퀘스트를 이미 해냈으면(값 비교 퀘스트 포함) 다음으로 넘어간다.
- 자유 모드에서 앞 단계를 막히게 바꿨으면(측정을 지움) 그 단계 퀘스트로 돌아간다. 대상이 숨었으면 멈추지 않고 건너뛴다.
- 잠시 치운 채 [튜토리얼 다시] · 끝내기 · 창 닫기에도 예외나 남는 버튼이 없다.
- 아이콘은 카드 머리 줄에 들어가 다른 버튼과 겹치지 않고, 1024×640 · 1920×1080 에서 이어 하기 버튼이 조작 위젯 · 글자를
  덮지 않으며 좁아진 단계 이름도 잘리지 않는다.
"""
from __future__ import annotations

import dataclasses
import re

from test_wizard_common import fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

HANGUL = re.compile(r'[가-힣]')


def _trial(qapp, fake, size=(1280, 800)):  # noqa: F811
    from secs.wizard import new_project

    win = make_window(new_project(True), fake, None)
    win.resize(*size)
    win.show()
    win.set_trial(True)
    qapp.processEvents()
    return win


def _goto(tut, pred):
    """둘러보기는 [계속 →], 눌러 보기는 두 번 눌러 pred 가 참인 퀘스트까지 간다."""
    for _ in range(80):
        q = tut.current()
        if q is None or pred(q):
            return q
        tut.coach.next_button.click()
        if tut.current() is q:
            tut.coach.next_button.click()
    raise AssertionError('퀘스트를 못 찾음')


def _spin(qapp, n=5):  # noqa: F811
    for _ in range(n):
        qapp.processEvents()


def _out_quest(q) -> bool:
    """구성 화면의 출력 번호 퀘스트(그 화면 [다음 →] 퀘스트가 아닌 눌러 보기) — 문구 · 판정이 바뀌어도 고를 수 있게."""
    return q.screen == 2 and q.kind == 'act' and q.blocked is None


def _gone(tut) -> bool:
    return not any(w.isVisible() for w in (tut.overlay, tut.coach, tut.panel, tut.panel.pill, tut.toast))


def _in_window(widget, win):
    from PyQt6 import QtCore

    return QtCore.QRect(widget.mapTo(win, QtCore.QPoint(0, 0)), widget.size())


def test_icon_on_card_pauses_and_frees_the_whole_window(qapp, fake):  # noqa: F811
    from PyQt6 import QtCore, QtTest
    from secs.wizard import tooltips

    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        button = tut.coach.pause_button
        # 아이콘 버튼(글자 없음) · 풍선 도움말 · 접근성 이름
        assert button.isVisible() and not button.icon().isNull() and button.text() == ''
        assert button.accessibleName() == '튜토리얼 잠시 치우기'
        assert tooltips.resolve(button)[1] == 'win.tutorial_pause'
        assert tooltips.tip_text(button).startswith('튜토리얼 잠시 치우기')
        # 막이 받던 자리 — 진행바의 '레벨 체크' 단계 이름(대상 밖)
        step_button = win.steps_bar.buttons[2]
        point = step_button.mapTo(win, step_button.rect().center())
        assert win.childAt(point) is tut.overlay
        button.click()
        _spin(qapp)
        assert tut.paused and tut.active and _gone(tut)
        # 이어 하기 버튼이 진행바 줄 끝에 들어가 단계 이름 자리가 조금 옮겨졌다 — 자리를 다시 잡는다
        point = step_button.mapTo(win, step_button.rect().center())
        assert tut.resume_button.isVisible() and tut.resume_button.text() == '튜토리얼 이어 하기'
        assert not tut.resume_button.icon().isNull()
        assert tooltips.resolve(tut.resume_button)[1] == 'win.tutorial_resume'
        assert not tut._poll.isActive()                              # 폴링 멈춤
        under = win.childAt(point)
        assert under is not None and under is not tut.overlay
        QtTest.QTest.mouseClick(under, QtCore.Qt.MouseButton.LeftButton)
        _spin(qapp)
        assert win.current_step == 3                                 # 대상 밖 위젯이 그대로 눌렸다
        # 창 크기를 바꿔도 막 · 카드가 다시 뜨지 않고 이어 하기 버튼은 창 안에 남는다
        win.resize(1024, 640)
        _spin(qapp, 8)
        tut._place()
        tut._tick()
        assert _gone(tut) and tut.resume_button.isVisible()
        assert win.rect().contains(_in_window(tut.resume_button, win))
    finally:
        win.tutorial.stop()
        win.close()


def test_resume_returns_to_the_same_quest_after_visiting_other_steps(qapp, fake):  # noqa: F811
    from secs.wizard.strings import CAL_STEP

    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        q = _goto(tut, _out_quest)                                   # 출력 번호 퀘스트
        k = tut.index
        tut.coach.pause_button.click()
        win.go(5)                                                    # 자유 모드에서 다른 단계로
        win.go(CAL_STEP)
        _spin(qapp)
        tut.resume_button.click()
        _spin(qapp)
        assert not tut.paused and not tut.resume_button.isVisible()
        assert tut.current() is q and tut.index == k and win.current_step == 2
        assert tut.overlay.isVisible() and tut.coach.isVisible() and tut._poll.isActive()
        for _ in range(3):
            tut._tick()
        assert tut.index == k                                        # 아무것도 안 바꿨으면 그 퀘스트 그대로
    finally:
        win.tutorial.stop()
        win.close()


def test_quest_done_in_free_mode_moves_on_but_signals_are_ignored(qapp, fake):  # noqa: F811
    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        # 신호 퀘스트([다시 확인]) — 치운 동안 누른 것은 세지 않는다(그동안은 신호에 반응하지 않는다)
        retry = _goto(tut, lambda x: x.kind == 'act' and x.signal is not None)
        k = tut.index
        tut.coach.pause_button.click()
        win.devices_page.retry_button.click()
        _spin(qapp)
        tut._tick()
        assert tut.index == k and tut.paused
        tut.resume_button.click()
        _spin(qapp)
        tut._tick()
        assert tut.current() is retry                                # 다시 보여 준다
        win.devices_page.retry_button.click()
        _spin(qapp)
        tut._tick()
        assert tut.index == k + 1                                    # 이어 한 뒤에는 신호로 넘어간다
        # 값으로 판정하는 퀘스트(출력 번호) — 치운 동안 해 두면 이어 하자마자 넘어간다(base 가 있는 퀘스트는 멈추기 전 값과
        # 비교). 자유 모드에서 할 일은 그 퀘스트의 '예시로 채우기'(auto)와 같은 변경이다.
        q = _goto(tut, _out_quest)
        k = tut.index
        tut.coach.pause_button.click()
        q.auto(win)
        win.config_page.refresh()
        tut._tick()
        assert tut.index == k                                        # 치운 동안은 판정하지 않는다
        tut.resume_button.click()
        _spin(qapp)
        tut._tick()
        assert tut.index == k + 1 and tut.toast.isVisible()
    finally:
        win.tutorial.stop()
        win.close()


def test_resume_goes_back_when_free_mode_removed_the_measurement(qapp, fake):  # noqa: F811
    from secs.wizard.strings import CAL_STEP

    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        _goto(tut, lambda x: x.screen == CAL_STEP and x.kind == 'act')       # [계산]
        _spin(qapp)
        assert win.project.measurements
        tut.coach.pause_button.click()
        win.go(5)
        win.measure_page.redo_last()                                 # [마지막 위치 다시] — 측정을 지움
        _spin(qapp)
        assert not win.project.measurements
        win.go(CAL_STEP)
        tut.resume_button.click()
        _spin(qapp)
        q = tut.current()
        assert q.screen == 5 and q.kind == 'act' and win.current_step == 5
        assert tut.index not in tut.completed                        # 다시 해야 하는 퀘스트로(한 번에 넘기지 않는다)
        assert '측정' in tut.toast.text() and tut.toast.isVisible()
        k = tut.index
        win.measure_page.sample_button.click()
        _spin(qapp)
        tut._tick()
        assert tut.index == k + 1
    finally:
        win.calibration_page.cancel()
        win.tutorial.stop()
        win.close()


def test_hidden_target_after_resume_is_skipped_not_stuck(qapp, fake):  # noqa: F811
    from PyQt6 import QtWidgets

    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        _goto(tut, lambda x: x.kind == 'act')
        k = tut.index
        tut.coach.pause_button.click()
        ghost = QtWidgets.QWidget(win)                               # 자유 모드에서 바꾼 상태로 대상이 숨었다
        ghost.hide()
        tut.quests[k] = dataclasses.replace(tut.quests[k], target=lambda w: ghost, signal=None)
        tut.resume_button.click()
        for _ in range(6):
            tut._tick()
            _spin(qapp, 2)
        assert tut.index > k and tut.active and not tut.paused
    finally:
        win.tutorial.stop()
        win.close()


def test_restart_stop_and_close_while_paused_leave_nothing_behind(qapp, fake):  # noqa: F811
    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        _goto(tut, lambda x: x.screen == 2)
        tut.coach.pause_button.click()
        win.tutorial_restart_button.click()                          # 사이드바 [튜토리얼 다시]
        _spin(qapp)
        assert not tut.paused and not tut.resume_button.isVisible()
        assert tut.index == 0 and win.current_step == 1 and tut.overlay.isVisible() and tut.coach.isVisible()
        tut.coach.pause_button.click()
        tut.stop()                                                   # 튜토리얼 끝(끝내기 · 마지막 카드)
        assert not tut.paused and not tut.resume_button.isVisible() and _gone(tut)
        tut.resume()                                                 # 끝난 뒤 이어 하기는 아무것도 하지 않는다
        tut.pause()
        assert _gone(tut) and not tut.resume_button.isVisible()
        tut.start()
        tut.coach.pause_button.click()
        assert tut.resume_button.isVisible()
    finally:
        win.close()                                                  # 치운 채 창 닫기
        _spin(qapp, 10)
        assert not win.tutorial._poll.isActive()


def test_icon_fits_the_card_and_resume_button_covers_nothing(qapp, fake):  # noqa: F811
    from secs.wizard.strings import FLOW

    win = _trial(qapp, fake, (1024, 640))
    try:
        tut = win.tutorial
        coach = tut.coach
        picks = [0, next(i for i, q in enumerate(tut.quests) if q.kind == 'act'), len(tut.quests) - 1]
        for i in picks:
            tut.index = i
            tut._show()
            _spin(qapp)
            tut._place()
            icon = coach.pause_button.geometry()
            assert coach.rect().contains(icon)
            for other in (coach.prev_button, coach.next_button, coach.why_button, coach.text):
                if other.isVisible():
                    assert not icon.intersects(other.geometry()), other
            head = coach.head
            assert head.width() >= head.fontMetrics().horizontalAdvance(head.text()), head.text()
            assert win.rect().contains(coach.geometry())
        for size in ((1024, 640), (1920, 1080)):
            win.resize(*size)
            _spin(qapp, 5)
            tut.index = 0
            tut._show()
            tut.coach.pause_button.click()
            for step in FLOW:
                win.go(step)
                _spin(qapp, 3)
                tut._place_resume()
                button = tut.resume_button
                box = _in_window(button, win)
                assert win.rect().contains(box)
                # 진행바 줄 오른쪽 끝 — 페이지 · 아래 내비를 덮지 않는다
                assert _in_window(win.steps_bar, win).contains(box)
                assert box.left() >= max(_in_window(b, win).right() for b in win.steps_bar.buttons)
                text_width = button.fontMetrics().horizontalAdvance(button.text())
                assert button.width() >= text_width + button.iconSize().width()
                # 좁아진 단계 이름도 잘리지 않는다
                for b in win.steps_bar.buttons:
                    assert b.width() >= b.fontMetrics().horizontalAdvance(b.text()), (size, b.text(), b.width())
                covered = [r for r, weight in tut._controls([]) if weight >= 0.9 and box.intersects(r)]
                assert not covered, (size, step, covered)
            tut.resume_button.click()
            _spin(qapp)
    finally:
        win.tutorial.stop()
        win.close()


def test_new_words_have_english(qapp):  # noqa: F811
    from secs.i18n import translate
    from secs.wizard import strings  # noqa: F401 - 번역표 등록
    from secs.wizard.tooltips import TIPS

    for text in ('튜토리얼 잠시 치우기', '튜토리얼 이어 하기', '{0} 단계에서 바뀐 것이 있어 거기서부터 이어 갑니다',
                 TIPS['win.tutorial_pause'], TIPS['win.tutorial_resume']):
        english = translate(text, 'en')
        assert english != text and not HANGUL.search(english), text
