# -*- coding: utf-8 -*-
"""작업 중 표시 — 계산·측정이 도는 동안 화면 구석에 "무엇을 하는 중 · 경과 · 예상" 을 띄운다(2026-09-22).

사용자: "연산·측정 중에 반응이 늦어지는데 간단한 로딩창을, 오래 걸리는 건 예상 시간도". 작업은 이미
스레드에서 돌지만 화면이 잠깐씩 멈춰 보여 아무 표시가 없으면 죽은 줄 안다. 이 패널은 입력을 막지
않는다(취소 버튼은 눌러야 하니까). 09-24 사용자("딜레이를 느낄 만한 지점엔 다 애니메이션")로
왼쪽에 작은 로고 방울(`loading.BusyDrop`, 48 px)이 돌고, 오버레이처럼 300 ms 넘게 도는 일에만 뜬다.

`WizardWindow` 가 `BusyPanel` 하나를 만들어 두고 250 ms 마다 `poll()` 로 단계들의 `running()` 을 본다
(신호를 일일이 잇는 것보다 단순하고, 새 작업이 생겨도 `JOBS` 에 한 줄이면 된다). 예상 시간은
같은 종류의 지난 실행 시간(`BassResult.elapsed_s` 등) 또는 스윕 수 × 스윕 길이에서 어림한다.
"""
from __future__ import annotations

import time
from typing import Callable

from PyQt6 import QtCore, QtWidgets

from secs.wizard.loading import SHOW_DELAY_MS, BusyDrop
from secs.wizard.strings import t
from secs.wizard.style import COLORS


class BusyPanel(QtWidgets.QFrame):
    """오른쪽 아래 작은 패널: [방울] + "계산 중… 경과 3 s · 예상 6 s" / 진행 막대(무한).

    `want()` = 창의 점검용(300 ms 지난 뒤에야 보인다), `start()` = 바로 보인다(QA 캡처·테스트).
    """

    DROP_PX = BusyDrop.SMALL

    def __init__(self, parent: QtWidgets.QWidget) -> None:
        super().__init__(parent)
        self.setObjectName('busyPanel')
        self.setStyleSheet(
            f'QFrame#busyPanel {{ background: {COLORS["chassis"]}; border-radius: 10px; }}'
            f'QLabel {{ color: #ffffff; font-size: 13px; }}')
        row = QtWidgets.QHBoxLayout(self)
        row.setContentsMargins(10, 8, 16, 8)
        row.setSpacing(10)
        # 방울은 패널이 보일 때 늘 돈다 — 패널 자체가 이미 300 ms 규칙으로 뜨므로 여기선 바로 켠다.
        self.drop = BusyDrop(size=self.DROP_PX)
        row.addWidget(self.drop, 0, QtCore.Qt.AlignmentFlag.AlignVCenter)
        column = QtWidgets.QVBoxLayout()
        column.setSpacing(6)
        column.addStretch(1)
        self.text = QtWidgets.QLabel('')
        self.bar = QtWidgets.QProgressBar()
        self.bar.setRange(0, 0)                # 무한 진행
        self.bar.setTextVisible(False)
        self.bar.setFixedHeight(6)
        column.addWidget(self.text)
        column.addWidget(self.bar)
        column.addStretch(1)
        row.addLayout(column, 1)
        self.setMinimumHeight(self.DROP_PX + 16)
        self.hide()
        self._started = 0.0
        self._what = ''
        self._estimate = 0.0
        self._pending = ''                     # want() 로 기다리는 작업 이름('' = 없음)
        self._timer = QtCore.QTimer(self)
        self._timer.setInterval(250)
        self._timer.timeout.connect(self._tick)
        self._delay = QtCore.QTimer(self)
        self._delay.setSingleShot(True)
        self._delay.timeout.connect(self._appear)

    # ── 상태 ──
    def want(self, what: str, estimate_s: float = 0.0) -> None:
        """이 작업이 돌고 있다 — 처음이면 `SHOW_DELAY_MS` 뒤에도 돌고 있을 때만 보인다(깜빡임 방지)."""
        if self.isVisible():
            self.start(what, estimate_s)
            return
        self._estimate = float(estimate_s or 0.0)
        if self._pending != what:
            self._pending = what
            self._started = time.monotonic()      # 경과는 처음 본 순간부터
            self._delay.start(SHOW_DELAY_MS)

    def _appear(self) -> None:
        if self._pending:
            what, started = self._pending, self._started
            self.start(what, self._estimate)
            self._started = started
            self._tick()

    def start(self, what: str, estimate_s: float = 0.0) -> None:
        self._pending = ''
        self._delay.stop()
        if self.isVisible() and what == self._what:
            estimate = float(estimate_s or self._estimate)
            if estimate != self._estimate:
                self._estimate = estimate
                self._tick()
            return
        was_visible = self.isVisible()
        self._what, self._estimate, self._started = what, float(estimate_s or 0.0), time.monotonic()
        self.show()
        self.raise_()
        self._tick()
        self._timer.start()
        self.drop.show()
        self.drop.start()
        if not was_visible:
            QtWidgets.QApplication.setOverrideCursor(QtCore.Qt.CursorShape.BusyCursor)

    def stop(self) -> None:
        self._pending = ''
        self._delay.stop()
        if not self.isVisible():
            return
        self._timer.stop()
        self.drop.stop()
        self.hide()
        QtWidgets.QApplication.restoreOverrideCursor()

    def _tick(self) -> None:
        elapsed = time.monotonic() - self._started
        text = f'{t(self._what)} · {t("경과")} {elapsed:.0f} s'
        if self._estimate > 0:
            remaining = max(0.0, self._estimate - elapsed)
            text += f' · {t("예상")} {self._estimate:.0f} s' + (f' ({t("남음")} {remaining:.0f} s)' if remaining >= 1 else '')
        self.text.setText(text)
        self.place()

    def place(self) -> None:
        """부모(단계 스택)의 오른쪽 아래 구석."""
        parent = self.parentWidget()
        if parent is None:
            return
        # adjustSize() 는 처음 뜰 때·글이 길어진 직후에 배치의 옛 크기 힌트를 써서 글이 잘렸다(09-24 QA
        # 캡처: 5단계 '예상 125 s') — 폭은 지금 글의 크기 힌트로 직접 더한다.
        margins = self.layout().contentsMargins()
        width = (margins.left() + self.DROP_PX + self.layout().spacing()
                 + self.text.sizeHint().width() + margins.right())
        height = max(self.minimumHeight(), margins.top() + self.DROP_PX + margins.bottom())
        self.resize(width, height)
        self.move(max(0, parent.width() - self.width() - 24), max(0, parent.height() - self.height() - 16))


Job = tuple[str, Callable[[], bool], Callable[[], float]]


def poll(panel: BusyPanel, jobs: list[Job]) -> str:
    """`jobs` 중 돌고 있는 첫 작업을 패널에 보인다(300 ms 규칙). 돌려주는 값은 그 작업 이름('' = 없음)."""
    for what, running, estimate in jobs:
        try:
            if running():
                try:
                    guess = float(estimate() or 0.0)
                except Exception:  # noqa: BLE001 - 예상은 없어도 된다
                    guess = 0.0
                panel.want(what, guess)
                return what
        except Exception:  # noqa: BLE001 - 상태 조회 실패는 '안 돎'
            continue
    panel.stop()
    return ''


__all__ = ['BusyPanel', 'poll']
