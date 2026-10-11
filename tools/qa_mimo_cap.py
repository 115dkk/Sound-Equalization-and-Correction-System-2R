# -*- coding: utf-8 -*-
"""보정 설계 MIMO 카드 캡처(2026-10-05 R28 '입력당 서포트 최대 n') — 합성 방 2.2(서브 2, 위치 3곳 → MIMO 를 켤 수 있다).

돌비 mdat 은 위치 1곳이라 MIMO 카드가 꺼져 있어 합성 방으로 찍는다: 보정 계산(최소 지연) 한 번 → MIMO 켬 · 입력당 최대 4 →
언어마다 1024×640 에서 MIMO 카드가 보이게 내린 화면 한 장 + `qa_capture.capture` 의 가로 스크롤·잘림 점검. 창은 화면에 띄우지
않는다(실제 windows 플랫폼, 숨은 창), 프로세스 우선순위 '보통 이하'.

    .venv-secs\\Scripts\\python.exe tools\\qa_mimo_cap.py --out <폴더> [--cap 4]
"""
from __future__ import annotations

import argparse
import copy
import os
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'src'), str(ROOT / 'tests'), str(ROOT / 'tools')]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--out', required=True)
    ap.add_argument('--cap', type=int, default=4)
    a = ap.parse_args()
    if sys.platform == 'win32':
        import ctypes
        ctypes.windll.kernel32.SetPriorityClass(ctypes.windll.kernel32.GetCurrentProcess(), 0x4000)
    os.environ['SECS4_SETTINGS_PATH'] = str(Path(tempfile.mkdtemp(prefix='secs-qa-')) / 'settings.ini')

    import qa_capture as qc
    import room_model_decay as rmd
    from PyQt6 import QtCore, QtWidgets

    import secs.rewapi as rewapi
    from secs.i18n import language_manager
    from secs.wizard import WizardWindow
    from secs.wizard.strings import CAL_STEP

    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    app = QtWidgets.QApplication.instance() or QtWidgets.QApplication(sys.argv)
    case = rmd.build_case('2.2', positions=3)
    project = copy.deepcopy(case.project)
    project.config.set_sub_count(2)
    roles = ('L', 'R', 'SW1', 'SW2')
    project.measurements = [{'pos': p, 'ir': {r: f'{r}_P{p}.wav' for r in roles}} for p in range(1, 4)]
    irs = {pos: {r: v for r, v in by_role.items() if r in roles} for pos, by_role in case.irs.items()}
    tmp = Path(tempfile.mkdtemp(prefix='secs-qa-mimo-'))
    rew = rewapi.FakeREW(delay_s=0.0)
    win = WizardWindow(project, rew, rew, rew, rew, project_path=tmp / 'mimo.secs')
    win.ask_before_close = False
    win.auto_calibrate = False
    win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    win.show()
    rows = []
    try:
        page = win.calibration_page
        page.set_context(tmp / 'mimo.secs', lambda: irs)
        win._set_calibration_context = lambda: None      # 화면 이동이 IR 공급 함수를 바꾸지 않게(R5 테스트와 같은 방식)
        win.go(CAL_STEP)
        page.mode_buttons['zero'].click()
        started = time.perf_counter()
        page.compute()
        while page.running():
            page.wait(1000)
            app.processEvents()
        print(f'보정 계산 {time.perf_counter() - started:.0f} s · {page.status.text()}', flush=True)
        if not page.mimo_check.isChecked():
            page.mimo_check.click()
        page.mimo_max_spin.setValue(a.cap)
        for lang in ('ko', 'en'):
            language_manager().set_language(lang, persist=False)
            qc.settle(app)
            row = qc.capture(win, app, CAL_STEP, (1024, 640), out, f'MIMO카드_1024x640_{lang}_전체', scan_text=(lang == 'en'))
            row['window'] = (win.width(), win.height())
            rows.append(row)
            area = win.stack.currentWidget()
            area.ensureWidgetVisible(page.mimo_status, 0, 40)
            qc.settle(app)
            win.grab().save(str(out / f'MIMO카드_1024x640_{lang}.png'))
    finally:
        language_manager().set_language('ko', persist=False)
        win.close()
        qc.settle(app)
        rew.close()
    lines = ['| 캡처 | 창 크기 | 가로 스크롤(px) | 잘림 의심 | 영어 화면의 한글 |', '|---|---|---|---|---|']
    for r in rows:
        lines.append(f"| {r['name']} | {r['window'][0]}×{r['window'][1]} | {r['hscroll']} | {'<br>'.join(r['clipped'][:8]) or '-'} | "
                     f"{'<br>'.join(r['hangul'][:6]) or '-'} |")
    text = '\n'.join(lines)
    print(text)
    (out / '점검 표.md').write_text(text + '\n', encoding='utf-8')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
