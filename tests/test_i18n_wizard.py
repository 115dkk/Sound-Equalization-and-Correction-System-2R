"""위저드 영어 화면 · 실시간 언어 전환 · 키보드 조작 (P6, 트랙 C·E)."""
from __future__ import annotations

import re

import pytest

from test_wizard_common import (PRESET, engine, fake, make_window,  # noqa: F401
                                project, qapp)
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)

HANGUL = re.compile(r'[가-힣]')


@pytest.fixture()
def window(qapp, project, fake, tmp_path):
    from secs.i18n import language_manager

    win = make_window(project, fake, tmp_path / "w.secs")
    yield win
    language_manager().set_language('ko', persist=False)
    win.close()
    qapp.processEvents()


def _visible_texts(root):
    from PyQt6 import QtWidgets

    texts = []
    for widget in root.findChildren(QtWidgets.QWidget):
        if isinstance(widget, (QtWidgets.QLabel, QtWidgets.QAbstractButton)):
            texts.append(widget.text())
            texts.append(widget.toolTip())
        elif isinstance(widget, QtWidgets.QTableWidget):
            for col in range(widget.columnCount()):
                head = widget.horizontalHeaderItem(col)
                texts.append(head.text() if head else '')
    return [text for text in texts if text]


# ──────────────────────────── 번역표 ────────────────────────────
def test_every_pair_keeps_its_placeholders(engine):
    from secs.wizard import en_g1, en_g2, en_g4, en_g8, strings

    # en_g3·en_g7 은 2026-10-10 슬림화로 지웠다(지금 화면도 쓰는 쌍은 en_g2 끝으로 옮김).
    pairs = (list(en_g1.PAIRS) + list(en_g2.PAIRS) + list(en_g4.PAIRS) + list(en_g8.PAIRS)
             + list(strings._P6_PAIRS))
    assert len(pairs) > 400
    for korean, english in pairs:
        assert english.strip(), korean
        assert set(re.findall(r'\{\d+\}', korean)) == set(re.findall(r'\{\d+\}', english)), korean
        assert not HANGUL.search(english), english


def test_wizard_words_do_not_override_the_legacy_gui(engine, qapp):
    """'Export' 는 기존 GUI 에서 '내보내기' 다 — 위저드의 '내보내기·검증' 이 덮으면 안 된다."""
    import secs.wizard.strings  # noqa: F401
    from secs.i18n import translate

    assert translate('Export', 'ko') == '내보내기'
    assert translate('내보내기·검증', 'en') == 'Export'
    # 10-10 3차 시험: 옛 1.41 화면의 'Normal'(일반)은 슬림화로 없어졌고, 극성 '정상' 의 영어가 'Normal' 이다
    assert translate('Normal', 'ko') == '정상' and translate('정상', 'en') == 'Normal'


def test_patterns_and_embedded_values_are_translated(engine, qapp):
    import secs.wizard.strings  # noqa: F401
    from secs.i18n import translate

    assert translate('측정 실패: REW 가 연결되지 않았습니다', 'en').startswith('Measurement failed: ')
    assert translate('1 · 청취 위치 · 완료', 'en').endswith('Done')
    assert translate('측정 실패: 무엇', 'ko') == '측정 실패: 무엇'   # 한국어 화면은 원문 그대로


# ──────────────────────────── 화면 ────────────────────────────
def test_english_screen_has_no_korean_left(window, qapp):
    from secs.i18n import language_manager

    language_manager().set_language('en', persist=False)
    left = []
    for step in (1, 2, 3, 4, 5, 8, 9):              # 9 = 보정 설계(09-26), 옛 7 은 10-10 에 지움
        window.go(step)
        qapp.processEvents()
        page = window.stack.currentWidget()
        left += [f'{step}: {text[:40]}' for text in _visible_texts(page) if HANGUL.search(text)]
    left += [text for text in _visible_texts(window.steps_bar) if HANGUL.search(text)]
    assert not left, left


def test_language_switches_live_and_back(window, qapp):
    from secs.i18n import language_manager

    manager = language_manager()
    assert window.steps_bar.buttons[-1].text() == '내보내기'
    manager.set_language('en', persist=False)
    assert window.steps_bar.buttons[-1].text() == 'Export'
    assert window.steps_bar.buttons[5].text() == 'Room correction'      # 10-10 3차 시험 결정 2A
    assert window.next_button.text() == 'Next →'
    assert 'Measurement Wizard' in window.windowTitle()
    window.go(8)
    assert window.next_button.text() == 'Finish'
    manager.set_language('ko', persist=False)
    assert window.steps_bar.buttons[-1].text() == '내보내기'
    assert window.next_button.text() == '완료'
    assert '측정 위저드' in window.windowTitle()


def test_combo_keeps_korean_source_for_the_project(window):
    """콤보는 화면만 영어다 — `currentText()` 는 원문이라 `.secs` 에 영어가 섞이지 않는다."""
    from PyQt6 import QtWidgets
    from secs.i18n import language_manager

    language_manager().set_language('en', persist=False)
    window.go(4)
    combos = window.step4.findChildren(QtWidgets.QComboBox)
    assert combos
    for combo in combos:
        for index in range(combo.count()):
            shown = QtWidgets.QComboBox.itemText(combo, index)
            assert not HANGUL.search(shown), shown
    window.step4.refresh()
    assert not re.search(r'[A-Za-z]{4}', str(window.project.extra.get('start_delay', '')))


# ──────────────────────────── 키보드 ────────────────────────────
def test_keyboard_shortcuts_move_between_steps(window):
    from PyQt6 import QtGui

    shortcuts = {s.key().toString(): s for s in window.findChildren(QtGui.QShortcut)}
    assert {'Alt+Left', 'Alt+Right', 'Ctrl+S', 'Ctrl+O', 'Ctrl+1', 'Ctrl+7'} <= set(shortcuts)
    assert 'Ctrl+8' not in shortcuts                 # 09-26: 진행바 7칸
    shortcuts['Ctrl+6'].activated.emit()
    assert window.current_step == 9                  # 진행바 6번째 = 보정 설계(화면 번호 9)
    shortcuts['Alt+Left'].activated.emit()
    assert window.current_step == 5
    shortcuts['Ctrl+4'].activated.emit()
    assert window.current_step == 4
    shortcuts['Alt+Left'].activated.emit()
    assert window.current_step == 3
    shortcuts['Ctrl+1'].activated.emit()
    shortcuts['Alt+Left'].activated.emit()
    assert window.current_step == 1

