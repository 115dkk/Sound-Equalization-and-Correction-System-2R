# -*- coding: utf-8 -*-
"""시작 로딩 창 — 무거운 모듈(numpy·scipy·pyqtgraph·위저드 화면)을 읽는 동안 로고와 문구를 보여 준다.

`secs.__main__.main()` 이 위저드로 가기로 정한 직후, 그 모듈들을 임포트하기 **전에** 만든다
(2026-09-22 사용자: "로딩이 길다면 로딩창을"). 빌드된 exe 는 켜서 창이 뜨기까지 2~3 초 걸리는데,
그동안 아무것도 안 보이면 안 켜진 줄 알고 두 번 누르게 된다. PyQt6 만 읽으면 되니 0.3 초 안에 뜬다.
창이 다 만들어지면 `run_wizard` 가 `finish(window)` 로 닫는다.
"""
from __future__ import annotations

from PyQt6 import QtCore, QtGui, QtWidgets

#: 여기서 만든 QApplication 의 참조(run_wizard 가 `instance()` 로 이어받는다).
_app = None


def _logo_background() -> QtGui.QColor:
    """로고 png(`app_256.png`)의 바탕색 — 그림 가장자리 안쪽 픽셀에서 읽는다. 없으면 섀시 색."""
    from secs.wizard.branding import ASSETS
    from secs.wizard.style import COLORS

    image = QtGui.QImage(str(ASSETS / 'app_256.png'))
    if not image.isNull():
        color = image.pixelColor(6, image.height() // 2)
        if color.alpha() == 255:
            return color
    return QtGui.QColor(COLORS['chassis'])


class StartupSplash(QtWidgets.QWidget):
    """로고 방울 애니메이션(로딩 카드와 같은 스프라이트) + 이름 + 진행 문구.

    09-24 사용자: "프로그램 켜는 로딩창도 애니메이션 만든 걸로". 임포트는 화면 스레드를 막으므로
    무거운 계산 모듈은 `preload()` 가 뒤 스레드에서 읽고, 그동안 화면 스레드가 이벤트를 돌려 방울이 움직인다.
    """

    SIZE = 380, 300

    def __init__(self) -> None:
        super().__init__(None, QtCore.Qt.WindowType.SplashScreen | QtCore.Qt.WindowType.FramelessWindowHint
                         | QtCore.Qt.WindowType.WindowStaysOnTopHint)
        from secs.version import version_text
        from secs.wizard.loading import LoadingIndicator

        self.setFixedSize(*self.SIZE)
        self.setAutoFillBackground(True)
        palette = self.palette()
        palette.setColor(QtGui.QPalette.ColorRole.Window, _logo_background())   # 로고 바탕색과 같게(09-22)
        self.setPalette(palette)
        box = QtWidgets.QVBoxLayout(self)
        box.setContentsMargins(24, 28, 24, 18)
        box.setSpacing(8)
        self.indicator = LoadingIndicator()
        box.addWidget(self.indicator, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)
        self.title = QtWidgets.QLabel(version_text())
        self.title.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        self.title.setStyleSheet('color: #f2f6f5; font-size: 17px; font-weight: 700;')
        box.addWidget(self.title)
        box.addStretch(1)
        self.status = QtWidgets.QLabel('')
        self.status.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        self.status.setStyleSheet('color: #a9c4c1; font-size: 13px;')
        box.addWidget(self.status)
        screen = QtWidgets.QApplication.primaryScreen()
        if screen is not None:
            self.move(screen.availableGeometry().center() - self.rect().center())

    def show(self) -> None:  # noqa: D401 - Qt 이름
        super().show()
        self.indicator.start()

    def message(self, text: str) -> None:
        """아래쪽 진행 문구를 바꾸고 바로 그린다(임포트 사이사이에 부른다)."""
        from secs.wizard.strings import t      # 언어 설정을 따른다(PyQt 만 읽으므로 가볍다)

        self.status.setText(str(t(text)))
        app = QtWidgets.QApplication.instance()
        if app is not None:
            app.processEvents()

    def preload(self, modules) -> None:
        """`modules` 를 뒤 스레드에서 임포트하는 동안 화면 이벤트를 돌려 애니메이션을 살린다.

        위젯을 만들지 않는 순수 계산 모듈(numpy·scipy)만 넘긴다. 뒤 스레드 임포트가 실패해도 조용히
        지나간다 — 뒤이은 화면 스레드의 실제 임포트가 제대로 된 오류를 낸다.
        """
        import importlib
        import threading

        def work() -> None:
            for name in modules:
                try:
                    importlib.import_module(name)
                except Exception:  # noqa: BLE001
                    pass

        worker = threading.Thread(target=work, name='splash-preload', daemon=True)
        worker.start()
        app = QtWidgets.QApplication.instance()
        while worker.is_alive():
            if app is not None:
                app.processEvents(QtCore.QEventLoop.ProcessEventsFlag.AllEvents, 15)
            worker.join(0.015)

    def finish(self, window=None) -> None:
        self.indicator.stop()
        self.close()


def show_startup_splash() -> StartupSplash | None:
    """QApplication 을 만들고(없으면) 로딩 창을 띄운다. 실패하면 None — 로딩 창은 있으면 좋은 것이다."""
    global _app
    try:
        import sys

        if QtWidgets.QApplication.instance() is None:
            # 참조를 들고 있어야 한다 — 지역 변수로 만들면 함수가 끝나며 파괴돼 다음 위젯에서 죽는다.
            _app = QtWidgets.QApplication(sys.argv[:1])
        splash = StartupSplash()
        splash.show()
        splash.message('시작하는 중…')
        return splash
    except Exception:  # noqa: BLE001 - 로딩 창 없이도 프로그램은 떠야 한다
        return None


__all__ = ['StartupSplash', 'show_startup_splash']
