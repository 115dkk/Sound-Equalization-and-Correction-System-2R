"""QA-B: 3~5단계의 순서 밖 조작과 실제 글꼴의 최소 창 폭 회귀 검사.

REW/오디오 장치 대신 FakeREW만 사용한다. 알려진 결함은 개정 1에 따라
xfail 이던 결함 2건은 리드가 수정해 표시를 뗐다(09-19).
"""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path

import pytest

from test_wizard_common import engine, qapp, make_window  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


@pytest.fixture()
def window(qapp, tmp_path):
    from PyQt6 import QtCore
    from secs.model import Project, SpeakerConfig
    from secs.rewapi import FakeREW

    project = Project(config=SpeakerConfig.from_preset('2.1', biamp=False))
    rew = FakeREW(delay_s=0.02)
    win = make_window(project, rew, tmp_path / 'QA-B.secs')
    win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    yield win
    win.step3._play_timer.stop()
    win.step5.cancel()
    assert win.step5.wait(30000), '정리 중 측정 스레드가 끝나지 않았다'
    win.close()
    rew.close()
    qapp.processEvents()


@pytest.mark.parametrize('preset,biamp', [('2.1', False), ('9.1.6', True)])
@pytest.mark.parametrize('step', [3, 4, 5])
def test_minimum_width_has_no_horizontal_scroll(qapp, tmp_path, preset, biamp, step):
    if (os.environ.get('QT_QPA_PLATFORM', '').lower() == 'offscreen'
            or qapp.platformName().lower() == 'offscreen'):
        pytest.skip('실제 글꼴 폭 검사: QT_QPA_PLATFORM=windows로 별도 실행')
    from PyQt6 import QtCore

    path = Path(__file__).resolve().parents[1] / 'tools' / 'qa_capture.py'
    spec = importlib.util.spec_from_file_location('qa_capture_b', path)
    capture_tools = importlib.util.module_from_spec(spec)
    # 도구 임포트가 플랫폼 환경변수를 지우므로 다른 테스트에 영향을 주지 않는다.
    platform = os.environ.get('QT_QPA_PLATFORM')
    try:
        spec.loader.exec_module(capture_tools)
    finally:
        if platform is not None:
            os.environ['QT_QPA_PLATFORM'] = platform
    win = capture_tools.build_window(preset, biamp, 2, tmp_path)
    win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    try:
        win.show()
        capture_tools.measure(win, qapp, 2)
        result = capture_tools.capture(win, qapp, step, (1024, 640), tmp_path,
                                       f'step{step}_{preset}_qa_b')
        assert result['hscroll'] == 0, result
        assert not result['clipped'], result
    finally:
        win.close()
        qapp.processEvents()


@pytest.mark.parametrize('subs', [0, 8])
def test_channel_extremes_and_repeated_navigation(window, qapp, subs):
    window.project.config.set_sub_count(subs)
    for _ in range(3):
        for step in (3, 4, 5, 4):
            window.go(step)
            qapp.processEvents()
    assert len(window.step3._rows) == 2 + subs
    assert len(window.step5.plan().roles) == 2 + subs
    assert window.step5.plan().validate() == []
    window.go(3)
    for value in (window.step3.master.minimum(), window.step3.master.maximum()):
        window.step3.master.setValue(value)
        assert window.step3.master_db() == value / 2
    window.step3.apply_defaults()
    window.step3.apply_defaults()
    assert window.step3.can_advance()[0]


def test_empty_configuration_and_actions_before_measurement(window):
    window.project.config.speakers.clear()
    for step in (3, 4, 5):
        window.go(step)
    window.step3.sequence_button.click()
    window.step3.apply_defaults()
    assert window.step3._playing is None
    assert window.step3._rows == {}
    for _ in range(3):
        window.step5.cancel()
        window.step5.redo_last()
        assert not window.step5.measure_position(1)
    assert not window.step5.can_advance()[0]
    assert not window.project.measurements


def test_layout_choices_and_delay_endpoints(window):
    step = window.step4
    window.go(4)
    for count in (17, 9, 13, 13, 17, 9):
        step._picks[count].click()
        assert step.count() == count
        assert len(step.points()) == count
        assert len(window.step5.plan().positions) == count
    for index in (0, step.delay_combo.count() - 1, 0):
        step.delay_combo.setCurrentIndex(index)
        assert window.project.extra['start_delay'] == step.delay_combo.currentText()


def test_repeated_start_cancel_navigation_and_restart(window, qapp):
    step = window.step5
    window.go(5)
    assert not step.can_advance()[0]
    assert not step.measure_position(0)
    assert not step.measure_position(-1)
    step.redo_button.click()
    for _ in range(3):
        step.measure_button.click()
        assert step._thread is not None
        thread = step._thread
        step.measure_button.click()
        assert not step.measure_position(1)
        assert step._thread is thread
        assert not step.sample_button.isEnabled()
        assert not step.mdat_button.isEnabled()
        step.cancel_button.click()
        step.cancel()
        window.go(4)
        assert step.wait(30000)
        qapp.processEvents()
        assert not step.done_positions()
        window.go(5)
    assert step.measure_position(1)
    assert step.wait(30000)
    assert step.done_positions() == [1]
    step.redo_button.click()
    step.redo_button.click()
    assert step.done_positions() == []
    assert not step.can_advance()[0]


def test_mdat_cancel_and_missing_file_leave_measurements_unchanged(window, monkeypatch):
    from PyQt6 import QtWidgets

    window.go(5)
    monkeypatch.setattr(QtWidgets.QFileDialog, 'getOpenFileName',
                        lambda *a, **k: ('', ''))
    for _ in range(3):
        window.step5.mdat_button.click()
    assert window.step5.load_mdat(str(window.step5.project_path.with_suffix('.mdat'))) == 0
    assert not window.project.measurements
    assert '읽지 못했습니다' in window.step5.log.toPlainText()


@pytest.mark.parametrize('roles', [[], ['L', 'R', 'SW1']])
def test_mdat_dialog_repeated_duplicate_skip_and_cancel(qapp, roles):
    from PyQt6 import QtWidgets
    from secs.wizard.step5_measure import MdatAssignDialog

    dialog = MdatAssignDialog(['L 측정', '참고 측정'], roles, [None, None])
    try:
        ok = dialog.buttons.button(QtWidgets.QDialogButtonBox.StandardButton.Ok)
        cancel = dialog.buttons.button(QtWidgets.QDialogButtonBox.StandardButton.Cancel)
        assert not ok.isEnabled()
        for _ in range(3):
            if roles:
                for combo in dialog.combos:
                    combo.setCurrentIndex(combo.findData('L'))
                assert not ok.isEnabled()
                assert '두 번' in dialog.problems()
                dialog.combos[1].setCurrentIndex(0)
                assert ok.isEnabled()
            for combo in dialog.combos:
                combo.setCurrentIndex(0)
            assert not ok.isEnabled()
            ok.click()
            assert dialog.result() != QtWidgets.QDialog.DialogCode.Accepted
        cancel.click()
        assert dialog.result() == QtWidgets.QDialog.DialogCode.Rejected
    finally:
        dialog.close()


def test_level_stop_cancels_timer_and_restores_buttons(window):
    window.go(3)
    step = window.step3
    step.sequence_button.click()
    assert step._play_timer.isActive()
    step.stop_monitor()
    assert not step._play_timer.isActive(), '단계 이탈 후에도 재생 완료 타이머가 남는다'
    assert all(button.isEnabled() for _, _, button in step._rows.values())


def test_reselecting_layout_preserves_existing_measurements(window):
    window.go(5)
    assert window.step5.measure_position(1)
    assert window.step5.wait(30000)
    before = list(window.project.measurements)
    window.go(4)
    window.step4._picks[window.step4.count()].click()
    assert window.project.measurements == before, '같은 배치 재선택으로 측정이 삭제된다'
