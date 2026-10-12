"""BA 경고 · 인프라 고르기(2026-10-12 사용자: 돌비룸에서 [서브만 계산] 이 "미친듯이 잘못됐다").

원인: 묶음(레이아웃, 필터 공유) SW1 · SW2 에 LP 25 Hz 가 남아 HP 25 + LP 25 = 통과 대역이 거의 없었고, BA 가 막힌 대역을 게인으로
채워 SW1 +51 dB. LP 는 인프라 고르기에서 묶음 서브를 골랐다가 SW3 로 되돌리면 남았다(묶음 공유가 LP · HP 를 서로 옮겨 적음).
'동작 → 변화' 로 본다.
"""
from __future__ import annotations

import copy
from types import SimpleNamespace

from test_calibration_step import _page, fast_engine  # noqa: F401
from test_calibration_sub_combine import fast_ba, two_subs  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

SUBS = ('SW1', 'SW2', 'SW3')


def _three_sub_page(two_subs, tmp_path):  # noqa: F811
    project, irs = two_subs
    p = copy.deepcopy(project)
    p.config.set_sub_count(3)
    page = _page(p, irs, tmp_path)
    s = page.settings
    s.set_sub('SW1', mode='bundle', bundle='A')
    s.set_sub('SW2', mode='bundle', bundle='A')
    s.set_layout('A', kind='multi')
    page._changed_setting()                              # 프로젝트에 적어 둔다(새로 고침이 거기서 다시 읽는다)
    return page


def _bands(page):
    return {r: (page.settings.sub(r)['hp_hz'], page.settings.sub(r)['lp_hz']) for r in SUBS}


def test_infra_on_a_bundle_member_then_back_leaves_no_lp(qapp, two_subs, tmp_path):  # noqa: F811
    from secs import calibration as cal

    page = _three_sub_page(two_subs, tmp_path)
    try:
        page.set_infra('SW3', 25.0, 'LR48')
        assert _bands(page) == {'SW1': (25.0, 0.0), 'SW2': (25.0, 0.0), 'SW3': (0.0, 25.0)}
        page.set_infra('SW1', 25.0, 'LR48')                  # 묶음 서브를 잘못 고름 → 묶음째 인프라(서로 옮겨 적지 않음)
        assert _bands(page) == {'SW1': (0.0, 25.0), 'SW2': (0.0, 25.0), 'SW3': (25.0, 0.0)}
        assert page.infra_state()[0] == 'SW1'
        page.set_infra('SW3', 25.0, 'LR48')                  # 되돌리면 묶음에 LP 가 남지 않는다(전: HP 25 + LP 25)
        assert _bands(page) == {'SW1': (25.0, 0.0), 'SW2': (25.0, 0.0), 'SW3': (0.0, 25.0)}
        assert not cal.sub_filter_warnings(page.settings, SUBS)
    finally:
        page.deleteLater()


def test_saved_broken_bundle_warns_and_reselecting_infra_clears_it(qapp, two_subs, tmp_path):  # noqa: F811
    """10-12 돌비룸 저장본 모양(SW1 · SW2 HP 25 + LP 25 LR48, SW3 LP 25) — BA 카드에 빨간 경고, 인프라를 다시 고르면 풀린다."""
    from secs import calibration as cal

    page = _three_sub_page(two_subs, tmp_path)
    try:
        s = page.settings
        s.set_sub('SW1', hp_hz=25.0, hp_type='LR48', lp_hz=25.0, lp_type='LR48')     # 묶음 공유로 SW2 도 같게
        s.set_sub('SW3', lp_hz=25.0, lp_type='LR48')
        page._changed_setting()
        warns = cal.sub_filter_warnings(s, SUBS)
        assert [w.split(':')[0] for w in warns] == ['SW1', 'SW2'] and '통과 대역' in warns[0]
        page.set_tab('subs')
        page.refresh()
        assert page.sub_warn_label.isVisibleTo(page) and 'SW1' in page.sub_warn_label.text()
        assert page.infra_state()[0] == 'SW3'
        page.set_infra('SW3', 25.0, 'LR48')
        page.refresh()
        assert not page.sub_warn_label.isVisibleTo(page)
        assert _bands(page)['SW1'] == (25.0, 0.0)
    finally:
        page.deleteLater()


def test_gain_warning_over_limit():
    from secs import calibration as cal

    run = SimpleNamespace(result=SimpleNamespace(subs={'SW1': SimpleNamespace(gain_db=51.1),
                                                       'SW2': SimpleNamespace(gain_db=cal.SUB_GAIN_WARN_DB),
                                                       'SW3': SimpleNamespace(gain_db=-9.0)}))
    warns = cal.sub_gain_warnings(run)
    assert len(warns) == 1 and warns[0].startswith('SW1 게인 +51.1 dB')


def test_warnings_are_english(qapp, two_subs, tmp_path):  # noqa: F811
    import re

    from secs.i18n import language_manager
    from secs.wizard.strings import t

    language_manager().set_language('en', persist=False)
    try:
        assert t('SW1: HP 25 Hz, LP 25 Hz — 통과 대역이 거의 없습니다') == 'SW1: HP 25 Hz, LP 25 Hz — almost no passband'
        text = t('SW1 게인 +51.1 dB — 너무 큽니다. 서브 필터(HP, LP) 설정을 확인하세요')
        assert not re.search('[가-힣]', text), text
    finally:
        language_manager().set_language('ko', persist=False)
