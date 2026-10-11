"""합성 측정 생성기(`tests/synth.py`) 자체 점검 — 결정적이고 채널마다 다르며, 확장 생성기가 기본 생성기와 같다.

2026-10-10 슬림화로 1.41 엔진·골든 대조(test_golden · test_channels · test_determinism 의 엔진 부분)를 지우면서
생성기 검사만 여기로 모았다. 생성기는 보정 설계·IR 로더 테스트가 입력으로 쓴다.
"""
from __future__ import annotations

import numpy as np
import pytest

import synth


def test_synth_is_deterministic():
    first = synth.measurement_set("2.2")
    second = synth.measurement_set("2.2")
    assert set(first) == set(second)
    for role in first:
        assert np.array_equal(first[role][0], second[role][0]), role
        assert first[role][1] == second[role][1]


@pytest.mark.parametrize("layout_value", ["2.0", "2.1", "2.2"])
def test_extended_synth_matches_the_basic_generator(layout_value):
    """확장 생성기가 2.0/2.1/2.2 에서 기본 생성기(옛 골든 입력)와 비트 동일하다."""
    main_ids = ("L", "R")
    sub_ids = {"2.0": (), "2.1": ("S1",), "2.2": ("S1", "S2")}[layout_value]

    basic = synth.measurement_set(layout_value)
    extended = synth.channel_measurement_set(main_ids, sub_ids)

    assert set(extended) == set(basic)
    for role in basic:
        basic_ir, basic_rate = basic[role]
        extended_ir, extended_rate = extended[role]
        assert extended_rate == basic_rate
        assert np.array_equal(extended_ir, basic_ir), f"{role} 이 비트 동일하지 않다"


def test_synth_channel_sets_are_deterministic_and_distinct():
    first = synth.role_measurement_set(15, 8)
    second = synth.role_measurement_set(15, 8)
    assert set(first) == set(second) == set(synth.MAIN_ROLE_ORDER + synth.SUB_ROLE_ORDER)
    for role in first:
        assert np.array_equal(first[role][0], second[role][0]), role
        assert np.all(np.isfinite(first[role][0])), role
        assert float(np.max(np.abs(first[role][0]))) > 1e-6, role
    roles = list(first)
    for left, right in zip(roles, roles[1:]):
        assert not np.array_equal(first[left][0], first[right][0]), (left, right)


def test_synth_channel_measurement_set_rejects_duplicate_ids():
    with pytest.raises(ValueError):
        synth.channel_measurement_set(("L", "R"), ("R",))
