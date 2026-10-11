"""`secs.rewapi` — REW 자동 측정 인터페이스와 구현(P2 트랙 B).

- `interfaces` : 데이터 클래스 + 프로토콜(계약)
- `fake`       : `FakeREW` — REW 없이 전 흐름을 돌리는 가짜 구현
- `rew`        : `RealREW` — 실제 REW API(HTTP, 127.0.0.1:4735)
- `callback`   : REW 구독을 받는 로컬 HTTP 콜백 서버
- `session`    : `MeasurementSession` — `.secs` 기록 + IR 파일 저장

Qt 에 의존하지 않는다(순수 파이썬). `RealREW` 는 `requests` 를, `IrSaver` 는
`soundfile` 을 쓰지만 둘 다 지연 임포트라 없으면 그 기능만 막힌다.
"""
from __future__ import annotations

from typing import Any

from secs.rewapi.interfaces import (
    PHASES, SWEEP_LENGTHS, DeviceInfo, DeviceLister, IRProvider, LevelMonitor,
    MeasurementPlan, MeasurementResult, Measurer, RewError, RewProRequired, RewStatus,
    emit, plan_from_project, progress, split_role, sweep_length_samples,
)
from secs.rewapi.fake import FakeREW, fake_plan
from secs.rewapi.session import IrSaver, MeasurementSession

__all__ = [
    'MeasurementPlan', 'MeasurementResult', 'DeviceInfo',
    'RewStatus', 'DeviceLister', 'LevelMonitor', 'Measurer', 'IRProvider',
    'RewError', 'RewProRequired', 'SWEEP_LENGTHS', 'PHASES',
    'split_role', 'sweep_length_samples', 'progress', 'emit', 'plan_from_project',
    'FakeREW', 'fake_plan', 'MeasurementSession', 'IrSaver',
    'RealREW', 'CallbackServer', 'REW_API_PORT',
]

# `requests` 없이도 임포트되도록 무거운 구현은 지연 로딩한다.
_LAZY = {
    'RealREW': ('secs.rewapi.rew', 'RealREW'),
    'REW_API_PORT': ('secs.rewapi.rew', 'REW_API_PORT'),
    'CallbackServer': ('secs.rewapi.callback', 'CallbackServer'),
}


def __getattr__(name: str) -> Any:
    if name in _LAZY:
        module_name, attr = _LAZY[name]
        import importlib
        return getattr(importlib.import_module(module_name), attr)
    raise AttributeError(f'module {__name__!r} has no attribute {name!r}')


def __dir__() -> list[str]:
    return sorted(set(__all__) | set(globals()))
