"""1단계 'REW 드라이버' 칸 — SECS 에서 ASIO · Java 를 바꾼다(2026-10-17 사용자).

외부 리포트(일반 사운드카드 7.1 이 2채널로 보임) 뒤 사용자: "secs2 에서 드라이버를 바꾸는게 나을거같아".
"""
from __future__ import annotations

from test_wizard_common import PRESET, engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def test_driver_combo_follows_rew_and_switches(qapp, project, fake):
    from secs.wizard.step1_devices import Step1Devices

    step = Step1Devices(project, rew=fake, devices=fake)
    try:
        assert step.driver_combo.currentData() == 'ASIO'           # REW 의 지금 드라이버
        assert step.driver_combo.isEnabled()
        step.driver_combo.setCurrentIndex(step.driver_combo.findData('Java'))
        qapp.processEvents()
        assert ('set_driver', {'driver': 'Java'}) in fake.calls
        assert fake.driver() == 'Java'
        assert step.driver_combo.currentData() == 'Java' and not step._switching
        before = len(fake.calls)
        step.refresh()                                              # 다시 그려도 REW 에 또 보내지 않는다
        assert len([c for c in fake.calls[before:] if c[0] == 'set_driver']) == 0
    finally:
        step.close()


def test_driver_combo_disabled_when_rew_down(qapp, project, fake):
    from secs.wizard.step1_devices import Step1Devices

    fake.set_running(False)
    step = Step1Devices(project, rew=fake, devices=fake)
    try:
        assert not step.driver_combo.isEnabled()
    finally:
        step.close()


def test_picking_device_opens_it_in_rew(qapp, project, fake):
    """10-17: 장치 칸은 이름만 기록했다 → 스윕이 REW 에 걸린 장치로 나갔다. 이제 고르면 REW 에도 연다."""
    from secs.wizard.step1_devices import Step1Devices

    step = Step1Devices(project, rew=fake, devices=fake)
    try:
        if step.output_combo.count() < 2:
            fake._outputs.append(type(fake._outputs[0])(id='X4', name='X4', channels=8, kind='output'))
            step.refresh()
        step.output_combo.setCurrentIndex(step.output_combo.count() - 1)
        qapp.processEvents()
        name = step.output_combo.currentData()
        assert ('set_device', {'kind': 'output', 'device': name}) in fake.calls
        sent = len(fake.calls)
        step.output_combo.setCurrentIndex(step.output_combo.currentIndex())   # 같은 장치는 다시 보내지 않는다
        assert len([c for c in fake.calls[sent:] if c[0] == 'set_device']) == 0
    finally:
        step.close()


def test_java_device_without_count_shows_name_only():
    from secs.rewapi.interfaces import DeviceInfo
    from secs.wizard.step1_devices import device_text

    assert device_text(DeviceInfo(id='a', name='X4', channels=8, kind='output')) == 'X4 · 8ch'
    assert device_text(DeviceInfo(id='b', name='Other', channels=0, kind='output',
                                  extra={'channels_unknown': True})) == 'Other'
