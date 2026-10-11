# -*- coding: utf-8 -*-
"""자바 "Modified UTF-8"(JOS 의 문자열 인코딩) 디코더 — 표준 UTF-8 과 널 문자·서로게이트
페어 처리가 달라 파이썬 내장 codec 을 못 쓴다. `serialization.reader.ByteReader.utf()`
가 부른다. 자바 런타임 없이 순수 파이썬으로 구현한다.
"""
from __future__ import annotations
import struct
from secs.rew_mdat.errors import StreamFormatError
def decode_modified_utf8(data: bytes) -> str:
    """Decode Java Modified UTF-8 without invoking a Java runtime."""
    units = []
    index = 0
    while index < len(data):
        first = data[index]
        if first <= 127:
            if first == 0:
                raise StreamFormatError('literal null is invalid Modified UTF-8')
            else:
                units.append(first)
                index += 1
        else:
            if first & 224 == 192:
                if index + 1 >= len(data):
                    raise StreamFormatError('truncated two-byte Modified UTF-8 sequence')
                else:
                    second = data[index + 1]
                    if second & 192!= 128:
                        raise StreamFormatError('invalid Modified UTF-8 continuation byte')
                    else:
                        value = (first & 31) << 6 | second & 63
                        if value < 128 and value!= 0:
                            raise StreamFormatError('overlong Modified UTF-8 sequence')
                        else:
                            units.append(value)
                            index += 2
            else:
                if first & 240 == 224:
                    if index + 2 >= len(data):
                        raise StreamFormatError('truncated three-byte Modified UTF-8 sequence')
                    else:
                        second, third = (data[index + 1], data[index + 2])
                        if second & 192!= 128 or third & 192!= 128:
                            raise StreamFormatError('invalid Modified UTF-8 continuation byte')
                        else:
                            value = (first & 15) << 12 | (second & 63) << 6 | third & 63
                            if value < 2048:
                                raise StreamFormatError('overlong Modified UTF-8 sequence')
                            else:
                                units.append(value)
                                index += 3
                else:
                    raise StreamFormatError(f'invalid Modified UTF-8 lead byte 0x{first:02x}')
    raw = b''.join((struct.pack('>H', unit) for unit in units))
    return raw.decode('utf-16-be', errors='surrogatepass')
