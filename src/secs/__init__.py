# -*- coding: utf-8 -*-
"""SECS 2 — Sound Equalization & Correction System(측정 위저드 + 보정 설계).

모듈끼리는 `secs.` 절대 임포트로 부른다. 제품 이름·버전은 `secs.version`, 진입점은 `secs.__main__`
(루트 `run_secs.py`). 모듈 지도는 저장소의 `구조 안내.md`.

2026-10-10 슬림화: 1.41 원본(secs4.py → `문서/지난 자료/secs4.py` 보존)에서 푼 옛 화면·옛 엔진 모듈과 그 공개 이름
지연 표(`secs.generate_filters_from_arrays` 같은 접근), `RELEASE_VERSION` 을 지웠다(되살릴 기준: git 태그
slim-before-2026-10-10).
"""
from __future__ import annotations

#: `--check` 가 임포트해 보는 모듈 — 위저드가 실제로 쓰는 계산·REW·mdat·화면 모듈(얼린 exe 에 빠진 것이 없나).
#: 단계 화면은 `wizard.window` 가 모두 임포트한다.
SOURCE_MODULES = (
    'version',
    'model',
    'i18n',
    'translations',
    'proc',
    'calibration',
    'correction_engine',
    'mimo',
    'mimo_iir',
    'scorecard',
    'export_camilladsp',
    'export_filters',
    'mdat_loader',
    'rew_mdat',
    'rew_mdat.builder',
    'rew_mdat.serialization.writer',
    'rewapi',
    'rewapi.interfaces',
    'rewapi.rew',
    'rewapi.callback',
    'rewapi.fake',
    'rewapi.session',
    'wizard',
    'wizard.window',
)
