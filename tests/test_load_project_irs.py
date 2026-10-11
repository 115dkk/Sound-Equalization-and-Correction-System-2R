# -*- coding: utf-8 -*-
"""`.secs` 측정 기록 → 위치별 IR(`secs.rewapi.session.load_project_irs`).

2026-10-10 슬림화로 옛 설계 엔진(`secs.design`)과 그 테스트(`test_design_engine`)를 지우면서, 살아 있는
로더 검사만 여기로 옮겼다(옛 판은 읽은 IR 로 Designer 를 한 번 더 돌렸다 — 그 부분은 엔진과 함께 빠졌다).
입력은 `tests/synth.py` 의 합성 IR, 위치마다 표본 단위로 밀고 살짝 줄인다.
"""
from __future__ import annotations

import numpy as np
import pytest

import synth
from conftest import load_engine

load_engine("package")  # sys.path 에 src 를 넣는다

from secs.model import Project, SpeakerConfig  # noqa: E402
from secs.rewapi.session import IrSaver, load_project_irs  # noqa: E402

SAMPLE_RATE = synth.SAMPLE_RATE
POSITION_SHIFT_SAMPLES = 7


def role_irs(project: Project, positions: int) -> dict[int, dict[str, tuple[np.ndarray, int]]]:
    """스피커 구성에 맞춘 위치별 합성 IR. 대역이 둘 이상이면 'L:LF' 꼴 role."""
    mains = [s.role for s in project.config.speakers if s.kind != 'sub']
    subs = [s.role for s in project.config.speakers if s.kind == 'sub']
    base = synth.channel_measurement_set(tuple(mains), tuple(subs))
    out: dict[int, dict[str, tuple[np.ndarray, int]]] = {}
    for pos in range(1, positions + 1):
        out[pos] = {}
        shift = (pos - 1) * POSITION_SHIFT_SAMPLES
        scale = 1.0 - 0.03 * (pos - 1)
        for speaker in project.config.speakers:
            ir, rate = base[speaker.role]
            moved = np.roll(np.asarray(ir, dtype=np.float64), shift) * scale
            bands = list(speaker.bands)
            for band in bands:
                role = speaker.role if len(bands) == 1 else f'{speaker.role}:{band.band}'
                out[pos][role] = ((moved / len(bands)).copy(), rate)
    return out


def _saved_project(tmp_path, preset: str, positions: int, t0: float | None = None):
    project = Project(config=SpeakerConfig.from_preset(preset))
    path = tmp_path / f'{preset}.secs'
    saver = IrSaver(path)
    source = role_irs(project, positions)
    for pos, role_map in source.items():
        entry = {'pos': pos, 'label': f'위치 {pos}', 'uuid': {}, 'mdat': None, 'ir': {},
                 't0': {}, 'spl_db': {}, 'done': True, 'sample_rate': SAMPLE_RATE}
        for role, (ir, rate) in role_map.items():
            saved = saver.save(role, pos, ir, rate)
            entry['ir'][role] = str(saved.relative_to(path.parent)).replace('\\', '/')
            if t0 is not None:
                entry['t0'][role] = t0
        project.measurements.append(entry)
    project.save(path)
    return path, source


def test_load_project_irs_reads_the_time_origin(tmp_path) -> None:
    path, _source = _saved_project(tmp_path, '2.0', 1, t0=-0.25)
    loaded = load_project_irs(Project.load(path), path)
    for role, value in loaded[1].items():
        assert len(value) == 3, role
        assert value[2] == pytest.approx(-0.25)


def test_load_project_irs_roundtrip(tmp_path) -> None:
    path, source = _saved_project(tmp_path, '2.2', 2)
    loaded = load_project_irs(Project.load(path), path)
    assert sorted(loaded) == [1, 2]
    for pos, role_map in source.items():
        assert sorted(loaded[pos]) == sorted(role_map)
        for role, (ir, rate) in role_map.items():
            got, got_rate = loaded[pos][role]          # 시간 원점이 0 이면 예전 그대로 2-튜플
            assert got_rate == rate
            assert np.allclose(got, np.asarray(ir, dtype=np.float32), atol=1e-6)


def test_load_project_irs_without_measurements() -> None:
    assert load_project_irs(Project(config=SpeakerConfig.from_preset('2.0'))) == {}
