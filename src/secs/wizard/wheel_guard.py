"""드롭다운 마우스 휠 막기 — 앱 전체에 한 번 거는 이벤트 필터.

누가 부름: `secs.wizard.run()` 이 QApplication 을 만든 직후 `install(app)`.
입력: 모든 위젯의 Wheel 이벤트. 출력: 닫힌 QComboBox 위의 휠은 값에 쓰지 않고 감싼 스크롤 영역으로 넘긴다.

왜: 페이지를 휠로 내리다가 커서가 드롭다운 위를 지나면 선택이 바뀌었다(09-23 사용자 지시 —
"실수하기 너무 좋다"). 펼친 목록 안의 휠은 목록 위젯이 받으므로 그대로 동작한다.
"""
from __future__ import annotations

from PyQt6 import QtCore, QtGui, QtWidgets


class WheelGuard(QtCore.QObject):
    def eventFilter(self, watched, event) -> bool:  # noqa: N802 - Qt 이름
        if event.type() != QtCore.QEvent.Type.Wheel or not isinstance(watched, QtWidgets.QComboBox):
            return False
        # 콤보는 휠을 못 받고, 이 방향으로 더 움직일 수 있는 가장 가까운 스크롤 영역에 직접 넘긴다
        # (Qt 의 부모 전달에 맡기면 합성 이벤트는 전달되지 않아 동작이 갈렸다). 2단계 패치 표처럼
        # 안쪽 영역이 끝까지 갔거나 스크롤할 게 없으면 바깥 페이지가 받는다(09-23 코드 리뷰).
        area = _scroll_target(watched, event.angleDelta())
        if area is not None:
            viewport = area.viewport()
            local = QtCore.QPointF(viewport.mapFromGlobal(event.globalPosition().toPoint()))
            forwarded = QtGui.QWheelEvent(local, event.globalPosition(), event.pixelDelta(), event.angleDelta(),
                                          event.buttons(), event.modifiers(), event.phase(), event.inverted())
            QtWidgets.QApplication.sendEvent(viewport, forwarded)
        event.accept()
        return True


def _can_move(area, delta) -> bool:
    """이 스크롤 영역이 휠 방향(delta: angleDelta)으로 더 갈 수 있는가."""
    vertical = abs(delta.y()) >= abs(delta.x())
    bar = area.verticalScrollBar() if vertical else area.horizontalScrollBar()
    step = delta.y() if vertical else delta.x()
    if bar is None or bar.maximum() <= bar.minimum() or step == 0:
        return False
    return bar.value() > bar.minimum() if step > 0 else bar.value() < bar.maximum()


def _scroll_target(widget, delta):
    """안에서 바깥으로 올라가며 움직일 수 있는 첫 스크롤 영역. 없으면 가장 바깥 영역."""
    outer = None
    node = widget.parentWidget()
    while node is not None:
        if isinstance(node, QtWidgets.QAbstractScrollArea):
            if _can_move(node, delta):
                return node
            outer = node
        node = node.parentWidget()
    return outer


def install(app: QtWidgets.QApplication) -> WheelGuard:
    """앱에 한 번만 건다. 두 번 불러도 같은 필터를 돌려준다."""
    guard = getattr(app, '_secs_wheel_guard', None)
    if guard is None:
        guard = WheelGuard(app)
        app.installEventFilter(guard)
        app._secs_wheel_guard = guard
    return guard
