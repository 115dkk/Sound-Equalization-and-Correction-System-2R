"""QA-D: 내보내기(화면 8)의 실제 글꼴 폭 회귀 검사.

`tools/qa_capture.py` 를 그대로 써서 1024·1280 폭에 가로 스크롤·잘린 글자가 없는지 본다
(**오프스크린 플랫폼은 글꼴이 없어 폭이 틀리다** — 그때는 건너뛴다). 옛 8단계(시트·Tesira·검증)의
순서 밖 조작 검사는 2026-10-10 슬림화로 그 화면과 함께 지웠다.
"""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path

import pytest

from conftest import engine_choice, load_engine

pytestmark = pytest.mark.skipif(
    engine_choice() != "package",
    reason="secs.wizard 는 패키지 엔진 전용이다 (SECS_ENGINE=package).",
)


@pytest.fixture(scope="module")
def engine():
    return load_engine("package")


@pytest.fixture(scope="module")
def qapp(engine):
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance() or QApplication([])
    yield app
    app.processEvents()


# ──────────────────────────── 폭 회귀 ────────────────────────────
@pytest.mark.parametrize('size', [(1024, 640), (1280, 800)], ids=['1024x640', '1280x800'])
@pytest.mark.parametrize('preset,biamp', [('2.1', False), ('9.1.6', True)])
def test_step8_has_no_horizontal_scroll_or_clipped_labels(qapp, tmp_path, size, preset, biamp):
    # qa_capture 임포트가 환경변수를 지우므로 플랫폼 확인을 반드시 먼저 한다.
    if os.environ.get('QT_QPA_PLATFORM') == 'offscreen' or qapp.platformName() == 'offscreen':
        pytest.skip('실제 글꼴 폭 검사: QT_QPA_PLATFORM=windows 로 실행')
    from PyQt6.QtCore import Qt

    path = Path(__file__).resolve().parents[1] / 'tools' / 'qa_capture.py'
    spec = importlib.util.spec_from_file_location('qa_capture_d', path)
    capture_tool = importlib.util.module_from_spec(spec)
    platform = os.environ.get('QT_QPA_PLATFORM')
    try:
        spec.loader.exec_module(capture_tool)
    finally:
        if platform is not None:
            os.environ['QT_QPA_PLATFORM'] = platform
    win = capture_tool.build_window(preset, biamp, 0, tmp_path)
    try:
        win.setAttribute(Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.show()
        row = capture_tool.capture(win, qapp, 8, size, tmp_path,
                                   f'step8_{preset}_{size[0]}x{size[1]}_qa_d')
        assert win.width() == size[0]
        assert row['hscroll'] == 0, row
        assert row['clipped'] == [], row
    finally:
        win.close()
        qapp.processEvents()
