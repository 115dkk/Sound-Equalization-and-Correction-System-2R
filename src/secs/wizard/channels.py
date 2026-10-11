# -*- coding: utf-8 -*-
"""입·출력 채널 이름 — REW 가 주는 풀 네임("11: Phones Out 1")으로 보여 준다(2026-09-22 사용자 지시).

번호만 보이면("Out 11") 인터페이스의 어느 단자인지 알 수 없다. REW 는 ASIO 출력·입력 목록을
`"번호: 이름"` 으로 주므로(`RealREW.output_channel_names` / `input_channel_names`) 화면의
모든 채널 선택·표시는 이 모듈을 거쳐 그 이름을 쓴다. 이름을 못 받으면(REW 미연결·Java 드라이버)
`Out N` / `In N` 으로 물러난다. 저장 값(`.secs` 의 out 번호)은 그대로 번호다.

부르는 곳: 1단계 입력 채널, 2단계 측정 출력 콤보·요약, 4단계 타이밍 레퍼런스(기준 출력), 5단계 로그.
"""
from __future__ import annotations

from typing import Iterable


def _names(devices, kind: str) -> list[str]:
    getter = getattr(devices, f'{kind}_channel_names', None)
    if getter is None:
        return []
    try:
        return [str(name) for name in (getter() or [])]
    except Exception:  # noqa: BLE001 - 이름은 있으면 좋은 것, 없으면 번호로
        return []


def output_channel_names(devices) -> list[str]:
    """REW 출력 채널 이름 목록(1-기준 순서). 없으면 빈 목록."""
    return _names(devices, 'output')


def input_channel_names(devices) -> list[str]:
    """REW 입력 채널 이름 목록(1-기준 순서). 없으면 빈 목록."""
    return _names(devices, 'input')


def pick(names: Iterable[str], number: int, fallback: str) -> str:
    """`number`(1-기준)번째 이름. 목록이 짧거나 없으면 `fallback`."""
    items = list(names)
    index = int(number) - 1
    if 0 <= index < len(items) and items[index]:
        return items[index]
    return fallback


def output_label(devices, out: int, names: list[str] | None = None) -> str:
    """출력 번호 → "11: Phones Out 1" (없으면 "Out 11")."""
    return pick(output_channel_names(devices) if names is None else names, out, f'Out {int(out)}')


def input_label(devices, channel: int, names: list[str] | None = None) -> str:
    """입력 번호 → "3: Mic/Line/Inst In 1" (없으면 "In 3")."""
    return pick(input_channel_names(devices) if names is None else names, channel, f'In {int(channel)}')


__all__ = ['output_channel_names', 'input_channel_names', 'output_label', 'input_label', 'pick']
