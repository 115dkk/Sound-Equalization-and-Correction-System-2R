"""위저드 창 · 단계 이동 · 진행 조건 (트랙 C).

`P2 인터페이스 계약 2026-09-16.md` 의 "C: `secs.wizard`" 절과 개정 1
(`current_step`·`go()` 는 1 기점 1~8)이 기준이다.
"""
from __future__ import annotations

import pytest

from test_wizard_common import (PRESET, engine, fake, make_window,  # noqa: F401
                                project, qapp)
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


@pytest.fixture()
def window(qapp, project, fake, tmp_path):
    win = make_window(project, fake, tmp_path / "w.secs")
    yield win
    win.close()
    qapp.processEvents()


def test_window_opens_at_step_one(window):
    from secs.wizard.strings import STEPS

    assert window.current_step == 1
    # 진행바 7칸 = 화면 7개(1~5 · 보정 설계 9 · 내보내기 8). 옛 6·7·10 화면은 2026-10-10 슬림화로 지웠다.
    assert len(STEPS) == 7
    assert window.stack.count() == 7


def test_go_walks_steps_one_to_five(window):
    for step in range(1, 6):
        window.go(step)
        assert window.current_step == step
        assert window.stack.currentIndex() == step - 1
        allowed, reason = window.can_advance()
        assert isinstance(allowed, bool) and isinstance(reason, str)
        if not allowed:
            assert reason


def test_go_out_of_range_raises(window):
    # 9 = 보정 설계, 8 = 내보내기. 6·7·10 은 2026-10-10 슬림화로 지운 옛 화면의 빈 번호다.
    for bad in (0, 6, 7, 10, 11, -1):
        with pytest.raises(ValueError):
            window.go(bad)
    assert window.current_step == 1


def test_every_step_is_a_real_screen(window):
    """모든 화면 번호가 실제 화면이다 — 자리표시는 없다. 8 = 내보내기(`Step8Mdat`), 9 = 보정 설계."""
    from secs.wizard.step8_mdat import Step8Mdat
    from secs.wizard.step_calibration import StepCalibration
    from secs.wizard.window import PAGE_NAMES

    for number, name in PAGE_NAMES.items():
        window.go(number)
        assert window.step_widget() is getattr(window, name)
    window.go(8)
    assert isinstance(window.step_widget(), Step8Mdat)
    assert window.can_advance() == (True, '')
    window.go(9)
    assert isinstance(window.step_widget(), StepCalibration)


def test_step1_preset_click_updates_project(window, project):
    import secs.model as model

    window.go(1)
    window.config_page._preset_buttons["5.1"].click()
    assert [s.role for s in project.config.speakers] == \
        [s.role for s in model.SpeakerConfig.from_preset("5.1").speakers]
    assert project.config.preset == "5.1"
    assert project.config.validate() == []
    allowed, _reason = window.can_advance()
    assert allowed is True


def test_step1_sub_stepper_and_validation(window, project):
    window.go(1)
    before = project.config.sub_count()
    window.config_page._sub_plus.click()
    assert project.config.sub_count() == before + 1
    window.config_page._sub_minus.click()
    assert project.config.sub_count() == before

    # 출력 채널을 겹치게 만들면 다음으로 넘어가지 못한다.
    project.config.speakers[1].bands[0].out = project.config.speakers[0].bands[0].out
    window.config_page.refresh()
    allowed, reason = window.config_page.can_advance()
    assert allowed is False and "중복" in reason


def test_devices_step_requires_running_rew(window, fake):
    window.go(1)
    allowed, _reason = window.can_advance()
    assert allowed is True

    fake.set_running(False)
    window.devices_page.recheck()
    allowed, reason = window.can_advance()
    assert allowed is False and reason
    fake.set_running(True)
    window.devices_page.recheck()
    assert window.can_advance()[0] is True


def test_step2_sweep_lengths_all_show_seconds(window):
    """B 가 SWEEP_LENGTHS 를 64k~4M 로 늘린 뒤에도 초 표시가 빠지지 않는다."""
    from secs.rewapi.interfaces import SWEEP_LENGTHS
    from secs.wizard.step1_devices import SWEEP_SECONDS, sweep_seconds

    assert set(SWEEP_SECONDS) == set(SWEEP_LENGTHS)
    assert SWEEP_SECONDS['64k'] == 1.4 and SWEEP_SECONDS['2M'] == 43.7
    assert sweep_seconds('nope') is None

    combo = window.devices_page.sweep_combo
    assert combo.count() == len(SWEEP_LENGTHS)
    for index in range(combo.count()):
        length = combo.itemData(index)
        assert combo.itemText(index) == f'{length} · {SWEEP_SECONDS[length]:.1f} s'


def test_step2_sweep_seconds_follow_the_sample_rate(window, project):
    from secs.wizard.step1_devices import sweep_seconds

    window.go(2)
    window.devices_page.set_sample_rate(44100)
    assert window.devices_page.rate_label.text() == '44100 Hz'
    index = window.devices_page.sweep_combo.findData('256k')
    assert window.devices_page.sweep_combo.itemText(index) == \
        f'256k · {sweep_seconds("256k", 44100):.1f} s'
    assert window.devices_page.sweep_combo.currentData() == '256k'   # 선택은 유지
    assert project.extra['devices']['sample_rate'] == 44100


def test_step2_stores_device_choice(window, project):
    window.go(2)
    devices = project.extra.get("devices")
    assert devices and devices["output"]
    assert project.config.device == devices["output"]


def test_step3_needs_levels_and_skip_applies_defaults(window, project):
    window.go(3)
    allowed, reason = window.can_advance()
    assert allowed is False and reason

    window.skip()   # 개정 1: 3 단계 건너뛰기 = 기준값 적용
    assert window.current_step == 4
    levels = project.extra["levels"]
    assert set(levels) == {s.role for s in project.config.speakers}
    window.go(3)
    assert window.can_advance()[0] is True


def test_step4_arrangement_choice(window, project):
    from secs.wizard.layout_map import ARRANGEMENTS

    window.go(4)
    assert window.can_advance()[0] is True
    for count in ARRANGEMENTS:
        window.step4.set_count(count)
        assert project.extra["array"] == count
        assert len(window.step4.points()) == count


def test_channel_names_follow_device_prefix(window):
    """목업 chName: 장치별 접두어(Out · Tx · AVB)."""
    from secs.wizard.step2_config import channel_name, channel_prefix

    assert channel_prefix("MOTU UltraLite-mk5 · ASIO") == "Out"
    assert channel_prefix("Realtek HD Audio · 2ch") == "Out"
    assert channel_prefix("Dante Virtual Soundcard · 64ch") == "Tx"
    assert channel_prefix("Tesira AVB (Windows AVB 드라이버)") == "AVB"
    assert channel_name("dvs Dante Virtual Soundcard", 7) == "Tx 7"

    window.go(1)
    assert window.config_page.channel_name(3).endswith(" 3")
    def widgets(layout):
        # 요약은 두 줄(글자 줄 · 알약 줄) 레이아웃이다(09-24) — 안쪽 레이아웃까지 훑는다.
        for i in range(layout.count()):
            item = layout.itemAt(i)
            if item.layout() is not None:
                yield from widgets(item.layout())
            else:
                yield item.widget()

    summary = list(widgets(window.config_page._summary))
    texts = [w.text() for w in summary if w is not None and hasattr(w, "text")]
    assert any(window.config_page.channel_name(
        max(b.out for s in window.project.config.speakers for b in s.bands)) in text
        for text in texts)


def test_sidebar_summary_tracks_project(window, project):
    window.go(1)
    outs = sum(len(s.bands) for s in project.config.speakers)
    assert f"출력 {outs}" in window.side_layout.text()
    assert "위치" in window.side_meas.text()
