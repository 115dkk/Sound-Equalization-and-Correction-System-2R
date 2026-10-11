"""한국어/영어 표시 계층 — 위저드(`secs.wizard.qt`)가 쓴다.

Qt 위젯을 그대로 감싼 번역 위젯(`QLabel`·`QPushButton`·`QComboBox` 등, 이 모듈에서 새로
정의)을 화면 쪽이 대신 임포트해 쓰면, 문구는 원문 그대로 넘기고 `translate()`(전체 카탈로그
`secs.translations.CATALOG` 조회 → `{n}` 패턴 → 줄/목록/구분자 단위 재귀)가 표시 시점의
언어로 바꿔 그린다. `LanguageManager` 가 현재 언어를 `QSettings` 에 저장하고 전환 시
등록된 위젯 전체의 `_retranslate()` 를 부른다. DSP 로 들어가는 값(콤보박스의 `currentText()`
등)과 파일 포맷 문자열은 항상 번역 전 원문 그대로 유지된다 — REW 채널 ID 등을 문구로 오인해
바꾸면 안 되기 때문이다.
"""
from __future__ import annotations
import os
import re
import weakref
from PyQt6 import QtCore, QtWidgets
from PyQt6 import sip as _sip
from secs.translations import CATALOG

_manager = None
_patterns = []

#: 사용자가 영어 화면을 고를 수 있는지. 2026-10-11 사용자: "영어화면은 임시로 닫아놓자"(공개 Beta 1.0.0 —
#: 영어 번역 완성도). False 면 사이드바 언어 칸이 없고 저장된 'en' 도 시작할 때 한국어로 연다(설정은 지우지 않음).
#: 번역 코드 · 영어 표는 그대로라 시험 · QA 도구의 set_language('en') 는 계속 된다. 다시 열려면 True.
ENGLISH_UI = False
_patterns_size = -1
_HANGUL = re.compile(r'[가-힣]')
#: translate() 메모 — (문구, 언어) → 결과. 창 하나를 그릴 때 같은 문구를 수천 번 묻는다
#: (09-22 프로파일: 2,693회 · 정규식 734k 회 · 0.36 s). 카탈로그 크기가 바뀌면 비운다.
_memo: dict = {}
_memo_size = -1
_MEMO_LIMIT = 8192


def _compile_patterns():
    """CATALOG 의 `{n}` 문구를 정규식으로. 위저드 번역표는 나중에 등록되므로 크기가 바뀌면 다시 만든다."""
    global _patterns_size
    _patterns.clear()
    for source, translations in CATALOG.items():
        if re.search(r'\{\d+\}', source):
            chunks = re.split(r'(\{\d+\})', source)
            fields = []
            regex = ''
            for chunk in chunks:
                if re.fullmatch(r'\{\d+\}', chunk):
                    index = int(chunk[1:-1])
                    regex += rf'(?P<f{index}>[\s\S]*?)' if index not in fields else f'(?P=f{index})'
                    fields.append(index)
                else:
                    regex += re.escape(chunk)
            _patterns.append((len(re.sub(r'\{\d+\}', '', source)), re.compile(regex), translations))
    _patterns.sort(key=lambda entry: entry[0], reverse=True)
    _patterns_size = len(CATALOG)


def translate(source, language=None):
    """Translate text into the requested language, or the current language if omitted.

    Try exact catalog matches, placeholder patterns, and translation by line, list item,
    or separator in order, returning untranslated parts unchanged. The function does
    not distinguish UI strings from user data.
    """
    text = str(source)
    code = language or language_manager().language
    index = 0 if code == 'ko' else 1
    if text in CATALOG:
        return CATALOG[text][index]
    global _memo_size
    if _memo_size != len(CATALOG):
        _memo.clear()
        _memo_size = len(CATALOG)
    key = (text, code)
    cached = _memo.get(key)
    if cached is not None:
        return cached
    result = _translate_uncached(text, code, index)
    if len(_memo) < _MEMO_LIMIT:
        _memo[key] = result
    return result


def _translate_uncached(text, code, index):
    if code == 'ko' and _HANGUL.search(text) and not re.search(r'[A-Za-z]{3}', text):
        return text  # 위저드 원문은 한국어다 — 영어 조각이 없으면 고칠 것이 없다.
    if _patterns_size != len(CATALOG):
        _compile_patterns()
    for _, pattern, translations in _patterns:
        match = pattern.fullmatch(text)
        if match:
            values = {key: value for key, value in match.groupdict().items()}
            # (옛 1.41 화면의 enum 값·REW 안내 칸을 옮기던 갈래는 2026-10-10 슬림화로 그 화면과 함께 뺐다.)
            for key, value in values.items():
                if code != 'ko' and value and _HANGUL.search(value):
                    # 위저드 문구에 끼워 넣은 한국어 값('사용자 정의', '반전' …)도 같이 옮긴다.
                    # 목록('밴드 3(+4 dB), 밴드 5(…)')은 항목별로 — 통째로 맞추면 첫 항목 패턴이 전부를 삼킨다.
                    values[key] = ', '.join(translate(part, code) for part in value.split(', '))
            result = translations[index]
            return re.sub(r'\{(\d+)\}', lambda m: values['f'+m.group(1)], result)
    if '\n' in text:
        return '\n'.join(translate(line, code) for line in text.split('\n'))
    if text.startswith('- ') or text.startswith('  • '):
        prefix = '- ' if text.startswith('- ') else '  • '
        return prefix + translate(text[len(prefix):], code)
    if code != 'ko' and _HANGUL.search(text):
        # 위저드가 ' · ' / ', ' / ' / ' 로 이어 붙인 문구는 조각별로 옮긴다(' / ' = 6·7단계 경고 목록, 09-23).
        for separator in (' · ', ', ', ' / '):
            if separator in text:
                return separator.join(translate(part, code) for part in text.split(separator))
    return text


class _QtTranslator(QtCore.QTranslator):
    def __init__(self, manager):
        super().__init__(manager)
        self.manager = manager

    def isEmpty(self):
        return False

    def translate(self, context, sourceText, disambiguation=None, n=-1):
        values = CATALOG.get(sourceText)
        return values[0 if self.manager.language == 'ko' else 1] if values else ''


class LanguageManager(QtCore.QObject):
    language_changed = QtCore.pyqtSignal(str)

    def __init__(self):
        super().__init__()
        test_path = os.environ.get('SECS4_SETTINGS_PATH')
        self.settings = (QtCore.QSettings(test_path, QtCore.QSettings.Format.IniFormat)
                         if test_path else QtCore.QSettings('SECS', 'SECS4'))
        value = self.settings.value('ui/language', 'ko', type=str)
        self.language = value if value in ('ko', 'en') and ENGLISH_UI else 'ko'
        self.widgets = weakref.WeakValueDictionary()
        self.qt_catalog = QtCore.QTranslator(self)
        self.qt_fallback = _QtTranslator(self)
        self._install_qt_language()

    def _install_qt_language(self):
        app = QtWidgets.QApplication.instance()
        if app is None:
            return
        app.removeTranslator(self.qt_catalog)
        app.removeTranslator(self.qt_fallback)
        locale = QtCore.QLocale('ko_KR' if self.language == 'ko' else 'en_US')
        QtCore.QLocale.setDefault(locale)
        if self.language == 'ko':
            directory = QtCore.QLibraryInfo.path(QtCore.QLibraryInfo.LibraryPath.TranslationsPath)
            if self.qt_catalog.load('qtbase_ko', directory):
                app.installTranslator(self.qt_catalog)
        app.installTranslator(self.qt_fallback)

    def register(self, widget):
        self.widgets[id(widget)] = widget

    def set_language(self, language, persist=True):
        if language not in ('ko', 'en'):
            raise ValueError('Unsupported language: '+str(language))
        if persist:
            self.settings.setValue('ui/language', language)
            self.settings.sync()
        if language == self.language:
            return
        self.language = language
        self._install_qt_language()
        for widget in list(self.widgets.values()):
            try:
                widget._retranslate()
            except RuntimeError:  # A Qt-owned child may already have been deleted.
                self.widgets.pop(id(widget), None)
        self.language_changed.emit(language)


def language_manager():
    global _manager
    # QApplication 이 다시 만들어지면(테스트) 앞선 관리자는 Qt 쪽에서 이미 지워져 있다.
    if _manager is None or _sip.isdeleted(_manager):
        _manager = LanguageManager()
    return _manager


_PROPERTIES = {'setText': 'text', 'setToolTip': 'toolTip', 'setWindowTitle': 'windowTitle',
               'setPlaceholderText': 'placeholderText', 'setTitle': 'title',
               'setInformativeText': 'informativeText', 'setDetailedText': 'detailedText'}


class Translated(str):
    """이미 옮긴 글자 + 원문. 위저드 `t()` 가 돌려준다 — 번역 위젯이 원문을 기억해 언어를 되돌릴 수 있다."""

    def __new__(cls, text, source):
        item = super().__new__(cls, text)
        item.source = str(getattr(source, 'source', source))
        return item


class _Localized:
    def __init__(self, *args, **kwargs):
        self._translation_sources = {}
        super().__init__(*args, **kwargs)
        given = next((a for a in args if isinstance(a, Translated)), None)
        for setter, getter in _PROPERTIES.items():
            if hasattr(self._qt_base, setter) and hasattr(self._qt_base, getter):
                value = getattr(self._qt_base, getter)(self)
                if isinstance(value, str) and value:
                    self._translation_sources[setter] = (given.source if given is not None
                                                         and value == str(given) else value)
        language_manager().register(self)
        self._retranslate()

    def _retranslate(self):
        for setter, source in self._translation_sources.items():
            getattr(self._qt_base, setter)(self, translate(source))


def _make_widget(base):
    methods = {'_qt_base': base, '__module__': __name__}
    for setter in _PROPERTIES:
        if not hasattr(base, setter):
            continue
        def localized_set(self, source, _setter=setter, _base=base):
            source = getattr(source, 'source', source)
            self._translation_sources[_setter] = str(source)
            getattr(_base, _setter)(self, translate(source))
        methods[setter] = localized_set
    return type(base.__name__, (_Localized, base), methods)


for _name in ('QWidget', 'QLabel', 'QPushButton', 'QCheckBox', 'QDialog', 'QFrame', 'QGroupBox',
              'QDoubleSpinBox', 'QSpinBox', 'QSlider', 'QProgressBar', 'QScrollArea', 'QSplitter',
              'QLineEdit', 'QToolButton', 'QRadioButton', 'QStackedWidget'):
    globals()[_name] = _make_widget(getattr(QtWidgets, _name))


class QPlainTextEdit(_make_widget(QtWidgets.QPlainTextEdit)):
    """로그 창 — 덧붙이는 순간의 언어로 적는다(지난 줄은 다시 옮기지 않는다)."""

    def appendPlainText(self, text):
        QtWidgets.QPlainTextEdit.appendPlainText(self, translate(text))

    def setPlainText(self, text):
        QtWidgets.QPlainTextEdit.setPlainText(self, translate(text))


class QComboBox(_make_widget(QtWidgets.QComboBox)):
    """Display translations while preserving the original text-based model API.

    currentText()/itemText() intentionally return canonical source strings, as
    REW uses them as channel IDs. displayText() exposes the selected UI label.
    itemData(), indexes, device names and emitted selection changes stay stable.
    """
    _SOURCE_ROLE = int(QtCore.Qt.ItemDataRole.UserRole) + 441

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        # Recalculate the preferred width when translated labels become longer.
        self.setSizeAdjustPolicy(QtWidgets.QComboBox.SizeAdjustPolicy.AdjustToContents)

    def addItem(self, *args, **kwargs):
        source = args[0] if args and isinstance(args[0], str) else args[1]
        position = 0 if isinstance(args[0], str) else 1
        values = list(args)
        values[position] = translate(source)
        index = self.count()
        QtWidgets.QComboBox.addItem(self, *values, **kwargs)
        self.setItemData(index, source, self._SOURCE_ROLE)

    def addItems(self, texts):
        for text in texts:
            self.addItem(text)

    def itemText(self, index):
        source = self.itemData(index, self._SOURCE_ROLE)
        return source if source is not None else QtWidgets.QComboBox.itemText(self, index)

    def currentText(self):
        return self.itemText(self.currentIndex()) if self.currentIndex() >= 0 else QtWidgets.QComboBox.currentText(self)

    def displayText(self):
        return QtWidgets.QComboBox.currentText(self)

    def setCurrentText(self, text):
        index = self.findText(text)
        if index >= 0:
            self.setCurrentIndex(index)
        else:
            QtWidgets.QComboBox.setCurrentText(self, text)

    def findText(self, text, flags=QtCore.Qt.MatchFlag.MatchExactly | QtCore.Qt.MatchFlag.MatchCaseSensitive):
        for index in range(self.count()):
            if self.itemText(index) == text:
                return index
        return QtWidgets.QComboBox.findText(self, text, flags)

    def setItemText(self, index, text):
        self.setItemData(index, text, self._SOURCE_ROLE)
        QtWidgets.QComboBox.setItemText(self, index, translate(text))

    def _retranslate(self):
        super()._retranslate()
        blocked = self.blockSignals(True)
        try:
            for index in range(self.count()):
                QtWidgets.QComboBox.setItemText(self, index, translate(self.itemText(index)))
        finally:
            self.blockSignals(blocked)


QMainWindow = _make_widget(QtWidgets.QMainWindow)


class QTableWidgetItem(QtWidgets.QTableWidgetItem):
    def __init__(self, text='', *args, translate_text=True):
        self._source_text = str(text) if isinstance(text, str) else None
        self._translate_text = translate_text
        super().__init__(translate(text) if isinstance(text, str) and translate_text else text, *args)
        language_manager().register(self)

    def setText(self, text):
        self._source_text = str(text)
        super().setText(translate(text) if self._translate_text else text)

    def _retranslate(self):
        if self._source_text is not None and self._translate_text:
            super().setText(translate(self._source_text))


class QTableWidget(_make_widget(QtWidgets.QTableWidget)):
    def setHorizontalHeaderLabels(self, labels):
        self._header_sources = list(labels)
        QtWidgets.QTableWidget.setHorizontalHeaderLabels(self, [translate(label) for label in labels])

    def _retranslate(self):
        super()._retranslate()
        if hasattr(self, '_header_sources'):
            QtWidgets.QTableWidget.setHorizontalHeaderLabels(self, [translate(label) for label in self._header_sources])


class QDialogButtonBox(_make_widget(QtWidgets.QDialogButtonBox)):
    def button(self, which):
        button = QtWidgets.QDialogButtonBox.button(self, which)
        if not hasattr(self, '_localized_buttons'):
            self._localized_buttons = {}
        if button is not None:
            # Retain borrowed Qt wrappers so their custom labels survive a switch.
            self._localized_buttons[which] = button
        if button is not None and not hasattr(button, '_retranslate'):
            reference = weakref.ref(button)
            button._source_text = None
            def set_text(text):
                target = reference()
                if target is not None:
                    target._source_text = str(text)
                    QtWidgets.QPushButton.setText(target, translate(text))
            def retranslate():
                target = reference()
                if target is not None and target._source_text is not None:
                    QtWidgets.QPushButton.setText(target, translate(target._source_text))
            button.setText = set_text
            button._retranslate = retranslate
            language_manager().register(button)
        return button


class QMessageBox(_make_widget(QtWidgets.QMessageBox)):
    def addButton(self, *args):
        if args and isinstance(args[0], str):
            button = QPushButton(args[0], self)
            QtWidgets.QMessageBox.addButton(self, button, *args[1:])
            return button
        return QtWidgets.QMessageBox.addButton(self, *args)


def _message(method):
    def show(parent, title, text, *args, **kwargs):
        language_manager()
        return getattr(QtWidgets.QMessageBox, method)(parent, translate(title), translate(text), *args, **kwargs)
    return staticmethod(show)


for _name in ('information', 'warning', 'critical', 'question', 'about'):
    setattr(QMessageBox, _name, _message(_name))


class QFileDialog(QtWidgets.QFileDialog):
    pass


def _file_dialog(method):
    def show(*args, **kwargs):
        language_manager()
        values = list(args)
        if len(values) > 1:
            values[1] = translate(values[1])
        if len(values) > 3 and isinstance(values[3], str):
            values[3] = translate(values[3])
        for key in ('caption', 'filter'):
            if key in kwargs:
                kwargs[key] = translate(kwargs[key])
        # Keep Qt's native system file picker; translate only the supplied text.
        return getattr(QtWidgets.QFileDialog, method)(*values, **kwargs)
    return staticmethod(show)


for _name in ('getOpenFileName', 'getOpenFileNames', 'getSaveFileName', 'getExistingDirectory'):
    setattr(QFileDialog, _name, _file_dialog(_name))


class QInputDialog(QtWidgets.QInputDialog):
    @staticmethod
    def getText(parent, title, label, *args, **kwargs):
        return QtWidgets.QInputDialog.getText(parent, translate(title), translate(label), *args, **kwargs)

    @staticmethod
    def getItem(parent, title, label, items, *args, **kwargs):
        translated = [translate(item) for item in items]
        value, accepted = QtWidgets.QInputDialog.getItem(parent, translate(title), translate(label), translated, *args, **kwargs)
        return (items[translated.index(value)] if value in translated else value, accepted)


class LanguageComboBox(QComboBox):
    @QtCore.pyqtSlot(int)
    def select_language(self, index):
        language_manager().set_language(self.itemData(index))

    @QtCore.pyqtSlot(str)
    def sync_language(self, language):
        blocked = self.blockSignals(True)
        self.setCurrentIndex(0 if language == 'ko' else 1)
        self.blockSignals(blocked)


def add_language_selector(window, header):
    manager = language_manager()
    label = QLabel('Language')
    label.setObjectName('languageLabel')
    combo = LanguageComboBox()
    combo.setObjectName('languageSelector')
    combo.setMinimumWidth(105)
    combo.addItem('한국어', 'ko')
    combo.addItem('English', 'en')
    combo.setToolTip('Change the interface language. Your selection is saved automatically.')
    combo.setCurrentIndex(0 if manager.language == 'ko' else 1)
    combo.currentIndexChanged.connect(combo.select_language)
    manager.language_changed.connect(combo.sync_language)
    header.addWidget(label)
    header.addWidget(combo)
    window.language_combo = combo


def __getattr__(name):
    # Untouched Qt types (layouts, constants and QApplication) keep their usual API.
    return getattr(QtWidgets, name)
