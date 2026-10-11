"""느린 REW 에서도 단계 이동이 화면 스레드를 막지 않는다 (2026-09-23).

사용자 보고(실제 exe + 실제 REW): "각 단계별로 이동할 때 딜레이가 굉장히 심하다." FakeREW 는 즉시 답해서
드러나지 않았지만, 화면 스레드가 단계 진입·새로 그리기·다음 버튼 판정마다 REW 에 동기 HTTP 를 불렀다
(장치 단계 한 번에 10~14회, `tools/slow_rew_profile.py`). 이제 화면은 `RewCache` 만 읽고 REW 에는
워커 스레드만 간다. 여기서는 호출마다 쉬는 데모 REW 로 그것을 지킨다.
"""
from __future__ import annotations

import threading
import time

from test_wizard_common import PRESET, engine, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)

DELAY_S = 0.15


class SlowREW:
    """FakeREW 를 감싸 모든 메서드 호출에 `DELAY_S` 를 더하고 (이름, 화면 스레드?) 를 기록한다.

    `probe()` 가 있으므로 창은 이것을 실제 REW 처럼(원격) 다룬다.
    """

    def __init__(self, delay_s: float = DELAY_S):
        from secs.rewapi.fake import FakeREW

        self._inner = FakeREW(delay_s=0.0)
        self._delay = delay_s
        self.calls: list[tuple[str, bool]] = []
        self._lock = threading.Lock()

    def _log(self, name: str) -> None:
        with self._lock:
            self.calls.append((name, threading.current_thread() is threading.main_thread()))
        time.sleep(self._delay)

    def probe(self) -> bool:
        self._log('probe')
        return self._inner.is_running()

    def __getattr__(self, name):
        value = getattr(self._inner, name)
        if not callable(value) or name.startswith('_'):
            return value

        def slow(*args, **kwargs):
            self._log(name)
            return value(*args, **kwargs)

        return slow

    def gui_calls(self, since: int = 0) -> list[str]:
        with self._lock:
            return [name for name, gui in self.calls[since:] if gui]


def _pump(qapp, seconds: float, until=None) -> bool:
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        qapp.processEvents()
        if until is not None and until():
            return True
        time.sleep(0.01)
    return bool(until()) if until is not None else True


def _window(project, slow):
    from secs.rewapi.fake import FakeREW
    from secs.wizard.window import WizardWindow

    win = WizardWindow(project, rew=slow, devices=slow, levels=slow, measurer=FakeREW(delay_s=0.0))
    win.REW_WATCH_S = 0.05
    return win


def test_building_the_window_does_not_call_rew_on_the_gui_thread(qapp, project):
    slow = SlowREW()
    win = _window(project, slow)
    try:
        assert slow.gui_calls() == []
    finally:
        win.close()
        qapp.processEvents()


def test_go_never_calls_rew_on_the_gui_thread_and_stays_fast(qapp, project):
    from secs.wizard.strings import FLOW

    slow = SlowREW()
    win = _window(project, slow)
    try:
        assert win.start_rew_watch() is True
        # 백그라운드 점검이 첫 값을 캐시에 채울 때까지(요청 7~8개 × 0.15 s)
        assert _pump(qapp, 10.0, until=lambda: win.rew_cache.snapshot().running is True
                     and bool(win.rew_cache.snapshot().outputs))
        start = len(slow.calls)
        worst = 0.0
        order = list(FLOW) + list(reversed(FLOW[:-1]))     # 화면 번호는 FLOW 로(6 · 7 은 2026-10-10 에 지움)
        for step in order:
            t0 = time.perf_counter()
            win.go(step)
            worst = max(worst, time.perf_counter() - t0)
            _pump(qapp, 0.05)
        assert slow.gui_calls(start) == []
        # REW 를 한 번만 불러도 0.15 s 다 — 그보다 훨씬 짧아야 한다(목표 50 ms, 시험 기계 여유를 둔다).
        assert worst < 0.12, f'가장 느린 go() {worst * 1000:.0f} ms'
    finally:
        win.close()
        qapp.processEvents()


def test_device_page_shows_cached_devices_and_buttons_do_not_block(qapp, project):
    slow = SlowREW()
    win = _window(project, slow)
    try:
        win.start_rew_watch()
        assert _pump(qapp, 10.0, until=lambda: bool(win.rew_cache.snapshot().outputs))
        page = win.devices_page
        win.go(1)
        assert 'Demo ASIO 16ch' in page.output_combo.itemText(0)
        assert '4735' in win.side_rew.text()
        start = len(slow.calls)
        t0 = time.perf_counter()
        page.read_button.click()                 # [설정 읽기] — 워커가 읽고 끝나면 보인다
        page.retry_button.click()                # [다시 확인]
        assert time.perf_counter() - t0 < 0.12
        assert _pump(qapp, 10.0, until=lambda: 'dBFS' in page.rew_hint.text())
        assert '-20 dBFS' in page.rew_hint.text()
        assert slow.gui_calls(start) == []
    finally:
        win.close()
        qapp.processEvents()


def test_level_page_sends_monitor_commands_off_the_gui_thread(qapp, project):
    slow = SlowREW()
    win = _window(project, slow)
    try:
        start = len(slow.calls)
        win.go(3)
        role = project.config.speakers[0].role
        win.step3.play(role, 1)
        win.go(4)
        assert slow.gui_calls(start) == []
        # 보낸 명령은 워커가 차례로 보냈다(구독 → 재생 → 정지 …)
        assert _pump(qapp, 5.0, until=lambda: any(n == 'stop' for n, _g in slow.calls[start:]))
        names = [n for n, _g in slow.calls[start:]]
        assert names.index('start') < names.index('play_pink') < names.index('stop')
    finally:
        win.close()
        qapp.processEvents()
