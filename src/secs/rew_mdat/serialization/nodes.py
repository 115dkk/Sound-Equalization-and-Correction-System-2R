# -*- coding: utf-8 -*-
"""`JavaSerializationParser` 가 만드는 원시 노드 타입들 — 아직 REW 도메인으로 해석되기 전의
자바 객체 그래프(클래스 서술자·객체·배열·enum·블록데이터). `secs.rew_mdat.rew.adapters` 가
이 노드들을 읽어 `secs.rew_mdat.models` 의 REW 전용 타입으로 바꾼다.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import Any
import numpy as np
import numpy.typing as npt
@dataclass(slots=True)
class JavaString:
    value: str
    long: bool = False
    def __str__(self) -> str:
        return self.value
@dataclass(slots=True)
class FieldDescriptor:
    typecode: str
    name: str
    type_string: str | None = None
    # JOS 는 필드 시그니처도 일반 문자열 객체로 써서 뒤 클래스 서술자가 같은
    # 핸들을 참조할 수 있다. 사람이 쓰는 값(`type_string`)과 별도로 그 정체성을
    # 보존해야 parse -> write 가 원본과 바이트 단위로 같다.
    type_string_node: JavaString | None = field(default=None, repr=False)
@dataclass(slots=True)
class ClassDescriptor:
    name: str
    serial_version_uid: int
    flags: int = 0
    fields: list[FieldDescriptor] = field(default_factory=list)
    annotations: list[Any] = field(default_factory=list)
    super_desc: ClassDescriptor | None = None
    interfaces: list[str] = field(default_factory=list)
    is_proxy: bool = False
@dataclass(slots=True)
class JavaObject:
    classdesc: ClassDescriptor
    field_data: dict[str, dict[str, Any]] = field(default_factory=dict)
    annotations: dict[str, list[Any]] = field(default_factory=dict)
    def class_hierarchy(self, *, derived_first: bool=False) -> list[ClassDescriptor]:
        result = []
        descriptor = self.classdesc
        while descriptor is not None:
            result.append(descriptor)
            descriptor = descriptor.super_desc
        if not derived_first:
            result.reverse()
        return result
    def get(self, name: str, default: Any=None) -> Any:
        for descriptor in self.class_hierarchy(derived_first=True):
            fields = self.field_data.get(descriptor.name, {})
            if name in fields:
                return fields[name]
        return default
    def all_fields(self) -> dict[str, Any]:
        result = {}
        for descriptor in self.class_hierarchy():
            result.update(self.field_data.get(descriptor.name, {}))
        return result
ArrayData = bytes | list[Any] | npt.NDArray[np.int8] | npt.NDArray[np.int16] | npt.NDArray[np.int32] | npt.NDArray[np.int64] | npt.NDArray[np.float32] | npt.NDArray[np.float64] | npt.NDArray[np.bool_]
@dataclass(slots=True)
class JavaArray:
    classdesc: ClassDescriptor
    data: ArrayData | None = None
@dataclass(slots=True)
class JavaEnum:
    classdesc: ClassDescriptor
    constant: Any = None
@dataclass(slots=True)
class JavaClass:
    classdesc: ClassDescriptor
@dataclass(slots=True)
class BlockData:
    data: bytes
    long: bool = False
@dataclass(slots=True)
class ExceptionState:
    exception: Any
@dataclass(slots=True)
class ParsedStream:
    values: list[Any]
    bytes_consumed: int
    handle_count: int
