# -*- coding: utf-8 -*-
"""1점 측정(2026-09-24 사용자 결정: "샘플 데이터는 1점 측정만 했으니까 1점만 넣어야지").

- 4단계 배치에 '단일 · 청취 위치 1점'(1 위치)이 있고 카드 4장이 같은 크기로 한 줄에 선다.
- 샘플 모드(`--fake-rew`)로 새로 열면 배치 = 1점, [데모 데이터] 는 위치 1 만. 구성은 7.1.4 + 서브 3(10-11 사용자 결정 —
  데모 방의 실측 서브 3개).
(옛 7단계 최적화 목표·5→8′ 흐름 검사는 2026-10-10 슬림화로 그 엔진·화면과 함께 지웠다.)
"""
from __future__ import annotations

import pytest

from test_wizard_common import engine, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


# ──────────────────────────── 새 프로젝트 · 배치 ────────────────────────────
def test_sample_mode_new_project_is_single_point(engine):
    from secs.wizard import SAMPLE_PRESET, SAMPLE_SUBS, new_project

    sample = new_project(fake=True)
    assert sample.config.preset == SAMPLE_PRESET            # 이름은 7.1.4 그대로 — [빈 구성] 이 켜지지 않게
    assert sample.config.sub_count() == SAMPLE_SUBS == 3
    # 서브는 SW1 옆에 모으고 출력은 표 순서대로 1~14(10-11 사용자: SW2 · SW3 이 13 · 14 로 끝에 붙어 "순서가 이상")
    assert [s.role for s in sample.config.speakers][:6] == ['L', 'R', 'C', 'SW1', 'SW2', 'SW3']
    assert [s.bands[0].out for s in sample.config.speakers] == list(range(1, 15))
    assert sample.config.validate() == []                    # 출력 1~14 겹침 없음
    assert sample.extra['array'] == 1
    assert 'array' not in new_project(fake=False).extra          # 일반 모드는 전처럼(기본 9)


def test_step4_has_four_equal_cards_in_one_row_at_1024(qapp, engine, tmp_path):
    import secs.model as model
    from secs.rewapi.fake import FakeREW

    project = model.Project(config=model.SpeakerConfig.from_preset('7.1.4'))
    rew = FakeREW(delay_s=0.0)
    win = make_window(project, rew, tmp_path / 'a.secs')
    try:
        win.resize(1024, 640)
        win.show()
        win.go(4)
        qapp.processEvents()
        cards = [win.step4._picks[n] for n in (1, 9, 13, 17)]
        assert list(win.step4._picks) == [1, 9, 13, 17]
        tops = {card.geometry().top() for card in cards}
        widths = {card.width() for card in cards}
        assert len(tops) == 1, tops                           # 한 줄
        assert max(widths) - min(widths) <= 1, widths         # 같은 크기
        assert min(widths) >= 86
        win.step4.set_count(1)
        assert project.extra['array'] == 1
        assert len(win.step4.points()) == 1
    finally:
        win.close()
        qapp.processEvents()
        rew.close()


def test_sample_fake_refuses_measuring_position_2(qapp, engine, tmp_path):
    """샘플 FakeREW 로 9 위치 배치의 위치 2 를 재면 실측 없음으로 실패하고 기록은 안 남는다."""
    from secs.rewapi.fake import FakeREW
    from secs.wizard import new_project

    project = new_project(fake=True)
    project.extra['array'] = 9
    rew = FakeREW(delay_s=0.0, use_sample_dolby=True)
    win = make_window(project, rew, tmp_path / 'b.secs')
    try:
        win.go(5)
        assert win.step5.measure_position(1) is True
        assert win.step5.wait(60000)
        assert win.step5.measure_position(2) is True
        assert win.step5.wait(60000)
        qapp.processEvents()
        assert win.step5.done_positions() == [1]
        assert '샘플은 실측 1점뿐' in win.step5.log.toPlainText()
    finally:
        win.close()
        qapp.processEvents()
        rew.close()
