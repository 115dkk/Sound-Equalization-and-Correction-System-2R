# -*- coding: utf-8 -*-
"""4단계 "예상 소요" — 위치당 표시가 주, 전체 합계는 작은 글자(2026-09-23 사용자 지시).

전에는 "약 14 분 20 초"(전 위치 합계)만 보여줘 위치 하나에 그만큼 걸리는 줄 오해했다.
"""
from __future__ import annotations

from test_wizard_common import PRESET, engine, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def test_estimate_label_is_per_position_and_sub_label_is_the_total(qapp, project):
    from secs.wizard.step4_layout import Step4Layout, format_estimate, position_seconds

    step = Step4Layout(project)
    try:
        step.set_count(9)
        per_position = position_seconds(project)
        assert step.estimate_label.text() == f'위치당 {format_estimate(per_position)}'
        assert step.estimate_sub_label.text() == (
            f'9 위치 합계 {format_estimate(9 * per_position)} · 마이크 이동 시간 제외')
    finally:
        step.deleteLater()


def test_estimate_follows_delay_and_position_count(qapp, project):
    from secs.wizard.step4_layout import Step4Layout, format_estimate, position_seconds

    step = Step4Layout(project)
    try:
        step.set_count(13)
        step.delay_combo.setCurrentIndex(step.delay_combo.findText('10 초'))
        per_position = position_seconds(project)
        assert '10' in project.extra['start_delay']
        assert step.estimate_label.text() == f'위치당 {format_estimate(per_position)}'
        assert step.estimate_sub_label.text().startswith('13 위치 합계')
    finally:
        step.deleteLater()


def test_position_seconds_is_channel_count_times_sweep_plus_delay(qapp, project):
    from secs.wizard.step4_layout import position_seconds, start_delay_seconds, sweep_seconds

    bands = sum(len(s.bands) for s in project.config.speakers)
    assert bands == 5      # 2.1 · 바이앰프: L(LF,HF) + R(LF,HF) + SW1
    expected = bands * sweep_seconds(project) + start_delay_seconds(
        project.extra.get('start_delay', ''))
    assert position_seconds(project) == expected


def test_format_estimate_matches_the_registered_pattern():
    from secs.wizard.step4_layout import format_estimate

    assert format_estimate(96) == '약 1 분 36 초'
    assert format_estimate(45) == '약 45 초'
