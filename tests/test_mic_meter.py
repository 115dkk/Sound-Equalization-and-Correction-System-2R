# -*- coding: utf-8 -*-
"""마이크 입력 미터 창(10-07) — 폴링 · 멈춘 REW 모니터 재시작 · 표시."""
import time

import pytest

pytest.importorskip('PyQt6')

from test_wizard_common import engine, qapp  # noqa: F401,E402  (src 경로를 sys.path 에 넣는다)
from test_wizard_common import pytestmark  # noqa: F401,E402



class _Levels:
    """`last_levels`·`get`·`post` 만 흉내 낸다. `frozen` 이면 같은 값을 계속 준다(멈춘 REW 모니터)."""

    level_index = 0

    @staticmethod
    def _level_payload(payload, index=0):
        from secs.rewapi.rew import RealREW
        return RealREW._level_payload(payload, index)

    def __init__(self, frozen: bool) -> None:
        self.frozen = frozen
        self.posts: list = []
        self.n = 0

    def get(self, path, timeout=None):
        return {'input': '1:MeasurementMic'}

    def post(self, path, body=None, timeout=None):
        self.posts.append((path, body))
        self.frozen = False

    def last_levels(self):
        self.n += 1
        v = -484.0 if self.frozen else -40.0 - (self.n % 3)
        return {'unit': 'dBFS', 'rms': [v], 'peak': [v + 8], 'timeSpanSeconds': 0.17}


def _run(qtbot_app, levels, seconds):
    from secs.wizard.mic_meter import MicMeterWindow
    win = MicMeterWindow(levels)
    win.show()
    end = time.time() + seconds
    while time.time() < end:
        qtbot_app.processEvents()
        time.sleep(0.02)
    win.close()
    return win


@pytest.fixture()
def app(monkeypatch, engine, qapp):  # noqa: F811
    from secs.wizard import mic_meter
    monkeypatch.setattr(mic_meter, 'POLL_S', 0.02)
    return qapp


def test_frozen_monitor_is_restarted(app):
    levels = _Levels(frozen=True)
    win = _run(app, levels, 0.6)
    assert ('/input-levels/command', {'command': 'Start'}) in levels.posts
    assert 'dBFS' in win.value.text() and win.value.text().startswith('-4')
    assert win.name.text() == '1:MeasurementMic'


def test_live_levels_no_restart(app):
    levels = _Levels(frozen=False)
    win = _run(app, levels, 0.4)
    assert levels.posts == []
    assert '-3' in win.peak.text()          # 피크 = rms + 8 ≈ -32~-34


def test_offline_without_backend(app):
    win = _run(app, None, 0.2)
    assert win.value.text() == '—'
