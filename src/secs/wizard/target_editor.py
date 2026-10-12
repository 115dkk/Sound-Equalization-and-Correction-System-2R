# -*- coding: utf-8 -*-
"""타겟 커브 프리셋 도우미 — 보정 설계 화면(`step_calibration`)의 SPL 그래프 우클릭 메뉴가 쓴다.

타겟 편집 창(`TargetEditor`)은 커밋 3faea8e(10-09, "타겟 커브를 SPL 그래프에서 바로 편집")로 화면에서 빠졌고
2026-10-10 슬림화로 지웠다. 남은 것은 프리셋 판정·적용 두 함수다.

입력: `project`(`project.target`·`project.bc['target_preset']` 를 읽고 쓴다). 출력: 프리셋 키 / 적용 여부.
"""
from __future__ import annotations

from secs.model import TargetCurve

#: 타겟 프리셋 (키, 화면 이름) — 보정 설계 SPL 그래프 우클릭 메뉴의 순서. 곡선은 `TargetCurve.presets()`.
TARGET_PRESETS: tuple[tuple[str, str], ...] = (
    ('flat', '플랫'), ('harman', '하만'), ('cinema', '시네마'))


def preset_of(project) -> str:
    """지금 타겟이 어느 프리셋과 같은가('' = 사용자 지정 — 손으로 고쳤거나 불러왔다)."""
    points = sorted((float(f), float(db)) for f, db in project.target.points)
    for key, _text in TARGET_PRESETS:
        preset = TargetCurve.presets().get(key)
        if preset is not None and sorted(preset.points) == points:
            return key
    return ''


def apply_preset(project, key: str) -> bool:
    preset = TargetCurve.presets().get(str(key))
    if preset is None:
        return False
    project.target.points = list(preset.points)
    project.bc['target_preset'] = str(key)
    return True


__all__ = ['TARGET_PRESETS', 'apply_preset', 'preset_of']
