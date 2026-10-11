"""pytest 공용 설정.

`load_engine()` 은 `src` 를 sys.path 에 넣고 `secs` 패키지를 돌려준다. 1.41 원본(`secs4.py`)을 고를 수 있던
엔진 선택(`SECS_ENGINE`)과 골든 대조(`tests/golden`, `make_golden.py`)는 2026-10-10 슬림화로 지웠다 — 원본은
`문서/지난 자료/secs4.py` 에 보존, 되살릴 기준은 git 태그 slim-before-2026-10-10. `engine_choice()` 는 옛 테스트의
`skipif(engine_choice() != "package")` 를 그대로 두려고 늘 'package' 를 돌려준다.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

import pytest

TESTS_DIR = Path(__file__).resolve().parent
ROOT = TESTS_DIR.parent
SRC = ROOT / "src"

# 하위 모듈이 PyQt6 를 끌어올 수 있다 — 따로 정하지 않으면 화면 없는 플랫폼으로.
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _keep_windows_off_screen() -> None:
    """실제 windows 플랫폼(글꼴 폭이 맞는 쪽)으로 돌려도 창이 사용자 화면에 튀어나오지 않게 한다.

    최상위 창은 보이기 직전에 WA_DontShowOnScreen 을 달아 화면 밖에서만 그린다 — 크기·글꼴·
    grab() 은 그대로다(qa_capture 와 같은 방식). 2026-09-24 사용자: 테스트 중 파이썬 창이 자꾸 뜬다.
    창을 직접 보고 싶으면 SECS_SHOW_WINDOWS=1.
    """
    if os.environ.get("SECS_SHOW_WINDOWS") or os.environ.get("QT_QPA_PLATFORM") != "windows":
        return
    from PyQt6 import QtCore, QtWidgets

    hidden = QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen

    def wrap(original):
        def set_visible(self, visible):
            if visible and self.isWindow():
                self.setAttribute(hidden, True)
            original(self, visible)
        return set_visible

    # C++ 쪽 show() 도 이 파이썬 재구현을 부르도록 클래스마다 단다(QDialog 는 자체 setVisible 이 있다).
    for cls in (QtWidgets.QWidget, QtWidgets.QDialog, QtWidgets.QMainWindow, QtWidgets.QMenu,
                QtWidgets.QFileDialog, QtWidgets.QInputDialog):
        cls.setVisible = wrap(cls.setVisible)


_keep_windows_off_screen()

# 화면 언어는 사용자 설정(레지스트리)에 저장된다. 테스트는 그 값에 흔들리지 않게 빈 ini 로 돌린다.
if "SECS4_SETTINGS_PATH" not in os.environ:
    import tempfile

    os.environ["SECS4_SETTINGS_PATH"] = str(Path(tempfile.mkdtemp(prefix="secs-test-")) / "settings.ini")

_ENGINE_CACHE: dict[str, object] = {}


def engine_choice() -> str:
    """늘 'package'(1.41 `secs4` 선택은 2026-10-10 에 지웠다)."""
    return "package"


def load_engine(which: str | None = None):
    """`src` 를 sys.path 에 넣고 `secs` 패키지를 돌려준다(`which` 는 옛 호출부 호환용, 'package' 만)."""
    if which not in (None, "package"):
        raise RuntimeError(f"엔진은 'package' 하나뿐이다: {which!r}")
    if "package" in _ENGINE_CACHE:
        return _ENGINE_CACHE["package"]
    if not (SRC / "secs").is_dir():
        raise RuntimeError(f"패키지를 찾을 수 없음: {SRC / 'secs'}")
    if str(SRC) not in sys.path:
        sys.path.insert(0, str(SRC))
    import secs as module  # type: ignore[import-not-found]

    _ENGINE_CACHE["package"] = module
    return module


@pytest.fixture(scope="session")
def engine():
    return load_engine()
