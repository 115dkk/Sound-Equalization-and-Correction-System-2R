"""도움말 창(F1) — 2026-09-24 부터 단계별 **따라 하기 튜토리얼**(그림 포함)을 그린다.

- 창을 열면 현재 단계의 따라 하기가 보이고, 단계를 옮기면 내용이 따라간다.
- 화면에 설명 문구를 깔지 않는다(09-22 결정 — 기능 설명은 풍선 도움말 `tooltips`).
- 가독성 규칙(09-24 재작업)은 그대로: 글 블록은 px 고정 줄 간격, 한글 낱말 중간에서 줄이 끊기지 않음,
  단계를 옮겨도 탭 버튼이 남아 겹치지 않음.
- 새로: 그림 폭이 창을 넘지 않음(가로 스크롤 없음) · 창 크기가 바뀌면 다시 맞춤 · 그림 누르면 큰 보기.

튜토리얼 문안·그림 키·영어 쌍 검사는 `tests/test_tutorial_text.py`.
"""
from __future__ import annotations

from PyQt6 import QtCore, QtGui, QtWidgets

from test_wizard_common import PRESET, engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def _hangul_char(ch: str) -> bool:
    return '가' <= ch <= '힣'


def _midword_hangul_breaks(document) -> list[tuple[str, int]]:
    """블록마다 줄 끝(공백이 아닌 자리)이 한글 음절 사이인 경우를 모은다."""
    violations: list[tuple[str, int]] = []
    block = document.begin()
    while block.isValid():
        layout = block.layout()
        text = block.text()
        if layout is not None:
            for i in range(layout.lineCount() - 1):
                line = layout.lineAt(i)
                end = line.textStart() + line.textLength()
                if 0 < end < len(text) and _hangul_char(text[end - 1]) and _hangul_char(text[end]):
                    violations.append((text, end))
        block = block.next()
    return violations


def _has_image(block) -> bool:
    it = block.begin()
    while not it.atEnd():
        if it.fragment().charFormat().isImageFormat():
            return True
        it += 1
    return False


def test_help_button_opens_the_dialog_for_the_current_step(qapp, project, fake, tmp_path):
    from secs.wizard.help_dialog import HelpDialog

    win = make_window(project, fake, tmp_path / 'w.secs')
    try:
        win.help_button.click()
        qapp.processEvents()
        dialog = win.findChild(HelpDialog)
        assert dialog is not None and dialog.isVisible()
        assert dialog.step == 1
        assert dialog.tab == 'intro'                       # 1단계는 '처음이라면' 부터
        assert '샘플 데이터' in dialog.browser.toPlainText()
        dialog.set_tab('steps')
        assert 'REW API 모드로 실행' in dialog.browser.toPlainText()
        win.go(2)
        qapp.processEvents()
        assert dialog.step == 2 and dialog.tab == 'steps'
        text = dialog.browser.toPlainText()
        assert '7.1.4' in text and '스피커 추가' in text
        dialog.close()
    finally:
        win.close()
        qapp.processEvents()


def test_help_button_tooltip_says_tutorial(qapp, project, fake, tmp_path):
    win = make_window(project, fake, tmp_path / 'w.secs')
    try:
        assert '따라 하기' in win.help_button.toolTip()
    finally:
        win.close()
        qapp.processEvents()


def test_explanations_left_the_screens(qapp, project, fake, tmp_path):
    """머리말(lead)과 긴 설명 note 가 화면에 남아 있지 않다."""
    win = make_window(project, fake, tmp_path / 'w.secs')
    try:
        leads = [w for w in win.findChildren(QtWidgets.QLabel) if w.objectName() == 'lead' and w.text()]
        assert leads == []
        long_notes = [w.text() for w in win.findChildren(QtWidgets.QLabel)
                      if w.objectName() == 'note' and len(w.text()) > 40 and not w.isHidden()]
        assert long_notes == [], long_notes[:3]
    finally:
        win.close()
        qapp.processEvents()


def test_keepall_wraps_words_but_not_long_tokens():
    from secs.wizard.help_dialog import _keepall

    out = _keepall('마이크를 <둡니다> https://example.com/a/very/long/path')
    assert out.count('white-space:nowrap') == 2
    assert '&lt;둡니다&gt;' in out
    assert 'https://example.com/a/very/long/path' in out and 'nowrap">https' not in out


def test_text_blocks_have_fixed_px_line_height_and_images_do_not(qapp):
    """글 블록은 FixedHeight px(09-24 UI 감사), 그림 블록은 고정하지 않는다 — 고정하면 그림이 잘린다."""
    from secs.wizard.help_dialog import HelpDialog

    dlg = HelpDialog()
    try:
        dlg.set_step(1)
        dlg.set_tab('steps')
        doc = dlg.browser.document()
        fixed = QtGui.QTextBlockFormat.LineHeightTypes.FixedHeight.value
        block = doc.begin()
        text_blocks = images = 0
        while block.isValid():
            fmt = block.blockFormat()
            if _has_image(block):
                images += 1
                assert fmt.lineHeightType() != fixed
            elif block.text().strip():
                text_blocks += 1
                assert fmt.lineHeightType() == fixed and fmt.lineHeight() > 0, block.text()
            block = block.next()
        assert text_blocks > 5 and images >= 4
    finally:
        dlg.close()
        qapp.processEvents()


def test_tabs_rebuild_without_leftovers(qapp):
    """단계를 옮기면 이전 탭 버튼이 남아 겹쳐 보이지 않는다(2026-09-24 실사용 QA 버그). 5단계는 mdat 불러오기 탭이 하나 더(10-09),
    내보내기(8)는 CamillaDSP 탭이 하나 더(10-10 사용자 요청 — 그래서 8 의 기대값이 4 → 5)."""
    from secs.wizard.help_dialog import HelpDialog

    dlg = HelpDialog()
    try:
        dlg.show()          # 창을 연 채로 단계를 옮기면 탭 줄 높이가 0 이 되던 버그(QA 09-24)도 같이 본다
        for step, expected in ((1, 5), (9, 4), (2, 4), (0, 0), (5, 5), (1, 5), (8, 5)):
            dlg.set_step(step)
            qapp.processEvents()
            buttons = dlg.tabs_frame.findChildren(QtWidgets.QPushButton)
            assert len(buttons) == expected, (step, len(buttons))
            assert dlg.tabs_frame.isVisibleTo(dlg) == bool(expected)
            if expected:
                assert dlg.tabs_strip.height() >= 20, (step, dlg.tabs_strip.height())
    finally:
        dlg.close()
        qapp.processEvents()


def test_step5_mdat_tab_is_named_load_mdat_and_picture_first(qapp):
    """10-10 사용자: 탭 이름은 "mdat 불러오기"(규칙이 아니다), 대충 읽는 사람도 알게 그림 위주 — 그림 둘 이상(한·영)."""
    import json
    import re

    from secs.i18n import language_manager
    from secs.wizard import tutorial_text as tut
    from secs.wizard.help_dialog import HelpDialog, original_image, tabs_for

    assert ('names', 'mdat 불러오기') in tabs_for(5)
    assert all(label != 'mdat 이름 규칙' for _key, label in tabs_for(5))
    manifest = json.loads((tut_dir() / 'MANIFEST.json').read_text(encoding='utf-8'))['shots']
    keys = tut.names_image_keys()
    assert len(keys) >= 2 and len(keys) == len(set(keys))
    for key in keys:                       # 캡션의 번호 수 = 그림 속 빨간 번호 수
        assert len(tut.caption_of(key)[1]) == len(manifest[key]['marks']), key

    manager = language_manager()
    before = manager.language
    dlg = HelpDialog()
    try:
        for lang, label in (('ko', 'mdat 불러오기'), ('en', 'Load mdat')):
            manager.set_language(lang, persist=False)
            dlg.set_step(5)
            dlg.set_tab('names')
            qapp.processEvents()
            assert dlg._tab_buttons['names'].text() == label
            html = dlg.browser.toHtml()
            assert len(re.findall(r'<img ', html)) >= 2, lang
            for key in keys:
                assert original_image(key, lang) is not None, (key, lang)
    finally:
        manager.set_language(before, persist=False)
        dlg.close()
        qapp.processEvents()


def test_step8_camilla_tab_is_named_camilladsp_and_picture_first(qapp):
    """10-10 사용자: 내보내기 도움말에 '카밀라DSP 탭' — 다른 사람이 윈도우 · 맥에서 아무것도 없는 상태에서 CamillaGUI 로
    소리 날 때까지. 그림 많이(한 · 영 각각 6장 이상), 캡션 번호 수 = 그림 속 빨간 번호 수, 영어에 한글 없음."""
    import json
    import re

    from secs.i18n import language_manager
    from secs.wizard import tutorial_text as tut
    from secs.wizard.help_dialog import HelpDialog, original_image, tabs_for

    assert tabs_for(8)[-1] == ('camilla', 'CamillaDSP')
    assert all(key != 'camilla' for step in (1, 2, 3, 4, 5, 9) for key, _label in tabs_for(step))
    manifest = json.loads((tut_dir() / 'MANIFEST.json').read_text(encoding='utf-8'))['shots']
    keys = tut.camilla_image_keys()
    assert len(keys) >= 6 and len(keys) == len(set(keys))
    assert set(keys) == set(tut.CAMILLA_CAPTIONS)
    for key in keys:                       # 캡션의 번호 수 = 그림 속 빨간 번호 수
        assert len(tut.caption_of(key)[1]) == len(manifest[key]['marks']), key
        for lang in ('ko', 'en'):
            assert manifest[key][lang] == f'{key}_{lang}.png' and len(manifest[key]['by_lang'][lang]['marks']) == \
                len(manifest[key]['marks']), (key, lang)

    hangul = re.compile(r'[가-힣]')
    manager = language_manager()
    before = manager.language
    dlg = HelpDialog()
    try:
        for lang in ('ko', 'en'):
            manager.set_language(lang, persist=False)
            dlg.set_step(8)
            dlg.set_tab('camilla')
            qapp.processEvents()
            assert dlg._tab_buttons['camilla'].text() == 'CamillaDSP'
            html = dlg.browser.toHtml()
            assert len(re.findall(r'<img ', html)) == len(keys), lang
            text = dlg.browser.toPlainText()
            for fact in ('route_in', 'route_out', 'Rate adjust', 'BlackHole', 'F32_LE', '48000', '../coeffs/'):
                assert fact in text, (lang, fact)
            if lang == 'en':
                assert not hangul.search(text), hangul.findall(text)[:5]
            for key in keys:
                assert original_image(key, lang) is not None, (key, lang)
    finally:
        manager.set_language(before, persist=False)
        dlg.close()
        qapp.processEvents()


def tut_dir():
    from secs.wizard.help_dialog import TUTORIAL_DIR

    return TUTORIAL_DIR


def test_help_window_controls_have_balloon_tips(qapp):
    """도움말 창 자체의 검색칸·탭·닫기·큰 보기에도 풍선 도움말 키가 달려 있다(09-24 팀 리드)."""
    from secs.wizard import tooltips
    from secs.wizard.help_dialog import HelpDialog

    dlg = HelpDialog()
    try:
        dlg.set_step(1)
        assert tooltips.resolve(dlg.search_edit)[1] == 'win.help_search'
        assert all(tooltips.resolve(b)[1] == 'win.help_tabs' for b in dlg._tab_buttons.values())
        close = [b for b in dlg.findChildren(QtWidgets.QPushButton) if b.text() in ('닫기', 'Close')]
        assert close and tooltips.resolve(close[0])[1] == 'win.help_close'
        viewer = dlg.show_image('s1_rew')
        assert tooltips.resolve(viewer.picture)[1] == 'win.help_viewer'
        viewer.close()
    finally:
        dlg.close()
        qapp.processEvents()


def test_same_step_keeps_the_tab_new_step_starts_at_the_first(qapp):
    from secs.wizard.help_dialog import HelpDialog

    dlg = HelpDialog()
    try:
        dlg.set_step(3)
        dlg.set_tab('trouble')
        dlg.set_step(3)                      # 언어 전환 등으로 다시 불려도 보던 탭 그대로
        assert dlg.tab == 'trouble'
        dlg.set_step(4)
        assert dlg.tab == 'steps'
    finally:
        dlg.close()
        qapp.processEvents()


def test_images_fit_the_window_and_follow_resize(qapp):
    """그림 폭 ≤ 뷰포트(가로 스크롤 없음), 창을 줄이면 디바운스 뒤 그림도 줄어든다."""
    from secs.wizard.help_dialog import HelpDialog

    dlg = HelpDialog()
    try:
        dlg.resize(900, 720)
        dlg.set_step(2)
        dlg.show()
        qapp.processEvents()
        wide = dlg._rendered_width
        dlg.resize(520, 420)
        qapp.processEvents()
        QtCore.QThread.msleep(dlg.RESIZE_DEBOUNCE_MS + 60)
        qapp.processEvents()
        assert dlg._rendered_width < wide
        assert dlg.browser.horizontalScrollBar().maximum() == 0
        doc_width = dlg.browser.document().idealWidth()
        assert doc_width <= dlg.browser.viewport().width() + 1, (doc_width, dlg.browser.viewport().width())
    finally:
        dlg.close()
        qapp.processEvents()


def test_clicking_an_image_opens_the_viewer(qapp):
    from secs.wizard.help_dialog import HelpDialog, ImageViewer

    dlg = HelpDialog()
    try:
        dlg.set_step(1)
        dlg.set_tab('steps')
        dlg.show()
        qapp.processEvents()
        dlg.browser.anchorClicked.emit(QtCore.QUrl('tut:s1_rew'))
        qapp.processEvents()
        viewer = dlg.viewer
        assert isinstance(viewer, ImageViewer) and viewer.key == 's1_rew' and viewer.isVisible()
        screen = viewer.screen().availableGeometry()
        size = viewer.picture.pixmap().deviceIndependentSize()      # 논리 크기(화면 배율 뺀 것)
        assert size.width() <= screen.width() * 0.9 + 1 and size.height() <= screen.height() * 0.9 + 1
        QtWidgets.QApplication.sendEvent(viewer, QtGui.QKeyEvent(
            QtCore.QEvent.Type.KeyPress, QtCore.Qt.Key.Key_Escape, QtCore.Qt.KeyboardModifier.NoModifier))
        qapp.processEvents()
        assert not viewer.isVisible()
    finally:
        dlg.close()
        qapp.processEvents()


def test_search_finds_text_in_the_current_tab(qapp):
    from secs.wizard.help_dialog import HelpDialog

    dlg = HelpDialog()
    try:
        dlg.set_step(5)
        dlg.search_edit.setText('이 위치 측정')
        assert dlg.browser.textCursor().hasSelection()
        dlg.set_tab('real')
        dlg.search_edit.setText('스윕')
        assert dlg.browser.textCursor().hasSelection()
    finally:
        dlg.close()
        qapp.processEvents()


def test_help_text_wraps_only_at_word_boundaries(qapp):
    """1~8단계 튜토리얼을 한/영 × 창 폭 2개(기본 640, 좁게 520) × 전 탭으로 그려도 한글 낱말이 중간에서
    끊기지 않는다(2026-09-24 사용자 지적: "줄 넘김도 제대로 안 돼 있잖아").

    화면에는 띄우지 않는다(WA_DontShowOnScreen, conftest 가 QT_QPA_PLATFORM=windows 일 때 자동으로 건다).
    """
    from secs.i18n import language_manager
    from secs.wizard.help_dialog import HelpDialog

    manager = language_manager()
    before = manager.language
    try:
        for lang in ('ko', 'en'):
            manager.set_language(lang, persist=False)
            for width in (640, 520):
                dlg = HelpDialog()
                dlg.resize(width, 720)
                try:
                    for step in range(0, 9):
                        dlg.set_step(step)
                        dlg.show()
                        for tab in dlg.tab_keys():
                            dlg.set_tab(tab)
                            qapp.processEvents()
                            violations = _midword_hangul_breaks(dlg.browser.document())
                            assert not violations, (lang, width, step, tab, violations[:3])
                finally:
                    dlg.close()
                    qapp.processEvents()
    finally:
        manager.set_language(before, persist=False)
