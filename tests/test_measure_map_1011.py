"""5 · 측정 배치도 크기 — 10-11 사용자: "측정 위치 표시가 너무 작은데" → "세로 배치이되 그래프를 끌 수 있게".

[그래프 숨기기] 를 누르면 그래프 · 범례 · 위치 칩이 숨고 배치도가 그 높이를 받아 커진다. 다시 누르면 되돌아온다.
가로 배치(배치도를 그래프 옆에)는 사용자가 거절했다 — 배치도는 늘 그래프 위.
"""
from __future__ import annotations

import pytest

from conftest import load_engine


@pytest.fixture(scope="module")
def engine():
    return load_engine("package")


@pytest.fixture(scope="module")
def qapp():
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance() or QApplication([])
    yield app
    app.processEvents()


def test_hiding_the_graph_enlarges_the_map(qapp, engine):
    import secs.model as model
    import secs.rewapi as rewapi
    from secs.wizard import WizardWindow

    project = model.Project(config=model.SpeakerConfig.from_preset("5.1", biamp=False))
    project.extra["array"] = 9
    rew = rewapi.FakeREW(delay_s=0.0)
    window = WizardWindow(project, rew, rew, rew, rew)
    try:
        window.resize(1280, 800)
        window.show()
        window.go(5)
        qapp.processEvents()
        page = window.step5
        shown = page.view.height()
        assert page.plot_toggle.text() == "그래프 숨기기" and not page.plot.isHidden()

        page.plot_toggle.click()
        qapp.processEvents()
        assert page.plot_hidden()
        assert page.plot.isHidden() and page._legend_holder.isHidden() and page.position_chips.isHidden()
        assert page.plot_toggle.text() == "그래프 보기"
        assert page.view.height() > shown * 2                           # 배치도가 그래프 몫을 받는다
        assert page.view.width() <= page._left_card.width()             # 카드 폭 안에서

        page.plot_toggle.click()
        qapp.processEvents()
        assert not page.plot_hidden() and not page.plot.isHidden()
        assert abs(page.view.height() - shown) <= 8                     # 넘침을 다시 재서 몇 px 다를 수 있다
        # 배치도는 늘 그래프 위(가로 배치는 거절됨)
        assert page.view.geometry().bottom() < page.plot.geometry().top()
    finally:
        window.close()
        qapp.processEvents()
