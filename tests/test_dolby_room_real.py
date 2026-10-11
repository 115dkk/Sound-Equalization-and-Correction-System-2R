# -*- coding: utf-8 -*-
"""돌비 룸 실측(2026-09-23)에서 찾은 결함의 회귀 테스트 — 5단계 mdat 역할 배정.

돌비 표기 제목(`LRS`·`LTF`·`LTR`…)을 역할로 못 고르고, 빈 역할을 순서대로 채워 `DBA L`→L 같은 오배정
→ `model.role_from_title`·`mdat_loader.suggest_assignment`. (옛 설계 엔진·시트에 대한 2~4번 검사는
2026-10-10 슬림화로 엔진과 함께 지웠다.)

실측 파일 테스트는 파일이 있을 때만 돈다: `tests/data/250310 돌비룸 보정.mdat`(git 제외, 167 MB) 또는
환경변수 `SECS_DOLBY_MDAT`.
"""
from __future__ import annotations

import os
from pathlib import Path

import pytest

from conftest import load_engine

load_engine("package")

import secs.model as model  # noqa: E402
from secs.mdat_loader import suggest_assignment  # noqa: E402

MDAT = Path(os.environ.get('SECS_DOLBY_MDAT')
            or Path(__file__).resolve().parent / 'data' / '250310 돌비룸 보정.mdat')
needs_file = pytest.mark.skipif(not MDAT.is_file(), reason=f'실측 파일 없음: {MDAT}')

#: 돌비룸 mdat 의 원측정 제목 → SECS 역할.
DOLBY = {'L': 'L', 'C': 'C', 'R': 'R', 'LS': 'Ls', 'RS': 'Rs', 'LRS': 'Lb', 'RRS': 'Rb',
         'LTF': 'Tfl', 'RTF': 'Tfr', 'LTR': 'Tbl', 'RTR': 'Tbr'}
ROLES_714 = [s.role for s in model.SpeakerConfig.from_preset('7.1.4').speakers]


# ──────────────────────────── 1. 제목 → 역할 ────────────────────────────
@pytest.mark.parametrize('title, role', [
    ('LRS', 'Lb'), ('RRS', 'Rb'), ('LTF', 'Tfl'), ('RTF', 'Tfr'), ('LTR', 'Tbl'), ('RTR', 'Tbr'),
    ('LS', 'Ls'), ('rs', 'Rs'), ('L', 'L'), ('C', 'C'), ('L 1', 'L'), ('Lrs', 'Lb'),
    ('LFE 1', 'SW1'), ('LFE 2', 'SW2'), ('SW3', 'SW3'), ('SUB', 'SW1'), ('Ltm', 'Tml'),
])
def test_role_from_title_knows_dolby_names(title, role) -> None:
    assert model.role_from_title(title) == role


@pytest.mark.parametrize('title', ['L EQ', 'L LF SUM', 'LRS EQ 예측', 'DBA L', 'INFRA', 'L + 전면흡음',
                                   'Aligned sum', 'LF 찐', ''])
def test_role_from_title_skips_non_raw_titles(title) -> None:
    assert model.role_from_title(title, ROLES_714) is None


def test_suggest_assignment_keeps_first_raw_measurement_only() -> None:
    titles = ['DBA L', 'L', 'L EQ', 'LRS', 'LRS EQ 예측', 'LRS', 'LTF', 'Tfl', 'INFRA']
    assert suggest_assignment(titles, ROLES_714) == [None, 'L', None, 'Lb', None, None, 'Tfl', None, None]


# ──────────────────────────── 실측 파일 ────────────────────────────
@pytest.fixture(scope='module')
def dolby_titles():
    from secs.mdat_loader import load_mdat_project

    return [str(m.title or '') for m in load_mdat_project(MDAT).measurements]


@needs_file
def test_real_titles_are_assigned_to_the_right_roles(dolby_titles) -> None:
    titles = dolby_titles
    suggested = suggest_assignment(titles, ROLES_714)
    picked = {titles[i]: role for i, role in enumerate(suggested) if role}
    assert picked == DOLBY

