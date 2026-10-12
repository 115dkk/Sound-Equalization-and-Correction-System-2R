# -*- coding: utf-8 -*-
"""보정 엔진(2026-09-25 새로 — 6·7단계 통합, `문서/요구 대장.md` 최상위 결정): 측정 → 레벨 · 딜레이 → BA →
채널 보정 필터 순서로 푼다.

순서:
  1. 기준 위치(첫 측정 = 청취 위치)의 도착 시각으로 **딜레이**, 대역 레벨로 **레벨 트림**을 맞춘다.
  2. **BA (Bass Alignment)**: 서브마다 게인 1 + 올패스 2 + (도착 보정 뒤 미세) 지연·극성으로 서브끼리 결합하고,
     서브 버스 전체 지연·극성을 골라 메인(HP)·서브(LP)가 XO 에서 합쳐지게 한다(저역 타겟은 모든 채널 공통).
     서브마다 수동 설정(10-05 요구 대장 R27, `sub_manual`): 고정 = 사람이 넣은 값 그대로, 묶음 = 안의 상대값은 그대로
     두고 묶음 전체의 게인·지연·극성만 찾는다(요구 대장 R27 설계 문서, 10-05). 서브마다 고정 대역 필터
     (HP·LP, 종류 LR 24 기본)·고정 PEQ(예: INFRA 로우패스 25 Hz)는 그 서브 스피커 응답에 처음부터 곱해 둔다(결합·버스 PEQ·MIMO 가
     모두 그 응답을 본다). 묶음 레이아웃(10-09 R30, `bundle_layouts`): DBA·멀티서브 묶음은 2단 합성 — 묶음 안을 먼저
     합치고(같은 필터 + 서브별 레벨, DBA 뒤 줄 반전·전파 지연, 묶음 PEQ) 그 덩어리를 한 서브처럼 BA 에 넣는다(`_layout_stage`).
  3. **채널 보정 필터는 베이스 매니지먼트 앞(입력 쪽)**에 둔다 — 메인+서브를 합친 응답을 한 필터로 고친다.
     크기: 위치 평균(기준 위치 가중)을 저역은 촘촘히(1/24 oct), 고역은 성기게(1/6 oct) 편 뒤 타겟과의 차를
     최소위상 FIR 로(부스트 ≤ 10 dB, 컷 ≤ 25 dB). 위상: 합의 초과위상(1/6 oct 복소 스무딩, 고역 3–6 kHz 기준 0)을
     ≤ 1 kHz 에서 되돌리고 4 kHz 까지 페이드(지연 `lift_ms` 200 ms — 60·120 ms 는 45 Hz 에서 FIR 밖으로 잘려
     크기가 무너졌다). 만든 FIR 크기가 설계와 어긋나면(RMS 0.5·최대 2 dB) 위상 보정 세기를 줄인다.
누가 부르나: `secs.calibration`(보정 설계 화면 · 내보내기), `tools/scorecard.py`(성적표). 입력: 위치 → role → (ir, sr, t0).
출력: `CorrectionResult`. 여러 스피커로 저역 늦은 울림을 줄이는 MIMO 는 이 결과 위에서 `secs.mimo` · `secs.mimo_iir` 가 푼다.
"""
from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

SR = 48000
N_FFT = 1 << 17                 # 설계 격자(0.37 Hz) — 저역 모드를 풀 만큼
TAPS = 32768                    # 채널 FIR 길이(683 ms, 1.41 과 같음)
MAX_BOOST_DB = 10.0             # 최대 부스트(dB) — 화면 '부스트 상한' 칸의 최대(`calibration.BOOST_RANGE`)
MAX_CUT_DB = 25.0
LIFT_MS = 200.0                  # 초과위상 보정의 지연(앞쪽 여유)
RC_AUTO_MAX_HZ = 500.0          # 자동 RC 상한의 최대(슈뢰더 주파수가 더 높거나 계산 못 하면 이 값)
# 화면 이름(10-10 사용자 결정 3A — '저사양 · 최고사양' 이 PC 사양처럼 읽혔다): 최소 지연 · 저지연(mixed) · 표준(linear) · 초고지연.
# 모드 구성(09-26 사용자 확정): 최소 지연 = RC IIR 만(PEQ 32, FIR 없음) + MIMO IIR · 저사양 = RC IIR(PEQ 16) + FIR 2,048
# + MIMO IIR · 최고사양 = RC IIR(PEQ 16) + FIR 2,048 + MIMO IIR 또는 FIR(서포트당 ≤ 8,000탭, 비교 뒤 확정).
# 위상 모드(09-25 사용자 명칭) — 전체 지연 예산(필터가 만드는 지연, 입출력 제외)으로 가른다. FIR 2,048탭 + 입력 PEQ 16.
#   최소 지연(zero, 09-26 이름 변경 — 옛 '무지연'): 위상을 펴지 않는다(최소위상 보정만 — 크기·룸 모드 공진은 고침), 지연 0
#   저사양(mixed — 옛 '믹스드'): 10 ms 안에서 저역 초과위상을 편다(주로 XO LR4)
#   최고사양(linear — 옛 '리니어'): 30 ms 안에서 편다(서브 롤오프·방 초과위상까지 더)
#   긴 필터: 32,768탭·200 ms — 참고용(카밀라 DSP 등 긴 FIR·지연 허용 환경)
MODES = {
    'zero': {'label': '최소 지연 모드', 'taps': 0, 'peq_bands': 32, 'lift_ms': 0.0, 'max_latency_ms': 0.0, 'mimo': 'iir'},
    'mixed': {'label': '저지연 모드', 'taps': 2048, 'lift_ms': 8.0, 'max_latency_ms': 10.0, 'mimo': 'iir'},
    'linear': {'label': '표준 모드', 'taps': 2048, 'lift_ms': 28.0, 'max_latency_ms': 30.0, 'mimo': 'iir', 'phase_frac': 24.0},
    'long': {'label': '긴 필터 모드', 'taps': 32768, 'lift_ms': 200.0, 'max_latency_ms': 215.0},
}
DIP_ALLOW_DB = 3.0             # 좁은 딥(1/24 가 1/3 oct 보다 깊은 곳)은 이만큼만 채운다(한 점 간섭 과적합 방지)
HF_BOOST_DB = 2.0               # 16 kHz 위 부스트 한도
PEQ_BANDS = 16                  # 짧은 FIR 일 때 저역 크기를 맡는 IIR(입력 쪽 PEQ)
FIR_TOUCHUP_ROUNDS = 2          # 3차 맞춤(2차 PEQ 뒤 남은 넓은 오차를 FIR 이 다시 맡기) 횟수 — 10-05 R1, 0 = 끔(비교용)
#: 보정 PEQ 밴드의 울림 상한(ms, 10-05 감쇠 악화). RBJ 피킹의 극 Q = Q·A(A = 10^(게인/40)) — 밴드는 시간 상수 Q·A/(π f) 로
#: 운다: 돌비 C 94 Hz +9.5 dB·Q 15.8(극 Q 27, 92 ms)은 같은 자리 넓은 컷이 크기로는 상쇄해도 울림이 남아 93 Hz 늦은 에너지
#: +28 dB, 125 Hz 대역 T20 0.38 → 0.44 s(채널 평균). 얕고 좁은 컷도 운다(Tbr 100 Hz −6.5 dB·Q 20 → 44 ms) — 깊은 노치는 A 가
#: 작아 괜찮다(모달 EQ). XO 타겟 PEQ 에는 걸지 않는다(서브 버스까지 걸면 50 Hz T20 0.42 → 0.53 s). 기본 30 ms(10-05 사용자
#: 결정 — 돌비 3서브 최소 지연·저사양·최고사양): 늦은 에너지 증가(버터워스, 보정 전 대비) +1.50/+1.44/+1.48 → +1.27/+1.14/+1.03 dB,
#: 125 Hz T20 0.44/0.42/0.45 → 0.39/0.39/0.40 s, 1/6 oct ±1 dB 최저 89.6/93.5/93.2 → 89.8/93.7/92.7 %, 1/24 평균 −1.8 %p.
#: 20 ms 는 늦은 에너지 +0.6–0.8 dB 까지 줄지만 최고사양 Tfr 1/6 88.6 %·1/24 −4~5 %p 라 버렸다. 'none' = 끔(비교용).
BOOST_RING_MS = (lambda v: None if v in ('', 'none') else float(v))(__import__('os').environ.get('SECS_BOOST_RING_MS', '30'))
#: BA 지연 탐색 폭(±ms, 도착 정렬 기준)과 합산 효율 가중(10-09 사용자: UM18 반전 +9.5 ms 처럼 더 잘 더해지는 값을
#: 엔진이 스스로 찾게) — 효율 = 서브 합 에너지 ÷ 개별 에너지 합(dB, 20 Hz–상한 평균, 높을수록 상쇄가 적어 최대음압이 크다).
#: 4.12.0 기본 12 ms · 1(돌비룸 실측 9점: ±3 ms · 0 대비 서브 합 평탄 0.66 → 0.32 dB, 합산 효율 1.4 → 2.5 dB, 같은 헤드룸
#: 최대음압 +4.0 dB, 채널 ±1 dB 비율 71.4 → 75.3 %, GD 최대 190 → 174 — 계산 시간 같음). 예전 답 = 3 · 0.
BA_DELAY_MS = float(__import__('os').environ.get('SECS_BA_DELAY_MS', '12'))
BA_EFF_WEIGHT = float(__import__('os').environ.get('SECS_BA_EFF', '1'))
DECAY_WEIGHT = 0.5              # BA: 늦은 에너지가 결합 전보다 늘면 (dB)² 벌점
SUB_MODES = ('auto', 'fixed', 'bundle')   # BA 방식(R27): 자동 · 고정(값 그대로) · 묶음(상대값 그대로, 공통 지연·극성만)
#: 묶음 레이아웃(10-09 사용자 R30 "서브우퍼 레이아웃에 DBA와 위치기반 멀티서브우퍼 2개의 레이아웃 선택지"): 2단 합성 — 묶음 안을
#: 먼저 합치고(필터 완전히 같음 + 레벨만 서브별 자동, DBA 뒤 줄은 반전 + 앞→뒤 전파 지연) 그 덩어리를 한 서브처럼 BA 에 넣는다.
#: 'values' = R27 그대로(넣은 상대값 고정) — 엔진에는 오지 않는다.
BUNDLE_LAYOUTS = ('dba', 'multi')
DBA_MAX_MS = 60.0               # DBA 뒤 줄 전파 지연 탐색 상한(≈ 20 m 방)
DBA_LATE_WEIGHT = 0.2           # 전파 지연 탐색 비용에서 늦은 에너지(dB) 가중 — DBA 의 목적이 뒷벽 반사(길이 모드) 지우기
BUNDLE_PEQ_BANDS = 6            # 묶음 합에 한 번 설계해 묶음 서브 전부에 똑같이 거는 PEQ 밴드 수(부스트 ≤ 6 dB, 서브 버스 PEQ 와 같은 한도)
GD_WEIGHT = 0.02                # BA 비용에서 GD 분산(ms²) 가중
#: 서브 버스 PEQ 맞춤 상한 = 가장 높은 메인 XO × 이 값(10-09 돌비룸 실측: 상한이 평균 XO × 2(≈ 240 Hz)라 XO 위에 197 Hz +6 ·
#: 229 Hz −6 dB 밴드가 생겼고, 서브 경로가 438 Hz 에서도 탑보다 −24 dB 로 남아 L 의 438 Hz GD 봉우리를 +26 ms 키웠다).
#: 1.25 × XO 에서 LR24 LP 가 이미 −10.7 dB 라 그 위는 메인이 합을 정한다 — 서브 버스가 고칠 몫이 아니다(그 위는 채널 FIR 몫).
#: 밴드 중심도 이 상한 안에 묶는다(`fit_peq(f_max=)` — 다듬기에서 XO 위로 미끄러지지 않게).
BUS_PEQ_XO_RATIO = 1.25
BA_BUS_ITERS = 40             # BA 2차(버스 PEQ 를 넣은 GD 항)의 DE 세대 수 — 1차 60 세대의 ⅔(돌비룸 계산 32 → 40 s)
#: 위 둘(버스 PEQ 상한 1.25 × XO + BA 2차)을 켜는 스위치 — 기본 끔(10-09). 돌비룸 실측(저장 설정, 맥)에서 켜면 늦은 에너지가
#: 11채널 모두 줄고(평균 −2.6 dB), RC 대역 1/24 GD 절대 봉우리가 42 → 31 ms, Tbl 149 Hz 580 → 214 ms 였지만, 20–200 Hz ±1 dB 가
#: L 27 → 23 · C 87 → 84 %, Tbr ±2 dB 86 → 84 %, Tfl 486 Hz GD +6 ms, 서브 합 평탄도 1.75 → 1.79 · 좌석 편차 0.71 → 0.74 dB 로
#: 나빠졌다('어느 채널도 나빠지면 안 된다' — 사용자). 상한·세대 수를 바꿔도(1.1–2.0 × XO, 20–60 세대) 늘 어느 채널은 나빠졌다
#: (XO 근처 합 널이 서브 매개변수에 민감). 켜는 것은 사용자 결정 — `SECS_SUB_BUS_FIX=1`.
SUB_BUS_FIX = __import__('os').environ.get('SECS_SUB_BUS_FIX', '0') == '1'
GD_CUT_MAX = 6                  # GD 컷(09-26 사용자: 초과위상 GD 봉우리는 높은 Q 로 살짝 파면 최소위상 GD 가 상쇄)
GD_CUT_DEPTH_DB = 3.0
GD_SPIKE_MS = 10.0              # 1/24 oct GD 가 주변(1/3 oct 중앙값)보다 이만큼 높으면 봉우리
MAIN_WEIGHT = 5.0               # 기준 위치 가중(09-26 맥 시뮬 3차: 2 → 5 에서 청취 위치 ±1 dB 평균 46 → 60 %, 확인 위치 49 → 47 %)
#: RC 크기 보정의 기준 위치 가중을 주파수마다(09-27 보정 결과 분석에서 나온 제안: 100–500 Hz 는 다점 평균, 500 Hz 위는 청취 위치 쪽).
#: 200 Hz 아래 MAIN_WEIGHT_LO, 500 Hz 위 MAIN_WEIGHT, 사이는 로그 주파수로 잇는다. 기본은 같은 값(= 전과 동일) —
#: RAW 6.1.4 시험에서 저역을 2·1.5 로 낮추면 청취 위치 ±1 dB 62 → 55·52 %, 확인 위치는 그대로라 채택 안 함.
MAIN_WEIGHT_LO = float(__import__('os').environ.get('SECS_MAIN_W_LO', str(MAIN_WEIGHT)))
PHASE_POSITIONS = __import__('os').environ.get('SECS_PHASE_POS', 'common')   # 초과위상 보정 기준: common(위치 공통) · main
LONG_MODE_RATIO = 1.3           # 긴 모드 = 봉우리 T60 중앙값의 이 배 이상(RC 부스트 금지)
LONG_MODE_MIN_S = 0.3
LONG_MODE_BOOST_DB = float(__import__('os').environ.get('SECS_LONG_BOOST_DB', '3.0'))   # 09-26: 0 dB(금지)는 돌비 ±1 dB 93 → 83 %
WINDOW_MS = 500.0               # 응답을 볼 IR 창(REW 기본에 가깝게)


# ──────────────────────────── 기본 연산 ────────────────────────────
def onset_index(ir: np.ndarray) -> int:
    a = np.abs(ir)
    peak = int(np.argmax(a))
    above = np.nonzero(a[:peak + 1] >= 0.1 * a[peak])[0]
    return int(above[0]) if above.size else peak


def windowed(ir: np.ndarray, sr: int, start: int) -> np.ndarray:
    """start 앞 2 ms 부터 WINDOW_MS 까지, 끝 20 % 반 해닝."""
    a = max(0, start - int(0.002 * sr))
    w = np.array(ir[a:a + int(WINDOW_MS / 1000 * sr)], dtype=float)
    k = int(w.size * 0.2)
    if k:
        w[-k:] *= 0.5 * (1 + np.cos(np.pi * np.arange(k) / k))
    return w, a


def spectrum(ir: np.ndarray, n: int = N_FFT) -> np.ndarray:
    return np.fft.rfft(ir, n)


def freqs(n: int = N_FFT, sr: int = SR) -> np.ndarray:
    return np.fft.rfftfreq(n, 1.0 / sr)


def frac_for(f: np.ndarray) -> np.ndarray:
    """주파수별 스무딩 폭(옥타브 분수): 200 Hz 아래 1/24, 2 kHz 위 1/6, 사이는 로그 보간."""
    lf = np.log10(np.clip(f, 1e-3, None))
    t = np.clip((lf - np.log10(200)) / (np.log10(2000) - np.log10(200)), 0, 1)
    return 24.0 * (6.0 / 24.0) ** t


def smooth_power(f: np.ndarray, power: np.ndarray, frac) -> np.ndarray:
    """선형 격자 power 를 [f/h, f·h] 평균(누적합) — frac 은 스칼라나 f 모양 배열."""
    frac = np.broadcast_to(np.asarray(frac, float), f.shape)
    h = 2.0 ** (0.5 / frac)
    c = np.concatenate([[0.0], np.cumsum(power)])
    df = f[1] - f[0]
    lo = np.clip(np.floor(f / h / df).astype(int), 0, f.size - 1)
    hi = np.clip(np.ceil(f * h / df).astype(int), 0, f.size - 1)
    hi = np.maximum(hi, lo + 1)
    return (c[hi + 1] - c[lo]) / (hi + 1 - lo)


def smooth_complex(f: np.ndarray, h: np.ndarray, frac) -> np.ndarray:
    return smooth_power(f, h.real, frac) + 1j * smooth_power(f, h.imag, frac)


def minimum_phase(mag: np.ndarray) -> np.ndarray:
    """rfft 크기 → 같은 크기의 최소위상 복소 응답(켑스트럼)."""
    n = 2 * (mag.size - 1)
    logm = np.log(np.maximum(mag, 1e-8))
    cep = np.fft.irfft(logm, n)
    fold = np.zeros(n)
    fold[0] = cep[0]
    fold[1:n // 2] = 2 * cep[1:n // 2]
    fold[n // 2] = cep[n // 2]
    return np.exp(np.fft.rfft(fold, n))


def lr4(f: np.ndarray, fc: float, kind: str) -> np.ndarray:
    s = 1j * f / fc
    bw2 = 1.0 / (s * s + np.sqrt(2) * s + 1)          # 2차 버터워스 LP
    lp = bw2 * bw2
    if kind == 'LP':
        return lp
    hp2 = (s * s) * bw2
    return hp2 * hp2


#: 서브 고정 대역 필터 종류(R27): 이름 → (모양, 차수). LR = 버터워스(차수/2) 제곱 — 'LR24' 는 XO 의 `lr4` 와 같은 식.
BAND_TYPES = {'LR24': ('LR', 4), 'LR12': ('LR', 2), 'LR48': ('LR', 8), 'BW12': ('BW', 2), 'BW24': ('BW', 4)}


def band_filter(f: np.ndarray, fc: float, kind: str, typ: str = 'LR24') -> np.ndarray:
    """아날로그 원형 HP·LP(kind 'HP'|'LP') 복소 응답 — 서브 고정 대역 필터(R27). 카밀라 BiquadCombo 와 같은 모양."""
    if typ == 'LR24':
        return lr4(f, fc, kind)
    from scipy.signal import butter

    shape, order = BAND_TYPES[typ]
    b, a = butter(order // 2 if shape == 'LR' else order, 1.0, btype='low' if kind == 'LP' else 'high', analog=True)
    s = 1j * f / fc
    h = np.polyval(b, s) / np.polyval(a, s)
    return h * h if shape == 'LR' else h


def allpass2(f: np.ndarray, f0: float, q: float) -> np.ndarray:
    s = 1j * f / f0
    return (s * s - s / q + 1) / (s * s + s / q + 1)


def peaking_response(f: np.ndarray, f0: float, q: float, gain_db: float, sr: int = SR) -> np.ndarray:
    """RBJ 피킹 바이쿼드의 복소 응답(DSP 에 그대로 올리는 형식)."""
    z = np.exp(-1j * 2 * np.pi * np.asarray(f, float) / sr)
    return _peaking_response_z(z, f0, q, gain_db, sr)


def _peaking_response_z(z, f0, q, gain_db, sr=SR):
    """고정 주파수 격자의 z 를 나눠 쓴다. 원래 RBJ 수식과 연산 순서를 유지한다."""
    a = 10 ** (gain_db / 40)
    w0 = 2 * np.pi * f0 / sr
    al = np.sin(w0) / (2 * q)
    b = np.array([1 + al * a, -2 * np.cos(w0), 1 - al * a])
    d = np.array([1 + al / a, -2 * np.cos(w0), 1 - al / a])
    return (b[0] + b[1] * z + b[2] * z * z) / (d[0] + d[1] * z + d[2] * z * z)


def peq_response(f: np.ndarray, bands) -> np.ndarray:
    out = np.ones(np.asarray(f).shape, complex)
    z = np.exp(-1j * 2 * np.pi * np.asarray(f, float) / SR)
    for f0, q, g in bands:
        out = out * _peaking_response_z(z, f0, q, g)
    return out


def fit_peq(g: np.ndarray, corr_db: np.ndarray, n: int, lo: float = 20.0, hi: float = 500.0,
            boost: float = MAX_BOOST_DB, ring_ms: float | None = None, f_max: float | None = None) -> list:
    """보정 곡선(dB, 로그 격자 g)의 lo–hi 를 피킹 n 개로(탐욕 배치 → 최소제곱 다듬기). ring_ms 를 주면 밴드마다
    울림 시간 상수(극 Q/(π f))가 그 안에 들게 한다(`BOOST_RING_MS`). f_max 를 주면 다듬기에서 밴드 중심이 그 위로 못 간다
    (None = 전과 같은 max(2 kHz, hi × 1.2) — 서브 버스 PEQ 만 준다, 10-09)."""
    from scipy.optimize import least_squares

    region = (g >= lo) & (g <= hi)
    bands = []
    for _ in range(n):
        cur = 20 * np.log10(np.abs(peq_response(g, bands)) + 1e-12)
        r = np.where(region, corr_db - cur, 0.0)
        k = int(np.argmax(np.abs(r)))
        if abs(r[k]) < 0.5:
            break
        gain = float(np.clip(r[k], -MAX_CUT_DB, boost))
        half = np.abs(r) >= abs(r[k]) / 2
        i0 = k
        while i0 > 0 and half[i0 - 1] and np.sign(r[i0 - 1]) == np.sign(r[k]):
            i0 -= 1
        i1 = k
        while i1 < g.size - 1 and half[i1 + 1] and np.sign(r[i1 + 1]) == np.sign(r[k]):
            i1 += 1
        q = float(np.clip(g[k] / max(g[i1] - g[i0], g[k] * 0.02), 0.5, 15.0))
        if ring_ms is not None:
            q = max(min(q, _ring_q_cap(float(g[k]), gain, ring_ms)), 0.31)
        bands.append((float(g[k]), q, gain))
    if not bands:
        return []
    x0 = np.array([v for b in bands for v in (np.log(b[0]), np.log(b[1]), b[2])])
    lb = np.array([v for _ in bands for v in (np.log(15.0), np.log(0.3), -MAX_CUT_DB)])
    fmax = max(2000.0, hi * 1.2) if f_max is None else float(f_max)
    ub = np.array([v for _ in bands for v in (np.log(fmax), np.log(20.0), boost)])
    if ring_ms is not None:
        lb, ub = _ring_boxes(bands, lb, ub, ring_ms)
    x0 = np.clip(x0, lb + 1e-6, ub - 1e-6)
    wreg = np.where(region, 1.0, 0.15)

    # 09-26 프로파일: 수치 미분이 한 변수만 바꾸는데 모든 밴드를 다시 계산했다.
    # 맞춤 한 번 안에서만 캐시한다(격자/샘플레이트 고정). 크기는 밴드 수에 비례해 제한한다.
    from functools import lru_cache

    z = np.exp(-1j * 2 * np.pi * np.asarray(g, float) / SR)

    @lru_cache(maxsize=4 * len(bands))
    def band_response(log_f, log_q, gain):
        return _peaking_response_z(z, np.exp(log_f), np.exp(log_q), gain)

    def res(x):
        response = np.ones(np.asarray(g).shape, complex)
        for i in range(len(bands)):
            response = response * band_response(x[3 * i], x[3 * i + 1], x[3 * i + 2])
        return wreg * (20 * np.log10(np.abs(response) + 1e-12) - corr_db)

    # 울림 상자를 걸면 trf 가 경계 근처에서 400 번을 다 쓰고도 못 멈췄다(10-05: 맞춤 2.1 s, 150 번에서 끊으면 오차 +3–8 %·0.6 s,
    # 상한 없는 맞춤도 60–127 번이면 끝난다).
    sol = least_squares(res, x0, bounds=(lb, ub), max_nfev=400 if ring_ms is None else 150)
    x = sol.x
    out = []
    for i in range(len(bands)):
        f0, q, gain = float(np.exp(x[3 * i])), float(np.exp(x[3 * i + 1])), float(x[3 * i + 2])
        if ring_ms is not None:
            q = min(q, _ring_q_cap(f0, gain, ring_ms))
        out.append((f0, q, gain))
    return out


def _ring_q_cap(f0: float, gain_db: float, ring_ms: float) -> float:
    """밴드의 Q 상한 — 극 Q(= Q·A)의 울림 시간 상수 Q·A/(π f0) 가 ring_ms 를 넘지 않게(깊은 컷일수록 좁아도 된다)."""
    return float(np.pi * f0 * ring_ms / 1000.0 / 10 ** (gain_db / 40))


def _ring_boxes(bands, lb, ub, ring_ms: float):
    """최소제곱 상자를 밴드마다 좁힌다 — 주파수는 탐욕 배치 자리 ±20 %, 게인 부호는 그대로(부스트 0..지금 + 3 dB, 컷은 최대
    컷..지금의 절반 깊이 — 0 dB 까지 열면 Q 가 π·f·τ 로 묶여 모드를 깎는 깊은 노치를 못 만든다), Q 는 그 상자 안 가장 얕은
    게인·가장 낮은 주파수에서도 울림 상한 안. 벌점 항(경계 밖 비용)이나 두 번째 맞춤은 계산을 2–4배 느리게 했다(10-05)."""
    lb, ub = lb.copy(), ub.copy()
    for i, (f0, _q, gain) in enumerate(bands):
        lb[3 * i] = max(lb[3 * i], np.log(f0 / 1.2))
        ub[3 * i] = max(min(ub[3 * i], np.log(f0 * 1.2)), lb[3 * i] + 1e-3)
        if gain > 0:
            lb[3 * i + 2], ub[3 * i + 2] = 0.0, min(gain + 3.0, ub[3 * i + 2])
        else:
            ub[3 * i + 2] = min(gain / 2.0, -0.25)
        ub[3 * i + 1] = np.log(max(min(np.exp(ub[3 * i + 1]), _ring_q_cap(f0 / 1.2, ub[3 * i + 2], ring_ms)), 0.31))
    return lb, ub


def gd_curve(H: np.ndarray, f: np.ndarray, g: np.ndarray, frac: float = 24.0) -> np.ndarray:
    """복소 응답의 GD(ms, 로그 격자 g) — IR 피크 지연을 뺀 뒤 1/frac oct 복소 스무딩(REW 식)."""
    t = np.argmax(np.abs(np.fft.irfft(H))) / (2 * f[-1])
    Hs = smooth_complex(f, H * np.exp(2j * np.pi * f * t), frac)
    c = np.interp(g, f, Hs.real) + 1j * np.interp(g, f, Hs.imag)
    return -np.gradient(np.unwrap(np.angle(c)), 2 * np.pi * g) * 1000.0


def fit_gd_cuts(f: np.ndarray, H: np.ndarray, lo: float, hi: float, n_max: int = GD_CUT_MAX,
                depth: float = GD_CUT_DEPTH_DB) -> list:
    """RC 대역의 1/24 oct GD 봉우리마다 높은 Q 의 얕은 컷(최소위상 — 중심 GD 가 음수라 봉우리를 상쇄).
    봉우리 하나씩: f0·Q·게인(−depth..0) 을 봉우리 둘레 ±1/3 oct 의 GD 평탄 + 크기 변화 벌점으로 맞춘다."""
    from scipy.optimize import least_squares

    g = np.geomspace(lo, hi, 1500)
    cuts = []
    cur = H.copy()
    for _ in range(n_max):
        gd = gd_curve(cur, f, g)
        base = np.array([np.median(gd[max(0, i - 40):i + 40]) for i in range(g.size)])
        exc = gd - base
        k = int(np.argmax(exc))
        if exc[k] < GD_SPIKE_MS:
            break
        f0 = float(g[k])
        win = (g >= f0 / 2 ** (1 / 3)) & (g <= f0 * 2 ** (1 / 3))
        gw = g[win]
        mag0 = 20 * np.log10(np.abs(np.interp(gw, f, np.abs(cur))) + 1e-12)

        def res(x):
            P = peaking_response(f, np.exp(x[0]), np.exp(x[1]), x[2])
            gd2 = gd_curve(cur * P, f, gw)
            b = np.interp(gw, g, base)
            dm = 20 * np.log10(np.abs(np.interp(gw, f, np.abs(cur * P))) + 1e-12) - mag0
            return np.concatenate([(gd2 - b) / 5.0, dm / 1.0])

        x0 = np.array([np.log(f0), np.log(8.0), -1.0])
        lb = np.array([np.log(f0 / 1.1), np.log(2.0), -depth])
        ub = np.array([np.log(f0 * 1.1), np.log(40.0), 0.0])
        sol = least_squares(res, x0, bounds=(lb, ub), max_nfev=60)
        band = (float(np.exp(sol.x[0])), float(np.exp(sol.x[1])), float(sol.x[2]))
        before = float(np.sum(res(np.array([np.log(f0), np.log(8.0), 0.0])) ** 2))
        if float(np.sum(sol.fun ** 2)) >= before * 0.95 or band[2] > -0.1:
            break                                   # 컷이 봉우리를 줄이지 못하면 멈춘다
        cuts.append(band)
        cur = cur * peaking_response(f, *band)
    return cuts


def _term(H: np.ndarray, f: np.ndarray, g: float, d: float, pol: int, aps) -> np.ndarray:
    """서브 하나의 출력 항(게인 dB · 지연 ms · 극성 · 올패스) — `CorrectionEngine._sub_sum` 의 한 항과 같은 식."""
    out = H * (10 ** (g / 20)) * pol * np.exp(-2j * np.pi * f * d / 1000)
    for f0, q in aps:
        out = out * allpass2(f, f0, q)
    return out


# ──────────────────────────── 결과 ────────────────────────────
@dataclass
class SubAlign:
    role: str
    gain_db: float = 0.0
    delay_ms: float = 0.0          # 도착 정렬 딜레이 포함 최종 출력 딜레이
    polarity: int = 1
    allpass: list = field(default_factory=list)   # [(f, q), (f, q)]
    mode: str = 'auto'             # 'auto' · 'fixed'(넣은 값 그대로) · 'bundle:A'(묶음 A) — R27
    hp_hz: float = 0.0             # 고정 대역 필터(R27 10-05 추가): HP·LP 주파수(0 = 없음)·종류(`BAND_TYPES`) · 고정 PEQ [(f, q, dB)]
    lp_hz: float = 0.0
    peq: list = field(default_factory=list)
    hp_type: str = 'LR24'
    lp_type: str = 'LR24'


@dataclass
class ChannelOut:
    role: str
    fir: np.ndarray                # 입력 쪽(BM 앞) 보정 FIR, SR
    gain_db: float
    delay_ms: float                # 메인 출력 딜레이(도착 정렬)
    xo_hz: float | None = None     # 베이스 매니지먼트 XO(HP 메인 / LP 서브 센드)
    peq: list = field(default_factory=list)   # 입력 쪽 PEQ [(f, q, gain_db)] — FIR 앞


@dataclass
class CorrectionResult:
    channels: dict
    subs: dict
    xo: dict
    sub_bus_delay_ms: float
    sub_bus_polarity: int
    lift_ms: float
    f: np.ndarray
    before: dict                   # role → 기준 위치 복소 응답(메인 단독, 도착 정렬 전 원 측정)
    after: dict                    # role → 기준 위치 최종 복소 응답(보정 필터 · 메인+서브 합)
    combined: dict                 # role → 기준 위치 보정 전 합(메인 HP + 서브 LP)
    sub_sum: np.ndarray | None     # 기준 위치 서브 합(정렬 뒤, LP 전)
    target_db: np.ndarray
    notes: list = field(default_factory=list)


# ──────────────────────────── 엔진 ────────────────────────────
class CorrectionEngine:
    def __init__(self, irs: dict, mains: list[str], subs: list[str], groups: dict[str, list[str]],
                 target_db_fn, xo: dict[str, float] | None = None, main_position: int = 1,
                 taps: int = TAPS, lift_ms: float = LIFT_MS, max_boost_db: float = MAX_BOOST_DB,
                 peq_bands: int | None = None, max_latency_ms: float | None = None, xo_stage: bool = True,
                 rc_range: tuple = (20.0, None), sub_allpass: str = 'none', room_volume_m3: float | None = None,
                 gd_cuts: bool = False, phase_frac: float = 6.0, sub_manual: dict | None = None,
                 sub_main_delay_ms: float = 0.0, bundle_gain: str = 'auto', bundle_layouts: dict | None = None):
        self.irs, self.mains, self.subs, self.groups = irs, list(mains), list(subs), groups
        # 서브 수동 설정(R27): 역할 → {'mode', 'bundle', 'gain_db', 'delay_ms', 'polarity', 'allpass'}. 없는 서브는 자동.
        # sub_main_delay_ms = 고정 서브가 있을 때 메인 출력에 더하는 지연(고정 값은 DSP 출력 딜레이 그대로라 엔진이 정하지 않는다).
        # bundle_gain = 'auto'(10-05 사용자 결정: 묶음 = 공통 게인·지연·극성, 안의 상대값은 고정) | 'fixed'(묶음 게인도 사람 값 —
        # 처음 요구 대장 글자, 돌비에서 서브 합 +10.6 dB 를 버스 PEQ 가 −21 dB 밴드로 먹었다. 엔진 손잡이로만 남김).
        self.sub_manual = {r: dict(v) for r, v in (sub_manual or {}).items() if r in self.subs and isinstance(v, dict)}
        self.sub_main_delay_ms = float(sub_main_delay_ms or 0.0)
        self.bundle_gain = bundle_gain
        # 서브마다 고정 대역 필터·PEQ(10-05 R27 추가 — 사용자: INFRA 는 로우패스 25 Hz LR 24 쯤으로 최저역만, DBA L·R 이 그 위):
        # 역할 → {'hp', 'hp_type', 'lp', 'lp_type', 'peq'}. 서브 출력에 늘 걸리고 서브 버스 신호와 MIMO 서포트 신호가 모두 지난다
        # (카밀라는 MIMO 갈래에도 같은 필터를 넣는다) → 스피커 응답의 일부로 본다.
        self.sub_filters = {}
        for r, m in self.sub_manual.items():
            hp, lp = float(m.get('hp_hz') or 0.0), float(m.get('lp_hz') or 0.0)
            peq = [(float(a), float(b), float(c)) for a, b, c in (m.get('peq') or []) if float(c) != 0.0]
            if hp > 0 or lp > 0 or peq:
                self.sub_filters[r] = {'hp': hp, 'hp_type': m.get('hp_type') if m.get('hp_type') in BAND_TYPES else 'LR24',
                                       'lp': lp, 'lp_type': m.get('lp_type') if m.get('lp_type') in BAND_TYPES else 'LR24',
                                       'peq': peq}
        # 묶음 레이아웃(R30): 이름 → {'kind': 'dba'|'multi', 'rear_trim_ms'}. 묶음 서브의 고정 출력 필터는 묶음 첫 서브(서브 순서)
        # 것으로 맞춘다 — 사용자 결정 "묶음 안 서브들은 필터 완전히 동일"(화면도 한 서브를 고치면 묶음 전부에 옮겨 적는다).
        self.layouts: dict[str, dict] = {}
        for name, v in (bundle_layouts or {}).items():
            members = [r for r in self.subs if (self.sub_manual.get(r) or {}).get('mode') == 'bundle'
                       and str(self.sub_manual[r].get('bundle') or 'A') == str(name)]
            if isinstance(v, dict) and v.get('kind') in BUNDLE_LAYOUTS and members:
                self.layouts[str(name)] = {'kind': v['kind'], 'rear_trim_ms': float(v.get('rear_trim_ms') or 0.0),
                                           'members': members}
                lead = self.sub_filters.get(members[0])
                for r in members[1:]:
                    if lead is None:
                        self.sub_filters.pop(r, None)
                    else:
                        self.sub_filters[r] = {k: (list(x) if isinstance(x, list) else x) for k, x in lead.items()}
        self.layout_rel: dict[str, dict] = {}      # 묶음 → 역할 → (상대 게인, 상대 지연, 상대 극성, []) — 1단 결과
        self.layout_info: dict[str, dict] = {}     # 묶음 → 결과 요약(화면·노트·내보내기)
        self.target_db_fn, self.main_position = target_db_fn, main_position
        self.xo = dict(xo or {})
        self.taps, self.lift_ms, self.max_boost_db = int(taps), float(lift_ms), float(max_boost_db)
        cap_ms = 0.7 * max(self.taps, 0) / SR * 1000.0          # 여유 지연은 FIR 길이 안에 들어가야 한다(1,024탭 = 21.3 ms)
        self.lift_note = ''
        if self.lift_ms > cap_ms:
            self.lift_note = f'여유 지연 {self.lift_ms:.0f} ms 가 FIR {self.taps}탭 길이를 넘어 {cap_ms:.1f} ms 로 줄임'
            if max_latency_ms is not None:
                max_latency_ms = float(max_latency_ms) - (self.lift_ms - cap_ms)
            self.lift_ms = cap_ms
        # 전체 지연 예산(09-25 사용자: 20 ms 이내) = FIR lift + 서브를 앞당기느라 메인을 늦추는 양
        self.max_latency_ms = float(max_latency_ms) if max_latency_ms is not None else self.lift_ms + 15.0
        # RC 보정 범위(밖은 반 옥타브에 걸쳐 0 dB). 상한 None = 자동(슈뢰더 주파수, 계산 못 하거나 500 Hz 넘으면 500 Hz,
        # 09-25 사용자). 값을 주면 그 값(수동으로 올릴 수 있다).
        self.rc_lo = float(rc_range[0])
        self.rc_hi_manual = None if rc_range[1] is None else float(rc_range[1])
        self.rc_hi = self.rc_hi_manual or RC_AUTO_MAX_HZ
        self.room_volume_m3 = room_volume_m3
        self.gd_cuts = bool(gd_cuts)
        self.phase_frac = float(phase_frac)       # 초과위상을 볼 스무딩(옥타브 분수) — 클수록 좁은 GD 봉우리까지 따라간다
        self.sub_allpass = sub_allpass            # BA 올패스: 'full'(Q ≤ 3) · 'lowq'(Q ≤ 1) · 'none'
        self.xo_stage = bool(xo_stage)        # 1차: 스피커별 음향 XO 타겟 보정(출력 쪽 PEQ, 09-25 사용자 제안)
        self.top_peq: dict[str, list] = {}
        self.chain: dict[str, dict] = {}          # MIMO 가 긴 창으로 다시 합성할 때 쓰는 채널 체인
        self.sub_bus_peq: list = []
        # 입력 PEQ 는 탭 수와 상관없이 기본으로 켠다 — FIR 뒤 2차 맞춤이 저역 오차를 줄인다(09-25 탭 비교: 8,192탭
        # PEQ 끔 86 % → 켬 92 %, 32,768탭도 같음). 0 을 주면 끈다.
        self.peq_bands = int(PEQ_BANDS if peq_bands is None else peq_bands)
        self.f = freqs()
        self.positions = sorted(irs)
        self.notes: list[str] = [self.lift_note] if self.lift_note else []
        self.phase_strength: dict[str, float] = {}
        self.peq: dict[str, list] = {}
        self.phase_band: dict[str, float | None] = {}
        self.design_db: dict[str, np.ndarray] = {}      # role → 크기 설계 곡선(dB, 선형 격자 — 필터가 따라갈 목표, 진단용)

    # ── 1. 측정 준비: 도착 시각·창 씌운 스펙트럼(모든 위치가 기준 위치 도착 기준의 같은 시간축) ──
    def _prepare(self):
        arrivals = {}
        main = self.irs[self.main_position]
        for role, (ir, sr, *rest) in main.items():
            t0 = float(rest[0]) if rest else 0.0
            if int(sr) != SR:
                raise ValueError(f'{role}: 샘플레이트 {sr} (48 kHz 만 지원)')
            arrivals[role] = t0 + onset_index(np.asarray(ir, float)) / SR
        self.arrival = arrivals
        latest = max(arrivals[r] for r in self.mains + self.subs if r in arrivals)
        self.align_delay_ms = {r: 1000.0 * (latest - a) for r, a in arrivals.items()}
        # 위치마다 스펙트럼: 창 시작 = 그 위치에서 그 role 의 도착, 그 뒤 "도착 시각" 위상을 되돌려
        # 모든 스펙트럼을 '기준 위치 도착 = 0' 시간축에 둔다(합산할 때 실제 상대 지연이 살아 있게).
        self.H: dict[int, dict[str, np.ndarray]] = {}
        base = min(arrivals.values())
        for pos in self.positions:
            self.H[pos] = {}
            for role, (ir, sr, *rest) in self.irs[pos].items():
                ir = np.asarray(ir, float)
                t0 = float(rest[0]) if rest else 0.0
                w, a = windowed(ir, SR, onset_index(ir))
                start_t = t0 + a / SR - base
                self.H[pos][role] = spectrum(w) * np.exp(-2j * np.pi * self.f * start_t)
        self._apply_sub_filters(self.H)
        self.H0 = {p: dict(d) for p, d in self.H.items()}      # 원 측정(1차 보정 전, 서브는 고정 출력 필터까지) — '보정 전' 표시용

    def sub_filter_response(self, role: str):
        """서브의 고정 대역 필터·PEQ 복소 응답(선형 격자) — 없으면 None."""
        if role not in self.sub_filters:
            return None
        sf = self.sub_filters[role]
        out = np.ones(self.f.size, complex)
        if sf['hp'] > 0:
            out = out * band_filter(self.f, sf['hp'], 'HP', sf['hp_type'])
        if sf['lp'] > 0:
            out = out * band_filter(self.f, sf['lp'], 'LP', sf['lp_type'])
        if sf['peq']:
            out = out * peq_response(self.f, sf['peq'])
        return out

    def sub_passband(self, role: str) -> tuple | None:
        """서브 고정 대역 필터가 정하는 통과 대역 (HP Hz 또는 0, LP Hz 또는 0) — 대역 필터가 없으면 None(MIMO 서포트 대역 판정)."""
        sf = self.sub_filters.get(role)
        if not sf or not (sf['hp'] or sf['lp']):
            return None
        return sf['hp'], sf['lp']

    def _apply_sub_filters(self, H: dict) -> dict:
        """스펙트럼 모음(위치 → 역할)의 서브에 고정 출력 필터를 곱한다(제자리). 필터가 없으면 그대로(같은 값)."""
        for role in self.sub_filters:
            F = self.sub_filter_response(role)
            for pos in H:
                if role in H[pos]:
                    H[pos][role] = H[pos][role] * F
        return H

    def long_spectra(self, window_ms: float = 2000.0) -> dict:
        """MIMO 용: _prepare 와 같은 시간축에 긴 창(모드 감쇠가 잘리지 않게) 스펙트럼."""
        base = min(self.arrival.values())
        out = {}
        for pos in self.positions:
            out[pos] = {}
            for role, (ir, sr, *rest) in self.irs[pos].items():
                ir = np.asarray(ir, float)
                t0 = float(rest[0]) if rest else 0.0
                a = max(0, onset_index(ir) - int(0.002 * SR))
                w = np.array(ir[a:a + int(window_ms / 1000 * SR)], dtype=float)
                k = int(w.size * 0.1)
                w[-k:] *= 0.5 * (1 + np.cos(np.pi * np.arange(k) / k))
                out[pos][role] = spectrum(w) * np.exp(-2j * np.pi * self.f * (t0 + a / SR - base))
        return self._apply_sub_filters(out)

    def stage_responses(self, role: str, H: dict, pos: int) -> dict:
        """입력 role 의 위치 pos 단계별 복소 응답(스펙트럼 모음 H — 긴 창이면 감쇠까지):
        top_xo = 탑(게인·딜레이·1차 PEQ·전기 HP), sub_xo = 서브 버스(BA 정렬·버스 PEQ·전기 LP·버스 지연·극성),
        combined = 둘의 합(2차 보정 전), final = 입력 보정 필터(PEQ + FIR)까지."""
        c, st = self.chain[role], self.sub_state
        top = H[pos][role] * self._delay(self.align_delay_ms[role]) * 10 ** (c['gain_db'] / 20)
        if c['top_P'] is not None:
            top = top * c['top_P']
        out = {}
        if c['xo'] and self.subs:
            saved, self.H = self.H, H
            ssum = self._sub_sum(st['params'], np.arange(self.f.size), [pos])[pos]   # 최종 게인·지연·극성·올패스
            self.H = saved
            if st['bus_P'] is not None:
                ssum = ssum * st['bus_P']
            out['top_xo'] = top * lr4(self.f, c['xo'], 'HP')
            out['sub_xo'] = ssum * lr4(self.f, c['xo'], 'LP')
            out['combined'] = out['top_xo'] + out['sub_xo']
        else:
            out['top_xo'] = out['combined'] = top
        out['final'] = out['combined'] * c['F']
        return out

    def input_response(self, role: str, H: dict, pos: int) -> np.ndarray:
        return self.stage_responses(role, H, pos)['final']

    def _weights(self) -> dict[int, float]:
        return {p: (MAIN_WEIGHT if p == self.main_position else 1.0) for p in self.positions}

    def _freq_weights(self) -> dict:
        """위치 → 주파수별 가중(선형 격자 배열) — 기준 위치만 200 Hz 아래 MAIN_WEIGHT_LO, 500 Hz 위 MAIN_WEIGHT."""
        t = np.clip(np.log2(np.maximum(self.f, 1.0) / 200.0) / np.log2(500.0 / 200.0), 0.0, 1.0)
        main = MAIN_WEIGHT_LO + (MAIN_WEIGHT - MAIN_WEIGHT_LO) * t
        return {p: (main if p == self.main_position else np.ones(self.f.size)) for p in self.positions}

    def _delay(self, ms: float) -> np.ndarray:
        return np.exp(-2j * np.pi * self.f * ms / 1000.0)

    # ── 2. 레벨 ──
    def _band_level(self, role: str, lo: float, hi: float) -> float:
        m = (self.f >= lo) & (self.f <= hi)
        w = self._weights()
        p = sum(w[pos] * np.abs(self.H[pos][role][m]) ** 2 for pos in self.positions if role in self.H[pos])
        p /= sum(w[pos] for pos in self.positions if role in self.H[pos])
        tgt = self.target_db_fn(self.f[m])
        return float(np.median(10 * np.log10(p + 1e-30) - tgt))

    # ── 3. BA: 서브끼리 결합 ──
    def _sub_sum(self, params, grid_idx, positions):
        out = {}
        for pos in positions:
            s = 0
            for k, role in enumerate(self.subs):
                g, d, pol, aps = params[k]
                resp = self.H[pos][role][grid_idx] * (10 ** (g / 20)) * pol * np.exp(-2j * np.pi * self.f[grid_idx] * d / 1000)
                for f0, q in aps:
                    resp = resp * allpass2(self.f[grid_idx], f0, q)
                s = s + resp
            out[pos] = s
        return out

    def two_way_split(self) -> tuple[list, list, float] | None:
        """2-way 서브(10-09 사용자: 인프라 서브) — 고정 LP 만 있는 서브(저역)와, 같은 주파수의 고정 HP 를 가진 나머지 서브(고역).
        (저역 역할들, 고역 역할들, XO Hz) 또는 None."""
        lows = [r for r in self.subs if (sf := self.sub_filters.get(r)) and sf['lp'] > 0 and not sf['hp']]
        highs = [r for r in self.subs if r not in lows]
        if not lows or not highs:
            return None
        xo = float(self.sub_filters[lows[0]]['lp'])
        if any(abs(self.sub_filters.get(r, {}).get('hp', 0) - xo) > 0.5 for r in highs):
            return None
        return lows, highs, xo

    def _match_two_way(self, params, manual):
        """2-way 서브의 대역 레벨 맞춤(10-09 사용자: SW1·SW2 묶음은 둘이 합쳐 +6 dB 인데 인프라 SW3 는 한 대라 인프라
        대역이 낮게 남는다). 정렬 뒤 서브 합에서 XO 아래(저역)와 위(고역) 의 타겟 대비 레벨(청취 위치, 1/3 oct 중앙값)을
        재고, 저역 서브 게인에 그 차이를 더한다(올림은 `infra_boost_max_db` 까지, 고정 게인 서브는 건드리지 않는다)."""
        split = self.two_way_split()
        if split is None:
            return params
        lows, highs, xo = split
        fixed = set((manual or {}).get('fixed', []))
        movable = [r for r in lows if r not in fixed]
        if not movable:
            return params
        params = [list(p) for p in params]
        idx = np.arange(self.f.size)
        lo_band = (self.f >= max(12.0, xo * 0.55)) & (self.f <= xo * 0.85)
        hi_band = (self.f >= xo * 1.3) & (self.f <= min(80.0, xo * 3.2))
        if not lo_band.any() or not hi_band.any():
            return params
        tgt = self.target_db_fn(np.maximum(self.f, 1.0))
        cap_up = float(getattr(self, 'infra_boost_max_db', 24.0))
        total = 0.0
        for _ in range(4):
            H = self._sub_sum([tuple(p) for p in params], idx, [self.main_position])[self.main_position]
            db = 10 * np.log10(smooth_power(self.f, np.abs(H) ** 2, 3.0) + 1e-30) - tgt
            diff = float(np.median(db[hi_band]) - np.median(db[lo_band]))
            step = float(np.clip(diff, -24.0 - total, cap_up - total))
            if abs(step) < 0.1:
                break
            for k, r in enumerate(self.subs):
                if r in movable:
                    params[k][0] += step
            total += step
        if abs(total) >= 0.1:
            self.notes.append(f"2-way 서브 레벨 맞춤: {'·'.join(movable)} {total:+.1f} dB (XO {xo:g} Hz)")
        return [tuple(p) for p in params]

    def _ba_setup(self, hi_hz: float) -> dict:
        """BA 비용의 재료(자동·수동 경로가 같이 쓴다): 로그 격자 · 모든 서브를 잰 위치 · 위치 가중 · 늦은 에너지 잣대와
        결합 전(같은 레벨 단순 합, 도착 정렬) 늦은 에너지."""
        base = {r: self.align_delay_ms[r] for r in self.subs}
        idx = np.nonzero((self.f >= 20) & (self.f <= hi_hz))[0]
        grid = np.unique(np.round(np.geomspace(idx[0], idx[-1], 160)).astype(int))
        positions = [p for p in self.positions if all(r in self.H[p] for r in self.subs)]
        # 울림(09-25 사용자: XO 타겟 서브의 디케이가 40 Hz 아래에서 뭉친다) — 게인을 벌리면 마주 보는 서브끼리의
        # 모드 상쇄가 깨져 22–40 Hz 가 97 → 403 ms 울렸다. 선형 격자로 시간 응답을 만들어 늦은 에너지를 본다.
        lin = np.arange(int(np.searchsorted(self.f, 200.0)))
        band = np.clip((self.f[lin] - 15.0) / 5.0, 0, 1) * np.clip((hi_hz * 1.3 - self.f[lin]) / (hi_hz * 0.3), 0, 1)
        fs_eff = 2.0 * self.f[lin[-1]]

        def late_db(spec):
            y = np.fft.irfft(spec * band, 2 * (lin.size - 1))
            k = int(np.argmax(np.abs(y) > 0.1 * np.max(np.abs(y))))
            early = np.sum(y[k:k + int(0.05 * fs_eff)] ** 2)
            late = np.sum(y[k + int(0.1 * fs_eff):k + int(0.38 * fs_eff)] ** 2)
            return 10 * np.log10(late / (early + 1e-30) + 1e-30)

        raw = [(0.0, base[r], 1, []) for r in self.subs]
        raw_late = {pos: late_db(self._sub_sum(raw, lin, [pos])[pos]) for pos in positions}
        return {'base': base, 'grid': grid, 'positions': positions, 'w': self._weights(), 'fgrid': self.f[grid],
                'lin': lin, 'late_db': late_db, 'raw_late': raw_late}

    def _ba_cost(self, p, s: dict, indiv: dict | None = None, bus_P: np.ndarray | None = None) -> float:
        """서브 매개변수 p(서브 순서 [(게인, 지연, 극성, 올패스)])의 결합 비용. indiv = 위치 → 개별 에너지 합(상쇄 벌점 기준) —
        None 이면 p 의 게인을 건 값(R27: 게인이 묶인 서브가 있으면 원 레벨 기준 벌점이 상수처럼 커져 자동 서브 게인을 위로 민다).
        bus_P(선형 격자 복소 응답)를 주면 GD 항만 서브 버스 PEQ 를 건 합으로 본다(10-09 `_ba_bus_pass`)."""
        grid, positions, w, fgrid, late_db = s['grid'], s['positions'], s['w'], s['fgrid'], s['late_db']
        if indiv is None:
            indiv = {pos: sum(np.abs(self.H[pos][r][grid]) ** 2 * 10 ** (g / 10) for r, (g, _d, _p, _a) in zip(self.subs, p))
                     for pos in positions}
        sums = self._sub_sum(p, grid, positions)
        total, levels = 0.0, []
        for pos in positions:
            db = 20 * np.log10(np.abs(sums[pos]) + 1e-20)
            # 모양: 로그 주파수 2차 추세를 뺀 잔차(평탄) + 상쇄 벌점(개별 에너지 합 대비 손실)
            lf = np.log(fgrid)
            coef = np.polyfit(lf, db, 2)
            rough = np.mean((db - np.polyval(coef, lf)) ** 2)
            loss = np.mean(np.clip(10 * np.log10(indiv[pos] + 1e-30) - db - 3.0, 0, None) ** 2)
            # GD 평탄(09-25): 서브 합의 비최소위상 성분은 짧은 FIR(≤ 20 ms)로 못 되돌린다 → 결합 단계에서 막는다
            ph = np.unwrap(np.angle(sums[pos] if bus_P is None else sums[pos] * bus_P[grid]))
            gd = -np.gradient(ph, 2 * np.pi * fgrid) * 1000.0
            gdv = np.mean((gd - np.median(gd)) ** 2)
            lsum = self._sub_sum(p, s['lin'], [pos])[pos]
            worse = max(0.0, late_db(lsum) - s['raw_late'][pos])     # 결합 전(같은 레벨 단순 합)보다 더 울리면 벌점
            eff = float(np.mean(db - 10 * np.log10(indiv[pos] + 1e-30)))     # 합산 효율(dB) — 클수록 좋다
            total += w[pos] * (rough + 0.5 * loss + GD_WEIGHT * gdv + DECAY_WEIGHT * worse ** 2 - BA_EFF_WEIGHT * eff)
            levels.append(db)
        total /= sum(w[pos] for pos in positions)
        if len(levels) > 1:
            total += 0.5 * float(np.mean(np.var(np.array(levels), axis=0)))
        return total

    def _align_subs(self, hi_hz: float):
        from scipy.optimize import differential_evolution

        base = {r: self.align_delay_ms[r] for r in self.subs}
        if len(self.subs) == 1:
            return [(0.0, base[self.subs[0]], 1, [])]
        s = self._ba_setup(hi_hz)
        n = len(self.subs)
        # 변수: 서브마다 게인(-8..0 dB), 미세 지연(±3 ms), 극성(0..1 → 반올림), 올패스 2개(f 20..150, q 0.3..3)
        bounds = []
        for k in range(n):
            qmax = 1.0 if self.sub_allpass == 'lowq' else 3.0
            bounds += [(-8, 0), (-BA_DELAY_MS, BA_DELAY_MS), (0, 1), (20, 150), (0.3, qmax), (20, 150), (0.3, qmax)]

        def unpack(x):
            out = []
            for k in range(n):
                g, d, pol, f1, q1, f2, q2 = x[7 * k:7 * k + 7]
                aps = [] if self.sub_allpass == 'none' else [(f1, q1), (f2, q2)]
                out.append((g, base[self.subs[k]] + d, 1 if pol < 0.5 else -1, aps))
            return out

        grid = s['grid']
        indiv = {pos: sum(np.abs(self.H[pos][r][grid]) ** 2 for r in self.subs) for pos in s['positions']}
        res = differential_evolution(lambda x: self._ba_cost(unpack(x), s, indiv), bounds, maxiter=60, popsize=12,
                                     tol=1e-6, seed=1, polish=False)
        self.notes.append(f'BA 비용 {res.fun:.2f}')
        return unpack(self._ba_bus_pass(unpack, bounds, s, indiv, res.x))

    # ── 3‴. BA 가 서브 버스 PEQ 를 알게(10-09) ──
    def _bus_peq_of(self, params) -> np.ndarray | None:
        """서브 매개변수 params 로 `compute()` 가 걸 서브 버스 PEQ 의 응답(선형 격자) — 같은 버스 게인·레벨·대역(`_bus_ctx`).
        버스 PEQ 를 안 거는 설정(XO 단계 끔·XO 없음)이면 None."""
        ctx = getattr(self, '_bus_ctx', None)
        if not ctx:
            return None
        full = np.arange(self.f.size)
        by_pos = {pos: self._sub_sum(params, full, [pos])[pos] for pos in self.positions
                  if all(r in self.H[pos] for r in self.subs)}
        if self.main_position not in by_pos:
            return None
        if not ctx['anchored']:
            m = (self.f >= 30) & (self.f <= 80)
            lvl = float(np.median(20 * np.log10(np.abs(by_pos[self.main_position][m]) + 1e-20) - self.target_db_fn(self.f[m])))
            gain = 10 ** ((ctx['ref'] - lvl) / 20)
            by_pos = {p: v * gain for p, v in by_pos.items()}
        peq = self._bus_peq_fit(by_pos)
        return peq_response(self.f, peq) if peq else None

    def _bus_peq_fit(self, by_pos: dict) -> list:
        """서브 버스 PEQ: 버스 게인을 건 서브 합(위치별)을 20 Hz – `BUS_PEQ_XO_RATIO` × 가장 높은 메인 XO 에서 타겟에."""
        if not SUB_BUS_FIX:                       # 10-09 전 그대로: 20 Hz – 메인 XO 평균 × 2, 밴드 중심 제한 없음
            return self._xo_target_peq(by_pos, self._bus_ctx['ref'], 20.0, 2.0 * self._bus_ctx['xo_mean'], 10,
                                       min(6.0, self.max_boost_db))
        hi = self._bus_ctx['hi']
        return self._xo_target_peq(by_pos, self._bus_ctx['ref'], 20.0, hi, 10, min(6.0, self.max_boost_db), f_max=hi)

    def _ba_bus_pass(self, unpack, bounds, s: dict, indiv, x1):
        """BA 2차: 1차 답(x1)으로 만든 서브 버스 PEQ 를 GD 항에 넣고 짧게 다시 찾은 뒤, 두 답을 '각자의 버스 PEQ 를 건'
        비용으로 견줘 작은 쪽을 돌려준다(10-09 돌비룸: BA 가 버스 PEQ 전 응답으로 GD 를 봐서 PEQ 가 만드는 20 Hz +5.3 dB
        부스트와 그 GD 봉우리(31 → 60 ms)를 몰랐고, 그 GD 항 때문에 SW3 게인을 −3.2 dB 낮췄다). 버스 PEQ 는 서브 합에 따라
        바뀌니 후보마다 맞추면 DE 가 수십 배 느려진다 — 1차 PEQ 를 고정해 근사하고, 마지막 선택만 정확히 잰다.
        1차 답을 DE 초기 집단에 넣어(x0) 2차 답의 근사 비용은 1차보다 나빠질 수 없고, 마지막 비교로 정확한 비용도 마찬가지.
        `SUB_BUS_FIX` 가 꺼져 있으면 1차 답 그대로."""
        if not SUB_BUS_FIX:
            return x1
        P1 = self._bus_peq_of(unpack(x1))
        if P1 is None:
            return x1
        from scipy.optimize import differential_evolution

        res = differential_evolution(lambda x: self._ba_cost(unpack(x), s, indiv, P1), bounds, maxiter=BA_BUS_ITERS,
                                     popsize=12, tol=1e-6, seed=2, polish=False, x0=np.clip(x1, *np.array(bounds).T))

        def exact(x):
            return self._ba_cost(unpack(x), s, indiv, self._bus_peq_of(unpack(x)))

        c1, c2 = exact(x1), exact(res.x)
        self.notes.append(f'BA 버스 PEQ 반영 비용 {c1:.2f} → {min(c1, c2):.2f}' + ('' if c2 < c1 else ' (1차 답 유지)'))
        return res.x if c2 < c1 else x1

    # ── 3′. 서브 수동 설정(R27): 고정 · 묶음 ──
    def manual_value(self, role: str) -> tuple:
        """사람이 넣은 (게인 dB, 딜레이 ms, 극성 ±1, 올패스 [(f, q)] ≤ 2) — 고정이면 DSP 출력 값, 묶음이면 묶음 안 상대값."""
        m = self.sub_manual.get(role) or {}
        return (float(m.get('gain_db', 0.0)), float(m.get('delay_ms', 0.0)), -1 if int(m.get('polarity', 1)) < 0 else 1,
                [(float(a), float(b)) for a, b in (m.get('allpass') or [])][:2])

    def manual_plan(self) -> dict | None:
        """서브 수동 설정 → {'fixed': [역할], 'bundles': {이름: [역할]}, 'auto': [역할], 'gain_anchored': bool}.
        모든 서브가 자동이면 None(지금 경로 그대로 — 같은 DE·같은 답). gain_anchored = 게인이 사람 값으로 묶인 서브(고정,
        `bundle_gain='fixed'` 이면 묶음도)가 있다 → 서브 버스 게인을 걸 수 없다."""
        fixed, bundles, auto = [], {}, []
        for r in self.subs:
            m = self.sub_manual.get(r) or {}
            mode = m.get('mode', 'auto')
            if mode == 'fixed':
                fixed.append(r)
            elif mode == 'bundle':
                bundles.setdefault(str(m.get('bundle') or 'A'), []).append(r)
            else:
                auto.append(r)
        if not fixed and not bundles:
            return None
        return {'fixed': fixed, 'bundles': bundles, 'auto': auto,
                'gain_anchored': bool(fixed) or (bool(bundles) and self.bundle_gain == 'fixed')}

    def sub_mode(self, role: str) -> str:
        """결과 표·내보내기에 적을 방식: 'auto' · 'fixed' · 'bundle:A'."""
        m = self.sub_manual.get(role) or {}
        if m.get('mode') == 'fixed':
            return 'fixed'
        if m.get('mode') == 'bundle':
            name = str(m.get('bundle') or 'A')
            info = self.layout_info.get(name)
            if info:                                        # R30 레이아웃 묶음: 'bundle:A:dba-rear' · 'bundle:A:multi'
                kind = info['kind']
                if kind == 'dba':
                    kind = 'dba-rear' if role in info['rear'] else 'dba-front'
                return f'bundle:{name}:{kind}'
            return f'bundle:{name}'
        return 'auto'

    # ── 3″. 묶음 레이아웃(R30) 1단: 묶음 안을 먼저 합친다 ──
    def _layout_stage(self, hi_hz: float) -> None:
        """레이아웃 묶음마다 안쪽 합성을 정한다(2단 합성의 1단). 결과는 `layout_rel`(BA 가 묶음 덩어리로 쓰는 상대값)과
        `layout_info`(요약), 묶음 PEQ 는 멤버 전부의 고정 출력 필터 `sub_filters[역할]['peq']` 뒤에 붙고 `self.H` 에 곱해진다
        (고정 필터와 같은 자리 — 긴 창·MIMO·내보내기가 그대로 따라온다. `H0` = 보정 전 표시는 그대로).

        * 레벨(사용자 결정 "레벨만 서브별로 자동"): 서브마다 20 Hz–hi 위치 평균(기준 위치 가중) 대역 레벨의 중앙값을 재고,
          가장 작은 서브에 맞춰 깎는다(부스트 없음 — 묶음 전체 게인은 2단 BA 가 맞춘다).
        * 멀티서브: 지연·극성은 모두 같다(사용자 "레벨만").
        * DBA: 뒤 줄 = 반전 + 전파 지연 P(앞 출력 대비). P 는 위치마다 도착 차 |뒤 − 앞| 의 최댓값(앞→뒤 거리 ≥ 두 서브까지
          거리 차라 P 의 하한)부터 `DBA_MAX_MS` 까지 0.25 ms 로 훑어 묶음 합의 평탄 + 좌석 편차 + 늦은 에너지가 가장 작은 값
          (DBA 의 목적 = 앞 파면이 뒷벽에 닿을 때 지워 길이 방향 모드를 없애기). 도착 차 자체를 쓰면 뒤 서브가 청취 위치에서
          앞 직접음과 같은 때에 반전으로 와 직접음을 지운다 — 그래서 하한으로만 쓴다. 사람 보정 `rear_trim_ms` 를 더한다.
          앞이나 뒤 줄이 비면 멀티서브처럼 한다(노트).
        * 묶음 PEQ: 위 상대값을 건 묶음 합(위치별)을 타겟에 맞춘 PEQ `BUNDLE_PEQ_BANDS` 개 — 멤버 전부에 똑같이.
        올패스는 2단에서 묶음 공통 변수로 찾는다(`sub_allpass` 가 켜졌을 때만)."""
        if not self.layouts:
            return
        f = self.f
        band = (f >= 20.0) & (f <= hi_hz)
        w = self._weights()
        idx = np.nonzero(band)[0]
        grid = np.unique(np.round(np.geomspace(idx[0], idx[-1], 160)).astype(int))
        lin = np.arange(int(np.searchsorted(f, 200.0)))
        lband = np.clip((f[lin] - 15.0) / 5.0, 0, 1) * np.clip((hi_hz * 1.3 - f[lin]) / (hi_hz * 0.3), 0, 1)
        fs_eff = 2.0 * f[lin[-1]]

        def late_db(spec):
            y = np.fft.irfft(spec * lband, 2 * (lin.size - 1))
            k = int(np.argmax(np.abs(y) > 0.1 * np.max(np.abs(y))))
            early = np.sum(y[k:k + int(0.05 * fs_eff)] ** 2)
            late = np.sum(y[k + int(0.1 * fs_eff):k + int(0.38 * fs_eff)] ** 2)
            return 10 * np.log10(late / (early + 1e-30) + 1e-30)

        def arrival_ms(pos, role):
            ir, _sr, *rest = self.irs[pos][role]
            return 1000.0 * ((float(rest[0]) if rest else 0.0) + onset_index(np.asarray(ir, float)) / SR)

        for name, lay in self.layouts.items():
            members = lay['members']
            positions = [p for p in self.positions if all(r in self.H[p] for r in members)]
            levels = {}
            for r in members:
                ps = [p for p in self.positions if r in self.H[p]]
                pw = sum(w[p] * np.abs(self.H[p][r][band]) ** 2 for p in ps) / sum(w[p] for p in ps)
                levels[r] = float(np.median(10 * np.log10(pw + 1e-30)))
            low = min(levels.values())
            gains = {r: low - levels[r] for r in members}
            kind = lay['kind']
            rear = [r for r in members if str((self.sub_manual.get(r) or {}).get('dba_row')) == 'rear']
            front = [r for r in members if r not in rear]
            info = {'kind': kind, 'members': list(members), 'gains': dict(gains), 'front': front, 'rear': [],
                    'rear_ms': 0.0, 'auto_ms': None, 'trim_ms': 0.0, 'min_ms': None, 'peq': []}
            rel = {r: (gains[r], 0.0, 1, []) for r in members}
            if kind == 'dba' and not (front and rear):
                self.notes.append(f'묶음 {name} DBA: 앞·뒤 줄 중 하나가 비어 멀티서브처럼 계산')
                info['kind'] = 'multi'
            elif kind == 'dba':
                gap = max(abs(np.mean([arrival_ms(p, r) for r in rear]) - np.mean([arrival_ms(p, r) for r in front]))
                          for p in positions)
                lo = float(np.ceil(min(gap, DBA_MAX_MS - 1.0) * 4) / 4)
                fgrid = f[grid]
                lf = np.log(fgrid)
                best = None
                for P in np.arange(lo, DBA_MAX_MS + 0.01, 0.25):
                    trial = {r: (gains[r], P if r in rear else 0.0, -1 if r in rear else 1, []) for r in members}
                    levels_db, cost = [], 0.0
                    for p in positions:
                        s = sum(_term(self.H[p][r][grid], fgrid, *trial[r]) for r in members)
                        db = 20 * np.log10(np.abs(s) + 1e-20)
                        cost += w[p] * np.mean((db - np.polyval(np.polyfit(lf, db, 2), lf)) ** 2)
                        sl = sum(_term(self.H[p][r][lin], f[lin], *trial[r]) for r in members)
                        cost += w[p] * DBA_LATE_WEIGHT * late_db(sl)
                        levels_db.append(db)
                    cost /= sum(w[p] for p in positions)
                    if len(levels_db) > 1:
                        cost += 0.5 * float(np.mean(np.var(np.array(levels_db), axis=0)))
                    if best is None or cost < best[0]:
                        best = (cost, float(P))
                auto = best[1]
                trim = float(lay['rear_trim_ms'])
                P = max(0.0, auto + trim)
                rel = {r: (gains[r], P if r in rear else 0.0, -1 if r in rear else 1, []) for r in members}
                info.update(rear=rear, rear_ms=P, auto_ms=auto, trim_ms=trim, min_ms=float(gap))
            # 묶음 PEQ: 묶음 합(위치별)을 타겟에 — 레벨은 30–80 Hz 중앙값(모양만 고친다)
            by_pos = {p: sum(_term(self.H[p][r], f, *rel[r]) for r in members) for p in positions}
            m = (f >= 30) & (f <= 80)
            lvl = float(np.median(20 * np.log10(np.abs(by_pos[self.main_position if self.main_position in by_pos
                                                                 else positions[0]][m]) + 1e-20)
                                  - self.target_db_fn(f[m])))
            peq = self._xo_target_peq(by_pos, lvl, 20.0, hi_hz, BUNDLE_PEQ_BANDS, 6.0) if by_pos else []
            if peq:
                P_resp = peq_response(f, peq)
                for r in members:
                    sf = self.sub_filters.setdefault(r, {'hp': 0.0, 'hp_type': 'LR24', 'lp': 0.0, 'lp_type': 'LR24', 'peq': []})
                    sf['peq'] = list(sf['peq']) + [tuple(b) for b in peq]
                    for p in self.H:
                        if r in self.H[p]:
                            self.H[p][r] = self.H[p][r] * P_resp
            info['peq'] = [tuple(b) for b in peq]
            self.layout_rel[name] = rel
            self.layout_info[name] = info
            text = (f"묶음 {name} {'DBA' if info['kind'] == 'dba' else '멀티서브'}: 레벨 "
                    + ', '.join(f'{r} {gains[r]:+.1f}' for r in members))
            if info['kind'] == 'dba':
                text += (f" · 뒤 {'·'.join(rear)} 반전 {info['rear_ms']:.2f} ms(자동 {info['auto_ms']:.2f}"
                         f" + 보정 {info['trim_ms']:+.2f}, 도착 차 {info['min_ms']:.2f})")
            self.notes.append(text + f' · 묶음 PEQ {len(peq)}')

    def _align_subs_manual(self, hi_hz: float, plan: dict):
        """고정·묶음이 섞인 BA. 돌려주는 값은 `_align_subs` 와 같은 꼴(서브 순서, 메인 추가 지연 전 내부 시간축).

        단위: 자동 서브 = 변수 7개(지금과 같다), 묶음 = 공통 지연·극성(+ `bundle_gain='auto'` 이면 공통 게인), 고정 = 상수.
        고정이 있으면 내부 시간축 = 출력 딜레이 − 메인 추가 지연이고, 자동·묶음은 출력이 0 아래로 가지 않게 지연 범위를 자른다.
        자동·묶음의 ±3 ms 중심은 고정 서브가 자기 도착 정렬에서 벗어난 양(중앙값)만큼 옮긴다(사람이 서브를 일부러 늦췄으면
        나머지도 그 근처에서 찾게). 게인이 묶인 서브가 있으면 자동 서브의 게인은 절대값 — 범위 = 묶인 서브 게인 평균 ±8 dB
        (자동 모드의 '서브마다 −8…0 dB' = 서로 8 dB 안을 묶인 서브 기준으로 옮긴 것. 10-05 돌비 b1: 묶인 서브들의 파워 합
        레벨 −8…0 dB 로 잡았더니 자동 결과의 INFRA(DBA 보다 7.9 dB 낮음)가 범위 밖이라 평탄도 3.65 → 5.14 dB)."""
        from scipy.optimize import differential_evolution

        base = {r: self.align_delay_ms[r] for r in self.subs}
        main_extra = self.sub_main_delay_ms if plan['fixed'] else 0.0
        const = {}
        for r in plan['fixed']:
            g, d, pol, aps = self.manual_value(r)
            const[r] = (g, d - main_extra, pol, aps)
        off = float(np.median([const[r][1] - base[r] for r in plan['fixed']])) if plan['fixed'] else 0.0
        floor = -main_extra if plan['fixed'] else None

        def delay_bounds(center):
            lo = -BA_DELAY_MS if floor is None else max(-BA_DELAY_MS, floor - center)
            return lo, max(BA_DELAY_MS, lo + 1.0)

        # 게인: 묶인 서브(고정 · 글자대로면 묶음)의 게인 평균 ±8 dB. rel = 묶음 안 상대 게인 평균(묶음 공통 게인일 때) — 묶음의
        # 평균 게인이 자동 서브와 같은 범위에 오게 뺀다(10-05 돌비 b1: 묶음 값 −10.6 dB 를 안 빼니 묶음이 INFRA 보다 늘 10 dB 넘게
        # 작아 자동 결과(DBA 가 INFRA 보다 7.9 dB 큼)에 못 가 평탄도 3.65 → 4.02 dB).
        anchored = list(plan['fixed'])
        if self.bundle_gain == 'fixed':
            anchored += [r for rs in plan['bundles'].values() for r in rs]
        ref = float(np.mean([self.manual_value(r)[0] for r in anchored])) if anchored else None

        def gain_bounds(rel=0.0):
            if ref is None:
                return (-rel - 8.0, -rel)
            return (ref - rel - 8.0, ref - rel + 8.0)

        qmax = 1.0 if self.sub_allpass == 'lowq' else 3.0
        units, bounds = [], []
        for r in plan['auto']:
            center = base[r] + off
            units.append({'kind': 'auto', 'role': r, 'center': center})
            bounds += [gain_bounds(), delay_bounds(center), (0, 1), (20, 150), (0.3, qmax), (20, 150), (0.3, qmax)]
        for name, roles in plan['bundles'].items():
            # R30 레이아웃 묶음: 상대값 = 1단 결과(레벨·DBA 뒤 반전·지연), 게인은 늘 묶음 공통 변수, 올패스는 묶음 공통(켜졌을 때)
            layout = name in self.layout_rel
            vals = dict(self.layout_rel[name]) if layout else {r: self.manual_value(r) for r in roles}
            dmin = min(v[1] for v in vals.values())
            # 묶음 기준 지연: 묶음의 첫 음향 도착이 도착 정렬 시점에 오게(상대 딜레이를 건 뒤 가장 이른 멤버 기준)
            center = max(base[r] - (vals[r][1] - dmin) for r in roles) + off
            gain_var = self.bundle_gain == 'auto' or layout
            ap_var = layout and self.sub_allpass != 'none'
            units.append({'kind': 'bundle', 'roles': roles, 'center': center, 'gain_var': gain_var, 'ap_var': ap_var,
                          'rel': {r: (g, d - dmin, pol, aps) for r, (g, d, pol, aps) in vals.items()}})
            if gain_var:
                bounds.append(gain_bounds(float(np.mean([vals[r][0] for r in roles]))))
            bounds += [delay_bounds(center), (0, 1)]
            if ap_var:
                bounds += [(20, 150), (0.3, qmax), (20, 150), (0.3, qmax)]

        def unpack(x):
            out = dict(const)
            k = 0
            for u in units:
                if u['kind'] == 'auto':
                    g, d, pol, f1, q1, f2, q2 = x[k:k + 7]
                    k += 7
                    aps = [] if self.sub_allpass == 'none' else [(f1, q1), (f2, q2)]
                    out[u['role']] = (g, u['center'] + d, 1 if pol < 0.5 else -1, aps)
                    continue
                g0 = 0.0
                if u['gain_var']:
                    g0 = x[k]
                    k += 1
                d, pol = x[k:k + 2]
                k += 2
                common = []
                if u.get('ap_var'):
                    f1, q1, f2, q2 = x[k:k + 4]
                    k += 4
                    common = [(f1, q1), (f2, q2)] if f1 > 0 and f2 > 0 else []   # 0 = DE 없이 기본값으로 풀 때(묶음 하나뿐)
                sign = 1 if pol < 0.5 else -1
                for r, (g, dr, p, aps) in u['rel'].items():
                    out[r] = (g + g0, u['center'] + dr + d, p * sign, list(aps) + common)
            return [out[r] for r in self.subs]

        names = [f"{r} 고정" for r in plan['fixed']] + [f"{'·'.join(rs)} 묶음 {b}" for b, rs in plan['bundles'].items()]
        self.notes.append('BA 수동: ' + ', '.join(names))
        if not units or (len(units) == 1 and not plan['fixed']):
            # 고정뿐이거나 묶음 하나뿐(자동·고정 없음) — 공통 지연·게인·극성은 합의 모양을 안 바꿔 BA 가 정할 것이 없다
            # (서브 1개일 때와 같은 이유). 묶음은 기준 지연 · 정상 극성 그대로, 메인 XO 맞춤은 버스 단계가 한다.
            return unpack(np.zeros(len(bounds)))
        s = self._ba_setup(hi_hz)
        indiv = None
        if ref is None:
            grid = s['grid']
            indiv = {pos: sum(np.abs(self.H[pos][r][grid]) ** 2 for r in self.subs) for pos in s['positions']}
        res = differential_evolution(lambda x: self._ba_cost(unpack(x), s, indiv), bounds, maxiter=60, popsize=12,
                                     tol=1e-6, seed=1, polish=False)
        self.notes.append(f'BA 비용 {res.fun:.2f}')
        return unpack(self._ba_bus_pass(unpack, bounds, s, indiv, res.x))

    # ── 4. 메인·서브 XO 결합: 서브 버스 전체 지연·극성 ──
    def _bus_delay(self, sub_sum_by_pos, xo_of, lo: float = -15.0):
        best = (1e18, 0.0, 1)
        m_all = {}
        for role in self.mains:
            xo = xo_of.get(role)
            if xo:
                m_all[role] = np.nonzero((self.f >= xo / 2) & (self.f <= xo * 2))[0]
        if not m_all:
            return 0.0, 1
        # 후보마다 N_FFT 전체 지연 응답을 만들고 잘랐던 것을 XO 구간에서만 계산한다.
        # 합산 순서·후보 순서·동점 선택은 그대로 둔다(09-26 결과 동일성).
        prepared = []
        for role, m in m_all.items():
            f = self.f[m]
            xo = xo_of[role]
            delay = np.exp(-2j * np.pi * f * self.align_delay_ms[role] / 1000.0)
            hp, lp = lr4(f, xo, 'HP'), lr4(f, xo, 'LP')
            for pos in self.positions:
                if role in self.H[pos]:
                    main = self.H[pos][role][m] * delay * hp
                    sub = sub_sum_by_pos[pos][m] * lp
                    prepared.append((f, main, sub))
        for pol in (1, -1):
            for d in np.arange(lo, 15.01, 0.25):
                c = 0.0
                for f, main, sub_base in prepared:
                    sub = sub_base * pol * np.exp(-2j * np.pi * f * d / 1000.0)
                    tot = np.abs(main + sub) ** 2
                    ref = np.abs(main) ** 2 + np.abs(sub) ** 2
                    c += float(np.mean((10 * np.log10(ref + 1e-30) - 10 * np.log10(tot + 1e-30)) ** 2))
                if c < best[0]:
                    best = (c, float(d), pol)
        return best[1], best[2]

    # ── 5. 채널 보정 필터 ──
    def _no_boost_mask(self, H: np.ndarray, spike_ms: float | None = None) -> np.ndarray:
        """RC 대역에서 1/24 oct GD 가 주변(±1/3 oct 중앙값)보다 spike_ms 넘게 튀는 곳의 ±1/12 oct(선형 격자 불 배열)."""
        if spike_ms is None:
            spike_ms = float(__import__('os').environ.get('SECS_NOBOOST_MS', '1e9'))   # 기본 끔(09-26: 30 ms 로 켜면 최고사양 20–200 Hz ±1 dB 93 → 73 %, 147 Hz 봉우리도 안 줄어듦)
        f = self.f
        g = np.geomspace(max(self.rc_lo, 20.0), self.rc_hi, 1200)
        gd = gd_curve(H, f, g)
        base = np.array([np.median(gd[max(0, i - 60):i + 60]) for i in range(g.size)])
        mask = np.zeros(f.size, bool)
        for k in np.nonzero(gd - base > spike_ms)[0]:
            mask |= (f >= g[k] / 2 ** (1 / 12)) & (f <= g[k] * 2 ** (1 / 12))
        return mask

    def _long_mode_mask(self, H: np.ndarray) -> np.ndarray:
        """오래 울리는 모드 둘레 ±1/24 oct(선형 격자 불 배열) — 여기서는 부스트를 LONG_MODE_BOOST_DB 까지만 한다(09-26 사용자: BA·RC 뒤 65 Hz
        감쇠가 더 느려짐 — RC 가 63.5 Hz 모드 둘레를 +2.5 dB 올려 T60 0.71 → 0.77 s). '오래' = 늦은 스펙트럼 봉우리들의
        T60 중앙값 × LONG_MODE_RATIO 이상이면서 LONG_MODE_MIN_S 이상. 방 전체가 긴 홀에서 다 막지 않게 상대값으로 본다."""
        from secs.mimo import FS_LOW, _late_peaks, mode_t60, to_low

        f = self.f
        mask = np.zeros(f.size, bool)
        x = to_low(H, f, int(WINDOW_MS / 1000 * FS_LOW))
        peaks = [p for p in _late_peaks([x], 8) if self.rc_lo <= p <= min(self.rc_hi, 160.0)]
        t60 = {p: mode_t60(x, FS_LOW, p, 1.5) for p in peaks}
        t60 = {p: v for p, v in t60.items() if v}
        if len(t60) < 2:
            return mask
        ref = float(np.median(list(t60.values())))
        for p, v in t60.items():
            if v >= max(LONG_MODE_MIN_S, LONG_MODE_RATIO * ref):
                mask |= (f >= p / 2 ** (1 / 24)) & (f <= p * 2 ** (1 / 24))
        if mask.any():
            self.notes.append(f'긴 모드 부스트 ≤ {LONG_MODE_BOOST_DB:.0f} dB ' + ', '.join(f'{p:.0f} Hz {v:.2f} s' for p, v in sorted(t60.items())
                                                            if v >= max(LONG_MODE_MIN_S, LONG_MODE_RATIO * ref)))
        return mask

    def _correction(self, role: str, combined_by_pos: dict, level_db: float) -> tuple[np.ndarray, np.ndarray]:
        f = self.f
        w = self._freq_weights()
        power = sum(w[p] * np.abs(combined_by_pos[p]) ** 2 for p in combined_by_pos) / sum(w[p] for p in combined_by_pos)
        sm = 10 * np.log10(smooth_power(f, power, frac_for(f)) + 1e-30)
        sm3 = 10 * np.log10(smooth_power(f, power, 3.0) + 1e-30)
        sm = np.maximum(sm, sm3 - DIP_ALLOW_DB)               # 좁은 딥은 DIP_ALLOW_DB 만 채운다
        tgt = self.target_db_fn(np.maximum(f, 1.0)) + level_db
        cap = np.where(f > 16000, min(HF_BOOST_DB, self.max_boost_db), self.max_boost_db)
        corr = np.clip(tgt - sm, -MAX_CUT_DB, cap)
        # 초과위상 GD 봉우리(비최소위상 딥) 둘레 ±1/12 oct 는 부스트 금지 — 최소위상 부스트로 채우면 GD 가 더 커진다
        # (09-26 L 147 Hz: 입력 PEQ 가 Q 11 +10 dB 로 채워 봉우리 145 → 171 ms).
        nb = self._no_boost_mask(combined_by_pos[self.main_position])
        corr = np.where(nb, np.minimum(corr, 0.0), corr)
        lm = self._long_mode_mask(combined_by_pos[self.main_position])
        corr = np.where(lm, np.minimum(corr, LONG_MODE_BOOST_DB), corr)
        # 보정 범위(rc_lo–rc_hi) 밖은 반 옥타브에 걸쳐 0 dB 로
        lf = np.log2(np.maximum(f, 1e-3))
        hi_edge = min(self.rc_hi * np.sqrt(2), SR / 2 * 0.99)
        fade = np.clip((lf - np.log2(self.rc_lo / np.sqrt(2))) / 0.5, 0, 1) *             np.clip((np.log2(hi_edge) - lf) / max(np.log2(hi_edge / self.rc_hi), 1e-3), 0, 1)
        corr = corr * fade
        corr0 = corr.copy()
        self.design_db[role] = corr0
        peq, P_peq = [], np.ones(f.size, complex)
        if self.taps <= 0:
            # IIR 만(09-25 사용자: 무지연 모드는 IIR 로만): FIR 없이 입력 PEQ 가 전 대역을 맡는다(최소위상, 지연 0).
            gl = np.geomspace(15, 20000, 900)
            # 밴드를 저역(≤ 400 Hz)과 그 위로 반씩 나눠 맞춘다 — 한꺼번에 맞추면 오차가 큰 저역이 밴드를 다 가져가
            # 고역이 57 % 에 머물렀다(09-25, PEQ 48 도 59 %). 자동 RC 상한 500 Hz 에서는 400–500 Hz 에 16개가 가지만
            # 바꿔 봐도 나아지지 않았다(10-05 R1: 32개 한 번에 = 1/24 oct ±1 dB 평균 91 → 94 % 대신 계산 37 → 77 s,
            # 나누는 점을 옥타브 가운데로 = 평균 그대로 채널만 오르내림). 남은 오차는 대부분 덜 채운 널 둘레(설계 상한 94 %).
            cg = np.interp(gl, f, corr)
            n_lo = self.peq_bands // 2
            lo_part = fit_peq(gl, np.where(gl <= 500, cg, 0.0), n_lo, lo=max(15.0, self.rc_lo / 1.4), hi=400.0,
                              boost=self.max_boost_db, ring_ms=BOOST_RING_MS)
            rest = cg - 20 * np.log10(np.abs(peq_response(gl, lo_part)) + 1e-12)
            hi_part = fit_peq(gl, np.where(gl >= 300, rest, 0.0), self.peq_bands - n_lo, lo=400.0,
                              hi=min(20000.0, self.rc_hi), boost=self.max_boost_db, ring_ms=BOOST_RING_MS)
            peq = lo_part + hi_part
            self.peq[role] = peq
            self.phase_strength[role] = 0.0
            self.phase_band[role] = None
            return np.array([1.0]), peq_response(f, peq)
        if self.peq_bands:
            # 짧은 FIR(2,048탭 ≈ 23 Hz 해상도)은 저역 모드를 못 푼다 → 저역 크기는 입력 PEQ(최소위상, 지연 0)가,
            # 나머지 크기·위상은 FIR 이. PEQ 는 로그 격자에서 맞춘다.
            gl = np.geomspace(15, 20000, 700)
            peq = fit_peq(gl, np.interp(gl, f, corr), self.peq_bands, boost=self.max_boost_db, ring_ms=BOOST_RING_MS,
                          f_max=self.rc_hi)          # 밴드 중심은 RC 상한 안(10-09: R 에 191–222 Hz 밴드)
            P_peq = peq_response(f, peq)
            corr = corr - 20 * np.log10(np.abs(P_peq) + 1e-12)
            lo_fir = 1.5 * SR / self.taps                     # FIR 이 푸는 곳 위로만 남은 크기를 맡긴다
            corr = corr * np.clip((f - lo_fir / 2) / (lo_fir / 2), 0, 1)
        self.peq[role] = peq
        mag = 10 ** (corr / 20)
        M = minimum_phase(mag)
        # 초과위상: 크기 보정 뒤 기준 위치 합의 초과위상(1/3 oct 복소 스무딩, 도착 지연 뺀 뒤)
        # 여러 위치에 공통인 초과위상만(09-26 사용자 승인): 위치마다 순수 지연을 빼고 초과위상 단위 페이저를
        # 가중 평균한다 — 한 점에만 있는 좁은 GD 봉우리(간섭)는 평균에서 흩어지고, 방 전체의 공통 성분만 남는다.
        # 위치가 하나면 전과 같다. SECS_PHASE_POS=main 이면 기준 위치만(비교용).
        w = self._weights()
        use = [self.main_position] if PHASE_POSITIONS == 'main' else list(combined_by_pos)
        U = 0
        for p in use:
            ref = combined_by_pos[p] * P_peq * M
            t = np.argmax(np.abs(np.fft.irfft(ref, N_FFT))) / SR    # 순수 지연을 먼저 빼야 복소 스무딩이 안 상쇄된다
            ref = ref * np.exp(2j * np.pi * f * t)
            mp = minimum_phase(np.sqrt(smooth_power(f, np.abs(ref) ** 2, frac_for(f))) + 1e-12)
            ex = ref / (mp + 1e-30)
            U = U + w[p] * ex / (np.abs(ex) + 1e-30)
        ex_s = smooth_complex(f, U / sum(w[p] for p in use), self.phase_frac)
        phase = np.unwrap(np.angle(np.nan_to_num(ex_s)))
        # 고역(3–6 kHz, 보정 안 하는 대역)의 위상을 기준 0 으로 — 저역을 고역 도착에 맞춘다(기울기를 빼면 저역·고역이
        # 서로 어긋나 REW 에서 고역 위상이 계속 돌았다, 09-25).
        hf = (f >= 3000) & (f <= 6000)
        shape = phase - np.median(phase[hf])
        fade_hz = self.rc_hi * np.sqrt(2)         # 위상 보정도 RC 대역 안(09-25 사용자: RC 대역의 GD 를 쭉 평탄하게)
        g = np.clip((np.log2(fade_hz) - np.log2(np.maximum(f, 1e-3))) / 0.5, 0, 1)
        g[f < 15] = np.clip(f[f < 15] / 15, 0, 1)
        # 실현 검사: 위상 보정이 lift 보다 앞당겨야 하면 FIR 밖으로 잘려 크기가 무너진다(09-25 45 Hz −20 dB) —
        # 만든 FIR 의 크기가 설계 크기 M 과 20 Hz–4 kHz 에서 RMS ≤ 0.5 dB·최대 ≤ 2 dB 가 될 때까지 위상 보정 세기를 줄인다.
        band = (f >= max(20.0, 1.5 * SR / max(self.taps, 1))) & (f <= fade_hz)
        k = int(self.taps * 0.1)
        # GD 예산 방식(09-25, 지연 ≤ 20 ms·2,048탭): 초과 GD(고역 3–6 kHz 기준, 1/6 oct)을 주파수마다
        # lift 안에서만 당긴다 — 보정 필터의 지연 d(f) = lift − 당김, 위상 = −2π∫d df. 모양을 통째로 되돌리면
        # 저역에서 lift 보다 많이 당겨야 해 FIR 밖으로 잘렸다. 실현 검사에 걸리면 예산을 줄인다.
        df = f[1] - f[0]
        # 0 Hz 와 첫 칸의 x 가 같아(둘 다 df) 그 두 칸은 0 으로 나눈다 — 값은 아래 nan_to_num 이 0 으로 바꾼다. 경고만 막는다
        # (10-10 AI 시험: 보정 계산마다 'divide by zero' 가 찍혔다. 결과는 전과 같다).
        with np.errstate(divide='ignore', invalid='ignore'):
            tau = -np.gradient(phase, 2 * np.pi * np.maximum(f, df))
        tau = smooth_power(f, np.nan_to_num(tau, nan=0.0, posinf=0.0, neginf=0.0), self.phase_frac)
        excess = (tau - np.median(tau[hf])) * g
        lift = self.lift_ms / 1000.0
        def realize(frac, M=M):
            adv = np.clip(excess, -0.005, lift * frac)
            phi = -2 * np.pi * np.cumsum(lift - adv) * df
            h = np.fft.irfft(M * np.exp(1j * phi), N_FFT)[:self.taps]
            h[-k:] *= 0.5 * (1 + np.cos(np.pi * np.arange(k) / k))
            F = np.fft.rfft(h, N_FFT)
            err = 20 * np.log10(np.abs(F[band]) + 1e-12) - 20 * np.log10(np.abs(M[band]) + 1e-12)
            return h, F, float(np.sqrt(np.mean(err ** 2))), float(np.max(np.abs(err)))

        # 기준 = 위상 보정 없는(순수 지연) FIR 의 오차 — 짧은 FIR 은 그 자체로 좁은 모양을 못 만든다.
        h, F, rms0, max0 = realize(0.0)
        f_lo = 0.0
        if self.lift_ms <= 0:                         # 무지연 모드: 최소위상만(초과위상 보정·지연 없음)
            h = np.fft.irfft(M, N_FFT)[:self.taps]
            h[-k:] *= 0.5 * (1 + np.cos(np.pi * np.arange(k) / k))
            F = np.fft.rfft(h, N_FFT)
        for frac in (() if self.lift_ms <= 0 else (0.9, 0.7, 0.5, 0.3)):
            hh, FF, rms, mx = realize(frac)
            if rms <= max(0.5, rms0 + 0.3) and mx <= max(2.0, max0 + 1.0):
                h, F, f_lo = hh, FF, frac
                break
        strength = f_lo or 0.0
        self.phase_band[role] = f_lo
        self.phase_strength[role] = strength
        if self.peq_bands:
            # 2차 맞춤: FIR 이 실제로 만든 크기를 빼고 PEQ 를 다시 맞춘다(FIR 해상도·위상 보정이 남긴 저역 오차까지)
            gl = np.geomspace(15, 20000, 700)
            fdb = 20 * np.log10(np.abs(F) + 1e-12)
            resid = np.where(nb, np.minimum(corr0 - fdb, 0.0), corr0 - fdb)
            peq = fit_peq(gl, np.interp(gl, f, resid), self.peq_bands, boost=self.max_boost_db, ring_ms=BOOST_RING_MS,
                          f_max=self.rc_hi)
            if any(g > 0 and np.interp(f0, f, nb.astype(float)) > 0.5 for f0, _q, g in peq):
                peq = [(f0, q, min(g, 0.0) if np.interp(f0, f, nb.astype(float)) > 0.5 else g) for f0, q, g in peq]
            P_peq = peq_response(f, peq)
            self.peq[role] = peq
            # 3차 맞춤(10-05 R1): 2차 PEQ 는 좁은 오차(FIR 이 못 푸는 모드)에 밴드를 먼저 쓰고, 넓은 기울기는 남긴다(돌비
            # 최고사양 Lb: 16개 중 12개가 100–420 Hz, 20–60 Hz +0.4 · 60–100 Hz −0.6 dB 가 남아 1/6 oct ±1 dB 81 %, 설계
            # 그대로면 92 %). 남은 오차의 넓은 성분(1/3 oct 평균)만 FIR 크기에 곱해 다시 만든다 — 넓은 모양은 짧은
            # 임펄스라 FIR 의 좁은 물결(2차 PEQ 가 맞춰 둔 것)은 거의 그대로다. FIR 을 통째로 '설계 − 2차 PEQ' 에 다시
            # 맞추면 그 짝이 깨져 1/24 오차가 0.80 → 1.18 dB 로 커졌다. 설계(1/24 oct, RC 범위)에 더 가까울 때만 쓴다.
            rc = (f >= max(self.rc_lo, 20.0)) & (f <= self.rc_hi)

            def design_err(Fx):
                e = 10 * np.log10(smooth_power(f, np.abs(Fx * P_peq) ** 2, 24.0) + 1e-30)[rc] - corr0[rc]
                return float(np.sqrt(np.mean((e - np.median(e)) ** 2)))

            fir_band = np.clip((f - lo_fir / 2) / (lo_fir / 2), 0, 1) * rc
            for _ in range(FIR_TOUCHUP_ROUNDS):
                r = np.where(rc, corr0 - 20 * np.log10(np.abs(F * P_peq) + 1e-12), 0.0)
                broad = smooth_power(f, r, 3.0) * fir_band
                M2 = minimum_phase(np.abs(M) * 10 ** (broad / 20))
                if self.lift_ms > 0:
                    h2, F2 = realize(f_lo, M2)[:2]
                else:
                    h2 = np.fft.irfft(M2, N_FFT)[:self.taps]
                    h2[-k:] *= 0.5 * (1 + np.cos(np.pi * np.arange(k) / k))
                    F2 = np.fft.rfft(h2, N_FFT)
                if design_err(F2) >= design_err(F):
                    break
                h, F, M = h2, F2, M2
        return h, F * P_peq

    # ── 1차: 음향 XO 타겟 보정(출력 쪽 PEQ) ──
    def xo_peq_band(self, xo: float) -> tuple[float, float]:
        """메인 탑 XO PEQ 를 맞출 대역(Hz). 10-09 사용자: RC 를 185 Hz 까지만 했는데 XO PEQ 가 XO × 4(480 Hz)까지 걸렸다 →
        상한을 사람이 정하게 했다 — 자동 = XO ½ … XO × 4(전과 같음 — 실측: RC 상한으로 자르면 GD 최대 177 → 148 대신
        ±2 dB 비율 76.7 → 68.9 %, 사용자가 평탄 우선을 골랐다), 사람 값(`xo_peq_lo`·`xo_peq_hi`, 0 = 자동), 끔이면 (0, 0)."""
        if not getattr(self, 'xo_peq_on', True):
            return 0.0, 0.0
        lo = float(getattr(self, 'xo_peq_lo', 0.0) or xo / 2)
        hi = float(getattr(self, 'xo_peq_hi', 0.0) or xo * 4)     # 자동 = 예전 그대로(사용자 결정 10-09: 평탄 우선)
        return lo, hi

    def _xo_target_peq(self, by_pos: dict, level_db: float, lo: float, hi: float, bands: int, boost: float,
                       f_max: float | None = None) -> list:
        """by_pos(위치 → 복소 응답, 전기 XO 전)를 lo–hi 에서 타겟 + level 에 맞추는 PEQ. 전기 LR4 는 양쪽에 같이
        곱해지므로 '음향 = 타겟 × LR4' 를 맞추는 것과 같다. 구간 밖은 0 dB(다음 단계 몫)."""
        f = self.f
        w = self._weights()
        power = sum(w[p] * np.abs(by_pos[p]) ** 2 for p in by_pos) / sum(w[p] for p in by_pos)
        sm = 10 * np.log10(smooth_power(f, power, frac_for(f)) + 1e-30)
        sm3 = 10 * np.log10(smooth_power(f, power, 3.0) + 1e-30)
        sm = np.maximum(sm, sm3 - DIP_ALLOW_DB)
        corr = np.clip(self.target_db_fn(np.maximum(f, 1.0)) + level_db - sm, -MAX_CUT_DB, boost)
        gl = np.geomspace(15, 2000, 500)
        cg = np.interp(gl, f, corr)
        soft = np.clip(np.log2(gl / (lo / 1.4)) / 0.5, 0, 1) * np.clip(np.log2((hi * 1.4) / gl) / 0.5, 0, 1)
        return fit_peq(gl, cg * soft, bands, lo=lo, hi=hi, boost=boost, f_max=f_max)

    # ── 전체 ──
    def schroeder_hz(self) -> float | None:
        """f_s = 2000·√(T60 / V). T60 = 기준 위치 메인들의 500 Hz·1 kHz 옥타브 T20(Schroeder 적분, −5~−25 dB 외삽) 중앙값.
        룸 볼륨가 없거나 감쇠를 못 재면 None."""
        if not self.room_volume_m3:
            return None
        from scipy.signal import butter, sosfilt

        t60s = []
        for role in self.mains:
            if role not in self.irs[self.main_position]:
                continue
            ir = np.asarray(self.irs[self.main_position][role][0], float)
            for fc in (500.0, 1000.0):
                y = sosfilt(butter(4, [fc / np.sqrt(2), fc * np.sqrt(2)], btype='band', fs=SR, output='sos'), ir)
                e = np.cumsum(y[::-1] ** 2)[::-1]
                db = 10 * np.log10(e / (e[0] + 1e-30) + 1e-30)
                i5, i25 = np.argmax(db < -5), np.argmax(db < -25)
                if i25 > i5 > 0:
                    t60s.append(3.0 * (i25 - i5) / SR)
        if not t60s:
            return None
        return float(2000.0 * np.sqrt(np.median(t60s) / float(self.room_volume_m3)))

    def _step(self, done: int) -> None:
        """진행 알림(10-09 사용자: 계산 중 얼마나 됐는지 안 나온다) — 1 = BA, 그다음 메인 채널마다 하나."""
        cb = getattr(self, 'progress', None)
        if cb is not None:
            try:
                cb(done, 1 + len(self.mains))
            except Exception:  # noqa: BLE001 - 화면 알림 실패가 계산을 망치지 않게
                pass

    def compute(self, subs_only: bool = False) -> CorrectionResult:
        """subs_only = BA(정렬·버스 게인·버스 PEQ·버스 지연)까지만 — 메인 RC 를 건너뛴다(10-09 사용자: 서브만 보는데
        매번 전부 다시 계산하는 게 불편). 결과의 channels 는 비어 있다."""
        self._prepare()
        if self.sub_filters:
            self.notes.append('서브 고정 필터: ' + ', '.join(
                f"{r}" + (f" HP {sf['hp']:g} {sf['hp_type']}" if sf['hp'] else '') + (f" LP {sf['lp']:g} {sf['lp_type']}" if sf['lp'] else '')
                + (f" PEQ {len(sf['peq'])}" if sf['peq'] else '') for r, sf in self.sub_filters.items()))
        if self.rc_hi_manual is None:
            fs = self.schroeder_hz()
            self.rc_hi = RC_AUTO_MAX_HZ if (fs is None or fs > RC_AUTO_MAX_HZ) else max(fs, 100.0)
            self.notes.append(f'RC 상한 {self.rc_hi:.0f} Hz (' + ('기본값' if fs is None else f'슈뢰더 {fs:.0f} Hz') + ')')
        else:
            self.notes.append(f'RC 상한 {self.rc_hi:.0f} Hz (수동)')
        xo_of = {}
        for group, roles in self.groups.items():
            if group == 'sub':
                continue
            for r in roles:
                if self.subs and self.xo.get(group):
                    xo_of[r] = float(self.xo[group])
        # 레벨 기준: 메인 중 가장 작은 대역 레벨(모두 컷으로 맞춤)
        levels = {r: self._band_level(r, 300, 3000) for r in self.mains if r in self.H[self.main_position]}
        ref_level = min(levels.values())
        sub_params, sub_sum_by_pos, bus_d, bus_pol, bus_gain = [], None, 0.0, 1, 1.0
        manual = self.manual_plan() if self.subs else None
        self.sub_level_db = None
        # 서브 버스 PEQ 의 재료 — BA 2차(`_ba_bus_pass`)가 같은 PEQ 를 미리 맞춰 보도록 먼저 정한다(10-09)
        self._bus_ctx = ({'ref': ref_level, 'anchored': bool(manual and manual['gain_anchored']),
                          'hi': BUS_PEQ_XO_RATIO * max(xo_of.values()), 'xo_mean': float(np.mean(list(xo_of.values())))}
                         if self.subs and self.xo_stage and xo_of else None)
        if self.subs:
            hi = max(xo_of.values(), default=100.0) * 1.5
            self._layout_stage(hi)                          # R30: 레이아웃 묶음 안을 먼저 합친다(2단 합성의 1단)
            sub_params = self._align_subs(hi) if manual is None else self._align_subs_manual(hi, manual)
            sub_params = self._match_two_way(sub_params, manual)
            self._step(1)
            sub_sum_by_pos = {}
            for pos in self.positions:
                if all(r in self.H[pos] for r in self.subs):
                    sub_sum_by_pos[pos] = self._sub_sum(sub_params, np.arange(self.f.size), [pos])[pos]
            # 서브 버스 레벨: 30–80 Hz 가 타겟 + 기준 레벨이 되게
            m = (self.f >= 30) & (self.f <= 80)
            sub_lvl = float(np.median(20 * np.log10(np.abs(sub_sum_by_pos[self.main_position][m]) + 1e-20)
                                      - self.target_db_fn(self.f[m])))
            self.sub_level_db = sub_lvl - ref_level           # 버스 게인 전 서브 합이 메인 레벨보다 큰 양(dB)
            if manual is None or not manual['gain_anchored']:
                bus_gain = 10 ** ((ref_level - sub_lvl) / 20)
            else:
                # R27: 게인이 사람 값으로 묶인 서브(고정·묶음)가 있으면 공통 게인을 걸 수 없다 — 서브 합 레벨은 넣은 게인이 정한다
                self.notes.append(f'서브 버스 게인 안 걸음(고정·묶음 게인 그대로) — 서브 합이 메인 레벨 {self.sub_level_db:+.1f} dB')
            for pos in sub_sum_by_pos:
                sub_sum_by_pos[pos] = sub_sum_by_pos[pos] * bus_gain
            if self._bus_ctx:
                self.sub_bus_peq = self._bus_peq_fit(sub_sum_by_pos)
                P = peq_response(self.f, self.sub_bus_peq)
                for pos in sub_sum_by_pos:
                    sub_sum_by_pos[pos] = sub_sum_by_pos[pos] * P
            earliest = min(d for _g, d, _p, _a in sub_params)
            if self.xo_stage:
                for role, xo in xo_of.items():
                    lo, hi = self.xo_peq_band(xo)
                    if hi <= lo:                    # XO PEQ 끔(10-09 사용자: XO PEQ 를 따로 정하게)
                        continue
                    by_pos = {p: self.H[p][role] * 10 ** ((ref_level - levels[role]) / 20)
                              for p in self.positions if role in self.H[p]}
                    self.top_peq[role] = self._xo_target_peq(by_pos, ref_level, lo, hi, 6, min(6.0, self.max_boost_db), f_max=hi)
                    P = peq_response(self.f, self.top_peq[role])
                    for p in self.positions:
                        if role in self.H[p]:
                            self.H[p][role] = self.H[p][role] * P
            if manual is None or not manual['fixed']:
                lo = -(self.max_latency_ms - self.lift_ms) - earliest       # 모든 딜레이 ≥ 0, 전체 지연 ≤ 예산
                bus_d, bus_pol = self._bus_delay(sub_sum_by_pos, xo_of, lo=float(np.ceil(lo * 4) / 4))
            # 고정 서브가 있으면 버스 지연·극성을 찾지 않는다 — 고정 서브가 서브·메인 시간 관계를 정하고, 자동·묶음은
            # BA 에서 고정 서브에 맞췄다(R27).
            sub_params = [(g + 20 * np.log10(bus_gain), d + bus_d, pol * bus_pol, aps) for g, d, pol, aps in sub_params]
        self.sub_state = {'params': sub_params, 'bus_gain': bus_gain if self.subs else 1.0, 'bus_d': bus_d,
                          'bus_pol': bus_pol, 'bus_P': peq_response(self.f, self.sub_bus_peq) if self.sub_bus_peq else None}
        self.ref_level = ref_level
        channels, before, after, combined_ref = {}, {}, {}, {}
        for k_role, role in enumerate([] if subs_only else self.mains):
            self._step(1 + k_role)
            if role not in self.H[self.main_position]:
                continue
            gain_db = ref_level - levels[role]
            xo = xo_of.get(role)
            comb = {}
            for pos in self.positions:
                if role not in self.H[pos]:
                    continue
                main = self.H[pos][role] * self._delay(self.align_delay_ms[role]) * 10 ** (gain_db / 20)
                if xo and sub_sum_by_pos and pos in sub_sum_by_pos:
                    sub = sub_sum_by_pos[pos] * bus_pol * self._delay(bus_d) * lr4(self.f, xo, 'LP')
                    comb[pos] = main * lr4(self.f, xo, 'HP') + sub
                else:
                    comb[pos] = main
            if self.phase_frac > 6.0:
                # 위상 해상도 두 가지(촘촘 · 1/6 oct)로 만들고 청취 위치 1/24 oct 초과 GD 봉우리가 작은 쪽(09-26: 돌비 11채널 중
                # 8채널은 1/24 가 낫고 R·Lb 는 1/6 이 나았다 — 30 ms 안에서 부분 보정이 옆에 새 봉우리를 만든 경우).
                frac0, best = self.phase_frac, None
                for fr in (frac0, 6.0):
                    self.phase_frac = fr
                    cand = self._correction(role, comb, ref_level)
                    g = np.geomspace(max(self.rc_lo, 20.0), self.rc_hi, 1200)
                    gd = gd_curve(comb[self.main_position] * cand[1], self.f, g)
                    peak = float(np.max(gd - np.median(gd)))
                    if best is None or peak < best[0]:
                        best = (peak, fr, cand, self.peq[role], self.phase_strength[role], self.phase_band[role])
                self.phase_frac = frac0
                _pk, fr, (fir, F), self.peq[role], self.phase_strength[role], self.phase_band[role] = best
                self.phase_frac_used = getattr(self, 'phase_frac_used', {})
                self.phase_frac_used[role] = fr
            else:
                fir, F = self._correction(role, comb, ref_level)
            if self.gd_cuts:
                cuts = fit_gd_cuts(self.f, comb[self.main_position] * F, max(self.rc_lo, 20.0), self.rc_hi)
                if cuts:
                    F = F * peq_response(self.f, cuts)
                    self.peq[role] = list(self.peq.get(role, [])) + cuts
                    self.notes.append(f'{role} GD 컷 {[(round(a), round(q, 1), round(g, 1)) for a, q, g in cuts]}')
            self.chain[role] = {'gain_db': gain_db, 'xo': xo, 'F': F,
                                'top_P': peq_response(self.f, self.top_peq[role]) if role in self.top_peq else None}
            channels[role] = ChannelOut(role, fir, gain_db, self.align_delay_ms[role], xo, self.peq.get(role, []))
            before[role] = self.H0[self.main_position][role]
            combined_ref[role] = comb[self.main_position]
            after[role] = comb[self.main_position] * F
        if manual is not None and manual['fixed']:
            # R27: 고정 서브는 DSP 출력 딜레이 그대로 나가야 한다 — 메인 추가 지연은 엔진이 정하지 않고 설정 값(내부 시간축이
            # 이미 그만큼 당겨져 있고, 자동·묶음 서브는 그 아래로 못 가게 풀었다).
            shift = self.sub_main_delay_ms
        else:
            shift = max(0.0, -min([d for _g, d, _p, _a in sub_params], default=0.0))
        for c in channels.values():
            c.delay_ms += shift
        self.latency_ms = self.lift_ms + shift
        self.main_extra_ms = shift                  # 메인 추가 지연 — [결과 값 가져오기](R27)가 고정 값과 함께 옮긴다
        self.notes.append(f'전체 지연 {self.latency_ms:.1f} ms (FIR {self.lift_ms:.1f} + 메인 추가 {shift:.2f})')
        subs = {r: SubAlign(r, float(g), float(d) + shift, int(pol), [(float(a), float(b)) for a, b in aps], self.sub_mode(r))
                for r, (g, d, pol, aps) in zip(self.subs, sub_params)}
        for r in (manual or {}).get('fixed', []):
            g, d, pol, aps = self.manual_value(r)       # 넣은 값 그대로(내부 축으로 갔다 오며 생기는 반올림도 없게)
            subs[r] = SubAlign(r, g, d, pol, aps, 'fixed')
        for r, sf in self.sub_filters.items():
            subs[r].hp_hz, subs[r].lp_hz, subs[r].peq = sf['hp'], sf['lp'], list(sf['peq'])
            subs[r].hp_type, subs[r].lp_type = sf['hp_type'], sf['lp_type']
        tgt = self.target_db_fn(np.maximum(self.f, 1.0)) + ref_level
        return CorrectionResult(channels, subs, dict(self.xo), bus_d, bus_pol, self.lift_ms, self.f, before, after,
                           combined_ref, sub_sum_by_pos.get(self.main_position) if sub_sum_by_pos else None,
                           tgt, self.notes + [f'위상 보정 세기 {self.phase_strength}', f'위상 보정 시작 Hz {self.phase_band}'])
