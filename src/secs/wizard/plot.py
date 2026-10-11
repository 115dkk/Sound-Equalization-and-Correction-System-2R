# -*- coding: utf-8 -*-
"""그래프 공용 — 주파수 축 고정 눈금, 곡선 색(목업 `drawDesign` 색 그대로), 표시 스무딩 폭.

부르는 곳: 5단계 측정 그래프(`step5_measure`, `freq_ticks`·`octave_smoothing_sigma`)와 보정 설계 그래프
(`step_calibration`, 눈금 + 색).
옛 6단계 그래프(`DesignPlot`·`XoStrip`, 커튼·XO·타깃 제어점 끌기)는 2026-10-10 슬림화로 6단계 화면과 함께 지웠다
(되살릴 기준: git 태그 slim-before-2026-10-10).
"""
from __future__ import annotations

import numpy as np

#: 목업 drawDesign 의 색(그대로).
C_MEASURED = '#c23f9b'
C_CORRECTED = '#7f8a1f'
C_TARGET = '#8c781d'

#: 주파수 축 고정 눈금(09-24 QA: 로그 축 기본 눈금이 1920 폭에서 '5·10¹ 6·10¹…' 로 겹치고
#: 1024 폭에서는 10²·10³ 지수 표기였다).
FREQ_TICKS = (5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000)


def octave_smoothing_sigma(frequencies: np.ndarray, octave_fraction: float) -> float:
    """로그 격자에서 반치폭이 1/`octave_fraction` 옥타브인 가우시안의 sigma(표본 수).

    격자는 옥타브당 표본 수가 일정하다고 본다(`np.geomspace`). 옛 1.41 엔진(`room_correction`)에서 옮겼다(10-10).
    """
    total_octaves = np.log2(frequencies[-1] / frequencies[0])
    samples_per_octave = (len(frequencies) - 1) / total_octaves
    fwhm_to_sigma = 2.3548
    return samples_per_octave / (octave_fraction * fwhm_to_sigma)


def freq_tick_label(hz: float) -> str:
    hz = float(hz)
    return f'{hz / 1000:g}k' if hz >= 1000 else f'{hz:g}'


def freq_ticks(freqs=None):
    """pyqtgraph `setTicks` 용 [(log10 x, 글자)] — 보조 눈금은 비운다."""
    values = FREQ_TICKS if freqs is None else freqs
    return [[(float(np.log10(hz)), freq_tick_label(hz)) for hz in values], []]


#: 주파수 축 양 끝 여유(로그 범위에 대한 비율). 끝 눈금 글자('5' · '20k')가 축 밖으로 반쯤 나가면 pyqtgraph 가 그 글자를
#: 통째로 숨긴다 — 10-10 3차 시험: 보정 설계 SPL 가로축 눈금이 10k 에서 끝나 20k 가 없었다(범위 끝 = 20 kHz, 여유 0).
FREQ_PAD = 0.02


def set_freq_range(plot, hz_range) -> None:
    """로그 주파수 축 범위(Hz) — 끝 눈금 글자가 보이게 양 끝에 조금 여유를 둔다."""
    plot.setXRange(float(np.log10(hz_range[0])), float(np.log10(hz_range[1])), padding=FREQ_PAD)


def axis_label(plot, side: str, text) -> None:
    """축 이름(‘dB’ · ‘Hz’ · ‘ms’) — 눈금 글자와 같은 진하기(ink2)로. pyqtgraph 는 축 선 색(setPen = 옅은 격자색)을
    이름 색으로 덮어써 거의 안 보였다(10-10 3차 시험) — 색을 이름에 직접 준다."""
    from secs.wizard.style import COLORS

    plot.setLabel(side, str(text), color=COLORS['ink2'])


__all__ = ['C_MEASURED', 'C_CORRECTED', 'C_TARGET', 'FREQ_TICKS', 'FREQ_PAD', 'axis_label', 'freq_tick_label',
           'freq_ticks', 'octave_smoothing_sigma', 'set_freq_range']
