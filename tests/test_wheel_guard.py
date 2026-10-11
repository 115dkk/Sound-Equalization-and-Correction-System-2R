"""드롭다운 휠 막기(09-23 사용자 지시): 닫힌 콤보 위 휠은 선택을 바꾸지 않고 페이지를 스크롤한다."""
from __future__ import annotations

from PyQt6 import QtCore, QtGui, QtWidgets

from test_wizard_common import qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def _wheel(widget, delta_y: int = -120) -> None:
    pos = QtCore.QPointF(widget.width() / 2, widget.height() / 2)
    event = QtGui.QWheelEvent(pos, widget.mapToGlobal(pos), QtCore.QPoint(0, 0), QtCore.QPoint(0, delta_y),
                              QtCore.Qt.MouseButton.NoButton, QtCore.Qt.KeyboardModifier.NoModifier,
                              QtCore.Qt.ScrollPhase.NoScrollPhase, False)
    QtWidgets.QApplication.sendEvent(widget, event)


def test_combo_wheel_is_blocked_and_page_scrolls(qapp):  # noqa: F811
    from secs.wizard import wheel_guard

    app = qapp
    assert wheel_guard.install(app) is wheel_guard.install(app)   # 두 번 걸어도 하나

    scroll = QtWidgets.QScrollArea()
    page = QtWidgets.QWidget()
    box = QtWidgets.QVBoxLayout(page)
    combo = QtWidgets.QComboBox()
    combo.addItems(['A', 'B', 'C'])
    box.addWidget(combo)
    box.addSpacing(3000)
    scroll.setWidget(page)
    scroll.resize(300, 200)
    scroll.show()
    app.processEvents()

    _wheel(combo)
    app.processEvents()
    assert combo.currentIndex() == 0
    assert scroll.verticalScrollBar().value() > 0
    scroll.close()


def test_combo_in_a_table_that_cannot_scroll_passes_the_wheel_to_the_page(qapp):  # noqa: F811
    """1단계 패치 표 안의 드롭다운: 표가 더 못 움직이면 바깥 페이지가 스크롤된다(09-23 코드 리뷰)."""
    from secs.wizard import wheel_guard

    wheel_guard.install(qapp)
    scroll = QtWidgets.QScrollArea()
    page = QtWidgets.QWidget()
    box = QtWidgets.QVBoxLayout(page)
    table = QtWidgets.QTableWidget(2, 1)
    combo = QtWidgets.QComboBox()
    combo.addItems(['A', 'B', 'C'])
    table.setCellWidget(0, 0, combo)
    table.setMinimumHeight(200)                  # 두 행이 다 보여 표 자체는 스크롤할 게 없다
    box.addWidget(table)
    box.addSpacing(3000)
    scroll.setWidget(page)
    scroll.resize(300, 300)
    scroll.show()
    qapp.processEvents()

    _wheel(combo)
    qapp.processEvents()
    assert combo.currentIndex() == 0
    assert scroll.verticalScrollBar().value() > 0
    scroll.close()


def test_combo_in_a_scrollable_table_scrolls_the_table_first(qapp):  # noqa: F811
    from secs.wizard import wheel_guard

    wheel_guard.install(qapp)
    table = QtWidgets.QTableWidget(60, 1)
    combo = QtWidgets.QComboBox()
    combo.addItems(['A', 'B', 'C'])
    table.setCellWidget(0, 0, combo)
    table.resize(300, 200)
    table.show()
    qapp.processEvents()

    _wheel(combo)
    qapp.processEvents()
    assert combo.currentIndex() == 0
    assert table.verticalScrollBar().value() > 0
    table.close()
