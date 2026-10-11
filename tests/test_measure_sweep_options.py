# -*- coding: utf-8 -*-
"""5단계 측정 화면의 스윕 길이·반복 선택 회귀."""
from __future__ import annotations

import pytest

from test_wizard_common import engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


@pytest.fixture()
def window(qapp, project, fake, tmp_path):
    win = make_window(project, fake, tmp_path / '스윕 옵션.secs')
    win.go(5)
    yield win
    win.close()
    qapp.processEvents()


def _set_timing(project, kind: str) -> None:
    devices = dict(project.extra.get('devices', {}) or {})
    devices['timing_reference'] = kind
    project.extra['devices'] = devices


def test_measure_page_has_sweep_and_repeat_combos(window):
    step = window.step5
    assert [step.sweep_combo.itemData(i) for i in range(step.sweep_combo.count())] == [
        '64k', '128k', '256k', '512k', '1M', '2M', '4M']
    assert [step.repeat_combo.itemData(i) for i in range(step.repeat_combo.count())] == [1, 2, 4, 8]
    assert '256k · 5.5 s' in [
        step.sweep_combo.itemText(i) for i in range(step.sweep_combo.count())]


def test_changes_update_extra_plan_and_step1_combo(window, project):
    step = window.step5
    _set_timing(project, 'loopback')
    step.refresh()

    step.sweep_combo.setCurrentIndex(step.sweep_combo.findData('512k'))
    step.repeat_combo.setCurrentIndex(step.repeat_combo.findData(2))
    assert project.extra['devices']['sweep_length'] == '512k'
    assert project.extra['devices']['repetitions'] == 2
    assert step.plan().sweep_length == '512k'
    assert step.plan().repetitions == 2

    window.devices_page.refresh()
    assert window.devices_page.sweep_combo.currentData() == '512k'
    assert [window.devices_page.sweep_combo.itemText(i)
            for i in range(window.devices_page.sweep_combo.count())] == [
                step.sweep_combo.itemText(i) for i in range(step.sweep_combo.count())]

    window.devices_page.sweep_combo.setCurrentIndex(
        window.devices_page.sweep_combo.findData('1M'))
    step.refresh()
    assert project.extra['devices']['sweep_length'] == '1M'
    assert step.sweep_combo.currentData() == '1M'


def test_acoustic_timing_locks_repeat_to_one(window, project):
    devices = dict(project.extra.get('devices', {}) or {})
    devices.update({'timing_reference': 'acoustic', 'repetitions': 8})
    project.extra['devices'] = devices

    window.step5.refresh()
    assert window.step5.repeat_combo.currentData() == 1
    assert not window.step5.repeat_combo.isEnabled()
    assert window.step5.repeat_note.isVisibleTo(window.step5)
    assert project.extra['devices']['repetitions'] == 1
    assert window.step5.plan().repetitions == 1


def test_sweep_controls_lock_during_countdown(window, project):
    _set_timing(project, 'loopback')
    window.step5._countdown_left = 1
    try:
        window.step5.refresh()
        assert not window.step5.delay_combo.isEnabled()
        assert not window.step5.sweep_combo.isEnabled()
        assert not window.step5.repeat_combo.isEnabled()
    finally:
        window.step5._countdown_left = 0
        window.step5.refresh()


def test_two_repeats_increase_position_seconds(project):
    from secs.wizard.step4_layout import position_seconds

    project.extra['start_delay'] = '끄기'
    _set_timing(project, 'loopback')
    project.extra['devices']['repetitions'] = 1
    once = position_seconds(project)
    project.extra['devices']['repetitions'] = 2
    twice = position_seconds(project)
    assert twice == pytest.approx(once * 2.0)
    assert twice > once
