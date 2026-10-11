# -*- coding: utf-8 -*-
"""1단계 장치·마이크 — 목업 `step2()`(2026-09-23 사용자 지시로 첫 단계가 됐다: REW 연결이 먼저).

REW 상태 배지(실행 중·버전·Pro)와 재시도, 출력/입력 장치 선택, 입력 채널,
스윕 길이. 타이밍 레퍼런스는 구성의 스피커 목록이 필요해 4단계 '측정 옵션'으로 옮겼다(`wizard.timing`). 실행 파일 경로·캘리브레이션 파일·측정 레벨은 목업의
고정 문구 대신 REW 에서 실제로 읽는다(2026-09-23, `rewapi.rew.RealREW.measure_level_dbfs`·
`input_calibration_file`, `find_rew_exe`) — 읽을 수 없으면 '알 수 없음'.

`WizardWindow` 가 `rew`·`devices` 자리에 `rew_cache.RewCache` 를 주며 만들고 `refresh()`
와 `can_advance()` 를 부른다 — 화면 스레드는 캐시만 읽고 REW 에 직접 묻지 않는다(09-23 반응 속도).
[다시 확인]·[설정 읽기]·장치 목록 재시도는 캐시에 새로 읽기를 부탁하고, 끝나면 다시 그린다
(REW 객체를 그대로 주면 이 화면이 캐시로 감싼다 — 프로세스 안 흉내면 캐시는 그대로 통과시킨다). 바뀌면 `changed` 를 낸다. 선택은
`project.config.device` 와 `project.extra['devices']`(output · input ·
input_channel · sweep_length · sample_rate)에 저장하고(같은 사전의 타이밍 키는 4단계 몫이라
그대로 둔다), 5단계의 `MeasurementPlan` 이 그 값을 읽는다.
"""
from __future__ import annotations

import os
import threading

from PyQt6 import QtCore
from secs.wizard import qt as QtWidgets

from secs.rewapi.interfaces import SWEEP_LENGTHS, sweep_length_samples
# rew_process_running 은 이 모듈에서 부르지 않고 `RewCache` 가 이 이름으로 찾아 부른다(테스트가 바꿔 끼운다).
from secs.rewapi.rew import find_rew_exe, rew_process_running  # noqa: F401
from secs.wizard.channels import input_channel_names
from secs.wizard.loading import BusyDrop
from secs.wizard.timing import TIMING_KEYS
from secs.wizard.rew_cache import RewCache
from secs.wizard.strings import t
from secs.wizard.tooltips import attach, attach_all, attach_cells, attach_header
from secs.wizard.style import (add_heading, card, help_box, label, pill, repolish,
                               set_variant, two_columns)


#: 샘플레이트를 모를 때 쓰는 기본값(REW 기본).
DEFAULT_SAMPLE_RATE = 48000


def sweep_seconds(length: str, sample_rate: int = DEFAULT_SAMPLE_RATE) -> float | None:
    """스윕 길이의 재생 시간(초). 샘플 수 ÷ 샘플레이트, 모르는 길이면 None.

    `SWEEP_LENGTHS` 가 REW 실목록(64k~4M)이라 표를 손으로 들고 있지 않고 계산한다.
    44.1 kHz 로 재면 그만큼 길어진 값이 나온다.
    """
    try:
        samples = sweep_length_samples(length)
    except (TypeError, ValueError):
        return None
    rate = float(sample_rate or DEFAULT_SAMPLE_RATE)
    return samples / rate if rate > 0 else None


#: 목업 step2 의 스윕 길이 표기(길이 → 초, 48 kHz 기준). 64k 1.4 s ~ 4M 87.4 s.
#: 화면은 이 표가 아니라 `sweep_seconds()` 를 현재 샘플레이트로 부른다 — 이 표는
#: 48 kHz 기준값을 못박아 두는 테스트 전용이다.
SWEEP_SECONDS: dict[str, float] = {
    length: round(sweep_seconds(length) or 0.0, 1) for length in SWEEP_LENGTHS
}


def fill_sweep_combo(combo: QtWidgets.QComboBox, sample_rate: int = DEFAULT_SAMPLE_RATE,
                     selected: str | None = None) -> None:
    """스윕 길이 콤보를 ``길이 · 초`` 항목으로 채우고 저장값을 고른다.

    1단계와 5단계가 같은 함수로 항목을 만들어 표시·저장값이 어긋나지 않게 한다.
    """
    wanted = combo.currentData() if selected is None else selected
    combo.blockSignals(True)
    combo.clear()
    for length in SWEEP_LENGTHS:
        seconds = sweep_seconds(length, sample_rate)
        combo.addItem(f'{length} · {seconds:.1f} s' if seconds else length, length)
    index = combo.findData(wanted or '256k')
    combo.setCurrentIndex(index if index >= 0 else max(0, combo.findData('256k')))
    combo.blockSignals(False)


class ChannelCombo(QtWidgets.QComboBox):
    """채널 번호를 고르는 콤보 — 항목은 REW 의 풀 네임("3: Mic/Line/Inst In 1"), 값은 1-기준 번호.

    예전 QSpinBox 자리에 그대로 들어가도록 `value()`/`setValue()`/`valueChanged` 를 같은 뜻으로 둔다.
    """

    valueChanged = QtCore.pyqtSignal(int)

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self.setSizeAdjustPolicy(QtWidgets.QComboBox.SizeAdjustPolicy.AdjustToMinimumContentsLengthWithIcon)
        self.setMinimumContentsLength(14)
        self.currentIndexChanged.connect(lambda _index: self.valueChanged.emit(self.value()))

    def set_names(self, names: list[str], fallback: str = 'In {0}', count: int = 8) -> None:
        """항목을 다시 채운다. 이름이 없으면 `fallback.format(번호)` 로 `count` 개."""
        keep = self.value()
        self.blockSignals(True)
        self.clear()
        items = list(names) or [fallback.format(n) for n in range(1, max(1, int(count)) + 1)]
        for number, name in enumerate(items, start=1):
            self.addItem(str(name), number)
        self.blockSignals(False)
        self.setValue(keep or 1)

    def value(self) -> int:
        data = self.currentData()
        return int(data) if data is not None else 0

    def setValue(self, number: int) -> None:  # noqa: N802 - QSpinBox 호환
        number = int(number)
        index = self.findData(number)
        if index < 0 and number >= 1:          # 목록에 없는 번호(저장값이 더 클 때)는 항목을 만들어 둔다
            self.addItem(f'{number}', number)
            index = self.findData(number)
        if index >= 0 and index != self.currentIndex():
            self.setCurrentIndex(index)


class Step1Devices(QtWidgets.QWidget):
    """2 · 장치 · 마이크."""

    changed = QtCore.pyqtSignal()
    #: REW 실행 스레드가 끝났다(성공 여부, 안내 문구). 화면 스레드에서 받는다.
    launchFinished = QtCore.pyqtSignal(bool, str)

    #: REW 가 뜨고 API 가 응답할 때까지 기다리는 최대 시간(초). REW 는 보통 10~20 초.
    LAUNCH_WAIT_S = 40.0
    #: 상태 배지 옆 방울 크기 — 표 한 줄 높이에 맞춘다(48 px 이면 줄 사이가 벌어진다).
    DROP_PX = 32

    def __init__(self, project, rew=None, devices=None, parent=None) -> None:
        super().__init__(parent)
        self.project = project
        if not isinstance(rew, RewCache):
            rew = RewCache(rew, devices)
        self.rew = rew
        self.devices = rew                  # 장치·채널 목록도 캐시에서 읽는다
        self._building = False
        self._launching = False
        self._checking = False
        self.launchFinished.connect(self._on_launch_finished)
        # REW 가 막 뜨면 /version 은 답해도 오디오 장치 목록은 잠깐 비어 있다 → 채워질 때까지 2 초마다 다시 읽는다
        # (09-22 사용자: REW 를 열어도 [다시 확인] 을 누르기 전엔 마이크·오디오가 안 뜬다).
        self._device_retry = QtCore.QTimer(self)
        self._device_retry.setInterval(2000)
        self._device_retry.timeout.connect(self._retry_devices)
        self._device_retry_left = 0
        self.sample_rate = int(
            (project.extra.get('devices', {}) or {}).get('sample_rate')
            or DEFAULT_SAMPLE_RATE)
        self._build()
        self.refresh()

    # ──────────────────────────── 스윕 길이 ────────────────────────────
    def _fill_sweep_combo(self) -> None:
        """`SWEEP_LENGTHS` 를 '길이 · 초' 로 채운다(현재 샘플레이트 기준)."""
        fill_sweep_combo(self.sweep_combo, self.sample_rate)

    def set_sample_rate(self, rate: int) -> None:
        """샘플레이트가 바뀌면 표시와 스윕 시간 표기를 같이 고친다."""
        rate = int(rate or DEFAULT_SAMPLE_RATE)
        if rate == self.sample_rate:
            return
        self.sample_rate = rate
        self.rate_label.setText(f'{rate} Hz')
        self._fill_sweep_combo()
        self._store()

    # ──────────────────────────── 뼈대 ────────────────────────────
    def _build(self) -> None:
        outer = QtWidgets.QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(14)
        add_heading(outer, t('1 · 장치·마이크'))

        body, main, side = two_columns(outer)      # 단계 공통 2열(style.two_columns, 09-23 정렬)

        rew_card, rew_box = card(t('REW 연결'))
        grid = QtWidgets.QGridLayout()
        grid.setHorizontalSpacing(12)
        grid.setVerticalSpacing(6)
        self.status_pill = pill(t('미연결'), 'bad')
        # 값은 refresh() 가 find_rew_exe() 로 다시 읽는다 — 목업의 고정 경로가 아니다(09-23).
        self.path_label = label('—', 'num', wrap=True)
        self.version_label = label('—', 'num')
        self.pro_pill = pill('연결 후 확인', 'na')
        self.blocking_label = label('—', 'num')
        # 상태 배지 옆 작은 방울 — REW 연결 확인·장치 목록 새로 읽기·REW 띄우기 동안 돈다(09-24 사용자:
        # "딜레이를 느낄 만한 지점엔 다 애니메이션"). 자리는 늘 지켜 배지가 흔들리지 않는다.
        self.busy_drop = BusyDrop(size=self.DROP_PX, keep_space=True)
        status_row = QtWidgets.QWidget()
        status_box = QtWidgets.QHBoxLayout(status_row)
        status_box.setContentsMargins(0, 0, 0, 0)
        status_box.setSpacing(8)
        status_box.addWidget(self.status_pill, 0, QtCore.Qt.AlignmentFlag.AlignVCenter)
        status_box.addWidget(self.busy_drop, 0, QtCore.Qt.AlignmentFlag.AlignVCenter)
        status_box.addStretch(1)
        for row, (name, widget) in enumerate((
                ('상태', status_row), ('실행 파일', self.path_label),
                ('버전', self.version_label), ('Pro 라이선스', self.pro_pill),
                ('blocking 모드', self.blocking_label))):
            grid.addWidget(label(name), row, 0)
            if widget is self.path_label or widget is status_row:
                # 긴 경로는 줄바꿈해 칸 폭을 다 쓴다 — 1024 폭 가로 스크롤 방지(실사용 QA 09-19).
                grid.addWidget(widget, row, 1)
            else:
                grid.addWidget(widget, row, 1, QtCore.Qt.AlignmentFlag.AlignLeft)
        grid.setColumnStretch(1, 1)
        rew_box.addLayout(grid)

        buttons = QtWidgets.QHBoxLayout()
        buttons.setSpacing(8)
        self.retry_button = QtWidgets.QPushButton(t('REW API 모드로 실행'))
        set_variant(self.retry_button, 'accent', 'sm')
        self.retry_button.clicked.connect(self.recheck)
        self.read_button = QtWidgets.QPushButton(t('설정 읽기'))
        set_variant(self.read_button, 'ghost', 'sm')
        self.read_button.clicked.connect(self._read_settings)
        attach(self.retry_button, 's1.rew_launch')
        attach(self.read_button, 's1.read_settings')
        buttons.addWidget(self.retry_button)
        buttons.addWidget(self.read_button)
        buttons.addStretch(1)
        rew_box.addLayout(buttons)
        self.rew_hint = help_box('', bad=True)
        rew_box.addWidget(self.rew_hint)
        rew_box.addStretch(1)
        main.addWidget(rew_card)

        mic_card, mic_box = card(t('마이크 · 오디오'))
        mic_grid = QtWidgets.QGridLayout()
        mic_grid.setHorizontalSpacing(12)
        mic_grid.setVerticalSpacing(6)
        self.input_combo = QtWidgets.QComboBox()
        self.input_combo.currentIndexChanged.connect(self._store)
        self.input_channel = ChannelCombo()          # 항목 = REW 입력 이름, 값 = 번호
        self.input_channel.valueChanged.connect(self._store)
        # 값은 refresh() 가 REW 의 GET /audio/input-cal 로 다시 읽는다(09-23) — 못 읽으면 '알 수 없음'.
        self.cal_pill = pill(t('알 수 없음'), 'na')
        self.output_combo = QtWidgets.QComboBox()
        self.output_combo.currentIndexChanged.connect(self._store)
        self.rate_label = label(f'{self.sample_rate} Hz', 'num')
        self.sweep_combo = QtWidgets.QComboBox()
        self._fill_sweep_combo()
        self.sweep_combo.setCurrentIndex(max(0, self.sweep_combo.findData('256k')))
        self.sweep_combo.currentIndexChanged.connect(self._store)

        attach(self.input_combo, 's1.input_device')
        attach(self.input_channel, 's1.input_channel')
        attach(self.output_combo, 's1.output_driver')
        attach(self.sweep_combo, 's1.sweep')
        # 긴 항목(장치 이름)이 카드 최소 폭을 밀지 않게 — 좁은 창에서 가로 스크롤 방지.
        for combo in (self.input_combo, self.output_combo, self.sweep_combo):
            combo.setSizeAdjustPolicy(
                QtWidgets.QComboBox.SizeAdjustPolicy.AdjustToMinimumContentsLengthWithIcon)
            combo.setMinimumContentsLength(14)
            # 남는 폭은 칸이 받는다 — 전엔 이름 열이 남는 폭을 가져가 칸이 164 px 에 묶여 장치 이름이 잘렸다(10-10 3차 시험)
            combo.setSizePolicy(QtWidgets.QSizePolicy.Policy.Expanding, QtWidgets.QSizePolicy.Policy.Fixed)
        self.input_channel.setSizePolicy(QtWidgets.QSizePolicy.Policy.Expanding, QtWidgets.QSizePolicy.Policy.Fixed)

        for row, (name, widget) in enumerate((
                ('입력 장치', self.input_combo), ('입력 채널', self.input_channel),
                ('캘리브레이션', self.cal_pill), ('출력 드라이버', self.output_combo),
                ('샘플레이트', self.rate_label), ('스윕 길이', self.sweep_combo))):
            mic_grid.addWidget(label(name), row, 0)
            mic_grid.addWidget(widget, row, 1)
        mic_grid.setColumnStretch(1, 1)
        mic_box.addLayout(mic_grid)
        mic_box.addStretch(1)
        side.box.addWidget(mic_card)

        # 09-22: 다음 단계 조건 설명은 화면에서 뺐다 — 도움말(F1)에 있고, 진행 불가 이유는
        # 다음 버튼 옆 내비 줄이 보여 준다(can_advance()).
        outer.addStretch(1)

    # ──────────────────────────── 갱신 ────────────────────────────
    def refresh(self) -> None:
        self._building = True
        try:
            self._refresh_status()
            self._refresh_devices()
        finally:
            self._building = False

    def recheck(self) -> None:
        """[REW API 모드로 실행] / [다시 확인] 버튼.

        REW 가 응답하면 상태·장치 목록만 다시 읽고, 응답하지 않으면 REW 를 `-api` 로
        실제로 띄운다(09-22 전까지는 이름과 달리 다시 묻기만 해서 "버튼이 안 먹힌다"고 보였다).
        """
        if self._launching or self._checking:
            return
        self._checking = True
        self.retry_button.setEnabled(False)
        self.poll_busy()

        def after(snap) -> None:
            self._checking = False
            self.retry_button.setEnabled(True)
            self.refresh()
            if snap.running:
                self.changed.emit()
                return
            self._launch_rew()

        # REW 에 실제로 묻는 것은 캐시의 워커 스레드다 — 기다리는 동안 화면은 멈추지 않는다.
        self.rew.request_refresh(then=after)

    def working(self) -> bool:
        """REW 에 묻거나 REW 를 띄우는 중인가(상태 배지 옆 방울 — 창의 250 ms 점검이 본다)."""
        busy = getattr(self.rew, 'busy', None)
        return bool(self._checking or self._launching
                    or (callable(busy) and busy()) or not self.read_button.isEnabled())

    def poll_busy(self) -> None:
        self.busy_drop.set_busy(self.working())

    def _launch_rew(self) -> None:
        """REW 를 -api 로 띄운다. 기다리는 동안 화면이 멈추지 않게 스레드에서 한다."""
        if self.rew.process_running():
            # 켜져 있는데 API 가 없다 = -api 없이 켠 것. 새로 하나 더 띄우면 두 개가 된다.
            self._show_hint('REW 가 이미 켜져 있지만 API 모드가 아닙니다. REW 를 닫은 뒤 '
                            '이 버튼을 다시 누르면 -api 옵션으로 띄웁니다.', bad=True)
            return
        if not self.rew.can_launch():
            self._show_hint('이 실행 모드에서는 REW 를 띄울 수 없습니다. REW 를 -api 옵션으로 '
                            '직접 실행한 뒤 [다시 확인] 을 누르세요.', bad=True)
            return
        if find_rew_exe() is None:
            self._show_hint('REW 실행 파일을 찾지 못했습니다(C:\\Program Files\\REW). REW 를 '
                            '-api 옵션으로 직접 실행한 뒤 [다시 확인] 을 누르세요.', bad=True)
            return
        self._launching = True
        self.retry_button.setEnabled(False)
        self.retry_button.setText(t('REW 실행 중…'))
        self._show_hint('REW 를 API 모드로 띄우는 중입니다. 창이 뜨고 응답할 때까지 '
                        '10~20 초 걸립니다.', bad=False)

        def work() -> None:
            try:
                ok = bool(self.rew.launch(wait_s=self.LAUNCH_WAIT_S))
            except Exception as error:  # noqa: BLE001 - 실패 이유를 화면에 보여 준다
                self.launchFinished.emit(False, f'REW 실행 실패: {error}')
                return
            self.launchFinished.emit(ok, '' if ok else 'REW 가 시간 안에 응답하지 않았습니다. '
                                     'REW 창이 떴으면 [다시 확인] 을 누르세요.')

        threading.Thread(target=work, name='secs-rew-launch', daemon=True).start()

    @QtCore.pyqtSlot(bool, str)
    def _on_launch_finished(self, ok: bool, message: str) -> None:
        self._launching = False
        self.retry_button.setEnabled(True)
        self.refresh()
        if not ok and message:
            self._show_hint(message, bad=True)
        self.changed.emit()

    def _retry_devices(self) -> None:
        """장치 목록이 빈 동안 다시 읽는다. 채워지거나 횟수가 다하면 멈춘다."""
        self._device_retry_left -= 1
        if self._device_retry_left <= 0:
            self._device_retry.stop()
        self.rew.request_refresh(then=lambda _snap: self.refresh())

    def _show_hint(self, text: str, bad: bool) -> None:
        self.rew_hint.setObjectName('helpBad' if bad else 'help')
        repolish(self.rew_hint)
        self.rew_hint.setText(t(text))
        self.rew_hint.setVisible(True)

    def _status(self) -> tuple[bool, str | None, bool]:
        # 캐시 값 — REW 에 묻지 않는다(원격이면 마지막으로 읽은 값, 흉내면 바로 읽은 값).
        return self.rew.is_running(), self.rew.version(), self.rew.is_pro()

    def _refresh_status(self) -> None:
        running, version, pro = self._status()
        self.status_pill.setText('API 연결됨 · 127.0.0.1:4735' if running else t('미연결'))
        self.status_pill.setProperty('tone', 'ok' if running else 'bad')
        repolish(self.status_pill)
        self.version_label.setText(version or '—')
        self.pro_pill.setText('확인됨 · 자동 측정 가능' if pro
                              else '연결 후 확인' if not running else 'Pro 아님 · 수동 측정')
        self.pro_pill.setProperty('tone', 'ok' if pro else 'na' if not running else 'warn')
        repolish(self.pro_pill)
        self.blocking_label.setText('on' if running else '—')
        if not self._launching:
            self.retry_button.setText(t('다시 확인') if running else t('REW API 모드로 실행'))
        self.rew_hint.setVisible(not running)
        if not running and not self._launching:
            if self.rew.process_running():
                self._show_hint('REW 가 켜져 있지만 API 모드가 아닙니다. REW 를 닫고 '
                                '[REW API 모드로 실행] 을 누르세요.', bad=True)
            else:
                self._show_hint('REW 가 꺼져 있습니다. [REW API 모드로 실행] 을 누르면 '
                                'REW 를 -api 옵션으로 띄웁니다.', bad=True)
        self._refresh_rew_info(running)

    def _refresh_rew_info(self, running: bool) -> None:
        """실행 파일 경로·캘리브레이션 — 목업의 고정 문구 대신 실제로 읽는다(2026-09-23).

        실행 파일은 `find_rew_exe()`(파일시스템 확인, REW 연결과 무관하게 늘 다시 읽는다).
        캘리브레이션은 캐시가 REW 에서 읽어 둔 `GET /audio/input-cal` 값이고(REW 가 응답할 때만),
        그 밖(연결 안 됨·엔드포인트 실패·입력별로 달라 특정 못 함)은 '알 수 없음'이다.
        `측정 레벨` 은 이 화면에 상시 표시하지 않고 [설정 읽기]가 그때 새로 읽는다
        (09-23 코드 리뷰 9번: 예전에는 refresh 때마다 화면 스레드에서 GET 을 불렀다).
        """
        self.path_label.setText(find_rew_exe() or t('알 수 없음'))
        cal_path = self.rew.input_calibration_file() if running else None
        if cal_path is None:
            self.cal_pill.setText(t('알 수 없음'))
            self.cal_pill.setProperty('tone', 'na')
            self.cal_pill.setToolTip('')
        elif cal_path == '':
            self.cal_pill.setText(t('로드된 파일 없음'))
            self.cal_pill.setProperty('tone', 'warn')
            self.cal_pill.setToolTip('')
        else:
            self.cal_pill.setText(os.path.basename(cal_path) or cal_path)
            self.cal_pill.setProperty('tone', 'ok')
            self.cal_pill.setToolTip(cal_path)
        repolish(self.cal_pill)

    def _refresh_devices(self) -> None:
        stored = dict(self.project.extra.get('devices', {}) or {})
        if stored.get('sample_rate'):
            self.set_sample_rate(int(stored['sample_rate']))
        outputs = list(self.devices.output_devices())
        inputs = list(self.devices.input_devices())
        running = self._status()[0]
        if running and (not outputs or not inputs):
            if not self._device_retry.isActive():
                self._device_retry_left = 15          # 최대 30 초
                self._device_retry.start()
        else:
            self._device_retry.stop()
        # 입력 채널·루프백 출력은 REW 채널 이름으로 고른다(번호만으로는 어느 단자인지 모른다 — 09-22).
        self.input_channel.set_names(input_channel_names(self.devices),
                                     fallback='In {0}', count=max((d.channels for d in inputs), default=8))
        for combo, items, wanted in ((self.output_combo, outputs, stored.get('output')),
                                     (self.input_combo, inputs, stored.get('input'))):
            combo.blockSignals(True)
            combo.clear()
            if items:
                for device in items:
                    # 이름에 이미 채널 수가 있으면 덧붙이지 않는다('Demo ASIO 16ch · 16ch' 가 칸에서 잘렸다 — 10-10 3차 시험)
                    tag = f'{device.channels}ch'
                    text = device.name if tag.lower() in device.name.lower() else f'{device.name} · {tag}'
                    combo.addItem(text, device.name)
                    combo.setItemData(combo.count() - 1, text, QtCore.Qt.ItemDataRole.ToolTipRole)
                index = combo.findData(wanted)
                combo.setCurrentIndex(index if index >= 0 else 0)
            else:
                combo.addItem('REW 연결 후 표시됨' if not running else '장치 목록을 읽을 수 없음', '')
            combo.blockSignals(False)
        if stored.get('input_channel'):
            self.input_channel.setValue(int(stored['input_channel']))
        if stored.get('sweep_length'):
            index = self.sweep_combo.findData(stored['sweep_length'])
            if index >= 0:
                self.sweep_combo.setCurrentIndex(index)
        self._store()

    def _store(self, *_args) -> None:
        if self._building and _args:
            return
        output = str(self.output_combo.currentData() or '')
        previous = dict(self.project.extra.get('devices', {}) or {})
        self.project.extra['devices'] = {
            'output': output,
            'input': str(self.input_combo.currentData() or ''),
            'input_channel': int(self.input_channel.value()),
            'sweep_length': str(self.sweep_combo.currentData() or '256k'),
            'repetitions': int(previous.get('repetitions', 1) or 1),
            'sample_rate': int(self.sample_rate),
            # 타이밍 선택은 4단계가 쓴다 — 여기서 다시 쓰면 지워진다.
            **{key: previous[key] for key in TIMING_KEYS if key in previous},
        }
        if output:
            self.project.config.device = output
        if not self._building:
            self.changed.emit()

    def _read_settings(self) -> None:
        """[설정 읽기] — REW 에서 실제 값을 다시 읽어 안내 상자에 보여준다(2026-09-23).

        샘플레이트·스윕 길이는 화면이 REW 에 이미 보낸 값을 그대로 쓰고(다시 읽을
        엔드포인트가 있어도 같은 값이다), 측정 레벨은 `GET /measure/level`(실검증
        확정)로 다시 읽는다 — REW 에서 사람이 직접 바꿀 수 있는 값이라 화면 값과
        어긋날 수 있다. 실행 파일·캘리브레이션도 이 김에 같이 다시 읽는다.
        """
        running, _version, _pro = self._status()
        self.rew_hint.setVisible(True)
        self.rew_hint.setObjectName('help' if running else 'helpBad')
        repolish(self.rew_hint)
        if not running:
            self.rew_hint.setText(t('REW 를 켜 주세요.'))
            return
        self.read_button.setEnabled(False)
        # 읽기는 캐시의 워커 스레드가 한다 — 끝나면 `_show_settings` 가 화면 스레드에서 보인다.
        self.rew.request_refresh(then=self._show_settings, level=True)

    def _show_settings(self, snap) -> None:
        self.read_button.setEnabled(True)
        self._refresh_status()
        if not snap.running:
            self.rew_hint.setVisible(True)
            self.rew_hint.setObjectName('helpBad')
            repolish(self.rew_hint)
            self.rew_hint.setText(t('REW 를 켜 주세요.'))
            return
        self.rew_hint.setVisible(True)
        self.rew_hint.setObjectName('help')
        repolish(self.rew_hint)
        level = snap.level_dbfs
        # level_text 는 t() 로 미리 옮기지 않는다 — 아래 문장 전체가 en_g4 의 '{0}…{2}'
        # 패턴으로 등록돼 있어, 끼워 넣은 한국어 값은 `i18n.translate` 가 알아서 옮긴다.
        level_text = f'{level:.0f} dBFS' if level is not None else '알 수 없음'
        self.rew_hint.setText(f'REW 측정 설정을 읽어왔습니다: {self.sample_rate} Hz, '
                              f'{self.sweep_combo.currentData()}, 레벨 {level_text}')

    # ──────────────────────────── 진행 조건 ────────────────────────────
    def can_advance(self) -> tuple[bool, str]:
        running, _version, pro = self._status()
        if not running:
            return False, t('REW 를 켜 주세요.')
        if not pro:
            return False, 'REW Pro 라이선스가 없어 자동 측정을 시작할 수 없습니다.'
        return True, ''


__all__ = ['Step1Devices', 'SWEEP_SECONDS', 'sweep_seconds', 'fill_sweep_combo',
           'DEFAULT_SAMPLE_RATE']
