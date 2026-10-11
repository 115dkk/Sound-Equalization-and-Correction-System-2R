"""위저드 테스트 공용 도우미 (트랙 C).

테스트 자체는 없고, `test_wizard_*.py` 가 쓰는 픽스처·헬퍼만 둔다.
오프스크린(`QT_QPA_PLATFORM=offscreen`)은 `conftest.py` 가 이미 걸어 준다.
"""
from __future__ import annotations

import pytest

from conftest import engine_choice, load_engine

pytestmark = pytest.mark.skipif(
    engine_choice() != "package",
    reason="secs.wizard 는 패키지 엔진 전용이다 (SECS_ENGINE=package).",
)

#: 테스트 기본 프리셋(스피커 3대 → 측정 role 5개).
PRESET = "2.1"


@pytest.fixture(scope="module")
def engine():
    return load_engine("package")


@pytest.fixture(scope="module")
def qapp(engine):
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance() or QApplication([])
    yield app
    app.processEvents()


@pytest.fixture()
def project(engine):
    import secs.model as model

    # 테스트는 바이앰프(LF/HF) 배정을 전제로 쓰였다 — 제품 기본값은 09-22 부터 끔이라 명시한다.
    return model.Project(name="위저드", config=model.SpeakerConfig.from_preset(PRESET, biamp=True))


@pytest.fixture()
def fake(engine):
    """FakeREW 하나가 다섯 프로토콜을 모두 맡는다(계약 B)."""
    from secs.rewapi.fake import FakeREW

    rew = FakeREW(delay_s=0.0)
    yield rew
    rew.close()


def make_window(project, rew, path=None):
    from secs.wizard import WizardWindow

    window = WizardWindow(project, rew, rew, rew, rew, project_path=path)
    window.ask_before_close = False      # 테스트는 닫을 때 저장 대화상자를 띄우지 않는다
    return window
