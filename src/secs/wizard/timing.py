# -*- coding: utf-8 -*-
"""타이밍 레퍼런스 — 음향·루프백·없음과 기준 출력(2026-09-18 사용자 지시로 프로그램에서 고른다).

4단계 '측정 옵션' 카드(`step4_layout`)가 고르고, 5단계 측정 계획(`step5_measure.plan`)이 읽는다.
2026-09-23 단계 순서를 바꾸며(1 장치 → 2 구성) 장치 단계에서 이리로 옮겼다 — 기준 스피커 목록은
구성의 스피커를 쓰는데, 장치 단계가 먼저 오면 그때는 아직 기본 프리셋뿐이라서다.
값은 예전 그대로 `project.extra['devices']` 의 `timing_reference`·`timing_ref_role`·
`timing_ref_output` 에 둔다(`.secs` 뜻은 바뀌지 않았다).
"""
from __future__ import annotations

#: 타이밍 레퍼런스 선택지(저장 키, 화면 문구). 키는 REW 선택지의 소문자다.
TIMING_CHOICES: tuple[tuple[str, str], ...] = (
    # 문구는 짧게 — 콤보가 카드 폭을 밀어 1024 폭에서 가로 스크롤이 생겼다.
    ('acoustic', '음향 (레퍼런스 스피커)'),
    ('loopback', '루프백 (케이블)'),
    ('none', '없음'),
)

#: `project.extra['devices']` 안의 타이밍 키 — 장치 단계가 저장할 때 지우지 않는다.
TIMING_KEYS = ('timing_reference', 'timing_ref_role', 'timing_ref_output')


def measurement_roles(project) -> list[tuple[str, int]]:
    """측정 role 과 그 출력 채널 — 5단계 `plan()` 과 같은 규칙(바이앰프는 `L:LF`). 순서는 역할 순서(`ordered_speakers`
    — 10-10 사용자: 나중에 넣은 SW2 · SW3 이 레퍼런스 출력 목록 맨 뒤에 나왔다). 서브를 맨 뒤로 모으는 것은 측정
    순서(`plan()`)뿐이다."""
    from secs.model import ordered_speakers

    out: list[tuple[str, int]] = []
    for speaker in ordered_speakers(project.config.speakers):
        if len(speaker.bands) <= 1:
            out.append((speaker.role, int(speaker.bands[0].out if speaker.bands else 1)))
        else:
            out.extend((f'{speaker.role}:{band.band}', int(band.out))
                       for band in speaker.bands)
    return out


def default_ref_role(project) -> str:
    """저장된 기준 스피커가 없을 때의 기본: 목업대로 R, 없으면 서브가 아닌 첫 채널.
    바이앰프면 고역 대역이 기준음을 낸다(서브는 기준음 대역을 못 낸다)."""
    roles = measurement_roles(project)
    mains = [role for role, _out in roles if not role.upper().startswith('SW')]
    wanted = next((role for role in mains if role.split(':')[0] == 'R'),
                  mains[0] if mains else (roles[0][0] if roles else ''))
    if ':' in wanted:
        base = wanted.split(':')[0]
        highs = [role for role, _out in roles if role.startswith(base + ':') and role.endswith(':HF')]
        wanted = highs[0] if highs else wanted
    return wanted


def timing_settings(project) -> tuple[str, int | None]:
    """저장된 선택 → `(timing_reference, timing_ref_output)`. 5단계 계획이 쓴다.

    음향은 기준 스피커 role 의 **지금** 출력 채널로 풀어 준다(구성에서 출력을 바꿔도
    따라간다). 기준 role 이 사라졌으면 기본 기준 스피커, 그것도 없으면 None.
    """
    stored = dict(project.extra.get('devices', {}) or {})
    kind = str(stored.get('timing_reference') or 'acoustic').lower()
    if kind not in {key for key, _text in TIMING_CHOICES}:
        kind = 'acoustic'
    if kind == 'acoustic':
        table = dict(measurement_roles(project))
        role = str(stored.get('timing_ref_role') or '')
        if role not in table:
            role = default_ref_role(project)
        return kind, table.get(role)
    if kind == 'loopback':
        channel = stored.get('timing_ref_output')
        return kind, int(channel) if channel else None
    return kind, None


__all__ = ['TIMING_CHOICES', 'TIMING_KEYS', 'measurement_roles', 'default_ref_role', 'timing_settings']
