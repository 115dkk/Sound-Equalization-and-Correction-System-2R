# -*- coding: utf-8 -*-
"""마이크 입력 미터 창 — 측정하지 않을 때도 마이크가 들어오는지 본다(10-07 사용자 요청).

사이드바 [마이크 미터] 또는 Ctrl+M 으로 여는 작은 창(떠 있음, 다른 단계로 가도 그대로).
REW 의 `GET /input-levels/last-levels` 를 워커 스레드에서 0.25 s 마다 읽는다(구독이 아니라 폴링 —
3단계의 구독과 겹치지 않는다). REW 의 API 레벨 모니터는 REW 화면 미터와 따로 돌고, 다른 일
(발생기·측정·3단계를 떠날 때의 Stop)로 멈추면 값이 그대로 굳는다(10-07: SECS 에만 레벨이 안 뜸).
같은 값이 `STALE_POLLS` 번 이어지면 멈춘 것으로 보고 Start 를 다시 보낸다.
"""
from __future__ import annotations

import threading

from PyQt6 import QtCore
from secs.wizard import qt as QtWidgets

from secs.wizard.step3_level import LevelMeter
from secs.wizard.strings import t
from secs.wizard.tooltips import attach
from secs.wizard.style import label, set_variant

POLL_S = 0.25
#: 같은 본문이 이만큼 이어지면 REW 모니터가 멈춘 것으로 본다(실값은 매번 조금씩 바뀐다).
STALE_POLLS = 4


class _Bridge(QtCore.QObject):
    level = QtCore.pyqtSignal(dict)       # {'input_dbfs','peak','clip'} 또는 {'offline': True}


class MicMeterWindow(QtWidgets.QWidget):
    """마이크 입력 레벨 · 피크 · 클립 · REW 수신 상태."""

    def __init__(self, levels, parent=None, embedded: bool = False) -> None:
        # embedded=True: 3단계 '마스터 출력' 카드 안에 붙는 미터(10-08 사용자: 레벨 탭에선 자동으로 떠야)
        if embedded:
            super().__init__(parent)
        else:
            super().__init__(parent, QtCore.Qt.WindowType.Tool | QtCore.Qt.WindowType.WindowStaysOnTopHint)
        self.embedded = embedded
        self.levels = levels
        self.setWindowTitle(t('마이크 입력 미터'))
        self.setMinimumWidth(300)
        self._bridge = _Bridge()
        self._bridge.level.connect(self._on_level)
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._peak_hold: float | None = None

        box = QtWidgets.QVBoxLayout(self)
        box.setContentsMargins(*((0, 0, 0, 0) if embedded else (14, 12, 14, 12)))
        box.setSpacing(6)
        self.name = label('—', 'note')
        self.meter = LevelMeter()
        self.value = label('—', 'num' if embedded else 'sideValue')   # sideValue 는 밝은 글자 — 흰 카드에선 안 보였다
        self.peak = label('—', 'note')
        self.state = label('—', 'note', wrap=True)      # 튜토리얼 안내가 300 px 칸을 넘었다(10-10 레이아웃 시험) — 줄바꿈
        row = QtWidgets.QHBoxLayout()
        row.addWidget(self.meter, 1)
        row.addWidget(self.value)
        box.addWidget(self.name)
        box.addLayout(row)
        box.addWidget(self.peak)
        box.addWidget(self.state)
        reset = QtWidgets.QPushButton(t('피크 지우기'))
        reset.clicked.connect(self._reset_peak)
        attach(reset, 'mic.reset_peak')
        if embedded:                       # 카드 안에선 장치 이름은 위 '마이크 입력' 줄이 이미 보인다
            self.name.setVisible(False)
            set_variant(reset, 'ghost', 'sm')
        box.addWidget(reset)

    # ──────────────────────────── 창 열고 닫기 ────────────────────────────
    def showEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().showEvent(event)
        if self._thread is None or not self._thread.is_alive() or self._stop.is_set():
            self._stop = threading.Event()        # 멈춘 옛 스레드와 깃발을 나눠 쓰지 않게 새로
            self._thread = threading.Thread(target=self._poll, name='secs-mic-meter', daemon=True)
            self._thread.start()

    def closeEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        self._stop.set()
        super().closeEvent(event)

    def hideEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        # 붙은 미터는 단계를 떠나면(화면이 숨으면) 폴링을 멈춘다. 떠 있는 창은 최소화돼도 계속.
        if self.embedded:
            self._stop.set()
        super().hideEvent(event)

    def _reset_peak(self) -> None:
        self._peak_hold = None
        self.peak.setText('—')

    # ──────────────────────────── 워커 스레드 ────────────────────────────
    def _poll(self) -> None:
        stop = self._stop            # 이 스레드의 깃발 — 다시 보이면 새 스레드가 새 깃발을 쓴다
        levels = self.levels
        if levels is None or not hasattr(levels, 'last_levels'):
            # 튜토리얼의 데모 REW 는 레벨 미터가 없다 — '응답 없음' 이라 하면 앞의 'API 연결됨' 과 어긋난다(10-10 AI 시험)
            fake = type(levels).__name__.startswith('Fake')
            self._bridge.level.emit({'fake': True} if fake else {'offline': True})
            return
        try:
            name = levels.get('/audio/asio/input', timeout=2.0).get('input')
        except Exception:  # noqa: BLE001 - 이름은 없어도 된다
            name = None
        if name:
            self._bridge.level.emit({'name': str(name)})
        last, same = None, 0
        while not stop.is_set():
            data = levels.last_levels()
            if not data:
                same = STALE_POLLS            # 모니터를 켠 적이 없다 → 바로 Start
            else:
                key = (tuple(data.get('rms') or ()), tuple(data.get('peak') or ()))
                same = same + 1 if key == last else 0
                last = key
            if same >= STALE_POLLS:
                try:
                    levels.post('/input-levels/command', {'command': 'Start'})
                except Exception:  # noqa: BLE001 - REW 가 꺼져 있으면 아래에서 offline 표시
                    pass
                same = 0
                self._bridge.level.emit({'stale': True})
            elif data:
                parsed = levels._level_payload(data, getattr(levels, 'level_index', 0))
                peak = None
                peaks = data.get('peak') or []
                if peaks:
                    peak = float(peaks[min(getattr(levels, 'level_index', 0), len(peaks) - 1)])
                self._bridge.level.emit({'input_dbfs': parsed['input_dbfs'], 'peak': peak,
                                         'clip': parsed['clip']})
            else:
                self._bridge.level.emit({'offline': True})
            stop.wait(POLL_S)

    # ──────────────────────────── 화면 스레드 ────────────────────────────
    def _on_level(self, payload: dict) -> None:
        if 'name' in payload:
            self.name.setText(payload['name'])
            return
        if payload.get('fake'):
            self.meter.set_value(None)
            self.value.setText('—')
            self.state.setText(t('● 튜토리얼 — 마이크 레벨은 실제 REW 에서만 보입니다'))
            return
        if payload.get('offline'):
            self.meter.set_value(None)
            self.value.setText('—')
            self.state.setText(t('● REW 응답 없음 — REW 를 -api 로 켜 주세요'))
            return
        if payload.get('stale'):
            self.state.setText(t('● REW 레벨 모니터 다시 시작 중…'))
            return
        dbfs = float(payload['input_dbfs'])
        shown = max(dbfs, -120.0)
        self.meter.set_value(shown, payload.get('clip', False))
        self.value.setText('{0:.1f} dBFS'.format(shown))
        peak = payload.get('peak')
        if peak is not None and peak > -200:
            self._peak_hold = peak if self._peak_hold is None else max(self._peak_hold, peak)
        if self._peak_hold is not None:
            clip = t('클립!') if self._peak_hold > -1.0 else t('클립 없음')
            self.peak.setText(t('피크 {0:.1f} dBFS · {1}').format(self._peak_hold, clip))
        self.state.setText(t('● REW 수신 중'))


__all__ = ['MicMeterWindow']
