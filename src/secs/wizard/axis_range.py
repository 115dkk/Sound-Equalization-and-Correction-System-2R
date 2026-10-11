# -*- coding: utf-8 -*-
"""그래프 축 범위 조절 — 모서리의 작은 [축] 단추 → x·y 하한/상한 네 칸 + [적용]·[자동] (2026-09-24 사용자 요청).

사용자: "그래프가 나오는 모든 부분은 축의 상한과 하한 파라미터를 조정할 수 있게". 5단계 측정, 6단계
(주파수 응답·임펄스·최종 합산), 7단계 저역 통합, 8단계 예측 vs 실측 그래프가 `AxisControl` 을 붙인다.

- 그래프는 전처럼 자기 기본 범위를 잡고, 그 뒤 `apply()` 를 부른다 — 사용자가 정한 범위가 있으면 그것으로 덮는다.
- 값은 그래프 종류(`kind`)별로 **이 프로그램 실행 동안만** 기억한다(모듈 사전 `_SESSION`). `.secs` 에는 없다.
- [자동] = 그 종류의 기억을 지우고 그래프의 기본 범위로 되돌린다(`on_change` 로 그래프가 다시 그린다).
- 로그 주파수 축(`x_log=True`)은 칸에는 Hz, 그래프에는 log10 으로 넣는다. 그래프들은 마우스 확대가 꺼져 있어
  (드래그는 커튼·XO 손잡이 몫) 이 단추가 범위를 바꾸는 유일한 길이다.
"""
from __future__ import annotations

import math

from PyQt6 import QtCore
from PyQt6 import QtWidgets as _PlainQt

from secs.wizard import qt as QtWidgets
from secs.wizard.style import COLORS

#: kind → (x 하한, x 상한, y 하한, y 상한) — 칸에 보이는 단위(Hz·ms·dB) 그대로.
_SESSION: dict[str, tuple[float, float, float, float]] = {}

_BUTTON_STYLE = (
    'QPushButton {{ background: {panel}; color: {ink2}; border: 1px solid {line}; border-radius: 3px;'
    ' font-size: 11px; font-weight: 400; padding: 0px 6px; min-height: 0px; }}'
    'QPushButton:hover {{ color: {ink}; border-color: {accent}; }}'
    'QPushButton[active="true"] {{ color: #ffffff; background: {chassis}; border-color: {chassis}; }}'
).format(**COLORS)


def session_range(kind: str):
    return _SESSION.get(str(kind))


def clear_session() -> None:
    """테스트용 — 기억한 범위를 모두 지운다."""
    _SESSION.clear()


class AxisControl(QtCore.QObject):
    """그래프(`pg.PlotWidget`) 하나에 붙는 축 범위 조절기."""

    changed = QtCore.pyqtSignal()

    def __init__(self, plot, kind: str, *, x_log: bool = True, x_unit: str = 'Hz', y_unit: str = 'dB',
                 on_change=None) -> None:
        super().__init__(plot)
        self.plot = plot
        self.kind = str(kind)
        self.x_log = bool(x_log)
        self.x_unit = str(x_unit)
        self.y_unit = str(y_unit)
        self._on_change = on_change
        self.button = QtWidgets.QPushButton('축', plot)      # 번역 단추(영어 'Axes')
        from secs.wizard.tooltips import attach
        attach(self.button, 'win.plot_axes')             # 풍선 도움말(tooltips.py 가 정본)
        self.button.setStyleSheet(_BUTTON_STYLE)
        self.button.setCursor(QtCore.Qt.CursorShape.PointingHandCursor)
        self.button.setFocusPolicy(QtCore.Qt.FocusPolicy.TabFocus)
        self.button.setFixedHeight(20)
        self.button.setMinimumWidth(30)
        self.button.clicked.connect(self.open_editor)
        self.button.raise_()
        plot.installEventFilter(self)
        self._editor = None
        self._place()
        self._mark()

    # ── 모양 ──
    def eventFilter(self, obj, event) -> bool:  # noqa: N802 - Qt 이름
        if obj is self.plot and event.type() in (QtCore.QEvent.Type.Resize, QtCore.QEvent.Type.Show):
            self._place()
        return False

    def _place(self) -> None:
        self.button.adjustSize()
        self.button.move(max(0, self.plot.width() - self.button.width() - 4), 4)
        self.button.raise_()

    def _mark(self) -> None:
        self.button.setProperty('active', 'true' if self.kind in _SESSION else 'false')
        self.button.style().unpolish(self.button)
        self.button.style().polish(self.button)

    # ── 범위 ──
    def current(self) -> tuple[float, float, float, float]:
        """지금 보이는 범위(칸 단위)."""
        (x0, x1), (y0, y1) = self.plot.getPlotItem().vb.viewRange()
        if self.x_log:
            x0, x1 = 10.0 ** x0, 10.0 ** x1
        return float(x0), float(x1), float(y0), float(y1)

    def apply(self) -> bool:
        """기억한 범위가 있으면 그래프에 넣는다. 그래프가 자기 기본 범위를 잡은 뒤에 부른다."""
        value = _SESSION.get(self.kind)
        self._mark()
        if value is None:
            return False
        x0, x1, y0, y1 = value
        if self.x_log:
            x0, x1 = math.log10(max(x0, 1e-3)), math.log10(max(x1, 1e-3))
        self.plot.setXRange(x0, x1, padding=0.0)
        self.plot.setYRange(y0, y1, padding=0.0)
        return True

    def set_range(self, x0: float, x1: float, y0: float, y1: float) -> bool:
        """범위를 정한다(하한 < 상한, 로그 축은 0 보다 커야). 틀리면 False, 그대로 둔다."""
        x0, x1, y0, y1 = (float(v) for v in (x0, x1, y0, y1))
        if not (x0 < x1 and y0 < y1) or (self.x_log and x0 <= 0):
            return False
        _SESSION[self.kind] = (x0, x1, y0, y1)
        self._changed()
        return True

    def reset(self) -> None:
        """[자동] — 기억을 지우고 그래프 기본 범위로."""
        _SESSION.pop(self.kind, None)
        self._changed()

    def _changed(self) -> None:
        if self._on_change is not None:
            self._on_change()          # 그래프가 기본 범위를 다시 잡고 apply() 를 부른다
        else:
            self.apply()
        self._mark()
        self.changed.emit()

    # ── 편집 칸 ──
    def open_editor(self) -> None:
        editor = self.editor()
        editor.fill(self.current())
        editor.popup(self.button.mapToGlobal(QtCore.QPoint(self.button.width(), self.button.height())))

    def editor(self) -> '_RangeMenu':
        if self._editor is None:
            self._editor = _RangeMenu(self)
        return self._editor


class _RangeMenu(_PlainQt.QMenu):
    """네 칸 + [적용]·[자동]. Enter 는 [적용]."""

    def __init__(self, control: AxisControl) -> None:
        super().__init__(control.plot)
        self.control = control
        body = QtWidgets.QWidget()
        grid = QtWidgets.QGridLayout(body)
        grid.setContentsMargins(10, 8, 10, 8)
        grid.setHorizontalSpacing(8)
        grid.setVerticalSpacing(4)
        x_name = '주파수' if control.x_log else '시간'
        self.boxes: list[_PlainQt.QDoubleSpinBox] = []
        rows = ((f'{x_name} 하한', control.x_unit), (f'{x_name} 상한', control.x_unit),
                ('세로 하한', control.y_unit), ('세로 상한', control.y_unit))
        for row, (name, unit) in enumerate(rows):
            grid.addWidget(QtWidgets.QLabel(name), row, 0)
            box = _PlainQt.QDoubleSpinBox()
            box.setDecimals(1 if unit != 'Hz' else 0)
            box.setRange(0.1 if (control.x_log and row < 2) else -100000.0, 100000.0)
            box.setSuffix(f' {unit}')
            box.setKeyboardTracking(False)
            box.setMinimumWidth(110)
            grid.addWidget(box, row, 1)
            self.boxes.append(box)
        buttons = QtWidgets.QHBoxLayout()
        self.apply_button = QtWidgets.QPushButton('적용')
        self.auto_button = QtWidgets.QPushButton('자동')
        self.apply_button.clicked.connect(self._apply)
        self.auto_button.clicked.connect(self._auto)
        buttons.addWidget(self.auto_button)
        buttons.addStretch(1)
        buttons.addWidget(self.apply_button)
        grid.addLayout(buttons, 4, 0, 1, 2)
        self.note = QtWidgets.QLabel('')
        self.note.setObjectName('note')
        grid.addWidget(self.note, 5, 0, 1, 2)
        action = _PlainQt.QWidgetAction(self)
        action.setDefaultWidget(body)
        self.addAction(action)

    def fill(self, values) -> None:
        self.note.setText('')
        for box, value in zip(self.boxes, values):
            box.setValue(float(value))

    def keyPressEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        if event.key() in (QtCore.Qt.Key.Key_Return, QtCore.Qt.Key.Key_Enter):
            self._apply()
            return
        super().keyPressEvent(event)

    def _apply(self) -> None:
        if self.control.set_range(*(box.value() for box in self.boxes)):
            self.close()
        else:
            self.note.setText('하한은 상한보다 작아야 합니다.')

    def _auto(self) -> None:
        self.control.reset()
        self.close()


__all__ = ['AxisControl', 'session_range', 'clear_session']
