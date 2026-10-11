"""튜토리얼 AI 시험 3차(2026-10-10, 4명 · 모두 31/31 완주) 지적 — 고친 것이 다시 깨지지 않게.

- 안내 카드 [계속 →](옛 '다음으로 →')은 한 번 누른 뒤에도 그 자리(규칙 줄 자리를 처음부터 잡는다).
- 구성 화면이 출력 번호 말고 다른 이유로 막혀 있으면 두 번 눌러도 넘어가지 않고 이유를 카드에(코드 리뷰).
- 단계 끝 알림 · 목록 제목은 업무 말(사용자 결정 1A — '클리어 ★' · '퀘스트' 없음), 틀린 곳 알림은 '카드의 [계속 →]'.
- 레벨 체크: 못 잰 채널이 4개 넘으면 '외 n개', 재생 중 배지는 숨겨도 자리를 지킨다.
- 측정 그래프: 모든 채널을 서로 다른 색으로(전엔 앞 6채널만 · 8색), 축 이름은 눈금 글자 진하기, 튜토리얼 로그에 임시 경로 없음.
- 보정 설계: XO 탭도 SPL 탭과 같은 타깃 기준, 결과 줄 '6 % → 100 %', 계산 전 GD · XO 탭 안내, SPL 가로축 20k 눈금.
- 장치 화면: 'Demo ASIO 16ch · 16ch' 처럼 채널 수를 두 번 붙이지 않는다.
"""
from __future__ import annotations

import re

import numpy as np

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401
from test_calibration_step import _compute, _page, _project, fast_engine, synth  # noqa: F401


def _trial(qapp, fake):  # noqa: F811
    from secs.wizard import new_project

    win = make_window(new_project(True), fake, None)
    win.resize(1280, 800)
    win.show()
    win.set_trial(True)
    qapp.processEvents()
    return win


def _goto(tut, pred):
    for _ in range(80):
        q = tut.current()
        if q is None or pred(q):
            return q
        tut.coach.next_button.click()
        if tut.current() is q:
            tut.coach.next_button.click()
    raise AssertionError('퀘스트를 못 찾음')


def test_card_button_stays_put_and_toasts_use_work_words(qapp, fake):  # noqa: F811
    from PyQt6 import QtCore

    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        assert tut.coach.next_button.text() in ('계속 →', 'Continue →')
        assert '퀘스트' not in tut.panel.title.text() and '퀘스트' not in tut.panel.pill.text()
        tut._outside()                                            # 둘러보기 카드에서 막을 누르면
        assert '카드의 [계속 →]' in tut.toast.text()
        act = _goto(tut, lambda q: q.kind == 'act')
        tut._place()
        qapp.processEvents()
        before = (tut.coach.height(), tut.coach.next_button.mapTo(win, QtCore.QPoint(0, 0)))
        tut.coach.next_button.click()                             # 첫 번: 규칙 줄만 분홍으로
        for _ in range(5):
            qapp.processEvents()
        tut._place()
        assert tut.current() is act
        assert (tut.coach.height(), tut.coach.next_button.mapTo(win, QtCore.QPoint(0, 0))) == before
        # 단계를 넘기면 업무 말 알림(게임 말 '클리어 ★' 없음)
        _goto(tut, lambda q: q.screen == 2)
        assert tut.toast.text().startswith('단계 1/7 끝') and '★' not in tut.toast.text()
    finally:
        win.tutorial.stop()
        win.close()


def test_skip_does_not_pass_a_screen_that_stays_blocked(qapp, fake):  # noqa: F811
    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        q = _goto(tut, lambda x: x.screen == 2 and x.blocked is not None)
        win.can_advance = lambda *a: (False, '역할이 겹칩니다')      # 출력 번호 자동 정렬로는 못 고치는 막힘
        tut.coach.next_button.click()
        tut.coach.next_button.click()
        for _ in range(5):
            qapp.processEvents()
        assert tut.current() is q and win.current_step == 2
        assert '역할이 겹칩니다' in tut.coach.text.text()
    finally:
        win.tutorial.stop()
        win.close()


def test_skipping_the_sample_does_not_skip_the_graph_card(qapp, fake):  # noqa: F811
    """[데모 데이터] 를 두 번 눌러 대신 채우면 채우는 동안 판정이 먼저 넘겨, 다음 카드(측정 그래프)까지 건너뛰었다."""
    win = _trial(qapp, fake)
    try:
        tut = win.tutorial
        q = _goto(tut, lambda x: x.screen == 5 and x.kind == 'act')
        index = tut.index
        tut.coach.next_button.click()
        tut.coach.next_button.click()
        for _ in range(20):
            qapp.processEvents()
        assert win.project.measurements
        assert tut.index == index + 1 and tut.current() is not q
        assert tut.current().target(win) is win.measure_page.plot
    finally:
        win.tutorial.stop()
        win.close()


def test_level_lists_how_many_more_and_keeps_room_for_the_badge(qapp, fake):  # noqa: F811
    import secs.model as model
    import secs.wizard.strings  # noqa: F401
    from secs.i18n import translate
    from secs.wizard.step3_level import Step3Level

    project = model.Project(config=model.SpeakerConfig.from_preset('7.1.4'))
    page = Step3Level(project, levels=fake)
    try:
        ok, message = page.can_advance()
        assert not ok and message.endswith('외 8개'), message
        assert translate(message, 'en').endswith('and 8 more')
        assert page.playing_pill.sizePolicy().retainSizeWhenHidden()
    finally:
        page.deleteLater()


def test_measure_graph_draws_every_channel_in_its_own_color(qapp, fake):  # noqa: F811
    from secs.wizard.style import COLORS

    win = _trial(qapp, fake)
    try:
        win.tutorial.stop()
        win.go(5)
        page = win.measure_page
        page.load_sample()
        for _ in range(10):
            qapp.processEvents()
        roles = page.plan().roles
        items = page.plot.listDataItems()
        assert len(items) == len(roles) >= 12
        assert len({item.opts['pen'].color().name() for item in items}) == len(roles)
        assert page.plot.getAxis('left').labelStyle.get('color') == COLORS['ink2']
        log = page.log.toPlainText()
        assert str(page.project_path) not in log and '튜토리얼 임시 프로젝트' in log
        assert win.devices_page.output_combo.itemText(0).count('16ch') == 1
    finally:
        win.close()


def test_calibration_tabs_share_the_target_level(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    from secs import calibration as cal
    from secs.wizard.style import COLORS

    case, irs = synth
    project = _project(case, 3)
    page = _page(project, irs, tmp_path)
    try:
        # 계산 전 GD · XO 탭 — 빈 그래프에 SPL 눈금 대신 그 탭의 범위와 안내 한 줄
        page.set_tab('gd')
        assert '계산 전' in page.summary_label.text()
        assert page.plot.getViewBox().viewRange()[1] == [-10.0, 100.0]
        page.set_tab('xo')
        assert '계산 전' in page.summary_label.text()
        page.set_tab('spl')
        run = _compute(page, qapp)
        assert re.search(r'\d+ % → \d+ %', page.metrics_label.text()), page.metrics_label.text()
        assert page.plot.getViewBox().viewRange()[0][1] > np.log10(20000.0)       # 20k 눈금이 보이게 끝에 여유
        assert page.plot.getAxis('left').labelStyle.get('color') == COLORS['ink2']
        # XO 탭: 타깃이 SPL 탭처럼 제 값(프로젝트 타깃)에, 곡선은 같은 만큼만 옮겨 그린다
        page.set_tab('xo')
        group = page.xo_group
        x, target = page._xo_curves['target'].getData()
        want = np.asarray(project.target.sample(cal.XO_GRID), float)
        assert abs(float(np.median(target - want))) < 0.5
        view = cal.xo_preview(run, group, page.xo_of(group), role=page.channel)
        _x, main = page._xo_curves['main'].getData()
        assert np.allclose(main, view['main'] - page._level_offset(run))
        assert '타깃 기준' in page.plot.getAxis('left').labelString()
    finally:
        page.deleteLater()
