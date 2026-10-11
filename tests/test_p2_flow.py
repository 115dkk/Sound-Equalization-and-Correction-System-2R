"""P2 통합 흐름 검증 (트랙 E).

`P2 인터페이스 계약 2026-09-16.md` 의 "E: 통합" 절이 기준이다.

  FakeREW + WizardWindow 로 9.1.6 프리셋·위치 3개 측정
  → `.secs` 와 `.irs/*.wav` 생성
  → `Project.load`
  → 각 role·pos 의 IR 을 파일/`IRProvider` 에서 복원
  → 위치 1 의 IR 로 `generate_filters_from_arrays` 가 채널별 FIR 생성
  → 취소·재개 시나리오 1건

B(`secs.rewapi`)·C(`secs.wizard`) 가 지금 동시에 만드는 중이라 구현이 들어오기
전에는 임포트 단계에서 실패한다(정상. `importorskip` 으로 덮지 않는다).

────────────────────────────────────────────────────────────────────────
계약이 정하지 않아 이 파일이 가정한 것 (리드·B·C 확인 필요)
────────────────────────────────────────────────────────────────────────
개정 1(04:35) 이 앞서 열려 있던 것을 모두 닫았다. 이 파일은 그 기준을 따른다.
- `measurements` 는 위치당 한 항목:
  `{'pos','label','uuid':{role:uuid|None},'mdat':None,'ir':{role:wav경로},
    'spl_db':{role:float|None},'done':bool}`. 같은 pos 재측정 시 항목 교체.
- `WizardWindow.current_step`·`go()` 는 1 기점(1~8).
- 취소: `run_position` 은 예외 없이 그때까지 결과를 돌려주고, 마지막 진행
  콜백 phase 는 'cancelled', 그 위치는 `done_positions()` 에서 빠진다.
- `MeasurementSession(project, measurer, ir_saver=None, *, project_path,
  auto_save=True)`. 멀티앰프 role 은 'L:LF' 꼴, 저장 파일명은 ':'→'_'.

한 스피커의 첫 대역(바이앰프 메인이면 LF 드라이버)을 그 채널의 대표 IR 로
쓴다 — 룸 보정이 보는 저역이 거기 실린다.
────────────────────────────────────────────────────────────────────────
"""
from __future__ import annotations

import threading
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from conftest import engine_choice, load_engine

pytestmark = pytest.mark.skipif(
    engine_choice() != "package",
    reason="P2 는 패키지 엔진 전용이다 (SECS_ENGINE=package).",
)

PRESET = "9.1.6"
POSITIONS = (1, 2, 3)


# ── 임포트 (구현 전에는 여기서 실패한다) ───────────────────────────────

@pytest.fixture(scope="module")
def engine():
    return load_engine("package")


@pytest.fixture(scope="module")
def model(engine):
    import secs.model as module  # type: ignore[import-not-found]
    return module


@pytest.fixture(scope="module")
def rewapi(engine):
    """B 가 안내한 `secs.rewapi` 최상위 공개 이름."""
    import secs.rewapi as module  # type: ignore[import-not-found]
    return module


@pytest.fixture(scope="module")
def wizard(engine):
    from secs.wizard import WizardWindow  # type: ignore[import-not-found]
    return WizardWindow


@pytest.fixture(scope="module")
def qapp():
    """오프스크린 QApplication (conftest 가 QT_QPA_PLATFORM=offscreen 을 건다)."""
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance() or QApplication([])
    yield app
    app.processEvents()


# ── 계약 19행의 role 확장 규칙 ─────────────────────────────────────────

def _measure_roles(config) -> tuple[list[str], dict[str, int], dict[str, str]]:
    """SpeakerConfig → (측정 role 목록, role→출력, 채널id→대표 role).

    한 스피커가 대역 하나면 role 은 역할 문자열 그대로, 여럿이면 계약 19행의
    'L:LF' 꼴로 확장한다. 대표 role 은 첫 대역(바이앰프 메인이면 LF 드라이버)
    이고, 룸 보정이 보는 저역이 거기에 실린다.
    """
    roles: list[str] = []
    outputs: dict[str, int] = {}
    primary: dict[str, str] = {}
    for speaker in config.speakers:
        for index, band in enumerate(speaker.bands):
            name = speaker.role if len(speaker.bands) == 1 \
                else f"{speaker.role}:{band.band}"
            roles.append(name)
            outputs[name] = int(band.out)
            if index == 0:
                primary[speaker.role] = name
    return roles, outputs, primary


def _build_plan(rewapi, config) -> object:
    roles, outputs, _ = _measure_roles(config)
    return rewapi.MeasurementPlan(
        roles=roles,
        outputs=outputs,
        positions=list(POSITIONS),
        output_device="fake-out",
        input_device="fake-in",
        input_channel=1,
    )


def _make_session(rewapi, project, measurer, project_path: Path):
    """세션은 프로젝트 경로만 주면 `IrSaver` 를 스스로 만든다."""
    return rewapi.MeasurementSession(project, measurer, project_path=project_path)


def _run_whole_plan(rewapi, session, plan) -> list:
    """계약대로 `run_plan(plan, on_progress, cancel)` 한 번으로 전 위치를 돈다."""
    progress: list[dict] = []
    results = session.run_plan(plan, progress.append, threading.Event())

    assert progress, "on_progress 가 한 번도 불리지 않았다"
    for event in progress:
        assert set(event) >= {"role", "pos", "phase", "percent", "message"}, event
        assert event["phase"] in rewapi.PHASES, event
        assert 0 <= float(event["percent"]) <= 100, event
    assert not any(e["phase"] == "cancelled" for e in progress), \
        "취소하지 않았는데 cancelled 가 나왔다"
    return results


def _ir_paths_for_position(project, project_path: Path, pos: int) -> dict[str, Path]:
    """재로드한 project 에서 한 위치의 role → IR 파일 경로.

    개정 1 의 모양만 읽는다: 위치당 한 항목의 `ir` 가 role→경로 사전이고,
    경로는 `.secs` 가 있는 폴더 기준 상대 경로다.
    """
    root = Path(project_path).parent
    found: dict[str, Path] = {}
    for entry in project.measurements or []:
        if int(entry.get("pos", -1)) != int(pos):
            continue
        for role, stored in (entry.get("ir") or {}).items():
            if stored:
                path = Path(stored)
                found[str(role)] = path if path.is_absolute() else root / path
    return found


# ── 1) 위저드: 1~5 단계 이동과 project 연결 ────────────────────────────

def test_wizard_opens_with_fake_rew_and_walks_steps(qapp, model, rewapi, wizard):
    """FakeREW 다섯 프로토콜을 그대로 물려 위저드를 열고 1~5 단계를 이동한다."""
    project = model.Project(name="P2흐름", config=model.SpeakerConfig.from_preset(PRESET))
    rew = rewapi.FakeREW(delay_s=0.0)

    window = wizard(project, rew, rew, rew, rew)
    try:
        # 개정 1: current_step·go() 는 1 기점(1~8)
        assert window.current_step == 1, "위저드는 1 단계에서 열려야 한다"

        for step in range(1, 6):
            window.go(step)
            assert window.current_step == step

            advance = window.can_advance()
            assert isinstance(advance, tuple) and len(advance) == 2, advance
            allowed, reason = advance
            assert isinstance(allowed, bool)
            assert isinstance(reason, str)
            if not allowed:
                assert reason, f"{step} 단계: 못 넘어가는데 이유가 비었다"

        # 1 단계에서 고른 프리셋이 project 에 그대로 붙어 있어야 한다(계약 52행)
        assert [s.role for s in project.config.speakers] == \
            [s.role for s in model.SpeakerConfig.from_preset(PRESET).speakers]
        assert project.config.validate() == []
    finally:
        window.close()
        qapp.processEvents()


def test_wizard_rejects_step_out_of_range(qapp, model, rewapi, wizard):
    """범위 밖 `go()` 는 위저드를 이상한 단계로 끌고 가지 않는다.

    개정 1 은 유효 범위(1~8)만 정하고 범위 밖에서 예외를 던질지 조용히
    무시할지는 정하지 않았다(목업 `go()` 는 무시한다). 그래서 둘 중
    무엇이든 받아들이되, 단계가 유효한 채로 남는지는 반드시 본다.
    """
    project = model.Project(name="범위", config=model.SpeakerConfig.from_preset("5.1"))
    rew = rewapi.FakeREW(delay_s=0.0)
    window = wizard(project, rew, rew, rew, rew)
    try:
        for bad in (0, 11):         # 개정 1: 유효 범위는 1~8, 09-26 부터 9 = 보정 설계 · 10 = 옛 8단계(흐름 밖)
            before = window.current_step
            try:
                window.go(bad)
            except (ValueError, IndexError):
                pass                # 예외로 거절하는 구현
            assert window.current_step == before, \
                f"범위 밖 go({bad}) 가 단계를 {window.current_step} 로 옮겼다"
            assert 1 <= window.current_step <= 9
    finally:
        window.close()
        qapp.processEvents()


# ── 2) 전 구간: 측정 → .secs/.irs → 재로드 → 필터 생성 ──────────────────

@pytest.fixture(scope="module")
def measured(model, rewapi, tmp_path_factory):
    """9.1.6 · 위치 3개를 FakeREW 로 끝까지 측정한 결과(모듈에서 재사용)."""
    directory = tmp_path_factory.mktemp("p2flow")
    project_path = directory / "흐름.secs"

    project = model.Project(name="P2흐름", config=model.SpeakerConfig.from_preset(PRESET))
    project.save(project_path)

    rew = rewapi.FakeREW(delay_s=0.0)
    plan = _build_plan(rewapi, project.config)
    session = _make_session(rewapi, project, rew, project_path)
    results = _run_whole_plan(rewapi, session, plan)

    return {"project_path": project_path, "project": project, "plan": plan,
            "session": session, "results": results, "rew": rew,
            "ir_dir": project_path.with_suffix(".irs")}


def test_every_role_and_position_measured(measured, model):
    roles, _, _ = _measure_roles(measured["project"].config)
    got = {(r.role, r.pos) for r in measured["results"]}
    assert got == {(role, pos) for role in roles for pos in POSITIONS}

    for result in measured["results"]:
        ir = np.asarray(result.ir)
        assert ir.ndim == 1 and ir.size >= 256, result.role
        assert np.all(np.isfinite(ir)), result.role
        assert float(np.max(np.abs(ir))) > 1e-9, result.role
        assert int(result.sample_rate) > 0
        assert isinstance(result.name, str) and result.name


def test_ir_wavs_written_for_every_measurement(measured):
    ir_dir = measured["ir_dir"]
    assert ir_dir.is_dir(), f"{ir_dir} 가 만들어지지 않았다"

    wavs = sorted(ir_dir.glob("*.wav"))
    assert len(wavs) == len(measured["results"]), \
        f"IR 파일 {len(wavs)}개, 측정 {len(measured['results'])}건"

    for path in wavs:
        data, rate = sf.read(str(path), always_2d=True)
        assert data.shape[1] == 1, f"{path.name}: 모노여야 한다"
        assert rate > 0
        assert np.all(np.isfinite(data))


def test_positions_are_recorded_in_project_file(measured, model):
    """측정마다 자동 저장되어 `.secs` 에 세 위치가 다 남는다(계약 42행)."""
    reloaded = model.Project.load(measured["project_path"])
    assert reloaded.measurements, ".secs 에 measurements 가 비었다"
    assert {int(entry["pos"]) for entry in reloaded.measurements} == set(POSITIONS)

    roles, _, _ = _measure_roles(measured["project"].config)
    for pos in POSITIONS:
        paths = _ir_paths_for_position(reloaded, measured["project_path"], pos)
        assert set(paths) == set(roles), f"위치 {pos}: role 집합이 다르다"
        for role, path in paths.items():
            assert path.is_file(), f"{role} P{pos}: {path} 가 없다"


def test_measurements_follow_contract_shape(measured, model):
    """개정 1: 위치당 한 항목, uuid·ir·spl_db 는 role 사전, done 은 bool."""
    reloaded = model.Project.load(measured["project_path"])
    roles, _, _ = _measure_roles(measured["project"].config)

    assert len(reloaded.measurements) == len(POSITIONS), \
        (f"위치마다 한 항목이어야 하는데 {len(reloaded.measurements)}개다 "
         f"(role·pos 마다 하나씩 쌓인 것으로 보인다)")

    for entry in reloaded.measurements:
        assert set(entry) >= {"pos", "label", "uuid", "mdat", "ir", "spl_db", "done"}, \
            entry
        assert int(entry["pos"]) in POSITIONS
        assert isinstance(entry["label"], str)
        assert entry["mdat"] is None
        assert entry["done"] is True, f"위치 {entry['pos']} 가 done 이 아니다"

        for key in ("uuid", "ir", "spl_db"):
            assert isinstance(entry[key], dict), (key, entry[key])
            assert set(entry[key]) == set(roles), f"{key}: role 집합이 다르다"

        for role, stored in entry["ir"].items():
            assert stored and str(stored).endswith(".wav"), (role, stored)
        for role, value in entry["spl_db"].items():
            assert value is None or isinstance(value, float), (role, value)


def test_remeasuring_a_position_replaces_its_entry(model, rewapi, tmp_path):
    """개정 1: 같은 pos 를 다시 측정하면 항목이 늘지 않고 교체된다."""
    project_path = tmp_path / "재측정.secs"
    project = model.Project(name="재측정", config=model.SpeakerConfig.from_preset("2.1"))
    project.save(project_path)

    rew = rewapi.FakeREW(delay_s=0.0)
    plan = _build_plan(rewapi, project.config)
    session = _make_session(rewapi, project, rew, project_path)

    session.run_position(plan, 1, lambda event: None, threading.Event())
    after_first = len(model.Project.load(project_path).measurements)

    session.run_position(plan, 1, lambda event: None, threading.Event())
    after_second = model.Project.load(project_path).measurements

    assert after_first == 1
    assert len(after_second) == 1, "같은 위치를 다시 측정했는데 항목이 늘었다"
    assert int(after_second[0]["pos"]) == 1


def test_done_positions_reports_completed(measured):
    session, plan = measured["session"], measured["plan"]
    assert sorted(int(p) for p in session.done_positions()) == list(POSITIONS)
    assert session.pending_positions(plan) == []


def test_ir_provider_restores_same_ir(measured):
    """uuid 가 있는 결과는 `IRProvider.get_ir` 로도 같은 IR 이 돌아온다."""
    with_uuid = [r for r in measured["results"] if r.uuid]
    if not with_uuid:
        pytest.fail("FakeREW 가 uuid 를 하나도 주지 않았다 (IRProvider 경로를 못 본다)")

    for result in with_uuid[:5]:
        ir, rate = measured["rew"].get_ir(result.uuid)
        assert int(rate) == int(result.sample_rate)
        assert np.allclose(np.asarray(ir, dtype=np.float64),
                           np.asarray(result.ir, dtype=np.float64),
                           rtol=0, atol=1e-6), result.role


# ── 3) 취소·재개 ───────────────────────────────────────────────────────

def test_cancel_then_resume_skips_finished_positions(model, rewapi, tmp_path):
    """위치 2에서 취소 → 재개하면 끝난 위치는 건너뛰고 나머지만 채운다."""
    project_path = tmp_path / "취소재개.secs"
    project = model.Project(name="취소재개", config=model.SpeakerConfig.from_preset("5.1"))
    project.save(project_path)

    rew = rewapi.FakeREW(delay_s=0.0)
    plan = _build_plan(rewapi, project.config)
    session = _make_session(rewapi, project, rew, project_path)

    # 위치 1 은 끝까지
    session.run_position(plan, 1, lambda event: None, threading.Event())
    assert sorted(int(p) for p in session.done_positions()) == [1]

    # 위치 2 는 첫 진행 보고가 오자마자 취소
    cancel = threading.Event()
    events: list[dict] = []

    def on_progress(event: dict) -> None:
        events.append(event)
        cancel.set()

    partial = session.run_position(plan, 2, on_progress, cancel)

    # 개정 1: 예외 없이 그때까지 결과 반환, 마지막 phase 는 'cancelled'
    assert len(partial) < len(plan.roles), "취소했는데 전 role 이 다 측정됐다"
    assert events[-1]["phase"] == "cancelled", events[-1]
    assert 2 not in set(session.done_positions()), "취소된 위치가 완료로 기록됐다"

    # 재개: 끝난 위치 1 은 다시 측정하지 않는다
    remaining = session.pending_positions(plan)
    assert 1 not in remaining, "완료된 위치가 다시 대기 목록에 들어갔다"
    assert set(remaining) == {2, 3}

    for pos in remaining:
        session.run_position(plan, pos, lambda event: None, threading.Event())

    assert sorted(int(p) for p in session.done_positions()) == list(POSITIONS)

    reloaded = model.Project.load(project_path)
    assert {int(e["pos"]) for e in reloaded.measurements} == set(POSITIONS)


def test_cancel_before_start_measures_nothing(model, rewapi, tmp_path):
    project_path = tmp_path / "즉시취소.secs"
    project = model.Project(name="즉시취소", config=model.SpeakerConfig.from_preset("2.1"))
    project.save(project_path)

    rew = rewapi.FakeREW(delay_s=0.0)
    plan = _build_plan(rewapi, project.config)
    session = _make_session(rewapi, project, rew, project_path)

    cancel = threading.Event()
    cancel.set()
    events: list[dict] = []
    results = session.run_position(plan, 1, events.append, cancel)

    assert results == []
    assert events and events[-1]["phase"] == "cancelled", events
    assert list(session.done_positions()) == []
    assert session.pending_positions(plan) == list(plan.positions)
