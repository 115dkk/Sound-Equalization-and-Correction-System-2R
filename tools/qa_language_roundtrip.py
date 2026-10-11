# -*- coding: utf-8 -*-
"""언어 왕복 점검 — 각 단계에서 한국어 → English → 한국어로 바꿨을 때 글자가 처음과 같은지.

    .venv-secs\\Scripts\\python.exe tools\\qa_language_roundtrip.py [--preset 9.1.6 --biamp]

`t()` 로 이미 영어가 된 글자를 위젯에 넣으면 원문을 잃어 한국어로 못 돌아온다. 그런 곳을 찾는다.
FakeREW 로 돈다(소리 안 남).
"""
from __future__ import annotations

import argparse
import os
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
sys.path.insert(0, str(ROOT / 'tools'))
os.environ.pop('QT_QPA_PLATFORM', None)
os.environ['SECS4_SETTINGS_PATH'] = str(Path(tempfile.mkdtemp(prefix='secs-qa-')) / 'settings.ini')


def texts(root) -> dict[str, str]:
    from PyQt6 import QtWidgets, sip

    found: dict[str, str] = {}
    for index, w in enumerate(root.findChildren(QtWidgets.QWidget)):
        if sip.isdeleted(w) or not w.isVisible():
            continue
        if isinstance(w, (QtWidgets.QLabel, QtWidgets.QAbstractButton)):
            found[f'{index}:{type(w).__name__}'] = w.text()
            found[f'{index}:tip'] = w.toolTip()
        elif isinstance(w, QtWidgets.QComboBox):
            found[f'{index}:combo'] = '|'.join(QtWidgets.QComboBox.itemText(w, i) for i in range(w.count()))
        elif isinstance(w, QtWidgets.QTableWidget):
            heads = [w.horizontalHeaderItem(c).text() if w.horizontalHeaderItem(c) else ''
                     for c in range(w.columnCount())]
            cells = [w.item(r, c).text() for r in range(w.rowCount()) for c in range(w.columnCount())
                     if w.item(r, c) is not None]
            found[f'{index}:table'] = '|'.join(heads + cells)
        elif isinstance(w, QtWidgets.QListWidget):
            found[f'{index}:list'] = '|'.join(w.item(r).text() for r in range(w.count()))
    return found


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--preset', default='2.1')
    ap.add_argument('--biamp', action='store_true')
    args = ap.parse_args(argv)

    from PyQt6 import QtCore
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance() or QApplication([])
    import qa_capture
    from secs.i18n import language_manager

    manager = language_manager()
    manager.set_language('ko', persist=False)
    problems = []
    with tempfile.TemporaryDirectory() as tmp:
        win = qa_capture.build_window(args.preset, args.biamp, 2, Path(tmp))
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.show()
        qa_capture.measure(win, app, 2)
        from secs.wizard.strings import FLOW

        for step in FLOW:                      # 화면 번호, 흐름 순서(옛 6·7 은 2026-10-10 에 지움)
            win.go(step)
            widget = win.step_widget()
            if hasattr(widget, 'wait'):
                widget.wait(180000)
            qa_capture.settle(app)
            before = texts(win)
            manager.set_language('en', persist=False)
            qa_capture.settle(app)
            manager.set_language('ko', persist=False)
            qa_capture.settle(app)
            after = texts(win)
            # 언어가 바뀌면 화면을 다시 만들어 위젯 순서가 달라진다 → 글자 묶음으로 비교한다.
            from collections import Counter

            had, has = Counter(v for v in before.values() if v), Counter(v for v in after.values() if v)
            for text in (had - has):
                problems.append(f'{step}단계 사라짐: {text[:70]!r}')
            for text in (has - had):
                problems.append(f'{step}단계 새로 생김: {text[:70]!r}')
        win.close()
        qa_capture.settle(app)
    print(f'언어 왕복 후 달라진 글자 {len(problems)}건')
    for line in problems:
        print('  ' + line)
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
