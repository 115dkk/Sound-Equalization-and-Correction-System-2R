from __future__ import annotations
from collections.abc import Callable
from typing import Any, TypeVar
import numpy as np
from secs.rew_mdat.errors import InvalidHandleError, LimitExceededError, StreamFormatError, UnsupportedFeatureError
from secs.rew_mdat.limits import ParseLimits
from secs.rew_mdat.serialization.constants import BASE_HANDLE, SC_BLOCK_DATA, SC_EXTERNALIZABLE, SC_SERIALIZABLE, SC_WRITE_METHOD, TC_ARRAY, TC_BLOCKDATA, TC_BLOCKDATALONG, TC_CLASS, TC_CLASSDESC, TC_ENDBLOCKDATA, TC_ENUM, TC_EXCEPTION, TC_LONGSTRING, TC_NULL, TC_OBJECT, TC_PROXYCLASSDESC, TC_REFERENCE, TC_RESET, TC_STRING
from secs.rew_mdat.serialization.nodes import BlockData, ClassDescriptor, ExceptionState, FieldDescriptor, JavaArray, JavaClass, JavaEnum, JavaObject, JavaString, ParsedStream
from secs.rew_mdat.serialization.reader import ByteReader
T = TypeVar('T')
_END = object()
_RESET = object()
# ──── 자바 객체 직렬화 스트림 파서 ────
class JavaSerializationParser:
    """A bounded token parser. It never loads or executes serialized classes.

    `secs.rew_mdat.loads()`/`load()` 가 진입점으로 쓴다. 토큰 하나(`_content_inner`)마다
    분기해 문자열·클래스서술자·객체·배열·enum·블록데이터 노드(`serialization.nodes`)를
    만들고, 뒤에 나오는 `TC_REFERENCE` 가 앞서 만든 값을 핸들 번호로 재참조할 수 있도록
    `_handles` 에 쌓는다. `ParseLimits` 로 깊이·핸들 수·배열 크기를 모두 제한해 손상되거나
    악의적인 .mdat 이 무한 재귀·메모리 폭주를 일으키지 않게 한다.
    """
    def __init__(self, data: bytes, *, limits: ParseLimits | None=None) -> None:
        self.limits = limits or ParseLimits()
        if len(data) > self.limits.max_file_bytes:
            raise LimitExceededError(f'file size {len(data)} exceeds limit {self.limits.max_file_bytes}', 0)
        else:
            self.reader = ByteReader(data, self.limits)
            self._handles = {}
            self._next_handle = BASE_HANDLE
            self._total_handles = 0
            self._depth = 0
    @property
    def handle_count(self) -> int:
        return self._total_handles
    def parse(self) -> ParsedStream:
        if self.reader.u2()!= 44269 or self.reader.u2()!= 5:
            raise StreamFormatError('invalid Java serialization header', 0)
        else:
            values = []
            while not self.reader.at_eof():
                if len(values) >= self.limits.max_top_level_values:
                    raise LimitExceededError('top-level value limit exceeded', self.reader.offset)
                values.append(self._content())
            return ParsedStream(values, self.reader.offset, self._total_handles)
    def _with_depth(self, operation: Callable[[], T]) -> T:
        self._depth += 1
        if self._depth > self.limits.max_depth:
            self._depth -= 1
            raise LimitExceededError('nesting depth limit exceeded', self.reader.offset)
        else:
            try:
                return operation()
            finally:
                self._depth -= 1
    def _add_handle(self, value: T) -> T:
        if self._total_handles >= self.limits.max_handles:
            raise LimitExceededError('handle limit exceeded', self.reader.offset)
        else:
            self._handles[self._next_handle] = value
            self._next_handle += 1
            self._total_handles += 1
            return value
    def _reset(self) -> None:
        self._handles.clear()
        self._next_handle = BASE_HANDLE
    def _content(self, token: int | None=None) -> Any:
        offset = self.reader.offset
        try:
            return self._with_depth(lambda: self._content_inner(token))
        except StreamFormatError as error:
            if error.offset is None:
                error.offset = offset
                error.args = (f'{error.args[0]} at offset 0x{offset:x}',)
            raise
    def _content_inner(self, token: int | None) -> Any:
        token = self.reader.u1() if token is None else token
        if token == TC_NULL:
            return
        else:
            if token == TC_REFERENCE:
                handle = self.reader.u4()
                if handle not in self._handles:
                    raise InvalidHandleError(f'invalid or reset handle 0x{handle:08x}')
                else:
                    return self._handles[handle]
            else:
                if token == TC_STRING:
                    return self._new_string(long=False)
                else:
                    if token == TC_LONGSTRING:
                        return self._new_string(long=True)
                    else:
                        if token == TC_CLASSDESC:
                            return self._new_classdesc()
                        else:
                            if token == TC_PROXYCLASSDESC:
                                return self._new_proxy_classdesc()
                            else:
                                if token == TC_OBJECT:
                                    return self._new_object()
                                else:
                                    if token == TC_ARRAY:
                                        return self._new_array()
                                    else:
                                        if token == TC_CLASS:
                                            return self._new_class()
                                        else:
                                            if token == TC_ENUM:
                                                return self._new_enum()
                                            else:
                                                if token == TC_BLOCKDATA:
                                                    return BlockData(self.reader.read(self.reader.u1()), long=False)
                                                else:
                                                    if token == TC_BLOCKDATALONG:
                                                        length = self.reader.i4()
                                                        if length > self.limits.max_array_bytes:
                                                            raise LimitExceededError('block-data byte limit exceeded', self.reader.offset)
                                                        else:
                                                            return BlockData(self.reader.read(length), long=True)
                                                    else:
                                                        if token == TC_ENDBLOCKDATA:
                                                            return _END
                                                        else:
                                                            if token == TC_RESET:
                                                                self._reset()
                                                                return _RESET
                                                            else:
                                                                if token == TC_EXCEPTION:
                                                                    value = ExceptionState(self._content())
                                                                    self._reset()
                                                                    return value
                                                                else:
                                                                    raise StreamFormatError(f'unknown serialization token 0x{token:02x}')
    def _new_string(self, *, long: bool) -> JavaString:
        return self._add_handle(JavaString(self.reader.utf(long=long), long=long))
    def _classdesc(self) -> ClassDescriptor | None:
        token = self.reader.u1()
        if token == TC_NULL:
            return
        else:
            if token == TC_REFERENCE:
                handle = self.reader.u4()
                value = self._handles.get(handle)
                if not isinstance(value, ClassDescriptor):
                    raise InvalidHandleError(f'handle 0x{handle:08x} is not a class descriptor')
                else:
                    return value
            else:
                if token == TC_CLASSDESC:
                    return self._new_classdesc()
                else:
                    if token == TC_PROXYCLASSDESC:
                        return self._new_proxy_classdesc()
                    else:
                        raise StreamFormatError(f'expected class descriptor, got token 0x{token:02x}')
    def _new_classdesc(self) -> ClassDescriptor:
        descriptor = self._add_handle(ClassDescriptor(self.reader.utf(), self.reader.i8()))
        descriptor.flags = self.reader.u1()
        count = self.reader.u2()
        if count > self.limits.max_fields_per_class:
            raise LimitExceededError('class field limit exceeded', self.reader.offset)
        else:
            for _ in range(count):
                typecode = chr(self.reader.u1())
                if typecode not in 'BCDFIJSZL[':
                    raise StreamFormatError(f'invalid field typecode {typecode!r}')
                else:
                    name = self.reader.utf()
                    type_string = None
                    type_string_node = None
                    if typecode in 'L[':
                        value = self._content()
                        if value is not None and (not isinstance(value, JavaString)):
                            raise StreamFormatError('field type signature is not a string')
                        else:
                            type_string = value.value if isinstance(value, JavaString) else None
                            type_string_node = value if isinstance(value, JavaString) else None
                    descriptor.fields.append(FieldDescriptor(typecode, name, type_string,
                                                             type_string_node))
            descriptor.annotations = self._annotations()
            descriptor.super_desc = self._classdesc()
            return descriptor
    def _new_proxy_classdesc(self) -> ClassDescriptor:
        descriptor = self._add_handle(ClassDescriptor('<proxy>', 0, SC_SERIALIZABLE,
                                                      is_proxy=True))
        count = self.reader.i4()
        if count < 0 or count > self.limits.max_fields_per_class:
            raise LimitExceededError('proxy interface limit exceeded', self.reader.offset)
        else:
            descriptor.interfaces = [self.reader.utf() for _ in range(count)]
            descriptor.annotations = self._annotations()
            descriptor.super_desc = self._classdesc()
            return descriptor
    def _annotations(self) -> list[Any]:
        values = []
        while True:
            value = self._content()
            if value is _END:
                return values
            else:
                if value is not _RESET:
                    values.append(value)
    def _new_object(self) -> JavaObject:
        descriptor = self._classdesc()
        if descriptor is None:
            raise StreamFormatError('object has a null class descriptor')
        else:
            value = self._add_handle(JavaObject(descriptor))
            for current in value.class_hierarchy():
                if current.flags & SC_SERIALIZABLE:
                    fields = {}
                    for field in current.fields:
                        fields[field.name] = self._field_value(field.typecode)
                    value.field_data[current.name] = fields
                    if current.flags & SC_WRITE_METHOD:
                        value.annotations[current.name] = self._annotations()
                else:
                    if current.flags & SC_EXTERNALIZABLE:
                        if current.flags & SC_BLOCK_DATA:
                            value.annotations[current.name] = self._annotations()
                        else:
                            raise UnsupportedFeatureError(f'externalizable class {current.name!r} lacks block-data framing')
            return value
    def _field_value(self, typecode: str) -> Any:
        readers = {'B': self.reader.i1, 'C': lambda: chr(self.reader.u2()), 'D': self.reader.f8, 'F': self.reader.f4, 'I': self.reader.i4, 'J': self.reader.i8, 'S': self.reader.i2, 'Z': lambda: self.reader.u1()!= 0, 'L': self._content, '[': self._content}
        return readers[typecode]()
    def _new_array(self) -> JavaArray:
        descriptor = self._classdesc()
        if descriptor is None:
            raise StreamFormatError('array has a null class descriptor')
        else:
            value = self._add_handle(JavaArray(descriptor))
            count = self.reader.i4()
            if count < 0:
                raise StreamFormatError(f'negative array length {count}')
            else:
                if count > self.limits.max_array_elements:
                    raise LimitExceededError('array element limit exceeded', self.reader.offset)
                else:
                    typecode = descriptor.name[1] if descriptor.name.startswith('[') else 'L'
                    widths = {'B': 1, 'Z': 1, 'C': 2, 'S': 2, 'F': 4, 'I': 4, 'D': 8, 'J': 8}
                    width = widths.get(typecode)
                    if width is not None and count * width > self.limits.max_array_bytes:
                        raise LimitExceededError('array byte limit exceeded', self.reader.offset)
                    else:
                        dtype_map = {'B': '>i1', 'Z': '>u1', 'C': '>u2', 'S': '>i2', 'I': '>i4', 'J': '>i8', 'F': '>f4', 'D': '>f8'}
                        if typecode in dtype_map:
                            raw = self.reader.read(count * widths[typecode])
                            array = np.frombuffer(raw, dtype=dtype_map[typecode])
                            if typecode == 'Z':
                                value.data = array.astype(np.bool_, copy=True)
                            else:
                                if typecode == 'C':
                                    value.data = [chr(int(item)) for item in array]
                                else:
                                    value.data = array.astype(array.dtype.newbyteorder('='), copy=True)
                        else:
                            value.data = [self._content() for _ in range(count)]
                        return value
    def _new_class(self) -> JavaClass:
        descriptor = self._classdesc()
        if descriptor is None:
            raise StreamFormatError('class token has a null descriptor')
        else:
            return self._add_handle(JavaClass(descriptor))
    def _new_enum(self) -> JavaEnum:
        descriptor = self._classdesc()
        if descriptor is None:
            raise StreamFormatError('enum token has a null descriptor')
        else:
            value = self._add_handle(JavaEnum(descriptor))
            value.constant = self._content()
            return value
