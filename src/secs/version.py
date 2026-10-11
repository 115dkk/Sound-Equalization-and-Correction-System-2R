# -*- coding: utf-8 -*-
"""제품 이름과 버전 — 여기 한 곳만 고친다.

표기(2026-09-22 사용자 지시): **"SECS 2 Alpha 1.1.2"** = 이름(`APP_NAME`, 세대 번호 포함) + 단계(`STAGE`) + 빌드 번호(`BUILD`).
- `BUILD` 는 주.부.수. 고치기만 하면 수(1.0.0 → 1.0.1), 기능이 늘거나 화면·동작이 눈에 띄게 바뀌면
  부(→ 1.1.0), 저장 파일(`.secs`)이나 DSP 시트의 뜻이 바뀌면 주(→ 2.0.0). **사용자에게 나가는 변경을
  커밋할 때마다 올린다** — 올리면 `변경 기록.md` 맨 위에 항목을 쓰고, 빌드(`build_exe.ps1 -Archive`)하고,
  git 태그 `v<__version__>` 를 붙인다(절차: CLAUDE.md "버전·릴리스").
- `STAGE` 는 실장비 검증 전에는 Alpha, 실측을 통과하면 Beta, 그 뒤에는 빈 문자열(정식).
- 화면(창 제목·사이드바)·`--version`·`--check`·exe 파일 이름·작업 관리자 표시가 모두 여기서 나온다.
- (옛 `secs.RELEASE_VERSION`(1.41, secs4 원본 엔진의 판 번호)은 2026-10-10 슬림화로 그 엔진과 함께 지웠다.)
- `APP_NAME` 은 가칭이다. 패키지 이름 `secs` 와 확장자 `.secs` 는 이름이 바뀌어도 그대로 둔다(저장 파일 호환).
"""
from __future__ import annotations

APP_NAME = 'SECS 2'
STAGE = 'Beta'
BUILD = '1.1.0'

#: 사람이 읽는 꼴 — 'Alpha 1.1.2'(사이드바 두 번째 줄)
VERSION_LABEL = f'{STAGE} {BUILD}'
#: 기계가 읽는 꼴(유의적 버전) — '2.0.0-alpha.1.0.0'. git 태그는 앞에 v 를 붙인다.
__version__ = f'2.0.0-{STAGE.lower()}.{BUILD}'      # 단계 꼬리표는 STAGE 에서(10-11 Beta 부터 — 전에는 'alpha' 를 박아 두었다)


def version_text() -> str:
    """'SECS 2 Alpha 1.1.2' — 창 제목·`--version`·exe 이름용."""
    return f'{APP_NAME} {VERSION_LABEL}'


__all__ = ['APP_NAME', 'STAGE', 'BUILD', 'VERSION_LABEL', '__version__', 'version_text']
