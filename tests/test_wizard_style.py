"""스타일·배치도가 목업과 같은 값을 쓰는지 (트랙 C).

기대값 출처는 `UI 목업/secs-ui-mockup.html`:
- `:root` 디자인 토큰      (10~44행)
- `chColor()`              (582행)
- `ARR`                    (316행)
- `arrPoints()`            (520~526행)
- `roomSVG()` 의 pos       (475행)
"""
from __future__ import annotations

import pytest

from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def test_tokens_match_mockup(engine):
    from secs.wizard.style import CHANNEL_COLORS, COLORS

    assert COLORS["chassis"] == "#1e3736"
    assert COLORS["chassis2"] == "#234d4a"
    assert COLORS["ground"] == "#e6eeed"
    assert COLORS["panel"] == "#f2f6f5"
    assert COLORS["paper"] == "#d5e0df"
    assert COLORS["accent"] == "#1e8c86"
    assert COLORS["marker"] == "#30e3da"
    assert (COLORS["ok"], COLORS["warn"], COLORS["bad"]) == ("#1f8a3f", "#a8561f", "#a63a25")
    # 10-10 3차 시험: 8색이라 12채널에서 같은 색이 둘씩 — 16색(첫 색은 목업 그대로)
    assert CHANNEL_COLORS[0] == "#c23f9b" and len(CHANNEL_COLORS) == 16 and len(set(CHANNEL_COLORS)) == 16


def test_qss_uses_the_tokens(engine):
    from secs.wizard.style import COLORS, QSS

    assert "@chassis" not in QSS, "치환되지 않은 토큰이 남았다"
    for key in ("chassis", "accent", "ground", "panel", "line"):
        assert COLORS[key] in QSS


def test_arrangements_and_points_match_mockup(engine):
    from secs.wizard.layout_map import ARRANGEMENTS, arr_points

    assert sorted(ARRANGEMENTS) == [1, 9, 13, 17]
    assert [p.text for p in arr_points(1)] == ["청취 위치 · 귀 높이 (레퍼런스)"]   # 1점 = 청취 위치 하나(09-24)
    assert ARRANGEMENTS[13].name == "커플 · 2~3인 소파"
    for count in (9, 13, 17):
        points = arr_points(count)
        assert len(points) == count
        assert (points[0].x, points[0].y, points[0].h) == (0.0, 0.0, 0)
        assert points[0].text.startswith("청취 위치")
    nine = arr_points(9)
    assert (nine[1].x, nine[1].y) == (35.0, 0.0)
    assert nine[5].h == 1 and nine[8].h == -1
    thirteen = arr_points(13)
    assert (thirteen[9].x, thirteen[9].y) == (70.0, 0.0)


def test_room_positions_cover_every_role(engine):
    from secs.model import ROLES
    from secs.wizard.layout_map import ROOM_POSITIONS

    assert set(ROOM_POSITIONS) == set(ROLES)
    assert ROOM_POSITIONS["L"] == (.27, .18)
    assert ROOM_POSITIONS["SW1"] == (.16, .4)


def test_layout_views_draw(qapp, engine):
    from secs.model import SpeakerConfig
    from secs.wizard.layout_map import ArrayLayoutView, RoomLayoutView

    room = RoomLayoutView()
    room.set_speakers(SpeakerConfig.from_preset("9.1.6").speakers, selected=0)
    assert room.scene().items()

    array = ArrayLayoutView()
    array.set_state(17, [0, 1], 2)
    assert array.scene().items()


def test_step_labels_match_mockup(engine):
    from secs.wizard.strings import STEPS

    # 09-23 사용자 지시: REW 연결(장치)이 먼저, 구성이 둘째.
    # 09-26: 6 필터 설계 · 7 저역 통합 → '보정 설계' 하나(요구 대장 최상위 결정).
    # 10-10 사용자 결정 5A: 3단계 '레벨 체크'.
    assert STEPS == ("장치·마이크", "구성", "레벨 체크", "배치", "측정", "보정 설계", "내보내기")
