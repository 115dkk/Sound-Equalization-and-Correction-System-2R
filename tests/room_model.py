"""직육면체 모달 룸 모델 — P4 저역 통합 엔진의 합성 시험장.

`실험/fir_taps_vs_decay2.py`(2026-09-15)의 모델을 그대로 옮겼다: 6.2×4.4×2.7 m,
모드 합(≤ 260 Hz, 164개), 모드 Q 8, 룸 성분은 청취 위치 30~150 Hz 리플이 14 dB 를
넘도록 스케일을 고른다(실제 방 수준). 소스·수음 좌표 → 복소 전달함수 / IR.
"""
from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

SPEED_OF_SOUND = 343.0

#: 실험의 배치. 주 스피커 L(앞 왼쪽), 서포트 후보 6개, 청취 위치 5곳(첫 번째가 MLP).
PRIMARY = (1.6, 3.9, 1.0)
SUPPORTS = ((1.2, 0.5, 0.4), (5.0, 0.5, 0.4), (0.4, 2.2, 0.5),
            (5.8, 2.2, 0.5), (4.6, 3.9, 1.0), (3.1, 0.3, 0.4))
POSITIONS = ((3.1, 1.9, 1.15), (2.6, 1.9, 1.15), (3.6, 1.9, 1.15),
             (3.1, 2.4, 1.15), (3.1, 1.4, 1.15))


@dataclass
class ModalRoom:
    dims: tuple[float, float, float] = (6.2, 4.4, 2.7)
    q: float = 8.0
    sample_rate: int = 1200              # 실험과 같은 저속 격자(48 kHz 의 1/40)
    n_fft: int = 8192
    max_mode_hz: float = 260.0
    room_scale: float | None = None      # None 이면 실험처럼 리플 14 dB 가 되는 값을 고른다
    modes: list[tuple[int, int, int, float]] = field(default_factory=list, init=False)

    def __post_init__(self) -> None:
        lx, ly, lz = self.dims
        for nx in range(8):
            for ny in range(6):
                for nz in range(4):
                    hz = SPEED_OF_SOUND / 2 * np.sqrt((nx / lx) ** 2 + (ny / ly) ** 2
                                                      + (nz / lz) ** 2)
                    if 0 < hz <= self.max_mode_hz:
                        self.modes.append((nx, ny, nz, float(hz)))
        if self.room_scale is None:
            self.room_scale = self._calibrated_scale()

    @property
    def freqs(self) -> np.ndarray:
        return np.fft.rfftfreq(self.n_fft, 1.0 / self.sample_rate)

    def _modal(self, source, receiver) -> np.ndarray:
        omega = 2 * np.pi * self.freqs
        total = np.zeros_like(omega, dtype=complex)
        for nx, ny, nz, hz in self.modes:
            shape = 1.0
            for n, s, r, length in zip((nx, ny, nz), source, receiver, self.dims):
                shape *= np.cos(n * np.pi * s / length) * np.cos(n * np.pi * r / length)
            weight = (2 if nx else 1) * (2 if ny else 1) * (2 if nz else 1)
            mode_w = 2 * np.pi * hz
            total += weight * shape / (mode_w ** 2 - omega ** 2 + 1j * omega * mode_w / self.q)
        return total

    def direct(self, source, receiver) -> np.ndarray:
        """방 없는 직접음 — 거리 지연과 1/r."""
        distance = float(np.linalg.norm(np.subtract(source, receiver)))
        return np.exp(-1j * 2 * np.pi * self.freqs * distance / SPEED_OF_SOUND) / distance

    def transfer(self, source, receiver) -> np.ndarray:
        """소스 → 수음 복소 전달함수(모드 합 + 직접음)."""
        return self.room_scale * self._modal(source, receiver) + self.direct(source, receiver)

    def ir(self, source, receiver) -> np.ndarray:
        return np.fft.irfft(self.transfer(source, receiver), self.n_fft)

    def _calibrated_scale(self) -> float:
        band = (self.freqs >= 30) & (self.freqs <= 150)
        modal = self._modal(PRIMARY, POSITIONS[0])
        direct = self.direct(PRIMARY, POSITIONS[0])
        for scale in (1e3, 2e3, 4e3, 8e3, 1.6e4, 3.2e4):
            magnitude = 20 * np.log10(np.abs(scale * modal + direct)[band])
            if np.percentile(magnitude, 95) - np.percentile(magnitude, 5) >= 14:
                return scale
        return 3.2e4

    # 묶음 도우미 ─ [위치, bin]
    def stack(self, source, positions=POSITIONS) -> np.ndarray:
        return np.array([self.transfer(source, p) for p in positions])

    def stack_direct(self, source, positions=POSITIONS) -> np.ndarray:
        return np.array([self.direct(source, p) for p in positions])
