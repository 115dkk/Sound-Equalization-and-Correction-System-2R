# -*- coding: utf-8 -*-
"""합성 다점 **감쇠** 방 — 개정 8(6·7단계 재편) 전후 비교용 시험장(2026-09-24).

`tests/room_model.py` 의 `ModalRoom`(6.2×4.4×2.7 m, 모드 합 ≤ 260 Hz)을 넓힌 것. 기존 파일은 그대로 둔다.
- **모드별 Q**: 기본 8, 축 모드 넷은 Q 20~30(긴 감쇠). T60 ≈ 2.2·Q/f 라 27.7 Hz(Q 30)는 2.4 s 운다.
- 스피커 자체 응답: 메인 2차 HP 45 Hz(탑 60 Hz), 서브 4차 LP 160 Hz + 2차 HP 20 Hz.
- 구성 `'2.2'`(L·R·SW1·SW2) / `'5.1.2'`(L·R·C·Ls·Rs·Tfl·Tfr + SW1·SW2), 청취 위치 5(첫 번째가 청취 위치).
  서포트 = 서브 2 + 가까운 메인(2.2 는 입력당 3, 5.1.2 는 `maxPer` 5). 7단계 방식은 하이브리드(감쇠 보호 켬).
- `build_case()` → `SynthCase`: 엔진이 받는 `Project` + `irs[pos][role] = (ir 48 kHz, 48000, 0.0)`, 그리고 같은 모델의
  저속 격자(1200 Hz · 8192점) 복소 전달함수 `low[role][pos, bin]`.
- `mode_t60()`: 모드 하나의 T60(대역 통과 → Schroeder 적분 → −5~−25 dB 직선 맞춤을 −60 dB 로 외삽).

지금은 보정 설계(새 엔진·MIMO) 테스트와 도구가 합성 방으로 쓴다. 옛 6·7단계 엔진을 시트 그대로 방에 걸어 재던
지표부(`chain_response`·`predict`·`metrics`·`evaluate`)는 2026-10-10 슬림화로 그 엔진과 함께 지웠다.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from functools import lru_cache

import numpy as np

from room_model import POSITIONS, SPEED_OF_SOUND, ModalRoom

#: 긴 감쇠 축 모드(nx, ny, nz) → Q. 6.2×4.4×2.7 m: 27.7 / 39.0 / 55.3 / 63.5 Hz.
LONG_Q: dict[tuple[int, int, int], float] = {(1, 0, 0): 30.0, (0, 1, 0): 25.0, (2, 0, 0): 22.0, (0, 0, 1): 20.0}
SAMPLE_RATE = 48000
N_FFT = 1 << 17                       # 2.73 s — Q 30 · 27.7 Hz 의 T60 2.4 s 가 감기지 않는다
LOW_RATE = 1200
LOW_N_FFT = 8192
MODE_BAND_HZ = 3.0                    # T60 대역 통과 반폭
T60_FIT_DB = (-5.0, -25.0)
STRONG_MODES = 6
STRONG_MODE_MAX_HZ = 150.0

#: 역할의 방 안 좌표(m). 앞벽 y = 4.4, 청취 위치는 y = 1.9 줄(`room_model.POSITIONS`).
ROLE_SOURCES: dict[str, tuple[float, float, float]] = {
    'L': (1.6, 3.9, 1.0), 'R': (4.6, 3.9, 1.0), 'C': (3.1, 4.1, 0.9),
    'Ls': (0.3, 1.6, 1.1), 'Rs': (5.9, 1.6, 1.1),
    'Tfl': (2.0, 3.0, 2.6), 'Tfr': (4.2, 3.0, 2.6),
    'SW1': (0.5, 4.1, 0.4), 'SW2': (5.8, 0.4, 0.4),
}
LAYOUTS = ('2.2', '5.1.2')
XO_HZ = 80.0


# ──────────────────────────── 방 ────────────────────────────
@dataclass
class DecayRoom(ModalRoom):
    """모드마다 Q 가 다른 `ModalRoom`. 모드 합은 행렬곱으로 여러 소스·수음 쌍을 한 번에 낸다."""

    long_q: dict = field(default_factory=lambda: dict(LONG_Q))

    def q_of(self, mode: tuple[int, int, int, float]) -> float:
        return float(self.long_q.get(tuple(mode[:3]), self.q))

    def _coefficients(self, pairs) -> np.ndarray:
        """[쌍, 모드] = 가중 × 소스 모양 × 수음 모양."""
        out = np.empty((len(pairs), len(self.modes)))
        for m, (nx, ny, nz, _hz) in enumerate(self.modes):
            weight = (2 if nx else 1) * (2 if ny else 1) * (2 if nz else 1)
            for i, (source, receiver) in enumerate(pairs):
                shape = 1.0
                for n, s, r, length in zip((nx, ny, nz), source, receiver, self.dims):
                    shape *= np.cos(n * np.pi * s / length) * np.cos(n * np.pi * r / length)
                out[i, m] = weight * shape
        return out

    def _modal_many(self, pairs) -> np.ndarray:
        omega = 2 * np.pi * self.freqs
        coeff = self._coefficients(pairs)
        mode_w = np.array([2 * np.pi * m[3] for m in self.modes])[:, None]
        qs = np.array([self.q_of(m) for m in self.modes])[:, None]
        out = np.empty((len(pairs), omega.size), dtype=complex)
        chunk = 8192
        for lo in range(0, omega.size, chunk):
            w = omega[None, lo:lo + chunk]
            out[:, lo:lo + chunk] = coeff @ (1.0 / (mode_w ** 2 - w ** 2 + 1j * w * mode_w / qs))
        return out

    def _modal(self, source, receiver) -> np.ndarray:
        return self._modal_many([(source, receiver)])[0]

    def transfer_many(self, pairs) -> np.ndarray:
        modal = self._modal_many(pairs)
        direct = np.array([self.direct(s, r) for s, r in pairs])
        return self.room_scale * modal + direct

    def mode_strength(self, source, receiver, max_hz: float = STRONG_MODE_MAX_HZ) -> list[tuple[float, float]]:
        """(모드 Hz, 공진점 크기) — 공진에서 |가중·모양|·Q/ω². 센 순."""
        coeff = self._coefficients([(source, receiver)])[0]
        rows = [(m[3], abs(c) * self.q_of(m) / (2 * np.pi * m[3]) ** 2)
                for c, m in zip(coeff, self.modes) if m[3] <= max_hz]
        return sorted(rows, key=lambda r: -r[1])


@lru_cache(maxsize=None)
def calibrated_scale() -> float:
    """저속 격자에서 `ModalRoom` 과 같은 규칙(청취 위치 리플 14 dB)으로 고른 룸 성분 배율."""
    return float(DecayRoom(sample_rate=LOW_RATE, n_fft=LOW_N_FFT).room_scale)


def room(sample_rate: int = SAMPLE_RATE, n_fft: int = N_FFT) -> DecayRoom:
    return DecayRoom(sample_rate=sample_rate, n_fft=n_fft, room_scale=calibrated_scale())


def speaker_response(role: str, freqs: np.ndarray) -> np.ndarray:
    """스피커 자체의 저역 한계(아날로그 원형). 메인 2차 HP 45 Hz(탑 60), 서브 4차 LP 160 + 2차 HP 20."""
    f = np.asarray(freqs, dtype=float)

    def hp2(corner):
        s = 1j * f / corner
        return s * s / (s * s + np.sqrt(2.0) * s + 1.0)

    def lp2(corner, q=np.sqrt(0.5)):
        s = 1j * f / corner
        return 1.0 / (s * s + s / q + 1.0)

    if role.startswith('SW'):
        return lp2(160.0, 0.541) * lp2(160.0, 1.307) * hp2(20.0)
    return hp2(60.0 if role.startswith('T') else 45.0)


# ──────────────────────────── 구성 ────────────────────────────
@dataclass
class SynthCase:
    layout: str
    project: object
    irs: dict                                   # [pos][role] = (ir, 48000, 0.0)
    low: dict                                   # role → [pos, bin] 저속 격자 복소 전달함수
    low_freqs: np.ndarray
    roles: list[str]
    inputs: list[str]
    strong_modes: dict                          # input → [Hz, …] (청취 위치에서 센 순 6개, 오름차순 정렬)


def make_project(layout: str, support_mode: str = 'hybrid'):
    import secs.model as model

    if layout == '2.2':
        config = model.SpeakerConfig.from_preset('2.2', biamp=False)
    elif layout == '5.1.2':
        config = model.SpeakerConfig.from_preset('5.1', biamp=False)
        for role in ('Tfl', 'Tfr'):
            config.speakers.append(model.Speaker(role=role, size='small',
                                                 bands=model.default_bands(role, config.next_free_out(), False)))
        config.set_sub_count(2)
    else:
        raise KeyError(layout)
    project = model.Project(name=f'합성 감쇠 {layout}', config=config)
    project.dsp = model.DspProfile.presets()['tesira']
    project.dsp.support_mode = support_mode
    for group in {model.role_group(r) for r in config.roles() if not r.startswith('SW')}:
        project.xo[group] = XO_HZ
    project.mso['maxPer'] = 3 if layout == '2.2' else 5
    project.mso['decay'] = True
    return project


def build_case(layout: str, support_mode: str = 'hybrid', positions: int = 5) -> SynthCase:
    """결정적 합성 구성 하나. 같은 인자 → 같은 배열."""
    project = make_project(layout, support_mode)
    roles = project.config.roles()
    seats = POSITIONS[:positions]
    hi, lo = _transfers(tuple(roles), tuple(seats))
    freqs_hi = np.fft.rfftfreq(N_FFT, 1.0 / SAMPLE_RATE)
    freqs_lo = np.fft.rfftfreq(LOW_N_FFT, 1.0 / LOW_RATE)
    irs: dict = {pos: {} for pos in range(1, len(seats) + 1)}
    low: dict = {}
    for i, role in enumerate(roles):
        rows = []
        for p in range(len(seats)):
            index = i * len(seats) + p
            spectrum = hi[index] * speaker_response(role, freqs_hi)
            irs[p + 1][role] = (np.fft.irfft(spectrum, N_FFT), SAMPLE_RATE, 0.0)
            rows.append(lo[index] * speaker_response(role, freqs_lo))
        low[role] = np.array(rows)
    inputs = [r for r in roles if not r.startswith('SW')]
    geometry = room(LOW_RATE, LOW_N_FFT)
    strong = {}
    for role in inputs:
        top = geometry.mode_strength(ROLE_SOURCES[role], seats[0])[:STRONG_MODES]
        strong[role] = sorted(round(hz, 2) for hz, _s in top)
    return SynthCase(layout, project, irs, low, freqs_lo, roles, inputs, strong)


@lru_cache(maxsize=4)
def _transfers(roles: tuple, seats: tuple) -> tuple[np.ndarray, np.ndarray]:
    pairs = [(ROLE_SOURCES[r], seat) for r in roles for seat in seats]
    return room().transfer_many(pairs), room(LOW_RATE, LOW_N_FFT).transfer_many(pairs)


# ──────────────────────────── 지표 ────────────────────────────
def mode_t60(ir: np.ndarray, rate: float, center_hz: float, half_band: float = MODE_BAND_HZ) -> float | None:
    """모드 하나의 T60(s): ±half_band Hz 대역 통과 → Schroeder 적분 → −5~−25 dB 직선 맞춤을 −60 dB 로 외삽.

    대역 통과는 4차 버터워스를 **시간을 뒤집어** 건다(역방향 필터링 — 필터 자신의 울림이 감쇠 곡선 뒤가 아니라
    앞에 붙어 짧은 T60 을 덜 부풀린다). 영위상 창(FFT)은 느린 모드의 앞 울림이 IR 끝으로 감겨 Schroeder 곡선이
    평평해진다(09-24 시험: T60 170 s 로 나옴). 앞 울림이 들어갈 자리로 2 s 를 앞에 붙인다.
    """
    from scipy.signal import butter, sosfilt

    low, high = max(center_hz - half_band, 1.0), min(center_hz + half_band, rate * 0.45)
    sos = butter(2, [low, high], btype='bandpass', fs=rate, output='sos')
    padded = np.concatenate((np.zeros(int(2.0 * rate)), np.asarray(ir, dtype=float)))
    band = sosfilt(sos, padded[::-1])[::-1]
    energy = band ** 2
    start = int(np.argmax(energy))
    edc = np.cumsum(energy[start:][::-1])[::-1]
    if edc[0] <= 0:
        return None
    edc_db = 10 * np.log10(np.maximum(edc / edc[0], 1e-30))
    hi_i = np.argmax(edc_db <= T60_FIT_DB[0])
    lo_i = np.argmax(edc_db <= T60_FIT_DB[1])
    if lo_i <= hi_i + 2:
        return None
    t = np.arange(hi_i, lo_i) / rate
    slope = np.polyfit(t, edc_db[hi_i:lo_i], 1)[0]
    return float(-60.0 / slope) if slope < 0 else None


__all__ = ['LONG_Q', 'ROLE_SOURCES', 'LAYOUTS', 'DecayRoom', 'room', 'calibrated_scale', 'speaker_response',
           'SynthCase', 'make_project', 'build_case', 'mode_t60']
