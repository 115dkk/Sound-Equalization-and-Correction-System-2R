# -*- coding: utf-8 -*-
"""2026-09-25 사용자 지적(3.0.0 실사용)을 되살아나지 않게 지키는 검사 — 새 프로젝트 기본값.

지적 원문·보고서: `보고서/09 지난 보고서/구 엔진/3.0.0 결함 원인과 개선 계획 2026-09-25.html`.
옛 6·7단계 화면에 대한 나머지 검사(응답 없음·그래프·XO 막대·상태 문구 등)는 2026-10-10 슬림화로 그 화면과
함께 지웠다.
"""
from __future__ import annotations

from conftest import load_engine

load_engine("package")  # sys.path 에 src 를 넣는다


def test_new_project_uses_full_band_mixed_phase_defaults() -> None:
    from secs.wizard import new_project

    project = new_project(fake=True)
    assert project.curtain == (20.0, 20000.0)
    assert project.bc['phase'] == 'mixed'
