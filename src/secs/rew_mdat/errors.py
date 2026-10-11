# -*- coding: utf-8 -*-
"""rew_mdat 전용 예외 계층 — 파싱 실패(`StreamFormatError` 계열)와 스키마 불일치(`SchemaError`)를
구분해, 호출자(`secs.mdat_loader`)가 "손상된 파일"과 "REW 형식이 아님"을 가른다.
"""
from __future__ import annotations
class MdatError(Exception):
    """Base error for MDAT parsing and adaptation."""
    pass
class StreamFormatError(MdatError):
    """The Java serialization stream is malformed."""
    def __init__(self, message: str, offset: int | None=None) -> None:
        self.offset = offset
        suffix = f' at offset 0x{offset:x}' if offset is not None else ''
        super().__init__(f'{message}{suffix}')
class UnsupportedFeatureError(StreamFormatError):
    """A valid but intentionally unsupported stream feature was found."""
class InvalidHandleError(StreamFormatError):
    """A reference points to a handle that is not active."""
class LimitExceededError(StreamFormatError):
    """A configured resource limit was exceeded."""
class SchemaError(MdatError):
    """The stream is valid, but does not contain the expected REW schema."""
