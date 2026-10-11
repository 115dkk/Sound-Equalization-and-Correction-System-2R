"""P2 B `secs.rewapi.fake` 검사 — REW 없이 도는 가짜 구현.

기준: `P2 인터페이스 계약 2026-09-16.md` 의 "B: `secs.rewapi`" 절.
"""
from __future__ import annotations

import threading
import time

import numpy as np
import pytest

from conftest import engine_choice, load_engine

pytestmark = pytest.mark.skipif(
    engine_choice() != "package",
    reason="secs.rewapi 는 패키지 엔진 전용이다 (SECS_ENGINE=package).",
)


@pytest.fixture(scope="module")
def rewapi():
    load_engine("package")              # src 를 sys.path 에 올린다
    import secs.rewapi as module        # type: ignore[import-not-found]
    return module


@pytest.fixture()
def fake(rewapi):
    rew = rewapi.FakeREW(delay_s=0.0, level_interval_s=0.005)
    yield rew
    rew.close()


# ── 계약: 다섯 프로토콜 ────────────────────────────────────────────────
def test_fake_satisfies_all_protocols(rewapi, fake):
    for proto in (rewapi.RewStatus, rewapi.DeviceLister, rewapi.LevelMonitor,
                  rewapi.Measurer, rewapi.IRProvider):
        assert isinstance(fake, proto), proto.__name__


def test_status_and_devices(rewapi, fake):
    assert fake.is_running() is True
    assert fake.is_pro() is True
    assert isinstance(fake.version(), str)
    outs = fake.output_devices()
    ins = fake.input_devices()
    assert outs and ins
    assert all(d.kind == "output" for d in outs)
    assert all(d.kind == "input" for d in ins)
    assert all(isinstance(d, rewapi.DeviceInfo) for d in outs + ins)

    fake.set_running(False)
    assert fake.is_running() is False
    assert fake.version() is None
    assert fake.is_pro() is False


# ── MeasurementPlan ───────────────────────────────────────────────────
def test_plan_validate_and_naming(rewapi):
    plan = rewapi.fake_plan(["L", "R", "SW1"], [1, 2, 3])
    assert plan.validate() == []
    assert plan.name_for("L", 2) == "L P2"
    assert plan.output_for("SW1") == 3
    assert plan.total_steps() == 9

    bad = rewapi.MeasurementPlan(roles=["L", "L"], outputs={"L": 1}, positions=[1, 1],
                                 repetitions=3, sweep_length="300k")
    msgs = bad.validate()
    assert any("역할 중복" in m for m in msgs)
    assert any("위치 번호 중복" in m for m in msgs)
    assert any("반복" in m for m in msgs)          # acoustic 에서 반복 1 규칙
    assert any("스윕 길이" in m for m in msgs)


def test_split_role_and_sweep_length(rewapi):
    assert rewapi.split_role("L:LF") == ("L", "LF")
    assert rewapi.split_role("SW1") == ("SW1", None)
    assert rewapi.sweep_length_samples("256k") == 262144
    assert rewapi.sweep_length_samples("1M") == 1048576


def test_plan_from_project_expands_multiamp_roles(rewapi):
    load_engine("package")
    import secs.model as model  # type: ignore[import-not-found]

    project = model.Project(config=model.SpeakerConfig.from_preset("2.1", biamp=True))   # L, R 바이앰프 + SW1 (제품 기본은 09-22 부터 바이앰프 끔)
    plan = rewapi.plan_from_project(project, [1, 2])
    assert plan.roles == ["L:LF", "L:HF", "R:LF", "R:HF", "SW1"]
    assert plan.outputs == {"L:LF": 1, "L:HF": 2, "R:LF": 3, "R:HF": 4, "SW1": 5}
    assert plan.positions == [1, 2]
    assert plan.validate() == []


# ── 측정 ──────────────────────────────────────────────────────────────
def test_run_position_returns_ir_per_role(rewapi, fake):
    plan = rewapi.fake_plan(["L", "R", "SW1"], [1])
    events: list[dict] = []
    results = fake.run_position(plan, 1, events.append, threading.Event())

    assert [r.role for r in results] == ["L", "R", "SW1"]
    for result in results:
        assert isinstance(result.ir, np.ndarray)
        assert result.ir.dtype == np.float32
        assert result.ir.size > 1000
        assert result.sample_rate == 48000
        assert result.pos == 1
        assert result.name == f"{result.role} P1"
        assert np.isfinite(result.ir).all()

    phases = {e["phase"] for e in events}
    assert {"sweep", "process", "done"} <= phases
    assert all(set(e) == {"role", "pos", "phase", "percent", "message"} for e in events)
    assert all(0.0 <= e["percent"] <= 100.0 for e in events)
    assert events[-1]["phase"] == "done" and events[-1]["percent"] == 100.0


def test_ir_is_deterministic_and_position_dependent(rewapi, fake):
    a = fake.ir_for("L", 1)
    b = fake.ir_for("L", 1)
    c = fake.ir_for("L", 2)
    assert np.array_equal(a, b)                      # 같은 입력 → 비트 동일
    assert not np.array_equal(a, c)                  # 위치마다 소량 변화
    assert a.shape == c.shape
    assert abs(float(np.abs(c).max())) < abs(float(np.abs(a).max()))   # 위치당 -0.4 dB

    other = rewapi.FakeREW(delay_s=0.0)
    try:
        assert np.array_equal(a, other.ir_for("L", 1))    # 인스턴스가 달라도 같다
    finally:
        other.close()


def test_ir_provider_round_trip(rewapi, fake):
    plan = rewapi.fake_plan(["L"], [1])
    result = fake.run_position(plan, 1, None, threading.Event())[0]
    ir, rate = fake.get_ir(result.uuid)
    assert np.array_equal(ir, result.ir)
    assert rate == result.sample_rate
    with pytest.raises(KeyError):
        fake.get_ir("없는-uuid")


def test_cancel_stops_after_current_role(rewapi):
    rew = rewapi.FakeREW(delay_s=0.02)
    try:
        plan = rewapi.fake_plan(["L", "R", "SW1"], [1])
        cancel = threading.Event()
        events: list[dict] = []

        def on_progress(payload: dict) -> None:
            events.append(payload)
            if payload["role"] == "R" and payload["phase"] == "sweep":
                cancel.set()

        results = rew.run_position(plan, 1, on_progress, cancel)
        assert [r.role for r in results] == ["L"]           # 취소 전까지만
        assert events[-1]["phase"] == "cancelled"
    finally:
        rew.close()


def test_delay_s_controls_sweep_time(rewapi):
    rew = rewapi.FakeREW(delay_s=0.1)
    try:
        plan = rewapi.fake_plan(["L"], [1])
        start = time.monotonic()
        rew.run_position(plan, 1, None, threading.Event())
        assert time.monotonic() - start >= 0.09
    finally:
        rew.close()


# ── 레벨 모니터 ───────────────────────────────────────────────────────
def test_level_monitor_and_generator(rewapi, fake):
    got: list[dict] = []
    fake.play_pink("L", 1, -20.0)
    assert fake.generator_running is True
    fake.start(got.append)
    deadline = time.monotonic() + 2.0
    while len(got) < 3 and time.monotonic() < deadline:
        time.sleep(0.01)
    fake.stop()
    fake.stop_generator()

    assert len(got) >= 3
    assert fake.level_running is False
    assert fake.generator_running is False
    for sample in got:
        assert set(sample) == {"input_dbfs", "spl_db", "clip"}
        assert isinstance(sample["clip"], bool)
        assert -120.0 < sample["input_dbfs"] < 0.0
    assert all(s["spl_db"] is not None for s in got)      # 재생 중에는 SPL 표시
    assert ("play_pink", {"role": "L", "output": 1, "level_db": -20.0}) in fake.calls


def test_level_without_generator_is_noise_floor(rewapi, fake):
    got: list[dict] = []
    fake.start(got.append)
    deadline = time.monotonic() + 2.0
    while not got and time.monotonic() < deadline:
        time.sleep(0.01)
    fake.stop()
    assert got and got[0]["input_dbfs"] < -50.0
    assert got[0]["spl_db"] is None
    assert got[0]["clip"] is False


# ── 2단계 [설정 읽기] 실값 흉내(2026-09-23) ─────────────────────────────
def test_measure_level_dbfs_and_calibration_when_running(rewapi):
    rew = rewapi.FakeREW(delay_s=0.0, measure_level_dbfs=-24.0, cal_file="C:\\cal\\UMIK-1.cal")
    try:
        assert rew.measure_level_dbfs() == -24.0
        assert rew.input_calibration_file() == "C:\\cal\\UMIK-1.cal"
    finally:
        rew.close()


def test_measure_level_dbfs_and_calibration_default_to_no_file(rewapi, fake):
    assert fake.measure_level_dbfs() == -20.0
    assert fake.input_calibration_file() == ""     # 기본값 = 파일 없음(실기와 같은 뜻)


def test_measure_level_dbfs_and_calibration_are_none_when_not_running(rewapi):
    rew = rewapi.FakeREW(delay_s=0.0, running=False)
    try:
        assert rew.measure_level_dbfs() is None
        assert rew.input_calibration_file() is None
    finally:
        rew.close()


# ── 백로그 2: 'running' phase (2026-09-23) ──────────────────────────────
def test_running_phase_is_sent_during_a_sweep(rewapi):
    rew = rewapi.FakeREW(delay_s=0.05)
    try:
        plan = rewapi.fake_plan(["L"], [1])
        events: list[dict] = []
        rew.run_position(plan, 1, events.append, threading.Event())
        phases = {e["phase"] for e in events}
        assert "running" in phases
        assert phases <= set(rewapi.PHASES)
    finally:
        rew.close()


def test_phases_include_running(rewapi):
    assert rewapi.PHASES == ("sweep", "running", "process", "done", "cancelled")
