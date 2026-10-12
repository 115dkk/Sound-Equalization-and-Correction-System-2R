"""튜토리얼 보정 설계 · BA 퀘스트(2026-10-11 사용자: "튜토리얼에서 보정설계 쪽을 더 자세히, 특히 서브정렬 쪽 설명이
많이 부실").

'동작 → 변화': 데모 방(7.1.4 + 서브 3)으로 튜토리얼을 열면 BA 탭을 부분마다 짚는 퀘스트(그래프 → 결과 줄과 값 표 →
설정 칸 → 인프라 서브 → 고정 필터 → [서브만 계산] → 그 뒤 결과)가 차례로 나오고, 카드를 띄우면 BA 탭이 펴지며 대상
위젯이 보이고 강조 테두리가 그려지며 안내 카드는 테두리를 덮지 않는다. 고정 필터 칸은 맨 아래 PEQ 줄까지 화면 안에 든다.
[서브만 계산] 퀘스트는 새 결과가 와야 끝나고 계산하는 동안은 끝나지 않는다. SPL · GD · XO 둘러보기도 그 탭을 편다.
"""
from __future__ import annotations

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

#: BA 탭 퀘스트의 문구 머리(나오는 순서) — 문구를 바꾸면 여기도.
SUB_QUESTS = ('BA 그래프', '결과 줄과 값 표', 'BA 설정', '[인프라 서브]', '고정 필터',
              '설정이 바뀌었습니다', '[서브만 계산] 뒤의 결과')


def _trial(qapp, fake):  # noqa: F811
    from secs.wizard import new_project

    win = make_window(new_project(True), fake, None)
    win.resize(1280, 800)
    win.show()
    win.set_trial(True)
    qapp.processEvents()
    return win


def _widgets(raw) -> list:
    items = raw if isinstance(raw, (list, tuple)) else [raw]
    return [w for item in items for w in (item if isinstance(item, (list, tuple)) else [item]) if w is not None]


def _show(tut, qapp, index):  # noqa: F811
    tut.index = index
    tut._show()
    for _ in range(4):
        qapp.processEvents()
        tut._tick()


def test_sub_alignment_quests_show_their_targets(qapp, fake):  # noqa: F811
    from PyQt6 import QtCore
    from secs.wizard.strings import CAL_STEP

    win = _trial(qapp, fake)
    page = win.calibration_page
    try:
        tut = win.tutorial
        cal = [i for i, q in enumerate(tut.quests) if q.screen == CAL_STEP]
        found = [next(i for i in cal if tut.quests[i].text.startswith(head)) for head in SUB_QUESTS]
        assert found == sorted(found), found                      # 그래프 → … → 결과 순서
        for index in found:
            q = tut.quests[index]
            if q.text.startswith('고정 필터'):
                page.set_infra('SW3', 25.0, 'LR24')               # 앞 퀘스트(인프라 서브)를 한 것처럼
                for _ in range(4):
                    qapp.processEvents()                          # 인프라 줄이 생겨 카드가 길어진 뒤 자리를 잡게
            _show(tut, qapp, index)
            assert page.tab == 'subs', q.text
            widgets = _widgets(q.target(win))
            assert widgets and widgets[0].isVisible(), q.text
            rings = tut.overlay.rings()
            assert rings, q.text                                   # 강조 테두리
            coach = tut.coach.geometry()
            assert not any(coach.intersects(r) for r in rings), (q.text, coach, rings)
            if q.text.startswith('고정 필터'):
                # 맨 아래 PEQ 줄까지 스크롤되어 테두리 안에 든다(첫 위젯만 보이게 스크롤하면 PEQ 가 창 밖으로 잘렸다)
                last = page.sub_peq_spins[-1][-1]
                rect = QtCore.QRect(last.mapTo(win, QtCore.QPoint(0, 0)), last.size())
                assert any(r.contains(rect) for r in rings), (rect, rings)
                assert page.sub_hp_spin.value() == 25.0           # SW1 에 인프라 XO 의 HP 가 들어갔다
                names = [w for w in widgets if w.metaObject().className() == 'QLabel']
                assert names and names[0].text() == 'HP'           # 줄 이름까지 감싼다
    finally:
        win.tutorial.stop()
        win.close()


def test_subs_only_quest_waits_for_a_new_result(qapp, fake):  # noqa: F811
    from secs.wizard.strings import CAL_STEP

    win = _trial(qapp, fake)
    page = win.calibration_page
    try:
        tut = win.tutorial
        q = next(x for x in tut.quests if x.screen == CAL_STEP and x.text.startswith('설정이 바뀌었습니다'))
        assert q.kind == 'act' and q.target(win) is page.sub_compute_button
        before = q.base(win)
        assert not q.done(win, before)
        page._job = {'kind': 'subs', 'token': object(), 'thread': None, 'started': 0.0}
        assert q.busy(win)                                         # 계산 중에는 끝나지 않는다
        page._job = None
        page.sub_run = object()                                    # 새 결과가 왔다
        assert q.done(win, before) and not q.busy(win)
        page.sub_run = None
        assert q.auto(win) is None and q.note                     # 건너뛰면 계산하지 않고 알림만
    finally:
        win.tutorial.stop()
        win.close()


def test_tab_tour_quests_open_their_tab(qapp, fake):  # noqa: F811
    from secs.wizard.strings import CAL_STEP

    win = _trial(qapp, fake)
    page = win.calibration_page
    try:
        tut = win.tutorial
        for head, tab in (('SPL 탭', 'spl'), ('GD 탭', 'gd'), ('XO 탭 —', 'xo')):
            index = next(i for i, q in enumerate(tut.quests) if q.screen == CAL_STEP and q.text.startswith(head))
            page.set_tab('subs')
            _show(tut, qapp, index)
            assert page.tab == tab, head
            assert tut.overlay.rings(), head
    finally:
        win.tutorial.stop()
        win.close()
