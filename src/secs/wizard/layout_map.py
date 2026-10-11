# -*- coding: utf-8 -*-
"""배치도 — 목업의 `roomSVG()`(2단계 스피커 평면도)와 `arrSVG()`/`arrPoints()`
(4·5단계 측정 위치 배치도)를 QGraphicsView 로 옮긴 것.

`RoomLayoutView` 는 2단계(`step2_config`)가 만들고 `set_speakers(스피커 목록, 선택)`
으로 다시 그린다. 마커를 누르면 `speaker_clicked(index)` 를 낸다. `ArrayLayoutView`
는 4단계(`step4_layout`)와 5단계(`step5_measure`)가 만들고 `set_state(위치 수, 완료
목록, 현재)` 로 다시 그린다. 두 화면 모두 `arr_points(n)`·`ARRANGEMENTS` 를 같이 쓴다.
상태를 들고 있지 않고 받은 값만 그린다.

좌표·치수·문구는 목업 그대로다. 측정 위치 좌표계는 단위 cm, 원점은 청취자
머리(MLP), x 는 오른쪽 +, y 는 앞(스크린) +, h 는 귀 높이 기준 위(+1)/아래(-1).
"""
from __future__ import annotations

from dataclasses import dataclass

from PyQt6 import QtCore, QtGui
from secs.i18n import translate as _translate
from secs.wizard import qt as QtWidgets

from secs.model import role_code, role_kind
from secs.wizard.style import COLORS


# ──────────────────────────── 측정 위치 ────────────────────────────
@dataclass(frozen=True)
class MicPoint:
    """마이크 위치 하나(cm 단위, 목업 arrPoints 의 {x,y,h,t})."""

    x: float
    y: float
    h: int
    text: str


@dataclass(frozen=True)
class Arrangement:
    """배치 선택지(목업 ARR) — 위치 수와 버튼에 쓰는 이름."""

    count: int
    name: str


#: 목업 ARR. 4단계의 배치 버튼과 5단계의 위치 수 판정이 이 표를 쓴다.
#: 1 위치(청취 위치 하나)는 2026-09-24 사용자 결정 — 샘플 데이터가 실측 1점뿐이라 샘플 모드의
#: 기본이고, 실제 사용자도 빠른 1점 측정에 쓴다(좌석 편차가 없어 7단계 최적화 목표는 무의미).
ARRANGEMENTS: dict[int, Arrangement] = {
    1: Arrangement(1, '단일 · 청취 위치 1점'),
    9: Arrangement(9, '싱글 · 1인 의자'),
    13: Arrangement(13, '커플 · 2~3인 소파'),
    17: Arrangement(17, '패밀리 · 소파 두 줄'),       # 10-11 이름: 집중 → 싱글, 포커스 → 커플, 와이드 → 패밀리(사용자). 옛 '넓은 소파/두 줄' — 카드 4장이 된 1024 폭에서 잘려 줄임(09-24)
}

_RING1 = (
    (35, 0, 0, '우 35'), (-35, 0, 0, '좌 35'), (0, 35, 0, '앞 35'), (0, -30, 0, '뒤 30'),
    (25, 25, 1, '앞우 · 위 20'), (-25, 25, 1, '앞좌 · 위 20'),
    (25, -20, -1, '뒤우 · 아래 20'), (-25, -20, -1, '뒤좌 · 아래 20'),
)
_RING2 = (
    (70, 0, 0, '우 70 (소파 끝)'), (-70, 0, 0, '좌 70 (소파 끝)'),
    (0, 70, 1, '앞 70 · 위 20'), (0, -55, 0, '뒤 55 (소파 밖)'),
)
_RING3 = (
    (55, 50, 0, '앞우 55'), (-55, 50, 0, '앞좌 55'),
    (55, -50, -1, '뒤우 55 · 아래'), (-55, -50, -1, '뒤좌 55 · 아래'),
)


def arr_points(n: int) -> list[MicPoint]:
    """목업 arrPoints(n) 과 같은 좌표 목록."""
    points = [MicPoint(0.0, 0.0, 0, '청취 위치 · 귀 높이 (레퍼런스)')]
    for x, y, h, text in _RING1:
        points.append(MicPoint(float(x), float(y), int(h), text))
    if n >= 13:
        for x, y, h, text in _RING2:
            points.append(MicPoint(float(x), float(y), int(h), text))
    if n >= 17:
        for x, y, h, text in _RING3:
            points.append(MicPoint(float(x), float(y), int(h), text))
    return points[:n]


#: 목업 roomSVG() 의 pos — 역할 → 평면도 안 상대 좌표(0~1).
from secs.model import ROLE_POSITIONS as ROOM_POSITIONS  # noqa: E402 - 엔진과 같은 표를 쓴다(09-22)


# ──────────────────────────── 공통 그리기 도우미 ────────────────────────────
def _pen(color: str, width: float = 1.0, dash: tuple[float, ...] | None = None) -> QtGui.QPen:
    pen = QtGui.QPen(QtGui.QColor(color))
    pen.setWidthF(width)
    pen.setCosmetic(True)
    if dash:
        pen.setStyle(QtCore.Qt.PenStyle.CustomDashLine)
        pen.setDashPattern(list(dash))
    return pen


_FONTS: dict[tuple[float, bool], QtGui.QFont] = {}


def _font(size: float, mono: bool = True) -> QtGui.QFont:
    """글꼴은 (크기, 모노) 마다 하나만 만들어 다시 쓴다 — 매번 만들면 글꼴 찾기가 마커 130개에 0.4 s 걸렸다(09-22)."""
    key = (float(size), bool(mono))
    font = _FONTS.get(key)
    if font is None:
        font = QtGui.QFont('Consolas' if mono else 'Segoe UI')
        font.setPointSizeF(size)
        font.setStyleHint(QtGui.QFont.StyleHint.Monospace if mono else QtGui.QFont.StyleHint.SansSerif)
        _FONTS[key] = font
    return font


class _BaseView(QtWidgets.QGraphicsView):
    """배경·테두리·안티에일리어싱이 목업과 같은 고정 크기 캔버스."""

    def __init__(self, width: int, height: int, parent=None) -> None:
        super().__init__(parent)
        self._scene = QtWidgets.QGraphicsScene(0, 0, width, height, self)
        # 마커 몇십 개짜리 고정 그림이라 공간 색인이 필요 없다. BSP 색인은 글자 하나 넣을 때마다
        # 다시 만들어져 2단계 배치도가 0.4 s 걸렸다(09-22 프로파일).
        self._scene.setItemIndexMethod(QtWidgets.QGraphicsScene.ItemIndexMethod.NoIndex)
        self.setScene(self._scene)
        self.setFixedSize(width + 2, height + 2)
        self._view_scale = 1.0
        self.setRenderHints(QtGui.QPainter.RenderHint.Antialiasing
                            | QtGui.QPainter.RenderHint.TextAntialiasing)
        self.setHorizontalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setVerticalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        self.setStyleSheet(
            f'background:{COLORS["paper"]};border:1px solid {COLORS["line"]};'
            f'border-radius:{COLORS["radius"]};'
        )
        self.setBackgroundBrush(QtGui.QBrush(QtGui.QColor(COLORS['paper'])))

    def _text(self, text: str, x: float, y: float, size: float, color: str,
              align: str = 'center', mono: bool = True) -> QtWidgets.QGraphicsSimpleTextItem:
        item = self._scene.addSimpleText(_translate(text), _font(size, mono))
        item.setBrush(QtGui.QBrush(QtGui.QColor(color)))
        rect = item.boundingRect()
        dx = {'center': rect.width() / 2.0, 'left': 0.0, 'right': rect.width()}[align]
        item.setPos(x - dx, y - rect.height() / 2.0)
        return item


# ──────────────────────────── 2단계 스피커 평면도 ────────────────────────────
class RoomLayoutView(_BaseView):
    """목업 roomSVG(): 스크린·청취 위치·역할별 스피커 마커. 클릭하면 선택된다."""

    speaker_clicked = QtCore.pyqtSignal(int)

    W = 340
    H = 340

    def __init__(self, parent=None) -> None:
        super().__init__(self.W, self.H, parent)
        self._hit: list[tuple[QtCore.QRectF, int]] = []
        self.set_speakers([], -1)

    def set_speakers(self, speakers, selected: int = -1) -> None:
        """speakers: `secs.model.Speaker` 목록. selected 는 강조할 인덱스."""
        scene, W, H = self._scene, self.W, self.H
        scene.clear()
        self._hit = []
        scene.setBackgroundBrush(QtGui.QBrush(QtGui.QColor(COLORS['paper'])))
        scene.addRect(0, 0, W, H, _pen(COLORS['paper']), QtGui.QBrush(QtGui.QColor(COLORS['paper'])))
        # 스크린
        scene.addRect(W * .22, 10, W * .56, 7, _pen('#4f6462'), QtGui.QBrush(QtGui.QColor('#4f6462')))
        self._text('스크린', W / 2, 26, 7.5, COLORS['ink3'], mono=False)
        # 청취 위치(소파)
        soft = QtGui.QBrush(QtGui.QColor(COLORS['panel']))
        scene.addRect(W * .36, H * .62, W * .28, H * .11, _pen('#8fa3a1'), soft)
        scene.addRect(W * .38, H * .61, W * .24, H * .05, _pen('#8fa3a1'),
                      QtGui.QBrush(QtGui.QColor('#dfe6e4')))
        self._text('청취 위치', W / 2, H * .70, 7.5, COLORS['ink2'], mono=False)

        unknown: list[int] = []
        for index, speaker in enumerate(speakers):
            place = ROOM_POSITIONS.get(speaker.role)
            if place is None:
                unknown.append(index)
                continue
            cx, cy = place[0] * W, place[1] * H
            kind = role_kind(speaker.role)
            if index == selected:
                scene.addEllipse(cx - 17, cy - 17, 34, 34, _pen('transparent'),
                                 QtGui.QBrush(QtGui.QColor(30, 140, 134, 64)))
            if kind in ('top', 'height'):                # 위쪽 스피커(천장 탑 · 벽 위 하이트, 10-10 에 종류를 나눔)는 점선
                scene.addEllipse(cx - 10, cy - 10, 20, 20,
                                 _pen(COLORS['chassis'], 1.5, (3, 2)),
                                 QtGui.QBrush(QtGui.QColor(COLORS['panel'])))
            elif kind == 'sub':
                scene.addRect(cx - 11, cy - 11, 22, 22, _pen(COLORS['sub']),
                              QtGui.QBrush(QtGui.QColor(COLORS['sub'])))
            else:
                scene.addEllipse(cx - 10, cy - 10, 20, 20, _pen(COLORS['chassis']),
                                 QtGui.QBrush(QtGui.QColor(COLORS['chassis'])))
            self._text(role_code(speaker.role), cx, cy + 20, 7.0, COLORS['ink'])
            outs = '/'.join(str(b.out) for b in speaker.bands)
            self._text(outs, cx, cy + 30, 5.8, COLORS['ink2'])
            self._hit.append((QtCore.QRectF(cx - 13, cy - 13, 26, 26), index))

        for slot, index in enumerate(unknown):
            cx, cy = 30 + slot * 34, H - 22
            scene.addEllipse(cx - 10, cy - 10, 20, 20, _pen(COLORS['ink3']),
                             QtGui.QBrush(QtGui.QColor(COLORS['ink3'])))
            self._text(str(speakers[index].bands[0].out if speakers[index].bands else '?'),
                       cx, cy, 5.5, '#ffffff')
            self._hit.append((QtCore.QRectF(cx - 12, cy - 12, 24, 24), index))
        if unknown:
            self._text('역할 미지정', 30, H - 44, 6.5, COLORS['ink3'], align='left', mono=False)

    def mousePressEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        point = self.mapToScene(event.pos())
        for rect, index in self._hit:
            if rect.contains(point):
                self.speaker_clicked.emit(index)
                return
        super().mousePressEvent(event)


# ──────────────────────────── 4·5단계 측정 위치 배치도 ────────────────────────────
def _set_view_scale(view: '_BaseView', scale: float) -> None:
    """캔버스를 `scale` 배로 줄여 보인다(그림 좌표 · 클릭 위치는 그대로). 5단계가 낮은 창에서 쓴다(10-10)."""
    scale = max(0.3, min(1.0, float(scale)))
    if abs(scale - view._view_scale) < 0.01:
        return
    view._view_scale = scale
    rect = view.scene().sceneRect()
    view.setFixedSize(int(rect.width() * scale) + 2, int(rect.height() * scale) + 2)
    view.setTransform(QtGui.QTransform.fromScale(scale, scale))


_BaseView.set_view_scale = _set_view_scale


class ArrayLayoutView(_BaseView):
    """목업 arrSVG(): 측정 위치 마커 · 소파 실루엣 · 축척 · 범례.

    완료한 위치는 초록(--ok), 다음(또는 고른) 측정 위치는 액센트 테두리 고리로 강조한다.

    5단계는 `set_selectable(True)` 로 켜서 마커를 누르거나(손 모양 커서) 배치도에 초점을 두고
    ←/→(↑/↓) 로 위치를 고른다 — `position_clicked(0-기준 번호)` 를 낸다(2026-09-23 사용자 지시).
    4단계는 보기 전용(기본 꺼짐)이라 눌러도 아무 일이 없다.
    """

    #: 마커를 눌렀거나 화살표로 옮겼다(0-기준 위치 번호). `set_selectable(True)` 일 때만.
    position_clicked = QtCore.pyqtSignal(int)

    W = 360
    H = 300
    K = 1.55  # 1 cm = 1.55 px (목업과 동일)

    def __init__(self, parent=None) -> None:
        super().__init__(self.W, self.H, parent)
        self._count = 9
        self._done: list[int] = []
        self._current = -1
        self._selectable = False
        self._hits: list[tuple[QtCore.QRectF, int]] = []
        self.set_state(9, [], -1)

    # ── 고르기(5단계) ──
    def set_selectable(self, on: bool) -> None:
        self._selectable = bool(on)
        self.setMouseTracking(self._selectable)
        self.viewport().setMouseTracking(self._selectable)
        self.setFocusPolicy(QtCore.Qt.FocusPolicy.StrongFocus if self._selectable
                            else QtCore.Qt.FocusPolicy.NoFocus)

    def marker_at(self, pos: QtCore.QPoint) -> int:
        """뷰 좌표의 마커 번호(0-기준). 없으면 -1."""
        point = self.mapToScene(pos)
        for rect, index in self._hits:
            if rect.contains(point):
                return index
        return -1

    def marker_center(self, index: int) -> QtCore.QPoint:
        """마커 가운데의 뷰 좌표(테스트가 누를 자리)."""
        for rect, number in self._hits:
            if number == index:
                return self.mapFromScene(rect.center())
        return QtCore.QPoint(-1, -1)

    def mousePressEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        if self._selectable:
            index = self.marker_at(event.pos())
            if index >= 0:
                self.setFocus()
                self.position_clicked.emit(index)
                return
        super().mousePressEvent(event)

    def mouseMoveEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        if self._selectable:
            over = self.marker_at(event.pos()) >= 0
            self.viewport().setCursor(QtCore.Qt.CursorShape.PointingHandCursor if over
                                      else QtCore.Qt.CursorShape.ArrowCursor)
        super().mouseMoveEvent(event)

    def keyPressEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        Key = QtCore.Qt.Key
        step = {Key.Key_Left: -1, Key.Key_Up: -1, Key.Key_Right: 1, Key.Key_Down: 1}.get(event.key(), 0)
        if self._selectable and step:
            start = self._current if self._current >= 0 else (0 if step > 0 else self._count - 1)
            target = start + step if self._current >= 0 else start
            if 0 <= target < self._count:
                self.position_clicked.emit(target)
            return
        super().keyPressEvent(event)

    def _x(self, cm: float) -> float:
        return self.W / 2.0 + cm * self.K

    def _y(self, cm: float) -> float:
        return self.H * .56 - cm * self.K

    def set_state(self, count: int, done=(), current: int = -1) -> None:
        self._count, self._done, self._current = int(count), list(done), int(current)
        self._redraw()

    def _redraw(self) -> None:
        scene, W, H, k = self._scene, self.W, self.H, self.K
        scene.clear()
        self._hits = []
        cx, cy = W / 2.0, H * .56
        scene.addRect(0, 0, W, H, _pen(COLORS['paper']),
                      QtGui.QBrush(QtGui.QColor(COLORS['paper'])))
        scene.addRect(W * .2, 8, W * .6, 6, _pen('#4f6462'), QtGui.QBrush(QtGui.QColor('#4f6462')))
        self._text('스크린 · 프론트 스피커 방향', cx, 24, 7.0, COLORS['ink3'], mono=False)
        # 지름 1 m 안내 원
        scene.addEllipse(cx - 50 * k, cy - 50 * k, 100 * k, 100 * k,
                         _pen('#8fa3a1', 1.0, (3, 3)), QtGui.QBrush(QtCore.Qt.BrushStyle.NoBrush))
        self._text('지름 1 m', cx + 50 * k + 26, cy - 6, 6.0, COLORS['ink3'], mono=False)
        # 의자/소파
        pale = QtGui.QBrush(QtGui.QColor('#eef2f1'))
        dark = QtGui.QBrush(QtGui.QColor('#dfe6e4'))
        if self._count >= 13:
            scene.addRect(self._x(-95), self._y(28), 190 * k, 56 * k, _pen('#8fa3a1', 1.2), pale)
            scene.addRect(self._x(-95), self._y(-16), 190 * k, 12 * k, _pen('#8fa3a1', 1.2), dark)
            scene.addRect(self._x(-95), self._y(28), 14 * k, 56 * k, _pen('#8fa3a1', 1.2), dark)
            scene.addRect(self._x(81), self._y(28), 14 * k, 56 * k, _pen('#8fa3a1', 1.2), dark)
        else:
            scene.addRect(self._x(-30), self._y(28), 60 * k, 56 * k, _pen('#8fa3a1', 1.2), pale)
            scene.addRect(self._x(-30), self._y(-16), 60 * k, 12 * k, _pen('#8fa3a1', 1.2), dark)
        # 머리
        scene.addEllipse(cx - 9 * k, cy - 9 * k, 18 * k, 18 * k, _pen('#8fa3a1'),
                         QtGui.QBrush(QtGui.QColor('#c9d5d3')))
        # 축척 30 cm
        scene.addLine(W - 18 - 30 * k, H - 16, W - 18, H - 16, _pen(COLORS['ink2'], 1.5))
        scene.addLine(W - 18 - 30 * k, H - 20, W - 18 - 30 * k, H - 12, _pen(COLORS['ink2'], 1.5))
        scene.addLine(W - 18, H - 20, W - 18, H - 12, _pen(COLORS['ink2'], 1.5))
        self._text('30 cm', W - 18 - 15 * k, H - 24, 6.0, COLORS['ink2'])
        # 마커
        for index, point in enumerate(arr_points(self._count)):
            x, y = self._x(point.x), self._y(point.y)
            is_done = index in self._done
            is_current = index == self._current
            radius = 9.0 if index == 0 else 7.5
            fill = COLORS['ok'] if is_done else COLORS['accent'] if is_current else '#ffffff'
            if is_current:
                scene.addEllipse(x - radius - 5, y - radius - 5, (radius + 5) * 2, (radius + 5) * 2,
                                 _pen(COLORS['accent'], 2.0) if is_done else _pen('transparent'),
                                 QtGui.QBrush(QtGui.QColor(30, 140, 134, 64)))
            self._hits.append((QtCore.QRectF(x - radius - 4, y - radius - 4, (radius + 4) * 2,
                                             (radius + 4) * 2), index))
            marker = scene.addEllipse(x - radius, y - radius, radius * 2, radius * 2,
                                      _pen(COLORS['chassis'], 2.0 if index == 0 else 1.2),
                                      QtGui.QBrush(QtGui.QColor(fill)))
            marker.setToolTip(_translate(f'{index + 1} · {point.text}'
                                         + (' · 완료' if is_done and not is_current else ' · 다시 측정' if is_done
                                            else ' · 다음 측정' if is_current else '')))
            if point.h != 0:
                half = QtGui.QPainterPath()
                rect = QtCore.QRectF(x - radius, y - radius, radius * 2, radius * 2)
                half.moveTo(x - radius, y)
                half.arcTo(rect, 180, -180 if point.h == 1 else 180)
                half.closeSubpath()
                tone = QtGui.QColor(255, 255, 255, 90) if (is_done or is_current) \
                    else QtGui.QColor(COLORS['chassis'])
                scene.addPath(half, _pen('transparent'), QtGui.QBrush(tone))
            # 목업은 흰 마커 위 숫자에 외곽선을 준다. Qt 에서는 대비되는 잉크로 대신한다.
            ink = '#ffffff' if (is_done or is_current) else COLORS['ink']
            self._text(str(index + 1), x, y, 5.8, ink)
        # 범례
        for offset, (label, mark) in enumerate((('귀 높이', 0), ('+20 cm 위', 1), ('−20 cm 아래', -1))):
            y = H - 42 + offset * 14
            scene.addEllipse(9, y - 5, 10, 10, _pen(COLORS['chassis']),
                             QtGui.QBrush(QtGui.QColor('#ffffff')))
            if mark != 0:
                path = QtGui.QPainterPath()
                rect = QtCore.QRectF(9, y - 5, 10, 10)
                path.moveTo(9, y)
                path.arcTo(rect, 180, -180 if mark == 1 else 180)
                path.closeSubpath()
                scene.addPath(path, _pen('transparent'),
                              QtGui.QBrush(QtGui.QColor(COLORS['chassis'])))
            self._text(label, 24, y, 6.5, COLORS['ink2'], align='left', mono=False)


__all__ = ['MicPoint', 'Arrangement', 'ARRANGEMENTS', 'arr_points', 'ROOM_POSITIONS',
           'RoomLayoutView', 'ArrayLayoutView']
