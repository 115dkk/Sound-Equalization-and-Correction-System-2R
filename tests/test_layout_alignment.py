"""단계를 넘겨도 본문·오른쪽 칸이 흔들리지 않는다 (2026-09-23 사용자 지시).

"각 단계별로 가운데 영역과 오른쪽 영역의 크기가 계속 달라진다 — 깔끔하게 정렬해 달라."
모든 단계가 `style.two_columns` 를 쓰므로, 같은 창 크기에서 제목·본문 칸의 시작 x·y 와
오른쪽 칸의 x·폭이 단계마다 같아야 한다(2 px 이내). 오른쪽 칸 폭은 창 폭만의 함수다.
"""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path

import pytest

from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

SIZES = [(1024, 640), (1280, 800), (1920, 1080), (2560, 1440)]


def _qa_capture():
    path = Path(__file__).resolve().parents[1] / "tools" / "qa_capture.py"
    spec = importlib.util.spec_from_file_location("qa_capture_layout", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_side_width_rule(engine):
    from secs.wizard.style import SIDE_MAX, SIDE_MIN, side_width_for

    assert side_width_for(700) == SIDE_MIN
    assert side_width_for(900) == 360
    assert side_width_for(1600) == SIDE_MAX


@pytest.mark.parametrize("preset,biamp", [("2.1", False), ("9.1.6", True)])
def test_columns_line_up_across_steps(qapp, tmp_path, preset, biamp):
    if (os.environ.get("QT_QPA_PLATFORM", "").split(":")[0] == "offscreen"
            or qapp.platformName() == "offscreen"):
        pytest.skip("실제 글꼴 폭 검사는 QT_QPA_PLATFORM=windows 에서 실행")
    from PyQt6 import QtCore
    from secs.wizard.strings import FLOW

    qa = _qa_capture()
    win = qa.build_window(preset, biamp, 0, tmp_path)
    try:
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.show()
        for size in SIZES:
            win.resize(*size)
            rows = {}
            for step in FLOW:                             # 1–5 · 보정 설계(9) · 내보내기(8) — 6 · 7 은 2026-10-10 에 지움
                win.go(step)
                qa.settle(qapp)
                rows[step] = qa.layout_geometry(win)
                area = win.stack.currentWidget()
                assert area.horizontalScrollBar().maximum() == 0, (size, step)
            heads = {(g["head_x"], g["head_y"]) for g in rows.values()}
            assert len(heads) == 1, (size, heads)
            mains = [(g["main_x"], g["main_y"]) for s, g in rows.items() if g["main_x"] is not None]
            assert len(set(mains)) == 1, (size, mains)
            sides = [(s, g["side_x"], g["side_w"]) for s, g in rows.items() if g["side_x"] is not None]
            # 오른쪽 칸이 있는 단계: 1·3·4·5·6·7·보정 설계(구성은 넓은 창에서만 옆에 온다)
            assert {s for s, _x, _w in sides} >= {1, 3, 4, 5, 9}, (size, sides)
            xs = [x for _s, x, _w in sides]
            ws = [w for _s, _x, w in sides]
            assert max(xs) - min(xs) <= 2 and max(ws) - min(ws) <= 2, (size, sides)
    finally:
        win.close()
        qa.settle(qapp)
