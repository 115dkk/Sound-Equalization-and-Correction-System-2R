"""요구 대장 R5(2026-10-05): 1024×640 창에서 지금 흐름(`strings.FLOW`)의 화면 전부 — 특히 보정 설계(SPL·XO·BA 탭)와
내보내기 — 에 가로 스크롤·글자 잘림이 없고, 창이 정말 1024×640 이다. 한국어·영어.

고치기 전: 사이드바(칸 사이 고정 14 px)가 창 최소 높이를 707 px 로 끌어올려 1024×640 이 1024×707 로 열렸고, 보정 설계의
결과 카드 맨 위 줄(채널 + 탭 4개)이 본문 칸보다 넓어 가로 스크롤이 났다(한국어 25 px, 영어 118 px).
'동작 → 변화' 로 본다: 같은 화면이 넓은 창에서는 탭이 채널 옆 한 줄, 1024 영어에서는 아랫줄로 내려간다.

실제 글꼴 폭이 필요하다 — `QT_QPA_PLATFORM=windows` 로 실행(오프스크린이면 건너뜀). 점검은 `tools/qa_capture.py` 를
그대로 쓴다(test_qa_layout_c·d 와 같은 방식). 보정 계산은 합성 방 2.2(서브 2개)를 빠른 엔진으로 한 번.
"""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path

import pytest

from test_calibration_step import _compute, fast_engine  # noqa: F401
from test_calibration_sub_combine import fast_ba, two_subs  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

SIZE = (1024, 640)


def _capture_tool():
    # qa_capture 임포트가 환경변수를 지우므로 되돌린다.
    path = Path(__file__).resolve().parents[1] / 'tools' / 'qa_capture.py'
    spec = importlib.util.spec_from_file_location('qa_capture_r5', path)
    tool = importlib.util.module_from_spec(spec)
    platform = os.environ.get('QT_QPA_PLATFORM')
    try:
        spec.loader.exec_module(tool)
    finally:
        if platform is not None:
            os.environ['QT_QPA_PLATFORM'] = platform
    return tool


@pytest.fixture(scope='module')
def flow(qapp, fast_engine, fast_ba, two_subs, tmp_path_factory):  # noqa: F811
    """보정 계산을 마친 위저드 창(화면에 안 뜬다) + 캡처 도구."""
    if os.environ.get('QT_QPA_PLATFORM') == 'offscreen' or qapp.platformName() == 'offscreen':
        pytest.skip('실제 글꼴 폭 검사: QT_QPA_PLATFORM=windows 로 실행')
    import copy

    import secs.rewapi as rewapi
    from PyQt6.QtCore import Qt
    from secs.wizard import WizardWindow

    tool = _capture_tool()
    project, irs = two_subs
    tmp = tmp_path_factory.mktemp('r5')
    rew = rewapi.FakeREW(delay_s=0.0)
    win = WizardWindow(copy.deepcopy(project), rew, rew, rew, rew, project_path=tmp / 'r5.secs')
    win.ask_before_close = False
    win.auto_calibrate = False
    win.setAttribute(Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    win.show()
    page = win.calibration_page
    with pytest.MonkeyPatch.context() as mp:
        page.set_context(tmp / 'r5.secs', lambda: irs)
        # 화면 이동(go)이 IR 공급 함수를 창 것(실제 wav)으로 바꾸지 않게 — 계산은 위에서 한 번 끝낸다.
        mp.setattr(win, '_set_calibration_context', lambda: None)
        page.mode_buttons['zero'].click()
        _compute(page, qapp)
        try:
            yield win, page, tool, tmp
        finally:
            from secs.i18n import language_manager
            language_manager().set_language('ko', persist=False)
            win.close()
            qapp.processEvents()
            rew.close()


@pytest.mark.parametrize('lang', ['ko', 'en'])
def test_every_flow_screen_fits_1024x640(flow, qapp, lang):  # noqa: F811
    from secs.i18n import language_manager
    from secs.wizard.strings import CAL_STEP, FLOW

    win, page, tool, tmp = flow
    language_manager().set_language(lang, persist=False)
    qapp.processEvents()
    bad = []
    for step in FLOW:
        for tab in (('spl', 'xo', 'subs') if step == CAL_STEP else ('',)):
            if tab:
                page.set_tab(tab)
                assert page.tab == tab
            row = tool.capture(win, qapp, step, SIZE, tmp, f'r5_{lang}_{step}{tab}')
            if (win.width(), win.height()) != SIZE or row['hscroll'] or row['clipped']:
                bad.append((row['name'], (win.width(), win.height()), row['hscroll'], row['clipped']))
    page.set_tab('spl')
    assert not bad, bad


def test_result_tabs_wrap_only_when_the_column_is_narrow(flow, qapp):  # noqa: F811
    from secs.i18n import language_manager
    from secs.wizard.strings import CAL_STEP

    win, page, tool, tmp = flow
    language_manager().set_language('en', persist=False)
    qapp.processEvents()
    tool.capture(win, qapp, CAL_STEP, (1920, 1080), tmp, 'r5_wide')
    assert page.result_bar.one_line()                             # 넓으면 탭이 채널 옆(전과 같은 모양)
    tool.capture(win, qapp, CAL_STEP, SIZE, tmp, 'r5_narrow')
    bar = page.result_bar
    need = bar.left.sizeHint().width() + bar.row.spacing() + bar.right.sizeHint().width()
    # 1024 영어: 'Sub alignment' 탭일 때는 118 px 넘쳐 아랫줄이었다. 10-12 탭 이름이 'BA' 로 짧아져 한 줄에 들 수 있다 —
    # 폭이 모자랄 때만 내리는지는 줄을 일부러 좁혀 본다.
    assert bar.one_line() == (bar.width() >= need)
    bar.resize(need - 40, bar.height())
    bar.fit()                                       # 부모 레이아웃이 폭을 되돌리기 전에 판단만 본다
    assert not bar.one_line()
    bar.row.setGeometry(bar.rect())                 # 아랫줄 배치를 지금 폭으로 바로 그린다
    tabs = page.tab_buttons
    assert tabs['spl'].y() > page.channel_combo.y()
    assert tabs['spl'].x() < tabs['subs'].x()                     # 아랫줄은 왼쪽부터(늘어나 퍼지지 않는다)
    assert tabs['spl'].width() == tabs['spl'].sizeHint().width()
