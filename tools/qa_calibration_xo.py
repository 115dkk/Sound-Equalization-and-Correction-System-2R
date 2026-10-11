# -*- coding: utf-8 -*-
"""보정 설계 화면 XO 카드·XO 탭 실사용 QA 캡처(2026-09-27) — `tools/qa_capture.py` 의 창·측정·점검을 그대로 쓴다.
FakeREW 2.1(서브 1) 측정 2위치 → 보정 계산 → 창 크기마다 SPL 탭·XO 탭(선 100 Hz 로 옮긴 뒤) 캡처, 한/영.
결과: 보고서/05 화면 QA/<날짜> 보정 설계 XO/ + 표준 출력 표(가로 스크롤·글자 잘림·영어 화면의 한글).
    python tools/qa_calibration_xo.py [--sizes 1024x640,1280x800,1920x1080,1024x420]
"""
from __future__ import annotations

import argparse
import datetime as _dt
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'src'), str(ROOT / 'tools')]


def main() -> int:
    import numpy as np
    import qa_capture as qc
    from PyQt6 import QtCore, QtWidgets
    from secs.i18n import language_manager
    from secs.wizard.strings import CAL_STEP

    ap = argparse.ArgumentParser()
    ap.add_argument('--sizes', default='1024x640,1280x800,1920x1080,1024x420')
    ap.add_argument('--dolby', action='store_true', help='샘플 모드(돌비룸 실측 7.1.4 · 서브 3, 1점)')
    ap.add_argument('--langs', default='ko,en')
    a = ap.parse_args()
    sizes = [tuple(int(v) for v in s.split('x')) for s in a.sizes.split(',')]
    out = ROOT / '보고서' / '05 화면 QA' / f'{_dt.date.today():%Y-%m-%d} 보정 설계 XO'
    out.mkdir(parents=True, exist_ok=True)
    app = QtWidgets.QApplication.instance() or QtWidgets.QApplication(sys.argv)
    tmp = Path(tempfile.mkdtemp())
    win = qc.build_window('7.1.4', False, 1, tmp, sample_dolby=True) if a.dolby else qc.build_window('2.1', False, 2, tmp)
    if a.dolby:
        win.project.config.set_sub_count(3)
        out = out.parent / (out.name + ' 돌비')
        out.mkdir(parents=True, exist_ok=True)
    win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    win.show()
    qc.measure(win, app, 1 if a.dolby else 2)
    rows = []
    try:
        for lang in a.langs.split(','):
            language_manager().set_language(lang, persist=False)
            qc.settle(app)
            for size in sizes:
                tag = f'{size[0]}x{size[1]}_{lang}'
                page = win.calibration_page
                page.set_tab('spl')
                rows.append(qc.capture(win, app, CAL_STEP, size, out, f'보정설계_{tag}_spl', scan_text=(lang == 'en')))
                page.set_tab('xo')
                page.xo_line.setValue(np.log10(100.0))              # 끄는 중 상태(곡선·칸만 바뀜)
                qc.settle(app)
                rows.append(qc.capture(win, app, CAL_STEP, size, out, f'보정설계_{tag}_xo', scan_text=(lang == 'en')))
    finally:
        language_manager().set_language('ko', persist=False)
        win.close()
    print('| 캡처 | 가로 스크롤 | 잘림 | 영어 화면 한글 |')
    print('|---|---|---|---|')
    for r in rows:
        print(f"| {r['name']} | {r['hscroll']} | {'; '.join(r['clipped']) or '-'} | {'; '.join(r['hangul']) or '-'} |")
    print(out)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
