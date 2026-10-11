"""5단계 배치도에서 위치를 눌러 고른다 (2026-09-23 사용자 지시).

"측정(5단계)에서 가운데 영역의 배치도에서 각 위치를 클릭하면 그 위치가 선택되게 해줘."
마커 클릭·목록 줄·←/→ 가 같은 선택이 되고, [이 위치 측정] 은 그 위치를 잰다. 이미 잰 위치면
확인 없이 다시 재서 기록을 바꾸고, 측정 중에는 선택이 바뀌지 않는다. 4단계 배치도는 보기 전용.
"""
from __future__ import annotations

from PyQt6 import QtCore
from PyQt6.QtTest import QTest

from test_wizard_common import PRESET, engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def _click_marker(view, index: int) -> None:
    QTest.mouseClick(view.viewport(), QtCore.Qt.MouseButton.LeftButton,
                     QtCore.Qt.KeyboardModifier.NoModifier, view.marker_center(index))


def _window(project, fake, tmp_path):
    win = make_window(project, fake, tmp_path / 'p.secs')
    win.go(5)
    return win, win.measure_page


def test_clicking_a_marker_picks_the_position_and_measures_it(qapp, project, fake, tmp_path):
    win, page = _window(project, fake, tmp_path)
    try:
        assert page.current_position() == 1
        _click_marker(page.view, 2)                      # 3번 위치
        assert page.current_position() == 3
        assert page.view._current == 2                   # 배치도 강조가 따라온다
        assert page.positions.currentRow() == 2          # 목록도 같은 줄
        assert page.measure_position(page.current_position())
        assert page.wait(60000)
        assert page.done_positions() == [3]
        assert page.current_position() == 1              # 잰 뒤엔 다시 '다음 안 잰 위치'
    finally:
        win.close()
        qapp.processEvents()


def test_list_row_and_arrow_keys_pick_the_same_way(qapp, project, fake, tmp_path):
    win, page = _window(project, fake, tmp_path)
    try:
        page.positions.setCurrentRow(4)
        assert page.current_position() == 5 and page.view._current == 4
        QTest.keyClick(page.view, QtCore.Qt.Key.Key_Right)
        assert page.current_position() == 6 and page.positions.currentRow() == 5
        QTest.keyClick(page.view, QtCore.Qt.Key.Key_Left)
        assert page.current_position() == 5
    finally:
        win.close()
        qapp.processEvents()


def test_picking_a_done_position_measures_it_again_and_replaces_the_record(qapp, project, fake, tmp_path):
    win, page = _window(project, fake, tmp_path)
    try:
        assert page.measure_position(1) and page.wait(60000)
        before = dict(next(m for m in project.measurements if m['pos'] == 1)['uuid'])
        _click_marker(page.view, 0)
        assert page.current_position() == 1
        assert page.measure_button.text() == '이 위치 다시 측정'
        page.measure_current()
        assert page.wait(60000)
        assert '다시 측정' in page.log.toPlainText()
        records = [m for m in project.measurements if m['pos'] == 1]
        assert len(records) == 1                          # 기록은 하나 — 바뀌었지 늘지 않았다
        assert records[0]['uuid'] != before
    finally:
        win.close()
        qapp.processEvents()


def test_clicks_are_ignored_while_measuring(qapp, project, tmp_path):
    from secs.rewapi.fake import FakeREW

    slow = FakeREW(delay_s=0.05)                          # 스윕 흉내에 시간이 든다
    win = make_window(project, slow, tmp_path / 'p.secs')
    try:
        win.go(5)
        page = win.measure_page
        assert page.measure_position(1)
        _click_marker(page.view, 4)
        page.positions.setCurrentRow(6)
        assert page.current_position() == 1
        assert page.positions.currentRow() == 0
        assert page.wait(60000)
    finally:
        win.close()
        qapp.processEvents()
        slow.close()


def test_step4_map_stays_view_only(qapp, project, fake, tmp_path):
    win = make_window(project, fake, tmp_path / 'p.secs')
    try:
        win.go(4)
        view = win.layout_page.view
        seen = []
        view.position_clicked.connect(seen.append)
        _click_marker(view, 2)
        QTest.keyClick(view, QtCore.Qt.Key.Key_Right)
        assert seen == []
    finally:
        win.close()
        qapp.processEvents()
