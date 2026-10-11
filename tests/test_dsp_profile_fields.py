# -*- coding: utf-8 -*-
"""DSP 프로파일의 자원 칸(`.secs` 파일 형식) — 프리셋 값 · 저장/읽기 · 옛 파일 기본값.

옛 설계 엔진 테스트(test_c1_c4_0924 · test_mixed_phase · test_lowband_model)에 들어 있던 모델 전용 검사를
2026-10-10 슬림화 때 여기로 모았다. 엔진은 지웠지만 이 칸들은 `.secs` 형식이라 그대로 읽고 쓴다.
"""
from __future__ import annotations

from conftest import load_engine

load_engine("package")  # sys.path 에 src 를 넣는다

import secs.model as model  # noqa: E402


def test_dsp_profile_bm_path_delays() -> None:
    presets = model.DspProfile.presets()
    assert presets['tesira'].bm_path_delays == 16 and presets['minidsp'].bm_path_delays == 0
    assert presets['camilla'].bm_path_delays == 64
    assert model.DspProfile.from_dict(presets['tesira'].to_dict()).bm_path_delays == 16
    old = presets['minidsp'].to_dict()
    del old['bm_path_delays']                       # 09-24 이전 .secs
    assert model.DspProfile.from_dict(old).bm_path_delays == 0
    old['profile'] = 'unknown'
    assert model.DspProfile.from_dict(old).bm_path_delays == 16


def test_dsp_profile_allpass_bands() -> None:
    presets = model.DspProfile.presets()
    assert presets['tesira'].allpass_bands == 16 and presets['minidsp'].allpass_bands == 0
    assert model.DspProfile.from_dict(presets['tesira'].to_dict()).allpass_bands == 16
    old = presets['minidsp'].to_dict()
    del old['allpass_bands']                        # 09-23 이전 .secs
    assert model.DspProfile.from_dict(old).allpass_bands == 0
    old['profile'] = 'unknown'
    assert model.DspProfile.from_dict(old).allpass_bands == 16


def test_dsp_profile_lowband_resources() -> None:
    presets = model.DspProfile.presets()
    tesira = presets['tesira']
    assert (tesira.bm_path_polarity, tesira.bm_path_allpass, tesira.path_peq_bands,
            tesira.path_allpass_bands, tesira.per_output_lp) == (False, 0, 0, 0, False)
    assert presets['camilla'].per_output_lp and presets['camilla'].bm_path_polarity
    again = model.DspProfile.from_dict(presets['camilla'].to_dict())
    assert again.per_output_lp and again.bm_path_allpass == presets['camilla'].bm_path_allpass
    old = presets['camilla'].to_dict()
    for key in ('bm_path_polarity', 'bm_path_allpass', 'path_peq_bands', 'path_allpass_bands', 'per_output_lp'):
        del old[key]                                # 개정 8 이전 .secs → 프로파일 기본값
    assert model.DspProfile.from_dict(old).per_output_lp is True
