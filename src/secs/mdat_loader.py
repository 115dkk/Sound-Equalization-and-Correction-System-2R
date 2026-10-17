"""REW .mdat 프로젝트를 REW 실행 없이 직접 읽는다 — 위저드 5단계 [mdat 불러오기]가 부른다.

`secs.rew_mdat.load()`(자체 Java 직렬화 파서)로 .mdat 을 파싱해 `MdatFile` 을 얻고,
`load_mdat_project(path)` 가 이를 `MeasurementInfo` 목록과 IR 조회 객체(`MdatIRProvider.get_ir()`)로
바꿔 돌려준다. `suggest_assignment` 는 측정 이름에서 역할·위치를 짐작한다(5단계 배정 창).
"""
from __future__ import annotations
import re
from dataclasses import dataclass
from pathlib import Path
import numpy as np
from secs.rew_mdat import load
from secs.rew_mdat.models import MdatFile
@dataclass
class MeasurementInfo:
    """mdat 측정 하나의 요약(옛 1.41 `secs.constants` 에서 옮김, 2026-10-10). `role` 은 옛 화면의 역할 칸이다."""
    uuid: str
    title: str
    notes: str
    sample_rate: int
    start_freq: float
    end_freq: float
    date: str = ''
    role: str = 'Ignore'
    start_time_s: float = 0.0
    measurement_delay_s: float = 0.0
    timing_reference: str = ''
    #: 원 측정의 REW 측정 환경(`rew_mdat.source_info.extract`) — 내보낼 때 그대로 넣는다(10-17).
    source_info: dict | None = None
class MdatIRProvider:
    """Read-only IR provider compatible with the existing grouping helper."""
    def __init__(self, impulse_responses: dict[str, tuple[np.ndarray, int]]) -> None:
        self._impulse_responses = impulse_responses
    def get_ir(self, measurement_id: str) -> tuple[np.ndarray, int] | None:
        result = self._impulse_responses.get(measurement_id)
        if result is None:
            return
        else:
            samples, sample_rate = result
            return (samples.copy(), sample_rate)
@dataclass(slots=True)
class LoadedMdatProject:
    mdat: MdatFile
    measurements: list[MeasurementInfo]
    ir_provider: MdatIRProvider
def suggest_assignment(titles, roles) -> list[str | None]:
    """mdat 측정 제목 → 역할 제안(측정 순서대로, 못 정하면 None). 2.x 위저드 5단계용.

    제목이 채널 이름 하나(역할 ID·돌비 약어 `model.role_code`·흔한 별칭, 뒤에 측정 번호 허용)일 때만
    배정하고, 같은 역할은 **처음 나온 측정** 만 쓴다. `L EQ`·`L LF SUM`·`… 예측`·`DBA L` 처럼 다른 말이
    붙은 제목은 원측정이 아니라 비워 둔다(09-23 돌비룸 mdat 76건: 예전 규칙은 빈 역할을 순서대로 채워
    `DBA L`→L, `INFRA`→C 로 오배정했다). 서브처럼 다른 이름의 측정은 사람이 배정 창에서 고른다.
    """
    from secs.model import role_from_title

    wanted = list(roles)
    out: list[str | None] = []
    used: set[str] = set()
    for title in titles:
        base = role_from_title(str(title or ''), wanted)
        role = next((r for r in wanted if str(r).split(':')[0] == base and r not in used), None) \
            if base else None
        if role is not None:
            used.add(role)
        out.append(role)
    return out


def load_mdat_project(path: str | Path) -> LoadedMdatProject:
    """Parse an MDAT locally and expose its measurements to the existing UI."""
    from secs.rew_mdat.serialization.nodes import JavaString
    from secs.rew_mdat.source_info import extract
    mdat = load(path)
    measurements = []
    impulse_responses = {}
    for index, measurement in enumerate(mdat.measurements):
        measurement_id = f'mdat:{index}'
        reference_match = re.search('Acoustic reference played from (.+?) with (?:no |a )?timing offset', measurement.notes or '', flags=re.IGNORECASE)
        measurements.append(MeasurementInfo(uuid=measurement_id, title=measurement.name or f'Measurement {index + 1}', notes=measurement.notes, sample_rate=measurement.sample_rate_hz, start_freq=measurement.frequency_response.spacing.valid_start_hz, end_freq=measurement.frequency_response.spacing.valid_end_hz, start_time_s=float(measurement.impulse_response.start_time_s), measurement_delay_s=float(measurement.impulse_response.measurement_delay_s), timing_reference=reference_match.group(1).strip() if reference_match is not None else '',
                                            source_info=extract({**measurement.unknown_fields, 'measNotes': JavaString(measurement.notes or '')})))
        # mdat IR 은 REW API 단위의 1/100 — API 로 잰 측정과 같은 단위가 되게 되돌린다(10-09, `rew_mdat.builder.REW_MDAT_IR_SCALE`).
        from secs.rew_mdat.builder import REW_MDAT_IR_SCALE
        impulse_responses[measurement_id] = (np.asarray(measurement.impulse_response.samples, dtype=np.float32) / REW_MDAT_IR_SCALE,
                                             measurement.sample_rate_hz)
    return LoadedMdatProject(mdat=mdat, measurements=measurements, ir_provider=MdatIRProvider(impulse_responses))
