"""5단계 측정 — FakeREW 로 측정 완료 · 취소 · `.secs` 저장 (트랙 C).

계약 C: "FakeREW 로 5단계 측정 완료까지(QThread 완료를 이벤트 루프로 대기)
→ `.secs` 저장 확인". 기록 모양은 개정 1(위치당 한 항목, `ir` 가 role 사전).
"""
from __future__ import annotations

import json
from pathlib import Path

import numpy as np
import pytest

from test_wizard_common import (PRESET, engine, fake, make_window,  # noqa: F401
                                project, qapp)
from test_wizard_common import pytestmark  # noqa: F401


@pytest.fixture()
def window(qapp, project, fake, tmp_path):
    win = make_window(project, fake, tmp_path / "측정.secs")
    win.project.extra["array"] = 9
    win.go(5)
    yield win
    win.close()
    qapp.processEvents()


def _wait(window, qapp, timeout_ms=30000):
    assert window.step5.wait(timeout_ms), "측정 스레드가 끝나지 않았다"
    qapp.processEvents()


def test_position_estimate_matches_step4s_per_position_value(window, project):
    """5단계의 "이 위치 약 …" 은 4단계 `position_seconds()` 와 같은 값이다(2026-09-23)."""
    from secs.wizard.step4_layout import format_estimate, position_seconds

    window.step5.refresh()
    expected = format_estimate(position_seconds(project))
    assert window.step5.position_estimate.text() == f'이 위치 {expected}'


def test_plan_expands_multiband_roles(window, project):
    plan = window.step5.plan()
    assert plan.validate() == []
    assert plan.positions == list(range(1, 10))
    for speaker in project.config.speakers:
        if len(speaker.bands) == 1:
            assert speaker.role in plan.roles
        else:
            for band in speaker.bands:
                assert f"{speaker.role}:{band.band}" in plan.roles
                assert plan.outputs[f"{speaker.role}:{band.band}"] == band.out


def test_measure_position_saves_secs_and_irs(window, qapp, project, tmp_path):
    assert window.step5.measure_position(1) is True
    _wait(window, qapp)

    assert window.step5.done_positions() == [1]
    path = tmp_path / "측정.secs"
    assert path.exists(), ".secs 가 자동 저장되지 않았다"
    data = json.loads(path.read_text(encoding="utf-8"))
    entries = data["measurements"]
    assert len(entries) == 1
    entry = entries[0]
    assert entry["pos"] == 1 and entry["done"] is True
    plan = window.step5.plan()
    assert set(entry["ir"]) == set(plan.roles)
    for stored in entry["ir"].values():
        assert (tmp_path / stored).exists()

    # 측정 직후 Before 곡선이 채널마다 생겼는지
    assert window.step5._magnitudes
    for curve in window.step5._magnitudes.values():
        assert curve.shape[0] == len(__import__("secs.wizard.step5_measure", fromlist=["FREQS"]).FREQS) and np.isfinite(curve).all()

    allowed, _reason = window.can_advance()
    assert allowed is True


def test_measure_two_positions_then_redo_last(window, qapp):
    for pos in (1, 2):
        assert window.step5.measure_position(pos) is True
        _wait(window, qapp)
    assert window.step5.done_positions() == [1, 2]
    assert window.step5.current_position() == 3

    window.step5.redo_last()
    assert window.step5.done_positions() == [1]
    assert window.step5.current_position() == 2


def test_cancel_drops_the_position(qapp, project, tmp_path):
    from secs.rewapi.fake import FakeREW

    slow = FakeREW(delay_s=0.3)
    window = make_window(project, slow, tmp_path / "취소.secs")
    try:
        window.project.extra["array"] = 9
        window.go(5)
        assert window.step5.measure_position(1) is True
        window.step5.cancel()
        assert window.step5.wait(30000)
        qapp.processEvents()
        assert window.step5.done_positions() == []
        assert window.can_advance()[0] is False
    finally:
        window.close()
        qapp.processEvents()
        slow.close()


def test_sample_data_fills_only_position_1(window, qapp, tmp_path):
    """샘플은 실측 1점뿐(09-24 사용자 결정) — 9 위치 배치여도 위치 1 만 채우고 나머지는 비운다."""
    filled = window.step5.load_sample()
    assert filled == 1
    assert window.step5.done_positions() == [1]
    assert window.step5.current_position() == 2
    assert '샘플은 실측 1점뿐' in window.step5.log.toPlainText()
    assert (tmp_path / "측정.secs").exists()
    assert window.can_advance()[0] is True


def test_sample_data_needs_no_save_dialog(qapp, project, fake, tmp_path, monkeypatch):
    """D 보고 — 저장 위치 창을 취소하면 0/9 그대로였다. 이제 묻지 않고 만든다."""
    import secs.wizard.step5_measure as step5_mod
    from PyQt6 import QtWidgets

    def no_dialog(*_args, **_kwargs):      # 창이 뜨면 테스트가 실패해야 한다
        raise AssertionError('저장 위치 창이 떴다')

    monkeypatch.setattr(QtWidgets.QFileDialog, 'getSaveFileName', no_dialog)
    monkeypatch.setattr(step5_mod, 'DEFAULT_PROJECT_DIR', tmp_path / 'Documents' / 'SECS')

    win = make_window(project, fake, None)      # 경로 없이 연 창
    try:
        assert win.project_path is None
        win.project.extra['array'] = 9
        win.go(5)
        assert win.step5.load_sample(2) == 2

        path = win.step5.project_path
        assert path is not None and path.exists()
        assert path.parent == tmp_path / 'Documents' / 'SECS'
        assert path.name.startswith('샘플 ') and path.suffix == '.secs'
        assert len(win.step5.done_positions()) == 2

        # 창과 설계 엔진이 같은 경로를 본다(IR wav 가 이 파일 기준 상대경로라서).
        assert win.current_project_path() == path
        assert win.side_file.text() == path.name
        assert win.side_file.toolTip() == str(path)

        # [프로젝트 저장] 으로 옮기면 IR 폴더도 따라가고 기록도 새 이름을 가리킨다.
        moved = tmp_path / '옮긴.secs'
        assert win.save_project(str(moved)) == moved
        assert (tmp_path / '옮긴.irs').is_dir()
        assert win.side_file.text() == '옮긴.secs'
        for record in project.measurements:
            for wav in record['ir'].values():
                assert wav.startswith('옮긴.irs/')
                assert (moved.parent / wav).exists()
    finally:
        win.close()
        qapp.processEvents()


def test_real_measurement_gets_the_measurement_prefix(qapp, project, fake, tmp_path,
                                                      monkeypatch):
    """실측은 `측정 …`, Fake 샘플은 `샘플 …` 로 이름이 갈린다."""
    import secs.wizard.step5_measure as step5_mod
    from PyQt6 import QtWidgets

    monkeypatch.setattr(QtWidgets.QFileDialog, 'getSaveFileName',
                        lambda *a, **k: (_ for _ in ()).throw(AssertionError('창이 떴다')))
    monkeypatch.setattr(step5_mod, 'DEFAULT_PROJECT_DIR', tmp_path / 'SECS')

    win = make_window(project, fake, None)
    try:
        win.project.extra['array'] = 9
        win.go(5)
        assert win.step5.measure_position(1) is True
        _wait(win, qapp)
        path = win.step5.project_path
        assert path.name.startswith('측정 ') and path.exists()
        assert win.step5.done_positions() == [1]
    finally:
        win.close()
        qapp.processEvents()


def test_mdat_titles_match_biamp_roles(engine):
    """실측 mdat 검증 — `"L Fin"` 이 `'L:LF'` 에 안 걸려 순서대로 배정됐다."""
    from secs.wizard.step5_measure import match_role

    biamp = ['L:LF', 'L:HF', 'R:LF', 'R:HF', 'SW1']
    used: set[str] = set()

    # 대역이 안 적힌 제목은 기본 role 로 자르고 LF → HF 순서로 채운다.
    first = match_role('L Fin', biamp, used)
    assert first == 'L:LF'
    used.add(first)
    assert match_role('L Fin 2', biamp, used) == 'L:HF'

    # 제목에 대역이 있으면 순서와 무관하게 그 대역.
    assert match_role('R HF sweep', biamp, set()) == 'R:HF'
    assert match_role('R Fin LF', biamp, set()) == 'R:LF'
    assert match_role('SW1 80Hz', biamp, set()) == 'SW1'

    # 풀레인지 구성에서 'Ls' 가 'L' 에 먹히지 않는다(긴 role 우선).
    full = ['L', 'R', 'C', 'SW1', 'Ls', 'Rs']
    assert match_role('Ls Fin', full, set()) == 'Ls'
    assert match_role('L Fin', full, set()) == 'L'
    assert match_role('C', full, set()) == 'C'

    # 못 고르면 None — 호출부가 순서 배정으로 떨어진다.
    assert match_role('Sweep 3', full, set()) is None
    assert match_role('', full, set()) is None


def test_load_mdat_assigns_roles_by_title(qapp, engine, fake, tmp_path, monkeypatch):
    """제목 매칭이 `load_mdat` 까지 이어지는지(순서 배정으로 안 떨어지는지)."""
    import types

    import secs.mdat_loader as mdat_mod
    import secs.model as model

    project = model.Project(config=model.SpeakerConfig.from_preset("2.1", biamp=True))
    roles = ["L:LF", "L:HF", "R:LF", "R:HF", "SW1"]

    # REW 가 내보낸 것처럼 순서를 섞고 대역을 안 적은 제목.
    titles = ["SW1 sweep", "R Fin", "L Fin", "R Fin 2", "L Fin 2"]
    infos = [types.SimpleNamespace(title=text, uuid=f"u{i}")
             for i, text in enumerate(titles)]
    provider = types.SimpleNamespace(
        get_ir=lambda uuid: (np.zeros(2048, dtype=float) + 1e-3, 48000))
    monkeypatch.setattr(mdat_mod, "load_mdat_project",
                        lambda _path: types.SimpleNamespace(measurements=infos,
                                                            ir_provider=provider))

    # 어떤 제목이 어떤 role 로 갔는지 본다(순서 배정이면 여기서 어긋난다).
    from secs.rewapi.session import MeasurementSession

    seen: list[tuple[str, str]] = []
    original = MeasurementSession.record

    def spy(self, result, plan=None):
        seen.append((result.name, result.role))
        return original(self, result, plan)

    monkeypatch.setattr(MeasurementSession, "record", spy)

    win = make_window(project, fake, tmp_path / "mdat.secs")
    try:
        assert win.step5.plan().roles == roles
        assert win.step5.load_mdat(str(tmp_path / "가짜.mdat")) == len(titles)
        assert dict(seen) == {
            "SW1 sweep": "SW1",
            "R Fin": "R:LF", "R Fin 2": "R:HF",
            "L Fin": "L:LF", "L Fin 2": "L:HF",
        }
        record = next(r for r in project.measurements if int(r["pos"]) == 1)
        assert set(record["ir"]) == set(roles)      # 5개 role 이 모두 채워졌다
    finally:
        win.close()
        qapp.processEvents()


#: 실측 REW 프로젝트 v2(측정 3개: 'L 1'·'LF'·'L Fin', L 의 start_time_s ≈ −0.9996).
REAL_MDAT = Path(__file__).resolve().parent / "data" / "오토이큐 테스트용 v2.mdat"


@pytest.mark.skipif(not REAL_MDAT.exists(), reason="실측 mdat 가 없다")
def test_real_mdat_carries_t0_into_the_secs(window, qapp, project):
    """WAV 는 시간 원점을 못 담는다 — `.secs` 의 't0' 만이 진실이다."""
    # 자동 배정(묻지 않는 코드 호출): 'L 1' → L:LF, 'LF' 는 제목으로 못 고르지만
    # 저역만 나는 측정이라 서브 role 로. 'L Fin'(보정 후 참고 측정)은 예전엔 다음 빈 대역 L:HF 로
    # 잘못 들어갔지만, 09-24 부터 이름이 채널과 맞지 않으면 비워 둔다(역할 배정 창에서 사람이 고른다).
    assert window.step5.load_mdat(str(REAL_MDAT)) == 2

    record = next(r for r in project.measurements if int(r["pos"]) == 1)
    assert set(record["ir"]) == {"L:LF", "SW1"}
    assert record["t0"]["L:LF"] == pytest.approx(-0.9996, abs=1e-3)
    assert record["t0"]["SW1"] == pytest.approx(-0.9852, abs=1e-3)      # 'LF' = 서브 합

    # 저장 → 재로드해도 남아 있어야 한다(설계가 도착시간을 이 값으로 잡는다).
    import secs.model as model

    path = window.save_project(str(window.project_path))
    reloaded = model.Project.load(path)
    again = next(r for r in reloaded.measurements if int(r["pos"]) == 1)
    assert again["t0"]["L:LF"] == pytest.approx(-0.9996, abs=1e-3)


def test_magnitude_curve_is_smoothed_and_centred(engine):
    from secs.wizard.step5_measure import FREQS, magnitude_db

    rate = 48000
    impulse = np.zeros(4096, dtype=float)
    impulse[10] = 1.0
    curve = magnitude_db(impulse, rate)
    assert curve.shape == FREQS.shape
    assert np.allclose(curve, 0.0, atol=0.5), "임펄스는 평탄해야 한다"


def test_skip_from_step5_loads_mdat(window, qapp, project, monkeypatch):
    """10-09 사용자: 측정 탭의 샘플은 빼고 mdat 불러오기로 — 5단계 [건너뛰기]는 mdat 불러오기를 연다."""
    called = []
    monkeypatch.setattr(window.step5, 'load_mdat', lambda *a, **k: called.append(1))
    window.skip()
    assert called


def test_sample_fill_goes_to_calibration(window, qapp, project):
    window.step5.load_sample()
    window.go(9)
    assert window.current_step == 9          # 09-26: 측정 다음은 보정 설계(화면 번호 9)
    assert project.measurements
