"""2단계 [REW API 모드로 실행] 이 실제로 REW 를 띄운다 (2026-09-22).

사용자 보고: "REW API 로 실행 버튼이 안 먹혀" — 09-22 전에는 이름과 달리 상태만 다시 물었다.
이제 REW 가 응답하지 않으면 `RealREW.launch` 를 스레드에서 부르고, 끝나면 화면을 다시 채운다.
REW 가 켜져 있지만 -api 가 아닐 때는 띄우지 않고 그 이유를 말한다.
"""
from __future__ import annotations

import time

from PyQt6 import QtWidgets

from test_wizard_common import PRESET, engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


class _DownThenUp:
    """처음엔 꺼져 있고 `launch()` 뒤에 켜지는 REW 흉내. 장치 목록은 켜진 뒤에만 준다."""

    def __init__(self):
        self.up = False
        self.launched = 0

    def is_running(self):
        return self.up

    def version(self):
        return '5.40 (stub)' if self.up else None

    def is_pro(self):
        return self.up

    def launch(self, wait_s=20.0):
        self.launched += 1
        self.up = True
        return True

    def output_devices(self):
        from secs.rewapi.interfaces import DeviceInfo
        return [DeviceInfo(id='MOTU', name='MOTU', channels=22, kind='output')] if self.up else []

    def input_devices(self):
        from secs.rewapi.interfaces import DeviceInfo
        return [DeviceInfo(id='MOTU', name='MOTU', channels=20, kind='input')] if self.up else []


def _pump(qapp, seconds=2.0, until=None):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        qapp.processEvents()
        if until is not None and until():
            return True
        time.sleep(0.02)
    return until() if until is not None else True


def test_button_launches_rew_when_it_is_down(qapp, project, monkeypatch):
    from secs.wizard.step1_devices import Step1Devices
    import secs.wizard.step1_devices as module

    monkeypatch.setattr(module, 'rew_process_running', lambda: False)
    monkeypatch.setattr(module, 'find_rew_exe', lambda: 'C:/fake/roomeqwizard.exe')
    stub = _DownThenUp()
    step = Step1Devices(project, rew=stub, devices=stub)
    try:
        assert step.retry_button.text() == 'REW API 모드로 실행'
        assert step.output_combo.itemText(0) == 'REW 연결 후 표시됨'     # 왜 비었는지 말한다
        step.retry_button.click()
        assert _pump(qapp, 5.0, until=lambda: not step._launching)
        assert stub.launched == 1
        assert step.retry_button.isEnabled()
        assert step.retry_button.text() == '다시 확인'
        assert step.output_combo.itemText(0) == 'MOTU · 22ch'
        assert step.input_combo.itemText(0) == 'MOTU · 20ch'
    finally:
        step.close()


def test_button_does_not_launch_a_second_rew_without_api(qapp, project, monkeypatch):
    from secs.wizard.step1_devices import Step1Devices
    import secs.wizard.step1_devices as module

    monkeypatch.setattr(module, 'rew_process_running', lambda: True)     # REW 는 떠 있는데 API 는 안 됨
    stub = _DownThenUp()
    step = Step1Devices(project, rew=stub, devices=stub)
    try:
        assert 'API 모드가 아닙니다' in step.rew_hint.text()
        step.retry_button.click()
        _pump(qapp, 0.5)
        assert stub.launched == 0
        assert '닫은 뒤' in step.rew_hint.text()
    finally:
        step.close()


def test_button_only_refreshes_when_rew_is_up(qapp, project, monkeypatch):
    from secs.wizard.step1_devices import Step1Devices
    import secs.wizard.step1_devices as module

    monkeypatch.setattr(module, 'rew_process_running', lambda: True)
    stub = _DownThenUp()
    stub.up = True
    step = Step1Devices(project, rew=stub, devices=stub)
    try:
        assert step.retry_button.text() == '다시 확인'
        step.retry_button.click()
        _pump(qapp, 0.5)
        assert stub.launched == 0
        assert not step.rew_hint.isVisible() or step.rew_hint.text() == ''
    finally:
        step.close()


def test_missing_exe_explains_instead_of_launching(qapp, project, monkeypatch):
    from secs.wizard.step1_devices import Step1Devices
    import secs.wizard.step1_devices as module

    monkeypatch.setattr(module, 'rew_process_running', lambda: False)
    monkeypatch.setattr(module, 'find_rew_exe', lambda: None)
    stub = _DownThenUp()
    step = Step1Devices(project, rew=stub, devices=stub)
    try:
        step.retry_button.click()
        _pump(qapp, 0.5)
        assert stub.launched == 0
        assert '실행 파일을 찾지 못했습니다' in step.rew_hint.text()
    finally:
        step.close()


def test_rew_process_detection_reads_tasklist(monkeypatch):
    import secs.rewapi.rew as rew

    class _Done:
        stdout = '"roomeqwizard.exe","22336","Console","3","397,164 K"\n'

    monkeypatch.setattr(rew, '_IS_WINDOWS', True)
    monkeypatch.setattr(rew.subprocess, 'run', lambda *a, **k: _Done())
    assert rew.rew_process_running() is True
    _Done.stdout = '"python.exe","1","Console","1","1 K"\n'
    assert rew.rew_process_running() is False


def test_read_settings_shows_real_level_and_calibration(qapp, project, monkeypatch):
    """[설정 읽기] · 캘리브레이션 배지 — REW 에서 실제로 읽는다(2026-09-23).

    목업은 '90° 파일 로드됨' · '-20 dBFS' 를 고정 문구로 보여줬다. 이제 REW 가 주는
    실제 값(측정 레벨·캘리브레이션 파일)을 쓰고, 그 REW 흉내에 메서드가 없으면
    '알 수 없음' 으로 떨어진다.
    """
    from secs.wizard.step1_devices import Step1Devices
    import secs.wizard.step1_devices as module

    monkeypatch.setattr(module, 'find_rew_exe', lambda: 'C:/fake/roomeqwizard.exe')

    class _WithRealValues(_DownThenUp):
        def measure_level_dbfs(self):
            return -24.0

        def input_calibration_file(self):
            return 'C:\\cal\\UMIK-1.cal'

    stub = _WithRealValues()
    stub.up = True
    step = Step1Devices(project, rew=stub, devices=stub)
    try:
        assert step.path_label.text() == 'C:/fake/roomeqwizard.exe'
        assert step.cal_pill.text() == 'UMIK-1.cal'
        assert step.cal_pill.toolTip() == 'C:\\cal\\UMIK-1.cal'
        step.read_button.click()
        assert '-24 dBFS' in step.rew_hint.text()
    finally:
        step.close()


def test_read_settings_and_calibration_are_unknown_without_the_new_methods(qapp, project, monkeypatch):
    """옛 REW 흉내(메서드 없음)에서는 '알 수 없음' 이지 예외가 아니다."""
    from secs.wizard.step1_devices import Step1Devices
    import secs.wizard.step1_devices as module

    monkeypatch.setattr(module, 'find_rew_exe', lambda: None)
    stub = _DownThenUp()
    stub.up = True
    step = Step1Devices(project, rew=stub, devices=stub)
    try:
        assert step.path_label.text() == '알 수 없음'
        assert step.cal_pill.text() == '알 수 없음'
        step.read_button.click()
        assert '알 수 없음' in step.rew_hint.text()
    finally:
        step.close()


def test_device_lists_fill_in_by_themselves_after_rew_comes_up(qapp, project):
    """REW 가 막 떠서 /version 은 답하는데 장치 목록이 비어 있으면, [다시 확인] 없이도 채워질 때까지
    다시 읽는다(09-22 사용자: REW 를 열어도 다시 확인을 누르기 전엔 마이크·오디오가 안 뜬다)."""
    from secs.rewapi.interfaces import DeviceInfo
    from secs.wizard.step1_devices import Step1Devices

    class _SlowAudio(_DownThenUp):
        calls = 0

        def output_devices(self):
            _SlowAudio.calls += 1
            if _SlowAudio.calls < 3:                  # 처음 두 번은 아직 오디오 준비 전
                return []
            return [DeviceInfo(id='MOTU', name='MOTU', channels=22, kind='output')]

        def input_devices(self):
            return [DeviceInfo(id='MOTU', name='MOTU', channels=20, kind='input')] if _SlowAudio.calls >= 3 else []

    stub = _SlowAudio()
    stub.up = True
    step = Step1Devices(project, rew=stub, devices=stub)
    try:
        assert step.output_combo.itemText(0) != 'MOTU · 22ch'
        assert step._device_retry.isActive()
        step._device_retry.setInterval(50)
        assert _pump(qapp, 3.0, until=lambda: step.output_combo.itemText(0) == 'MOTU · 22ch')
        assert step.input_combo.itemText(0) == 'MOTU · 20ch'
        _pump(qapp, 0.2)
        assert not step._device_retry.isActive()
    finally:
        step.close()
