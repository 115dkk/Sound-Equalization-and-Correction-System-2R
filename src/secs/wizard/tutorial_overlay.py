# -*- coding: utf-8 -*-
"""튜토리얼 오버레이(2026-10-09 사용자: 게임식 튜토리얼 — 목업 `UI 목업/2026-10-09 게임식 튜토리얼` 그대로의 동작을 실제 화면 위에).

- `Overlay`: 창 전체를 덮는 아주 옅은 막. 강조할 위젯 자리는 구멍(마스크)이라 그 위젯은 그대로 눌린다. 구멍 밖을 누르면
  막이 받아 안내 카드가 흔들린다. 둘러보기는 흰 테두리를 깐 굵은 점선뿐(꼬리표 없음 — 10-10 사용자), 눌러 보기는 분홍 실선 + 깜빡이는 고리 +
  분홍 꼬리표 '눌러 볼까요?'. 번호(k / n)는 안내 카드에만 — 꼬리표에도 따로 세면 둘이 어긋나 헷갈렸다(10-10 AI 시험).
- `Coach`: 안내 카드(단계 · k / n, 문구, 처음 나온 용어 풀이, [왜?], [← 이전으로] · [계속 →]). 눌러 볼 차례의
  [계속 →] 은 흐리게 — 카드 아래 규칙 줄이 '두 번 누르면 예시로 채우고 넘어간다' 고 알리고, 처음 누르면 그 줄이 분홍으로
  바뀌며 카드가 흔들린다. 한 번 더 누르면 결과를 대신 채우고(무엇을 했는지 알림) 넘어간다. 오래 걸리는 동작(레벨 순차 재생 ·
  계산)은 끝날 때까지 '진행 중' 문구로 기다린다. 마지막 카드의 버튼은 [튜토리얼 끝내기].
  이미 끝낸 퀘스트([← 이전으로] 로 돌아온 것)는 다시 시키지 않는다 — [계속 →] 한 번에 넘어간다.
  (카드 버튼 이름은 10-10 3차 시험 사용자 결정 3A 로 '다음으로 →' → '계속 →' — 아래 내비 [다음 →] 과 헷갈렸다.)
- `QuestPanel`: 오른쪽 위 튜토리얼 목록(접으면 알약 '튜토리얼 · <단계> k / n ▾' — '퀘스트' 낱말은 3차 시험 결정 1A 로 뺐다).
- `Tutorial`: 퀘스트 진행(`tutorial_quests.QUESTS`). 창(`WizardWindow`)이 튜토리얼 모드에서 만든다.
- 잠시 치우기(10-11 사용자: "튜토리얼 잠시 치우고 다른 기능들 막 만질 수 있게", "아이콘으로"): 안내 카드 오른쪽 위 아이콘
  (`eye_icon` — 빗금 친 눈)을 누르면 막 · 카드 · 목록 · 알림이 모두 사라지고(`Tutorial.pause`) 신호 · 폴링도 멈춘다. 진행바
  오른쪽 끝(창 오른쪽 위 구석, 아무것도 덮지 않는 자리)에 [튜토리얼 이어 하기](뜬 눈)가 남고, 누르면 멈춘 퀘스트로 돌아간다
  (`Tutorial.resume` — 앞 단계가 막혔으면 그 단계 퀘스트부터).
"""
from __future__ import annotations

from PyQt6 import QtCore, QtGui, QtWidgets

from secs.wizard import tooltips
from secs.wizard.strings import FLOW, STEPS, t
from secs.wizard.style import COLORS, TRIAL_COLORS
from secs.wizard.tutorial_quests import QUESTS, SCREENS, glossary_for

HOT = QtGui.QColor('#d6338a')
INK = QtGui.QColor('#1b2a29')
PAD = 6


def _area(r: QtCore.QRect) -> int:
    return max(0, r.width()) * max(0, r.height())


def _dark(widget) -> bool:
    return widget.palette().color(QtGui.QPalette.ColorRole.Window).lightness() < 110


NAV_H = 84            # 아래 내비 막대 높이 — 안내 카드 · 퀘스트 창이 덮지 않는다


def eye_icon(color: str, hidden: bool = False, size: int = 18) -> QtGui.QIcon:
    """눈 아이콘(QPainter, 이모지 · 글자 없이) — hidden=True 면 빗금 친 눈(잠시 치우기 = 튜토리얼을 안 보이게), False 면 뜬 눈
    (이어 하기 = 다시 보이게). 숨기기 · 보이기에 널리 쓰는 짝이라 처음 보는 사람도 읽고, 카드에서 누른 눈이 진행바 끝에서
    떠 있는 것으로 이어진다(10-11). 버린 모양: `×` 는 닫기(튜토리얼 끝)로, `‖` 는 소리 재생 멈춤으로, `–` 는 오른쪽 위
    목록 접기로 읽힌다. 구석 꺾쇠로 들어가는 화살표는 첫 캡처에서 '새 창으로 열기' 아이콘처럼 보였다.
    고해상도 화면용으로 1 · 1.5 · 2 · 3 배 픽스맵을 넣는다."""
    icon = QtGui.QIcon()
    for scale in (1.0, 1.5, 2.0, 3.0):
        side = int(round(size * scale))
        pixmap = QtGui.QPixmap(side, side)
        pixmap.fill(QtCore.Qt.GlobalColor.transparent)
        p = QtGui.QPainter(pixmap)
        p.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing)
        width = 1.7 * scale
        p.setPen(QtGui.QPen(QtGui.QColor(color), width, QtCore.Qt.PenStyle.SolidLine,
                            QtCore.Qt.PenCapStyle.RoundCap, QtCore.Qt.PenJoinStyle.RoundJoin))
        s = float(side)

        def pt(x: float, y: float) -> QtCore.QPointF:
            return QtCore.QPointF(x * s, y * s)

        lids = QtGui.QPainterPath(pt(0.06, 0.50))                     # 아몬드 모양 눈꺼풀
        lids.cubicTo(pt(0.28, 0.16), pt(0.72, 0.16), pt(0.94, 0.50))
        lids.cubicTo(pt(0.72, 0.84), pt(0.28, 0.84), pt(0.06, 0.50))
        p.drawPath(lids)
        p.setBrush(QtGui.QColor(color))
        p.drawEllipse(pt(0.50, 0.50), 0.13 * s, 0.13 * s)             # 눈동자
        if hidden:
            # 빗금 양옆을 투명하게 지워 어느 바탕(카드 · 눌림 색)에서도 빗금이 눈꺼풀과 갈라져 보이게
            p.setCompositionMode(QtGui.QPainter.CompositionMode.CompositionMode_Clear)
            p.setPen(QtGui.QPen(QtCore.Qt.GlobalColor.black, width * 3.2, QtCore.Qt.PenStyle.SolidLine,
                                QtCore.Qt.PenCapStyle.RoundCap))         # Clear 는 색과 상관없이 지운다
            p.drawLine(pt(0.16, 0.12), pt(0.84, 0.88))
            p.setCompositionMode(QtGui.QPainter.CompositionMode.CompositionMode_SourceOver)
            p.setPen(QtGui.QPen(QtGui.QColor(color), width, QtCore.Qt.PenStyle.SolidLine,
                                QtCore.Qt.PenCapStyle.RoundCap))
            p.drawLine(pt(0.16, 0.12), pt(0.84, 0.88))
        p.end()
        icon.addPixmap(pixmap)
    return icon


class Overlay(QtWidgets.QWidget):
    outside = QtCore.pyqtSignal()

    def __init__(self, parent):
        super().__init__(parent)
        self.hole = QtCore.QRect()
        self.look = True
        self.label = ''
        self.phase = 0.0
        self._pulse = QtCore.QTimer(self)
        self._pulse.timeout.connect(self._tick)
        self._pulse.start(60)

    def _tick(self):
        self.phase = (self.phase + 0.06) % 1.0
        if not self.look:
            self.update()

    def set_target(self, rect: QtCore.QRect, look: bool, label: str, exits=(), label_at=None, parts=()) -> None:
        """`rect` = 강조 칸(창 좌표). `parts` 를 주면 그 칸들을 따로따로 테두리친다(떨어진 위젯 둘 — 10-10 3차 시험:
        보정 설계 탭 줄과 오른쪽 카드를 한 칸으로 합치면 그 사이 그래프 · 모드 카드까지 강조됐다)."""
        self.setGeometry(self.parentWidget().rect())
        self.hole, self.look, self.label = rect.adjusted(-PAD, -PAD, PAD, PAD), look, label
        self.parts = [r.adjusted(-PAD, -PAD, PAD, PAD) for r in parts if not r.isNull()]
        self._label_at = label_at
        region = QtGui.QRegion(self.rect())
        # 눌러 보기만 구멍을 뚫는다 — 둘러보기 칸은 보기만(2차 시험 t08: 둘러보기 칸 안에서 스피커 삭제 · 순차 재생 ·
        # 파일 창이 그대로 눌려 튜토리얼 상태가 바뀌었다). 휠은 막이 아래로 넘긴다(wheelEvent).
        if not self.hole.isNull() and not look:
            for r in self.rings():
                region = region.subtracted(QtGui.QRegion(r))
        for r in exits:                         # 늘 눌리는 탈출구(튜토리얼 끝내기 · 다시)
            region = region.subtracted(QtGui.QRegion(r))
        self.setMask(region)
        self.update()

    def rings(self) -> list[QtCore.QRect]:
        """테두리를 칠 칸들 — 따로 칠 칸(parts)이 있으면 그것, 없으면 강조 칸 하나."""
        if self.hole.isNull():
            return []
        return list(getattr(self, 'parts', None) or [self.hole])

    def paintEvent(self, _event):  # noqa: N802 - Qt 이름
        p = QtGui.QPainter(self)
        p.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing)
        dim = QtGui.QRegion(self.rect())
        for hole in self.rings():                        # 강조 칸은 어둡게 덮지 않는다(둘러보기는 막이 덮어도 맑게)
            dim = dim.subtracted(QtGui.QRegion(hole))
        p.setClipRegion(dim)
        # 바깥은 조금 더 어둡게(10-10 사용자: 강조 칸이 잘 안 보였다) — 밝은 테마 22 → 35 %, 어두운 테마 22 → 38 %
        p.fillRect(self.rect(), QtGui.QColor(0, 0, 0, 96 if _dark(self) else 90))
        p.setClipping(False)
        if self.hole.isNull():
            return
        for hole in self.rings():
            self._outline(p, hole)
        if self.label:
            self._paint_label(p)

    def _outline(self, p: QtGui.QPainter, hole: QtCore.QRect) -> None:
        r = QtCore.QRectF(hole).adjusted(-1.5, -1.5, 1.5, 1.5).intersected(
            QtCore.QRectF(self.rect()).adjusted(3, 3, -3, -3))
        if self.look:
            # 둘러보기 점선: 흰 바탕 줄(5 px) 위에 진한 점선(3 px) — 밝은 칸 · 어두운 칸 어디서나 보이게(10-10 사용자:
            # 2 px 검은 점선은 가시성이 나빴다)
            p.setPen(QtGui.QPen(QtGui.QColor(255, 255, 255, 230), 5))
            p.drawRoundedRect(r, 6, 6)
            pen = QtGui.QPen(INK, 3, QtCore.Qt.PenStyle.CustomDashLine)
            pen.setDashPattern([3, 2])
        else:
            ring = QtGui.QColor(HOT)
            ring.setAlphaF(max(0.0, 0.6 * (1.0 - self.phase)))
            grow = 2 + 8 * self.phase
            p.setPen(QtGui.QPen(ring, 2))
            p.drawRoundedRect(r.adjusted(-grow, -grow, grow, grow), 8, 8)
            pen = QtGui.QPen(HOT, 3)
        p.setPen(pen)
        p.drawRoundedRect(r, 6, 6)

    def _paint_label(self, p: QtGui.QPainter) -> None:
        fill = INK if self.look else HOT
        p.setFont(self._label_font())
        box = QtCore.QRectF(self.label_rect())
        p.setPen(QtGui.QPen(QtGui.QColor('#ffffff'), 1))
        p.setBrush(fill)
        p.drawRoundedRect(box, 4, 4)
        p.setPen(QtGui.QColor('#ffffff'))
        p.drawText(box, QtCore.Qt.AlignmentFlag.AlignCenter, self.label)

    def _label_font(self) -> QtGui.QFont:
        font = QtGui.QFont(self.font())
        font.setBold(True)
        font.setPixelSize(12)
        return font

    def label_size(self, text: str | None = None) -> QtCore.QSize:
        m = QtGui.QFontMetrics(self._label_font())
        text = self.label if text is None else text
        return QtCore.QSize(m.horizontalAdvance(text) + 16, m.height() + 8)

    def label_rect(self) -> QtCore.QRect:
        """꼬리표 자리(창 좌표) — 안내 카드가 꼬리표를 덮지 않게 `Tutorial._place` 도 쓴다. 꼬리표가 없으면 빈 사각형."""
        if not self.label or self.hole.isNull():
            return QtCore.QRect()
        size = self.label_size()
        if getattr(self, '_label_at', None) is not None:          # Tutorial 이 이웃을 피해 고른 자리(2차 시험)
            return QtCore.QRect(self._label_at, size)
        r = QtCore.QRectF(self.hole).adjusted(-1.5, -1.5, 1.5, 1.5)
        w, h = size.width(), size.height()
        if r.top() - h - 4 > 0:                 # 위 오른쪽 끝(2차 시험: 왼쪽에 붙이면 화면 제목을 가렸다)
            y, x = r.top() - h - 4, max(r.left(), r.right() - w)
        elif r.bottom() + h + 4 < self.height():
            y, x = r.bottom() + 4, r.left()
        else:                                   # 위 · 아래 자리가 없으면(사이드바처럼 긴 것) 오른쪽 옆(구멍 안은 마스크라 안 그려진다)
            y, x = r.top() + 12, r.right() + 8
        return QtCore.QRect(int(x), int(y), int(w), int(h))

    def mousePressEvent(self, event):  # noqa: N802 - Qt 이름
        event.accept()
        self.outside.emit()

    def wheelEvent(self, event):  # noqa: N802 - 스크롤은 막지 않는다
        """휠은 막 아래 위젯으로 넘긴다 — ignore() 는 부모 창으로만 가서 튜토리얼 중엔 스크롤이 안 됐다(2차 시험 t07:
        측정 그래프 아래 축을 못 봤다)."""
        parent = self.parentWidget()
        pos = event.position().toPoint()
        self.setAttribute(QtCore.Qt.WidgetAttribute.WA_TransparentForMouseEvents, True)
        try:
            under = parent.childAt(self.mapTo(parent, pos))
        finally:
            self.setAttribute(QtCore.Qt.WidgetAttribute.WA_TransparentForMouseEvents, False)
        if under is None or under is self:
            event.ignore()
            return
        # 만든 휠 이벤트는 Qt 가 부모로 올려 주지 않는다(spontaneous 가 아니라서) — 받을 위젯이 나올 때까지 직접 올린다
        w = under
        while w is not None and w is not parent:
            local = w.mapFrom(self, pos)
            forwarded = QtGui.QWheelEvent(QtCore.QPointF(local), event.globalPosition(), event.pixelDelta(),
                                          event.angleDelta(), event.buttons(), event.modifiers(), event.phase(),
                                          event.inverted())
            forwarded.setAccepted(False)
            QtWidgets.QApplication.sendEvent(w, forwarded)
            if forwarded.isAccepted():
                break
            w = w.parentWidget()
        event.accept()


class Coach(QtWidgets.QFrame):
    """안내 카드."""

    # 카드 버튼 이름 '다음으로 →' → '계속 →'(10-10 3차 시험 사용자 결정 3A: 아래 내비 [다음 →] 과 헷갈렸다)
    NEXT = '계속 →'
    HINT = '직접 해 보세요 — [계속 →] 을 두 번 누르면 예시로 채우고 넘어갑니다.'
    HINT_ARMED = '한 번 더 누르면 예시로 채우고 넘어갑니다.'

    def __init__(self, parent):
        super().__init__(parent)
        self.setObjectName('tutorialCoach')
        self.setStyleSheet('#tutorialCoach { background: #f7faf9; border: 1px solid #acbcba; border-left: 4px solid #d6338a;'
                           ' border-radius: 6px; } #tutorialCoach QLabel { color: #1b2a29; background: transparent; }'
                           ' #tutorialCoach QPushButton { padding: 4px 10px; }'
                           ' #tutorialCoach QPushButton[soft="true"] { color: #1b2a29; background: #ffffff;'
                           ' border: 1px dashed #7a8e8c; }')
        self.setFixedWidth(340)
        box = QtWidgets.QVBoxLayout(self)
        box.setContentsMargins(14, 10, 12, 10)
        box.setSpacing(6)
        self.head = QtWidgets.QLabel()
        self.head.setStyleSheet('font-size: 11px; color: #4f6462;')
        # 잠시 치우기 아이콘(10-11 사용자: "아이콘으로") — 머리 줄 오른쪽 끝. 아래 버튼 줄([← 이전으로] · [계속 →]) ·
        # [왜?] 자리는 그대로 두고, 카드 폭(340) 안이라 1024×640 에서도 잘리지 않는다.
        self.pause_button = QtWidgets.QToolButton()
        self.pause_button.setObjectName('tutorialPause')
        self.pause_button.setIcon(eye_icon(COLORS['ink2'], hidden=True))
        self.pause_button.setIconSize(QtCore.QSize(18, 18))
        self.pause_button.setFixedSize(26, 26)
        self.pause_button.setAccessibleName(t('튜토리얼 잠시 치우기'))
        self.pause_button.setStyleSheet(
            f"QToolButton {{ border: 1px solid transparent; border-radius: 4px; background: transparent; }}"
            f" QToolButton:hover {{ background: {COLORS['sel_bg']}; border-color: {COLORS['line']}; }}"
            f" QToolButton:pressed {{ background: {COLORS['paper']}; }}")
        tooltips.attach(self.pause_button, 'win.tutorial_pause')
        top = QtWidgets.QHBoxLayout()
        top.setContentsMargins(0, 0, 0, 0)
        top.setSpacing(6)
        top.addWidget(self.head, 1)
        top.addWidget(self.pause_button, 0, QtCore.Qt.AlignmentFlag.AlignTop)
        self.text = QtWidgets.QLabel()
        self.text.setWordWrap(True)
        self.text.setStyleSheet('font-size: 13px;')
        self.why_button = QtWidgets.QToolButton()
        self.why_button.setText(t('왜?') + ' ▸')
        self.why_button.setCheckable(True)
        self.why_button.setStyleSheet('QToolButton { border: 0; color: #1e8c86; font-size: 12px; font-weight: 700; }')
        self.why = QtWidgets.QLabel()
        self.why.setWordWrap(True)
        self.why.setStyleSheet('font-size: 12px; color: #4f6462;')
        self.why.hide()
        self.why_button.toggled.connect(self._toggle_why)
        # 처음 나온 약어 · 용어 풀이(10-10 AI 시험: XO · FIR · dBFS … 풀이가 없었다)
        self.gloss = QtWidgets.QLabel()
        self.gloss.setWordWrap(True)
        self.gloss.setStyleSheet('font-size: 12px; color: #33494a; border-top: 1px solid #d5e0df; padding-top: 4px;')
        self.gloss.hide()
        # 눌러 보기 규칙(2차 시험: 흐린 [다음으로] 가 고장처럼 보였고, 규칙 알림이 카드에서 먼 위쪽에 떴다)
        self.hint = QtWidgets.QLabel()
        self.hint.setWordWrap(True)
        self.hint.setAlignment(QtCore.Qt.AlignmentFlag.AlignLeft | QtCore.Qt.AlignmentFlag.AlignTop)
        self.hint.hide()
        row = QtWidgets.QHBoxLayout()
        self.prev_button = QtWidgets.QPushButton(t('← 이전으로'))
        self.next_button = QtWidgets.QPushButton(t(self.NEXT))
        row.addWidget(self.prev_button)
        row.addStretch(1)
        row.addWidget(self.next_button)
        # 카드 바탕은 끌기용 '이동' 커서지만 버튼은 손가락(10-10 사용자: 버튼 위 커서가 이상했다)
        for button in (self.prev_button, self.next_button, self.why_button, self.pause_button):
            button.setCursor(QtCore.Qt.CursorShape.PointingHandCursor)
        box.addLayout(top)
        for w in (self.text, self.gloss, self.why_button, self.why, self.hint):
            box.addWidget(w)
        box.addLayout(row)

    moved = False          # 사용자가 끌어 옮겼으면 다음 퀘스트까지 그 자리를 지킨다

    def mousePressEvent(self, event):  # noqa: N802 - Qt 이름
        if event.button() == QtCore.Qt.MouseButton.LeftButton:
            self._drag = event.position().toPoint()
        super().mousePressEvent(event)

    def mouseMoveEvent(self, event):  # noqa: N802 - Qt 이름
        if getattr(self, '_drag', None) is not None:
            parent = self.parentWidget()
            point = self.mapToParent(event.position().toPoint() - self._drag)
            x = min(max(0, point.x()), parent.width() - self.width())
            y = min(max(0, point.y()), parent.height() - self.height())
            self.move(x, y)
            self.moved = True
        super().mouseMoveEvent(event)

    def mouseReleaseEvent(self, event):  # noqa: N802 - Qt 이름
        self._drag = None
        super().mouseReleaseEvent(event)

    def _toggle_why(self, on: bool) -> None:
        self.why.setVisible(on)
        self.why_button.setText(t('왜?') + (' ▾' if on else ' ▸'))
        self.fit()

    def fit(self) -> None:
        """높이를 내용(줄바꿈 포함)에 맞춘다 — adjustSize 는 줄바꿈 라벨 높이를 덜 잡아 영어 [왜?] · 풀이가 잘렸다(2차 시험 t06)."""
        layout = self.layout()
        layout.activate()
        height = layout.totalHeightForWidth(self.width()) if layout.hasHeightForWidth() else layout.totalSizeHint().height()
        self.resize(self.width(), max(height, layout.totalMinimumSize().height()))

    def show_quest(self, head: str, text: str, why: str, soft: bool, gloss=(), why_gloss=(), next_text: str = '') -> None:
        self.moved = False
        self.setCursor(QtCore.Qt.CursorShape.SizeAllCursor)
        self.head.setText(head)
        self.text.setText(text)
        self.gloss.setText('\n'.join(str(g) for g in gloss))
        self.gloss.setVisible(bool(gloss))
        extra = '\n'.join(str(g) for g in why_gloss)
        self.why.setText(f'{why}\n\n{extra}' if why and extra else why)
        self.why_button.setChecked(False)
        self.why_button.setText(t('왜?') + ' ▸')
        self.why_button.setVisible(bool(why))
        self.hint.setMinimumHeight(self._hint_room() if soft else 0)
        self.set_hint(soft, armed=False)
        self.next_button.setText(next_text or t(self.NEXT))
        self.next_button.setProperty('soft', 'true' if soft else 'false')
        self.next_button.style().unpolish(self.next_button)
        self.next_button.style().polish(self.next_button)
        self.fit()

    @staticmethod
    def _hint_style(armed: bool) -> str:
        return ('font-size: 11px; color: #d6338a; font-weight: 700;' if armed
                else 'font-size: 11px; color: #7a8e8c;')

    def _hint_room(self) -> int:
        """규칙 줄 자리 — 두 문구(처음 · 한 번 누른 뒤) 중 높은 쪽. 처음부터 그만큼 잡아 두어 한 번 누른 뒤 카드 높이와
        [계속 →] 자리가 그대로다(10-10 3차 시험: 줄이 바뀌며 버튼이 y 510 → 494 로 움직여 두 번째 클릭이 빗나갔다)."""
        layout = self.layout()
        margins = layout.contentsMargins()
        width = max(50, self.width() - margins.left() - margins.right())
        room = 0
        for armed in (False, True):
            self.hint.setText(t(self.HINT_ARMED if armed else self.HINT))
            self.hint.setStyleSheet(self._hint_style(armed))
            self.hint.ensurePolished()
            room = max(room, self.hint.heightForWidth(width))
        return room

    def set_hint(self, on: bool, armed: bool) -> None:
        """눌러 보기 규칙 줄 — armed(한 번 눌렀음)면 분홍 굵게. 자리는 `_hint_room` 이 미리 잡는다."""
        self.hint.setText(t(self.HINT_ARMED if armed else self.HINT))
        self.hint.setStyleSheet(self._hint_style(armed))
        self.hint.setVisible(bool(on))
        self.fit()

    def set_busy(self, text: str) -> None:
        """오래 걸리는 동작 중 — 본문을 진행 안내로 바꾸고 규칙 줄은 숨긴다."""
        self.text.setText(text)
        self.hint.hide()
        self.fit()

    def shake(self) -> None:
        start = self.pos()
        anim = QtCore.QPropertyAnimation(self, b'pos', self)
        anim.setDuration(220)
        anim.setKeyValueAt(0.0, start)
        anim.setKeyValueAt(0.33, start + QtCore.QPoint(-6, 0))
        anim.setKeyValueAt(0.66, start + QtCore.QPoint(6, 0))
        anim.setKeyValueAt(1.0, start)
        anim.start(QtCore.QAbstractAnimation.DeletionPolicy.DeleteWhenStopped)


class QuestPanel(QtWidgets.QFrame):
    """오른쪽 위 퀘스트 목록 — 접으면 알약."""

    def __init__(self, parent):
        super().__init__(parent)
        self.setObjectName('tutorialQuests')
        self.setStyleSheet('#tutorialQuests { background: #f7faf9; border: 1px solid #acbcba; border-radius: 6px; }'
                           ' #tutorialQuests QLabel { color: #1b2a29; background: transparent; }')
        self.setFixedWidth(240)
        box = QtWidgets.QVBoxLayout(self)
        box.setContentsMargins(12, 8, 8, 10)
        box.setSpacing(4)
        top = QtWidgets.QHBoxLayout()
        self.title = QtWidgets.QLabel()
        self.title.setStyleSheet('font-weight: 700; font-size: 13px;')
        self.fold_button = QtWidgets.QToolButton()
        self.fold_button.setText('–')
        self.fold_button.setToolTip(t('튜토리얼 목록 접기'))
        self.fold_button.setStyleSheet('QToolButton { border: 0; font-size: 16px; color: #4f6462; }')
        top.addWidget(self.title, 1)
        top.addWidget(self.fold_button)
        box.addLayout(top)
        self.bar = QtWidgets.QProgressBar()
        self.bar.setTextVisible(False)
        self.bar.setFixedHeight(5)
        self.bar.setStyleSheet('QProgressBar { background: #d5e0df; border: 0; border-radius: 2px; }'
                               ' QProgressBar::chunk { background: #d6338a; border-radius: 2px; }')
        box.addWidget(self.bar)
        self.items = QtWidgets.QLabel()
        self.items.setWordWrap(True)
        self.items.setTextFormat(QtCore.Qt.TextFormat.RichText)
        self.items.setStyleSheet('font-size: 12px;')
        box.addWidget(self.items)
        self.overall = QtWidgets.QLabel()
        self.overall.setStyleSheet('font-size: 11px; color: #4f6462;')
        box.addWidget(self.overall)
        self.pill = QtWidgets.QPushButton(parent)
        self.pill.setObjectName('tutorialPill')
        self.pill.setStyleSheet('#tutorialPill { background: #f7faf9; color: #1b2a29; border: 1px solid #acbcba;'
                                ' border-radius: 14px; padding: 5px 12px; }')
        self.pill.hide()
        self.fold_button.clicked.connect(lambda: self.set_folded(True))
        self.pill.clicked.connect(self._open_by_user)
        self.folded = False
        self.user_open = False          # 알약을 눌러 직접 연 동안은 조작 위젯 위라도 접지 않는다(화면이 바뀌면 풀림)

    def _open_by_user(self) -> None:
        self.user_open = True
        self.set_folded(False)

    def set_folded(self, on: bool) -> None:
        self.folded = bool(on)
        self.setVisible(not self.folded)
        self.pill.setVisible(self.folded)
        self.pill.adjustSize()
        self.place()

    def place(self) -> None:
        parent = self.parentWidget()
        layout = self.layout()
        layout.activate()          # 줄바꿈 항목 높이까지(2차 시험 t05: 구성 목록 5번째 항목이 잘렸다)
        self.resize(self.width(), layout.totalHeightForWidth(self.width()) if layout.hasHeightForWidth()
                    else layout.totalSizeHint().height())
        bar = getattr(parent, 'steps_bar', None)      # 진행바 아래(진행바를 가리지 않게)
        top = bar.mapTo(parent, QtCore.QPoint(0, bar.height())).y() + 8 if bar is not None else 16
        self.move(parent.width() - self.width() - 16, top)
        self.pill.adjustSize()
        self.pill.move(parent.width() - self.pill.width() - 16, top)
        self.raise_()
        self.pill.raise_()


class Tutorial(QtCore.QObject):
    """퀘스트 진행. `window` = `WizardWindow`(튜토리얼 모드). start() · stop() · next() · prev()."""

    finished = QtCore.pyqtSignal()

    def __init__(self, window):
        super().__init__(window)
        self.win = window
        self.quests = []
        self.index = 0
        self.warned = False
        self.cleared: set[int] = set()
        self.completed: set[int] = set()     # 끝낸 퀘스트 번호 — [← 이전으로] 로 돌아와도 다시 시키지 않는다
        self.active = False
        self._done_flag = False
        self._connected = None
        self._base = None
        self._gloss: dict[int, list[str]] = {}
        self._dir = 1                        # 마지막 이동 방향(건너뛰기도 같은 쪽으로 — 이전으로가 되튕기지 않게)
        self._missing = 0
        self._label = ''
        self.overlay = Overlay(window)
        self.overlay.outside.connect(self._outside)
        self.coach = Coach(window)
        self.panel = QuestPanel(window)
        self.toast = QtWidgets.QLabel(window)
        self.toast.setObjectName('tutorialToast')
        self.toast.setStyleSheet('#tutorialToast { background: #1b2a29; color: #ffffff; border-radius: 6px; padding: 8px 14px;'
                                 ' font-size: 13px; }')
        self.toast.hide()
        self._toast_timer = QtCore.QTimer(self)
        self._toast_timer.setSingleShot(True)
        self._toast_timer.timeout.connect(self.toast.hide)
        # 잠시 치운 동안 남는 [튜토리얼 이어 하기](10-11) — 진행바 줄 오른쪽 끝에 끼워 둔다(숨은 동안은 자리를 차지하지
        # 않는다). 페이지 위에 띄우면 튜토리얼이 스크롤해 둔 표 · 목록을 덮었다(첫 캡처: 구성 표 머리 · 세로 막대) — 진행바
        # 줄은 단계 이름이 조금씩 좁아질 뿐 아무것도 덮지 않는다. 자주색 막대 위 흰 알약 + 분홍 테두리라 처음 보는 사람도 찾는다.
        self.paused = False
        self._paused_at: tuple[int, object] | None = None      # (퀘스트 번호, 그때 찍어 둔 base)
        self.resume_button = QtWidgets.QPushButton(window)
        self.resume_button.setObjectName('tutorialResume')
        self.resume_button.setIcon(eye_icon(TRIAL_COLORS['accent']))
        self.resume_button.setIconSize(QtCore.QSize(18, 18))
        self.resume_button.setFixedHeight(28)
        self.resume_button.setCursor(QtCore.Qt.CursorShape.PointingHandCursor)
        self.resume_button.setStyleSheet(
            f"#tutorialResume {{ background: #ffffff; color: {COLORS['ink']}; border: 2px solid {TRIAL_COLORS['accent']};"
            f" border-radius: 14px; padding: 0px 14px 0px 10px; margin-left: 10px; font-size: 13px; font-weight: 700; }}"
            f" #tutorialResume:hover {{ background: #fbe9f2; }}"
            f" #tutorialResume:pressed {{ background: #f6d3e5; }}")
        tooltips.attach(self.resume_button, 'win.tutorial_resume')
        self.resume_button.hide()
        bar = getattr(window, 'steps_bar', None)
        self._docked = bar is not None and isinstance(bar.layout(), QtWidgets.QBoxLayout)
        if self._docked:
            bar.layout().addWidget(self.resume_button, 0, QtCore.Qt.AlignmentFlag.AlignVCenter)
        self.resume_button.clicked.connect(lambda: self.resume())
        self.coach.pause_button.clicked.connect(lambda: self.pause())
        self.coach.next_button.clicked.connect(self.next_pressed)
        self.coach.prev_button.clicked.connect(self.prev)
        # [왜?] 를 펼치면 카드 높이가 바뀐다 — 다시 자리 잡기(2차 시험: 펼친 카드가 대상을 덮었다)
        # 지금 자리를 먼저 후보로(`prefer`) — 자리가 괜찮으면 아래로 커진다(3차 시험: 위로 커져 탭 · 버튼을 덮었다)
        self.coach.why_button.toggled.connect(lambda _on: QtCore.QTimer.singleShot(0, self._replace_here))
        self._busy_on = False
        self._blocked_on = ''
        self._anim_timer = QtCore.QTimer(self)          # 퀘스트의 animate(화면을 움직여 보여 주기)
        self._anim_timer.setInterval(3600)              # 10-11 사용자: 2배 느리게(1.8 → 3.6 s)
        self._anim_timer.timeout.connect(self._animate)
        self._anim_quest = None
        self._anim_k = 0
        self._poll = QtCore.QTimer(self)
        self._poll.timeout.connect(self._tick)
        window.installEventFilter(self)
        for w in (self.overlay, self.coach, self.panel, self.toast):
            w.hide()

    # ── 진행 ──
    def start(self) -> None:
        self.quests = [q for q in QUESTS if q.when is None or q.when(self.win)]
        self._gloss = glossary_for(self.quests)
        self.index, self.cleared, self.completed, self.active, self._dir = 0, set(), set(), True, 1
        self._unpause()                  # 잠시 치운 채 [튜토리얼 다시] 를 눌러도 이어 하기 버튼이 남지 않게
        self.panel.set_folded(False)
        self._poll.start(150)
        self._show()

    def stop(self) -> None:
        self.active = False
        self._unpause()
        self._end_animation()
        self._poll.stop()
        self._disconnect()
        for w in (self.overlay, self.coach, self.panel, self.panel.pill, self.toast):
            w.hide()

    # ── 잠시 치우기(10-11) ──
    def pause(self) -> None:
        """튜토리얼을 잠시 치운다 — 막 · 카드 · 목록 · 알림을 숨기고 신호 · 폴링 · 화면 보여 주기를 멈춘다. 그동안 창의 모든
        기능(다른 단계 · 설정 · 계산 · 대화상자)을 그대로 쓴다. 구석에 [튜토리얼 이어 하기] 만 남는다."""
        if not self.active or self.paused:
            return
        self.paused = True
        self._paused_at = (self.index, self._base)
        self._poll.stop()
        self._end_animation()
        self._disconnect()
        self._toast_timer.stop()
        for w in (self.overlay, self.coach, self.panel, self.panel.pill, self.toast):
            w.hide()
        self.resume_button.setText(t('튜토리얼 이어 하기'))
        self.resume_button.setAccessibleName(t('튜토리얼 이어 하기'))
        self.resume_button.show()
        self._place_resume()

    def resume(self) -> None:
        """멈췄던 퀘스트로 돌아간다 — 그 퀘스트의 단계 화면으로 가서 다시 보여 주고, 판정(done)이 이미 참이면 다음 틱에
        평소처럼 넘어간다(값을 비교하는 퀘스트는 멈추기 전에 찍어 둔 base 로 본다 — 자유 모드에서 바꾼 것도 해낸 것).
        자유 모드에서 앞 단계를 막히게 바꿨으면(측정을 지움 · 구성을 바꿔 레벨이 빔) 그 단계 퀘스트부터(`_resume_index`).
        대상 위젯이 숨었으면 기존 규칙대로 0.45 s 뒤 앞으로 건너뛴다(`_missing` · `_skip`)."""
        if not self.paused:
            return
        paused_at = self._paused_at
        self._unpause()
        if not self.active or not self.quests:
            return
        if self.index >= len(self.quests):
            self._finish()
            return
        index = self._resume_index()
        rewound = index != self.index
        if rewound:
            screen = self.quests[index].screen
            self.completed = {i for i in self.completed if i < index}
            self.cleared = {s for s in self.cleared if s in FLOW and FLOW.index(s) < FLOW.index(screen)}
            self.index = index
        self._dir = 1                        # 대상이 없어 건너뛸 때는 앞으로
        self._poll.start(150)
        self._show()
        q = self.current()
        if (not rewound and q is not None and q.base is not None and paused_at is not None
                and paused_at[0] == self.index and self.index not in self.completed):
            self._base = paused_at[1]
        if rewound and q is not None:
            self.show_toast(t('{0} 단계에서 바뀐 것이 있어 거기서부터 이어 갑니다').format(
                t(STEPS[SCREENS.index(q.screen)])))

    def _unpause(self) -> None:
        self.paused = False
        self._paused_at = None
        self.resume_button.hide()

    def _resume_index(self) -> int:
        """이어 할 퀘스트 번호. 멈춘 퀘스트보다 앞 단계 화면 중 [다음 →] 이 막힌 화면(`can_advance_step`)이 있으면 그
        화면의 퀘스트로 돌아간다 — 그 화면에서 결과를 만드는 눌러 보기(판정이 화면 · 찍어 둔 값에 매이지 않는 것) 중 지금
        거짓인 첫 퀘스트, 없으면 그 화면의 [다음 →] 퀘스트(카드가 막힌 이유를 보인다), 그것도 없으면 그 화면 첫 퀘스트.
        (10-11: 자유 모드에서 측정을 지우고 보정 설계로 돌아오면 [계산] 이 할 일이 없었다.)"""
        index = self.index
        q = self.quests[index]
        if q.screen not in FLOW:
            return index
        for screen in FLOW[:FLOW.index(q.screen)]:
            try:
                ok = bool(self.win.can_advance_step(screen)[0])
            except Exception:  # noqa: BLE001 - 판정이 안 되면 막히지 않은 것으로
                ok = True
            if ok:
                continue
            same = [i for i, x in enumerate(self.quests[:index]) if x.screen == screen]
            if not same:                     # 그 화면 퀘스트가 없다(건너뛴 화면) — 그대로 이어 간다
                continue
            for i in same:
                x = self.quests[i]
                if x.kind != 'act' or x.done is None or x.base is not None or x.blocked is not None:
                    continue
                try:
                    done = bool(x.done(self.win))
                except Exception:  # noqa: BLE001
                    done = True
                if not done:
                    return i
            gates = [i for i in same if self.quests[i].blocked is not None]
            return gates[0] if gates else same[0]
        return index

    def _place_resume(self) -> None:
        """[튜토리얼 이어 하기] 자리 — 진행바 줄에 끼워 두었으면 레이아웃이 맡는다(진행바 오른쪽 끝). 진행바가 없는 창이면
        창 오른쪽 위 구석에 띄운다. 아래 내비는 이유 문구 · [건너뛰기] 로 1024 폭에서 꽉 차 쓰지 않았다."""
        button = self.resume_button
        if self._docked:
            return
        button.adjustSize()
        button.move(max(0, self.win.width() - button.width() - 16), 16)
        button.raise_()

    def current(self):
        return self.quests[self.index] if self.active and self.index < len(self.quests) else None

    def next(self, praise: bool = False, note: str = '') -> None:
        q = self.current()
        if q is None:
            return
        self.completed.add(self.index)
        self._dir = 1
        self.toast.hide()                    # 앞 알림이 다음 퀘스트까지 남지 않게(2차 시험)
        following = self.quests[self.index + 1] if self.index + 1 < len(self.quests) else None
        if following is None or following.screen != q.screen:
            self.cleared.add(q.screen)
            name = STEPS[SCREENS.index(q.screen)]
            # 업무 말로(10-10 3차 시험 사용자 결정 1A: '구성 클리어 ★2/7' 같은 게임 말이 프로 장비 화면에 낯설었다)
            head = t('단계 {0}/{1} 끝 · {2}').format(len(self.cleared), len(SCREENS), t(name))
            self.show_toast(f'{head} — {note}' if note else head)
        elif note:                           # 건너뛰기로 대신 한 일(2차 시험: 출력 번호를 말없이 바꿨다)
            self.show_toast(note)
        elif praise:                         # 직접 해낸 눌러 보기 — 눌렀다는 반응(10-10 AI 시험: [다시 확인] 뒤 변화가 없었다)
            self.show_toast(t(q.ok) if q.ok else t('확인 ✓'))
        self.index += 1
        if self.index >= len(self.quests):
            self._finish()
            return
        self._show()

    def prev(self) -> None:
        if not self.active or self.paused or self.index <= 0:
            return
        self._dir = -1
        self.index -= 1
        self.cleared.discard(self.current().screen)
        self._show()

    def _skip(self) -> None:
        """대상이 화면에 없는 눌러 보기를 건너뛴다 — [← 이전으로] 로 오는 중이면 한 칸 더 뒤로."""
        if self._dir < 0 and self.index > 0:
            self.index -= 1
            self._show()
        else:
            self.next()

    def next_pressed(self) -> None:
        """[계속 →] — 둘러보기 · 이미 끝낸 퀘스트는 바로, 눌러 보기는 처음엔 알림만, 두 번째에 결과를 대신 채우고 넘어간다."""
        q = self.current()
        if q is None or self.paused:
            return
        note = ''
        if q.kind == 'act' and self.index not in self.completed:
            if not self.warned:
                self.warned = True              # 규칙은 카드 안에서 알린다(2차 시험: 위쪽 알림은 카드에서 멀어 놓쳤다)
                self.coach.set_hint(True, armed=True)
                self.coach.shake()
                QtCore.QTimer.singleShot(0, self._replace_here)
                return
            if q.auto is not None and not (q.busy is not None and self._is_busy(q)):
                index = self.index
                try:
                    result = q.auto(self.win)        # 결과를 대신 채운다(계산은 뒤에서 이어 돈다)
                    note = t(q.note) if q.note else ''
                except Exception:  # noqa: BLE001 - 채우기가 안 돼도 넘어간다(10-10: 두 번 눌러도 안 넘어갔다)
                    result = None
                if self.index != index or not self.active:
                    # 채우는 동안 이벤트가 돌아(데모 데이터는 워커 + 로딩 표시) 판정이 먼저 넘겼다 — 또 넘기면 다음 카드
                    # (측정 그래프 소개)를 건너뛰었다(10-10 3차 고침 캡처)
                    if note:
                        self.show_toast(note)
                    return
                if result is False:              # 대신 해 줄 수 없는 이유로 막혔다 — 넘어가지 않고 이유를 카드에(코드 리뷰)
                    reason = ''
                    try:
                        reason = q.blocked(self.win) if q.blocked is not None else ''
                    except Exception:  # noqa: BLE001
                        reason = ''
                    self._blocked_on = reason or '이 화면에서 먼저 할 일이 있습니다'
                    self.coach.set_busy(t('아직 넘어갈 수 없어요 — {0}').format(t(self._blocked_on)))
                    self._prefer = self.coach.pos()
                    self._layout_key = None
                    self.coach.shake()
                    return
        self.next(note=note)

    def _finish(self) -> None:
        self.stop()
        box = QtWidgets.QMessageBox(self.win)
        box.setWindowTitle(t('튜토리얼 완료'))
        box.setText(t('샘플 데이터로 측정부터 내보내기까지 한 바퀴 돌았습니다. 실제로는 REW 를 API 모드로 띄우고 '
                      '여러 위치(싱글 9곳 이상)를 잽니다.'))
        again = box.addButton(t('처음부터 다시'), QtWidgets.QMessageBox.ButtonRole.RejectRole)
        real = box.addButton(t('실제 측정 시작'), QtWidgets.QMessageBox.ButtonRole.AcceptRole)
        self._finish_box = box
        box.finished.connect(lambda _r: self._after_finish(box.clickedButton() is real, box.clickedButton() is again))
        box.open()
        self.finished.emit()

    def _after_finish(self, real: bool, again: bool) -> None:
        if real and hasattr(self.win, 'end_trial_mode'):
            QtCore.QTimer.singleShot(0, self.win.end_trial_mode)
        elif again:
            self.win.go(1)
            self.start()

    # ── 표시 ──
    def _disconnect(self) -> None:
        if self._connected is not None:
            try:
                self._connected.disconnect(self._on_signal)
            except (TypeError, RuntimeError):
                pass
            self._connected = None

    def _on_signal(self, *_args) -> None:
        self._done_flag = True

    def _show(self) -> None:
        q = self.current()
        if q is None:
            return
        self.warned = False
        self._done_flag = False
        self._missing = 0
        self._disconnect()
        if self.win.current_step != q.screen:
            self.win.go(q.screen)
        self._end_animation()
        if q.animate is not None:
            self._anim_quest, self._anim_k = q, 0
            self._animate()
            self._anim_timer.start()
        again = self.index in self.completed
        self._base = None
        if q.kind == 'act' and not again:
            if q.base is not None:
                try:
                    self._base = q.base(self.win)
                except Exception:  # noqa: BLE001 - 값을 못 찍으면 [다음으로] 로만 넘긴다
                    self._base = None
            if q.signal is not None:
                try:
                    self._connected = q.signal(self.win)
                    self._connected.connect(self._on_signal)
                except Exception:  # noqa: BLE001 - 위젯이 없으면 [다음으로] 로만 넘긴다
                    self._connected = None
        same = [x for x in self.quests if x.screen == q.screen]
        k = same.index(q)
        name = t(STEPS[SCREENS.index(q.screen)])
        gloss, why_gloss = self._gloss.get(self.index, ([], []))
        last = self.index == len(self.quests) - 1
        self._busy_on = False
        self._blocked_on = ''
        self.coach.show_quest(f'{name} · {k + 1} / {len(same)}', t(q.text), t(q.why) if q.why else '',
                              q.kind == 'act' and not again, [t(g) for g in gloss], [t(g) for g in why_gloss],
                              t('튜토리얼 끝내기') if last else '')        # 3A: '튜토리얼 마치기 ✓' → 사이드바와 같은 말
        self._label = ''      # 꼬리표('둘러보기' · '눌러 볼까요?')는 없앴다(10-10 사용자) — 점선 · 깜빡이는 분홍 틀과 카드 문구로 충분
        items = []
        for i, x in enumerate(same):
            text = t(x.text).split('—')[0].split('. ')[0].strip()      # 자르지 않는다(10-10 AI 시험: 'REW connection wit' 처럼 잘림) — 창이 줄바꿈
            mark = '✓' if i < k else ('▶' if i == k else '·')
            color = '#1f8a3f' if i < k else ('#1b2a29' if i == k else '#7a8e8c')
            items.append(f'<div style="color:{color}">{mark} {text}</div>')
        if q.screen != getattr(self, '_last_screen', None):
            self.panel.user_open = False
            self._last_screen = q.screen
        self.coach.prev_button.setVisible(self.index > 0)           # 첫 카드에는 갈 곳이 없다(2차 시험)
        self.panel.title.setText(f"{t('튜토리얼')} · {name}")      # '퀘스트' 낱말은 뺐다(1A)
        self.panel.items.setText(''.join(items))
        self.panel.bar.setMaximum(len(same))
        self.panel.bar.setValue(k)
        self.panel.overall.hide()                 # 숫자는 카드 · 알약의 '단계 k / n' 과 별 ★ 둘만(2차 시험: 세 가지라 헷갈렸다)
        self.panel.pill.setText(f"{t('튜토리얼')} · {name} {k + 1} / {len(same)} ▾")     # 카드와 같은 번호(10-10 AI 시험)
        for w in (self.overlay, self.coach):
            w.show()
        self.panel.set_folded(self.panel.folded)
        self._layout_key = None
        self._spot_key = None
        self._settle = 2
        self._place()

    def _target(self):
        q = self.current()
        if q is None:
            return None
        try:
            return q.target(self.win)
        except Exception:  # noqa: BLE001 - 위젯을 못 찾으면 구멍 없이 카드만
            return None

    def _place(self) -> None:
        q = self.current()
        if q is None or self.paused:         # 잠시 치운 동안 막 · 카드 · 알약을 다시 띄우지 않는다(창 크기 변경 등)
            return
        raw = self._target()
        # 대상 = 위젯 하나 · 위젯 목록(합친 칸) — 목록 안의 목록은 한 묶음(apart 일 때 묶음마다 테두리 하나)
        groups = [[w for w in (item if isinstance(item, (list, tuple)) else [item]) if w is not None]
                  for item in (raw if isinstance(raw, (list, tuple)) else [raw])]
        groups = [g for g in groups if g]
        widgets = [w for g in groups for w in g]
        target = widgets[0] if widgets else None          # 기준 위젯(스크롤 · 사이드바 판정)
        rect = QtCore.QRect()
        parts: list[QtCore.QRect] = []
        if q.kind == 'act' and (target is None or not target.isVisible()):
            self._missing += 1
            if self._missing >= 3:              # 잠깐 안 보이는 것(화면 전환 중)과 구분 — 0.45 s 넘게 없으면 건너뜀
                self._missing = 0
                self._skip()
                return
        else:
            self._missing = 0
        if target is not None and target.isVisible():
            area = target.parentWidget()
            while area is not None and not isinstance(area, QtWidgets.QScrollArea):
                area = area.parentWidget()
            if area is not None and not getattr(self, '_scrolled', None) is target:
                area.ensureWidgetVisible(target, 20, 20)
                self._scrolled = target
            view = None
            if area is not None:            # 스크롤 영역 밖으로 나간 부분은 강조하지 않는다(2차 시험: 점선이 아래 내비까지)
                viewport = area.viewport()
                view = QtCore.QRect(viewport.mapTo(self.win, QtCore.QPoint(0, 0)), viewport.size())
            for group in groups:              # 여러 위젯이면 합친 칸(2차 시험 t10: '프리셋 줄' 인데 버튼 하나만 감쌌다)
                part = QtCore.QRect()
                for w in group:
                    if w.isVisible():
                        part = part.united(QtCore.QRect(w.mapTo(self.win, QtCore.QPoint(0, 0)), w.size()))
                part = part.intersected(self.win.rect())
                if view is not None:
                    part = part.intersected(view)
                if not part.isNull():
                    parts.append(part)
                    rect = rect.united(part)
        apart = parts if q.apart and len(parts) > 1 else []
        exits = [QtCore.QRect(b.mapTo(self.win, QtCore.QPoint(0, 0)), b.size())
                 for b in (getattr(self.win, 'trial_button', None), getattr(self.win, 'tutorial_restart_button', None))
                 if b is not None and b.isVisible()]
        c = self.coach
        c.fit()
        self.panel.place()
        key = (self.index, rect.getRect(), tuple(p.getRect() for p in apart), c.width(), c.height(), self.win.width(),
               self.win.height(), self.panel.folded, self.panel.user_open, self._label)
        if getattr(self, '_layout_key', None) != key:
            controls = self._controls(widgets)
            self._fold_panel(rect, controls)
            label_at = self._label_spot(rect, controls) if not rect.isNull() and self._label else None
            self.overlay.set_target(rect, q.kind == 'look', self._label if not rect.isNull() else '', exits, label_at,
                                    apart)
            keep = [self.overlay.label_rect(), *exits, *self._keep_rects(q)]
            # 카드 자리 — 화면을 움직여 보여 주는 퀘스트(animate)는 처음 잡은 자리에 고정한다(10-10 3차 시험: 탭이 바뀔
            # 때마다 강조 칸이 달라져 카드가 y 600 ↔ 428 로 뛰고, [계속 →] 을 누르려던 클릭이 막으로 샜다).
            spot_key = (self.index, c.width(), c.height(), self.win.width(), self.win.height())
            if q.animate is None or getattr(self, '_spot_key', None) != spot_key or getattr(self, '_spot', None) is None:
                card_look = (q.kind == 'look' and len(widgets) == 1 and target is not None
                             and target.objectName() == 'card')
                self._spot = self._coach_spot(rect, c.width(), c.height(), keep, target, controls,
                                              holes=apart or None, prefer=getattr(self, '_prefer', None),
                                              contents=self._target_content(target) if card_look else None)
                self._spot_key = spot_key
            self._prefer = None
            self._layout_key = (self.index, rect.getRect(), tuple(p.getRect() for p in apart), c.width(), c.height(),
                                self.win.width(), self.win.height(), self.panel.folded, self.panel.user_open,
                                self._label)
        else:
            self.overlay.update()
        self.overlay.raise_()
        if not c.moved:
            c.move(*self._spot)
        c.raise_()
        self.panel.place()
        # 퀘스트 창이 안내 카드를 가리면 접는다
        for w in (self.panel, self.panel.pill):
            if w.isVisible() and w is self.panel and w.geometry().intersects(c.geometry()):
                self.panel.set_folded(True)
        # 접힌 알약이 강조 테두리(눌러 보기는 퍼지는 고리까지)를 덮으면 그동안 숨긴다 — 단계 · 번호는 안내 카드 머리에도
        # 있다(10-10 3차 시험: 알약이 강조 테두리 오른쪽 위 모서리를 가렸다).
        if self.panel.folded:
            ring = 14 if q.kind == 'act' else 4
            clash = any(self.panel.pill.geometry().intersects(r.adjusted(-ring, -ring, ring, ring))
                        for r in self.overlay.rings())
            self.panel.pill.setVisible(not clash)
        self.toast.raise_()

    def _fold_panel(self, rect: QtCore.QRect, controls) -> None:
        """펼친 퀘스트 창이 강조 칸이나 조작 위젯(입력 칸 · 표 · 버튼)을 덮으면 접는다 — 2차 시험 t05: 1단계 입력 장치 ·
        레벨 숫자 · 위치 목록을 가렸다. 사용자가 알약을 눌러 직접 연 동안은 강조 칸을 덮을 때만 접는다."""
        panel = self.panel
        if panel.folded or not panel.isVisible():
            return
        box = panel.geometry()
        hole = rect.adjusted(-PAD - 30, -PAD - 30, PAD, PAD)
        if (not rect.isNull() and box.intersects(hole)) or (
                not panel.user_open and any(box.intersects(r) for r, w in controls if w != self.INFO_WEIGHT)):
            panel.set_folded(True)

    def _label_spot(self, rect: QtCore.QRect, controls) -> QtCore.QPoint:
        """꼬리표 자리 — 강조 칸 바깥 위 · 아래(양 끝) · 오른쪽 옆 중 조작 위젯 · 퀘스트 알약 · 진행바를 덜 가리는 곳
        (2차 시험: XO 탭 · 화면 제목 · 경로 글자 · 알약 밑에 깔림)."""
        size = self.overlay.label_size(self._label)
        w, h = size.width(), size.height()
        r = rect.adjusted(-PAD - 2, -PAD - 2, PAD + 2, PAD + 2)
        bar = getattr(self.win, 'steps_bar', None)
        top = bar.mapTo(self.win, QtCore.QPoint(0, bar.height())).y() + 2 if bar is not None and bar.isVisible() else 0
        W, H = self.win.width(), self.win.height()
        spots = [(r.right() - w, r.top() - h - 2), (r.left(), r.top() - h - 2),
                 (r.right() - w, r.bottom() + 2), (r.left(), r.bottom() + 2),
                 (r.right() + 6, r.top() + 12), (r.left() - w - 6, r.top() + 12)]
        avoid = [(self.panel.geometry(), 8.0)] if self.panel.isVisible() else []
        if self.panel.pill.isVisible():
            avoid.append((self.panel.pill.geometry(), 8.0))
        best = None
        for x, y in spots:
            if x < 0 or y < top or x + w > W or y + h > H - NAV_H:
                continue
            box = QtCore.QRect(int(x), int(y), w, h)
            cost = sum(weight * _area(box.intersected(o)) for o, weight in (*controls, *avoid))
            if best is None or cost < best[0]:
                best = (cost, QtCore.QPoint(int(x), int(y)))
            if cost == 0:
                break
        return best[1] if best is not None else QtCore.QPoint(max(0, r.left()), max(top, r.top() - h - 2))

    #: 안내 카드가 되도록 덮지 않을 조작 위젯(버튼 · 목록 · 입력 칸 · 그래프) — 10-10 AI 시험: 다음에 누를 배치 카드 · 그래프를 덮었다
    _CONTROLS = (QtWidgets.QAbstractButton, QtWidgets.QComboBox, QtWidgets.QAbstractSpinBox, QtWidgets.QLineEdit,
                 QtWidgets.QAbstractItemView, QtWidgets.QGraphicsView)

    def _controls(self, targets) -> list[tuple[QtCore.QRect, float]]:
        """창에 보이는 조작 위젯 · 정보 자리(창 좌표)와 무게 — 버튼 · 입력 칸 2, 목록 · 표 1, 글자 · 미터 1(10-10 3차
        시험: 레벨 dBFS 값 · 마이크 입력 글자 · 범례를 덮었다), 그래프 0.3(덮어도 덜 아프다). 강조 대상 안의 것과 튜토리얼
        위젯은 뺀다(2차 시험 t05: 카드가 [이 위치 측정] · [계산] 같은 이웃 버튼을 덮었다)."""
        mine = (self.overlay, self.coach, self.panel, self.panel.pill, self.toast, self.resume_button)
        rects = []
        for w, weight in self._content_items(self.win):
            p, skip = w, False
            while p is not None and p is not self.win:
                if p in mine or p in targets:
                    skip = True
                    break
                p = p.parentWidget()
            if not skip:
                rects.append((self._content_rect(w), weight))
        return rects

    #: 글자 · 미터 무게(넓이당) — 퀘스트 창은 글자 위에 떠도 접지 않는다(조작 위젯만 본다, `_fold_panel`).
    INFO_WEIGHT = 0.9

    def _content_items(self, root) -> list:
        """`root` 아래 보이는 조작 위젯 · 글자 · 미터와 무게."""
        out = []
        for w in root.findChildren(QtWidgets.QWidget):
            if not w.isVisible() or w.width() < 8 or w.height() < 8:
                continue
            if isinstance(w, self._CONTROLS):
                out.append((w, 0.3 if isinstance(w, QtWidgets.QGraphicsView)
                            else 1.0 if isinstance(w, QtWidgets.QAbstractItemView) else 2.0))
            elif isinstance(w, QtWidgets.QLabel) and w.text().strip() and not isinstance(
                    w.parentWidget(), (QtWidgets.QAbstractButton, QtWidgets.QAbstractItemView)):
                out.append((w, self.INFO_WEIGHT))
            elif type(w).__name__.endswith('Meter'):
                out.append((w, self.INFO_WEIGHT))
        return out

    def _content_rect(self, w) -> QtCore.QRect:
        """위젯 자리(창 좌표) — 한 줄 글자는 글자가 그려지는 폭만(늘어난 라벨의 빈 오른쪽은 덮어도 된다)."""
        rect = QtCore.QRect(w.mapTo(self.win, QtCore.QPoint(0, 0)), w.size())
        if isinstance(w, QtWidgets.QLabel) and not w.wordWrap() and w.textFormat() != QtCore.Qt.TextFormat.RichText:
            width = min(rect.width(), w.fontMetrics().horizontalAdvance(w.text()) + 8)
            align = w.alignment()
            if align & QtCore.Qt.AlignmentFlag.AlignRight:
                rect.setLeft(rect.right() - width)
            elif align & QtCore.Qt.AlignmentFlag.AlignHCenter:
                rect = QtCore.QRect(rect.center().x() - width // 2, rect.top(), width, rect.height())
            else:
                rect.setWidth(width)
        return rect

    def _target_content(self, target) -> list[QtCore.QRect]:
        """둘러볼 카드 안의 내용 자리 — 카드의 빈 아래쪽은 안내 카드를 둬도 된다(10-10 3차 시험: 마스터 출력 카드 둘러보기에서
        카드가 옆 채널 표 dBFS 값을 덮었다 — 그 카드 아래가 비어 있었다)."""
        return [self._content_rect(w) for w, _weight in self._content_items(target)]

    def _coach_spot(self, rect: QtCore.QRect, cw: int, ch: int, keep_clear=(), target=None,
                    controls=None, holes=None, prefer=None, contents=None) -> tuple[int, int]:
        """안내 카드 자리 — 진행바 아래 · 아래 내비 위 안에서, 강조 칸 · 꼬리표 · 탈출 버튼 · 다른 조작 위젯을 가장 덜
        가리는 곳. 후보는 대상의 아래 · 위 · 오른쪽 · 왼쪽(양 끝 맞춤 둘씩)과, 다 모자랄 때(그래프 · 사이드바처럼 큰
        대상) 대상 안쪽 구석. (10-10 AI 시험: 카드가 진행바 · 그래프 · 배치 카드 · 자기 꼬리표를 가렸다.)
        `_place` 가 자리 · 크기가 바뀔 때만 부른다(_tick 이 0.15 s 마다 돈다).

        `holes` = 따로 테두리친 강조 칸들(있으면 합친 칸 대신 이것으로 덮임을 잰다). 대상이 든 카드(QFrame 'card')의 나머지
        자리도 가볍게 피한다 — 대상 바로 옆 정보(이웃 탭 · 같은 카드의 값 · 범례)는 함께 봐야 하는 것이다(10-10 3차 시험:
        XO 탭 카드가 SPL · GD 탭을, 측정 그래프 카드가 x 축 · 범례를 덮었다). `prefer` = 지금 자리 — 비용이 같으면 그대로
        두어 [왜?] 를 펼치면 아래로 커진다(3차 시험: 위로 커져 탭 · 버튼을 덮었다)."""
        W, H = self.win.width(), self.win.height()
        bar = getattr(self.win, 'steps_bar', None)
        top = bar.mapTo(self.win, QtCore.QPoint(0, bar.height())).y() + 8 if bar is not None and bar.isVisible() else 16
        bottom = H - NAV_H
        avoid = [r for r in keep_clear if not r.isNull()]
        lo_y, hi_y = top, max(top, bottom - ch)
        lo_x, hi_x = 16, max(16, W - cw - 16)
        side = getattr(self.win, 'side_frame', None)        # 대상이 사이드바 밖이면 사이드바를 덮지 않는다(2차 시험)
        if (side is not None and side.isVisible() and target is not None and target is not side
                and not side.isAncestorOf(target)):
            lo_x = min(side.mapTo(self.win, QtCore.QPoint(side.width(), 0)).x() + 12, hi_x)

        def clamp_x(x):
            return min(max(lo_x, x), hi_x)

        def clamp_y(y):
            return min(max(lo_y, y), hi_y)

        if rect.isNull():
            spot = (clamp_x((W - cw) // 2), clamp_y((top + bottom - ch) // 2))
        else:
            hole_list = [r.adjusted(-PAD - 2, -PAD - 2, PAD + 2, PAD + 2) for r in (holes or [rect])]
            # 따로 테두리친 대상(탭 줄 + 오른쪽 카드)은 그 사이 그래프가 배경일 뿐이라 카드 피하기를 빼고 그래프 위에 둔다
            near = self._near_card(target, rect) if not holes else None
            gap = PAD + 12
            above = rect.top() - gap                  # 꼬리표가 위에 있으면 꼬리표 위로(전엔 카드가 꼬리표를 덮었다)
            for r in avoid[:1]:
                if r.bottom() <= rect.top() + PAD:
                    above = min(above, r.top() - 6)
            spots = []
            if prefer is not None:
                spots.append((int(prefer.x()), int(prefer.y())))
            for x in (rect.left(), rect.right() - cw):                       # 아래 · 위 — 왼쪽 · 오른쪽 끝 맞춤
                spots.append((clamp_x(x), rect.bottom() + gap))
            for x in (rect.left(), rect.right() - cw):
                spots.append((clamp_x(x), above - ch))
            for y in (rect.top(), rect.bottom() - ch):                       # 오른쪽 · 왼쪽 — 위 · 아래 끝 맞춤
                spots.append((rect.right() + gap, clamp_y(y)))
                spots.append((rect.left() - gap - cw, clamp_y(y)))
            for x, y in ((rect.right() - cw - 12, rect.bottom() - ch - 12), (rect.right() - cw - 12, rect.top() + 12),
                         (rect.left() + 12, rect.bottom() - ch - 12), (rect.left() + 12, rect.top() + 12)):  # 대상 안쪽 구석
                spots.append((clamp_x(x), clamp_y(y)))

            def covered(box: QtCore.QRect, other: QtCore.QRect) -> int:
                r = box.intersected(other)
                return max(0, r.width()) * max(0, r.height())

            controls = self._controls([target]) if controls is None else controls
            best = None
            for x, y in spots:
                if not (lo_x <= x <= hi_x and lo_y <= y <= hi_y):
                    continue
                box = QtCore.QRect(int(x), int(y), cw, ch)
                # 대상은 거의 절대 덮지 않는다(2차 시험: 샘플 버튼을 덮은 자리가 뽑혔다 — 대상 1 : 조작 위젯 0.5 였다).
                # 둘러볼 카드(contents)는 안의 내용만 그 무게로 — 빈 자리는 가볍게(테두리 안에 들어가도 내용은 보인다)
                if contents is not None:
                    hole_cost = (20 * sum(covered(box, r) for r in contents)
                                 + 0.1 * sum(covered(box, h) for h in hole_list))
                else:
                    hole_cost = 20 * sum(covered(box, h) for h in hole_list)
                cost = (hole_cost + 10 * sum(covered(box, r) for r in avoid)
                        + sum(weight * covered(box, r) for r, weight in controls))
                if near is not None:
                    cost += self.NEAR_WEIGHT * covered(box, near)
                if best is None or cost < best[0]:
                    best = (cost, int(x), int(y))
                if cost == 0:
                    break
            spot = (best[1], best[2]) if best is not None else (clamp_x(rect.left()), clamp_y(rect.bottom() + gap))
        return spot

    #: 대상이 든 카드의 나머지 자리 무게(넓이당) — 조작 위젯(2)과 그래프(0.3) 사이. 다른 카드로 옮길 수 있으면 옮기고,
    #: 갈 곳이 없으면(1280×800 오른쪽 칸처럼) 그 카드 안에서 조작 위젯을 덜 덮는 자리를 고른다.
    NEAR_WEIGHT = 2.0

    def _near_card(self, target, rect: QtCore.QRect):
        """대상이 든 카드 자리(창 좌표) — 대상이 카드 자체이거나 카드가 없으면 None."""
        if target is None:
            return None
        from secs.wizard.tutorial_quests import card_of
        frame = card_of(target)
        if frame is None or frame is target or not frame.isVisible():
            return None
        box = QtCore.QRect(frame.mapTo(self.win, QtCore.QPoint(0, 0)), frame.size()).intersected(self.win.rect())
        return None if box.isNull() or rect.contains(box) else box

    def _animate(self) -> None:
        q = self._anim_quest
        if q is None or q is not self.current():
            self._end_animation()
            return
        try:
            more = q.animate(self.win, self._anim_k)
        except Exception:  # noqa: BLE001 - 보여 주기가 안 돼도 튜토리얼은 이어 간다
            self._end_animation()
            return
        self._anim_k += 1
        self._layout_key = None            # 보이는 카드가 바뀌었다 — 강조 칸을 다시 잡는다(카드 자리는 그대로 — _place)
        if more is False:                  # 퀘스트가 그만 보여 주라고 하면 멈춘다(탭 넘기기는 10-11 부터 [계속] 까지 반복)
            self._anim_timer.stop()

    def _end_animation(self) -> None:
        self._anim_timer.stop()
        q, self._anim_quest = self._anim_quest, None
        if q is not None and q.animate_end is not None:
            try:
                q.animate_end(self.win)
            except Exception:  # noqa: BLE001
                pass

    def _replace(self) -> None:
        self._layout_key = None
        self._place()

    def _replace_here(self) -> None:
        """카드 높이가 바뀌었다 — 지금 자리를 먼저 보고 다시 잡는다."""
        self._prefer = self.coach.pos() if self.coach.isVisible() and not self.coach.moved else None
        self._replace()

    def _keep_rects(self, q) -> list[QtCore.QRect]:
        """카드가 덮지 말 자리 — 퀘스트의 결과 자리(keep), 퀘스트 창 · 알약, 계산 진행 알림(busyPanel)."""
        widgets = []
        if q.keep is not None:
            try:
                widgets += list(q.keep(self.win))
            except Exception:  # noqa: BLE001 - 결과 자리를 못 찾으면 그냥 둔다
                pass
        widgets += [self.panel, self.panel.pill]
        widgets += self.win.findChildren(QtWidgets.QWidget, 'busyPanel')
        # keep 은 창 좌표 사각형도 줄 수 있다(표 칸처럼 스크롤 창에 잘린 자리 — 3차 시험: 측정 출력 칸)
        return [w if isinstance(w, QtCore.QRect) else QtCore.QRect(w.mapTo(self.win, QtCore.QPoint(0, 0)), w.size())
                for w in widgets if isinstance(w, QtCore.QRect) or (w is not None and w.isVisible())]

    def _is_busy(self, q) -> bool:
        try:
            return bool(q.busy(self.win))
        except Exception:  # noqa: BLE001
            return False

    def _done(self, q) -> bool:
        if q.kind != 'act':
            return False
        if q.busy is not None and self._is_busy(q):     # 재생 · 계산이 끝나야 끝(2차 시험: 누르자마자 '완료')
            return False
        if self._done_flag:
            return True
        if q.done is None:
            return False
        try:
            return bool(q.done(self.win, self._base) if q.base is not None else q.done(self.win))
        except Exception:  # noqa: BLE001 - 판정이 안 되면 [다음으로] 로만 넘긴다
            return False

    def _tick(self) -> None:
        q = self.current()
        if q is None or self.paused:
            return
        if self.index in self.completed and self.win.current_step != q.screen:
            following = self.quests[self.index + 1] if self.index + 1 < len(self.quests) else None
            if following is not None and following.screen == self.win.current_step:
                self.next()              # 끝낸 퀘스트로 돌아왔다가 화면 [다음 →] 으로 넘어간 경우(2차 시험 t08)
                return
        if self.win.current_step != q.screen and q.kind == 'look':
            self.win.go(q.screen)
        # 이미 끝낸 퀘스트(이전으로 돌아온 것)는 판정하지 않는다 — 판정이 바로 참이라 저절로 넘어가 [← 이전으로] 가
        # 먹통처럼 보였다(10-10 AI 시험: 측정 4/5)
        if self.index not in self.completed and self._done(q):
            self.next(praise=True)
            return
        # 화면 [다음 →] 이 막혔으면(출력 번호 중복 등) 카드에 이유를 띄운다(10-10 사용자: 튜토리얼은 넘어가라는데 안 넘어갔다)
        blocked = ''
        if q.blocked is not None and self.index not in self.completed:
            try:
                blocked = q.blocked(self.win) or ''
            except Exception:  # noqa: BLE001
                blocked = ''
        if blocked != self._blocked_on:
            self._blocked_on = blocked
            if blocked:
                # 이유(화면 can_advance 문구)도 옮긴다 — 영어 카드에 한국어 이유가 그대로 나왔다(10-10 코드 리뷰)
                self.coach.set_busy(t('아직 넘어갈 수 없어요 — {0}').format(t(blocked)))
                self._prefer = self.coach.pos()
                self._layout_key = None
            else:
                self._show()
                return
        busy = q.busy is not None and self.index not in self.completed and self._is_busy(q)
        if busy != self._busy_on:                       # 진행 중이면 카드 문구를 '기다리세요' 로
            self._busy_on = busy
            if busy:
                self.coach.set_busy(t(q.busy_text))
                self._prefer = self.coach.pos()
                self._layout_key = None
            else:
                self._show()
                return
        if getattr(self, '_settle', 0) > 0:
            # 화면을 막 바꾼 뒤 처음 자리는 레이아웃이 덜 잡힌 채 고른 것일 수 있다 — 두 번 더 고른다(지금 자리가 먼저라
            # 괜찮으면 그대로). 3차 고침 캡처: 레벨 화면에 들어오자마자 고른 자리가 채널 표 첫 줄을 덮었다.
            self._settle -= 1
            if not self.coach.moved:
                self._prefer = self.coach.pos()
            self._layout_key = None
            self._spot_key = None               # 탭이 도는 퀘스트도 첫 탭에서는 다시 고른다(도는 건 1.8 s 뒤부터)
        self._place()

    def _outside(self) -> None:
        if self.paused:
            return
        self.coach.shake()
        q = self.current()
        if q is not None and not self.toast.isVisible():     # 틀린 곳을 눌렀을 때 무엇을 할지(10-10 AI 시험)
            if q.kind == 'act' and self.index not in self.completed:
                self.show_toast(t('분홍 테두리 안을 눌러 보세요'))
            else:
                # '카드의' — 아래 내비 [다음 →] 과 헷갈렸다(10-10 3차 시험)
                self.show_toast(t('카드의 [계속 →] 으로 넘어가세요'))

    def show_toast(self, text: str) -> None:
        self.toast.setText(text)
        self.toast.adjustSize()
        self.toast.move((self.win.width() - self.toast.width()) // 2, 70)
        self.toast.show()
        self.toast.raise_()
        self._toast_timer.start(1800)

    def eventFilter(self, obj, event):  # noqa: N802 - Qt 이름
        if obj is self.win and event.type() == QtCore.QEvent.Type.Resize and self.active:
            QtCore.QTimer.singleShot(0, self._place_resume if self.paused else self._place)
        return False


__all__ = ['Tutorial', 'Overlay', 'Coach', 'QuestPanel']
