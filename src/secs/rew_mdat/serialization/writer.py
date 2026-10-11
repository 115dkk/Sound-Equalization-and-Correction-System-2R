# -*- coding: utf-8 -*-
"""파서의 원시 자바 객체 그래프를 Java Object Serialization 바이트로 되쓴다.

`JavaSerializationParser` 가 보존한 객체 정체성을 기준으로 핸들을 다시 부여하므로
원본 그래프는 parse -> write 뒤 바이트 단위로 같다. `secs.rew_mdat.builder` 도 같은
노드 그래프를 복제·수정한 뒤 이 모듈로 REW `.mdat` 파일을 만든다.
"""
from __future__ import annotations

import io
import struct
from typing import Any

import numpy as np

from secs.rew_mdat.errors import StreamFormatError, UnsupportedFeatureError
from secs.rew_mdat.serialization.constants import (
    BASE_HANDLE, SC_BLOCK_DATA, SC_EXTERNALIZABLE, SC_SERIALIZABLE, SC_WRITE_METHOD,
    TC_ARRAY, TC_BLOCKDATA, TC_BLOCKDATALONG, TC_CLASS, TC_CLASSDESC, TC_ENDBLOCKDATA,
    TC_ENUM, TC_EXCEPTION, TC_LONGSTRING, TC_NULL, TC_OBJECT, TC_PROXYCLASSDESC,
    TC_REFERENCE, TC_STRING,
)
from secs.rew_mdat.serialization.nodes import (
    BlockData, ClassDescriptor, ExceptionState, JavaArray, JavaClass, JavaEnum,
    JavaObject, JavaString, ParsedStream,
)


def encode_modified_utf8(value: str) -> bytes:
    """자바 ``DataOutput.writeUTF`` 와 같은 Modified UTF-8 바이트."""
    raw = value.encode('utf-16-be', errors='surrogatepass')
    out = bytearray()
    for offset in range(0, len(raw), 2):
        unit = int.from_bytes(raw[offset:offset + 2], 'big')
        if 1 <= unit <= 0x7f:
            out.append(unit)
        elif unit <= 0x7ff:
            out.extend((0xc0 | unit >> 6, 0x80 | unit & 0x3f))
        else:
            out.extend((0xe0 | unit >> 12, 0x80 | unit >> 6 & 0x3f,
                        0x80 | unit & 0x3f))
    return bytes(out)


class JavaSerializationWriter:
    """객체 정체성과 JOS 핸들 부여 순서를 지키는 빅엔디안 스트림 writer."""

    def __init__(self) -> None:
        self.stream = io.BytesIO()
        self._handles: dict[int, int] = {}
        self._next_handle = BASE_HANDLE

    def write(self, parsed_or_values: ParsedStream | list[Any]) -> bytes:
        values = parsed_or_values.values if isinstance(parsed_or_values, ParsedStream) else parsed_or_values
        self.stream = io.BytesIO()
        self._handles.clear()
        self._next_handle = BASE_HANDLE
        self.stream.write(struct.pack('>HH', 0xaced, 5))
        for value in values:
            self._content(value)
        return self.stream.getvalue()

    def _u1(self, value: int) -> None:
        self.stream.write(struct.pack('>B', value))

    def _u2(self, value: int) -> None:
        self.stream.write(struct.pack('>H', value))

    def _i4(self, value: int) -> None:
        self.stream.write(struct.pack('>i', value))

    def _u4(self, value: int) -> None:
        self.stream.write(struct.pack('>I', value))

    def _i8(self, value: int) -> None:
        self.stream.write(struct.pack('>q', value))

    def _utf(self, value: str, *, long: bool = False) -> None:
        encoded = encode_modified_utf8(value)
        if long:
            self._i8(len(encoded))
        else:
            if len(encoded) > 0xffff:
                raise StreamFormatError(f'Modified UTF-8 value is too long for writeUTF: {len(encoded)} bytes')
            self._u2(len(encoded))
        self.stream.write(encoded)

    def _reference(self, value: Any) -> bool:
        handle = self._handles.get(id(value))
        if handle is None:
            return False
        self._u1(TC_REFERENCE)
        self._u4(handle)
        return True

    def _add_handle(self, value: Any) -> None:
        self._handles[id(value)] = self._next_handle
        self._next_handle += 1

    def _content(self, value: Any) -> None:
        if value is None:
            self._u1(TC_NULL)
        elif isinstance(value, JavaString):
            self._string(value)
        elif isinstance(value, ClassDescriptor):
            self._classdesc(value)
        elif isinstance(value, JavaObject):
            self._object(value)
        elif isinstance(value, JavaArray):
            self._array(value)
        elif isinstance(value, JavaClass):
            self._java_class(value)
        elif isinstance(value, JavaEnum):
            self._enum(value)
        elif isinstance(value, BlockData):
            self._block(value)
        elif isinstance(value, ExceptionState):
            self._u1(TC_EXCEPTION)
            self._content(value.exception)
            self._handles.clear()
            self._next_handle = BASE_HANDLE
        else:
            raise StreamFormatError(f'cannot serialize node {type(value).__name__}')

    def _string(self, value: JavaString) -> None:
        if self._reference(value):
            return
        encoded = encode_modified_utf8(value.value)
        long = value.long or len(encoded) > 0xffff
        self._u1(TC_LONGSTRING if long else TC_STRING)
        self._utf(value.value, long=long)
        self._add_handle(value)

    def _classdesc(self, descriptor: ClassDescriptor | None) -> None:
        if descriptor is None:
            self._u1(TC_NULL)
            return
        if self._reference(descriptor):
            return
        if descriptor.is_proxy:
            self._u1(TC_PROXYCLASSDESC)
            self._add_handle(descriptor)
            self._i4(len(descriptor.interfaces))
            for interface in descriptor.interfaces:
                self._utf(interface)
        else:
            self._u1(TC_CLASSDESC)
            self._utf(descriptor.name)
            self._i8(descriptor.serial_version_uid)
            # 클래스 서술자 핸들은 필드 시그니처보다 먼저 생긴다.
            self._add_handle(descriptor)
            self._u1(descriptor.flags)
            self._u2(len(descriptor.fields))
            for field in descriptor.fields:
                self._u1(ord(field.typecode))
                self._utf(field.name)
                if field.typecode in 'L[':
                    node = field.type_string_node
                    if node is None and field.type_string is not None:
                        node = JavaString(field.type_string)
                        field.type_string_node = node
                    self._content(node)
        self._annotations(descriptor.annotations)
        self._classdesc(descriptor.super_desc)

    def _annotations(self, values: list[Any]) -> None:
        for value in values:
            self._content(value)
        self._u1(TC_ENDBLOCKDATA)

    def _object(self, value: JavaObject) -> None:
        if self._reference(value):
            return
        self._u1(TC_OBJECT)
        self._classdesc(value.classdesc)
        self._add_handle(value)
        for descriptor in value.class_hierarchy():
            if descriptor.flags & SC_SERIALIZABLE:
                fields = value.field_data.get(descriptor.name, {})
                for field in descriptor.fields:
                    self._field(field.typecode, fields.get(field.name))
                if descriptor.flags & SC_WRITE_METHOD:
                    self._annotations(value.annotations.get(descriptor.name, []))
            elif descriptor.flags & SC_EXTERNALIZABLE:
                if not descriptor.flags & SC_BLOCK_DATA:
                    raise UnsupportedFeatureError(
                        f'externalizable class {descriptor.name!r} lacks block-data framing')
                self._annotations(value.annotations.get(descriptor.name, []))

    def _field(self, typecode: str, value: Any) -> None:
        if typecode == 'B':
            self.stream.write(struct.pack('>b', int(value)))
        elif typecode == 'C':
            self._u2(ord(value) if isinstance(value, str) else int(value))
        elif typecode == 'D':
            self.stream.write(struct.pack('>d', float(value)))
        elif typecode == 'F':
            self.stream.write(struct.pack('>f', float(value)))
        elif typecode == 'I':
            self._i4(int(value))
        elif typecode == 'J':
            self._i8(int(value))
        elif typecode == 'S':
            self.stream.write(struct.pack('>h', int(value)))
        elif typecode == 'Z':
            self._u1(1 if value else 0)
        elif typecode in 'L[':
            self._content(value)
        else:
            raise StreamFormatError(f'invalid field typecode {typecode!r}')

    def _array(self, value: JavaArray) -> None:
        if self._reference(value):
            return
        self._u1(TC_ARRAY)
        self._classdesc(value.classdesc)
        self._add_handle(value)
        data = value.data
        if data is None:
            raise StreamFormatError('array data is absent')
        typecode = value.classdesc.name[1] if value.classdesc.name.startswith('[') else 'L'
        count = len(data)
        self._i4(count)
        dtype_map = {'B': '>i1', 'Z': '>u1', 'C': '>u2', 'S': '>i2', 'I': '>i4',
                     'J': '>i8', 'F': '>f4', 'D': '>f8'}
        if typecode in dtype_map:
            if typecode == 'C':
                array = np.asarray([ord(item) if isinstance(item, str) else item for item in data],
                                   dtype=dtype_map[typecode])
            elif typecode == 'Z':
                array = np.asarray(data, dtype=np.uint8).astype(dtype_map[typecode], copy=False)
            else:
                array = np.asarray(data).astype(dtype_map[typecode], copy=False)
            self.stream.write(array.tobytes(order='C'))
        else:
            for item in data:
                self._content(item)

    def _java_class(self, value: JavaClass) -> None:
        if self._reference(value):
            return
        self._u1(TC_CLASS)
        self._classdesc(value.classdesc)
        self._add_handle(value)

    def _enum(self, value: JavaEnum) -> None:
        if self._reference(value):
            return
        self._u1(TC_ENUM)
        self._classdesc(value.classdesc)
        self._add_handle(value)
        self._content(value.constant)

    def _block(self, value: BlockData) -> None:
        long = value.long or len(value.data) > 0xff
        self._u1(TC_BLOCKDATALONG if long else TC_BLOCKDATA)
        if long:
            self._i4(len(value.data))
        else:
            self._u1(len(value.data))
        self.stream.write(value.data)


def dumps(parsed_or_values: ParsedStream | list[Any]) -> bytes:
    """원시 파싱 결과 또는 최상위 노드 목록을 JOS 바이트로 만든다."""
    return JavaSerializationWriter().write(parsed_or_values)


def dump(parsed_or_values: ParsedStream | list[Any], path) -> None:
    """`dumps` 결과를 경로에 쓴다."""
    from pathlib import Path
    Path(path).write_bytes(dumps(parsed_or_values))


__all__ = ['JavaSerializationWriter', 'encode_modified_utf8', 'dump', 'dumps']
