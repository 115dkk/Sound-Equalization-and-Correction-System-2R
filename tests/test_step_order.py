"""단계 순서: 1 장치·마이크(REW 연결) → 2 구성 → 3 레벨 … (2026-09-23 사용자 지시).

사용자: "REW 연결·장치 확인(지금 2단계)이 1단계로 와야 한다." 창의 화면 순서·제목·건너뛰기·
도움말·단축키가 같이 바뀌었는지, 타이밍 레퍼런스가 4단계로 옮겨 가도 장치 값과 서로 지우지 않는지 본다.
"""
from __future__ import annotations

from test_wizard_common import PRESET, engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def test_pages_are_in_the_new_order_and_new_projects_open_on_devices(qapp, project, fake, tmp_path):
    from secs.wizard.step1_devices import Step1Devices
    from secs.wizard.step2_config import Step2Config
    from secs.wizard.window import PAGE_NAMES

    win = make_window(project, fake, tmp_path / 'o.secs')
    try:
        assert win.current_step == 1
        assert isinstance(win.step_widget(), Step1Devices)
        assert isinstance(win.steps[1], Step2Config)
        assert [getattr(win, name) for name in PAGE_NAMES.values()] == win.steps
        assert win.step3 is win.level_page and win.step8 is win.export_page
        assert not hasattr(win, 'step1') and not hasattr(win, 'step2')     # 뜻이 바뀐 번호 이름은 없앴다
        assert win.steps_bar.buttons[0].text() == '장치·마이크'
        assert not win.skip_button.isHidden() and '장치 확인' in win.skip_button.text()
        win.skip()
        assert win.current_step == 2
        assert win.skip_button.isHidden()
    finally:
        win.close()
        qapp.processEvents()


def test_headings_follow_the_order(qapp, project, fake, tmp_path):
    win = make_window(project, fake, tmp_path / 'o.secs')
    try:
        assert '1 · 장치·마이크' in [lbl.text() for lbl in win.devices_page.findChildren(type(win.nav_message))]
        assert '2 · 구성' in [lbl.text() for lbl in win.config_page.findChildren(type(win.nav_message))]
    finally:
        win.close()
        qapp.processEvents()


def test_timing_lives_on_step4_and_device_saves_keep_it(qapp, project, fake, tmp_path):
    from secs.wizard.timing import timing_settings

    win = make_window(project, fake, tmp_path / 'o.secs')
    try:
        assert not hasattr(win.devices_page, 'timing_combo')
        win.go(4)
        page = win.layout_page
        page.timing_combo.setCurrentIndex(page.timing_combo.findData('loopback'))
        page.timing_channel.setValue(7)
        assert timing_settings(project) == ('loopback', 7)
        win.go(1)
        win.devices_page.input_channel.setValue(2)        # 장치 단계 저장이 타이밍 키를 지우지 않는다
        win.devices_page.refresh()
        assert timing_settings(project) == ('loopback', 7)
        assert project.extra['devices']['input_channel'] == 2
        assert win.measure_page.plan().timing_reference == 'loopback'
    finally:
        win.close()
        qapp.processEvents()


def test_default_reference_speaker_without_visiting_step4(qapp, engine):
    """4단계를 한 번도 안 열어도 음향 기준 스피커는 R(바이앰프면 R:HF)의 출력으로 풀린다."""
    import secs.model as model
    from secs.wizard.timing import measurement_roles, timing_settings

    project = model.Project(config=model.SpeakerConfig.from_preset('2.1', biamp=True))
    outputs = dict(measurement_roles(project))
    assert timing_settings(project) == ('acoustic', outputs['R:HF'])
