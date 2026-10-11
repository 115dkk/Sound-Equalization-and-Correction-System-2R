# -*- coding: utf-8 -*-
"""단일 측정 REW 템플릿을 복제해 새 `.mdat` 프로젝트를 만든다.

`build_mdat()` 가 제목·노트·샘플레이트·float32 IR·시간 원점이 다른 측정들을
`template_1.mdat` 의 ``roomeqwizard.MeasData`` 그래프에 넣고, 직렬화 writer로 파일을
쓴다. 보정 설계·내보내기(`secs.calibration` 의 REW mdat 내보내기)와 프로젝트 저장 때 곁 mdat
(`WizardWindow._write_side_mdat`)이 부른다. 반환값은 쓴 경로다.
"""
from __future__ import annotations

import copy
import struct
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

import numpy as np

from secs.rew_mdat.errors import SchemaError
from secs.rew_mdat.serialization.nodes import BlockData, JavaArray, JavaObject, JavaString
from secs.rew_mdat.serialization.parser import JavaSerializationParser
from secs.rew_mdat.serialization.writer import dumps


DEFAULT_TEMPLATE = Path(__file__).with_name('template_1.mdat')


#: SECS(REW API 단위) IR → mdat IR 배율. `secs.mdat_loader` 는 반대로 곱한다.
REW_MDAT_IR_SCALE = 0.01


@dataclass(frozen=True, slots=True)
class MeasurementInput:
    """새 REW 측정 하나. ``samples`` 는 파일에 float32로 정확히 저장된다."""

    title: str
    notes: str
    sample_rate_hz: int
    samples: np.ndarray
    start_time_s: float = 0.0
    #: dB SPL 오프셋(REW `splOffsetdB`). None = 템플릿 값(120 dB) 그대로 — 실측 IR 은 꼭 넘길 것(10-09 레벨 어긋남).
    spl_offset_db: float | None = None


def _field_owner(obj: JavaObject, name: str) -> dict:
    for descriptor in obj.class_hierarchy(derived_first=True):
        fields = obj.field_data.get(descriptor.name, {})
        if name in fields:
            return fields
    raise SchemaError(f'template object {obj.classdesc.name!r} has no field {name!r}')


def _set(obj: JavaObject, name: str, value) -> None:
    _field_owner(obj, name)[name] = value


def _boxed_set(value, new_value) -> None:
    if not isinstance(value, JavaObject) or not value.classdesc.name.startswith('java.lang.'):
        raise SchemaError('template boxed value is absent')
    _set(value, 'value', new_value)


def _array_set(obj: JavaObject, name: str, values, dtype=np.float32) -> None:
    array = obj.get(name)
    if not isinstance(array, JavaArray):
        raise SchemaError(f'template field {name!r} is not an array')
    array.data = np.ascontiguousarray(values, dtype=dtype)


def _next_fft_size(length: int) -> int:
    size = 1
    while size < max(2, int(length)):
        size <<= 1
    return size


def _frequency_data(samples: np.ndarray, rate: int, spl_offset: float,
                    points_per_octave: int = 96) -> tuple[np.ndarray, ...]:
    """IR에서 REW가 처음 그릴 캐시 배열을 만든다.

    REW의 저장 배열은 화면 캐시라 프로그램 버전에 따라 다시 계산될 수 있다. 그래도
    stale 템플릿 곡선을 보이지 않게 SPL·위상·unwrapped phase·group delay와 raw/FDW
    동형 배열을 모두 같은 IR에서 계산한다. IR 자체가 원본이며 REW는 창/스무딩을 바꾸면
    이것으로 응답을 다시 계산한다.
    """
    low = 10.0
    high = min(20000.0, rate * 0.5 * 0.999)
    count = max(2, int(np.floor(np.log2(high / low) * points_per_octave)) + 1)
    freq = low * np.power(2.0, np.arange(count, dtype=np.float64) / points_per_octave)
    freq = freq[freq <= high]
    size = _next_fft_size(samples.size)
    bins = np.fft.rfftfreq(size, 1.0 / rate)
    spectrum = np.fft.rfft(samples.astype(np.float64, copy=False), size)
    real = np.interp(freq, bins, spectrum.real)
    imag = np.interp(freq, bins, spectrum.imag)
    values = real + 1j * imag
    spl = 20.0 * np.log10(np.maximum(np.abs(values), 1e-30)) + float(spl_offset)
    phase = np.degrees(np.angle(values))
    unwrapped = np.degrees(np.unwrap(np.angle(values)))
    group_delay = -np.gradient(np.radians(unwrapped), 2.0 * np.pi * freq)
    return freq, spl, phase, unwrapped, group_delay, real, imag


def _measurement_object(values: list) -> JavaObject:
    matches = [value for value in values if isinstance(value, JavaObject)
               and value.classdesc.name == 'roomeqwizard.MeasData']
    if not matches:
        raise SchemaError('template has no roomeqwizard.MeasData object')
    return matches[0]


def _preview_pair(values: list) -> tuple[object, object]:
    try:
        count = struct.unpack('>i', values[3].data)[0]
    except (AttributeError, IndexError, struct.error) as error:
        raise SchemaError('template preview header is invalid') from error
    if count < 1 or len(values) < 6:
        raise SchemaError('template needs one preview entry')
    return values[4], values[5]


def _replace_measurement(obj: JavaObject, item: MeasurementInput) -> None:
    # mdat 의 IR 은 REW API(`normalised=false`) 값의 1/100 이다 — 같은 측정을 REW 가 저장한 mdat 과 비교해 정확히 100배(10-09).
    # 이걸 안 하면 SECS 가 쓴 mdat 이 REW 에서 40 dB 높게 보였다(사용자: spl 오프셋도 개판).
    samples = np.ascontiguousarray(np.asarray(item.samples, dtype=np.float64) * REW_MDAT_IR_SCALE,
                                   dtype=np.float32).reshape(-1)
    if not samples.size:
        raise ValueError(f'{item.title}: IR samples are empty')
    rate = int(item.sample_rate_hz)
    if rate <= 0:
        raise ValueError(f'{item.title}: sample rate must be positive')

    _set(obj, 'shortDesc', JavaString(str(item.title)))
    _set(obj, 'measNotes', JavaString(str(item.notes)))
    _set(obj, 'sampleRate', rate)
    _set(obj, 'sourceFileDate', 0)
    source_format = obj.get('sourceFileFormat')
    if isinstance(source_format, JavaString):
        _set(obj, 'sourceFileFormat', JavaString('SECS predicted impulse response'))

    # 복제 측정마다 UUID가 같으면 REW 내부 선택/덮어쓰기에서 충돌할 수 있다.
    uid = uuid.uuid4().int
    uuid_obj = obj.get('uuid')
    if isinstance(uuid_obj, JavaObject):
        high, low = uid >> 64, uid & ((1 << 64) - 1)
        high = high - (1 << 64) if high >= (1 << 63) else high
        low = low - (1 << 64) if low >= (1 << 63) else low
        _set(uuid_obj, 'mostSigBits', high)
        _set(uuid_obj, 'leastSigBits', low)
        uuid2 = obj.get('uuid2')
        if isinstance(uuid2, JavaObject) and isinstance(uuid2.get('value'), JavaArray):
            _array_set(uuid2, 'value', [high, low], dtype=np.int64)

    ir_data = obj.get('irData')
    if not isinstance(ir_data, JavaObject):
        raise SchemaError('template MeasData.irData is absent')
    sampled = ir_data.get('ir')
    if not isinstance(sampled, JavaObject):
        raise SchemaError('template IRData.ir is absent')

    interval = 1.0 / rate
    start = float(item.start_time_s)
    peak = int(np.argmax(np.abs(samples)))
    peak_value = float(abs(samples[peak]))
    end = start + (samples.size - 1) * interval
    _set(sampled, 'T', interval)
    _set(sampled, 'n', int(samples.size))
    _set(sampled, 'startTime', start)
    _set(sampled, 'endTime', end)
    _set(sampled, 'absMaxIndex', peak)
    _set(sampled, 'absMaxInterpIndex', float(peak))
    _set(sampled, 'absMaxVal', peak_value)
    _set(sampled, 'normVal', peak_value)
    _array_set(sampled, 'data', samples)
    for boxed_name in ('absMaxDoubleIndex',):
        boxed = sampled.get(boxed_name)
        if isinstance(boxed, JavaObject):
            _boxed_set(boxed, float(peak))
    _set(ir_data, 'origPeakTime', start + peak * interval)
    windows = ir_data.get('windows')
    if isinstance(windows, JavaObject):
        _set(windows, 'T', interval)
        _set(windows, 'n', int(samples.size))
        _set(windows, 'startTime', start)
        _set(windows, 'winRefIndex', peak)
        _set(windows, 'winRefTime', start + peak * interval)
        _set(windows, 'preImpulseIndex', max(0, peak - min(peak, rate // 3)))
        _set(windows, 'postImpulseIndex', min(samples.size - 1, peak + rate // 2))
        _set(windows, 'preLength', max(1, peak - int(windows.get('preImpulseIndex', 0))))
        _set(windows, 'postLength', max(1, int(windows.get('postImpulseIndex', peak)) - peak + 1))

    if item.spl_offset_db is not None:
        _set(ir_data, 'splOffset', float(item.spl_offset_db))
    spl_offset = float(ir_data.get('splOffset', 0.0) or 0.0)
    freq, spl, phase, unwrapped, gd, real, imag = _frequency_data(samples, rate, spl_offset)
    count = int(freq.size)
    ratio = float(freq[1] / freq[0])
    for name, value in (
        ('dataLength', count), ('sampleRate', rate), ('startFreq', float(freq[0])),
        ('endFreq', float(freq[-1])), ('validStartFreq', float(freq[0])),
        ('validEndFreq', float(freq[-1])), ('freqStep', float(freq[1] - freq[0])),
        ('isLogSpaced', True), ('ppo', 96), ('logStep', ratio),
        ('logStepLog', float(np.log(ratio))),
    ):
        _set(obj, name, value)
    _set(ir_data, 'startFreq', float(freq[0]))
    _set(ir_data, 'endFreq', float(freq[-1]))
    arrays = {
        'splValues': spl, 'rawValues': spl, 'phaseValues': phase,
        'unwPhaseValues': unwrapped, 'rawUnwPhaseValues': unwrapped,
        'gdValues': gd, 'rawGDValues': gd, 'fdwReal': real, 'fdwImag': imag,
        'unwPhaseRefPeak': np.zeros(count, dtype=np.float32),
    }
    for name, data in arrays.items():
        if isinstance(obj.get(name), JavaArray):
            _array_set(obj, name, data)
    # 템플릿 측정의 고조파/노이즈 분석은 새 IR과 무관하므로 보존하지 않는다.
    if 'distortionData' in obj.all_fields():
        _set(obj, 'distortionData', None)


def build_mdat_bytes(measurements: Iterable[MeasurementInput], *,
                     template_path: str | Path = DEFAULT_TEMPLATE, spl_offset_db: float | None = None) -> bytes:
    """측정 N개가 든 REW mdat 바이트를 만든다. `spl_offset_db` = 자기 값이 없는 측정에 넣을 SPL 오프셋(dB)."""
    items = list(measurements)
    if spl_offset_db is not None:
        import dataclasses
        items = [it if it.spl_offset_db is not None else dataclasses.replace(it, spl_offset_db=float(spl_offset_db))
                 for it in items]
    if not items:
        raise ValueError('at least one measurement is required')
    template = Path(template_path)
    parsed = JavaSerializationParser(template.read_bytes()).parse()
    prototype = _measurement_object(parsed.values)
    preview = _preview_pair(parsed.values)

    values = copy.deepcopy(parsed.values[:3])
    values.append(BlockData(struct.pack('>i', len(items))))
    for item in items:
        icon, text = copy.deepcopy(preview)
        values.extend((icon, text))
    # REW V2 mdat 은 각 MeasData 앞에 정수 3, 마지막에 0을 쓴다.
    values.append(BlockData(struct.pack('>i', 3)))
    for index, item in enumerate(items):
        measurement = copy.deepcopy(prototype)
        _replace_measurement(measurement, item)
        values.append(measurement)
        values.append(BlockData(struct.pack('>i', 0 if index == len(items) - 1 else 3)))
    return dumps(values)


def build_mdat(measurements: Iterable[MeasurementInput], path: str | Path, *,
               template_path: str | Path = DEFAULT_TEMPLATE, spl_offset_db: float | None = None) -> Path:
    """측정 N개가 든 mdat 을 쓰고 경로를 반환한다."""
    destination = Path(path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(build_mdat_bytes(measurements, template_path=template_path, spl_offset_db=spl_offset_db))
    return destination


__all__ = ['DEFAULT_TEMPLATE', 'MeasurementInput', 'build_mdat', 'build_mdat_bytes']
