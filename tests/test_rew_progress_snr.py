"""REW 진행 문자열 → 5단계 진행률, 낮은 SNR 경고 (P4 앞 작은 항목 3).

콜백 본문과 SNR 필드는 2026-09-18 실스윕으로 확인한 실제 모양이다.
"""
from __future__ import annotations

import json
import threading
import urllib.request

import pytest

from conftest import load_engine
from test_rewapi_real import StubSession, _Response, _client


@pytest.fixture(scope="module")
def engine():
    return load_engine("package")


@pytest.fixture()
def rew_mod(engine):
    import secs.rewapi.rew as module
    return module


@pytest.fixture(scope="module")
def qapp():
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance() or QApplication([])
    yield app
    app.processEvents()


class CallingStub(StubSession):
    """측정 명령을 받으면 REW 처럼 구독 URL 로 진행 문자열을 POST 한다."""

    BODIES = ("0% Capturing noise floor...50%", "41% Remaining sweeps: 1, time: 4 s",
              "100% Measurement complete")

    def __init__(self, snr: float | None = 12.9, **kwargs) -> None:
        super().__init__(**kwargs)
        self.snr = snr
        self.subscriber: str | None = None

    def _route(self, method, path, body):
        if path == "/measure/subscribe":
            self.subscriber = (body or {}).get("url")
        if path == "/measure/unsubscribe":
            self.subscriber = None
        if path == "/measure/command" and (body or {}).get("command") == "SPL" and self.subscriber:
            for text in self.BODIES:
                request = urllib.request.Request(
                    self.subscriber, data=json.dumps(text).encode("utf-8"),
                    headers={"Content-Type": "application/json"}, method="POST")
                urllib.request.urlopen(request, timeout=5).read()
        if method == "GET" and path.startswith("/measurements/m") and path.count("/") == 2:
            summary = {"uuid": path.split("/")[2], "title": "L P1"}
            if self.snr is not None:
                summary["signalToNoisedB"] = self.snr
            return _Response(200, summary)
        return super()._route(method, path, body)


def test_rew_progress_strings_reach_the_progress_callback(rew_mod, engine):
    import secs.rewapi as rewapi

    stub = CallingStub(channels=["L", "R"])
    client = _client(rew_mod, stub)
    events: list[dict] = []
    try:
        results = client.run_position(rewapi.fake_plan(["L"], [1]), 1, events.append,
                                      threading.Event())
    finally:
        client.close()

    running = [e for e in events if e["phase"] == "running"]
    assert [round(e["percent"], 1) for e in running] == [0.0, 36.9, 90.0]     # 0~90 % 로 환산
    assert running[1]["message"] == "Remaining sweeps: 1, time: 4 s"
    assert all(e["role"] == "L" for e in running)
    assert events[-1]["phase"] == "done"
    assert stub.subscriber is None                   # 끝나면 구독을 푼다
    assert results[0].snr_db == pytest.approx(12.9)


def test_missing_snr_is_none(rew_mod, engine):
    import secs.rewapi as rewapi

    stub = CallingStub(snr=None, channels=["L"])
    client = _client(rew_mod, stub)
    try:
        results = client.run_position(rewapi.fake_plan(["L"], [1]), 1, None, threading.Event())
    finally:
        client.close()
    assert results[0].snr_db is None
    assert "/measure/subscribe" not in stub.paths()       # 콜백이 없으면 구독도 안 한다


def test_step5_warns_about_low_snr(qapp, engine, tmp_path):
    import numpy as np
    import secs.model as model
    import secs.rewapi as rewapi
    from secs.rewapi.interfaces import MeasurementResult
    from secs.wizard import WizardWindow

    project = model.Project(config=model.SpeakerConfig.from_preset("2.0", biamp=False))
    rew = rewapi.FakeREW(delay_s=0.0)
    window = WizardWindow(project, rew, rew, rew, rew)
    try:
        window.go(5)
        step = window.step5
        lines: list[str] = []
        step._append = lines.append                       # 로그 줄만 본다
        ir = np.zeros(4096, dtype=np.float32)
        ir[100] = 1.0
        quiet = MeasurementResult("L", 1, "a", ir, 48000, "L P1", snr_db=12.9)
        fine = MeasurementResult("R", 1, "b", ir, 48000, "R P1", snr_db=45.0)
        step._on_finished(1, [quiet, fine])
        warning = [text for text in lines if "S/N" in text]           # 10-10 현장 말: 신호 대 잡음 → S/N
        assert len(warning) == 1
        assert "L 13 dB" in warning[0] and "R " not in warning[0]

        lines.clear()
        step._on_finished(1, [fine])
        assert not [text for text in lines if "신호 대 잡음" in text]
    finally:
        window.close()
        qapp.processEvents()
