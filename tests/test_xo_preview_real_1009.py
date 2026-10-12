"""XO 탭 합 곡선 = SPL 탭 보정 후(돌비룸 실측 2026-10-09, 10-09 버그 조사).

증상: 보정 설계 XO 탭(프론트 120 Hz, 초고지연)의 '보정 후' 합이 97 Hz 근처에서 타겟보다 +15 dB 솟았는데 같은 채널의
SPL 탭 보정 후는 타겟에 붙어 있었다. 원인: `xo_preview` 가 서브 합을 원 측정 `engine.H0` 로 만들어 R30 레이아웃 묶음
PEQ(레이아웃 단계가 `engine.H` 의 서브에만 곱한다)가 빠졌다. 이 프로젝트는 SW1·SW2 가 묶음 A 멀티서브다.

실측 파일은 git 밖이라 있을 때만 돈다(`SECS_DOLBY_1009` 또는 바탕화면 원본 — 읽기만 한다, 저장하지 않는다).
"""
from __future__ import annotations

import os
from pathlib import Path

import numpy as np
import pytest

from conftest import engine_choice, load_engine

pytestmark = pytest.mark.skipif(engine_choice() != 'package', reason='패키지 엔진 전용')
load_engine()

SECS = Path(os.environ.get('SECS_DOLBY_1009')
            or Path.home() / 'Desktop' / 'SECS' / '돌비룸 실측 2026-10-09.secs')


@pytest.mark.skipif(not SECS.is_file() or not SECS.with_suffix('.irs').is_dir(), reason=f'실측 파일 없음: {SECS}')
def test_xo_sum_matches_spl_after_on_real_bundle_layout():
    from secs import calibration as cal
    from secs.rewapi.session import load_project_irs
    from secs.model import Project

    project = Project.load(SECS)
    irs = load_project_irs(project, SECS)
    settings = cal.CalibrationSettings.from_project(project)
    assert settings.bundle_layouts, '묶음 레이아웃이 있는 실측이어야 이 버그가 보인다'
    settings.xo = {g: 120.0 for g in cal.xo_groups(project, irs)}
    run = cal.compute(project, irs, settings)
    f = run.result.f
    band = (cal.XO_GRID >= 30) & (cal.XO_GRID <= 400)
    for group in run.engine.groups:
        if group == 'sub':
            continue
        view = cal.xo_preview(run, group, run.engine.xo[group])
        roles = [r for r in run.engine.groups[group] if r in run.result.after]
        power = np.mean([np.abs(run.result.after[r]) ** 2 for r in roles], axis=0)
        ref = np.interp(cal.XO_GRID, f, 10 * np.log10(cal.smooth_power(f, power, cal.XO_FRAC) + 1e-30))
        worst = float(np.max(np.abs(view['sum'][band] - ref[band])))
        assert worst < 1.0, (group, worst)
