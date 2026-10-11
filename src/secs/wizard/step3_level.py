# -*- coding: utf-8 -*-
"""3단계 레벨 캘리브레이션 — 목업 `step3()`.

채널마다 ▶ 로 핑크 노이즈(서브는 사인)를 재생하고 입력 레벨 미터를 본다.
`LevelMonitor` 콜백은 워커 스레드에서 오므로 Qt 시그널로 넘겨 받는다.

`WizardWindow` 가 `levels`(`LevelMonitor`)를 주며 만들고 `refresh()`·`can_advance()`
를 부른다. 단계에 들어오고 나갈 때 `start_monitor()`/`stop_monitor()` 도 창이 부르고,
'레벨 건너뛰기' 는 `apply_defaults()` 로 온다. 값이 바뀌면 `changed` 를 낸다.
결과는 `project.extra['levels']`(role → dBFS)에, SPL 이 같이 오면
`project.extra['spl']` 에 저장한다.
"""
from __future__ import annotations

from PyQt6 import QtCore, QtGui
from secs.wizard import qt as QtWidgets

from secs.model import ordered_speakers, role_code, role_label
from secs.wizard.strings import t
from secs.wizard.tooltips import attach, attach_all, attach_cells, attach_header
from secs.wizard.style import (COLORS, ElidedLabel, add_heading, card, chip, clear_layout, label, pill,
                               set_state, set_variant, two_columns)


#: 목업의 기준 구간(dBFS).
TARGET_LOW = -30.0
TARGET_HIGH = -15.0
METER_MIN = -60.0
METER_MAX = 0.0

#: 미터 막대 높이와 그 아래 눈금 글자 높이(px). 눈금은 TICKS 의 dBFS 마다 세로줄.
BAR_H = 14
SCALE_H = 11
TICKS = (-60, -50, -40, -30, -20, -10, 0)

#: 한 채널을 재생하는 시간(ms).
PLAY_MS = 2500      # 한 채널 재생 길이. 1.2 s 는 너무 짧았다(09-22 사용자: 2초는 나와야) — 레벨이 자리 잡을 시간
PLAY_MS_TRIAL = 178  # 튜토리얼(데모)은 빠르게(10-10 사용자, 10-11 "1.5배 빠르게" 두 번 400 → 267 → 178) — 못 잰 채널은 순차 재생이 끝날 때 기준값으로 채운다


class LevelMeter(QtWidgets.QWidget):
    """목업 .meter — 기준 구간 띠 + 레벨 막대."""

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self.setFixedHeight(BAR_H + SCALE_H)   # 막대 + 아래 dB 눈금 글자(10-08 사용자: 몇 dB 인지 줄을)
        self.setMinimumWidth(90)        # 본문 칸이 1024 폭에서 406 px — 막대는 남는 폭을 쓴다(09-23 정렬)
        self.setSizePolicy(QtWidgets.QSizePolicy.Policy.Expanding,
                           QtWidgets.QSizePolicy.Policy.Fixed)
        self._value: float | None = None
        self._clip = False

    def set_value(self, dbfs: float | None, clip: bool = False) -> None:
        self._value = None if dbfs is None else float(dbfs)
        self._clip = bool(clip)
        self.update()

    def paintEvent(self, _event) -> None:  # noqa: N802 - Qt 이름
        painter = QtGui.QPainter(self)
        painter.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing, False)
        width, height = self.width(), BAR_H
        painter.fillRect(0, 0, width, height, QtGui.QColor(COLORS['meter_bg']))
        span = METER_MAX - METER_MIN
        zone_x = int((TARGET_LOW - METER_MIN) / span * width)
        zone_w = int((TARGET_HIGH - TARGET_LOW) / span * width)
        painter.fillRect(zone_x, 0, zone_w, height, QtGui.QColor(19, 127, 146, 56))
        if self._value is not None:
            ratio = max(0.0, min(1.0, (self._value - METER_MIN) / span))
            if self._clip or self._value > TARGET_HIGH:
                color = COLORS['bad']
            elif self._value >= TARGET_LOW:
                color = COLORS['accent']
            else:
                color = COLORS['ink2']
            painter.fillRect(0, 0, int(ratio * width), height, QtGui.QColor(color))
        # dB 눈금: 막대 위 세로줄 + 아래 숫자
        tick_pen = QtGui.QPen(QtGui.QColor(COLORS['ink2']))
        tick_pen.setWidth(1)
        font = painter.font()
        font.setPixelSize(9)
        painter.setFont(font)
        metrics = painter.fontMetrics()
        for db in TICKS:
            x = min(width - 1, int((db - METER_MIN) / span * width))
            painter.setPen(tick_pen)
            painter.drawLine(x, 0, x, height + 2)
            text = str(db)
            text_w = metrics.horizontalAdvance(text)
            text_x = max(0, min(width - text_w, x - text_w // 2))
            painter.drawText(text_x, height + SCALE_H - 1, text)
        painter.end()


class _LevelBridge(QtCore.QObject):
    """워커 스레드의 레벨 콜백을 GUI 스레드로 옮기는 다리."""

    level = QtCore.pyqtSignal(dict)
    #: 재생 명령이 REW 에서 실패했다(워커 스레드 → 화면 스레드). 값은 role.
    play_failed = QtCore.pyqtSignal(str)


class Step3Level(QtWidgets.QWidget):
    """3 · 레벨 체크(마이크 입력 점검 · REW 출력 레벨)."""

    changed = QtCore.pyqtSignal()

    def __init__(self, project, levels=None, parent=None, devices=None) -> None:
        super().__init__(parent)
        self.project = project
        self.levels = levels
        self.devices = devices          # REW 입력 채널 이름(마이크 입력 표시용). 없으면 번호로.
        # REW 로 보내는 명령(레벨 구독·제너레이터)은 한 줄 대기열의 워커 스레드에서 차례로 보낸다 —
        # 실제 REW 는 명령마다 응답을 기다려 단계를 드나들 때 화면이 0.3~0.6 s 멈췄다(09-23).
        # 프로세스 안 흉내(FakeREW)는 그 자리에서 부른다.
        from secs.wizard.rew_cache import SerialWorker, is_remote
        self._sender = SerialWorker(inline=not is_remote(levels), name='secs-rew-levels')
        self._rows: dict[str, tuple[LevelMeter, QtWidgets.QLabel, QtWidgets.QPushButton]] = {}
        self._playing: str | None = None
        self.trial = False               # 튜토리얼이면 순차 재생을 빠르게(창 `set_trial` 이 켠다)
        self._queue: list[str] = []
        self._monitoring = False
        self._bridge = _LevelBridge()
        self._bridge.level.connect(self._on_level)
        self._bridge.play_failed.connect(self._on_play_failed)
        self._play_timer = QtCore.QTimer(self)
        self._play_timer.setSingleShot(True)
        self._play_timer.timeout.connect(self._finish_play)
        self._build()
        self.refresh()

    # ──────────────────────────── 뼈대 ────────────────────────────
    def _build(self) -> None:
        outer = QtWidgets.QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(14)
        add_heading(outer, t('3 · 레벨 체크'))

        body, main, side = two_columns(outer)      # 단계 공통 2열(style.two_columns, 09-23 정렬)

        list_card, list_box = card(t('채널별 레벨'))
        # 머리글과 채널 줄은 `refresh()` 가 한 격자(`_build_grid`)에 같이 만든다 — 열 시작이 맞는다.
        self._rows_box = QtWidgets.QVBoxLayout()
        self._rows_box.setSpacing(0)
        list_box.addLayout(self._rows_box)
        list_box.addStretch(1)
        main.addWidget(list_card)

        side_card, side_box = card(t('마스터 출력'))
        self.master_label = label('REW 출력 레벨 -20.0 dBFS', 'num', wrap=True)   # 영어 1024 폭 가로 스크롤(09-23 QA A)
        side_box.addWidget(self.master_label)
        # 재생 중 표시(09-22 사용자: 재생되고 있음을 알 수 있게) — 재생 중인 채널 이름, 평소엔 숨김
        self.playing_pill = pill('', 'ok')
        # 숨겨도 자리는 지킨다 — [전 채널 자동 순차 재생] 때 배지가 끼어들어 아래가 30 px 밀렸다(10-10 3차 시험)
        policy = self.playing_pill.sizePolicy()
        policy.setRetainSizeWhenHidden(True)
        self.playing_pill.setSizePolicy(policy)
        self.playing_pill.setText(t('재생 중 · {0}').format('SW1'))      # 자리 높이를 잡을 글자(보이지 않음)
        self.playing_pill.setVisible(False)
        side_box.addWidget(self.playing_pill, 0, QtCore.Qt.AlignmentFlag.AlignLeft)
        self.master = QtWidgets.QSlider(QtCore.Qt.Orientation.Horizontal)
        self.master.setRange(-80, 0)  # 0.5 dB 단위를 위해 2배 스케일 사용
        self.master.setValue(-40)
        self.master.valueChanged.connect(self._on_master)
        attach(self.master, 's3.master')
        side_box.addWidget(self.master)
        self.sequence_button = QtWidgets.QPushButton(t('전 채널 자동 순차 재생'))
        set_variant(self.sequence_button, size='sm')
        self.sequence_button.clicked.connect(self._play_all)
        attach(self.sequence_button, 's3.sequence')
        side_box.addWidget(self.sequence_button)
        # 전에는 목업 문구 'UMIK-1 · 100 % (고정)' 이 그대로 떠 있었다(09-23) — 1단계에서 고른 입력을 보인다.
        side_box.addWidget(label(t('마이크 입력'), 'h3'))
        self.mic_label = label('', wrap=True)
        side_box.addWidget(self.mic_label)
        # 실시간 마이크 레벨 — 이 단계에 들어오면 자동으로 뜬다(10-08 사용자). 사이드바 미터 창과 같은 폴링.
        from secs.wizard.mic_meter import MicMeterWindow     # 순환 import 피해서 여기서
        self.mic_meter = MicMeterWindow(self.levels, parent=self, embedded=True)
        side_box.addWidget(self.mic_meter)
        side_box.addStretch(1)
        side.box.addWidget(side_card)
        outer.addStretch(1)

    # ──────────────────────────── 갱신 ────────────────────────────
    def master_db(self) -> float:
        return self.master.value() / 2.0

    def _on_master(self, _value: int) -> None:
        self.master_label.setText(f'REW 출력 레벨 {self.master_db():.1f} dBFS')

    def _mic_text(self) -> str:
        stored = dict(self.project.extra.get('devices', {}) or {})
        channel = stored.get('input_channel')
        if not channel:
            return t('1단계에서 입력 채널을 고르세요.')
        from secs.wizard.channels import input_label

        name = input_label(self.devices, int(channel))
        device = str(stored.get('input') or '')
        return f'{name} · {device}' if device else name

    def refresh(self) -> None:
        """채널 목록을 현재 구성으로 다시 만든다."""
        self.mic_label.setText(self._mic_text())
        clear_layout(self._rows_box)
        self._rows.clear()
        self._rows_box.addWidget(self._build_grid())

    #: 채널 줄 하나의 높이(px) — 모든 줄이 같다(재생 버튼 30 px + 위아래 여백).
    ROW_HEIGHT = 38
    #: 값 열 폭(px) — '-60.0' 이 들어가는 폭.
    VALUE_WIDTH = 52

    def _build_grid(self) -> QtWidgets.QWidget:
        """머리글 + 채널 줄을 한 격자로(목업 .lvl: 34px | 칩·이름 | 막대 1fr | 값 64px).

        2026-09-24 사용자: 칩 폭이 글자 수마다 달라 이름 시작이 들쭉날쭉하고, 긴 돌비 이름이 좁은
        칸에서 2~3줄로 접혀 줄 높이가 제각각, 머리글이 열과 안 맞았다. 이제 열마다 한 칸 —
        칩은 가장 넓은 칩 폭으로 고정, 이름은 한 줄(모자라면 가운데 `…` + 풍선 도움말),
        줄 높이는 `ROW_HEIGHT` 로 모두 같다. 격자는 구성이 바뀔 때마다 새로 만든다(빈 줄이
        최소 높이를 들고 남지 않게).
        """
        holder = QtWidgets.QWidget()
        grid = QtWidgets.QGridLayout(holder)
        grid.setContentsMargins(0, 0, 0, 0)
        grid.setHorizontalSpacing(10)
        grid.setVerticalSpacing(0)
        center = QtCore.Qt.AlignmentFlag.AlignVCenter
        left = QtCore.Qt.AlignmentFlag.AlignLeft | center
        right = QtCore.Qt.AlignmentFlag.AlignRight | center

        # 머리글(줄 0) — 각 열의 시작 x 에 맞춘다. 막대 열은 1024 폭에서 ≈90 px 라 머리글은 짧게 두고
        # '마이크 입력 기준' 은 풍선 도움말로(전 문구는 이 칸에서 세 줄로 접혔다, 2026-09-24).
        grid.addWidget(label('채널', 'note'), 0, 1, 1, 2, left)
        level_head = label('레벨', 'note')
        level_head.setToolTip('레벨 (dBFS, 마이크 입력)')
        level_head.setSizePolicy(QtWidgets.QSizePolicy.Policy.Ignored,
                                 QtWidgets.QSizePolicy.Policy.Preferred)
        grid.addWidget(level_head, 0, 3, left)
        value_head = label('dBFS', 'note')            # 10-10 AI 시험: 카드는 dBFS · 머리글 dB · 값은 단위 없음
        value_head.setAlignment(right)
        grid.addWidget(value_head, 0, 4, right)
        grid.addWidget(self._rule(), 1, 0, 1, 5)

        stored = dict(self.project.extra.get('levels', {}) or {})
        speakers = ordered_speakers(self.project.config.speakers)   # 역할 순서 — 나중에 넣은 SW2 · SW3 도 SW1 뒤(10-10)
        chips = [chip(speaker.role) for speaker in speakers]
        chip_width = max((item.minimumWidth() for item in chips), default=0)
        for index, (speaker, role_chip) in enumerate(zip(speakers, chips)):
            role = speaker.role
            out = speaker.bands[0].out if speaker.bands else 1
            row = 2 + index * 2
            play = QtWidgets.QPushButton('▶')
            play.setObjectName('iconbtn')
            play.setToolTip(f'{role} 재생')
            play.clicked.connect(lambda _=False, r=role, o=out: self.toggle_play(r, o))
            attach(play, 's3.play')
            role_chip.setFixedWidth(chip_width)
            role_chip.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
            name_text = ElidedLabel(role_label(role))
            name_text.setObjectName('note')
            name_text.setMinimumWidth(48)
            meter = attach(LevelMeter(), 's3.meter')
            value = label('—', 'num')
            value.setFixedWidth(self.VALUE_WIDTH)
            value.setAlignment(right)
            grid.addWidget(play, row, 0, center)
            grid.addWidget(role_chip, row, 1, left)
            grid.addWidget(name_text, row, 2, left)
            grid.addWidget(meter, row, 3, center)
            grid.addWidget(value, row, 4, right)
            grid.setRowMinimumHeight(row, self.ROW_HEIGHT)
            grid.addWidget(self._rule(), row + 1, 0, 1, 5)
            self._rows[role] = (meter, value, play)
            if role in stored:
                meter.set_value(float(stored[role]))
                value.setText(f'{float(stored[role]):.1f}')
        grid.setColumnMinimumWidth(0, 30)
        grid.setColumnMinimumWidth(4, self.VALUE_WIDTH)
        grid.setColumnStretch(3, 1)
        return holder

    @staticmethod
    def _rule() -> QtWidgets.QFrame:
        line = QtWidgets.QFrame()
        line.setObjectName('rowRule')
        line.setFixedHeight(1)
        return line

    # ──────────────────────────── 모니터링 ────────────────────────────
    def start_monitor(self) -> None:
        """입력 레벨 구독을 켠다. 창이 단계에 들어올 때, 그리고 `play()` 가 부른다."""
        if self._monitoring or self.levels is None:
            return
        self._monitoring = True

        def failed(_error: Exception) -> None:      # 장치가 없으면 미터만 비어 있다
            self._monitoring = False

        self._sender.submit(self.levels.start, self._bridge.level.emit, on_error=failed)

    def stop_monitor(self, wait: bool = False) -> None:
        """제너레이터와 레벨 구독을 끄고 재생 대기열도 비운다.

        단계를 떠날 때(기다리지 않는다)와 창을 닫을 때(`wait=True` — 보낸 뒤 닫는다) 창이 부른다.
        """
        if self.levels is None:
            return
        self._sender.submit(self.levels.stop_generator)
        if self._monitoring:
            self._sender.submit(self.levels.stop)
            self._monitoring = False
        if wait:
            self._sender.drain(3.0)
        # 재생 타이머와 잠근 버튼도 정리한다 — 안 하면 단계를 떠난 뒤 타이머가 울려
        # 다음 채널 재생을 다시 시작할 수 있었다(실사용 QA 09-19, QA-B).
        self._play_timer.stop()
        for _name, (_meter, _value, button) in self._rows.items():
            button.setEnabled(True)
        self._playing = None
        self._show_playing(None)
        self._queue = []

    def toggle_play(self, role: str, output: int) -> None:
        """▶ 버튼: 재생 시작. 재생 중인 채널의 ■ 를 다시 누르면 바로 멈춘다(대기열도 비운다)."""
        if self._playing == role:
            self._queue = []
            self._play_timer.stop()
            self._finish_play()
            return
        self.play(role, output)

    def play(self, role: str, output: int) -> None:
        """한 채널을 재생하고 그동안 들어오는 레벨을 그 채널에 기록한다."""
        self.start_monitor()
        self._playing = role
        if self.levels is not None:
            failed = []
            self._sender.submit(self.levels.play_pink, role, int(output), self.master_db(),
                                on_error=lambda _e, r=role: (failed.append(r) if self._sender.inline
                                                             else self._bridge.play_failed.emit(r)))
            if failed:              # 그 자리에서 부르는 흉내면 실패를 바로 안다
                self._playing = None
                return
        for name, (_meter, _value, button) in self._rows.items():
            button.setEnabled(name == role)          # 재생 중인 채널의 버튼만 살려 두고(■ = 멈춤) 나머지는 잠근다
        self._show_playing(role)
        self._play_timer.start(PLAY_MS_TRIAL if self.trial else PLAY_MS)

    def _show_playing(self, role: str | None) -> None:
        """재생 중 표시: 그 줄의 버튼은 ■ 로, 배지에 채널 이름."""
        for name, (_meter, _value, button) in self._rows.items():
            playing = role is not None and name == role
            button.setText('■' if playing else '▶')
            set_state(button, 'on' if playing else '')
        if role is None:
            self.playing_pill.setVisible(False)
        else:
            self.playing_pill.setText(t('재생 중 · {0}').format(role_code(role)))
            self.playing_pill.setVisible(True)

    @QtCore.pyqtSlot(str)
    def _on_play_failed(self, role: str) -> None:
        """워커가 보낸 재생 명령이 실패했다 — 그 채널의 재생 표시를 거둔다(대기열은 이어 간다)."""
        if self._playing == role:
            self._play_timer.stop()
            self._finish_play()

    def _finish_play(self) -> None:
        if self.levels is not None:
            self._sender.submit(self.levels.stop_generator)
        for _name, (_meter, _value, button) in self._rows.items():
            button.setEnabled(True)
        self._playing = None
        self._show_playing(None)
        self.changed.emit()
        if self.trial and not self._queue:
            self.apply_defaults()               # 튜토리얼은 짧게 돌아 레벨이 안 들어온 채널이 있을 수 있다
        if self._queue:
            role = self._queue.pop(0)
            speaker = next((s for s in self.project.config.speakers if s.role == role), None)
            out = speaker.bands[0].out if speaker and speaker.bands else 1
            self.play(role, out)

    def _play_all(self) -> None:
        speakers = ordered_speakers(self.project.config.speakers)      # 표와 같은 순서로 차례 재생
        if not speakers:
            return
        self._queue = [s.role for s in speakers[1:]]
        speaker = speakers[0]
        self.play(speaker.role, speaker.bands[0].out if speaker.bands else 1)

    @QtCore.pyqtSlot(dict)
    def _on_level(self, payload: dict) -> None:
        role = self._playing
        if role is None or role not in self._rows:
            return
        meter, value, _button = self._rows[role]
        dbfs = float(payload.get('input_dbfs', METER_MIN))
        meter.set_value(dbfs, bool(payload.get('clip')))
        value.setText(f'{dbfs:.1f}')
        levels = dict(self.project.extra.get('levels', {}) or {})
        levels[role] = round(dbfs, 2)
        spl = payload.get('spl_db')
        if spl is not None:
            spl_map = dict(self.project.extra.get('spl', {}) or {})
            spl_map[role] = round(float(spl), 2)
            self.project.extra['spl'] = spl_map
        self.project.extra['levels'] = levels

    def apply_defaults(self) -> None:
        """목업의 '레벨 건너뛰기 (기본값 적용)'.

        아직 못 잰 채널만 기준 구간 안의 값으로 채운다(`setdefault`) — 이미 잰
        채널의 실측값은 그대로 둔다.
        """
        levels = dict(self.project.extra.get('levels', {}) or {})
        for speaker in self.project.config.speakers:
            base = -22.0 - (len(speaker.role) * 7 % 6)
            levels.setdefault(speaker.role, round(base, 2))
            meter, value, _button = self._rows.get(speaker.role, (None, None, None))
            if meter is not None:
                meter.set_value(levels[speaker.role])
                value.setText(f'{levels[speaker.role]:.1f}')
        self.project.extra['levels'] = levels
        self.changed.emit()

    # ──────────────────────────── 진행 조건 ────────────────────────────
    def can_advance(self) -> tuple[bool, str]:
        levels = dict(self.project.extra.get('levels', {}) or {})
        missing = [s.role for s in ordered_speakers(self.project.config.speakers) if s.role not in levels]
        if missing:
            shown = ', '.join(role_code(r) for r in missing[:4])
            if len(missing) > 4:            # 앞 4개만 보이면 그게 전부인 줄 알았다(10-10 3차 시험: 12개 중 4개)
                return False, f'레벨을 아직 못 잰 채널: {shown} 외 {len(missing) - 4}개'
            return False, f'레벨을 아직 못 잰 채널: {shown}'
        return True, ''


__all__ = ['Step3Level', 'LevelMeter', 'TARGET_LOW', 'TARGET_HIGH']
