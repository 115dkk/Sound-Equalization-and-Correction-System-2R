"""REW 가 꺼져 있을 때 화면이 멈추지 않게 — 연결 실패 직후에는 바로 실패한다 (2026-09-20).

사용자 보고: REW 없이 `SECS 2.0 Alpha1.exe` 를 켜면 창이 안 떴다. 1단계 표가 줄마다 장치 목록을
물었고, 꺼진 REW 에는 요청 하나가 2~4 s 걸려 9.1.6 에서 몇 분이 됐다.
"""
from __future__ import annotations

import pytest

from conftest import load_engine


class _DeadSession:
    """연결이 항상 거부되는 세션. 몇 번 불렸는지 센다."""

    def __init__(self):
        self.calls = 0
        self.timeouts = []

    def request(self, method, url, **kwargs):
        self.calls += 1
        self.timeouts.append(kwargs.get('timeout'))
        raise ConnectionError('refused')


@pytest.fixture()
def rew_module():
    load_engine("package")
    import secs.rewapi.rew as module
    return module


def test_second_request_fails_without_touching_the_network(rew_module):
    session = _DeadSession()
    rew = rew_module.RealREW(session=session)
    assert rew.is_running() is False
    assert session.calls == 1
    for _ in range(50):                       # 화면 한 번 그릴 때 묻는 횟수쯤
        assert rew.is_running() is False
        assert list(rew.output_devices()) == [] if hasattr(rew, 'output_devices') else True
    assert session.calls == 1                 # 실제 연결 시도는 처음 한 번뿐


def test_retries_after_the_window_and_uses_a_short_connect_timeout(rew_module, monkeypatch):
    session = _DeadSession()
    rew = rew_module.RealREW(session=session)
    rew.is_running()
    connect, read = session.timeouts[0]
    assert connect <= rew_module.CONNECT_TIMEOUT_S and read >= connect
    rew._down_until = 0.0                     # 창이 지났다고 치면
    rew.is_running()
    assert session.calls == 2                 # 다시 실제로 시도한다
