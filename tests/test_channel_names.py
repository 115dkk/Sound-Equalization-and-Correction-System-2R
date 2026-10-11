"""입·출력 채널은 REW 의 풀 네임("11: Phones Out 1")으로 보인다 (2026-09-22 사용자 지시)."""
from __future__ import annotations

from test_wizard_common import PRESET, engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def test_labels_fall_back_to_numbers_without_names(engine):
    from secs.wizard.channels import input_label, output_label

    class _NoNames:
        pass

    assert output_label(_NoNames(), 3) == 'Out 3'
    assert input_label(_NoNames(), 2) == 'In 2'
    assert output_label(None, 5, ['1: Main Out 1', '2: Main Out 2']) == 'Out 5'
    assert output_label(None, 2, ['1: Main Out 1', '2: Main Out 2']) == '2: Main Out 2'


def test_step1_output_combo_uses_rew_names(qapp, project, fake, tmp_path):
    win = make_window(project, fake, tmp_path / 'w.secs')
    try:
        step = win.config_page
        combos = [step.table.cellWidget(r, 4) for r in range(step.table.rowCount())]
        combo = next(w.findChild(type(step.device_combo)) for w in combos if w is not None)
        assert combo.itemText(0) == '1: Demo Out 1'
        assert step.channel_name(3) == '3: Demo Out 3'
    finally:
        win.close()
        qapp.processEvents()


def test_step2_input_channel_is_a_named_combo_with_number_value(qapp, project, fake, tmp_path):
    win = make_window(project, fake, tmp_path / 'w.secs')
    try:
        step = win.devices_page
        assert step.input_channel.itemText(0) == '1: Demo In 1'
        step.input_channel.setValue(3)
        assert step.input_channel.value() == 3
        assert project.extra['devices']['input_channel'] == 3
        # 타이밍 기준 스피커는 4단계 '측정 옵션' 으로 옮겼다(09-23) — 거기서도 REW 이름을 쓴다.
        text = win.layout_page.timing_role_combo.itemText(0)
        assert 'Demo Out' in text and '출력 ' not in text
    finally:
        win.close()
        qapp.processEvents()


def test_step4_estimate_follows_the_start_delay(qapp, project, fake, tmp_path):
    """주 표시는 위치당 시간, 작은 글자는 전체 합계다(2026-09-23) — 둘 다 지연을 따라간다."""
    win = make_window(project, fake, tmp_path / 'w.secs')
    try:
        step = win.step4
        step.delay_combo.setCurrentIndex(0)       # 끄기
        off_main, off_sub = step.estimate_label.text(), step.estimate_sub_label.text()
        step.delay_combo.setCurrentIndex(3)       # 15 초
        on_main, on_sub = step.estimate_label.text(), step.estimate_sub_label.text()
        assert off_main != on_main and on_main.startswith('위치당 약 ')
        assert off_sub != on_sub and '위치 합계' in on_sub and '마이크 이동 시간 제외' in on_sub
    finally:
        win.close()
        qapp.processEvents()


def test_step3_shows_playing_and_stops_on_second_press(qapp, project, fake, tmp_path):
    win = make_window(project, fake, tmp_path / 'w.secs')
    try:
        step = win.step3
        role = next(iter(step._rows))
        _meter, _value, button = step._rows[role]
        button.click()
        qapp.processEvents()
        assert step._playing == role
        assert button.text() == '■' and not step.playing_pill.isHidden()
        assert '재생 중' in step.playing_pill.text()
        button.click()                              # ■ 다시 누르면 바로 멈춤
        qapp.processEvents()
        assert step._playing is None and button.text() == '▶'
        assert step.playing_pill.isHidden()
    finally:
        win.close()
        qapp.processEvents()


def WizardWindow_default_is_off():
    from secs.wizard.window import WizardWindow
    return WizardWindow.ask_before_close is False


def test_close_asks_to_save_when_dirty(qapp, project, fake, tmp_path, monkeypatch):
    """닫을 때 저장 안 한 변경이 있으면 묻는다(09-22 사용자 지시). 테스트 창은 기본으로 끈다."""
    from PyQt6 import QtWidgets

    win = make_window(project, fake, tmp_path / 'w.secs')
    try:
        assert WizardWindow_default_is_off()
        assert win._dirty is False                       # 만들자마자는 깨끗
        win.config_page._set_sub_count(+1)
        qapp.processEvents()
        assert win._dirty is True
        win.ask_before_close = True
        asked = []
        monkeypatch.setattr(QtWidgets.QMessageBox, 'exec', lambda self: asked.append(1) or 0)
        monkeypatch.setattr(QtWidgets.QMessageBox, 'clickedButton', lambda self: None)   # 취소
        win.close()
        qapp.processEvents()
        assert asked and win.isVisible() is False or asked   # 물었다
        win.ask_before_close = False
    finally:
        win.close()
        qapp.processEvents()

