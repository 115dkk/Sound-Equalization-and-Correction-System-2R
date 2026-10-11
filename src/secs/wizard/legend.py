# -*- coding: utf-8 -*-
"""다점 측정 위치 표시 — 위치 펜(`position_pen`·`position_z`)과 위치 칩(`PositionChips`, 켜고 끄기·커서 강조).

5단계 측정 그래프가 쓴다. 상태는 화면 위젯에만 있고 `.secs` 에는 저장하지 않는다. 옛 6·7단계 그래프의
범례 단추(`LegendToggle`, 2026-09-24)는 2026-10-10 슬림화로 그 화면과 함께 지웠다.
"""
from __future__ import annotations

import re

from PyQt6 import QtCore, QtGui
from PyQt6 import QtWidgets as _PlainQt

from secs.wizard import qt as QtWidgets
from secs.wizard.style import COLORS
from secs.wizard.tooltips import attach

def parse_color(text: str) -> QtGui.QColor:
    """'#rrggbb' 또는 CSS 'rgba(r,g,b,a)'(a 는 0~1) → QColor. QColor 는 rgba() 글자를 못 읽는다."""
    match = re.fullmatch(r'\s*rgba?\(([^)]*)\)\s*', str(text))
    if match:
        parts = [float(v) for v in match.group(1).split(',')]
        alpha = parts[3] if len(parts) > 3 else 1.0
        return QtGui.QColor(int(parts[0]), int(parts[1]), int(parts[2]), int(round(alpha * 255)))
    return QtGui.QColor(str(text))


# ──────────────────────────── 측정 위치(다점) ────────────────────────────
# 09-24 사용자: "다점 측정 시 어느 게 정축(청취 위치 1번) 응답인지 확인하기 어렵다".
# 규칙(5단계, 옛 7단계와 같았다): 위치 1 = 굵고 진한 채널 색, 나머지 = 가늘고 옅은 같은 색. 위치 칩으로 켜고 끄고,
# 칩에 커서를 대면 그 위치만 강조(나머지는 더 흐림). 상태는 화면 위젯에만(저장 안 함).

#: 위치 1(청취 위치) 선 두께 · 다른 위치 선 두께 · 다른 위치 불투명도 · 강조 중 나머지 불투명도.
MAIN_WIDTH, OTHER_WIDTH = 2.4, 1.0
OTHER_ALPHA, DIM_ALPHA = 0.38, 0.12


def position_pen(color, pos: int, hover: int = 0, width: float | None = None):
    """위치 곡선의 펜 — 위치 1 은 굵고 진하게, 나머지는 가늘고 옅게. `hover` 위치가 있으면 그것만 진하게."""
    import pyqtgraph as pg

    if isinstance(color, QtGui.QColor):
        base = QtGui.QColor(color)
    elif isinstance(color, (tuple, list)):
        base = QtGui.QColor(*[int(v) for v in color])
    else:
        base = parse_color(color)
    main = MAIN_WIDTH if width is None else float(width)
    if hover:
        strong = int(pos) == int(hover)
        alpha, size = (1.0, main + 0.4) if strong else (DIM_ALPHA, OTHER_WIDTH)
    elif int(pos) == 1:
        alpha, size = 1.0, main
    else:
        alpha, size = OTHER_ALPHA, OTHER_WIDTH
    pen_color = QtGui.QColor(base)
    pen_color.setAlphaF(base.alphaF() * alpha)
    return pg.mkPen(pen_color, width=size)


_CHIP_STYLE = (
    'QPushButton {{ background: {chassis}; color: #ffffff; border: 1px solid {chassis}; border-radius: 3px;'
    ' font-size: 11px; font-weight: 400; padding: 0px 5px; min-width: 14px; }}'
    'QPushButton:!checked {{ background: transparent; color: {ink3}; border-color: {line}; }}'
    'QPushButton[main="true"] {{ font-weight: 700; }}'
    'QPushButton:focus {{ border-color: {accent}; }}'
).format(**COLORS)


class PositionChips(QtWidgets.QWidget):
    """'위치 1 2 3 …' 칩 한 줄. 누르면 그 위치 곡선을 켜고 끄고(`toggled`), 커서를 대면 `hovered(pos)`
    (떠나면 0). 넘치면 가로로 스크롤한다(1024 폭에서 17위치). 켜짐 상태는 `hidden` 에만."""

    toggled = QtCore.pyqtSignal(int, bool)
    hovered = QtCore.pyqtSignal(int)

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self.hidden: set[int] = set()
        self.hover = 0
        self.chips: dict[int, _PlainQt.QPushButton] = {}
        outer = QtWidgets.QHBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(6)
        self.caption = QtWidgets.QLabel('측정 위치')
        self.caption.setObjectName('note')
        outer.addWidget(self.caption)
        self.area = _PlainQt.QScrollArea()
        self.area.setFrameShape(_PlainQt.QFrame.Shape.NoFrame)
        self.area.setWidgetResizable(True)
        self.area.setVerticalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.area.setHorizontalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        self.area.setStyleSheet('QScrollArea, QScrollArea > QWidget > QWidget { background: transparent; }')
        holder = _PlainQt.QWidget()
        self.row = _PlainQt.QHBoxLayout(holder)
        self.row.setContentsMargins(0, 0, 0, 0)
        self.row.setSpacing(3)
        self.area.setWidget(holder)
        self.area.setFixedHeight(34)
        outer.addWidget(self.area, 1)

    def set_count(self, count: int) -> None:
        """위치 수가 바뀌면 칩을 다시 만든다(끈 위치는 남은 번호만 기억). 1위치 이하면 줄을 숨긴다."""
        count = int(count)
        self.setVisible(count > 1)
        if list(self.chips) == list(range(1, count + 1)):
            return
        while self.row.count():
            item = self.row.takeAt(0)
            if item.widget() is not None:
                item.widget().deleteLater()
        self.chips = {}
        self.hidden = {pos for pos in self.hidden if pos <= count}
        for pos in range(1, count + 1):
            chip = _PlainQt.QPushButton(str(pos))
            chip.setCheckable(True)
            chip.setChecked(pos not in self.hidden)
            chip.setProperty('main', 'true' if pos == 1 else 'false')
            chip.setStyleSheet(_CHIP_STYLE)
            chip.setCursor(QtCore.Qt.CursorShape.PointingHandCursor)
            chip.setFixedHeight(22)
            chip.toggled.connect(lambda on, p=pos: self._on_toggle(p, on))
            chip.installEventFilter(self)
            attach(chip, 'win.plot_positions')
            self.row.addWidget(chip)
            self.chips[pos] = chip
        self.row.addStretch(1)

    def visible(self, pos: int) -> bool:
        return int(pos) not in self.hidden

    def _on_toggle(self, pos: int, on: bool) -> None:
        if on:
            self.hidden.discard(pos)
        else:
            self.hidden.add(pos)
        self.toggled.emit(pos, bool(on))

    def eventFilter(self, obj, event) -> bool:  # noqa: N802 - Qt 이름
        kind = event.type()
        if kind in (QtCore.QEvent.Type.Enter, QtCore.QEvent.Type.Leave):
            pos = next((p for p, chip in self.chips.items() if chip is obj), 0)
            self.hover = pos if kind == QtCore.QEvent.Type.Enter else 0
            self.hovered.emit(self.hover)
        return False


def position_z(pos: int, hover: int = 0) -> float:
    """위치 1(또는 강조 중인 위치)이 다른 선 위에 오게 하는 z 값 — 그리는 순서는 그대로 둔다."""
    if hover:
        return 2.0 if int(pos) == int(hover) else 0.0
    return 1.0 if int(pos) == 1 else 0.0


__all__ = ['PositionChips', 'position_pen', 'position_z', 'parse_color']
