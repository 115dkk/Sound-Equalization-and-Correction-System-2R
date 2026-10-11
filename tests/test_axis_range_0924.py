"""그래프 축 범위 조절(2026-09-24 사용자 요청: "그래프가 나오는 모든 부분은 축의 상한과 하한을 조정").

모서리 [축] 단추 → 네 칸 + [적용]·[자동]. 값은 그래프 종류별로 실행 동안만 기억(저장 파일 밖).
옛 6·7단계 그래프 검사는 2026-10-10 슬림화로 화면과 함께 빠졌고, 같은 조절기를 5단계 측정 그래프와
맨 `pg.PlotWidget` 으로 본다.
"""
from __future__ import annotations

import numpy as np
import pytest

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


@pytest.fixture(autouse=True)
def _fresh():
    from secs.wizard.axis_range import clear_session
    clear_session()
    yield
    clear_session()


def _view(plot):
    (x0, x1), (y0, y1) = plot.getPlotItem().vb.viewRange()
    return x0, x1, y0, y1


def test_range_is_checked_remembered_per_kind_and_reset(qapp, engine):
    import pyqtgraph as pg
    from secs.wizard.axis_range import AxisControl, session_range

    plot = pg.PlotWidget()
    plot.setLogMode(x=True)
    try:
        control = AxisControl(plot, 'unit_mag')
        assert not control.set_range(100, 50, -10, 10)                # 하한 ≥ 상한은 거절
        assert not control.set_range(0, 50, -10, 10)                  # 로그 축은 0 보다 커야
        assert control.set_range(30, 300, -12, 6)
        x0, x1, y0, y1 = _view(plot)
        assert (10 ** x0, 10 ** x1) == pytest.approx((30, 300)) and (y0, y1) == pytest.approx((-12, 6))
        assert session_range('unit_mag') == pytest.approx((30, 300, -12, 6))
        assert session_range('other') is None                         # 종류별로 따로
        control.reset()
        assert session_range('unit_mag') is None
    finally:
        plot.close()


def test_editor_fills_current_range_and_applies(qapp, engine):
    import pyqtgraph as pg
    from secs.wizard.axis_range import AxisControl

    plot = pg.PlotWidget()
    plot.setLogMode(x=True)
    plot.setXRange(np.log10(15), np.log10(22000), padding=0.0)
    plot.setYRange(-30, 15, padding=0.0)
    try:
        control = AxisControl(plot, 'unit_editor')
        editor = control.editor()
        editor.fill(control.current())
        values = [box.value() for box in editor.boxes]
        assert values[0] == pytest.approx(15, abs=1) and values[1] == pytest.approx(22000, abs=1)
        editor.boxes[2].setValue(-20)
        editor.boxes[3].setValue(12)
        editor._apply()
        assert _view(plot)[2:] == pytest.approx((-20, 12))
        editor.boxes[2].setValue(30)                                 # 하한 > 상한 → 알림, 그대로
        editor._apply()
        assert editor.note.text() and _view(plot)[2:] == pytest.approx((-20, 12))
        editor._auto()
        assert control.kind not in __import__('secs.wizard.axis_range', fromlist=['_SESSION'])._SESSION
    finally:
        plot.close()


def test_step5_axis_control_survives_navigation_and_is_not_saved(qapp, fake, tmp_path, engine):
    import secs.model as model

    project = model.Project(name='축', config=model.SpeakerConfig.from_preset('2.1'))
    win = make_window(project, fake, tmp_path / 'axis.secs')
    try:
        win.step5.load_sample(2)
        win.go(5)
        assert win.step5.axis_control.set_range(20, 2000, -20, 10)
        assert np.isclose(10 ** _view(win.step5.plot)[1], 2000)
        win.go(4)
        win.go(5)
        assert np.isclose(_view(win.step5.plot)[3], 10)              # 단계를 오가도 남는다
        path = tmp_path / 'saved.secs'
        win.save_project(str(path))
        text = path.read_text(encoding='utf-8')
        assert '2000.0, -20' not in text and 'axis' not in text              # 축 범위는 저장하지 않는다
    finally:
        win.close()
        qapp.processEvents()
