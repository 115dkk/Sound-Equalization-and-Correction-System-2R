# -*- coding: utf-8 -*-
"""P6 성능 점검 — 계획서 기준(측정 직후 표시 < 1 s, 화면 전환)을 이 PC 에서 잰다.

    .venv-secs\\Scripts\\python.exe tools\\bench_p6.py

옛 6단계 설계·7단계 저역 통합 시간 재기는 2026-10-10 슬림화로 그 엔진과 함께 뺐다(보정 설계 계산 시간은
`tests/test_correction_engine_speed.py`).

FakeREW 로 돈다(소리 안 남). 결과는 표준 출력의 마크다운 표.
"""
from __future__ import annotations

import os
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
sys.path.insert(0, str(ROOT / 'tests'))
sys.path.insert(0, str(ROOT / 'tools'))
os.environ.pop('QT_QPA_PLATFORM', None)
os.environ['SECS4_SETTINGS_PATH'] = str(Path(tempfile.mkdtemp(prefix='secs-bench-')) / 'settings.ini')


def timed(func, repeat: int = 1) -> float:
    best = float('inf')
    for _ in range(repeat):
        started = time.perf_counter()
        func()
        best = min(best, time.perf_counter() - started)
    return best


def bench_wizard(preset: str, biamp: bool, app, rows: list) -> None:
    import qa_capture
    from PyQt6 import QtCore

    with tempfile.TemporaryDirectory() as tmp:
        win = qa_capture.build_window(preset, biamp, 2, Path(tmp))
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.show()
        win.go(5)
        started = time.perf_counter()
        win.step5.measure_position(1)
        win.step5.wait(120000)
        app.processEvents()
        measured = time.perf_counter() - started
        rows.append((f'{preset} 5단계 한 위치 측정+그리기(Fake, 스윕 0 s)', measured, 1.0 * len(win.step5.plan().roles)))
        win.step5.measure_position(2)
        win.step5.wait(120000)
        draw = timed(lambda: (win.step5._draw(), app.processEvents()), repeat=3)
        rows.append((f'{preset} 5단계 곡선 다시 그리기', draw, 1.0))

        from secs.wizard.window import PAGE_NAMES

        for step in PAGE_NAMES:
            spent = timed(lambda s=step: (win.go(s), app.processEvents()))
            widget = win.step_widget()
            if hasattr(widget, 'wait'):
                widget.wait(180000)
            if spent > 0.3:
                rows.append((f'{preset} {step}단계 화면 전환', spent, 1.0))
        win.close()
        app.processEvents()


def bench_translate(rows: list) -> None:
    import secs.wizard.strings  # noqa: F401
    from secs.i18n import translate

    samples = ['-12.3 dB', '3 / 9 위치', '측정 실패: 장치를 찾을 수 없습니다', 'L · 왼쪽 프런트',
               '이 문구는 번역표에 없습니다 REW']
    for code in ('ko', 'en'):
        spent = timed(lambda: [translate(s, code) for s in samples * 200], repeat=3) / (len(samples) * 200)
        rows.append((f'translate() 1회 평균({code})', spent, 0.001))


def main() -> int:
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance() or QApplication([])
    rows: list = []
    bench_translate(rows)
    bench_wizard('2.1', False, app, rows)
    bench_wizard('9.1.6', True, app, rows)
    print('| 항목 | 시간 | 기준 | 판정 |')
    print('|---|---|---|---|')
    for name, spent, limit in rows:
        shown = f'{spent * 1000:.2f} ms' if spent < 0.1 else f'{spent:.2f} s'
        verdict = '-' if limit is None else ('통과' if spent <= limit else '**초과**')
        print(f'| {name} | {shown} | {"-" if limit is None else f"≤ {limit:g} s"} | {verdict} |')
    return 0


if __name__ == '__main__':
    sys.exit(main())
