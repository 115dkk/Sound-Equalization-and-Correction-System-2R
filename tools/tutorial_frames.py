"""튜토리얼 오버레이 눈 확인용 캡처(2026-10-09) — 실제 windows 플랫폼(글꼴)에서 창을 화면에 띄우지 않고(WA_DontShowOnScreen)
몇 장면을 PNG 로 뽑는다. 사람이 손으로 돌린다.

    .venv-secs\\Scripts\\python.exe tools\\tutorial_frames.py [출력 폴더]
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
os.environ.pop('QT_QPA_PLATFORM', None)


def main() -> int:
    from PyQt6 import QtCore, QtWidgets

    out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / '_mac_out' / 'tutorial_frames'
    out.mkdir(parents=True, exist_ok=True)
    app = QtWidgets.QApplication.instance() or QtWidgets.QApplication(sys.argv[:1])
    from secs.wizard import build_backend, new_project
    from secs.wizard.strings import CAL_STEP
    from secs.wizard.window import WizardWindow

    rew, devices, levels, measurer = build_backend(fake=True, use_sample_dolby=True)
    win = WizardWindow(new_project(True), rew=rew, devices=devices, levels=levels, measurer=measurer)
    win.ask_before_close = False
    win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    win.resize(1280, 800)
    win.show()
    win.set_trial(True)

    def settle(ms=400):
        timer = QtCore.QElapsedTimer()
        timer.start()
        while timer.elapsed() < ms:
            app.processEvents()

    settle()
    tut = win.tutorial

    def shot(name):
        tut._place()
        settle(250)
        win.grab().save(str(out / f'{name}.png'))
        print('saved', out / f'{name}.png')

    def goto(pred):
        for _ in range(200):
            if pred(tut.current()):
                return
            tut.index += 1
            tut._show()
        raise SystemExit('quest not found')

    shot('01_step1_look')
    goto(lambda q: q.kind == 'act' and q.screen == 1)
    shot('02_step1_act')
    goto(lambda q: q.screen == 5 and q.kind == 'act')
    shot('03_step5_sample')
    win.measure_page.load_sample()
    goto(lambda q: q.screen == CAL_STEP and q.kind == 'act')
    shot('04_cal_compute')
    tut.panel.set_folded(True)
    shot('05_cal_folded')
    tut.stop()
    win.close()
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
