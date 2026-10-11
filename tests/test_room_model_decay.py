"""합성 다점 감쇠 방(`room_model_decay`)의 자 자체를 점검한다 — 엔진은 부르지 않는다(빠름).

모드 T60 측정이 이론값 2.2·Q/f 와 맞는지, 픽스처가 엔진 입력 모양·결정성을 지키는지.
"""
from __future__ import annotations

import numpy as np
import pytest

import room_model_decay as rmd
from conftest import load_engine


@pytest.fixture(scope="module")
def case():
    load_engine("package")
    return rmd.build_case("2.2")


def test_long_axial_modes_ring_as_their_q_says(case):
    ir = np.fft.irfft(case.low["L"][0], rmd.LOW_N_FFT)
    for (nx, ny, nz), q in rmd.LONG_Q.items():
        hz = rmd.SPEED_OF_SOUND / 2 * np.sqrt((nx / 6.2) ** 2 + (ny / 4.4) ** 2 + (nz / 2.7) ** 2)
        if round(hz, 2) not in case.strong_modes["L"]:
            continue                                # 청취 위치가 마디인 모드는 재지 않는다
        measured = rmd.mode_t60(ir, rmd.LOW_RATE, hz)
        assert measured == pytest.approx(2.2 * q / hz, rel=0.1), (hz, q)


def test_fixture_matches_engine_input_shape_and_is_deterministic(case):
    assert case.inputs == ["L", "R"] and case.roles == ["L", "R", "SW1", "SW2"]
    assert sorted(case.irs) == [1, 2, 3, 4, 5]
    ir, rate, t0 = case.irs[1]["SW1"]
    assert rate == 48000 and t0 == 0.0 and ir.size == rmd.N_FFT
    assert case.low["L"].shape == (5, rmd.LOW_N_FFT // 2 + 1)
    again = rmd.build_case("2.2")
    assert np.array_equal(again.irs[3]["R"][0], case.irs[3]["R"][0])
    # 5.1.2 는 서브 2 + 탑 2 가 붙은 구성
    project = rmd.make_project("5.1.2")
    assert project.config.roles() == ["L", "R", "C", "SW1", "Ls", "Rs", "Tfl", "Tfr", "SW2"]

def test_modal_room_matches_the_experiment_setup():
    """`room_model.ModalRoom`(이 합성 방의 바탕) — `실험/FIR 탭수 vs 감쇠 실험 결과 2026-09-15.md` 의 방과 같다.
    (옛 `test_bass_metrics` 에서 옮김 — 옛 저역 엔진 지표 검사는 2026-10-10 슬림화로 엔진과 함께 지웠다.)"""
    import room_model

    room = room_model.ModalRoom()
    assert len(room.modes) == 164
    assert room.room_scale == pytest.approx(1.6e4)     # 리플 14 dB 를 처음 넘는 스케일
    assert room.freqs[1] == pytest.approx(1200 / 8192)
