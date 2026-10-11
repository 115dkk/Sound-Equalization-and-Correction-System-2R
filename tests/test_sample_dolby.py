# -*- coding: utf-8 -*-
"""샘플 모드 실측 IR(2026-09-24) — `배포\\SECS 위저드 (샘플 데이터).cmd` 가 켜는 FakeREW 가

`tools/make_sample_dolby.py` 로 뽑은 `secs/rewapi/sample_dolby_714.npz`(2026-03-10 돌비룸
실측)의 실제 임펄스 응답을 7.1.4 + 서브 3(SW1 = DBA L · SW2 = DBA R · SW3 = INFRA, 2026-10-11 사용자 결정)의
원측정 role 에 돌려주는지, 나머지는 그대로 합성인지, 채널 사이 도착 시각이 같은 시간축에서 그럴듯한지 본다.

`use_sample_dolby` 기본값은 꺼져 있다 — 다른 테스트는 전부 합성 IR 의 결정적 값을 가정하므로
여기서만 켠다(`CLAUDE.md`: 기존 테스트는 합성 그대로 두는 쪽을 우선).
"""
from __future__ import annotations

import numpy as np
import pytest

from conftest import engine_choice, load_engine

pytestmark = pytest.mark.skipif(
    engine_choice() != "package",
    reason="secs.rewapi 는 패키지 엔진 전용이다 (SECS_ENGINE=package).",
)


@pytest.fixture(scope="module")
def engine():
    return load_engine("package")


#: 실측이 있는 원측정 role(= sample_dolby_714.npz 안의 채널) — 7.1.4 + 서브 3.
SAMPLE_ROLES = ('L', 'R', 'C', 'Ls', 'Rs', 'Lb', 'Rb', 'Tfl', 'Tfr', 'Tbl', 'Tbr', 'SW1', 'SW2', 'SW3')
#: 서브 → 원본 mdat 측정(제목, 측정 순서 0부터). INFRA 는 제목이 둘 — 원측정은 #2(`tools/make_sample_dolby.py` 설명).
SUB_SOURCES = {'SW1': ('DBA L', 0), 'SW2': ('DBA R', 1), 'SW3': ('INFRA', 2)}


@pytest.fixture()
def sample_fake(engine):
    from secs.rewapi.fake import FakeREW

    rew = FakeREW(delay_s=0.0, use_sample_dolby=True)
    yield rew
    rew.close()


@pytest.fixture()
def plain_fake(engine):
    from secs.rewapi.fake import FakeREW

    rew = FakeREW(delay_s=0.0)
    yield rew
    rew.close()


def test_npz_file_exists_and_is_small(engine):
    from secs.rewapi.fake import _sample_dolby_path

    path = _sample_dolby_path()
    assert path.is_file(), f'샘플 npz 없음: {path} (tools/make_sample_dolby.py 로 만들 것)'
    assert path.stat().st_size < 3 * 1024 * 1024, '샘플 npz 가 3 MB 목표를 넘었다'


@pytest.mark.parametrize('role', SAMPLE_ROLES)
def test_sample_mode_returns_the_real_measurement_at_position_1(sample_fake, role):
    """위치 1 은 원본 그대로(`_position_variation` 이 손대지 않는다)."""
    from secs.rewapi.fake import _load_sample_dolby

    table = _load_sample_dolby()
    assert role in table, f'{role} 이 샘플 npz 에 없다'
    expected_ir, expected_rate, expected_t0 = table[role]

    ir = sample_fake.ir_for(role, 1, expected_rate)
    np.testing.assert_array_equal(ir, expected_ir)


def test_sample_mode_refuses_position_2(sample_fake):
    """샘플은 실측 1점뿐(09-24 사용자 결정) — 위치 2 이상은 변형 가짜를 만들지 않고 막는다.
    실측 밖 합성 role(SW4)도 같은 규칙(샘플 모드 = 1점)."""
    import threading

    from secs.rewapi.fake import SAMPLE_SINGLE_POSITION
    from secs.rewapi.interfaces import MeasurementPlan, RewError

    for role in ('L', 'SW3', 'SW4'):
        with pytest.raises(RewError, match=SAMPLE_SINGLE_POSITION):
            sample_fake.ir_for(role, 2, 48000)
    plan = MeasurementPlan(roles=['L', 'R'], outputs={'L': 1, 'R': 2}, positions=[1, 2],
                           output_device='Demo ASIO 16ch', input_device='Demo USB Mic',
                           input_channel=1)
    with pytest.raises(RewError, match=SAMPLE_SINGLE_POSITION):
        sample_fake.run_position(plan, 2, None, threading.Event())
    assert len(sample_fake.run_position(plan, 1, None, threading.Event())) == 2


def test_synthetic_fake_keeps_multi_position_variation(plain_fake):
    """테스트용 합성 FakeREW(기본)의 다점 변형은 그대로."""
    ir1 = plain_fake.ir_for('L', 1, 48000)
    ir2 = plain_fake.ir_for('L', 2, 48000)
    assert not np.array_equal(ir1, ir2)
    assert ir1.shape == ir2.shape


def test_sample_mode_keeps_the_real_inter_channel_t0_differences(sample_fake):
    """채널마다 실제 도착 시간 차이(t0)가 보존된다 — 전부 같은 값이면 안 된다."""
    from secs.rewapi.interfaces import MeasurementPlan

    plan = MeasurementPlan(roles=list(SAMPLE_ROLES),
                           outputs={r: i + 1 for i, r in enumerate(SAMPLE_ROLES)},
                           positions=[1], output_device='Demo ASIO 16ch',
                           input_device='Demo USB Mic', input_channel=1)
    import threading
    results = sample_fake.run_position(plan, 1, None, threading.Event())
    t0s = {r.role: r.t0_s for r in results}
    assert len(set(t0s.values())) > 1, t0s          # 채널마다 달라야 한다
    assert all(t0 != 0.0 for t0 in t0s.values())


def test_roles_outside_714_or_biamp_bands_stay_synthetic(sample_fake, plain_fake):
    """샘플에 없는 role(SW4·Lw·바이앰프 대역)은 켜져 있어도 합성 그대로."""
    for role in ('SW4', 'Lw', 'Tml', 'L:LF'):
        sample_ir = sample_fake.ir_for(role, 1, 48000)
        plain_ir = plain_fake.ir_for(role, 1, 48000)
        np.testing.assert_array_equal(sample_ir, plain_ir)


def test_default_fake_rew_stays_synthetic(plain_fake):
    """`use_sample_dolby` 를 안 주면(기존 테스트·도구) 여전히 합성 IR."""
    from secs.rewapi.fake import _fallback_ir

    ir = plain_fake.ir_for('L', 1, 48000)
    # tests/synth.py 가 있으면 그쪽, 없으면 _fallback_ir — 어느 쪽이든 npz 실측과는 다르다.
    from secs.rewapi.fake import _load_sample_dolby
    real_ir, real_rate, _t0 = _load_sample_dolby()['L']
    assert ir.shape != real_ir.shape or not np.allclose(ir, real_ir)



def test_three_real_subs_with_their_mdat_sources(engine):
    """서브 3개 = 원본 서브 원측정 셋(10-11 사용자 결정) — 셋이 서로 다른 실측이고, 어느 측정에서 왔는지 메타에 남는다."""
    import json

    from secs.rewapi.fake import _load_sample_dolby, _sample_dolby_path

    table = _load_sample_dolby()
    subs = [np.asarray(table[r][0]) for r in ('SW1', 'SW2', 'SW3')]
    for a in range(3):
        for b in range(a + 1, 3):
            assert subs[a].shape != subs[b].shape or not np.allclose(subs[a], subs[b])
    with np.load(_sample_dolby_path()) as data:
        meta = json.loads(str(data['meta_json']))
    for role, (title, index) in SUB_SOURCES.items():
        assert meta['sources'][role] == {'title': title, 'index': index}, role
    assert 'LF 찐' not in json.dumps(meta['sources'], ensure_ascii=False)


def test_arrivals_share_one_time_axis(engine):
    """엔진과 같은 식(t0 + onset/rate)의 도착 시각이 모든 채널에서 청취 위치까지 거리로 그럴듯하다 — 같은 음향 타이밍
    레퍼런스로 잰 원측정이라 서브도 메인과 같은 시간축이다(10-11: 서브 5.1 · 5.2 · 7.6 ms, 메인 2.9–5.6 ms).
    처리된 INFRA(#6, LP 걸림)를 골랐다면 SW3 가 12.6 ms 로 늦었다."""
    from secs.correction_engine import onset_index
    from secs.rewapi.fake import _load_sample_dolby

    table = _load_sample_dolby()
    arrival_ms = {r: 1000.0 * (t0 + onset_index(np.asarray(ir, float)) / rate) for r, (ir, rate, t0) in table.items()}
    assert set(arrival_ms) == set(SAMPLE_ROLES)
    assert all(2.0 < ms < 10.0 for ms in arrival_ms.values()), arrival_ms         # 0.7–3.4 m
    mains = [arrival_ms[r] for r in SAMPLE_ROLES if not r.startswith('SW')]
    for role in ('SW1', 'SW2', 'SW3'):
        assert min(mains) - 1.0 < arrival_ms[role] < max(mains) + 3.0, (role, arrival_ms)


def test_sample_project_measures_every_sub_from_the_real_data(engine):
    """샘플 프로젝트(`new_project(fake=True)` = 7.1.4 + 서브 3)의 역할은 모두 실측 — 서브 2 · 3 이 합성으로 채워지지 않는다."""
    from secs.rewapi.fake import FakeREW, _load_sample_dolby
    from secs.wizard import new_project

    project = new_project(fake=True)
    roles = [s.role for s in project.config.speakers]
    assert sorted(roles) == sorted(SAMPLE_ROLES)
    table = _load_sample_dolby()
    rew = FakeREW(delay_s=0.0, use_sample_dolby=True)
    try:
        for role in roles:
            ir, rate, t0 = table[role]
            np.testing.assert_array_equal(rew.ir_for(role, 1, rate), ir)
            assert rew._t0_for(role, rate) == t0
    finally:
        rew.close()
