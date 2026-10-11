# -*- coding: utf-8 -*-
"""위저드 문구 — 화면 코드는 한국어 원문(목업 문구 그대로)만 쓰고, 영어는 여기서 등록한다.

`t()` 는 `secs.i18n.translate` 를 감싼 것이다. 카탈로그에 없는 문자열은 그대로
돌려준다. 단계 제목·버튼은 `_PAIRS`, 나머지 화면 문구는 묶음별 표(`en_g1`·`en_g2`, 도움말 `en_g4`,
풍선 도움말·단위 `en_g5`, 도움말 튜토리얼 `en_g6`)와 `_P6_PAIRS` 에 있다. 영어 키는 덮지 않는다(`add_korean`).
"""
from __future__ import annotations

from secs.i18n import Translated, translate

#: 상단 진행바(사용자 흐름). 2026-09-26: 옛 6 필터 설계 · 7 저역 통합을 '보정 설계' 하나로(요구 대장 최상위 결정 —
#: 새 엔진 `secs.correction_engine`). 옛 두 화면은 2026-10-10 슬림화로 지웠다.
STEPS: tuple[str, ...] = ('장치·마이크', '구성', '레벨 체크', '배치', '측정', '보정 설계', '내보내기')

#: 보정 설계 화면의 화면 번호(창 `go()` 번호). 1~8 은 예전 번호 그대로 두고(테스트·도구가 번호로 부른다) 뒤에 붙였다.
#: 6·7(옛 필터 설계·저역 통합)과 10(옛 내보내기·검증)은 2026-10-10 에 지운 화면의 빈 번호다.
CAL_STEP = 9
#: 진행바 순서 → 화면 번호. 진행바 6번째 = 보정 설계(9), 7번째 = 내보내기(8, 보정 전·후 REW mdat).
FLOW: tuple[int, ...] = (1, 2, 3, 4, 5, CAL_STEP, 8)
#: 화면 번호 → 이름(도움말 창 제목 등).
PAGE_TITLES: dict[int, str] = {1: '장치·마이크', 2: '구성', 3: '레벨 체크', 4: '배치', 5: '측정',
                               8: '내보내기', CAL_STEP: '보정 설계'}


def bar_position(page: int) -> int:
    """화면 번호 → 진행바 위치(1 기점)."""
    return FLOW.index(int(page)) + 1

_PAIRS: tuple[tuple[str, str], ...] = (
    # 상단 진행바
    ('Setup', '구성'),
    ('Devices', '장치·마이크'),
    ('Levels', '레벨'),
    ('Positions', '배치'),
    ('Measure', '측정'),
    ('Filters', '필터 설계'),
    ('Bass', '저역 통합'),
    ('Export', '내보내기·검증'),
    ('Room correction', '보정 설계'),     # 10-10 3차 시험 결정 2A — 마이크 캘리브레이션과 가르려고 'Calibration' 에서 바꿈
    # 단계 제목
    ('2 · System Configuration', '2 · 시스템 구성'),
    ('1 · Devices · Microphone', '1 · 장치 · 마이크'),
    ('3 · Level Calibration', '3 · 레벨 캘리브레이션'),
    ('4 · Measurement Positions', '4 · 측정 위치 배치'),
    # 10-10: 화면 제목 = 진행바 이름
    ('1 · Devices', '1 · 장치·마이크'),
    ('2 · Setup', '2 · 구성'),
    ('3 · Levels', '3 · 레벨'),
    # 10-10 사용자 결정(5A): 하는 일은 마이크 입력 점검 — 업계의 '레벨 캘리브레이션'(채널 SPL 맞춤)과 뜻이 달랐다
    ('Level check', '레벨 체크'),
    ('3 · Level check', '3 · 레벨 체크'),
    ('4 · Positions', '4 · 배치'),
    ('5 · Measurement', '5 · 측정'),
    # 내비게이션
    ('← Back', '← 이전'),
    ('Next →', '다음 →'),
    ('Done', '완료'),
    ('Skip', '건너뛰기'),
    ('Measurement Wizard', '측정 위저드'),
    # 영어 제목 · 버튼은 문장형 대소문자(10-10 3차 시험: 'Output Patch Sheet' · 'Position List' 처럼 섞여 있었다)
    ('Save project', '프로젝트 저장'),
    ('Open project', '프로젝트 불러오기'),
    # 카드 제목
    ('Speaker Layout', '배치도'),
    ('Output patch sheet', '출력 패치 시트'),
    ('DSP Profile · Filter Resources', 'DSP 프로파일 · 필터 자원'),
    ('REW Connection', 'REW 연결'),
    ('Microphone · Audio', '마이크 · 오디오'),
    ('Channel Levels', '채널별 레벨'),
    ('Master Output', '마스터 출력'),
    ('Microphone Gain', '마이크 게인'),
    ('Arrangement Choice', '배치 선택'),
    ('Measurement Options', '측정 옵션'),
    ('Position list', '위치 목록'),
    ('View', '보기'),
    # 버튼
    ('+ Add Speaker', '+ 스피커 추가'),
    ('Auto-number Outputs', '출력 번호 자동 정렬'),
    ('Patch sheet CSV', '패치 시트 CSV'),
    ('Launch REW in API mode', 'REW API 모드로 실행'),
    ('Read Settings', '설정 읽기'),
    ('Retry', '다시 확인'),
    ('Play all in sequence', '전 채널 자동 순차 재생'),
    ('Measure This Position', '이 위치 측정'),
    ('Cancel', '측정 취소'),
    ('Redo Last', '마지막 위치 다시'),
    ('All positions done', '모든 위치 완료'),
    ('Load mdat', 'mdat 불러오기'),
    ('Demo Data', '데모 데이터'),
    # 그래프 탭 · 버튼
    ('Magnitude', '주파수 응답'),
    ('Impulse', '임펄스 응답'),
    ('Cancel Calculation', '계산 취소'),
    ('Save', '저장'),
    ('Load', '불러오기'),
    ('Save target curve', '타깃 커브 저장'),
    ('Open target curve', '타깃 커브 불러오기'),
    # 상태
    ('Not connected', '미연결'),
    ('Not saved', '저장 안 됨'),
    ('Please start REW.', 'REW 를 켜 주세요.'),
)

from secs.translations import add_korean as _add_korean  # noqa: E402
from secs.wizard import en_g1, en_g2, en_g4, en_g5, en_g6, en_g8, en_g9  # noqa: E402

# 영어 키는 덮지 않는다 — 'Export'·'Save' 처럼 기존 GUI 가 다른 한국어로 쓰는 낱말이 있다.
for _en, _ko in _PAIRS:
    _add_korean(_ko, _en)

# 나머지 화면 문구(P6): 한국어 원문 → 영어. 표가 커서 단계 묶음별 모듈로 나눴다.

#: P6 에서 새로 생긴 문구(단축키 안내 등) — 묶음 표에 없는 것.
_P6_PAIRS: tuple[tuple[str, str], ...] = (
    ('이전 단계 (Alt+←)', 'Previous step (Alt+←)'),
    ('다음 단계 (Alt+→) · 단계 바로 가기 Ctrl+1~7', 'Next step (Alt+→) · jump to a step with Ctrl+1–7'),
    ('프로젝트 저장 (Ctrl+S)', 'Save project (Ctrl+S)'),
    ('프로젝트 불러오기 (Ctrl+O)', 'Open project (Ctrl+O)'),
    (' 개', ' ea'),
    # 극성 '정상' 은 en_g1·en_g2(옛 en_g3 몫 포함)가 등록한다 — 모두 'Normal'(10-10). 여기서 다시 덮어쓰지 않는다.
    ('1채널', '1 ch'),
    ('극성 반전', 'Invert polarity'),
)

for _module in (en_g1, en_g2, en_g4, en_g5, en_g8, en_g9):
    for _ko, _en in _module.PAIRS:
        _add_korean(_ko, _en)
for _ko, _en in _P6_PAIRS:
    _add_korean(_ko, _en)
# 도움말 튜토리얼(en_g6)은 그림 캡션에 화면 이름(카드 제목 등)을 그대로 쓴다 — 이미 등록된 화면 쪽
# 번역을 덮으면 카드 제목의 영어가 바뀌므로 없는 것만 더한다(2026-09-24).
from secs.translations import CATALOG as _CATALOG  # noqa: E402
for _ko, _en in en_g6.PAIRS:
    if _ko not in _CATALOG:
        _add_korean(_ko, _en)


def t(text: str) -> str:
    """현재 언어로 번역한 문구(등록되지 않았으면 원문 그대로).

    원문을 함께 들고 다니는 `Translated` 를 돌려준다 — 번역 위젯에 그대로 넘기면
    언어를 바꿀 때 원문에서 다시 옮긴다.
    """
    return Translated(translate(text), text)


__all__ = ['STEPS', 'CAL_STEP', 'FLOW', 'PAGE_TITLES', 'bar_position', 't']
