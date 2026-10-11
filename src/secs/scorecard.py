# -*- coding: utf-8 -*-
"""좋은 응답 성적표의 지표 계산(2026-09-25 `tools/scorecard.py` 에서 옮김, 09-26) — REW 로 볼 때와 같은 것을 숫자로.

누가 부르나: `tools/scorecard.py`(돌비 성적표 명령), `secs.calibration`(보정 설계 화면의 채널별 지표 줄).
입력: 한 채널의 복소 응답 H(`correction_engine.freqs()` 선형 격자), 그 격자 f, 타깃 dB(같은 격자). 출력: 지표 사전.
  * SPL: 1/6 oct 로 편 곡선이 타깃(대역 100–400 Hz 중앙값으로 레벨 맞춤 — RC 범위 안) ±1·±2 dB 안인 비율 — 20–200 / 200–2k / 2k–16k
  * 위상: 도착 지연을 뺀 1/6 oct 복소 스무딩 위상이 20 Hz–20 kHz 에서 도는 바퀴 수(잘 펴진 응답 ≈ 1)
  * GD: 같은 위상의 GD p10–90 폭(ms) — 20–100 / 100–500 / 500–5k
  * 1/24 oct GD(20–500 Hz, 중앙값 기준) 최대 봉우리·p5–95 폭(09-26 사용자: 1/6 oct 가 147 Hz 봉우리를 가렸다) ·
    절대 봉우리 `gd24_abs`·그 주파수(10-09 — 화면 지표, 중앙값 기준은 도구 비교용으로 남김)
  * GD 폭 30–100 Hz(`gd_30`, 20–25 Hz 가장자리 제외)와 앞울림(`pre_db`, 피크 앞 3–50 ms / 뒤 50 ms) — 09-27
"""
from __future__ import annotations

import numpy as np


def metrics(H, f, target_db, norm=(100.0, 400.0)) -> dict:
    from secs.correction_engine import gd_curve, smooth_complex, smooth_power

    db = 10 * np.log10(smooth_power(f, np.abs(H) ** 2, 6.0) + 1e-30)
    m = (f >= norm[0]) & (f <= norm[1])          # 레벨 맞춤 대역 = RC 범위 안(09-25: RC 상한 500 Hz)
    dev = db - target_db
    dev = dev - np.median(dev[m])
    out = {'level': float(np.median((db - target_db)[m]))}
    for lo, hi in ((20, 200), (200, 2000), (2000, 16000)):
        b = (f >= lo) & (f <= hi)
        out[f'in1_{lo}'] = 100 * float(np.mean(np.abs(dev[b]) <= 1))
        out[f'in2_{lo}'] = 100 * float(np.mean(np.abs(dev[b]) <= 2))
    h = np.fft.irfft(H)
    t = np.argmax(np.abs(h)) / (2 * f[-1])
    Hs = smooth_complex(f, H * np.exp(2j * np.pi * f * t), 6.0)
    g = np.geomspace(20, 20000, 2000)
    ph = np.unwrap(np.angle(np.interp(g, f, Hs.real) + 1j * np.interp(g, f, Hs.imag)))
    k = (g >= 1000) & (g <= 10000)                       # 남은 순수 지연 = 1–10 kHz 위상 기울기(REW 'estimate IR delay' 처럼)
    ph = ph - np.polyfit(g[k], ph[k], 1)[0] * g
    out['turns'] = float((ph.max() - ph.min()) / (2 * np.pi))
    gd = -np.gradient(ph, 2 * np.pi * g) * 1000
    for lo, hi in ((20, 100), (100, 500), (500, 5000)):
        b = (g >= lo) & (g <= hi)
        out[f'gd_{lo}'] = float(np.percentile(gd[b], 90) - np.percentile(gd[b], 10))
    g24 = np.geomspace(20, 500, 1500)
    gd24 = gd_curve(H, f, g24)
    # 절대 봉우리(10-09): '봉우리 − 중앙값'(gd24_max)은 FIR 이 중앙값을 낮추면 봉우리가 그대로여도 커 보였다(돌비룸 L 438 Hz
    # 절대 211 ms 그대로인데 168 → 203). 화면은 이것을 쓴다 — gd_curve 가 IR 피크 지연을 이미 뺐으니 0 ms 가 피크 도착.
    out['gd24_abs'] = float(np.max(gd24))
    out['gd24_abs_hz'] = float(g24[int(np.argmax(gd24))])
    gd24 = gd24 - np.median(gd24)
    out['gd24_max'] = float(np.max(gd24))
    out['gd24_span'] = float(np.percentile(gd24, 95) - np.percentile(gd24, 5))
    # 09-27 성적표 보강 제안: 20–25 Hz 가장자리 급경사가 gd_20 을 부풀려 XO·모드 문제와 구분이 안 됐다 → 30–100 Hz 폭.
    b30 = (g >= 30) & (g <= 100)
    out['gd_30'] = float(np.percentile(gd[b30], 90) - np.percentile(gd[b30], 10))
    # 앞울림: 피크 앞 3–50 ms 에너지 / 피크 ±3 ms 뒤 50 ms 에너지(dB) — 선형위상 FIR 보정이 늘리는지 본다.
    k = int(np.argmax(np.abs(h)))
    fs = 2 * f[-1]
    a3, a50 = int(0.003 * fs), int(0.05 * fs)
    pre = np.sum(np.take(h, np.arange(k - a50, k - a3), mode='wrap') ** 2)
    main = np.sum(np.take(h, np.arange(k - a3, k + a50), mode='wrap') ** 2)
    out['pre_db'] = float(10 * np.log10(pre / (main + 1e-30) + 1e-30))
    return out


def row(name, m) -> str:
    return (f"{name:10s} {m['in1_20']:4.0f}/{m['in2_20']:3.0f} {m['in1_200']:4.0f}/{m['in2_200']:3.0f} "
            f"{m['in1_2000']:4.0f}/{m['in2_2000']:3.0f} | {m['turns']:5.1f} | {m['gd_20']:6.1f} {m['gd_100']:6.1f} "
            f"{m['gd_500']:6.1f} | {m['level']:6.1f}")


__all__ = ['metrics', 'row']
