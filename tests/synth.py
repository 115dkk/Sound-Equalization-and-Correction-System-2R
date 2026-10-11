"""결정적 합성 임펄스 응답 생성기.

48 kHz / 1 s. 검토 기록 2026-09-15 의 케이스(45 Hz Q12, 110 Hz Q8 공진)를 재현한다.
공진은 2차 IIR(peaking biquad) 을 임펄스에 적용해 만든다.

- L : 직접음 임펄스 → 45 Hz Q12 +12 dB, 110 Hz Q8 +9 dB
- R : L 과 같은 구성에서 공진 −1 dB, 직접음 0.92, 0.3 ms 지연
- S1: 20~120 Hz 대역통과 → 80 Hz Q6 +12 dB, 2.0 ms 지연
- S2: 같은 구성에서 공진 −1.5 dB, 진폭 0.85, 3.5 ms 지연

난수(바닥 잡음 −80 dBFS)는 시드 고정. 같은 인자 → 같은 배열(비트 동일).
"""
from __future__ import annotations

import math

import numpy as np
from scipy.signal import butter, lfilter, sosfilt

SAMPLE_RATE = 48000
DURATION_S = 1.0
SEED = 20260915
NOISE_DB = -80.0

MAIN_RESONANCES = ((45.0, 12.0, 12.0), (110.0, 8.0, 9.0))  # (Hz, Q, gain dB)
SUB_RESONANCES = ((80.0, 6.0, 12.0),)
SUB_BAND_HZ = (20.0, 120.0)


def _peaking_biquad(sample_rate: int, frequency_hz: float, q: float,
                    gain_db: float) -> tuple[np.ndarray, np.ndarray]:
    """RBJ peaking EQ 2차 IIR 계수."""
    amplitude = 10.0 ** (gain_db / 40.0)
    w0 = 2.0 * math.pi * frequency_hz / sample_rate
    alpha = math.sin(w0) / (2.0 * q)
    b = np.array([1.0 + alpha * amplitude, -2.0 * math.cos(w0),
                  1.0 - alpha * amplitude])
    a = np.array([1.0 + alpha / amplitude, -2.0 * math.cos(w0),
                  1.0 - alpha / amplitude])
    return b / a[0], a / a[0]


def _apply_resonances(ir: np.ndarray, sample_rate: int,
                      resonances, trim_db: float) -> np.ndarray:
    out = ir
    for frequency_hz, q, gain_db in resonances:
        b, a = _peaking_biquad(sample_rate, frequency_hz, q, gain_db + trim_db)
        out = lfilter(b, a, out)
    return out


def _delay(ir: np.ndarray, sample_rate: int, delay_ms: float) -> np.ndarray:
    shift = int(round(delay_ms * sample_rate / 1000.0))
    if shift <= 0:
        return np.asarray(ir, dtype=np.float64).copy()
    return np.concatenate((np.zeros(shift), np.asarray(ir, dtype=np.float64)))[:len(ir)]


def _noise(length: int, seed: int) -> np.ndarray:
    scale = 10.0 ** (NOISE_DB / 20.0)
    return np.random.default_rng(seed).standard_normal(length) * scale


def main_ir(sample_rate: int = SAMPLE_RATE, duration_s: float = DURATION_S,
            *, direct_amplitude: float = 1.0, resonance_trim_db: float = 0.0,
            delay_ms: float = 0.0, seed: int = SEED) -> np.ndarray:
    """메인 채널(L/R) 합성 IR."""
    length = int(round(sample_rate * duration_s))
    ir = np.zeros(length, dtype=np.float64)
    ir[0] = direct_amplitude
    ir = _apply_resonances(ir, sample_rate, MAIN_RESONANCES, resonance_trim_db)
    ir += _noise(length, seed)
    return _delay(ir, sample_rate, delay_ms)


def sub_ir(sample_rate: int = SAMPLE_RATE, duration_s: float = DURATION_S,
           *, amplitude: float = 1.0, resonance_trim_db: float = 0.0,
           delay_ms: float = 0.0, seed: int = SEED + 1) -> np.ndarray:
    """서브우퍼(S1/S2) 합성 IR: 20~120 Hz 대역통과 + 80 Hz Q6 공진."""
    length = int(round(sample_rate * duration_s))
    impulse = np.zeros(length, dtype=np.float64)
    impulse[0] = 1.0
    sos = butter(4, SUB_BAND_HZ, btype="bandpass", fs=sample_rate, output="sos")
    band = sosfilt(sos, impulse)
    peak = float(np.max(np.abs(band)))
    if peak > 0.0:
        band = band / peak
    ir = _apply_resonances(band * amplitude, sample_rate, SUB_RESONANCES,
                           resonance_trim_db)
    ir += _noise(length, seed)
    return _delay(ir, sample_rate, delay_ms)


def measurement_set(layout_value: str, sample_rate: int = SAMPLE_RATE,
                    duration_s: float = DURATION_S) -> dict[str, tuple[np.ndarray, int]]:
    """레이아웃('2.0'|'2.1'|'2.2') 에 필요한 역할별 (IR, sample_rate) 사전."""
    measurements: dict[str, tuple[np.ndarray, int]] = {
        "L": (main_ir(sample_rate, duration_s), sample_rate),
        "R": (main_ir(sample_rate, duration_s, direct_amplitude=0.92,
                      resonance_trim_db=-1.0, delay_ms=0.3, seed=SEED + 2), sample_rate),
    }
    if layout_value in ("2.1", "2.2"):
        measurements["S1"] = (sub_ir(sample_rate, duration_s, delay_ms=2.0), sample_rate)
    if layout_value == "2.2":
        measurements["S2"] = (sub_ir(sample_rate, duration_s, amplitude=0.85,
                                     resonance_trim_db=-1.5, delay_ms=3.5,
                                     seed=SEED + 3), sample_rate)
    return measurements


# ──────────────────────────────────────────────────────────────────────
# P1 채널 일반화용 확장 (메인 G개 · 서브 K개)
#
# 위의 main_ir / sub_ir / measurement_set 의 출력은 골든 29건의 입력이므로
# 절대 바꾸지 않는다. 아래는 순수 추가분이다.
#
# 인덱스 0·1 의 파라미터는 골든과 같은 값으로 못 박아 두었다. 그래서
#   channel_measurement_set(("L", "R"), ("S1", "S2")) == measurement_set("2.2")
# 가 비트 단위로 성립한다(test_channels.py 가 이 등식을 검사한다).
# ──────────────────────────────────────────────────────────────────────

#: 목업 ROLE_ORDER 의 메인/탑 역할(서브 제외) 순서. 10-10 에 더한 하이트(H*)는 없다 — FakeREW 가 자체 합성한다.
MAIN_ROLE_ORDER: tuple[str, ...] = (
    "L", "R", "C", "Ls", "Rs", "Lb", "Rb", "Lw", "Rw",
    "Tfl", "Tfr", "Tml", "Tmr", "Tbl", "Tbr",
)
#: 목업 SUB_ROLES.
SUB_ROLE_ORDER: tuple[str, ...] = tuple(f"SW{i}" for i in range(1, 9))


def main_ir_params(index: int) -> dict[str, float | int]:
    """메인 채널 index(0부터) 의 합성 파라미터. 0·1 은 골든의 L·R 과 동일."""
    if index == 0:
        return {"direct_amplitude": 1.0, "resonance_trim_db": 0.0,
                "delay_ms": 0.0, "seed": SEED}
    if index == 1:
        return {"direct_amplitude": 0.92, "resonance_trim_db": -1.0,
                "delay_ms": 0.3, "seed": SEED + 2}
    step = index - 1
    return {"direct_amplitude": round(0.92 - 0.02 * step, 6),
            "resonance_trim_db": round(-1.0 - 0.25 * step, 6),
            "delay_ms": round(0.3 + 0.12 * step, 6),
            "seed": SEED + 1000 + index}


def sub_ir_params(index: int) -> dict[str, float | int]:
    """서브 index(0부터) 의 합성 파라미터. 0·1 은 골든의 S1·S2 와 동일."""
    if index == 0:
        return {"amplitude": 1.0, "resonance_trim_db": 0.0,
                "delay_ms": 2.0, "seed": SEED + 1}
    if index == 1:
        return {"amplitude": 0.85, "resonance_trim_db": -1.5,
                "delay_ms": 3.5, "seed": SEED + 3}
    step = index - 1
    return {"amplitude": round(0.85 - 0.05 * step, 6),
            "resonance_trim_db": round(-1.5 - 0.3 * step, 6),
            "delay_ms": round(3.5 + 1.2 * step, 6),
            "seed": SEED + 2000 + index}


def channel_measurement_set(main_ids, sub_ids=(),
                            sample_rate: int = SAMPLE_RATE,
                            duration_s: float = DURATION_S
                            ) -> dict[str, tuple[np.ndarray, int]]:
    """임의 개수의 메인·서브 채널 id 에 대한 (IR, sample_rate) 사전.

    main_ids·sub_ids 는 채널 id 의 순서열(예: ('L','R','C','Ls','Rs'),
    ('SW1',)). 같은 인자에 항상 같은 배열을 돌려준다(비트 동일).
    """
    main_ids = tuple(main_ids)
    sub_ids = tuple(sub_ids)
    duplicates = [cid for cid in set(main_ids + sub_ids)
                  if (main_ids + sub_ids).count(cid) > 1]
    if duplicates:
        raise ValueError(f"채널 id 중복: {sorted(duplicates)}")

    measurements: dict[str, tuple[np.ndarray, int]] = {}
    for index, channel_id in enumerate(main_ids):
        measurements[channel_id] = (
            main_ir(sample_rate, duration_s, **main_ir_params(index)), sample_rate)
    for index, channel_id in enumerate(sub_ids):
        measurements[channel_id] = (
            sub_ir(sample_rate, duration_s, **sub_ir_params(index)), sample_rate)
    return measurements


def role_ids(main_count: int, sub_count: int) -> tuple[tuple[str, ...], tuple[str, ...]]:
    """메인 G개·서브 K개에 쓸 역할 id 를 목업 순서에서 잘라 온다."""
    if not 0 <= main_count <= len(MAIN_ROLE_ORDER):
        raise ValueError(f"메인 채널 수 범위 밖: {main_count}")
    if not 0 <= sub_count <= len(SUB_ROLE_ORDER):
        raise ValueError(f"서브 채널 수 범위 밖: {sub_count}")
    return MAIN_ROLE_ORDER[:main_count], SUB_ROLE_ORDER[:sub_count]


def role_measurement_set(main_count: int, sub_count: int,
                         sample_rate: int = SAMPLE_RATE,
                         duration_s: float = DURATION_S
                         ) -> dict[str, tuple[np.ndarray, int]]:
    """메인 G개·서브 K개의 합성 측정 세트(역할 id 는 목업 순서)."""
    main_ids, sub_ids = role_ids(main_count, sub_count)
    return channel_measurement_set(main_ids, sub_ids, sample_rate, duration_s)


# ──────────────────────────────────────────────────────────────────────
# P3 검증용 추가 (트랙 E) — "알려진 채널 지연" 의 단일 출처
#
# 위의 생성 함수는 건드리지 않는다(골든 29건의 입력). 아래는 순수 조회·계산분.
#
# `secs.rewapi.fake.FakeREW` 는 역할 하나를 측정할 때
#   role 이 MAIN_ROLE_ORDER[i] → role_measurement_set(i + 1, 0)[role]
#   role 이 SUB_ROLE_ORDER[i]  → role_measurement_set(0, i + 1)[role]
# 를 쓴다. 즉 역할별 IR 은 main_ir_params(i) / sub_ir_params(i) 가 전부 결정한다.
# 'L:LF' 처럼 대역이 붙은 role 은 split_role() 로 잘려 같은 기본 IR 을 받는다.
# 위치 변이(_position_variation) 는 위치 1 에서 항등이므로, 정렬 기준인
# 위치 1 의 IR = 아래 함수들이 돌려주는 값 그대로다.
# ──────────────────────────────────────────────────────────────────────

#: 채널 정렬의 도착시간 판정 기준(P3 계약: |ir| 이 최대의 −20 dB 를 처음 넘는 샘플).
ONSET_THRESHOLD_DB = -20.0

#: 음속 (P3 계약: 거리 = 343 m/s × 도착시간).
SPEED_OF_SOUND_MS = 343.0


def base_role(role: str) -> str:
    """'L:LF' → 'L'. 대역이 없으면 그대로."""
    return str(role).split(':', 1)[0]


def role_ir_params(role: str) -> dict[str, float | int]:
    """역할 하나의 합성 파라미터(대역 접미사는 무시)."""
    name = base_role(role)
    if name in MAIN_ROLE_ORDER:
        return main_ir_params(MAIN_ROLE_ORDER.index(name))
    if name in SUB_ROLE_ORDER:
        return sub_ir_params(SUB_ROLE_ORDER.index(name))
    raise KeyError(f'알 수 없는 역할: {role!r}')


def quantize_delay_ms(delay_ms: float, sample_rate: int = SAMPLE_RATE) -> float:
    """`_delay` 가 실제로 넣는 지연(정수 샘플로 반올림된 값)."""
    return int(round(float(delay_ms) * sample_rate / 1000.0)) * 1000.0 / sample_rate


def role_delay_ms(role: str, sample_rate: int = SAMPLE_RATE) -> float:
    """역할의 **공칭** 지연(샘플 격자로 반올림). 합성 파라미터에서 바로 온다.

    메인은 직접음이 임펄스 하나라 이 값이 곧 도착시간이지만, 서브는 20~120 Hz
    대역통과의 GD가 얹혀 도착시간이 이보다 늦다. 서브의 실제 도착시간은
    `role_onset_ms` 를 쓸 것.
    """
    return quantize_delay_ms(float(role_ir_params(role)['delay_ms']), sample_rate)


def role_direct_level_db(role: str) -> float:
    """메인 역할의 직접음 레벨(dB, L=0 dB 기준). 서브는 정의하지 않는다."""
    name = base_role(role)
    if name not in MAIN_ROLE_ORDER:
        raise KeyError(f'메인 역할이 아닙니다: {role!r}')
    amplitude = float(role_ir_params(name)['direct_amplitude'])
    return 20.0 * math.log10(amplitude)


def onset_index(ir, threshold_db: float = ONSET_THRESHOLD_DB) -> int:
    """|ir| 이 최대의 `threshold_db` 를 처음 넘는 샘플 번호."""
    magnitude = np.abs(np.asarray(ir, dtype=np.float64))
    peak = float(magnitude.max()) if magnitude.size else 0.0
    if peak <= 0.0:
        return 0
    crossing = magnitude >= peak * (10.0 ** (threshold_db / 20.0))
    return int(np.argmax(crossing)) if bool(crossing.any()) else 0


def onset_ms(ir, sample_rate: int = SAMPLE_RATE,
             threshold_db: float = ONSET_THRESHOLD_DB) -> float:
    """`onset_index` 를 ms 로."""
    return onset_index(ir, threshold_db) * 1000.0 / sample_rate


def role_onset_ms(role: str, sample_rate: int = SAMPLE_RATE,
                  duration_s: float = DURATION_S,
                  threshold_db: float = ONSET_THRESHOLD_DB) -> float:
    """역할의 **실측** 도착시간 — 합성 IR 에 −20 dB 규칙을 직접 적용한 값.

    FakeREW 와 같은 float32 로 맞춘다. 메인은 `role_delay_ms` 와 한 샘플 안에서
    같고, 서브는 대역통과 GD만큼 늦다.
    """
    name = base_role(role)
    if name in MAIN_ROLE_ORDER:
        table = role_measurement_set(MAIN_ROLE_ORDER.index(name) + 1, 0,
                                     sample_rate, duration_s)
    elif name in SUB_ROLE_ORDER:
        table = role_measurement_set(0, SUB_ROLE_ORDER.index(name) + 1,
                                     sample_rate, duration_s)
    else:
        raise KeyError(f'알 수 없는 역할: {role!r}')
    return onset_ms(np.asarray(table[name][0], dtype=np.float32),
                    sample_rate, threshold_db)
