# -*- coding: utf-8 -*-
"""위저드용 `QtWidgets` 대역 — 글자를 가진 위젯만 `secs.i18n` 의 번역 위젯으로 바꾼다.

단계 모듈은 `from secs.wizard import qt as QtWidgets` 로 가져다 쓴다. `setText`·
`setToolTip` 같은 설정자가 원문을 기억했다가 언어가 바뀌면 다시 옮기므로, 화면
코드는 한국어 원문만 쓰면 된다. 레이아웃·상수 등 나머지 이름은 PyQt6 그대로다.
"""
from __future__ import annotations

from PyQt6 import QtWidgets as _QtWidgets

from secs import i18n as _i18n
from secs.wizard import strings as _strings  # noqa: F401 - 번역표 등록

QWidget = _i18n.QWidget
QLabel = _i18n.QLabel
QPushButton = _i18n.QPushButton
QCheckBox = _i18n.QCheckBox
QDialog = _i18n.QDialog
QFrame = _i18n.QFrame
QSpinBox = _i18n.QSpinBox
QDoubleSpinBox = _i18n.QDoubleSpinBox
QSlider = _i18n.QSlider
QProgressBar = _i18n.QProgressBar
QScrollArea = _i18n.QScrollArea
QLineEdit = _i18n.QLineEdit
QStackedWidget = _i18n.QStackedWidget
QPlainTextEdit = _i18n.QPlainTextEdit
QMainWindow = _i18n.QMainWindow
QTableWidget = _i18n.QTableWidget
QTableWidgetItem = _i18n.QTableWidgetItem
QDialogButtonBox = _i18n.QDialogButtonBox
QMessageBox = _i18n.QMessageBox
QFileDialog = _i18n.QFileDialog
QInputDialog = _i18n.QInputDialog


class QComboBox(_i18n.QComboBox):
    """번역 콤보. 폭 정책은 Qt 기본값으로 되돌린다 — 실사용 QA 로 맞춘 폭이 긴 장치 이름에 밀리지 않게."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.setSizeAdjustPolicy(_QtWidgets.QComboBox.SizeAdjustPolicy.AdjustToContentsOnFirstShow)


QComboBox.__name__ = 'QComboBox'


def __getattr__(name):
    return getattr(_QtWidgets, name)
