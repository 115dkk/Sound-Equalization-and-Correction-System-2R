"""게임식 튜토리얼(2026-10-09 사용자: 체험 + 도움말을 합친 것, 목업 `UI 목업/2026-10-09 게임식 튜토리얼`).

'동작 → 변화': 튜토리얼 모드가 켜지면 퀘스트가 시작된다, 둘러보기는 [다음으로 →] 로 바로 넘어간다, 눌러 보기는 실제 위젯
신호로 넘어간다, 눌러 보기의 [다음으로 →] 는 첫 번은 알림만 · 두 번째에 넘어간다, [← 이전으로] 는 한 칸 뒤로, 퀘스트 창을
접고 편다, 구멍 밖 클릭은 막이 받는다, 영어에 한글이 안 남는다."""
from __future__ import annotations

import re

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
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


def test_tutorial_starts_and_quests_advance(qapp, fake):  # noqa: F811
    from PyQt6 import QtCore, QtTest

    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        assert tut.active and tut.index == 0 and tut.current().kind == 'look'
        # 퀘스트 창은 조작 위젯을 덮으면 알약으로 접힌다(2차 시험 t05) — 둘 중 하나는 보인다
        assert tut.overlay.isVisible() and tut.coach.isVisible() and (tut.panel.isVisible() or tut.panel.pill.isVisible())
        # 둘러보기 → 바로 넘어감
        while tut.current().kind == 'look':
            tut.coach.next_button.click()
        q = tut.current()
        assert q.kind == 'act' and q.screen == 1
        # 구멍 밖 클릭은 막이 받는다(퀘스트 그대로)
        before = tut.index
        QtTest.QTest.mouseClick(tut.overlay, QtCore.Qt.MouseButton.LeftButton, pos=QtCore.QPoint(5, win.height() - 5))
        assert tut.index == before
        # 실제 위젯 신호로 넘어감 — [다시 확인]
        win.devices_page.retry_button.click()
        qapp.processEvents()
        tut._tick()
        assert tut.index == before + 1
        # 눌러 보기의 [다음으로 →]: 첫 번은 알림만, 두 번째에 결과를 채우고 넘어감([다음 →] 퀘스트 → 2단계)
        assert tut.current().kind == 'act'
        tut.coach.next_button.click()
        assert tut.index == before + 1 and 'font-weight: 700' in tut.coach.hint.styleSheet()   # 규칙 줄이 분홍(2차 시험)
        tut.coach.next_button.click()
        qapp.processEvents()
        assert tut.index == before + 2 and win.current_step == 2
        # [← 이전으로]
        tut.coach.prev_button.click()
        assert tut.index == before + 1
        # 퀘스트 창 접기 · 펴기
        if tut.panel.folded:
            tut.panel.pill.click()
        tut.panel.fold_button.click()
        assert not tut.panel.isVisible() and tut.panel.pill.isVisible()
        tut.panel.pill.click()
        assert tut.panel.isVisible() and not tut.panel.pill.isVisible()
    finally:
        win.tutorial.stop()
        win.close()


def test_tutorial_quests_are_translated_and_targets_resolve(qapp, fake):  # noqa: F811
    from secs.i18n import translate
    from secs.wizard import tutorial_quests as tq

    for text in tq.all_strings():
        assert not HANGUL.search(translate(text, 'en')), text
    win = _trial(qapp, fake)
    try:
        for q in tq.QUESTS:
            win.go(q.screen)
            assert q.target(win) is not None, q.text
    finally:
        win.tutorial.stop()
        win.close()


def test_next_twice_always_reaches_the_end(qapp, tmp_path, monkeypatch):  # noqa: F811
    """10-10 AI 시험 10명이 모두 막힌 것: 서브 1개 샘플에서 'BA 탭' 퀘스트 · 계산 중 [다음으로]. [다음으로 →] 만
    눌러도(두 번씩) 끝까지 가야 한다(10-11 부터 샘플은 서브 3 — BA · 인프라 서브 퀘스트도 지난다)."""
    import time
    from PyQt6 import QtWidgets
    from secs.wizard import build_backend, new_project
    from secs.wizard.window import WizardWindow

    monkeypatch.setattr(QtWidgets.QMessageBox, 'open', lambda self: None)
    rew, devices, levels, measurer = build_backend(fake=True, use_sample_dolby=True)
    win = WizardWindow(new_project(True), rew=rew, devices=devices, levels=levels, measurer=measurer)
    win.ask_before_close = False
    win.resize(1280, 800)
    win.show()
    try:
        win.set_trial(True)
        tut = win.tutorial
        done = []
        tut.finished.connect(lambda: done.append(1))
        end = time.time() + 120
        while not done and time.time() < end:
            tut.coach.next_button.click()
            tut.coach.next_button.click()
            for _ in range(10):
                qapp.processEvents()
        assert done, (tut.index, len(tut.quests))
    finally:
        win.calibration_page.cancel()
        win.close()


def test_three_sub_demo_prepares_and_fills_the_sub_alignment_tab(qapp, monkeypatch):  # noqa: F811
    """10-11 사용자 결정(데모 방 = 실측 서브 3개 SW1 DBA L · SW2 DBA R · SW3 INFRA): 튜토리얼 미리 계산(`prepare_in_background`)이
    예외 없이 끝나고, [계산] 이 그 결과를 넘겨 BA 탭에 결과가 그려지며, BA · 인프라 서브 퀘스트가 나온다."""
    import time

    from PyQt6 import QtWidgets
    from secs import calibration as cal
    from secs.wizard import build_backend, new_project
    from secs.wizard import tutorial_quests as tq
    from secs.wizard.strings import CAL_STEP
    from secs.wizard.window import WizardWindow

    monkeypatch.setattr(QtWidgets.QMessageBox, 'open', lambda self: None)
    rew, devices, levels, measurer = build_backend(fake=True, use_sample_dolby=True)
    win = WizardWindow(new_project(True), rew=rew, devices=devices, levels=levels, measurer=measurer)
    win.ask_before_close = False
    win.resize(1280, 800)
    win.show()
    page = win.calibration_page
    try:
        win.set_trial(True)
        qapp.processEvents()
        win.tutorial.stop()                       # 퀘스트 창 없이 화면만
        win.go(5)
        assert win.measure_page.load_sample() == 1
        qapp.processEvents()
        # 10-11 사용자: "연산하는 척만 하라니까" — 튜토리얼은 보정 계산을 하지 않고 저장해 둔 데모 결과만 보여 준다
        def no_compute(*_a, **_k):
            raise AssertionError('튜토리얼이 보정 계산을 했다')
        monkeypatch.setattr(cal, 'compute', no_compute)
        win._prepare_tutorial_calibration()       # 데모 측정이 들어오면 창이 1.5 s 뒤 부르는 것
        assert getattr(page, '_prepare_thread', None) is None          # 뒤에서 도는 계산 없음
        assert page._prepared is not None
        prepared = page._prepared[1]
        assert list(prepared.engine.subs) == ['SW1', 'SW2', 'SW3']
        view = cal.sub_combine(prepared)
        assert view is not None and set(view['after']) == {'SW1', 'SW2', 'SW3'}

        win.go(CAL_STEP)
        qapp.processEvents()
        assert page.tab_buttons['subs'].isVisible()
        assert all(q.when(win) for q in tq.QUESTS if q.when is not None)      # BA · 인프라 서브 퀘스트가 나온다
        assert page.compute_clicked()             # 튜토리얼 [계산] = 미리 계산한 결과를 짧게 보여 주기
        end = time.time() + 60
        while page.run is None and time.time() < end:
            qapp.processEvents()
            time.sleep(0.02)
        assert page.run is not None and list(page.run.engine.subs) == ['SW1', 'SW2', 'SW3']
        page.set_tab('subs')
        qapp.processEvents()
        assert page.tab == 'subs' and page.plot.listDataItems()             # 서브마다 정렬 전 · 후 + 합
        # 인프라 서브 퀘스트를 대신 해 주면 마지막 서브 = SW3 = 실측 INFRA
        infra = next(q for q in tq.QUESTS if q.target(win) is page.infra_combo)
        infra.auto(win)
        assert page.infra_state()[0] == 'SW3'
        assert not page.stale()                   # 설정이 바뀌어도 다시 계산하라고 하지 않는다
        page.run = None
        assert page.compute_clicked()             # 설정이 바뀐 뒤 [계산] 도 계산하지 않고 저장 결과를 보여 준다
        end = time.time() + 30
        while page.run is None and time.time() < end:
            qapp.processEvents()
            time.sleep(0.02)
        assert page.run is not None
        for tab in ('gd', 'xo', 'subs', 'spl'):
            page.set_tab(tab)
            qapp.processEvents()
            assert page.plot.listDataItems(), tab
        # [서브만 계산] 도 계산하지 않고 저장해 둔 결과(SW3 = 인프라)를 보여 준다
        assert cal.load_demo_subs_run() is not None
        assert page.compute_subs()
        end = time.time() + 30
        while page.sub_run is None and time.time() < end:
            qapp.processEvents()
            time.sleep(0.02)
        assert page.sub_run is not None and list(page.sub_run.engine.subs) == ['SW1', 'SW2', 'SW3']
        page.set_tab('subs')
        qapp.processEvents()
        assert page.plot.listDataItems()
    finally:
        page.cancel()
        win.close()
