# -*- coding: utf-8 -*-
"""SECS 2.0 데이터 모델 — 스피커 구성 · DSP 프로파일 · 타깃 커브 · 프로젝트(.secs).

`P1 인터페이스 계약 2026-09-16.md` 의 A1 절이 규격이고, 수치·규칙은
`UI 목업/secs-ui-mockup.html` 의 상태 모델(CHINFO·PRESETS·defaultBands·
setSubCount·DSP_PROFILES·setTargetPreset·targetAt)을 그대로 옮긴 것이다.

위저드 8단계와 엔진(`design`·`bass`·`sheet`·`export`·`verify`)이 모두 이 객체를 읽고
쓴다 — 화면은 따로 상태를 들지 않는다. 입력·출력은 `.secs` 파일 하나이고
(`Project.save`/`Project.load`), 측정 IR 만 옆의 `<이름>.irs/` 폴더에 따로 남는다.

JSON 은 `ensure_ascii=False, indent=2`, 값은 순수 파이썬 타입만 쓴다.
"""
from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np


# ──────────────────────────── 역할 · 그룹 ────────────────────────────
#: 목업 ROLE_ORDER 와 같은 순서 + 하이트 넷(10-10). 화면 목록·측정 순서도 이 순서를 따른다(`ordered_speakers`).
#: T* = 탑(천장, Top Front/Middle/Rear), H* = 하이트(벽 위쪽, Front/Rear Height) — 돌비 표기에서 다른 채널이다
#: (10-10 사용자: "하이트랑 탑 채널은 다른 채널이니까 따로 분리해야해").
ROLES: tuple[str, ...] = (
    'L', 'R', 'C',
    'SW1', 'SW2', 'SW3', 'SW4', 'SW5', 'SW6', 'SW7', 'SW8',
    'Ls', 'Rs', 'Lb', 'Rb', 'Lw', 'Rw',
    'Tfl', 'Tfr', 'Tml', 'Tmr', 'Tbl', 'Tbr',
    'Hfl', 'Hfr', 'Hbl', 'Hbr',
)

#: 서브우퍼 역할(목업 SUB_ROLES).
SUB_ROLES: tuple[str, ...] = ('SW1', 'SW2', 'SW3', 'SW4', 'SW5', 'SW6', 'SW7', 'SW8')

#: 대역 이름.
BANDS: tuple[str, ...] = ('FULL', 'LF', 'MF', 'HF', 'SUB')

#: 허용하는 대역 조합(목업 setBandSet: FULL / BI / 3way, 서브는 SUB 만).
BAND_SETS: tuple[tuple[str, ...], ...] = (('FULL',), ('LF', 'HF'), ('LF', 'MF', 'HF'))

#: 역할 → (표시 이름, 종류 'main'|'sub'|'top'|'height', 그룹 이름).
#: 표시 이름은 돌비(Dolby Atmos) 표준 채널 이름(2026-09-22 사용자 지시: 한글 채널명 대신 표준 표기).
#: 그룹 이름(프론트·센터·서브·서라운드·백·와이드·탑·하이트)은 설계 엔진의 그룹 키이고 `.secs` 의 XO 설정 키다 — 바꾸면
#: `Project._migrate_legacy` 에 이주를 넣는다(10-10: T* 그룹 '하이트' → '탑', 옛 파일은 `migrate_group_keys`).
CHINFO: dict[str, tuple[str, str, str]] = {
    'L': ('Left', 'main', '프론트'),
    'R': ('Right', 'main', '프론트'),
    'C': ('Center', 'main', '센터'),
    'SW1': ('Subwoofer 1', 'sub', '서브'),
    'SW2': ('Subwoofer 2', 'sub', '서브'),
    'SW3': ('Subwoofer 3', 'sub', '서브'),
    'SW4': ('Subwoofer 4', 'sub', '서브'),
    'SW5': ('Subwoofer 5', 'sub', '서브'),
    'SW6': ('Subwoofer 6', 'sub', '서브'),
    'SW7': ('Subwoofer 7', 'sub', '서브'),
    'SW8': ('Subwoofer 8', 'sub', '서브'),
    'Ls': ('Left Surround', 'main', '서라운드'),
    'Rs': ('Right Surround', 'main', '서라운드'),
    'Lb': ('Left Rear Surround', 'main', '백'),
    'Rb': ('Right Rear Surround', 'main', '백'),
    'Lw': ('Left Wide', 'main', '와이드'),
    'Rw': ('Right Wide', 'main', '와이드'),
    'Tfl': ('Left Top Front', 'top', '탑'),
    'Tfr': ('Right Top Front', 'top', '탑'),
    'Tml': ('Left Top Middle', 'top', '탑'),
    'Tmr': ('Right Top Middle', 'top', '탑'),
    'Tbl': ('Left Top Rear', 'top', '탑'),
    'Tbr': ('Right Top Rear', 'top', '탑'),
    'Hfl': ('Left Front Height', 'height', '하이트'),
    'Hfr': ('Right Front Height', 'height', '하이트'),
    'Hbl': ('Left Rear Height', 'height', '하이트'),
    'Hbr': ('Right Rear Height', 'height', '하이트'),
}

#: 그룹 이름 이주(10-10) — 탑 · 하이트를 나누기 전(`.secs` version 1)에는 T*(탑)의 그룹 이름이 '하이트' 였다.
TOP_GROUP = '탑'
HEIGHT_GROUP = '하이트'
LEGACY_TOP_GROUP = '하이트'


def migrate_group_keys(mapping):
    """옛 그룹 키 '하이트'(= 지금의 탑)를 '탑' 으로 옮긴 새 dict. '탑' 이 이미 있으면 그 값을 둔다. dict 가 아니면 그대로.

    `.secs` version 1 의 XO 설정(`project.xo` · `extra['calibration']` 의 `xo`·`xo_rec`)과 그때 저장된 계산 결과
    캐시(`calibration.load_run_cache`)가 부른다 — 새 파일의 '하이트' 는 진짜 하이트(H*)라 옮기면 안 된다."""
    if not isinstance(mapping, dict) or LEGACY_TOP_GROUP not in mapping:
        return mapping
    out = {}
    for key, value in mapping.items():                  # 자리(순서)는 그대로 — 그룹 콤보 순서가 흔들리지 않게
        if key != LEGACY_TOP_GROUP:
            out[key] = value
        elif TOP_GROUP not in mapping:
            out[TOP_GROUP] = value
    return out

#: 역할의 방 안 위치(0~1, 스크린이 위). 배치도가 그리고, 저역 통합 엔진이 "가까운 채널" 을 고를 때 쓴다(09-22).
ROLE_POSITIONS: dict[str, tuple[float, float]] = {
    'L': (.27, .18), 'R': (.73, .18), 'C': (.5, .12),
    'SW1': (.16, .4), 'SW2': (.84, .4), 'SW3': (.16, .78), 'SW4': (.84, .78),
    'SW5': (.5, .04), 'SW6': (.5, .97), 'SW7': (.04, .5), 'SW8': (.96, .5),
    'Ls': (.1, .62), 'Rs': (.9, .62), 'Lb': (.22, .93), 'Rb': (.78, .93),
    'Lw': (.06, .34), 'Rw': (.94, .34),
    'Tfl': (.36, .3), 'Tfr': (.64, .3), 'Tml': (.36, .5), 'Tmr': (.64, .5),
    'Tbl': (.36, .84), 'Tbr': (.64, .84),
    # 하이트(10-10): 앞 · 뒤 벽 위쪽 구석 — L·R · 스크린 · 서브 · 뒤 서라운드 글자와 겹치지 않는 자리
    'Hfl': (.14, .08), 'Hfr': (.86, .08), 'Hbl': (.07, .86), 'Hbr': (.93, .86),
}


#: 화면에 보이는 짧은 채널 코드(돌비 표준 약어). 내부 역할 ID 와 다른 것만 적는다.
#: 시트·.secs·Tesira 태그는 내부 ID 를 쓰고, 화면(칩·배치도·범례)만 이 코드를 쓴다.
#: 서브는 역할 ID 그대로 SW1 … SW8(10-10 사용자 결정 4A — LFE 는 콘텐츠 채널(.1) 이름이라 서브 출력에 맞지 않는다).
ROLE_CODES: dict[str, str] = {
    'Lb': 'Lrs', 'Rb': 'Rrs',
    'Tfl': 'Ltf', 'Tfr': 'Rtf', 'Tml': 'Ltm', 'Tmr': 'Rtm', 'Tbl': 'Ltr', 'Tbr': 'Rtr',
    'Hfl': 'Lfh', 'Hfr': 'Rfh', 'Hbl': 'Lrh', 'Hbr': 'Rrh',
}


def role_code(role: str) -> str:
    """화면용 돌비 표준 약어(예: Lb → Lrs, Tfl → Ltf, Hfl → Lfh). 모르는 역할은 그대로."""
    return ROLE_CODES.get(role, role)


#: 측정 제목에 흔히 쓰는 다른 표기 → 역할(대소문자·공백 무시). 역할 ID·`ROLE_CODES` 는 따로 넣지 않아도
#: 된다(`role_from_title` 이 함께 본다). 09-23 돌비룸 mdat: 제목이 `LRS`·`LTF`·`LTR` 인데 5단계 자동 배정은
#: 역할 ID(`Lb`·`Tfl`)로만 찾아 하나도 못 고르고, 빈 역할을 순서대로 채워 `DBA L` → L 같은 오배정이 났다.
ROLE_ALIASES: dict[str, str] = {
    'FL': 'L', 'FR': 'R', 'FC': 'C', 'CEN': 'C', 'CENTER': 'C',
    'SL': 'Ls', 'SR': 'Rs', 'LSS': 'Ls', 'RSS': 'Rs', 'SIDEL': 'Ls', 'SIDER': 'Rs',
    'SBL': 'Lb', 'SBR': 'Rb', 'LSB': 'Lb', 'RSB': 'Rb', 'BL': 'Lb', 'BR': 'Rb',
    'FWL': 'Lw', 'FWR': 'Rw',
    # 탑(천장) — Top Front · Middle · Rear
    'TFL': 'Tfl', 'TFR': 'Tfr',
    'TML': 'Tml', 'TMR': 'Tmr',
    'TRL': 'Tbl', 'TRR': 'Tbr', 'TBL': 'Tbl', 'TBR': 'Tbr',
    # 하이트(벽 위쪽) — Front · Rear Height. 10-10 까지는 탑에 섞여 있었다(FHL → Tfl, RHL → Tbl). 돌비 약어 Lfh · Rfh ·
    # Lrh · Rrh 는 `ROLE_CODES`, 역할 ID 꼴 HFL · HBL 은 역할 ID 가 받는다.
    'FHL': 'Hfl', 'FHR': 'Hfr', 'RHL': 'Hbl', 'RHR': 'Hbr', 'HRL': 'Hbl', 'HRR': 'Hbr',
    # 번호 없는 서브 = SW1(10-11 사용자: "서브우퍼 1개인 사람 많으니까 아무것도 안붙으면 SW1"). 'Subwoofer' · 한글 '서브' ·
    # '서브우퍼' 와 붙여 쓴 번호(`Sub2` · `서브우퍼2`)도 — 전에는 못 알아봤다. 띄어 쓴 번호(`SW 2`)는 mdat 불러오기에서
    # 위치 번호다(`step5_measure.split_position`).
    'SUB': 'SW1', 'SW': 'SW1', 'LFE': 'SW1', 'SUBWOOFER': 'SW1', '서브': 'SW1', '서브우퍼': 'SW1',
    # 옛 화면 표기 'LFE 1' … 'LFE 8'(10-10 까지)로 지은 측정 제목도 그대로 받는다
    **{f'{word}{i}': f'SW{i}' for word in ('LFE', 'SUB', 'SUBWOOFER', '서브', '서브우퍼') for i in range(1, 9)},
}


def _title_key(text: str) -> str:
    return ''.join(ch for ch in str(text or '').upper() if ch.isalnum())


def role_from_title(title: str, roles=None) -> str | None:
    """측정 제목이 **정확히** 채널 이름 하나일 때만 그 역할(예: `LRS` → Lb, `LTF` → Tfl, `LFE 1` → SW1).

    뒤에 측정 번호만 붙은 것(`L 1`, `LRS 2`)도 받는다. 그 밖의 말이 붙은 제목(`L EQ`, `L LF SUM`,
    `LRS EQ 예측`, `DBA L`)은 원측정이 아니라 보고 None — 보정 뒤 측정·합 측정을 원측정 자리에 넣으면
    안 되기 때문이다. `roles` 를 주면 그 안의 역할만 돌려준다(바이앰프 `L:LF` 는 기본 역할 `L` 로 비교).
    """
    words = str(title or '').split()
    if not words:
        return None
    table = {_title_key(alias): role for alias, role in ROLE_ALIASES.items()}
    table.update({_title_key(role): role for role in ROLES})
    table.update({_title_key(code): role for role, code in ROLE_CODES.items()})
    role = table.get(_title_key(' '.join(words)))          # `LFE 2` 는 통째로 먼저(→ SW2)
    while role is None and len(words) > 1 and words[-1].isdigit():
        words = words[:-1]                                  # `L 1` → `L`
        role = table.get(_title_key(' '.join(words)))
    if role is None:
        return None
    if roles is not None:
        bases = {str(r).split(':')[0] for r in roles}
        if role not in bases:
            return None
    return role


#: 목업 CHINFO Proxy 의 기본값(모르는 역할).
CHINFO_DEFAULT: tuple[str, str, str] = ('Custom', 'main', '기타')

#: 목업 PRESETS — 프리셋 이름 → 역할 순서.
PRESETS: dict[str, tuple[str, ...]] = {
    '2.0': ('L', 'R'),
    '2.1': ('L', 'R', 'SW1'),
    '2.2': ('L', 'R', 'SW1', 'SW2'),
    '5.1': ('L', 'R', 'C', 'SW1', 'Ls', 'Rs'),
    '7.1.4': ('L', 'R', 'C', 'SW1', 'Ls', 'Rs', 'Lb', 'Rb', 'Tfl', 'Tfr', 'Tbl', 'Tbr'),
    '9.1.6': ('L', 'R', 'C', 'SW1', 'Ls', 'Rs', 'Lb', 'Rb', 'Lw', 'Rw',
              'Tfl', 'Tfr', 'Tml', 'Tmr', 'Tbl', 'Tbr'),
}


def role_label(role: str) -> str:
    """역할의 표시 이름 — 돌비 표준 채널 이름(CHINFO[role][0])."""
    return CHINFO.get(role, CHINFO_DEFAULT)[0]


def role_kind(role: str) -> str:
    """역할의 종류 'main' | 'sub' | 'top'(천장) | 'height'(벽 위쪽) (목업 CHINFO[role][1], 'top' 은 10-10 에 나눔)."""
    return CHINFO.get(role, CHINFO_DEFAULT)[1]


def role_group(role: str) -> str:
    """역할이 속한 그룹.

    목업 CHINFO[role][2] 를 그대로 쓰되(프론트·센터·서라운드·백·와이드·탑·하이트·기타),
    서브우퍼는 계약대로 'sub' 을 돌려준다.
    """
    if role_kind(role) == 'sub':
        return 'sub'
    return CHINFO.get(role, CHINFO_DEFAULT)[2]


def role_order(role: str) -> int:
    """`ROLES` 안의 자리(바이앰프 `L:LF` 는 `L` 의 자리). 모르는 역할(사용자 정의)은 맨 뒤 = len(ROLES)."""
    base = str(role).split(':')[0]
    return ROLES.index(base) if base in ROLES else len(ROLES)


def ordered_speakers(speakers, subs_last: bool = False) -> list:
    """스피커를 `ROLES` 순서로(L R C SW1 SW2 … Ls Rs …) — 화면 목록 · 측정 순서가 함께 쓴다.

    10-10 사용자 "서브우퍼 측정 순서가 뒤죽박죽": 구성에 서브를 나중에 넣으면 `config.speakers` 끝에 붙어 레벨 표 ·
    타이밍 레퍼런스 목록에서 SW2 · SW3 이 맨 뒤에 나왔다. `.secs` 의 저장 순서는 그대로 두고 보일 때만 정렬한다.
    모르는 역할은 구성 순서 그대로 뒤에. `subs_last` = 서브를 맨 뒤로 몰기(5단계 측정 순서 — 10-09 사용자 결정
    '서브는 맨 뒤에 몰아서 측정', 서브끼리는 역할 순서)."""
    def key(speaker):
        role = str(getattr(speaker, 'role', speaker))
        return (subs_last and role_kind(role) == 'sub', role_order(role))
    return sorted(speakers, key=key)


# ──────────────────────────── 스피커 ────────────────────────────
@dataclass
class Band:
    """스피커의 한 대역과 그 측정 출력 채널."""

    band: str
    out: int
    pol: str = '+'

    def to_dict(self) -> dict[str, Any]:
        return {'band': str(self.band), 'out': int(self.out), 'pol': str(self.pol)}

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> 'Band':
        return cls(band=str(d['band']), out=int(d['out']), pol=str(d.get('pol', '+')))


@dataclass
class Speaker:
    """역할 하나 = 스피커 하나. `bands` 는 그 스피커가 쓰는 출력 대역들."""

    role: str
    size: str
    bands: list[Band] = field(default_factory=list)

    @property
    def kind(self) -> str:
        """테스트 전용(2026-09-20): 제품 코드는 `role_kind(role)` 을 직접 부른다."""
        return role_kind(self.role)

    @property
    def group(self) -> str:
        """현재 호출 없음(2026-09-20), 계약 API 라 유지. `role_group(role)` 과 같은 값."""
        return role_group(self.role)

    def to_dict(self) -> dict[str, Any]:
        return {
            'role': str(self.role),
            'size': str(self.size),
            'bands': [b.to_dict() for b in self.bands],
        }

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> 'Speaker':
        return cls(
            role=str(d['role']),
            size=str(d.get('size', 'small')),
            bands=[Band.from_dict(b) for b in d.get('bands', [])],
        )


def default_bands(role: str, start: int, biamp: bool = True) -> list[Band]:
    """목업 defaultBands: 메인 + 바이앰프 기본이면 LF/HF 두 출력, 그 외 한 출력."""
    kind = role_kind(role)
    if kind == 'main' and biamp:
        return [Band('LF', start, '+'), Band('HF', start + 1, '+')]
    return [Band('SUB' if kind == 'sub' else 'FULL', start, '+')]


@dataclass
class SpeakerConfig:
    """스피커 구성(2단계). 목업 S.spk + S.dev + S.preset + S.biampDefault."""

    speakers: list[Speaker] = field(default_factory=list)
    device: str = ''
    preset: str = 'custom'
    biamp_default: bool = True

    # ── 생성 ──
    @classmethod
    def from_preset(cls, name: str, device: str = '', biamp: bool = False) -> 'SpeakerConfig':
        """목업 applyPreset: 출력 채널을 1부터 대역 순서대로 매긴다."""
        if name not in PRESETS:
            raise KeyError(f'알 수 없는 프리셋: {name!r} (가능: {", ".join(PRESETS)})')
        speakers: list[Speaker] = []
        out = 1
        for role in PRESETS[name]:
            bands = default_bands(role, out, biamp)
            out += len(bands)
            speakers.append(
                Speaker(role=role, size='sub' if role_kind(role) == 'sub' else 'small', bands=bands)
            )
        return cls(speakers=speakers, device=device, preset=name, biamp_default=biamp)

    # ── 조회 ──
    def roles(self) -> list[str]:
        return [s.role for s in self.speakers]

    def sub_count(self) -> int:
        return sum(1 for s in self.speakers if s.role in SUB_ROLES)

    def used_outs(self) -> set[int]:
        return {b.out for s in self.speakers for b in s.bands}

    def next_free_out(self) -> int:
        """목업 nextFreeOut: 쓰이지 않은 가장 작은 출력 번호."""
        used = self.used_outs()
        out = 1
        while out in used:
            out += 1
        return out

    def groups(self) -> dict[str, list[Speaker]]:
        """그룹 이름 → 스피커들(구성 순서 유지)."""
        out: dict[str, list[Speaker]] = {}
        for s in self.speakers:
            out.setdefault(role_group(s.role), []).append(s)
        return out

    # ── 편집 ──
    def set_sub_count(self, n: int) -> None:
        """목업 setSubCount: 0~8 로 자르고, 늘릴 때는 빈 역할·빈 출력을 자동 배정."""
        n = max(0, min(8, int(n)))
        cur = self.sub_count()
        while cur < n:
            taken = set(self.roles())
            role = next((r for r in SUB_ROLES if r not in taken), None)
            if role is None:
                break
            self.speakers.append(
                Speaker(role=role, size='sub', bands=[Band('SUB', self.next_free_out(), '+')])
            )
            cur += 1
        while cur > n:
            idx = max(i for i, s in enumerate(self.speakers) if s.role in SUB_ROLES)
            self.speakers.pop(idx)
            cur -= 1
        self.preset = 'custom'

    def renumber(self) -> None:
        """목업 renumber: 출력 번호를 1부터 다시 매긴다."""
        out = 1
        for s in self.speakers:
            for b in s.bands:
                b.out = out
                out += 1

    # ── 검증 ──
    def validate(self) -> list[str]:
        """역할 중복 · 출력 채널 중복 · 대역 조합 오류를 사람이 읽을 메시지로."""
        msgs: list[str] = []

        seen_roles: set[str] = set()
        for role in self.roles():
            if role in seen_roles:
                msgs.append(f'역할 중복: {role}')
            seen_roles.add(role)

        seen_outs: dict[int, str] = {}
        for s in self.speakers:
            for b in s.bands:
                if not isinstance(b.out, int) or b.out < 1:
                    msgs.append(f'{s.role}: 출력 채널 번호가 잘못되었습니다({b.out!r}, 1 이상 정수)')
                    continue
                if b.out in seen_outs:
                    msgs.append(f'출력 채널 중복: {b.out} ({seen_outs[b.out]} ↔ {s.role})')
                else:
                    seen_outs[b.out] = s.role

        for s in self.speakers:
            names = tuple(b.band for b in s.bands)
            for b in s.bands:
                if b.band not in BANDS:
                    msgs.append(f'{s.role}: 알 수 없는 대역 {b.band!r}')
                if b.pol not in ('+', '-'):
                    msgs.append(f'{s.role}: 극성은 + 또는 - 여야 합니다({b.pol!r})')
            if not names:
                msgs.append(f'{s.role}: 대역이 없습니다')
            elif role_kind(s.role) == 'sub':
                if names != ('SUB',):
                    msgs.append(f'{s.role}: 서브우퍼는 SUB 대역 하나만 가질 수 있습니다({"/".join(names)})')
            else:
                if names == ('SUB',) or 'SUB' in names:
                    msgs.append(f'{s.role}: 메인/탑/하이트 스피커에는 SUB 대역을 쓸 수 없습니다')
                elif names not in BAND_SETS:
                    allowed = ' · '.join('/'.join(x) for x in BAND_SETS)
                    msgs.append(f'{s.role}: 대역 조합 오류 {"/".join(names)} (가능: {allowed})')
        return msgs

    def to_dict(self) -> dict[str, Any]:
        return {
            'speakers': [s.to_dict() for s in self.speakers],
            'device': str(self.device),
            'preset': str(self.preset),
            'biamp_default': bool(self.biamp_default),
        }

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> 'SpeakerConfig':
        return cls(
            speakers=[Speaker.from_dict(s) for s in d.get('speakers', [])],
            device=str(d.get('device', '')),
            preset=str(d.get('preset', 'custom')),
            biamp_default=bool(d.get('biamp_default', True)),
        )


# ──────────────────────────── DSP 프로파일 ────────────────────────────
@dataclass
class DspProfile:
    """목업 S.dsp + DSP_PROFILES 표."""

    profile: str = 'tesira'
    fir_taps: int = 2048
    fir_budget: int = 32768
    iir_bands: int = 16
    support_mode: str = 'iir'
    support_taps: int = 1024
    #: 채널당 올패스(위상 전용) 밴드 — 혼합위상 하이브리드의 GD 등화 예산(P3 계약 개정 7).
    #: PEQ(`iir_bands`)와 따로 센다: Tesira 는 PEQ 와 별도의 All-Pass 블록(채널당 최대 16밴드)이
    #: 있다(조사 09-23). 0 이면 올패스를 쓰지 않는다. 옛 `.secs` 는 프로파일 기본값으로 읽는다.
    allpass_bands: int = 16
    #: 베이스 매니지먼트 경로 지연(설계안 개정 6 · C1, 2026-09-24) — 메인 입력의 저역을 서브 버스로 보내는 송출에
    #: 채널마다 따로 걸 수 있는 지연의 수. 옛 7단계 엔진이 채널마다 서브 경로 지연을 정하는 데 썼다(개정 8 — 엔진은
    #: 2026-10-10 슬림화로 지웠고 칸은 `.secs` 형식이라 남긴다).
    #: 0 이거나 베이스 매니지먼트되는 메인 수보다 적으면 지금처럼 공통 하나(서브 미세 지연)로 되돌아간다.
    bm_path_delays: int = 16
    #: 설계안 개정 8(2026-09-24) — 옛 7단계 저역 설계 엔진이 쓰던 자원(엔진은 10-10 에 지움, 칸은 `.secs` 형식).
    #: `bm_path_polarity`: 베이스 매니지먼트 센드(메인 → 서브 버스)마다 극성을 뒤집을 수 있는가(서브 쪽 극성 — 메인
    #: 출력의 극성은 어느 경우에도 바꾸지 않는다, 사용자 결정 3). `bm_path_allpass`: 송출당 올패스 밴드 수.
    #: `path_peq_bands`·`path_allpass_bands`: 서포트 경로(입력 → 서포트)당 PEQ·올패스 밴드(결정 7, 다음 단계에서 씀).
    #: `per_output_lp`: 송출마다 LP 를 따로 걸 수 있는가(결정 2) — 없으면 서브 출력에 공통 LP 하나.
    #: 기본값은 보수적(없음) — 프리셋이 자원이 확실한 DSP 만 켠다. 옛 `.secs` 는 프로파일 기본값으로 읽는다.
    bm_path_polarity: bool = False
    bm_path_allpass: int = 0
    path_peq_bands: int = 0
    path_allpass_bands: int = 0
    per_output_lp: bool = False
    #: 채널 FIR 블록의 **하드웨어 상한**(2026-09-24, 설계안 개정 7) — `fir_taps`(실제로 쓰는 값,
    #: 균등 분배가 낮출 수 있다)와 뜻을 분리했다. 예전에는 `fir_taps` 하나가 둘을 겸해서, 한 번
    #: 분배하면 다음 분배의 "프로파일 최대" 기준 자체가 줄어드는 문제가 있었다(분배를 반복할수록
    #: 상한이 계속 낮아짐). 프리셋을 고를 때는 둘이 같은 값으로 시작한다.
    fir_taps_max: int = 2048
    #: 채널/서포트 FIR 탭이 **권장값(균등 분배)을 따르는 중인지** — True 면 6·7단계 진입·방식
    #: 변경 때마다 새 권장값으로 조용히 갱신한다(사용자가 손댄 적 없다는 뜻). 사용자가 콤보에서
    #: 다른 값을 고르면 False 로 바뀌고, [권장값으로] 를 누르거나 값이 다시 권장과 같아지면 True 로
    #: 돌아간다(2026-09-24 사용자: "선택하도록 하지 말고 권장값을 먼저 보여 달라"). 옛 `.secs`(이
    #: 필드가 없는 파일)는 `Project._migrate_legacy()`(2번 항목)가 저장된 값과 새로 계산한 권장값을
    #: 비교해 한 번만 정한다 — `None` 은 "아직 못 정했다"는 신호로 `from_dict` 가 남겨 둔다.
    fir_taps_auto: bool | None = True
    support_taps_auto: bool | None = True
    name: str = ''

    @classmethod
    def presets(cls) -> dict[str, 'DspProfile']:
        """목업 DSP_PROFILES 그대로. support_* 는 목업 S.dsp 기본값.

        올패스 밴드(09-23): Tesira All-Pass 블록 16밴드(Biamp 도움말). miniDSP 는 전용 올패스
        블록이 없고 PEQ 밴드를 나눠 써야 해서 0. Q-SYS 는 Tesira 와 같은 16, CamillaDSP 는
        biquad 를 제한 없이 이을 수 있어 64. 사용자 정의는 16.
        """
        table = (
            # 10-09 사용자: 실측 연구값으로 다시(`테시라 vs 데논 DSP 비교/근거.md`). Tesira SERVER(DSP-2 3장) = FIR 블록 최대
            # 2,048탭, 2,048 × 25ch = 51,200탭이 실측 최대. Q-SYS Core 8 Flex / 110f = FIR 블록 최대 16,384탭, 실사용 최대
            # 2,048 × 32ch = 65,536탭(110f 실측, 고정 FIR 풀). miniDSP = 채널당 2,048탭, 합계 4,096탭.
            ('camilla', 'PC · CamillaDSP', 65536, 65536 * 16, 64, 64, 64),
            ('tesira', 'Biamp Tesira SERVER (DSP-2 ×3)', 2048, 51200, 16, 16, 16),
            ('qsys', 'Q-SYS Core 8 Flex / 110f', 16384, 65536, 24, 16, 16),
            ('minidsp', 'miniDSP Flex / 2x4 HD', 2048, 4096, 10, 0, 0),
            ('custom', '사용자 정의', 2048, 32768, 16, 16, 16),
        )
        # 베이스 매니지먼트 경로 지연(09-24): Tesira·Q-SYS 는 매트릭스 믹서 크로스포인트 딜레이(또는 입력별 딜레이 블록)로
        # 채널마다 둘 수 있어 16, CamillaDSP 는 파이프라인에 제한이 없어 64, miniDSP 는 베이스 매니지먼트 송출에 딜레이
        # 자리가 없어 0(공통 하나). 사용자 정의는 16.
        # 개정 8 자원(09-24): (송출 극성, 송출 올패스, 경로 PEQ, 경로 올패스, 송출별 LP). Tesira 는 매트릭스 믹서
        # 크로스포인트에 극성·필터가 없어 보수적으로 전부 끔(별도 블록으로 만들 수는 있지만 확인 전이다). Q-SYS 는 믹서
        # 뒤에 블록을 자유롭게 둘 수 있고, CamillaDSP 는 파이프라인 제한이 없다. miniDSP 는 자리가 없다.
        lowband = {
            'tesira': (False, 0, 0, 0, False),
            'minidsp': (False, 0, 0, 0, False),
            'qsys': (True, 2, 4, 2, True),
            'camilla': (True, 4, 16, 8, True),
            'custom': (False, 0, 0, 0, False),
        }
        out = {}
        for key, label, taps, budget, bands, allpass, bm_delays in table:
            polarity, bm_allpass, path_peq, path_allpass, per_output = lowband[key]
            out[key] = cls(profile=key, fir_taps=taps, fir_taps_max=taps, fir_budget=budget,
                           iir_bands=bands, support_mode='iir', support_taps=1024,
                           allpass_bands=allpass, bm_path_delays=bm_delays,
                           bm_path_polarity=polarity, bm_path_allpass=bm_allpass,
                           path_peq_bands=path_peq, path_allpass_bands=path_allpass,
                           per_output_lp=per_output, name=label)
        return out

    def allocate_taps(self, config: SpeakerConfig, phase: str = 'mixed', path_count: int = 0,
                      min_taps: int = 256) -> tuple[int, int]:
        """**권장** (채널 FIR 탭, 서포트 FIR 탭) — 저역 통합까지 포함해 예산을 고르게 나눈다
        (2026-09-24 사용자: "FIR 탭 분배는 저역 통합까지 포함해서 고르게" — 예전에는 채널 FIR 이
        먼저 최대부터 가져가고 서포트가 남는 것만 받아 늘 모자랐다).

        순수 함수다(`self` 를 바꾸지 않는다) — 화면이 "지금 값이 권장과 같은가"(자동/직접)를
        아무 때나 다시 물어볼 수 있어야 해서, 실제로 `dsp.fir_taps` 에 적용하는 것은 호출자 몫이다.

        FIR 을 쓰는 소비자 수 = (Mixed Phase 일 때 채널 FIR 수 = 대역 수) +
        (서포트 필터가 hybrid/fir 일 때 서포트 경로 수). 소비자가 없으면(Minimum Phase + IIR
        서포트) 분배할 것이 없어 `(fir_taps, support_taps)` 를 그대로 돌려준다 — 결정 원문:
        "Minimum Phase + 서포트 IIR 이면 FIR 소비자 0 → 분배 없음."

        공통 탭 수 T = 예산 ÷ 소비자 수를 넘지 않는 가장 큰 값, 채널당 하드웨어 상한
        (`fir_taps_max`) 이하. 반올림은 2의 거듭제곱 — DSP 마다 FIR 탭 그레인이 실제로 얼마나
        촘촘한지(예: 256 단위) 확인된 사양이 없고, 하드웨어 상한 자체가 늘 2^n 이라는 보장도 없어
        아는 값(상한)만 쓰고 중간 눈금은 보수적으로 2^n 만 쓴다 — 256 단위로 더 촘촘히 쓸 수 있는
        DSP 라도, 틀리게 더 크게 잡는 것보다 작게 잡는 편이 안전하다(예산 초과보다 낫다).
        """
        band_count = max(1, sum(len(s.bands) for s in config.speakers))
        chan_consumers = band_count if phase == 'mixed' else 0
        sup_consumers = int(path_count) if self.support_mode in ('hybrid', 'fir') else 0
        consumers = chan_consumers + sup_consumers
        if consumers <= 0:
            return int(self.fir_taps), int(self.support_taps)
        budget = int(self.fir_budget)
        ceiling = max(int(self.fir_taps_max), min_taps)
        ceiling_pow2 = 1 << (ceiling.bit_length() - 1)      # ceiling 이하 가장 큰 2^n
        target = budget // consumers
        taps = (1 << (target.bit_length() - 1)) if target >= 1 else min_taps
        taps = max(min_taps, min(taps, ceiling_pow2))
        while taps > min_taps and taps * consumers > budget:
            taps //= 2                                      # 반올림이 예산을 넘기면 한 단 내려간다
        channel = taps if chan_consumers else int(self.fir_taps)
        support = taps if sup_consumers else int(self.support_taps)
        return channel, support

    def fir_used(self, config: SpeakerConfig, phase: str, path_count: int = 0) -> dict[str, int]:
        """목업 firUsed: 채널 FIR + 서포트 FIR 합계."""
        band_count = sum(len(s.bands) for s in config.speakers)
        chan = 0 if phase == 'minphase' else band_count * self.fir_taps
        sup = 0 if self.support_mode == 'iir' else int(path_count) * self.support_taps
        return {'chan': chan, 'sup': sup, 'total': chan + sup}

    def to_dict(self) -> dict[str, Any]:
        return {
            'profile': str(self.profile),
            'fir_taps': int(self.fir_taps),
            'fir_taps_max': int(self.fir_taps_max),
            'fir_budget': int(self.fir_budget),
            'iir_bands': int(self.iir_bands),
            'support_mode': str(self.support_mode),
            'support_taps': int(self.support_taps),
            'allpass_bands': int(self.allpass_bands),
            'bm_path_delays': int(self.bm_path_delays),
            'bm_path_polarity': bool(self.bm_path_polarity),
            'bm_path_allpass': int(self.bm_path_allpass),
            'path_peq_bands': int(self.path_peq_bands),
            'path_allpass_bands': int(self.path_allpass_bands),
            'per_output_lp': bool(self.per_output_lp),
            'fir_taps_auto': bool(self.fir_taps_auto),
            'support_taps_auto': bool(self.support_taps_auto),
            'name': str(self.name),
        }

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> 'DspProfile':
        fir_taps = int(d.get('fir_taps', 2048))
        preset = cls.presets().get(str(d.get('profile', 'tesira'))) or cls()
        return cls(
            profile=str(d.get('profile', 'tesira')),
            fir_taps=fir_taps,
            # 09-24 이전 파일에는 없다 → 그때는 `fir_taps` 가 상한이었으니 저장된 값을 그대로 상한으로
            # 읽는다(프로파일을 알면 그 프로파일 기본 상한이 더 정확 — 사용자가 상한 밑으로
            # 분배해 뒀을 수 있어서).
            fir_taps_max=int(d.get('fir_taps_max', getattr(
                cls.presets().get(str(d.get('profile', 'tesira'))), 'fir_taps_max', fir_taps))),
            fir_budget=int(d.get('fir_budget', 32768)),
            iir_bands=int(d.get('iir_bands', 16)),
            support_mode=str(d.get('support_mode', 'iir')),
            support_taps=int(d.get('support_taps', 1024)),
            # 09-23 이전 파일에는 없다 → 그 프로파일의 기본값(모르는 프로파일은 16).
            allpass_bands=int(d.get('allpass_bands', getattr(
                cls.presets().get(str(d.get('profile', 'tesira'))), 'allpass_bands', 16))),
            # 09-24 이전 파일에는 없다 → 그 프로파일의 기본값(모르는 프로파일은 16).
            bm_path_delays=int(d.get('bm_path_delays', getattr(
                cls.presets().get(str(d.get('profile', 'tesira'))), 'bm_path_delays', 16))),
            # 09-24 이전 파일에는 없다 → `None`(아직 모름). `Project._migrate_legacy()` 가
            # 저장된 탭 값을 권장값과 비교해 한 번만 True/False 로 정한다(`fir_taps_max` 클래스
            # 독스트링 참고).
            fir_taps_auto=bool(d['fir_taps_auto']) if 'fir_taps_auto' in d else None,
            support_taps_auto=bool(d['support_taps_auto']) if 'support_taps_auto' in d else None,
            # 개정 8(09-24) 이전 파일에는 없다 → 그 프로파일의 기본값(모르는 프로파일은 보수적 기본값).
            bm_path_polarity=bool(d.get('bm_path_polarity', preset.bm_path_polarity)),
            bm_path_allpass=int(d.get('bm_path_allpass', preset.bm_path_allpass)),
            path_peq_bands=int(d.get('path_peq_bands', preset.path_peq_bands)),
            path_allpass_bands=int(d.get('path_allpass_bands', preset.path_allpass_bands)),
            per_output_lp=bool(d.get('per_output_lp', preset.per_output_lp)),
            name=str(d.get('name', '')),
        )


# ──────────────────────────── 타깃 커브 ────────────────────────────
#: 목업 S.tgt 초기값.
DEFAULT_TARGET_POINTS: tuple[tuple[float, float], ...] = (
    (25.0, 2.0), (80.0, 2.0), (200.0, 0.0), (1000.0, 0.0), (16000.0, -2.0),
)


@dataclass
class TargetCurve:
    """제어점 사이를 로그 주파수 스무스스텝으로 잇는 타깃(목업 targetAt)."""

    points: list[tuple[float, float]] = field(
        default_factory=lambda: [tuple(p) for p in DEFAULT_TARGET_POINTS]
    )

    def __post_init__(self) -> None:
        self.sort()

    def sort(self) -> None:
        """주파수 오름차순 유지(목업 targetAt 의 slice().sort())."""
        self.points = [(float(f), float(db)) for f, db in sorted(self.points, key=lambda p: p[0])]

    def at(self, f: float) -> float:
        """목업 targetAt 과 수치 동일. 범위 밖은 끝점 값."""
        pts = self.points
        if not pts:
            return 0.0
        f = float(f)
        if f <= pts[0][0]:
            return float(pts[0][1])
        if f >= pts[-1][0]:
            return float(pts[-1][1])
        for i in range(len(pts) - 1):
            (fa, da), (fb, db) = pts[i], pts[i + 1]
            if fa <= f <= fb:
                t = (np.log(f) - np.log(fa)) / (np.log(fb) - np.log(fa))
                u = t * t * (3.0 - 2.0 * t)
                return float(da + (db - da) * u)
        return 0.0

    def sample(self, freqs: np.ndarray) -> np.ndarray:
        """주파수 배열에 대한 타깃 dB 배열."""
        arr = np.asarray(freqs, dtype=float)
        return np.array([self.at(float(x)) for x in arr.ravel()], dtype=float).reshape(arr.shape)

    @classmethod
    def presets(cls) -> dict[str, 'TargetCurve']:
        """목업 setTargetPreset 의 3종(flat · harman · cinema)."""
        return {
            'flat': cls([(25.0, 0.0), (200.0, 0.0), (1000.0, 0.0), (16000.0, 0.0)]),
            'harman': cls([(25.0, 6.0), (60.0, 5.0), (120.0, 1.0), (1000.0, 0.0), (16000.0, -3.0)]),
            'cinema': cls([(25.0, 4.0), (80.0, 3.0), (200.0, 0.0), (1000.0, -1.0), (16000.0, -5.0)]),
        }

    def to_dict(self) -> dict[str, Any]:
        return {'points': [[float(f), float(db)] for f, db in self.points]}

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> 'TargetCurve':
        if isinstance(d, list):  # 관대한 입력: [[f, db], ...]
            return cls([(float(f), float(db)) for f, db in d])
        return cls([(float(f), float(db)) for f, db in d.get('points', [])])


# ──────────────────────────── 프로젝트 (.secs) ────────────────────────────
#: 목업 S.curt — 보정 커튼(저역 한계, 고역 한계) Hz.
DEFAULT_CURTAIN: tuple[float, float] = (24.0, 300.0)

#: 그룹 XO 의 관례적 출발값(Hz). **엔진은 이 값을 스스로 쓰지 않는다** —
#: XO 는 자동 선택하지 않고 6단계 커튼으로 반자동 선택하는 것이 핵심 결정이라,
#: `Project.xo` 가 비어 있으면 그 그룹은 풀레인지로 둔다. UI 가 처음 값을
#: 채워 넣을 때만 이 상수를 쓴다.
DEFAULT_XO_HZ: float = 80.0

#: 목업 S.bc (xoGroup 은 Project.xo 로 빠지므로 여기에는 두지 않는다).
DEFAULT_BC: dict[str, Any] = {'mode': 'full', 'phase': 'minphase', 'target_preset': 'flat'}

#: 목업 S.mso 중 계약이 정한 키들(7단계 **설정**). 계산된 경로는 개정 8(09-24)부터 `extra['bass']['paths']` 에 있다
#: — 옛 `mso['paths']` 는 읽지도 쓰지도 않는다.
DEFAULT_MSO: dict[str, Any] = {
    'lo': 20, 'hi': 150, 'hp': 25, 'maxGain': -6, 'maxDelay': 20, 'maxPer': 4,     # 가까운 3채널 + 서브(09-22)
    'focus': 50, 'active': {}, 'level': {}, 'decay': 1,
}
#: `mso['decay']` 는 빠른 감쇠 단계 int 0~3(0 끔 · 1 응답 우선 · 2 균형 · 3 감쇠 우선, 결정 8 — 09-24). 옛 bool 은 엔진이
#: 읽지 않고 기본 1 로 봤다(옛 `lowband.LowbandDesigner.decay_level`, 10-10 에 지움 — 값은 `.secs` 형식이라 남긴다).

#: 7단계 '최적화 목표' 5단계(2026-09-24 사용자: 슬라이더가 아니라 뜻이 뚜렷한 단계로).
#: `bass.integration_cost` 의 가중 w = focus/100 은 그대로다 — 이 값은 그 100단계 눈금 중
#: 딱 5개만 고르게 한 것뿐이다(옛 7단계 화면 · 엔진은 2026-10-10 에 지웠고 `.secs` 의 `focus` 반올림에만 쓴다).
FOCUS_STEPS: tuple[int, ...] = (0, 25, 50, 75, 100)


def snap_focus(value: Any) -> int:
    """`focus` 를 `FOCUS_STEPS` 중 가장 가까운 값으로(옛 `.secs` 의 중간 값 이관용)."""
    try:
        value = float(value)
    except (TypeError, ValueError):
        return 50
    return min(FOCUS_STEPS, key=lambda step: abs(step - value))

#: 목업 S.tesira.
DEFAULT_TESIRA: dict[str, Any] = {'ip': '', 'method': 'ttp', 'slot': 1}

#: 목업 S.verify.
DEFAULT_VERIFY: dict[str, Any] = {'A': None, 'B': None}

#: `.secs` 형식 판. 2(10-10) = 그룹 '하이트' 가 벽 위쪽 하이트(H*)를 뜻한다 — 1 에서는 탑(T*)이었다
#: (불러올 때 `_migrate_legacy` 4번이 '탑' 으로 옮긴다). 3(10-10) = 보정 설계 설정의 저역 감쇠 키 이름이 `mimo_*` 다 —
#: 2 이하는 `art_*` 였다(불러올 때 5번이 옮긴다, `LEGACY_CALIBRATION_KEYS`).
PROJECT_VERSION = 3

#: `.secs` 판 2 이하의 보정 설계 설정 키 → 판 3 키(10-10 기능 이름 정리). `extra['calibration']` 최상위와
#: `subs_manual` 의 서브마다(`art_support`) 옮긴다. 계산 결과 캐시의 같은 이름은 `calibration.load_run_cache` 가 옮긴다.
LEGACY_CALIBRATION_KEYS: dict[str, str] = {
    'art_on': 'mimo_on',
    'art_low': 'mimo_low',
    'art_max_supports': 'mimo_max_supports',
}
LEGACY_SUB_KEYS: dict[str, str] = {'art_support': 'mimo_support'}


def rename_keys(mapping, table: dict):
    """dict 의 옛 키를 새 키로 옮긴 새 dict(자리 순서 그대로, 새 키가 이미 있으면 그 값을 둔다). dict 가 아니면 그대로."""
    if not isinstance(mapping, dict) or not any(old in mapping for old in table):
        return mapping
    out = {}
    for key, value in mapping.items():
        new = table.get(key)
        if new is None:
            out[key] = value
        elif new not in mapping:
            out[new] = value
    return out


def migrate_calibration_keys(calibration):
    """판 2 이하 `extra['calibration']` → 판 3 키(`LEGACY_CALIBRATION_KEYS` · 서브마다 `LEGACY_SUB_KEYS`)."""
    if not isinstance(calibration, dict):
        return calibration
    out = rename_keys(calibration, LEGACY_CALIBRATION_KEYS)
    manual = out.get('subs_manual')
    if isinstance(manual, dict):
        out = {**out, 'subs_manual': {role: rename_keys(entry, LEGACY_SUB_KEYS) for role, entry in manual.items()}}
    return out

_PROJECT_KEYS = frozenset({
    'version', 'name', 'config', 'dsp', 'target', 'curtain', 'xo', 'bc', 'mso',
    'measurements', 'verify', 'tesira', 'snapshots', 'extra',
})


def _plain(value: Any) -> Any:
    """numpy 타입을 순수 파이썬 값으로 바꾼다(JSON 저장용)."""
    if isinstance(value, np.generic):
        return value.item()
    if isinstance(value, np.ndarray):
        return [_plain(v) for v in value.tolist()]
    if isinstance(value, dict):
        return {str(k): _plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_plain(v) for v in value]
    return value


@dataclass
class Project:
    """`.secs` 파일 하나 = 이 객체의 JSON."""

    version: int = PROJECT_VERSION
    name: str = ''
    config: SpeakerConfig = field(default_factory=SpeakerConfig)
    dsp: DspProfile = field(default_factory=DspProfile)
    target: TargetCurve = field(default_factory=TargetCurve)
    curtain: tuple[float, float] = DEFAULT_CURTAIN
    xo: dict[str, float] = field(default_factory=dict)
    bc: dict[str, Any] = field(default_factory=lambda: dict(DEFAULT_BC))
    mso: dict[str, Any] = field(default_factory=lambda: json.loads(json.dumps(DEFAULT_MSO)))
    measurements: list[dict] = field(default_factory=list)
    verify: dict = field(default_factory=lambda: dict(DEFAULT_VERIFY))
    tesira: dict = field(default_factory=lambda: dict(DEFAULT_TESIRA))
    snapshots: list[dict] = field(default_factory=list)
    extra: dict = field(default_factory=dict)

    # ── 직렬화 ──
    def to_dict(self) -> dict[str, Any]:
        # 모르는 키(상위 버전 .secs 의 것)는 최상위에 되돌려 쓴다. 알려진 키가 이긴다.
        d: dict[str, Any] = {str(k): _plain(v) for k, v in self.extra.items()
                             if str(k) not in _PROJECT_KEYS}
        d.update({
            'version': int(self.version),
            'name': str(self.name),
            'config': self.config.to_dict(),
            'dsp': self.dsp.to_dict(),
            'target': self.target.to_dict(),
            'curtain': [float(self.curtain[0]), float(self.curtain[1])],
            'xo': {str(k): float(v) for k, v in self.xo.items()},
            'bc': _plain(self.bc),
            'mso': _plain(self.mso),
            'measurements': _plain(self.measurements),
            'verify': _plain(self.verify),
            'tesira': _plain(self.tesira),
            'snapshots': _plain(self.snapshots),
        })
        return d

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> 'Project':
        curtain = d.get('curtain', DEFAULT_CURTAIN)
        extra = dict(d.get('extra', {}) or {})
        for key, value in d.items():  # 모르는 최상위 키도 extra 로 보존
            if key not in _PROJECT_KEYS:
                extra[key] = value
        project = cls(
            version=int(d.get('version', 1)),
            name=str(d.get('name', '')),
            config=SpeakerConfig.from_dict(d.get('config', {}) or {}),
            dsp=DspProfile.from_dict(d.get('dsp', {}) or {}),
            target=TargetCurve.from_dict(d.get('target', {}) or {}),
            curtain=(float(curtain[0]), float(curtain[1])),
            xo={str(k): float(v) for k, v in (d.get('xo', {}) or {}).items()},
            bc=dict(d.get('bc', DEFAULT_BC) or {}),
            mso=dict(d.get('mso', DEFAULT_MSO) or {}),
            measurements=list(d.get('measurements', []) or []),
            verify=dict(d.get('verify', DEFAULT_VERIFY) or {}),
            tesira=dict(d.get('tesira', DEFAULT_TESIRA) or {}),
            snapshots=list(d.get('snapshots', []) or []),
            extra=extra,
        )
        project._migrate_legacy()
        return project

    def _migrate_legacy(self) -> None:
        """옛 `.secs` 를 열 때 뜻이 바뀐 자리만 한 번 정리한다(2026-09-24, 설계안 개정 7).

        1. **위상 방식 `'fir'`('+ 채널 FIR') 를 없앴다** — 6단계 화면에서 고를 수 없고, 남아 있던
           값은 `'minphase'` 로 바꾼다. `_migrated_fir_phase` 플래그는 옛 6단계 화면이 힌트로 한 번 알리는 데
           썼다(그 화면은 2026-10-10 슬림화로 지웠다). 이 변환은 오직 **저장된 값** 을 위한 것이다.
        2. **FIR 탭 자동/직접 표시**(`fir_taps_auto`/`support_taps_auto`) 가 없는 파일 — 이 기능
           이전에 저장됐다는 뜻이다. 지금 값이 그 시점의 권장값과 같으면 '자동', 다르면(사용자가
           손으로 바꿔 둔 값) '직접' 으로 본다.
        3. **최적화 목표(`focus`)** 를 5단계(`FOCUS_STEPS`) 중 가장 가까운 값으로 반올림한다 —
           슬라이더가 100단계에서 5단계로 바뀌어(2026-09-24 사용자 결정) 중간 값은 뜻이 없다.
        4. **탑 · 하이트 분리**(2026-10-10, version 1 → 2): 옛 파일은 천장 탑(T*)의 그룹 이름이 '하이트' 였다 — 그룹을
           키로 쓰는 XO 설정(`xo` · `extra['calibration']` 의 `xo`·`xo_rec`)의 '하이트' 를 '탑' 으로 옮긴다. 새 파일
           (version 2)의 '하이트' 는 벽 위쪽 하이트(H*)라 손대지 않는다.
        5. **저역 감쇠 설정 키 이름**(2026-10-10, version 2 이하 → 3): 기능 이름을 MIMO 로 바꾸며 `extra['calibration']` 의
           `art_on` · `art_low` · `art_max_supports` 와 서브마다 `art_support` 를 `mimo_*` 로 옮긴다(값 · 뜻은 그대로).
        """
        if self.bc.get('phase') == 'fir':
            self.bc['phase'] = 'minphase'
            self.extra['_migrated_fir_phase'] = True
        if self.dsp.fir_taps_auto is None or self.dsp.support_taps_auto is None:
            phase = str(self.bc.get('phase', 'minphase'))
            chan_rec, sup_rec = self.dsp.allocate_taps(self.config, phase, self.bass_path_count())
            if self.dsp.fir_taps_auto is None:
                self.dsp.fir_taps_auto = int(self.dsp.fir_taps) == chan_rec
            if self.dsp.support_taps_auto is None:
                self.dsp.support_taps_auto = int(self.dsp.support_taps) == sup_rec
        self.mso['focus'] = snap_focus(self.mso.get('focus', 50))
        if int(self.version) < 2:
            self.xo = migrate_group_keys(self.xo)
            calibration = self.extra.get('calibration')
            if isinstance(calibration, dict):
                self.extra['calibration'] = {**calibration, **{key: migrate_group_keys(calibration[key])
                                                               for key in ('xo', 'xo_rec') if key in calibration}}
        if int(self.version) < 3:
            calibration = self.extra.get('calibration')
            if isinstance(calibration, dict):
                self.extra['calibration'] = migrate_calibration_keys(calibration)
        if int(self.version) < PROJECT_VERSION:
            self.version = PROJECT_VERSION

    def bass_path_count(self) -> int:
        """7단계가 푼 서포트 경로 수(`extra['bass'].paths`, 개정 8). `mso['paths']` 는 더 쓰지 않는다."""
        bass = self.extra.get('bass') if isinstance(self.extra.get('bass'), dict) else {}
        return len(bass.get('paths') or {})

    # ── FIR 탭 균등 분배(2026-09-24, 설계안 개정 7) ──
    def apply_recommended_taps(self) -> bool:
        """`dsp.fir_taps_auto`/`support_taps_auto` 를 따르는 값만 새 권장값으로 맞춘다.

        6단계(위상 방식)·7단계(서포트 필터·서포트 경로 수)가 진입하거나 바뀔 때마다 부른다
        (결정 2: "6단계 진입·모드 변경·7단계 서포트 필터 변경 때 자동으로 균등 분배"). 두
        단계가 같은 계산을 중복해서 두지 않도록 한곳(`DspProfile.allocate_taps`)만 쓴다.
        값이 바뀌면 True(호출자가 재계산을 걸어야 한다는 뜻).
        """
        phase = str(self.bc.get('phase', 'minphase'))
        if phase not in ('minphase', 'mixed'):
            phase = 'minphase'
        chan_rec, sup_rec = self.dsp.allocate_taps(self.config, phase, self.bass_path_count())
        changed = False
        if self.dsp.fir_taps_auto and int(self.dsp.fir_taps) != chan_rec:
            self.dsp.fir_taps = chan_rec
            changed = True
        if self.dsp.support_taps_auto and int(self.dsp.support_taps) != sup_rec:
            self.dsp.support_taps = sup_rec
            changed = True
        return changed

    # ── 파일 ──
    def save(self, path) -> None:
        p = Path(path)
        if p.parent and not p.parent.exists():
            p.parent.mkdir(parents=True, exist_ok=True)
        text = json.dumps(self.to_dict(), ensure_ascii=False, indent=2) + '\n'
        p.write_text(text, encoding='utf-8')

    @classmethod
    def load(cls, path) -> 'Project':
        data = json.loads(Path(path).read_text(encoding='utf-8'))
        return cls.from_dict(data)

    # ── 편의 ──
    @classmethod
    def from_preset(cls, name: str, **kwargs: Any) -> 'Project':
        return cls(config=SpeakerConfig.from_preset(name), name=kwargs.pop('name', ''), **kwargs)

    def validate(self) -> list[str]:
        """현재 호출 없음(2026-09-20), 계약 API 라 유지.

        화면(2단계)과 테스트는 `project.config.validate()` 를 직접 부른다.
        """
        return self.config.validate()


__all__ = [
    'ROLES', 'SUB_ROLES', 'BANDS', 'BAND_SETS', 'CHINFO', 'PRESETS',
    'role_label', 'role_kind', 'role_group', 'default_bands', 'role_code', 'role_from_title',
    'role_order', 'ordered_speakers', 'migrate_group_keys', 'TOP_GROUP', 'HEIGHT_GROUP', 'PROJECT_VERSION',
    'LEGACY_CALIBRATION_KEYS', 'LEGACY_SUB_KEYS', 'rename_keys', 'migrate_calibration_keys',
    'ROLE_CODES', 'ROLE_ALIASES',
    'Band', 'Speaker', 'SpeakerConfig', 'DspProfile', 'TargetCurve', 'Project',
    'DEFAULT_TARGET_POINTS', 'DEFAULT_BC', 'DEFAULT_MSO', 'DEFAULT_TESIRA', 'DEFAULT_VERIFY',
    'DEFAULT_CURTAIN', 'DEFAULT_XO_HZ', 'FOCUS_STEPS', 'snap_focus',
]
