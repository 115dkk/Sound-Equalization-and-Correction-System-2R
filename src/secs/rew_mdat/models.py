# -*- coding: utf-8 -*-
"""REW .mdat 파싱 결과를 담는 데이터 클래스 — `secs.rew_mdat.rew.adapters.adapt_stream` 이
만들고 `secs.mdat_loader.load_mdat_project()` 가 읽는다. `FieldMeaning` 은 각 필드의
의미가 REW 소스로 확인된 것인지('confirmed') 추정인지('inferred')를 남겨, 다음에 이
파서를 고칠 사람이 어디를 더 검증해야 하는지 알 수 있게 한다.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import Any, Literal
import numpy as np
import numpy.typing as npt
FloatArray = npt.NDArray[np.floating[Any]]
FieldStatus = Literal['confirmed', 'inferred', 'unknown']
@dataclass(frozen=True, slots=True)
class FieldMeaning:
    status: FieldStatus
    source_field: str | None = None
    note: str | None = None
@dataclass(frozen=True, slots=True)
class RewVersion:
    major: int
    minor: int
    suffix: str
@dataclass(frozen=True, slots=True)
class RoomDimensions:
    length_m: float
    width_m: float
    height_m: float
@dataclass(frozen=True, slots=True)
class FrequencySpacing:
    count: int
    start_hz: float
    end_hz: float
    step_hz: float
    is_log_spaced: bool
    points_per_octave: int
    valid_start_hz: float
    valid_end_hz: float
    def frequencies(self) -> npt.NDArray[np.float64]:
        if self.is_log_spaced:
            if self.count <= 1:
                return np.asarray([self.start_hz], dtype=np.float64)
            else:
                return np.geomspace(self.start_hz, self.end_hz, self.count)
        else:
            return self.start_hz + np.arange(self.count, dtype=np.float64) * self.step_hz
@dataclass(slots=True)
class FrequencyResponse:
    spacing: FrequencySpacing
    spl_db: FloatArray
    phase_deg: FloatArray
    unwrapped_phase_deg: FloatArray
    group_delay_s: FloatArray
    meanings: dict[str, FieldMeaning] = field(default_factory=dict)
    @property
    def frequency_hz(self) -> npt.NDArray[np.float64]:
        return self.spacing.frequencies()
@dataclass(slots=True)
class ImpulseResponse:
    samples: FloatArray
    sample_interval_s: float
    start_time_s: float
    measurement_delay_s: float
    spl_offset_db: float
    peak_index: int
    meanings: dict[str, FieldMeaning] = field(default_factory=dict)
    @property
    def sample_rate_hz(self) -> float:
        return 1.0 / self.sample_interval_s
    @property
    def time_s(self) -> npt.NDArray[np.float64]:
        return np.asarray(self.start_time_s + np.arange(self.samples.size) * self.sample_interval_s, dtype=np.float64)
@dataclass(slots=True)
class DistortionDataset:
    frequency_hz: FloatArray
    harmonic_arrays: list[FloatArray]
    points_per_octave: int
    noise_floor_spl_db: FloatArray | None
    meanings: dict[str, FieldMeaning] = field(default_factory=dict)
    unknown_fields: dict[str, Any] = field(default_factory=dict)
@dataclass(slots=True)
class Measurement:
    name: str
    sample_rate_hz: int
    rew_version: RewVersion
    room_dimensions: RoomDimensions
    frequency_response: FrequencyResponse
    impulse_response: ImpulseResponse
    distortion: DistortionDataset | None
    notes: str
    unknown_fields: dict[str, Any] = field(default_factory=dict)
@dataclass(slots=True)
class MdatFile:
    title: str
    rew_version_major: int
    rew_version_minor: int
    notes: str
    preview_count: int
    measurements: list[Measurement]
    bytes_consumed: int
    file_size: int
    top_level_count: int
    handle_count: int
    raw_top_level: list[Any] = field(repr=False)
    unknown_top_level: list[Any] = field(default_factory=list, repr=False)
    @property
    def trailing_bytes(self) -> int:
        return self.file_size - self.bytes_consumed
