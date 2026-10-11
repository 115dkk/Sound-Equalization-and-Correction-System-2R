"""영어 화면 임시로 닫기(2026-10-11 사용자: "코드는 남겨놓되 ui에서 선택창만 없애면 될 듯").

사이드바에 언어 칸이 없고, 전에 'en' 을 저장한 PC 도 한국어로 연다(되돌릴 칸이 없으니). 번역 코드는 그대로라
`set_language('en')` 은 계속 된다(시험 · QA 도구가 쓴다). 영어 화면을 다시 열면(`i18n.ENGLISH_UI = True`) 이 파일을 지운다.
"""
from __future__ import annotations

from test_wizard_common import engine, fake, make_window, project, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def test_flag_is_off(engine):
    from secs import i18n

    assert i18n.ENGLISH_UI is False


def test_sidebar_has_no_language_selector(qapp, project, fake):  # noqa: F811
    from PyQt6 import QtWidgets

    window = make_window(project, fake)
    try:
        assert not hasattr(window, 'language_combo')
        assert window.findChild(QtWidgets.QComboBox, 'languageSelector') is None
        assert window.findChild(QtWidgets.QLabel, 'languageLabel') is None
    finally:
        window.close()


def test_saved_english_opens_in_korean(qapp, tmp_path):  # noqa: F811
    import os

    from PyQt6 import QtCore
    from secs.i18n import LanguageManager

    ini = tmp_path / 'settings.ini'
    saved = QtCore.QSettings(str(ini), QtCore.QSettings.Format.IniFormat)
    saved.setValue('ui/language', 'en')
    saved.sync()
    old = os.environ.get('SECS4_SETTINGS_PATH')
    os.environ['SECS4_SETTINGS_PATH'] = str(ini)
    try:
        manager = LanguageManager()
        assert manager.language == 'ko'
        assert manager.settings.value('ui/language', type=str) == 'en'   # 설정은 지우지 않는다
    finally:
        if old is None:
            os.environ.pop('SECS4_SETTINGS_PATH', None)
        else:
            os.environ['SECS4_SETTINGS_PATH'] = old


def test_translation_code_still_works(qapp):  # noqa: F811
    from secs.i18n import language_manager, translate

    manager = language_manager()
    try:
        manager.set_language('en', persist=False)
        assert translate('저장') != '' and manager.language == 'en'
    finally:
        manager.set_language('ko', persist=False)
