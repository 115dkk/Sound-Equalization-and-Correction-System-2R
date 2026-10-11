"""QA-A: 구성·장치의 순서 밖 조작과 실제 글꼴의 1024 폭 회귀 검사."""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path

import pytest

from test_wizard_common import engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


@pytest.fixture()
def window(qapp, project, fake, tmp_path):
    from PyQt6 import QtCore

    win = make_window(project, fake, tmp_path / "qa_a.secs")
    win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    yield win
    win.close()
    qapp.processEvents()


def test_sub_stepper_repeated_clicks_stop_at_zero_and_eight(window):
    step = window.config_page
    for _ in range(10):
        step._sub_minus.click()
    assert step.config.sub_count() == 0
    assert not step._sub_minus.isEnabled()
    for _ in range(10):
        step._sub_plus.click()
    assert step.config.sub_count() == 8
    assert not step._sub_plus.isEnabled()
    assert step.can_advance() == (True, "")


def test_empty_preset_blocks_next_and_recovers(window):
    window.go(2)                     # 구성은 2단계(09-23 순서 변경)
    step = window.config_page
    for _ in range(3):
        step._preset_buttons["custom"].click()
        assert not step.config.speakers
        assert not step.can_advance()[0]
        assert window.advance() is False
        assert window.current_step == 2
        step._sub_plus.click()
        assert step.config.sub_count() == 1
        step._sub_minus.click()
    step._preset_buttons["2.1"].click()
    assert step.can_advance() == (True, "")


def test_biamp_toggle_and_dsp_limits_keep_model_in_sync(qapp, project, fake, tmp_path, monkeypatch):
    # 바이앰프 칸은 10-11 부터 닫혀 있다(`secs.wizard.BIAMP_UI`) — 칸 자체의 동작은 켠 창으로 본다.
    import secs.wizard

    monkeypatch.setattr(secs.wizard, "BIAMP_UI", True)
    window = make_window(project, fake, tmp_path / "qa_a_biamp.secs")
    try:
        step = window.config_page
        for checked in (True, False, True, False):
            step._biamp.setChecked(checked)
            assert step.config.biamp_default is checked
            assert step.config.validate() == []
        for control, field in ((step.dsp_budget, "fir_budget"),
                               (step.dsp_iir, "iir_bands")):
            for value in (control.minimum(), control.maximum(), control.minimum()):
                control.setValue(value)
                assert getattr(window.project.dsp, field) == value
    finally:
        window.close()
        qapp.processEvents()


def test_enter_and_leave_configuration_and_device_repeatedly(window, qapp):
    for _ in range(5):
        for number in (2, 1):
            window.go(number)
            qapp.processEvents()
            assert window.current_step == number
            assert window.can_advance()[0]


def test_empty_configuration_timing_modes_and_channel_limits(window):
    from secs.wizard.timing import timing_settings

    window.config_page._preset_buttons["custom"].click()
    window.go(4)                     # 타이밍 레퍼런스는 4단계 '측정 옵션'(09-23)
    step = window.layout_page
    devices = window.devices_page
    assert step.timing_role_combo.count() == 0
    assert timing_settings(window.project) == ("acoustic", None)
    for kind in ("none", "loopback", "acoustic", "loopback"):
        step.timing_combo.setCurrentIndex(step.timing_combo.findData(kind))
        for value in (1, 64, 1):
            step.timing_channel.setValue(value)
            devices.input_channel.setValue(value)       # 장치 값과 타이밍 값이 서로를 지우지 않는다
            assert window.project.extra["devices"]["input_channel"] == value
            expected = value if kind == "loopback" else None
            assert timing_settings(window.project) == (kind, expected)
    window.go(2)
    window.config_page._preset_buttons["2.1"].click()
    window.go(4)
    step.timing_combo.setCurrentIndex(step.timing_combo.findData("acoustic"))
    assert timing_settings(window.project)[1] is not None


def test_recheck_and_read_settings_repeat_across_disconnect(window, fake):
    window.go(1)
    step = window.devices_page
    for running in (False, True, False, True):
        fake.set_running(running)
        for _ in range(3):
            step.retry_button.click()
            step.read_button.click()
            assert step.can_advance()[0] is running
            assert bool(step.rew_hint.text())


def test_missing_device_provider_remains_usable(qapp, project):
    from secs.wizard.step1_devices import Step1Devices

    step = Step1Devices(project)
    try:
        for _ in range(3):
            step.retry_button.click()
            step.read_button.click()
        assert not step.can_advance()[0]
        assert step.output_combo.currentData() == ""
    finally:
        step.close()


def test_read_settings_reports_selected_sample_rate(window):
    step = window.devices_page
    step.set_sample_rate(44100)
    step.read_button.click()
    assert "44100" in step.rew_hint.text() or "44.1" in step.rew_hint.text()


@pytest.mark.parametrize("preset,biamp", [("2.1", False), ("9.1.6", True)])
@pytest.mark.parametrize("step", [1, 2])
def test_1024_layout_has_no_horizontal_scroll(qapp, tmp_path, preset, biamp, step):
    # qa_capture 임포트가 환경변수를 지우므로 반드시 그보다 먼저 검사한다.
    if (os.environ.get("QT_QPA_PLATFORM", "").split(":")[0] == "offscreen"
            or qapp.platformName() == "offscreen"):
        pytest.skip("실제 글꼴 폭 검사는 QT_QPA_PLATFORM=windows에서 실행")
    from PyQt6 import QtCore

    path = Path(__file__).resolve().parents[1] / "tools" / "qa_capture.py"
    spec = importlib.util.spec_from_file_location("qa_capture_a", path)
    capture_module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capture_module)
    win = capture_module.build_window(preset, biamp, 0, tmp_path)
    try:
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.show()
        result = capture_module.capture(
            win, qapp, step, (1024, 640), tmp_path, f"step{step}_{preset}_qa_a")
        assert result["hscroll"] == 0, result
    finally:
        win.close()
        capture_module.settle(qapp)
