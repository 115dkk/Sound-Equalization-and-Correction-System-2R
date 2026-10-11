# -*- coding: utf-8 -*-
"""REW .mdat(자바 직렬화) 파일을 REW 없이 직접 읽고 쓰는 독립 패키지.

`secs.mdat_loader.load_mdat_project()` 가 진입점으로 쓴다. `load(path)`/`loads(bytes)`
가 바이트열을 `secs.rew_mdat.serialization.parser.JavaSerializationParser` 로 토큰
파싱한 뒤 `secs.rew_mdat.rew.adapters.adapt_stream` 으로 REW 스키마(`MdatFile`,
측정마다의 주파수/임펄스 응답)에 맞게 해석한다. 신뢰할 수 없는 입력을 다루므로
`ParseLimits` 로 크기·깊이·핸들 수를 제한하고, 직렬화된 클래스를 로드·실행하지 않는다
(자바 런타임 불필요). `serialization.writer` 는 원시 그래프를 바이트 동일하게 되쓰고,
`builder` 는 단일 측정 템플릿을 복제해 새 측정 프로젝트를 만든다.
"""
from __future__ import annotations
from pathlib import Path
from secs.rew_mdat.limits import ParseLimits
from secs.rew_mdat.models import MdatFile
from secs.rew_mdat.rew.adapters import adapt_stream
from secs.rew_mdat.serialization.parser import JavaSerializationParser
__version__ = '0.1.0'
def loads(data: bytes, *, limits: ParseLimits | None=None) -> MdatFile:
    parsed = JavaSerializationParser(data, limits=limits).parse()
    return adapt_stream(parsed, len(data))
def load(path: str | Path, *, limits: ParseLimits | None=None) -> MdatFile:
    path = Path(path)
    size = path.stat().st_size
    active_limits = limits or ParseLimits()
    if size > active_limits.max_file_bytes:
        from secs.rew_mdat.errors import LimitExceededError
        raise LimitExceededError(f'file size {size} exceeds limit {active_limits.max_file_bytes}', 0)
    else:
        return loads(path.read_bytes(), limits=active_limits)
__all__ = ['MdatFile', 'ParseLimits', 'load', 'loads']
