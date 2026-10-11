# -*- coding: utf-8 -*-
"""로딩 오버레이 — 오래 걸리는 계산 동안 단계 화면 위에 "무엇을 하는 중 · 경과" 를 띄운다(2026-09-23).

사용자: "오래 걸리는 단계에서는 로딩창을 띄워줘." 그래서 표시기와 오버레이를 나눴다.
- `LoadingIndicator` — 로고 방울 24프레임 스프라이트 재생기. 파일을 못 읽으면 회전하는 점 8개로
  돌아간다(크기 인자 — 기본 `SIZE` 160, `start()`/`stop()`/`running()`).
- `BusyDrop` — 작은 방울(기본 48 px, 09-24). 화면을 덮지 않는 대기(구석 `busy.BusyPanel`,
  1단계 REW 확인, 5단계 측정)에 쓴다. `set_busy()` 도 300 ms 규칙을 따른다.
- `run_off_thread(func)` — 동기 호출처럼 보이게 워커 스레드에서 돌리며 화면을 계속 그린다.
- `LoadingOverlay` — 단계 스택을 덮는 반투명 막 + 가운데 카드(표시기 · 문구 · 경과/예상 · [취소]).
  `want(문구, 예상, 취소)` 를 계속 부르면 **300 ms 가 지난 뒤에야** 보인다(짧은 일에 깜빡이지 않게),
  `clear()` 로 거둔다. `show_now()` 는 바로 보인다(느렸던 단계로 이동하기 직전).

누가 부르나: `WizardWindow._poll_busy`(250 ms)가 단계들의 "돌고 있음"을 보고 오버레이 대상 작업
(6단계 설계, 7단계 저역 통합, 8단계 진입 PEQ 다듬기, 5단계 mdat 불러오기)이면 `want()`, 아니면 `clear()`.
측정(5단계 스윕)은 로그·진행 막대를 봐야 하므로 오버레이 대신 구석의 `busy.BusyPanel` 을 쓴다.
무거운 일은 워커 스레드에서 돌아야 한다 — 화면 스레드가 막히면 오버레이도 그려지지 않는다.
"""
from __future__ import annotations

import math
import threading
import time
from pathlib import Path
from typing import Callable

from PyQt6 import QtCore, QtGui, QtWidgets

from secs.wizard.strings import t
from secs.wizard.style import COLORS

#: 이만큼(ms) 계속 돌고 있을 때만 오버레이를 보인다.
SHOW_DELAY_MS = 300


class LoadingIndicator(QtWidgets.QWidget):
    """로고 방울 스프라이트 재생기. 한 변 크기는 인자(기본 160 px — 09-24 사용자: 로딩 카드는 크게).

    작은 크기(구석 진행 표시·단계 안 48 px 등, 09-24)도 같은 스프라이트를 쓴다. 320 px 원본을
    그릴 때마다 줄이면 QPainter 의 쌍선형 보간이 픽셀 4개만 봐서 1/7 로 줄일 때 가장자리가
    거칠어진다 — 그래서 `크기 × devicePixelRatio` 물리 px 로 한 번 면적 평균(SmoothTransformation)
    으로 줄인 24장을 크기별로 캐시해 두고 1:1 로 찍는다. 읽기 실패 때만 회전하는 점 8개.
    """

    SIZE = 160
    FRAME_COUNT = 24
    PERIOD_MS = 1067
    FRAME_MS = round(PERIOD_MS / FRAME_COUNT)
    SPRITE_FRAME_PX = 320
    SPRITE_PATH = Path(__file__).with_name('assets') / 'loading_drip.png'
    DOTS = 8
    FALLBACK_SIZE = 44
    _sprite_cache: QtGui.QImage | None = None
    _sprite_load_attempted = False
    #: 물리 px → 그 크기로 줄인 프레임 24장(프로세스 공용).
    _scaled_frames: dict[int, list[QtGui.QImage]] = {}

    def __init__(self, parent=None, size: int | None = None) -> None:
        super().__init__(parent)
        self._px = int(size or self.SIZE)
        self.setFixedSize(self._px, self._px)
        self._frame = 0
        self._sprite = self._load_sprite()
        self._timer = QtCore.QTimer(self)
        self._timer.setInterval(self.FRAME_MS)
        self._timer.timeout.connect(self._advance)

    def indicator_size(self) -> int:
        """논리 px 한 변(QWidget.size() 와 헷갈리지 않게 이름을 따로 둔다)."""
        return self._px

    def start(self) -> None:
        self._timer.start()

    def stop(self) -> None:
        self._timer.stop()

    def running(self) -> bool:
        return self._timer.isActive()

    def _advance(self) -> None:
        self._frame = (self._frame + 1) % self.FRAME_COUNT
        self.update()

    @classmethod
    def _load_sprite(cls) -> QtGui.QImage | None:
        """프로세스에서 한 번만 읽는다. 크기나 프레임 수가 다르면 대체 표시기로 간다."""
        if cls._sprite_load_attempted:
            return cls._sprite_cache
        cls._sprite_load_attempted = True
        image = QtGui.QImage(str(cls.SPRITE_PATH))
        expected_width = cls.SPRITE_FRAME_PX * cls.FRAME_COUNT
        if image.isNull() or image.width() != expected_width or image.height() != cls.SPRITE_FRAME_PX:
            cls._sprite_cache = None
        else:
            cls._sprite_cache = image
        return cls._sprite_cache

    def frames_for(self, physical_px: int) -> list[QtGui.QImage]:
        """`physical_px` 한 변으로 미리 줄인 24장(원본 320 px 보다 크게는 늘리지 않는다). 캐시한다."""
        sprite = self._sprite
        if sprite is None:
            return []
        full = self.SPRITE_FRAME_PX
        px = max(1, min(int(physical_px), full))
        frames = LoadingIndicator._scaled_frames.get(px)
        if frames is None:
            frames = []
            for index in range(self.FRAME_COUNT):
                frame = sprite.copy(index * full, 0, full, full)
                if px != full:
                    frame = frame.scaled(px, px, QtCore.Qt.AspectRatioMode.IgnoreAspectRatio,
                                         QtCore.Qt.TransformationMode.SmoothTransformation)
                frames.append(frame)
            LoadingIndicator._scaled_frames[px] = frames
        return frames

    def paintEvent(self, _event) -> None:  # noqa: N802 - Qt 이름
        painter = QtGui.QPainter(self)
        if self._sprite is not None:
            painter.setRenderHint(QtGui.QPainter.RenderHint.SmoothPixmapTransform)
            dpr = max(1.0, float(self.devicePixelRatioF()))
            frames = self.frames_for(round(self._px * dpr))
            # 대상 좌표는 논리 px — 미리 줄인 물리 px 프레임이라 화면 배율에서 1:1 로 찍힌다.
            painter.drawImage(QtCore.QRectF(0, 0, self._px, self._px), frames[self._frame % len(frames)])
            painter.end()
            return

        # 패키지 누락·손상 때도 로딩 상태를 잃지 않도록 기존 점 표시기를 남긴다(작으면 비례 축소).
        painter.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing)
        painter.setPen(QtCore.Qt.PenStyle.NoPen)
        center = self._px / 2.0
        span = min(self.FALLBACK_SIZE, self._px)
        radius = span / 2.0 - span * 6.0 / self.FALLBACK_SIZE
        dot = max(1.5, 3.6 * span / self.FALLBACK_SIZE)
        for index in range(self.DOTS):
            age = (self._frame % self.DOTS - index) % self.DOTS
            color = QtGui.QColor(COLORS['accent'])
            color.setAlphaF(1.0 - age / self.DOTS * 0.85)
            painter.setBrush(color)
            angle = 2 * math.pi * index / self.DOTS - math.pi / 2
            painter.drawEllipse(QtCore.QPointF(center + radius * math.cos(angle),
                                               center + radius * math.sin(angle)), dot, dot)
        painter.end()


class BusyDrop(LoadingIndicator):
    """작은 방울(기본 48 px) — 화면을 덮지 않는 대기 표시(09-24 사용자: "딜레이를 느낄 만한 지점엔 다 애니메이션").

    `set_busy(True)` 를 계속 불러도 되고(창의 250 ms 점검처럼), `SHOW_DELAY_MS` 가 지나도 **아직
    바쁠 때만** 보이고 돈다 — 오버레이와 같은 규칙이라 짧은 일에 깜빡이지 않는다. `set_busy(False)`
    는 곧바로 멈추고 숨긴다. `keep_space=True` 면 숨어 있어도 자리를 지켜 옆 글자가 흔들리지 않는다.
    """

    SMALL = 48

    def __init__(self, parent=None, size: int | None = None, keep_space: bool = False) -> None:
        super().__init__(parent, size or self.SMALL)
        if keep_space:
            policy = self.sizePolicy()
            policy.setRetainSizeWhenHidden(True)
            self.setSizePolicy(policy)
        self._wanted = False
        self._delay = QtCore.QTimer(self)
        self._delay.setSingleShot(True)
        self._delay.timeout.connect(self._appear)
        self.hide()

    def set_busy(self, busy: bool) -> None:
        busy = bool(busy)
        if busy == self._wanted:
            return
        self._wanted = busy
        if busy:
            self._delay.start(SHOW_DELAY_MS)
            return
        self._delay.stop()
        self.stop()
        self.hide()

    def busy(self) -> bool:
        """바쁘다고 들은 상태(보이기 전 300 ms 동안도 참)."""
        return self._wanted

    def _appear(self) -> None:
        if self._wanted:
            self.show()
            self.start()


def run_off_thread(func: Callable[[], object], name: str = 'secs-work'):
    """`func()` 를 워커 스레드에서 돌리고, 끝날 때까지 이벤트 루프를 돌린 뒤 결과를 돌려준다.

    부르는 쪽에는 동기 호출처럼 보이지만(테스트·버튼 모두 그대로) 그동안 화면이 그려진다 — 그래서
    오버레이·방울이 움직인다. 사용자 입력은 미뤄 둔다(도는 동안 다른 버튼이 끼어들지 않게).
    예외는 화면 스레드에서 그대로 다시 낸다. 5단계 mdat·샘플, 8단계 내보내기가 쓴다(09-23·09-24).
    """
    box: dict = {}

    def run() -> None:
        try:
            box['value'] = func()
        except BaseException as error:  # noqa: BLE001 - 화면 스레드에서 다시 낸다
            box['error'] = error

    worker = threading.Thread(target=run, name=name, daemon=True)
    worker.start()
    app = QtWidgets.QApplication.instance()
    while worker.is_alive():
        if app is not None:
            app.processEvents(QtCore.QEventLoop.ProcessEventsFlag.ExcludeUserInputEvents, 20)
        worker.join(0.01)
    if 'error' in box:
        raise box['error']
    return box.get('value')


class LoadingOverlay(QtWidgets.QWidget):
    """부모(단계 스택) 전체를 덮는 막. 막이 떠 있는 동안 아래 화면은 눌리지 않는다."""

    def __init__(self, parent: QtWidgets.QWidget) -> None:
        super().__init__(parent)
        self.setObjectName('loadingOverlay')
        self.setAttribute(QtCore.Qt.WidgetAttribute.WA_StyledBackground, True)
        self.setStyleSheet(
            'QWidget#loadingOverlay { background: rgba(236, 241, 240, 170); }'
            f'QFrame#loadingCard {{ background: {COLORS["panel"]}; border: 1px solid {COLORS["chassis"]};'
            ' border-radius: 12px; }'
            f'QLabel#loadingText {{ color: {COLORS["ink"]}; font-size: 20px; font-weight: 600; }}'
            f'QLabel#loadingTime {{ color: {COLORS["ink2"]}; font-size: 15px; }}')
        outer = QtWidgets.QVBoxLayout(self)
        outer.addStretch(1)
        self.card = QtWidgets.QFrame()
        self.card.setObjectName('loadingCard')
        # 09-24 사용자: 로딩 카드를 더 크게(스크린샷 기준 약 420×250) — 로고를 왼쪽에 크게, 글·버튼을 오른쪽에.
        self.card.setMinimumSize(420, 230)
        box = QtWidgets.QHBoxLayout(self.card)
        box.setContentsMargins(36, 32, 40, 32)
        box.setSpacing(28)
        self.indicator = LoadingIndicator()
        box.addWidget(self.indicator, 0, QtCore.Qt.AlignmentFlag.AlignVCenter)
        self.text = QtWidgets.QLabel('')
        self.text.setObjectName('loadingText')
        self.time_label = QtWidgets.QLabel('')
        self.time_label.setObjectName('loadingTime')
        self.cancel_button = QtWidgets.QPushButton(t('취소'))
        self.cancel_button.clicked.connect(self._on_cancel)
        from secs.wizard import tooltips
        tooltips.attach(self.cancel_button, 'load.cancel')
        side = QtWidgets.QVBoxLayout()
        side.setSpacing(10)
        side.addStretch(1)
        side.addWidget(self.text)
        side.addWidget(self.time_label)
        side.addSpacing(8)
        side.addWidget(self.cancel_button, 0, QtCore.Qt.AlignmentFlag.AlignLeft)
        side.addStretch(1)
        box.addLayout(side, 1)
        row = QtWidgets.QHBoxLayout()
        row.addStretch(1)
        row.addWidget(self.card)
        row.addStretch(1)
        outer.addLayout(row)
        outer.addStretch(1)
        self.hide()
        self._what = ''
        self._estimate = 0.0
        self._cancel: Callable[[], None] | None = None
        self._since = 0.0          # 처음 want() 가 온 시각(monotonic). 0 = 원하지 않음
        self._started = 0.0        # 보이기 시작한 시각
        self._delay = QtCore.QTimer(self)
        self._delay.setSingleShot(True)
        self._delay.timeout.connect(self._show)
        self._tick = QtCore.QTimer(self)
        self._tick.setInterval(250)
        self._tick.timeout.connect(self._update_time)
        parent.installEventFilter(self)

    # ── 부모 크기를 따라간다 ──
    def eventFilter(self, obj, event) -> bool:  # noqa: N802 - Qt 이름
        if obj is self.parentWidget() and event.type() == QtCore.QEvent.Type.Resize:
            self.setGeometry(obj.rect())
        return False

    # ── 상태 ──
    def want(self, what: str, estimate_s: float = 0.0, cancel: Callable[[], None] | None = None) -> None:
        """이 작업이 돌고 있다. 처음이면 `SHOW_DELAY_MS` 뒤에 보인다(그때도 돌고 있으면)."""
        self._what, self._estimate, self._cancel = what, float(estimate_s or 0.0), cancel
        if self.isVisible():
            self._render()
            return
        if not self._since:
            self._since = time.monotonic()
            self._delay.start(SHOW_DELAY_MS)

    def show_now(self, what: str) -> None:
        """기다리지 않고 바로 보이고 그린다 — 화면 스레드가 곧 잠깐 바빠질 때(느린 단계 이동)."""
        self._what, self._estimate, self._cancel = what, 0.0, None
        self._since = self._since or time.monotonic()
        self._show()
        self.repaint()

    def clear(self) -> None:
        self._delay.stop()
        self._since = 0.0
        if self.isVisible():
            self._tick.stop()
            self.indicator.stop()
            self.hide()

    def _show(self) -> None:
        if not self._since:
            return
        self._started = time.monotonic()
        parent = self.parentWidget()
        if parent is not None:
            self.setGeometry(parent.rect())
        self._render()
        self.show()
        self.raise_()
        self.indicator.start()
        self._tick.start()

    def _render(self) -> None:
        self.text.setText(str(t(self._what)))
        self.cancel_button.setVisible(self._cancel is not None)
        self._update_time()

    def _update_time(self) -> None:
        elapsed = time.monotonic() - (self._since or time.monotonic())
        text = f'{t("경과")} {elapsed:.0f} s'
        if self._estimate > 0:
            text += f' · {t("예상")} {self._estimate:.0f} s'
        self.time_label.setText(text)

    def _on_cancel(self) -> None:
        if self._cancel is not None:
            self._cancel()


__all__ = ['BusyDrop', 'LoadingIndicator', 'LoadingOverlay', 'SHOW_DELAY_MS', 'run_off_thread']
