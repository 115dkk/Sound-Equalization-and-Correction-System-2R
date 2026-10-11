"""풍선 도움말·숫자 칸 단위 (2026-09-24 사용자 요청).

- 모든 단계의 입력 위젯·버튼에 기능 설명 키가 달려 있고(`tooltips.resolve`), 보이는 표의 머리에도 있다.
- 모든 설명에 영어가 있고, 영어로 바꾸면 풍선 글에 한글이 남지 않는다(원래 툴팁은 첫 줄).
- 풍선은 폭 420 px 안에서 낱말 단위로 줄을 바꾸고, 필터가 ToolTip 이벤트에서 실제로 띄운다.
- 2단계 DSP 카드의 단위 접미사·이름(표시만 — 저장 값은 그대로).
"""
from __future__ import annotations

import re

import pytest
from PyQt6 import QtCore, QtGui, QtWidgets

from test_wizard_common import PRESET, engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)

HANGUL = re.compile('[가-힣]')

#: 순회하는 화면 번호(1~5 · 내보내기 8). 옛 6·7 은 2026-10-10 슬림화로 지웠다.
STEPS_WITHOUT_CALIBRATION = (1, 2, 3, 4, 5, 8)

#: 풍선 도움말이 없어도 되는 위젯 — Qt 가 표 구석에 만드는 '전체 선택' 단추뿐.
EXEMPT_OBJECT_NAMES = {'qt_tableview_cornerbutton'}


def _input_widgets(root):
    """사람이 누르거나 값을 넣는 위젯(안쪽 부품·스크롤바 제외)."""
    kinds = (QtWidgets.QAbstractButton, QtWidgets.QComboBox, QtWidgets.QAbstractSpinBox,
             QtWidgets.QAbstractSlider, QtWidgets.QLineEdit)
    for widget in root.findChildren(QtWidgets.QWidget):
        if not isinstance(widget, kinds) or isinstance(widget, QtWidgets.QScrollBar):
            continue
        if widget.objectName() in EXEMPT_OBJECT_NAMES:
            continue
        parent = widget.parentWidget()
        if isinstance(parent, (QtWidgets.QAbstractSpinBox, QtWidgets.QComboBox, QtWidgets.QScrollBar)):
            continue
        yield widget


def _describe(widget) -> str:
    text = widget.text() if hasattr(widget, 'text') and callable(widget.text) else ''
    return f'{type(widget).__name__}({text!r})'


@pytest.fixture(scope='module')
def measured_window(qapp, engine, tmp_path_factory):  # noqa: F811
    """2.1 바이앰프 · Fake 2 위치 측정까지 끝낸 창 하나를 이 파일의 테스트가 나눠 쓴다(보정 설계 화면은
    `test_calibration_step.test_every_calibration_control_has_a_tip_and_english` 가 따로 본다)."""
    import secs.model as model
    from secs.i18n import language_manager
    from secs.rewapi.fake import FakeREW

    language_manager().set_language('ko', persist=False)
    rew = FakeREW(delay_s=0.0)
    project = model.Project(name='풍선', config=model.SpeakerConfig.from_preset(PRESET, biamp=True))
    window = make_window(project, rew, tmp_path_factory.mktemp('tips') / 'tips.secs')
    window.go(5)
    for pos in (1, 2):
        assert window.step5.measure_position(pos)
        window.step5.wait(60000)
        qapp.processEvents()
    for step in STEPS_WITHOUT_CALIBRATION:
        window.go(step)
        qapp.processEvents()
        page = window.step_widget()
        if hasattr(page, 'wait'):
            page.wait(180000)
        qapp.processEvents()
    yield window
    language_manager().set_language('ko', persist=False)
    window.close()
    rew.close()


def test_every_tip_has_english_without_hangul(engine):
    from secs.i18n import translate
    from secs.wizard import strings  # noqa: F401 - 번역표 등록
    from secs.wizard.en_g5 import EN
    from secs.wizard.tooltips import TIPS

    assert set(EN) == set(TIPS)
    for key, korean in TIPS.items():
        assert HANGUL.search(korean), key
        english = translate(korean, 'en')
        assert english == EN[key], key
        assert not HANGUL.search(english), (key, english)


def test_every_input_widget_has_a_tip_on_every_step(measured_window, qapp):  # noqa: F811
    from secs.wizard import tooltips

    window = measured_window
    missing: list[str] = []
    counted = 0
    keys: set[str] = set()
    for step in STEPS_WITHOUT_CALIBRATION:
        window.go(step)
        qapp.processEvents()
        for widget in _input_widgets(window):
            owner, key = tooltips.resolve(widget)
            if not key:
                missing.append(f'{step}: {_describe(widget)}')
                continue
            counted += 1
            keys.add(key)
            assert tooltips.tip_text(widget).strip()
    assert not missing, missing
    assert counted > 100
    # 보이는 표의 머리에도 설명이 있다.
    for table in window.findChildren(QtWidgets.QTableView):
        header = table.horizontalHeader()
        for column in range(header.count() - (1 if table is window.config_page.table else 0)):
            assert tooltips.header_key(header, column), (table.objectName(), column)


def test_tips_follow_language_and_keep_own_tooltip_first(measured_window, qapp):  # noqa: F811
    from secs.i18n import language_manager
    from secs.wizard import tooltips
    from secs.wizard.en_g5 import EN

    window = measured_window
    window.go(2)
    qapp.processEvents()
    page = window.config_page
    # 원래 툴팁(프로파일 전체 이름)이 첫 줄, 설명은 아래.
    text = tooltips.tip_text(page.dsp_profile)
    first, _, rest = text.partition('\n')
    assert first == page.dsp_profile.toolTip() and first
    assert rest == tooltips.TIPS['s2.dsp_profile']

    language_manager().set_language('en', persist=False)
    qapp.processEvents()
    for step in STEPS_WITHOUT_CALIBRATION:
        window.go(step)
        qapp.processEvents()
        for widget in _input_widgets(window):
            owner, key = tooltips.resolve(widget)
            text = tooltips.tip_text(widget)
            assert EN[key] in text
            assert not HANGUL.search(text), (step, _describe(widget), text)
    language_manager().set_language('ko', persist=False)


def test_tooltip_html_wraps_by_word_within_width(qapp):  # noqa: F811
    from secs.wizard import tooltips

    body = tooltips.TIPS['s2.bm_delays']
    metrics = QtGui.QFontMetrics(QtWidgets.QToolTip.font())
    lines = tooltips._wrap(body, metrics, tooltips.MAX_WIDTH_PX)
    assert len(lines) > 1
    words = body.split(' ')
    rebuilt = ' '.join(line.strip() for line in lines).split(' ')
    assert rebuilt == words                                    # 낱말 안에서 끊지 않았다
    for line in lines:
        assert metrics.horizontalAdvance(line) <= tooltips.MAX_WIDTH_PX or ' ' not in line.strip()
    html = tooltips.to_html('Biamp Tesira', body)
    assert html.count('<br>') >= len(lines) and '<b>Biamp Tesira</b>' in html


def test_filter_shows_help_after_hover_delay(measured_window, qapp, monkeypatch):  # noqa: F811
    from secs.i18n import language_manager
    from secs.wizard import tooltips

    window = measured_window
    window.show()
    window.go(2)
    qapp.processEvents()
    assert tooltips.HOVER_DELAY_MS >= 700
    spin = window.config_page.dsp_budget
    inner = spin.findChild(QtWidgets.QLineEdit)           # 커서는 스핀박스 안쪽 편집칸 위에 있다
    local = inner.rect().center()
    # 실제 커서는 옮기지 않는다(사용자 마우스) — 추가 대기(HOVER_DELAY_MS - Qt 대기)를 0 으로 둬
    # 커서 위치 확인 없이 바로 띄우게 한다. 대기 자체는 `_show_pending` 이 맡는다.
    monkeypatch.setattr(tooltips, '_wake_delay', lambda: tooltips.HOVER_DELAY_MS)
    language_manager().set_language('ko', persist=False)
    event = QtGui.QHelpEvent(QtCore.QEvent.Type.ToolTip, local, inner.mapToGlobal(local))
    handled = qapp._secs_tip_filter.eventFilter(inner, event)
    assert handled
    deadline = QtCore.QDeadlineTimer(2000)
    while not QtWidgets.QToolTip.isVisible() and not deadline.hasExpired():
        qapp.processEvents()
    assert QtWidgets.QToolTip.isVisible()
    assert '예산' in QtWidgets.QToolTip.text()
    QtWidgets.QToolTip.hideText()
    window.hide()


def test_step2_units_are_display_only(measured_window, qapp):  # noqa: F811
    from secs.i18n import language_manager

    window = measured_window
    language_manager().set_language('ko', persist=False)
    window.go(2)
    qapp.processEvents()
    page = window.config_page
    dsp = window.project.dsp
    assert page.dsp_budget.text() == f'{dsp.fir_budget:,} 탭'
    assert page.dsp_iir.text() == f'{dsp.iir_bands} 밴드'
    assert page.dsp_allpass.text() == f'{dsp.allpass_bands} 밴드'
    assert page.dsp_bm_delays.text() == f'{dsp.bm_path_delays} 곳'
    index = page.dsp_taps.findData(2048)
    assert page.dsp_taps.itemText(index) == '2,048 탭 (선형 위상 최대 21.3 ms)'
    assert page.dsp_note.text() == (f'창 길이 ≈ {dsp.fir_taps / 48:.0f} ms · '
                                    f'분해능 ≈ {48000 / dsp.fir_taps:.1f} Hz')
    # 저장 값은 숫자 그대로 — 접미사를 넣은 칸에서 값을 바꿔도 정수로 들어간다.
    page.dsp_budget.setValue(40960)
    assert window.project.dsp.fir_budget == 40960
    page.dsp_iir.setValue(12)
    assert window.project.dsp.iir_bands == 12

    language_manager().set_language('en', persist=False)
    qapp.processEvents()
    page = window.config_page
    assert page.dsp_budget.text() == '40,960 taps'
    assert page.dsp_iir.text() == '12 bands'
    assert page.dsp_taps.displayText().endswith(' taps (linear phase max 21.3 ms)') or 'taps (linear phase max' in \
        page.dsp_taps.displayText()
    assert page.dsp_note.text().startswith('window ≈ ')
