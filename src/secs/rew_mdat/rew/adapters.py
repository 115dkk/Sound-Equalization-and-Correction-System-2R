# -*- coding: utf-8 -*-
"""REW 클래스 이름(`roomeqwizard.MeasData` 등)을 아는 유일한 곳 — 파서가 돌려준 raw
자바 객체 그래프(`JavaObject`/`JavaArray`/…)를 `secs.rew_mdat.models` 의 REW 도메인
타입으로 옮긴다. `secs.rew_mdat.load()`(패키지 진입점)가 부른다. 필수 필드가 없으면
`SchemaError` 로 "파싱은 됐지만 REW 스키마가 아니다"를 알린다.
"""
from __future__ import annotations
import struct
from typing import Any, cast
import numpy as np
from secs.rew_mdat.errors import SchemaError
from secs.rew_mdat.models import DistortionDataset, FieldMeaning, FrequencyResponse, FrequencySpacing, ImpulseResponse, MdatFile, Measurement, RewVersion, RoomDimensions
from secs.rew_mdat.serialization.nodes import BlockData, JavaArray, JavaEnum, JavaObject, JavaString, ParsedStream
def _string(value: Any, default: str='') -> str:
    if isinstance(value, JavaString):
        return value.value
    else:
        if isinstance(value, str):
            return value
        else:
            return default
def _boxed(value: Any, default: Any=None) -> Any:
    if isinstance(value, JavaObject) and value.classdesc.name.startswith('java.lang.'):
        return value.get('value', default)
    else:
        if isinstance(value, JavaEnum):
            return _boxed(value.constant, default)
        else:
            if isinstance(value, JavaString):
                return value.value
            else:
                return default if value is None else value
def _array(value: Any, field_name: str) -> np.ndarray[Any, Any]:
    if not isinstance(value, JavaArray) or value.data is None:
        raise SchemaError(f'required REW array {field_name!r} is absent')
    else:
        if isinstance(value.data, bytes):
            return np.frombuffer(value.data, dtype=np.uint8).copy()
        else:
            if isinstance(value.data, list):
                return np.asarray(value.data)
            else:
                return value.data
def _int_block(value: Any, label: str) -> int:
    if not isinstance(value, BlockData) or len(value.data)!= 4:
        raise SchemaError(f'{label} is not a four-byte block')
    else:
        return cast(int, struct.unpack('>i', value.data)[0])
def _version_block(value: Any) -> tuple[int, int]:
    if not isinstance(value, BlockData) or len(value.data)!= 8:
        raise SchemaError('REW version is not an eight-byte block')
    else:
        return struct.unpack('>ii', value.data)
def _unknown_fields(obj: JavaObject, known: set[str]) -> dict[str, Any]:
    return {name: value for name, value in obj.all_fields().items() if name not in known}
def adapt_measurement(obj: JavaObject, file_version: tuple[int, int]) -> Measurement:
    if obj.classdesc.name!= 'roomeqwizard.MeasData':
        raise SchemaError(f'expected roomeqwizard.MeasData, got {obj.classdesc.name}')
    else:
        fields = obj.all_fields()
        required = ('splValues', 'phaseValues', 'unwPhaseValues', 'gdValues', 'irData')
        missing = [name for name in required if fields.get(name) is None]
        if missing:
            raise SchemaError(f"MeasData is missing required fields: {', '.join(missing)}")
        else:
            spacing = FrequencySpacing(count=int(fields['dataLength']), start_hz=float(fields['startFreq']), end_hz=float(fields['endFreq']), step_hz=float(fields['freqStep']), is_log_spaced=bool(fields['isLogSpaced']), points_per_octave=int(fields['ppo']), valid_start_hz=float(fields['validStartFreq']), valid_end_hz=float(fields['validEndFreq']))
            response = FrequencyResponse(spacing=spacing, spl_db=_array(fields['splValues'], 'splValues'), phase_deg=_array(fields['phaseValues'], 'phaseValues'), unwrapped_phase_deg=_array(fields['unwPhaseValues'], 'unwPhaseValues'), group_delay_s=_array(fields['gdValues'], 'gdValues'), meanings={'spl_db': FieldMeaning('confirmed', 'splValues'), 'phase_deg': FieldMeaning('confirmed', 'phaseValues'), 'unwrapped_phase_deg': FieldMeaning('confirmed', 'unwPhaseValues'), 'group_delay_s': FieldMeaning('confirmed', 'gdValues')})
            ir_data = fields['irData']
            if not isinstance(ir_data, JavaObject) or ir_data.classdesc.name!= 'roomeqwizard.IRData':
                raise SchemaError('irData does not contain roomeqwizard.IRData')
            else:
                sampled = ir_data.get('ir')
                if not isinstance(sampled, JavaObject) or sampled.classdesc.name!= 'roomeqwizard.SampledData':
                    raise SchemaError('IRData.ir does not contain roomeqwizard.SampledData')
                else:
                    samples = _array(sampled.get('data'), 'IRData.ir.data')
                    impulse = ImpulseResponse(samples=samples, sample_interval_s=float(sampled.get('T')), start_time_s=float(sampled.get('startTime')), measurement_delay_s=float(ir_data.get('measurementDelay')), spl_offset_db=float(ir_data.get('splOffset')), peak_index=int(sampled.get('absMaxIndex')), meanings={'samples': FieldMeaning('confirmed', 'IRData.ir.data'), 'measurement_delay_s': FieldMeaning('confirmed', 'IRData.measurementDelay'), 'spl_offset_db': FieldMeaning('confirmed', 'IRData.splOffset')})
                    distortion = None
                    distortion_obj = fields.get('distortionData')
                    if isinstance(distortion_obj, JavaObject):
                        distortion_fields = distortion_obj.all_fields()
                        harmonic_container = distortion_fields.get('harmData')
                        harmonic_arrays = []
                        if isinstance(harmonic_container, JavaArray) and isinstance(harmonic_container.data, list):
                                harmonic_arrays = _harmonic_arrays(harmonic_container.data)
                        noise_value = distortion_fields.get('noiseFloorSPL')
                        noise = _array(noise_value, 'noiseFloorSPL') if noise_value is not None else None
                        known_distortion = {'noiseFloorSPL', 'harmData', 'ppo', 'freqs'}
                        distortion = DistortionDataset(frequency_hz=_array(distortion_fields.get('freqs'), 'freqs'), harmonic_arrays=harmonic_arrays, points_per_octave=int(distortion_fields.get('ppo', 0)), noise_floor_spl_db=noise, meanings={'frequency_hz': FieldMeaning('confirmed', 'DistortionData.freqs'), 'harmonic_arrays': FieldMeaning('inferred', 'DistortionData.harmData', 'array identity is preserved; individual harmonic order is not asserted')}, unknown_fields=_unknown_fields(distortion_obj, known_distortion))
                    major = int(_boxed(fields.get('rewVersion'), file_version[0]))
                    minor = int(_boxed(fields.get('rewSubVersion'), file_version[1]))
                    known_measurement = {'dataLength', 'irData', 'rewVersion', 'validEndFreq', 'distortionData', 'roomHeight', 'splValues', 'validStartFreq', 'sampleRate', 'measNotes', 'endFreq', 'phaseValues', 'freqStep', 'ppo', 'versionSt', 'isLogSpaced', 'rewSubVersion', 'gdValues', 'shortDesc', 'unwPhaseValues', 'roomWidth', 'roomLength', 'startFreq'}
                    return Measurement(name=_string(fields.get('shortDesc')), sample_rate_hz=int(fields['sampleRate']), rew_version=RewVersion(major, minor, _string(fields.get('versionSt'))), room_dimensions=RoomDimensions(float(fields['roomLength']), float(fields['roomWidth']), float(fields['roomHeight'])), frequency_response=response, impulse_response=impulse, distortion=distortion, notes=_string(fields.get('measNotes')), unknown_fields=_unknown_fields(obj, known_measurement))
def _harmonic_arrays(items: list) -> list:
    """고조파 배열 목록 — REW 'Save all' 파일은 없는 차수를 null 로 둔다(2026-09-27 YENA 사본: harmData[3] null).
    순서(차수)를 지키려고 null 자리는 빈 배열로 둔다."""
    out = []
    for index, item in enumerate(items):
        if item is None or (isinstance(item, JavaArray) and item.data is None):
            out.append(np.zeros(0, dtype=np.float32))
        else:
            out.append(_array(item, f'harmData[{index}]'))
    return out
def adapt_stream(parsed: ParsedStream, file_size: int) -> MdatFile:
    values = parsed.values
    if len(values) < 4:
        raise SchemaError('MDAT top-level sequence is incomplete')
    else:
        title = _string(values[0])
        version = _version_block(values[1])
        notes = _string(values[2])
        preview_count = _int_block(values[3], 'preview count')
        measurement_objects = [value for value in values if isinstance(value, JavaObject) and
                               value.classdesc.name == 'roomeqwizard.MeasData']
        measurements = [adapt_measurement(value, version) for value in measurement_objects]
        known_ids = {id(value) for value in measurement_objects}
        return MdatFile(title=title, rew_version_major=version[0], rew_version_minor=version[1], notes=notes, preview_count=preview_count, measurements=measurements, bytes_consumed=parsed.bytes_consumed, file_size=file_size, top_level_count=len(values), handle_count=parsed.handle_count, raw_top_level=values, unknown_top_level=[value for index, value in enumerate(values) if index > 3 and id(value) not in known_ids])
