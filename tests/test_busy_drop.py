"""작은 로고 방울 — 화면을 덮지 않는 대기에도 애니메이션 (2026-09-24 사용자 요청).

"측정이나 연산 등 사용자가 딜레이를 느낄 만한 지점에는 다 애니메이션을 넣어 줘." 큰 로딩 카드와 같은
스프라이트를 48 px 로 줄여 구석 진행 패널(`BusyPanel`)·1단계 REW 확인·5단계 측정에 쓴다. 모두 오버레이와
같은 규칙 — 300 ms 넘게 도는 일에만 보이고, 끝나면 멈추고 숨는다.
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

    win = make_window(project, fake, tmp_path / 'd.secs')
    win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    win.resize(1280, 800)
    win.show()
    return win


# ──────────────────────────── 표시기 크기 ────────────────────────────
def test_indicator_takes_a_size_and_keeps_160_by_default(qapp):
    from secs.wizard.loading import LoadingIndicator

    assert LoadingIndicator().indicator_size() == 160
    small = LoadingIndicator(size=48)
    assert small.width() == small.height() == 48 == small.indicator_size()


def test_small_frames_are_pre_scaled_per_physical_size(qapp):
    """작게 그릴 때는 원본을 매번 줄이지 않고 물리 px 로 면적 평균해 둔 24장을 쓴다(선명도)."""
    from secs.wizard.loading import LoadingIndicator

    drop = LoadingIndicator(size=48)
    frames = drop.frames_for(48)
    assert len(frames) == 24 and all(f.width() == f.height() == 48 for f in frames)
    assert drop.frames_for(48) is frames                          # 캐시
    hi = drop.frames_for(96)                                      # devicePixelRatio 2
    assert hi[0].width() == 96
    assert drop.frames_for(9999)[0].width() == LoadingIndicator.SPRITE_FRAME_PX   # 늘리지 않는다


def test_small_indicator_actually_paints(qapp):
    from PyQt6 import QtGui

    from secs.wizard.loading import LoadingIndicator

    drop = LoadingIndicator(size=48)
    image = QtGui.QImage(48, 48, QtGui.QImage.Format.Format_ARGB32)
    image.fill(0)
    drop.render(image)
    assert sum(1 for x in range(48) for y in range(48) if image.pixelColor(x, y).alpha()) > 100


def test_small_fallback_dots_fit_inside(qapp, monkeypatch, tmp_path):
    from PyQt6 import QtGui

    from secs.wizard.loading import LoadingIndicator

    monkeypatch.setattr(LoadingIndicator, 'SPRITE_PATH', tmp_path / 'missing.png')
    monkeypatch.setattr(LoadingIndicator, '_sprite_cache', None)
    monkeypatch.setattr(LoadingIndicator, '_sprite_load_attempted', False)
    drop = LoadingIndicator(size=24)
    assert drop._sprite is None
    image = QtGui.QImage(24, 24, QtGui.QImage.Format.Format_ARGB32)
    image.fill(0)
    drop.render(image)
    assert any(image.pixelColor(x, y).alpha() for x in range(24) for y in range(24))


# ──────────────────────────── BusyDrop: 300 ms 규칙 ────────────────────────────
def test_busy_drop_waits_then_spins_then_hides(qapp):
    from PyQt6 import QtWidgets

    from secs.wizard.loading import BusyDrop

    host = QtWidgets.QWidget()
    drop = BusyDrop(host)
    assert drop.indicator_size() == 48 and not drop.isVisibleTo(host)
    drop.set_busy(True)
    _pump(qapp, 0.1)
    assert not drop.isVisibleTo(host) and not drop.running()     # 아직 300 ms 전
    _pump(qapp, 0.35)
    assert drop.isVisibleTo(host) and drop.running()
    drop.set_busy(False)
    assert not drop.isVisibleTo(host) and not drop.running()


def test_busy_drop_never_flashes_for_short_work(qapp):
    from PyQt6 import QtWidgets

    from secs.wizard.loading import BusyDrop

    host = QtWidgets.QWidget()
    drop = BusyDrop(host)
    drop.set_busy(True)
    _pump(qapp, 0.15)
    drop.set_busy(False)
    _pump(qapp, 0.4)
    assert not drop.isVisibleTo(host) and not drop.running()


def test_keep_space_drop_holds_its_place_when_hidden(qapp):
    from secs.wizard.loading import BusyDrop

    assert BusyDrop(keep_space=True).sizePolicy().retainSizeWhenHidden()
    assert not BusyDrop().sizePolicy().retainSizeWhenHidden()


# ──────────────────────────── 구석 진행 패널 ────────────────────────────
def test_busy_panel_has_a_spinning_drop_beside_the_text(qapp, project, fake, tmp_path):
    from secs.wizard.busy import poll

    win = _shown_window(project, fake, tmp_path)
    try:
        win._busy_timer.stop()
        panel = win.busy_panel
        jobs = [('측정 중…', lambda: True, lambda: 12.0)]
        poll(panel, jobs)
        _pump(qapp, 0.1)
        assert not panel.isVisible()                               # 짧은 일엔 안 뜬다
        poll(panel, jobs)
        _pump(qapp, 0.35)
        assert panel.isVisible() and panel.drop.running()
        assert panel.drop.width() == 48
        drop, text, bar = panel.drop.geometry(), panel.text.geometry(), panel.bar.geometry()
        assert drop.right() < text.left() and drop.right() < bar.left()   # 겹치지 않음
        assert panel.rect().contains(drop) and panel.rect().contains(text)
        assert text.width() >= panel.text.sizeHint().width()              # 글자 안 잘림
        assert '측정 중' in panel.text.text()
        poll(panel, [('측정 중…', lambda: False, lambda: 0.0)])
        assert not panel.isVisible() and not panel.drop.running()
    finally:
        win.close()
        qapp.processEvents()


def test_busy_panel_short_job_never_flashes(qapp, project, fake, tmp_path):
    from secs.wizard.busy import poll

    win = _shown_window(project, fake, tmp_path)
    try:
        win._busy_timer.stop()
        panel = win.busy_panel
        poll(panel, [('필터 계산 중…', lambda: True, lambda: 0.0)])
        _pump(qapp, 0.15)
        poll(panel, [('필터 계산 중…', lambda: False, lambda: 0.0)])
        _pump(qapp, 0.4)
        assert not panel.isVisible()
    finally:
        win.close()
        qapp.processEvents()


# ──────────────────────────── 단계 안의 방울 ────────────────────────────
def test_step1_drop_spins_while_checking_rew(qapp, project, fake, tmp_path):
    win = _shown_window(project, fake, tmp_path)
    try:
        win._busy_timer.stop()
        page = win.devices_page
        win.go(1)
        page._checking = True
        win._poll_busy()
        _pump(qapp, 0.4)
        win._poll_busy()
        assert page.busy_drop.isVisible() and page.busy_drop.running()
        # 상태 배지와 같은 줄, 겹치지 않음
        pill = page.status_pill.geometry()
        drop = page.busy_drop.geometry()
        assert drop.left() > pill.right()
        assert abs(drop.center().y() - pill.center().y()) <= 2
        page._checking = False
        win._poll_busy()
        assert not page.busy_drop.isVisible() and not page.busy_drop.running()
    finally:
        win.close()
        qapp.processEvents()


def test_step5_drop_spins_during_the_countdown_and_sweep(qapp, project, fake, tmp_path):
    win = _shown_window(project, fake, tmp_path)
    try:
        win._busy_timer.stop()
        page = win.measure_page
        win.go(5)
        assert page.busy_drop.indicator_size() == 48
        before = page.log.geometry()
        page._countdown_left = 3                       # 카운트다운도 "기다리는 중"
        win._poll_busy()
        _pump(qapp, 0.4)
        win._poll_busy()
        assert page.busy_drop.isVisible() and page.busy_drop.running()
        assert page.log.geometry() == before           # 로그가 밀리거나 가려지지 않는다(자리를 늘 지킴)
        assert not page.busy_drop.geometry().intersects(page.log.geometry())
        page._countdown_left = 0
        win._poll_busy()
        assert not page.busy_drop.isVisible()
    finally:
        win.close()
        qapp.processEvents()


def test_sample_fill_runs_off_the_gui_thread_with_overlay(qapp, project, fake, tmp_path, monkeypatch):
    """샘플 데이터 채우기도 워커에서 — 오래 걸리면 로딩 카드('샘플 데이터를 불러오는 중…')가 뜬다."""
    from PyQt6 import QtCore

    import secs.wizard.step5_measure as step5

    win = _shown_window(project, fake, tmp_path)
    try:
        win.go(5)
        original = step5.magnitude_db

        def slow(ir, rate):
            time.sleep(0.15)
            return original(ir, rate)

        monkeypatch.setattr(step5, 'magnitude_db', slow)
        seen: list[str] = []
        probe = QtCore.QTimer()
        probe.setInterval(50)
        probe.timeout.connect(lambda: seen.append(win.loading.text.text() if win.loading.isVisible() else ''))
        probe.start()
        filled = win.measure_page.load_sample(positions=1)
        probe.stop()
        assert filled == 1
        assert any('샘플' in text for text in seen)
        _pump(qapp, 0.4)
        assert not win.loading.isVisible()
    finally:
        win.close()
        qapp.processEvents()
