"""창을 먼저 띄우고 REW 연결은 뒤에서 — 창을 만드는 동안 네트워크에 가지 않는다 (2026-09-20).

사용자: "프로그램은 더 빨리 열리도록 해줘. rew 연결은 프로그램 열고 안에서 해도 되잖아"
"""
from __future__ import annotations

import time

import pytest

from test_wizard_common import PRESET, engine, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


class _Session:
    """연결 시도를 세는 세션. `alive` 면 /version 에만 답한다."""

    def __init__(self, alive=False):
        self.alive = alive
        self.calls = []

    def request(self, method, url, **kwargs):
        self.calls.append(url)
        if not self.alive:
            raise ConnectionError('refused')

        class _Response:
            status_code = 200
            text = '{"message": "5.40 Beta 124 API 0.9.4"}'

            @staticmethod
            def json():
                return {'message': '5.40 Beta 124 API 0.9.4'}

        return _Response()


def _real(alive=False):
    from secs.rewapi.rew import RealREW

    session = _Session(alive)
    return RealREW(session=session), session


def test_building_the_window_makes_no_network_calls(qapp, project):
    from secs.wizard.window import WizardWindow

    rew, session = _real(alive=False)
    rew.hold_offline(60.0)
    started = time.perf_counter()
    win = WizardWindow(project, rew=rew, devices=rew, levels=rew, measurer=rew)
    try:
        for step in (2, 3, 4, 1):
            win.go(step)
        assert session.calls == []                       # 창을 만들고 다니는 동안 REW 를 부르지 않았다
        assert time.perf_counter() - started < 10.0
    finally:
        win.close()
        qapp.processEvents()


def test_probe_ignores_the_hold_and_reopens_it_when_rew_is_down(engine):
    from secs.rewapi import rew as module

    rew, session = _real(alive=False)
    rew.hold_offline(60.0)
    assert rew.is_running() is False and session.calls == []
    assert rew.probe() is False and len(session.calls) == 1          # 실제로 한 번 물어봤다
    assert rew.is_running() is False and len(session.calls) == 1     # 다시 바로 실패(화면이 안 막힌다)
    assert rew._down_until - time.monotonic() > module.PROBE_HOLD_S - 1.0


def test_watcher_reports_rew_coming_up_and_the_sidebar_follows(qapp, project):
    from secs.wizard.window import WizardWindow

    rew, session = _real(alive=False)
    rew.hold_offline(60.0)
    win = WizardWindow(project, rew=rew, devices=rew, levels=rew, measurer=rew)
    win.REW_WATCH_S = 0.05
    try:
        assert win.start_rew_watch() is True
        assert win.start_rew_watch() is False                        # 두 번 켜지지 않는다
        deadline = time.monotonic() + 3.0
        while win._rew_running is None and time.monotonic() < deadline:
            qapp.processEvents()
            time.sleep(0.01)
        assert win._rew_running is False
        session.alive = True                                         # 사용자가 REW 를 켰다
        deadline = time.monotonic() + 3.0
        while win._rew_running is not True and time.monotonic() < deadline:
            qapp.processEvents()
            time.sleep(0.01)
        assert win._rew_running is True
        assert '4735' in win.side_rew.text()                         # 사이드바가 연결됨으로 바뀌었다
    finally:
        win.close()
        qapp.processEvents()
    assert win._rew_watch_stop.is_set()


def test_fake_backend_needs_no_watcher(qapp, project):
    from secs.rewapi.fake import FakeREW
    from secs.wizard.window import WizardWindow

    fake = FakeREW(delay_s=0.0)
    win = WizardWindow(project, rew=fake, devices=fake, levels=fake, measurer=fake)
    try:
        assert win.start_rew_watch() is False
    finally:
        win.close()
        qapp.processEvents()
