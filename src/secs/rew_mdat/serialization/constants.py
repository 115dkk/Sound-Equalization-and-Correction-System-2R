# -*- coding: utf-8 -*-
"""자바 객체 직렬화 스트림 포맷(JOS)의 타입 코드(`TC_*`)·클래스 플래그(`SC_*`) 상수.

Java Object Serialization Specification 의 고정값을 그대로 옮긴 것으로, `JavaSerializationParser`
가 스트림을 바이트 단위로 해석할 때 참조한다. 값 자체를 SECS 가 바꿀 일은 없다.
"""
from __future__ import annotations
BASE_HANDLE = 8257536
TC_NULL = 112
TC_REFERENCE = 113
TC_CLASSDESC = 114
TC_OBJECT = 115
TC_STRING = 116
TC_ARRAY = 117
TC_CLASS = 118
TC_BLOCKDATA = 119
TC_ENDBLOCKDATA = 120
TC_RESET = 121
TC_BLOCKDATALONG = 122
TC_EXCEPTION = 123
TC_LONGSTRING = 124
TC_PROXYCLASSDESC = 125
TC_ENUM = 126
SC_WRITE_METHOD = 1
SC_SERIALIZABLE = 2
SC_EXTERNALIZABLE = 4
SC_BLOCK_DATA = 8
SC_ENUM = 16
