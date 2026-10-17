# -*- coding: utf-8 -*-
"""원 측정의 REW 측정 환경(마이크 캘 · 입출력 장치 · 볼륨 · OS)을 꺼내고 되넣는다.

`builder` 는 템플릿 MeasData 를 복제해 측정을 만든다. 예전에는 템플릿에 든 제작자 환경(마이크 캘 곡선 ·
캘 파일 경로 · 오디오 장치 이름)이 그대로 남아 남의 mdat 에 들어갔다(10-17 버그 리포트).
이제 측정마다 `scrub()` 로 먼저 비우고, 원 측정 정보(`extract()` 로 꺼낸 JSON 사전)가 있으면 `apply()` 로
그 값을 넣는다. 정보는 `<프로젝트>.irs/<role>_P<pos>.src.json` 으로 IR wav 옆에 남는다(`rewapi.session`).
"""
from __future__ import annotations

from typing import Any

import numpy as np

from secs.rew_mdat.serialization.nodes import ClassDescriptor, FieldDescriptor, JavaArray, JavaObject, JavaString

#: 측정 환경 문자열 — 없으면 빈 문자열.
STRING_FIELDS = ('outputName', 'sourceFileName', 'timingRefName', 'osName', 'osVersion', 'jre', 'sigGenLevelSt')
#: 박스 값(java.lang.Double / Boolean). 비울 때 개인 값(볼륨 · 마이크 감도)은 null, 캘 사용 표시는 False,
#: 나머지(SPL 기준 120 dB · dBV 오프셋)는 REW 기본값이라 템플릿 값을 둔다.
PERSONAL_BOXED = ('inputVolume', 'outputVolume', 'micSensitivity')
BOXED_FIELDS = ('inputVolume', 'outputVolume', 'micSensitivity', 'splCalOffset', 'dBVInputOffset', 'dBVOutputOffset',
                'usesEmbeddedCalData', 'calDataLimitApplied')
CAL_FIELDS = ('meterCal', 'scCal')
CAL_ARRAYS = ('freqArray', 'gainArray', 'phaseArray')
CAL_STRINGS = ('sourceName', 'sourcePath')
CAL_BOXED = ('sensitivitydB', 'sensitivitydBFS', 'serialNum', 'revision')

# 표준 JDK serialVersionUID — 템플릿에 없는 박스 형(Integer 등)을 만들 때 쓴다.
_BOX_UID = {'java.lang.Double': -9172774392245257468, 'java.lang.Integer': 1360826667806852920,
            'java.lang.Boolean': -3665804199014368530}
_BOX_CODE = {'java.lang.Double': 'D', 'java.lang.Integer': 'I', 'java.lang.Boolean': 'Z'}
_NUMBER_UID = -8742448824652078965


def _owner(obj: JavaObject, name: str) -> dict | None:
    for descriptor in obj.class_hierarchy(derived_first=True):
        fields = obj.field_data.get(descriptor.name, {})
        if name in fields:
            return fields
    return None


def _put(obj: JavaObject, name: str, value) -> None:
    fields = _owner(obj, name)
    if fields is not None:              # 템플릿에 없는 필드는 건드리지 않는다(REW 판 차이)
        fields[name] = value


def _boxed_value(value) -> Any:
    if isinstance(value, JavaObject) and value.classdesc.name in _BOX_UID:
        return {'type': value.classdesc.name, 'value': value.get('value')}
    return None


class _Boxes:
    """박스 객체 공장 — 같은 클래스 서술자를 나눠 쓴다(직렬화 핸들이 한 벌만 생긴다)."""

    def __init__(self) -> None:
        self._desc: dict[str, ClassDescriptor] = {}
        self._number: ClassDescriptor | None = None

    def learn(self, value) -> None:
        if isinstance(value, JavaObject) and value.classdesc.name in _BOX_UID:
            self._desc.setdefault(value.classdesc.name, value.classdesc)
            if value.classdesc.super_desc is not None and self._number is None \
                    and value.classdesc.super_desc.name == 'java.lang.Number':
                self._number = value.classdesc.super_desc

    def make(self, spec) -> JavaObject | None:
        if not isinstance(spec, dict) or spec.get('type') not in _BOX_UID or spec.get('value') is None:
            return None
        kind = spec['type']
        desc = self._desc.get(kind)
        if desc is None:
            sup = None
            if kind != 'java.lang.Boolean':
                if self._number is None:
                    self._number = ClassDescriptor('java.lang.Number', _NUMBER_UID, flags=2)
                sup = self._number
            desc = ClassDescriptor(kind, _BOX_UID[kind], flags=2,
                                   fields=[FieldDescriptor(_BOX_CODE[kind], 'value')], super_desc=sup)
            self._desc[kind] = desc
        raw = spec['value']
        value = bool(raw) if kind == 'java.lang.Boolean' else (int(raw) if kind == 'java.lang.Integer' else float(raw))
        data = {kind: {'value': value}}
        if desc.super_desc is not None:
            data[desc.super_desc.name] = {}
        return JavaObject(desc, data)


def _scan(obj: JavaObject, boxes: _Boxes, float_desc: list) -> None:
    for name in BOXED_FIELDS:
        boxes.learn(obj.get(name))
    for name in CAL_FIELDS:
        cal = obj.get(name)
        if isinstance(cal, JavaObject):
            for key in CAL_BOXED:
                boxes.learn(cal.get(key))
            for key in CAL_ARRAYS:
                arr = cal.get(key)
                if isinstance(arr, JavaArray) and not float_desc:
                    float_desc.append(arr.classdesc)
    for name in ('splValues', 'phaseValues'):
        arr = obj.get(name)
        if isinstance(arr, JavaArray) and arr.classdesc.name == '[F' and not float_desc:
            float_desc.append(arr.classdesc)


def extract(fields) -> dict:
    """MeasData(또는 그 필드 사전) → JSON 으로 저장할 수 있는 측정 환경 사전."""
    get = fields.get
    out: dict[str, Any] = {'strings': {}, 'boxed': {}, 'cal': {}}
    date = get('sourceFileDate')
    if isinstance(date, int) and date > 0:
        out['date'] = date                  # 측정 시각(ms)
    for key, name in (('notes', 'measNotes'), ('format', 'sourceFileFormat')):
        value = get(name)
        if isinstance(value, JavaString) and value.value:
            out[key] = value.value
    for name in STRING_FIELDS:
        value = get(name)
        if isinstance(value, JavaString):
            out['strings'][name] = value.value
    for name in BOXED_FIELDS:
        boxed = _boxed_value(get(name))
        if boxed is not None:
            out['boxed'][name] = boxed
    for name in CAL_FIELDS:
        cal = get(name)
        if not isinstance(cal, JavaObject):
            continue
        item: dict[str, Any] = {'inverseCSelected': bool(cal.get('inverseCSelected', False))}
        for key in CAL_STRINGS:
            value = cal.get(key)
            item[key] = value.value if isinstance(value, JavaString) else ''
        for key in CAL_ARRAYS:
            arr = cal.get(key)
            item[key] = (np.asarray(arr.data, dtype=np.float32).tolist()
                         if isinstance(arr, JavaArray) and arr.data is not None else None)
        for key in CAL_BOXED:
            boxed = _boxed_value(cal.get(key))
            if boxed is not None:
                item[key] = boxed
        out['cal'][name] = item
    return out


def scrub(obj: JavaObject) -> None:
    """템플릿 측정에서 측정 환경을 모두 비운다 — 원 측정 정보가 없을 때 남는 모양."""
    for name in STRING_FIELDS:
        _put(obj, name, JavaString(''))
    for name in PERSONAL_BOXED:
        _put(obj, name, None)
    flag = obj.get('usesEmbeddedCalData')
    if isinstance(flag, JavaObject):
        _put(obj, 'usesEmbeddedCalData', JavaObject(flag.classdesc, {flag.classdesc.name: {'value': False}}))
    for name in CAL_FIELDS:
        cal = obj.get(name)
        if not isinstance(cal, JavaObject):
            continue
        _put(cal, 'inverseCSelected', False)
        for key in CAL_STRINGS:
            _put(cal, key, JavaString(''))
        for key in CAL_ARRAYS + CAL_BOXED + ('freqList', 'gainList', 'phaseList'):
            _put(cal, key, None)


def apply(obj: JavaObject, info: dict | None, *, boxes: _Boxes | None = None, float_desc=None) -> None:
    """`scrub()` 뒤 원 측정 정보(`extract()` 결과)를 넣는다. `info` 가 없으면 비운 채로 둔다."""
    boxes = boxes or _Boxes()
    float_desc = list(float_desc or [])
    _scan(obj, boxes, float_desc)
    scrub(obj)
    if not isinstance(info, dict):
        return
    for name, value in (info.get('strings') or {}).items():
        if name in STRING_FIELDS and value is not None:
            _put(obj, name, JavaString(str(value)))
    for name, spec in (info.get('boxed') or {}).items():
        made = boxes.make(spec)
        if name in BOXED_FIELDS and (made is not None or name in PERSONAL_BOXED):
            _put(obj, name, made)
    for name, item in (info.get('cal') or {}).items():
        cal = obj.get(name)
        if name not in CAL_FIELDS or not isinstance(cal, JavaObject) or not isinstance(item, dict):
            continue
        _put(cal, 'inverseCSelected', bool(item.get('inverseCSelected', False)))
        for key in CAL_STRINGS:
            _put(cal, key, JavaString(str(item.get(key) or '')))
        for key in CAL_ARRAYS:
            values = item.get(key)
            if values is not None and float_desc:
                _put(cal, key, JavaArray(float_desc[0], np.asarray(values, dtype=np.float32)))
        for key in CAL_BOXED:
            _put(cal, key, boxes.make(item.get(key)))


def boxes_for(objects) -> tuple[_Boxes, list]:
    """템플릿 객체들에서 박스·float[] 서술자를 미리 모은다(scrub 뒤에도 쓸 수 있게)."""
    boxes, float_desc = _Boxes(), []
    for obj in objects:
        if isinstance(obj, JavaObject):
            _scan(obj, boxes, float_desc)
    return boxes, float_desc


def from_rew_measurement(cal_path: str | None, output_device: str = '', input_device: str = '',
                         output_channel: str = '') -> dict | None:
    """SECS 가 REW API 로 잰 측정의 환경 — 그때 REW 에 걸린 마이크 캘 파일(텍스트: 주파수 · 게인 · [위상],
    머리줄 `Sens Factor =-1.337dB`)과 장치 이름. 캘 파일을 못 읽으면 이름만, 아무것도 없으면 None."""
    import os
    import re

    import time

    info: dict[str, Any] = {'strings': {}, 'boxed': {}, 'cal': {}, 'date': int(time.time() * 1000)}   # 잰 시각
    if output_device:
        info['strings']['outputName'] = ', '.join(x for x in (output_device, output_channel) if x)
    if input_device:
        info['strings']['sourceFileName'] = input_device
    if cal_path:
        item: dict[str, Any] = {'inverseCSelected': False, 'sourceName': os.path.basename(cal_path),
                                'sourcePath': str(cal_path), 'freqArray': None, 'gainArray': None, 'phaseArray': None}
        try:
            rows, sens = [], None
            with open(cal_path, encoding='utf-8', errors='replace') as handle:
                for line in handle:
                    found = re.search(r'Sens\s*Factor\s*=\s*([-+]?[0-9.]+)', line, re.IGNORECASE)
                    if found:
                        sens = float(found.group(1))
                    parts = line.replace(',', ' ').replace(';', ' ').split()
                    try:
                        values = [float(p) for p in parts[:3]]
                    except ValueError:
                        continue
                    if len(values) >= 2 and values[0] > 0:
                        rows.append(values + [0.0] * (3 - len(values)))
            if rows:
                data = np.asarray(rows, dtype=np.float32)
                item.update(freqArray=data[:, 0].tolist(), gainArray=data[:, 1].tolist(), phaseArray=data[:, 2].tolist())
                info['boxed']['usesEmbeddedCalData'] = {'type': 'java.lang.Boolean', 'value': True}
            if sens is not None:
                item['sensitivitydB'] = {'type': 'java.lang.Double', 'value': sens}
        except OSError:
            pass                        # 파일이 옮겨졌으면 이름만 남긴다
        info['cal']['meterCal'] = item
    return info if (info['strings'] or info['cal']) else None


__all__ = ['extract', 'scrub', 'apply', 'boxes_for', 'from_rew_measurement']
