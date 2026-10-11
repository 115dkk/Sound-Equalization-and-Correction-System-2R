"""로딩 오버레이 — 긴 작업 동안만 보이고, 짧은 작업엔 깜빡이지 않는다 (2026-09-23 사용자 요청).

"오래 걸리는 단계에서는 로딩창을 띄워줘." 오버레이는 300 ms 넘게 도는 작업에만 뜨고(`SHOW_DELAY_MS`),
끝나면 사라진다. 움직이는 그림은 `LoadingIndicator` 한 곳에서 24프레임 스프라이트로 돈다.
"""
from __future__ import annotations

import time

from test_wizard_common import PRESET, engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def _pump(qapp, seconds: float) -> None:
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        qapp.processEvents()
        time.sleep(0.01)


def _shown_window(project, fake, tmp_path):
    from PyQt6 import QtCore

    win = make_window(project, fake, tmp_path / 'l.secs')
    win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    win.show()
    return win


def test_overlay_waits_before_showing_and_goes_away(qapp, project, fake, tmp_path):
    win = _shown_window(project, fake, tmp_path)
    try:
        win._busy_timer.stop()          # 창의 점검(250 ms)이 '돌고 있는 작업 없음'으로 거두지 않게
        overlay = win.loading
        overlay.want('저역 통합 계산 중…', 6.0, cancel=lambda: None)
        _pump(qapp, 0.1)
        assert not overlay.isVisible()                    # 아직 300 ms 전
        _pump(qapp, 0.4)
        assert overlay.isVisible()
        assert overlay.indicator.running()                # 움직이는 자리가 돈다
        assert overlay.geometry() == win.stack.rect()     # 단계 화면 전체를 덮는다
        assert overlay.card.width() >= overlay.indicator.width() + 56   # 좌우 28 px 여백 유지
        assert overlay.card.height() > overlay.indicator.height() + 44  # 160 px 로고 아래 글·버튼 유지
        assert overlay.cancel_button.isVisibleTo(overlay)
        overlay.clear()
        assert not overlay.isVisible() and not overlay.indicator.running()
    finally:
        win.close()
        qapp.processEvents()


def test_short_job_never_flashes(qapp, project, fake, tmp_path):
    win = _shown_window(project, fake, tmp_path)
    try:
        win._busy_timer.stop()
        win.loading.want('필터 계산 중…')
        _pump(qapp, 0.15)
        win.loading.clear()
        _pump(qapp, 0.4)
        assert not win.loading.isVisible()
    finally:
        win.close()
        qapp.processEvents()


def _mdat_with_parse_time(qapp, win, monkeypatch, tmp_path, seconds: float) -> list[bool]:
    """mdat 읽기가 `seconds` 걸리게 바꿔 불러오고, 그동안 오버레이가 보였는지 50 ms 마다 적는다."""
    from PyQt6 import QtCore

    import secs.mdat_loader as loader

    def slow_parse(_path):
        time.sleep(seconds)
        raise ValueError('테스트용 가짜 파일')

    monkeypatch.setattr(loader, 'load_mdat_project', slow_parse)
    seen: list[bool] = []
    probe = QtCore.QTimer()
    probe.setInterval(50)
    probe.timeout.connect(lambda: seen.append(win.loading.isVisible()))
    probe.start()
    win.go(5)
    fake_file = tmp_path / 'x.mdat'
    fake_file.write_bytes(b'')
    assert win.measure_page.load_mdat(str(fake_file), assignment=[]) == 0
    probe.stop()
    _pump(qapp, 0.3)
    return seen


def test_long_mdat_load_shows_the_overlay_then_hides_it(qapp, project, fake, tmp_path, monkeypatch):
    win = _shown_window(project, fake, tmp_path)
    try:
        seen = _mdat_with_parse_time(qapp, win, monkeypatch, tmp_path, 1.0)
        assert any(seen), '1 초 걸린 mdat 읽기 동안 오버레이가 한 번도 안 보였다'
        assert not win.loading.isVisible()
    finally:
        win.close()
        qapp.processEvents()


def test_quick_mdat_load_shows_nothing(qapp, project, fake, tmp_path, monkeypatch):
    win = _shown_window(project, fake, tmp_path)
    try:
        seen = _mdat_with_parse_time(qapp, win, monkeypatch, tmp_path, 0.05)
        assert not any(seen)
        assert not win.loading.isVisible()
    finally:
        win.close()
        qapp.processEvents()


def test_indicator_is_a_self_contained_widget():
    """오버레이가 쓰는 표시기 약속은 SIZE·start·stop·running 이다."""
    from secs.wizard.loading import LoadingIndicator

    for name in ('SIZE', 'start', 'stop', 'running'):
        assert hasattr(LoadingIndicator, name)


def test_indicator_loads_the_24_frame_sprite_once(qapp):
    from secs.wizard.loading import LoadingIndicator

    first = LoadingIndicator()
    second = LoadingIndicator()
    assert first.size().width() == first.size().height() == 160
    assert first._sprite is not None
    assert first._sprite is second._sprite
    assert first._sprite.width() == 24 * 320
    assert first._sprite.height() == 320
    assert first.FRAME_MS == round(1067 / 24)


def test_indicator_falls_back_to_eight_dots_when_sprite_is_missing(qapp, monkeypatch, tmp_path):
    from PyQt6 import QtGui

    from secs.wizard.loading import LoadingIndicator

    monkeypatch.setattr(LoadingIndicator, 'SPRITE_PATH', tmp_path / 'missing.png')
    monkeypatch.setattr(LoadingIndicator, '_sprite_cache', None)
    monkeypatch.setattr(LoadingIndicator, '_sprite_load_attempted', False)
    indicator = LoadingIndicator()
    assert indicator._sprite is None
    image = QtGui.QImage(160, 160, QtGui.QImage.Format.Format_ARGB32)
    image.fill(0)
    indicator.render(image)  # 대체 경로가 실제 paintEvent까지 예외 없이 그린다.
    assert any(image.pixelColor(x, y).alpha() for x in range(160) for y in range(160))
