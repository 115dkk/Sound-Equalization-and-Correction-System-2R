"""튜토리얼 AI 시험 1차(2026-10-10, 10명) 남은 지적 — 고친 것이 다시 깨지지 않게.

- [← 이전으로] 로 돌아간 끝낸 퀘스트는 다시 시키지 않고, 판정이 바로 참이어도 저절로 넘어가지 않는다(측정 4/5 먹통).
- 출력 번호 퀘스트는 출력 번호가 실제로 바뀌어야 끝난다(크기만 바꿔도 끝났다).
- 번호는 안내 카드에만(꼬리표 '둘러보기 n / 4' 와 카드 'n / 6' 이 따로 놀았다).
- 약어 · 용어는 처음 나오는 카드에 한 번만 풀이가 붙는다.
- 안내 카드는 진행바 · 아래 내비를 덮지 않는다.
- 영어: 'BM path delays … ea', '1 positions', 사이드바 '샘플' 파일 이름.
"""
from __future__ import annotations

import re

from test_wizard_common import fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

HANGUL = re.compile(r'[가-힣]')


def _trial(qapp, fake):  # noqa: F811
    from secs.wizard import new_project

    win = make_window(new_project(True), fake, None)
    win.resize(1280, 800)
    win.show()
    win.set_trial(True)
    qapp.processEvents()
    return win


def _goto(tut, pred):
    """둘러보기는 [다음으로], 눌러 보기는 두 번 눌러 pred 가 참인 퀘스트까지 간다."""
    for _ in range(80):
        q = tut.current()
        if q is None or pred(q):
            return q
        tut.coach.next_button.click()
        if tut.current() is q:
            tut.coach.next_button.click()
    raise AssertionError('퀘스트를 못 찾음')


def test_prev_to_finished_quest_does_not_bounce_or_reask(qapp, fake):  # noqa: F811
    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        sample = _goto(tut, lambda q: q.screen == 5 and q.kind == 'act' and '데모 데이터' in q.text)
        win.measure_page.sample_button.click()
        for _ in range(5):
            qapp.processEvents()
        tut._tick()
        assert tut.current() is not sample             # 샘플을 채우면 넘어간다
        after = tut.index
        tut.coach.prev_button.click()                  # 끝낸 샘플 퀘스트로 돌아감
        for _ in range(4):
            tut._tick()                                # 판정은 참이지만 저절로 넘어가면 안 된다
        assert tut.current() is sample and tut.index == after - 1
        assert tut.coach.next_button.property('soft') == 'false'
        tut.coach.next_button.click()                  # 한 번에 넘어간다(알림 없이)
        assert tut.index == after
    finally:
        win.tutorial.stop()
        win.close()


def test_output_quest_needs_a_real_output_change(qapp, fake):  # noqa: F811
    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        from secs.wizard import tutorial_quests as tq
        # 10-11: 튜토리얼을 시작하면 SW3 출력이 16 으로 틀리게 들어가고, 퀘스트는 16 → 6 으로 고치게 한다
        assert tq._out_of(win, 'SW3') == 16
        q = _goto(tut, lambda q: q.screen == 2 and q.kind == 'act' and q.done is tq._fixed_out)
        speaker = win.project.config.speakers[0]
        speaker.size = 'small' if speaker.size != 'small' else 'large'   # 크기만 바꿈
        win.config_page._touch()
        tut._tick()
        assert tut.current() is q
        win.project.config.speakers[0].bands[0].out = 15     # 다른 스피커 출력만 바꿈 — 아직
        win.config_page._touch()
        tut._tick()
        assert tut.current() is q
        win.project.config.speakers[0].bands[0].out = 1
        next(s for s in win.project.config.speakers if s.role == 'SW3').bands[0].out = 6   # 지시대로 고침
        win.config_page._touch()
        tut._tick()
        assert tut.current() is not q
    finally:
        win.tutorial.stop()
        win.close()


def test_numbers_only_on_the_card_and_glossary_once(qapp, fake):  # noqa: F811
    from secs.wizard import tutorial_quests as tq

    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        assert tut.overlay.label == ''                         # 둘러보기는 꼬리표 없이 점선만(10-10 사용자)
        assert re.search(r'\d+ / \d+', tut.coach.head.text())
        seen: dict[str, int] = {}
        table = tq.glossary_for(tut.quests)
        for index, (body, why) in table.items():
            for g in body + why:
                assert g not in seen, g
                seen[g] = index
        # XO 풀이는 본문에 XO 가 처음 나오는 퀘스트의 본문 아래에 붙는다
        first_xo = next(i for i, q in enumerate(tut.quests) if re.search(r'(?<![A-Za-z])XO(?![A-Za-z])', q.text))
        assert any(g.startswith('XO') for g in table[first_xo][0])
        # [왜?] 에만 나온 말(MIMO — [단일] 카드)은 본문 아래가 아니라 [왜?] 쪽에(2차 시험: 본문과 무관한 풀이)
        single = next(i for i, q in enumerate(tut.quests) if '[단일]' in q.text)
        assert not any(g.startswith('MIMO') for g in table.get(single, ([], []))[0])
        assert any(g.startswith('MIMO') for g in table[single][1])
        q = _goto(tut, lambda q: 'DSP 프로파일' in q.text)
        assert tut.coach.gloss.isVisibleTo(tut.coach) and 'FIR' in tut.coach.gloss.text()
    finally:
        win.tutorial.stop()
        win.close()


def test_coach_stays_between_step_bar_and_nav(qapp, fake):  # noqa: F811
    from PyQt6 import QtCore
    from secs.wizard.tutorial_overlay import NAV_H

    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        bar = win.steps_bar
        bar_bottom = bar.mapTo(win, QtCore.QPoint(0, bar.height())).y()
        for size in ((1280, 800), (1024, 640), (1920, 1080)):
            win.resize(*size)
            qapp.processEvents()
            tut.index = 0
            tut._show()
            for _ in range(len(tut.quests)):
                q = tut.current()
                if q is None or q.screen == 8:
                    break
                tut._place()
                g = tut.coach.geometry()
                bar_bottom = bar.mapTo(win, QtCore.QPoint(0, bar.height())).y()
                assert g.top() >= bar_bottom, (size, q.text)
                assert g.bottom() <= win.height() - NAV_H + 1, (size, q.text)
                tut.completed.add(tut.index)          # 결과를 채우지 않고 화면만 넘겨 본다
                tut.next()
    finally:
        win.tutorial.stop()
        win.close()


def test_english_fixes():
    from secs.i18n import translate

    assert translate(' 곳', 'en') == ' slots'
    assert translate('1 / 1 위치', 'en') == '1 / 1 position'
    assert translate('0 / 9 위치', 'en') == '0 / 9 positions'
    assert translate('위치 1 / 1 · 모두 완료', 'en') == 'Position 1 / 1 · all done'
    from secs.wizard import tutorial_quests as tq
    for text in tq.all_strings():
        assert not HANGUL.search(translate(text, 'en')), text


def test_round2_card_rules_busy_and_temp_files(qapp, fake):  # noqa: F811
    """2차 시험(10-10, 3명) 지적: 규칙 줄은 카드 안 · 건너뛰면 무엇을 했나 알림 · 재생 중엔 끝나지 않음 ·
    카드가 눌러 볼 대상을 덮지 않음 · 튜토리얼 파일은 임시 폴더 · 마지막 버튼은 [마무리]."""
    import tempfile
    from pathlib import Path

    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        act = _goto(tut, lambda q: q.kind == 'act')
        assert tut.coach.hint.isVisibleTo(tut.coach)                 # 규칙 줄
        tut.coach.next_button.click()                                # 첫 번: 카드 안 규칙 줄이 분홍으로
        assert tut.current() is act and 'font-weight: 700' in tut.coach.hint.styleSheet()
        # 출력 번호 퀘스트를 건너뛰면 무엇을 했는지 알림
        from secs.wizard import tutorial_quests as tq
        out_q = _goto(tut, lambda q: q.done is tq._fixed_out)
        tut.coach.next_button.click()
        tut.coach.next_button.click()
        assert tut.current() is not out_q and tut.toast.isVisible() and '출력' in tut.toast.text()
        # 레벨 순차 재생: 누른 뒤 재생이 끝나기 전에는 끝나지 않는다
        level = _goto(tut, lambda q: q.busy is not None and q.screen == 3)
        win.level_page.sequence_button.click()
        qapp.processEvents()
        tut._tick()
        assert tut.current() is level and tut._busy_on
        # 측정: 샘플 카드가 [샘플] 버튼을 덮지 않는다 · 파일은 임시 폴더
        sample = _goto(tut, lambda q: q.screen == 5 and q.kind == 'act')
        tut._place()
        button = win.measure_page.sample_button
        from PyQt6 import QtCore
        rect = QtCore.QRect(button.mapTo(win, QtCore.QPoint(0, 0)), button.size())
        assert not tut.coach.geometry().intersects(rect)
        button.click()
        for _ in range(30):
            qapp.processEvents()
        path = win.current_project_path()
        assert path is not None and Path(tempfile.gettempdir()) in path.parents
        assert win.side_file.text() in ('튜토리얼 · 저장 안 됨', 'Tutorial · not saved')
        # 마지막 카드 버튼
        tut.index = len(tut.quests) - 1
        tut._show()
        # 10-10 3차 시험 사용자 결정 3A: 마지막 카드 버튼 '튜토리얼 마치기 ✓' → '튜토리얼 끝내기'
        assert tut.coach.next_button.text() in ('튜토리얼 끝내기', 'End tutorial')
    finally:
        win.level_page.stop_all() if hasattr(win.level_page, 'stop_all') else None
        win.tutorial.stop()
        win.close()


def test_level_quest_finishes_after_back_and_forth(qapp, fake):  # noqa: F811
    """2차 시험 t04: 순차 재생 중 [이전으로] → [다음으로] 로 돌아오면 재생이 끝나도 퀘스트가 안 끝났다 — 이제 값(모든 채널
    레벨)으로 판정한다."""
    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        level = _goto(tut, lambda q: q.busy is not None and q.screen == 3)
        win.level_page.sequence_button.click()
        tut.coach.prev_button.click()
        tut.coach.next_button.click()
        assert tut.current() is level
        win.level_page.apply_defaults()          # 재생이 끝난 것과 같은 상태(모든 채널 레벨)
        win.level_page._queue = []
        win.level_page._playing = None
        tut._tick()
        assert tut.current() is not level
    finally:
        win.tutorial.stop()
        win.close()


def test_no_dev_notes_in_user_text():
    """퀘스트 · 풀이 문구에 개발 메모('시험' · 날짜)가 섞이지 않는다(10-10: why 문구에 '(2차 시험: …)' 이 들어갔었다)."""
    from secs.wizard import tutorial_quests as tq
    for text in tq.all_strings():
        assert '시험' not in text and '10-10' not in text, text


def test_wheel_scrolls_under_the_overlay(qapp, fake):  # noqa: F811
    """2차 시험 t07: 튜토리얼 막이 휠을 삼켜 측정 화면을 스크롤할 수 없었다 — 막 아래 위젯으로 넘긴다."""
    from PyQt6 import QtCore, QtGui, QtWidgets

    win = _trial(qapp, fake)
    try:
        win.resize(1024, 640)
        tut = win.tutorial
        _goto(tut, lambda q: q.screen == 5)
        for _ in range(5):
            qapp.processEvents()
        area = win.measure_page.plot.parentWidget()
        while area is not None and not isinstance(area, QtWidgets.QScrollArea):
            area = area.parentWidget()
        areas = [area] if area is not None and area.verticalScrollBar().maximum() > 0 else []
        if not areas:
            import pytest
            pytest.skip('이 크기에서 스크롤할 것이 없다')
        area = areas[0]
        bar = area.verticalScrollBar()
        bar.setValue(0)
        view = area.viewport()
        point = view.mapTo(tut.overlay, QtCore.QPoint(6, 6))      # 그래프 위는 휠이 확대라 페이지 바탕에서
        event = QtGui.QWheelEvent(QtCore.QPointF(point), QtCore.QPointF(tut.overlay.mapToGlobal(point)),
                                  QtCore.QPoint(0, 0), QtCore.QPoint(0, -240), QtCore.Qt.MouseButton.NoButton,
                                  QtCore.Qt.KeyboardModifier.NoModifier, QtCore.Qt.ScrollPhase.NoScrollPhase, False)
        tut.overlay.wheelEvent(event)
        qapp.processEvents()
        assert bar.value() > 0
    finally:
        win.tutorial.stop()
        win.close()


def test_look_area_is_view_only_and_screen_next_syncs(qapp, fake):  # noqa: F811
    """2차 시험 t08: 둘러보기 칸 안의 버튼(스피커 삭제 · 순차 재생 · mdat 파일 창)이 눌려 상태가 바뀌었다 — 둘러보기는
    보기만. 끝낸 퀘스트로 돌아갔다가 화면 [다음 →] 으로 넘어가면 퀘스트도 따라간다."""
    from PyQt6 import QtCore

    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        look = _goto(tut, lambda q: q.screen == 2 and q.kind == 'look' and '출력 패치' in q.text)
        tut._place()
        center = tut.overlay.hole.center()
        assert tut.overlay.mask().contains(center)          # 구멍이 뚫려 있지 않다 = 막이 클릭을 받는다
        act = _goto(tut, lambda q: q.screen == 2 and q.kind == 'act')
        tut._place()
        assert not tut.overlay.mask().contains(tut.overlay.hole.center())   # 눌러 보기는 대상이 눌린다
        # 화면 2 첫 카드에서 이전으로 → 장치 마지막([다음 →]) 퀘스트 → 화면 [다음 →]
        first2 = next(i for i, q in enumerate(tut.quests) if q.screen == 2)
        tut.index = first2
        tut._show()
        tut.coach.prev_button.click()
        assert tut.current().screen == 1 and win.current_step == 1
        win.go(2)
        qapp.processEvents()
        tut._tick()
        assert tut.index == first2
    finally:
        win.tutorial.stop()
        win.close()
