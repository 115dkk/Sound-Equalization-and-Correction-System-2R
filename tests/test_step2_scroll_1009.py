"""구성 탭: 표에서 출력을 고쳐도 스크롤이 맨 위로 튀지 않는다(2026-10-09 사용자)."""
from __future__ import annotations

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def test_changing_output_keeps_scroll(qapp, fake, tmp_path):  # noqa: F811
    from secs import model

    project = model.Project(name='s', config=model.SpeakerConfig.from_preset('7.1.4'))
    win = make_window(project, fake, None)
    win.resize(1100, 600)
    win.show()
    try:
        win.go(2)
        qapp.processEvents()
        page = win.step_widget()
        bar = page._scroll_bar()
        assert bar is not None and bar.maximum() > 0
        bar.setValue(bar.maximum())
        keep = bar.value()
        combo = page._band_cells[-1][0] if isinstance(page._band_cells[-1], tuple) else None
        combos = [c for c in page.table.findChildren(type(page.device_combo)) if c.count() > 10]
        target = combos[-1]
        page._table_key = None            # 표를 통째로 다시 만드는 길(중복 경고 등)도 본다
        target.setCurrentIndex((target.currentIndex() + 1) % target.count())
        for _ in range(5):
            qapp.processEvents()
        import time
        time.sleep(0.1)
        qapp.processEvents()
        assert bar.value() == keep, (bar.value(), keep)
        _ = combo
    finally:
        win.close()
