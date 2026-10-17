"""P2 B: REW 자동 측정 인터페이스(순수 파이썬, Qt 의존 없음).

`P2 인터페이스 계약 2026-09-16.md` 의 "B: `secs.rewapi`" 절이 기준이다.
여기에는 데이터 클래스와 프로토콜만 둔다. 구현은 `fake.py`(FakeREW)·`rew.py`(RealREW).

용어
- role: 측정할 채널 역할 id. 기본은 `secs.model.ROLES` 의 값('L','R','SW1' …)이고,
  멀티앰프로 대역별 측정이 필요하면 `'L:LF'` 처럼 `역할:대역` 으로 확장한다.
- pos: 측정 위치 번호(1부터). 한 위치의 모든 role 을 측정한 뒤 사용자가 마이크를 옮긴다.
"""
from __future__ import annotations

import threading
from dataclasses import dataclass, field
from typing import Any, Callable, Protocol, runtime_checkable

import numpy as np

# 진행 콜백: {'role','pos','phase','percent','message'}
ProgressCallback = Callable[[dict], None]
# 입력 레벨 콜백: {'input_dbfs','spl_db','clip'}
LevelCallback = Callable[[dict], None]

#: `MeasurementPlan.sweep_length` 에 쓸 수 있는 REW 스윕 길이.
#: REW 5.40 Beta 124(API 0.9.4) 의 `/measure/sweep/configuration/sweep-lengths` 실목록 순서
#: 그대로다(2026-09-16 실검증). `/measure/sweep/configuration` 의 `length` 는 샘플 수가
#: 아니라 이 문자열을 그대로 받는다.
SWEEP_LENGTHS: tuple[str, ...] = ('64k', '128k', '256k', '512k', '1M', '2M', '4M')

#: 진행 콜백의 phase 값. 'sweep' = 스윕 시작, 'running' = 스윕 진행 중(REW 진행 문자열
#: 중계, `rew.RealREW.run_position`), 'process' = IR 수신 처리, 'done'/'cancelled' = 종료.
#: 2026-09-20 에는 'running' 을 넣을지 결정 못 했었는데(백로그 2), 2026-09-23 에 넣기로
#: 했다 — `FakeREW` 도 스윕 도중 'running' 을 보낸다(둘의 phase 집합을 맞춘다).
PHASES: tuple[str, ...] = ('sweep', 'running', 'process', 'done', 'cancelled')


class RewError(RuntimeError):
    """REW API 호출 실패(연결 불가·4xx·스키마 불일치)."""


class RewProRequired(RewError):
    """측정 명령이 Pro 라이선스로 막힌 경우(REW 가 4xx 로 거부)."""


def split_role(role: str) -> tuple[str, str | None]:
    """`'L:LF'` → `('L', 'LF')`, `'L'` → `('L', None)`."""
    if ':' in role:
        base, _, band = role.partition(':')
        return base.strip(), band.strip() or None
    return role.strip(), None


def sweep_length_samples(length: str) -> int:
    """`'256k'` → 262144. 알 수 없는 값이면 ValueError."""
    text = str(length).strip().lower()
    if text.endswith('k'):
        return int(float(text[:-1]) * 1024)
    if text.endswith('m'):
        return int(float(text[:-1]) * 1024 * 1024)
    return int(text)


@dataclass
class MeasurementPlan:
    """한 세션에서 무엇을 어떤 순서로 측정할지."""

    roles: list[str]                       # 측정할 채널 역할 id, 순서대로
    outputs: dict[str, int]                # role -> Windows 출력 장치 채널(1부터)
    positions: list[int]                   # 측정 위치 번호 1..N
    output_device: str = ''
    input_device: str = ''
    input_channel: int = 1
    sweep_level_db: float = -20.0
    sweep_length: str = '256k'
    repetitions: int = 1
    timing_reference: str = 'acoustic'     # 설계 결정: 음향 타이밍 레퍼런스, 반복 1
    #: 타이밍 레퍼런스 신호가 나갈 출력 채널(1부터). None 이면 REW 에 잡힌 값을 둔다.
    #: 음향 = 기준음을 낼 스피커의 출력, 루프백 = 입력으로 되돌린 출력.
    timing_ref_output: int | None = None
    naming: str = '{role} P{pos}'
    start_frequency: float = 20.0
    end_frequency: float = 20000.0
    sample_rate: int = 48000

    def name_for(self, role: str, pos: int) -> str:
        """`naming` 템플릿을 적용한 측정 이름."""
        base, band = split_role(role)
        try:
            return self.naming.format(role=role, pos=pos, base=base, band=band or '')
        except (KeyError, IndexError):
            return f'{role} P{pos}'

    def output_for(self, role: str) -> int:
        """role 의 출력 채널. 없으면 1."""
        return int(self.outputs.get(role, 1))

    def validate(self) -> list[str]:
        """사람이 읽을 오류 메시지 목록(빈 목록이면 정상)."""
        msgs: list[str] = []
        if not self.roles:
            msgs.append('측정할 채널이 없습니다')
        if len(set(self.roles)) != len(self.roles):
            msgs.append('역할 중복이 있습니다')
        if not self.positions:
            msgs.append('측정 위치가 없습니다')
        if len(set(self.positions)) != len(self.positions):
            msgs.append('측정 위치 번호 중복이 있습니다')
        for role in self.roles:
            if role not in self.outputs:
                msgs.append(f'{role}: 측정 출력 채널이 지정되지 않았습니다')
        if self.repetitions != 1 and str(self.timing_reference).lower().startswith('acoustic'):
            # 검토 문서: acoustic 타이밍 레퍼런스에서는 다중 스윕 불가.
            msgs.append('음향 타이밍 레퍼런스에서는 스윕 반복이 1 이어야 합니다')
        if str(self.sweep_length) not in SWEEP_LENGTHS:
            msgs.append(f'스윕 길이는 {" / ".join(SWEEP_LENGTHS)} 중 하나여야 합니다')
        return msgs

    def total_steps(self) -> int:
        return len(self.roles) * len(self.positions)


#: 이 값보다 SNR 이 낮으면 5단계가 "레벨을 올리거나 방을 조용히" 경고를 띄운다.
LOW_SNR_DB = 20.0


@dataclass
class MeasurementResult:
    """측정 하나의 결과(IR 포함)."""

    role: str
    pos: int
    uuid: str | None
    ir: np.ndarray
    sample_rate: int
    name: str
    spl_db: float | None = None
    #: IR 시간 원점 — 배열 인덱스 0 의 절대 시각(초).
    #: REW 는 `/measurements/{id}/impulse-response` 응답의 `startTime` 으로 준다.
    #: 타이밍 레퍼런스가 있으면 음수일 수 있다(t=0 앞의 여유 구간).
    #: **WAV 는 이 값을 담지 못하므로 `.secs` 의 `'t0'` 가 진실이다.**
    t0_s: float = 0.0
    #: REW 측정의 SPL 오프셋(dB, `GET /measurements/{id}` 의 `splOffsetdB`) — IR 진폭 → dB SPL. mdat 로 다시 쓸 때
    #: 이 값이 없으면 템플릿 값(120 dB)이 들어가 레벨이 틀렸다(10-09 사용자: 실측 116.1 dB → mdat +3.9 dB).
    spl_offset_db: float | None = None
    #: REW 가 잰 신호 대 잡음(dB, MeasurementSummary.signalToNoisedB). 모르면 None.
    #: 실스윕(09-18)에서 −24 dBFS 로 12.9 dB 가 나왔다 — `LOW_SNR_DB` 아래면 경고한다.
    snr_db: float | None = None
    #: 원 측정의 REW 측정 환경(마이크 캘 · 장치, `rew_mdat.source_info`) — mdat 로 다시 쓸 때 그대로 넣는다(10-17).
    source_info: dict | None = None


@dataclass
class DeviceInfo:
    """오디오 장치 하나."""

    id: str
    name: str
    channels: int
    kind: str          # 'output' | 'input'
    extra: dict = field(default_factory=dict)


@runtime_checkable
class RewStatus(Protocol):
    def is_running(self) -> bool: ...
    def version(self) -> str | None: ...
    def is_pro(self) -> bool: ...


@runtime_checkable
class DeviceLister(Protocol):
    def output_devices(self) -> list[DeviceInfo]: ...
    def input_devices(self) -> list[DeviceInfo]: ...


@runtime_checkable
class LevelMonitor(Protocol):
    def start(self, on_level: LevelCallback) -> None:
        """입력 레벨 모니터링 시작. 콜백 dict: {'input_dbfs','spl_db','clip'}."""

    def stop(self) -> None: ...

    def play_pink(self, role: str, output: int, level_db: float) -> None:
        """레벨 확인용 신호 재생(서브는 사인, 그 외 핑크 노이즈)."""

    def stop_generator(self) -> None: ...


@runtime_checkable
class Measurer(Protocol):
    def run_position(self, plan: MeasurementPlan, pos: int,
                     on_progress: ProgressCallback,
                     cancel: threading.Event) -> list[MeasurementResult]:
        """한 위치의 모든 role 을 순차 측정(블로킹, 워커 스레드에서 호출).

        취소되면 그때까지의 결과만 돌려주고 마지막 진행 콜백 phase 는 'cancelled'.
        """


@runtime_checkable
class IRProvider(Protocol):
    def get_ir(self, uuid: str) -> tuple[np.ndarray, int]: ...


def plan_from_project(project: Any, positions: Any, **kwargs: Any) -> MeasurementPlan:
    """`Project.config` 의 스피커·대역에서 `MeasurementPlan` 을 만든다.

    대역이 둘 이상인 스피커(멀티앰프)는 role 을 `'L:LF'` 처럼 확장하고,
    출력 채널은 `SpeakerConfig.bands[].out` 을 그대로 쓴다.
    """
    roles: list[str] = []
    outputs: dict[str, int] = {}
    from secs.model import ordered_speakers
    # 서브는 맨 뒤로 모은다 — 나중에 넣은 서브만 끝에 따로 재던 문제(10-09 사용자 지적). 각 무리 안에서는 역할
    # 순서(10-10, `Step5Measure.plan()` 과 같은 도우미).
    speakers = ordered_speakers(getattr(project.config, 'speakers', []), subs_last=True)
    for speaker in speakers:
        bands = list(getattr(speaker, 'bands', []))
        for band in bands:
            role = speaker.role if len(bands) <= 1 else f'{speaker.role}:{band.band}'
            roles.append(role)
            outputs[role] = int(band.out)
    params: dict[str, Any] = {'output_device': getattr(project.config, 'device', '') or ''}
    params.update(kwargs)
    return MeasurementPlan(roles=roles, outputs=outputs,
                           positions=[int(p) for p in positions], **params)


def progress(role: str, pos: int, phase: str, percent: float, message: str = '') -> dict:
    """진행 콜백 dict 를 한 곳에서 만든다."""
    return {
        'role': role, 'pos': int(pos), 'phase': phase,
        'percent': float(max(0.0, min(100.0, percent))), 'message': message,
    }


def emit(on_progress: ProgressCallback | None, payload: dict) -> None:
    """콜백이 없거나 예외를 던져도 측정이 멈추지 않게 한다."""
    if on_progress is None:
        return
    try:
        on_progress(payload)
    except Exception:  # noqa: BLE001 - UI 콜백의 오류로 측정을 중단하지 않는다
        pass


__all__ = [
    'MeasurementPlan', 'MeasurementResult', 'DeviceInfo',
    'RewStatus', 'DeviceLister', 'LevelMonitor', 'Measurer', 'IRProvider',
    'RewError', 'RewProRequired', 'ProgressCallback', 'LevelCallback',
    'SWEEP_LENGTHS', 'PHASES', 'LOW_SNR_DB',
    'split_role', 'sweep_length_samples', 'progress', 'emit',
    'plan_from_project',
]
