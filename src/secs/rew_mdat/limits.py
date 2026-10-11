# -*- coding: utf-8 -*-
"""신뢰할 수 없는 .mdat 입력에 대한 파서 자원 상한. `JavaSerializationParser` 생성자와
`load()` 가 읽어 초과 시 `LimitExceededError` 를 던진다. 값을 조정하는 화면은 없다.
"""
from __future__ import annotations
from dataclasses import dataclass
@dataclass(frozen=True, slots=True)
class ParseLimits:
    max_file_bytes: int = 536870912
    max_array_bytes: int = 268435456
    max_array_elements: int = 50000000
    max_string_bytes: int = 16777216
    max_depth: int = 256
    max_handles: int = 2000000
    max_fields_per_class: int = 4096
    max_top_level_values: int = 100000
