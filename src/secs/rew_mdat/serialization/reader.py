# -*- coding: utf-8 -*-
"""빅엔디안 바이트 스트림 판독기 — `JavaSerializationParser` 가 부른다.

각 정수/실수/문자열 판독 메서드는 자바 `DataInputStream` 의 대응 타입 폭을 그대로 따르고,
길이가 스트림 끝을 넘으면 `StreamFormatError` 로 즉시 멈춘다(잘린 파일을 조용히 넘기지 않는다).
"""
from __future__ import annotations
import io
import struct
from typing import cast
from secs.rew_mdat.errors import StreamFormatError
from secs.rew_mdat.limits import ParseLimits
from secs.rew_mdat.serialization.modified_utf8 import decode_modified_utf8
class ByteReader:
    def __init__(self, data: bytes, limits: ParseLimits) -> None:
        self._stream = io.BytesIO(data)
        self.limits = limits
    @property
    def offset(self) -> int:
        return self._stream.tell()
    def at_eof(self) -> bool:
        position = self.offset
        value = self._stream.read(1)
        self._stream.seek(position)
        return not value
    def read(self, count: int) -> bytes:
        if count < 0:
            raise StreamFormatError(f'negative byte count {count}', self.offset)
        else:
            offset = self.offset
            value = self._stream.read(count)
            if len(value)!= count:
                raise StreamFormatError(f'truncated stream: needed {count} bytes, found {len(value)}', offset)
            else:
                return value
    def u1(self) -> int:
        return self.read(1)[0]
    def i1(self) -> int:
        return cast(int, struct.unpack('>b', self.read(1))[0])
    def u2(self) -> int:
        return cast(int, struct.unpack('>H', self.read(2))[0])
    def i2(self) -> int:
        return cast(int, struct.unpack('>h', self.read(2))[0])
    def i4(self) -> int:
        return cast(int, struct.unpack('>i', self.read(4))[0])
    def u4(self) -> int:
        return cast(int, struct.unpack('>I', self.read(4))[0])
    def i8(self) -> int:
        return cast(int, struct.unpack('>q', self.read(8))[0])
    def f4(self) -> float:
        return cast(float, struct.unpack('>f', self.read(4))[0])
    def f8(self) -> float:
        return cast(float, struct.unpack('>d', self.read(8))[0])
    def utf(self, *, long: bool=False) -> str:
        length = self.i8() if long else self.u2()
        if length < 0:
            raise StreamFormatError(f'negative string length {length}', self.offset)
        else:
            if length > self.limits.max_string_bytes:
                from secs.rew_mdat.errors import LimitExceededError
                raise LimitExceededError(f'string length {length} exceeds limit {self.limits.max_string_bytes}', self.offset)
            else:
                return decode_modified_utf8(self.read(length))
