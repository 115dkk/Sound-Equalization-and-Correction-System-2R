"""P2 B `secs.rewapi.session` 검사 — `.secs` 자동 저장·IR 파일·재개.

기준: `P2 인터페이스 계약 2026-09-16.md` 의 "B: `secs.rewapi`" 절.
"""
from __future__ import annotations

import json
import threading

import numpy as np
import pytest

from conftest import engine_choice, load_engine

pytestmark = pytest.mark.skipif(
    engine_choice() != "package",
    reason="secs.rewapi 는 패키지 엔진 전용이다 (SECS_ENGINE=package).",
)


@pytest.fixture(scope="module")
def rewapi():
    load_engine("package")
    import secs.rewapi as module  # type: ignore[import-not-found]
    return module


@pytest.fixture(scope="module")
def model():
    load_engine("package")
    import secs.model as module  # type: ignore[import-not-found]
    return module


@pytest.fixture()
def project_path(tmp_path, model):
    path = tmp_path / "측정.secs"
    project = model.Project.from_preset("2.1")
    project.config.biamp_default = False
    for speaker in project.config.speakers:       # 단순화: 스피커당 출력 1개
        speaker.bands = speaker.bands[:1]
    project.config.renumber()
    project.save(path)
    return path


@pytest.fixture()
def fake(rewapi):
    rew = rewapi.FakeREW(delay_s=0.0)
    yield rew
    rew.close()


def _session(rewapi, model, path, fake, **kwargs):
    project = model.Project.load(path)
    return rewapi.MeasurementSession(project, fake, project_path=path, **kwargs), project


# ── 기록·저장 ─────────────────────────────────────────────────────────
def test_run_plan_writes_secs_and_ir_files(rewapi, model, fake, project_path):
    session, project = _session(rewapi, model, project_path, fake)
    plan = rewapi.plan_from_project(project, [1, 2])
    assert plan.roles == ["L", "R", "SW1"]

    results = session.run_plan(plan)
    assert len(results) == 6

    saved = json.loads(project_path.read_text(encoding="utf-8"))
    assert len(saved["measurements"]) == 2          # 개정 1: 위치당 한 항목
    first = saved["measurements"][0]
    assert set(first) >= {"pos", "label", "uuid", "mdat", "ir", "spl_db", "t0", "done"}
    assert first["pos"] == 1 and first["label"] == "위치 1"
    assert first["mdat"] is None and first["done"] is True
    assert set(first["ir"]) == set(first["uuid"]) == set(first["spl_db"]) == set(plan.roles)
    assert set(first["t0"]) == set(plan.roles)
    assert first["ir"]["L"] == "측정.irs/L_P1.wav"
    assert first["sample_rate"] == 48000
    assert all(isinstance(v, float) for v in first["spl_db"].values())
    assert all(isinstance(v, float) for v in first["t0"].values())

    ir_dir = project_path.with_suffix(".irs")
    assert sorted(p.name for p in ir_dir.iterdir()) == [
        "L_P1.wav", "L_P2.wav", "R_P1.wav", "R_P2.wav", "SW1_P1.wav", "SW1_P2.wav"]


def test_saved_ir_matches_measured_ir(rewapi, model, fake, project_path):
    session, project = _session(rewapi, model, project_path, fake)
    plan = rewapi.plan_from_project(project, [1])
    results = session.run_plan(plan)

    for result in results:
        ir, rate = session.ir_of(result.role, result.pos)
        assert rate == result.sample_rate
        assert ir.shape == result.ir.shape
        assert np.allclose(ir, result.ir, atol=1e-6)


# ── IR 시간 원점(t0) ──────────────────────────────────────────────────
def test_t0_is_stored_in_secs_because_wav_cannot_hold_it(rewapi, model, project_path):
    """WAV 헤더에는 시간 원점을 담을 곳이 없다 → `.secs` 의 `'t0'` 가 진실이다."""
    rew = rewapi.FakeREW(delay_s=0.0, t0_s=-0.25)
    try:
        session, project = _session(rewapi, model, project_path, rew)
        plan = rewapi.plan_from_project(project, [1])
        results = session.run_plan(plan)
        assert all(r.t0_s == -0.25 for r in results)

        saved = json.loads(project_path.read_text(encoding="utf-8"))
        assert saved["measurements"][0]["t0"] == {"L": -0.25, "R": -0.25, "SW1": -0.25}

        # 다시 읽어도 값이 살아 있다(IR 파일이 아니라 .secs 에서 온다)
        reopened = model.Project.load(project_path)
        again = rewapi.MeasurementSession(reopened, rew, project_path=project_path)
        assert again.t0_of("L", 1) == -0.25
        assert again.t0_by_role(1) == {"L": -0.25, "R": -0.25, "SW1": -0.25}
        assert set(again.t0_by_role(1)) == set(again.irs_by_role(1))
    finally:
        rew.close()


def test_t0_defaults_to_zero(rewapi, model, fake, project_path):
    """FakeREW 기본은 0.0 이고, `'t0'` 가 없는 옛 `.secs` 도 0.0 으로 읽는다."""
    session, project = _session(rewapi, model, project_path, fake)
    plan = rewapi.plan_from_project(project, [1])
    session.run_plan(plan)
    assert session.t0_of("L", 1) == 0.0

    entry = session.entry_for(1)
    del entry["t0"]                                   # 옛 기록 흉내
    assert session.t0_of("L", 1) == 0.0
    assert session.t0_by_role(1) == {"L": 0.0, "R": 0.0, "SW1": 0.0}
    assert session.t0_of("없는역할", 1) == 0.0
    assert session.t0_of("L", 99) == 0.0              # 없는 위치


def test_reload_project_restores_irs_by_role(rewapi, model, fake, project_path):
    session, project = _session(rewapi, model, project_path, fake)
    plan = rewapi.plan_from_project(project, [1, 2])
    session.run_plan(plan)

    reloaded = model.Project.load(project_path)          # 새 세션에서 되읽기
    again = rewapi.MeasurementSession(reloaded, fake, project_path=project_path)
    table = again.irs_by_role(1)
    assert sorted(table) == ["L", "R", "SW1"]
    for role, (ir, rate) in table.items():
        assert rate == 48000
        assert np.allclose(ir, session.ir_of(role, 1)[0], atol=1e-6)


def test_ir_saver_paths_and_round_trip(rewapi, tmp_path):
    saver = rewapi.IrSaver(tmp_path / "p.secs")
    assert saver.directory == tmp_path / "p.irs"
    assert saver.file_for("L:LF", 3).name == "L_LF_P3.wav"   # ':' 는 파일명에 못 쓴다

    data = np.linspace(-1.0, 1.0, 512, dtype=np.float32)
    path = saver.save("L:LF", 3, data, 48000)
    assert path.is_file()
    ir, rate = saver.load("L:LF", 3)
    assert rate == 48000
    assert np.allclose(ir, data, atol=1e-6)


def test_ir_saver_without_soundfile(rewapi, tmp_path):
    """soundfile 이 없어도 자체 WAV 기록·복원이 된다(배포본 대비)."""
    saver = rewapi.IrSaver(tmp_path / "p.secs", use_soundfile=False)
    data = np.sin(np.linspace(0.0, 10.0, 1024)).astype(np.float32)
    saver.save("SW1", 1, data, 44100)
    from secs.rewapi.session import _read_wav_float32  # type: ignore[import-not-found]
    ir, rate = _read_wav_float32(saver.file_for("SW1", 1))
    assert rate == 44100
    assert np.allclose(ir, data, atol=1e-6)


# ── 취소·재개 ─────────────────────────────────────────────────────────
def test_cancel_then_resume_skips_done_positions(rewapi, model, project_path):
    rew = rewapi.FakeREW(delay_s=0.01)
    try:
        session, project = _session(rewapi, model, project_path, rew)
        plan = rewapi.plan_from_project(project, [1, 2, 3])
        cancel = threading.Event()

        def on_progress(payload: dict) -> None:
            if payload["pos"] == 2 and payload["role"] == "R" and payload["phase"] == "done":
                cancel.set()

        first = session.run_plan(plan, on_progress, cancel)
        assert session.done_positions(plan) == [1]          # 2번 위치는 미완성
        assert len(first) == 5                              # 위치1 전부(3) + 위치2 의 L,R
        assert session.pending_positions(plan) == [2, 3]

        reloaded = model.Project.load(project_path)         # 프로그램을 껐다 켠 셈
        resumed = rewapi.MeasurementSession(reloaded, rew, project_path=project_path)
        assert resumed.done_positions(plan) == [1]
        rest = resumed.run_plan(plan)
        assert {(r.role, r.pos) for r in rest} == {
            (role, pos) for pos in (2, 3) for role in plan.roles}
        assert resumed.done_positions(plan) == [1, 2, 3]

        saved = json.loads(project_path.read_text(encoding="utf-8"))
        assert len(saved["measurements"]) == 3              # 위치 3개, 재측정이 쌓이지 않는다
        assert all(entry["done"] is True for entry in saved["measurements"])
    finally:
        rew.close()


def test_remeasure_replaces_entry(rewapi, model, fake, project_path):
    session, project = _session(rewapi, model, project_path, fake)
    plan = rewapi.plan_from_project(project, [1])
    session.run_plan(plan)
    before = dict(project.measurements[0]["uuid"])

    session.run_plan(plan, skip_done=False)                 # 같은 위치를 다시
    assert len(project.measurements) == 1
    after = project.measurements[0]["uuid"]
    assert set(after) == set(before)
    assert all(after[role] != before[role] for role in before)


def test_partial_position_is_not_done(rewapi, model, project_path):
    """취소로 끊긴 위치는 done=False 로 남고 done_positions() 에서 빠진다."""
    rew = rewapi.FakeREW(delay_s=0.01)
    try:
        session, project = _session(rewapi, model, project_path, rew)
        plan = rewapi.plan_from_project(project, [1])
        cancel = threading.Event()

        def on_progress(payload: dict) -> None:
            if payload["role"] == "L" and payload["phase"] == "done":
                cancel.set()

        session.run_plan(plan, on_progress, cancel)
        entry = session.entry_for(1)
        assert entry is not None
        assert entry["done"] is False
        assert set(entry["ir"]) == {"L"}
        assert session.done_positions(plan) == []
        assert session.done_positions() == []
    finally:
        rew.close()


def test_auto_save_off_leaves_file_untouched(rewapi, model, fake, project_path):
    session, project = _session(rewapi, model, project_path, fake, auto_save=False)
    plan = rewapi.plan_from_project(project, [1])
    session.run_plan(plan)
    assert len(project.measurements) == 1
    assert set(project.measurements[0]["ir"]) == set(plan.roles)
    assert json.loads(project_path.read_text(encoding="utf-8"))["measurements"] == []
    session.save()
    assert len(json.loads(project_path.read_text(encoding="utf-8"))["measurements"]) == 1


def test_session_requires_a_path(rewapi, model, fake):
    with pytest.raises(ValueError):
        rewapi.MeasurementSession(model.Project(), fake)


def test_session_accepts_path_in_ir_saver_slot(rewapi, model, fake, project_path):
    project = model.Project.load(project_path)
    session = rewapi.MeasurementSession(project, fake, str(project_path))
    assert session.project_path == project_path
    assert session.ir_saver.directory == project_path.with_suffix(".irs")
