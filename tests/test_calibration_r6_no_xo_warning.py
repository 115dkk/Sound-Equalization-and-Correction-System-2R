"""요구 대장 R6(2026-10-04): 필터 설계 상태 줄에 'XO 가 정해지지 않아…' 경고가 나오지 않는다.

그 문구는 옛 엔진(`design.Designer`·`lowband.LowbandDesigner`, 2026-10-10 슬림화로 화면과 함께 지움)의 경고다.
지금 흐름(`strings.FLOW` = 1·2·3·4·5·보정 설계·내보내기)에는 옛 6·7 화면 번호가 없고, 보정 설계는 새 엔진
(`correction_engine`)만 써서 그 경고를 만들지 않는다.

'동작 → 변화' 로 본다: 사용자가 누를 수 있는 이동을 모두 눌러 본 화면 번호 · XO 를 비운 채 [계산] 한 뒤 화면 글자.
"""
from __future__ import annotations

from test_calibration_step import _compute, _page, _project, fast_engine, synth  # noqa: F401
from test_wizard_common import engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

OLD_PAGES = (6, 7)


def test_user_navigation_never_reaches_the_old_design_pages(qapp, fake, tmp_path):  # noqa: F811
    import secs.model as model
    from PyQt6 import QtGui
    from secs.wizard.strings import FLOW

    assert not set(OLD_PAGES) & set(FLOW)
    project = model.Project(name='R6', config=model.SpeakerConfig.from_preset('2.1'))
    win = make_window(project, fake, tmp_path / 'r6.secs')
    try:
        seen = set()
        for button in win.steps_bar.buttons:                 # 진행바 칸 전부
            button.click()
            seen.add(win.current_step)
        for page in FLOW:                                    # 흐름 칸마다 [다음]·[이전]
            win.go(page)
            seen.add(win.next_page() or page)
            seen.add(win.previous_page())
        shortcuts = [s for s in win.findChildren(QtGui.QShortcut) if s.key().toString().startswith('Ctrl+')
                     and s.key().toString()[5:].isdigit()]
        assert len(shortcuts) == len(FLOW)
        for shortcut in shortcuts:                           # Ctrl+1 … Ctrl+7
            shortcut.activated.emit()
            seen.add(win.current_step)
        assert seen == set(FLOW)
    finally:
        win.close()
        qapp.processEvents()


def test_calibration_without_xo_shows_no_xo_warning(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    from secs.wizard import qt as QtWidgets

    case, irs = synth
    project = _project(case, 1)
    project.xo = {}                                          # 자동 보정 전 = XO 없음(옛 경고가 늘 뜨던 상태)
    page = _page(project, irs, tmp_path)
    try:
        run = _compute(page, qapp)
        assert page.status.text() == '계산 완료'
        texts = [w.text() for w in page.findChildren(QtWidgets.QLabel)]
        texts += [str(n) for n in run.notes] + [str(n) for n in getattr(run.engine, 'notes', [])]
        assert not [x for x in texts if 'XO 가 정해지지' in x]      # 옛 엔진 경고 'XO 가 정해지지 않아 베이스 매니지먼트에서 제외한 그룹'
    finally:
        page.cancel()
        page.deleteLater()

