# -*- coding: utf-8 -*-
"""측정 위저드 창 — wizard.run_wizard가 Project, 선택적 프로젝트 경로와
REW·장치·레벨·측정 인터페이스를 주입해 WizardWindow를 만든다.
창은 단계 화면을 표시하고 공유 Project를 갱신하며, 저장 요청 시 .secs 파일을 쓴다.

이 창이 하는 일은 셋이다.
1. 단계 화면(`step1_devices` … `step5_measure` + `step_calibration` + `step8_mdat`)을 만들어 스택에 넣고 `go()` 로 오간다.
   화면은 뜻 이름으로 든다(`devices_page`, `config_page`, … `export_page`, `calibration_page` — `PAGE_NAMES`).
   3·4·5·8단계는 옛 번호 이름(`step3` … `step5`, `step8`)도 같은 화면을 가리킨다. 1·2단계는 2026-09-23 순서를
   바꾸며(1 장치 → 2 구성) 번호 이름을 없앴다 — `step1` 이 예전엔 구성이었어서 헷갈린다.
   **흐름**: 1–5 → 보정 설계(화면 번호 `CAL_STEP` = 9) → 내보내기(8). 진행바·다음/이전·Ctrl+숫자는 `strings.FLOW`.
   화면 번호 6·7(옛 필터 설계·저역 통합)과 10(옛 내보내기·검증)은 2026-10-10 슬림화로 화면째 지웠고 번호만 비워 둔다
   (8·9 를 그대로 두려고 — 테스트·도구·튜토리얼이 번호로 부른다).
2. 단계 사이에 넘겨야 하는 것을 넘긴다 — 보정 설계에 프로젝트 경로와 IR 공급 함수, 내보내기(8)에 프로젝트 경로
   (결과는 보정 설계 화면에서 읽는다).
3. 프로젝트 저장·불러오기, 단축키, 언어 전환 뒤 다시 그리기.
화면의 값은 전부 `self.project`(`secs.model.Project`) 하나에 모인다 — 단계 화면은 그것을 읽고 쓴다.
"""
from __future__ import annotations

import numpy as np
import threading
import os
import sys
import time
from pathlib import Path

from PyQt6 import QtCore, QtGui
from secs.wizard import qt as QtWidgets

from secs.i18n import ENGLISH_UI, add_language_selector, language_manager
from secs.model import Project
from secs.version import APP_NAME, VERSION_LABEL, version_text
from secs.wizard.branding import app_icon, mark_pixmap
from secs.wizard.step1_devices import Step1Devices
from secs.wizard.step2_config import Step2Config
from secs.wizard.step3_level import Step3Level
from secs.wizard.step4_layout import Step4Layout
from secs.wizard.step5_measure import Step5Measure
from secs.wizard.step8_mdat import Step8Mdat
from secs.wizard.step_calibration import StepCalibration
from secs.wizard.strings import CAL_STEP, FLOW, STEPS, bar_position, t
from secs.wizard.style import COLORS, FONT_UI, QSS, label, repolish, set_variant
from secs.wizard import tooltips

#: 목업 render() 의 skips — 단계 번호(개정 1: 1 기점) → 건너뛰기 버튼 문구.
SKIP_LABELS = {
    1: '장치 확인 건너뛰기',
    3: '레벨 건너뛰기 (기본값 적용)',
    5: '측정 건너뛰기 (mdat 불러오기)',
}


class _StepsBar(QtWidgets.QFrame):
    """목업 .steps — 연결선 + 단계 점 + 라벨."""

    stepClicked = QtCore.pyqtSignal(int)

    #: 점 중심 y. 현재 단계 점의 빛(반지름 11)과 라벨 윗변(위 여백 36) 사이를 3 px 띄운다
    #: (09-24 QA — 전에는 24·32 라 빛이 라벨에 닿았고, 버튼 높이 24 px 에 아래 여백 12 px 이라
    #: 영어 'Setup'·'Export' 의 p 꼬리가 잘렸다. 아래 여백은 style.py QPushButton#step 에서 6 px).
    DOT_Y = 22

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self.setObjectName('stepsBar')
        self.setFixedHeight(70)
        self._current = 1
        box = QtWidgets.QHBoxLayout(self)
        box.setContentsMargins(28, 36, 28, 4)
        box.setSpacing(0)
        self.buttons: list[QtWidgets.QPushButton] = []
        for index, name in enumerate(STEPS):
            button = QtWidgets.QPushButton(t(name))
            button.setObjectName('step')
            button.setCursor(QtGui.QCursor(QtCore.Qt.CursorShape.PointingHandCursor))
            button.clicked.connect(lambda _=False, i=index + 1: self.stepClicked.emit(i))
            tooltips.attach(button, 'win.step_bar')
            box.addWidget(button, 1)
            self.buttons.append(button)

    def set_current(self, step: int) -> None:
        """step 은 화면 번호(1~8, 보정 설계 9) — 진행바 위치로 바꿔 칠한다(`strings.bar_position`)."""
        self._current = bar_position(int(step))
        for index, button in enumerate(self.buttons, start=1):
            state = 'cur' if index == self._current else 'done' if index < self._current else ''
            button.setProperty('state', state)
            repolish(button)
        self.update()

    def paintEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().paintEvent(event)
        if not self.buttons:
            return
        painter = QtGui.QPainter(self)
        painter.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing)
        centers = [b.geometry().center().x() for b in self.buttons]
        painter.setPen(QtGui.QPen(QtGui.QColor(COLORS['step_off']), 2))
        painter.drawLine(centers[0], self.DOT_Y, centers[-1], self.DOT_Y)
        for index, x in enumerate(centers, start=1):
            if index < self._current:
                color = COLORS['step_done']
            elif index == self._current:
                color = COLORS['marker']
            else:
                color = COLORS['step_off']
            if index == self._current:
                painter.setPen(QtCore.Qt.PenStyle.NoPen)
                painter.setBrush(QtGui.QBrush(QtGui.QColor(48, 227, 218, 90)))
                painter.drawEllipse(QtCore.QPoint(x, self.DOT_Y), 11, 11)
            painter.setPen(QtGui.QPen(QtGui.QColor(COLORS['chassis2']), 2))
            painter.setBrush(QtGui.QBrush(QtGui.QColor(color)))
            painter.drawEllipse(QtCore.QPoint(x, self.DOT_Y), 8, 8)
        painter.end()


#: 화면 번호 → 창 속성 이름. 2026-09-23 사용자 지시로 장치(REW 연결)가 1단계, 구성이 2단계.
#: 9 = 보정 설계(2026-09-26, 옛 6·7 통합), 8 = 내보내기. 6·7·10(옛 필터 설계·저역 통합·내보내기·검증)은
#: 2026-10-10 슬림화로 지웠고 번호는 비워 둔다. 흐름 순서는 `strings.FLOW`.
PAGE_NAMES: dict[int, str] = {1: 'devices_page', 2: 'config_page', 3: 'level_page', 4: 'layout_page',
                              5: 'measure_page', 8: 'export_page', CAL_STEP: 'calibration_page'}


#: `_overlay_jobs` 가 속한 단계(같은 순서) — 로딩창은 그 단계를 볼 때만 덮는다.
OVERLAY_PAGES = (5, 5)


class WizardWindow(QtWidgets.QMainWindow):
    """측정 위저드. 화면 1~5 · 보정 설계(9) · 내보내기(8)."""

    #: REW 점검 주기(초). 점검은 워커 스레드에서 `/version` 하나만 묻고, 상태가 바뀌었거나 값이
    #: 오래됐을 때만 장치 목록까지 새로 읽는다(`rew_cache.RewCache.watch_tick`).
    REW_WATCH_S = 3.0
    #: 이 단계들에서는 점검을 쉰다 — 레벨·측정·검증이 REW 와 직접 이야기하는 중일 수 있다.
    REW_WATCH_SKIP_STEPS = (3, 5, 8)

    def __init__(self, project: Project, rew=None, devices=None, levels=None,
                 measurer=None, parent=None, project_path=None) -> None:
        super().__init__(parent)
        self.project = project
        self.rew = rew
        self.devices = devices
        self.levels = levels
        self.measurer = measurer
        self.project_path = Path(project_path) if project_path else None
        self.current_step = 1        # 화면 번호(`PAGE_NAMES` 의 키)
        self._rew_running: bool | None = None     # 백그라운드 점검이 아는 상태(None = 점검 안 함)
        self._rew_watch_stop = threading.Event()
        self._rew_watch_thread: threading.Thread | None = None
        # 보정 설계·측정 저장이 같은 IR wav 를 한 번만 읽게 나눠 쓰는 캐시(`rewapi.session.load_project_irs`).
        self._ir_cache: dict = {}
        self._go_ms: dict[int, float] = {}      # 단계별 마지막 go() 시간 — 느렸던 단계는 로딩 표시를 먼저 그린다
        # 화면 스레드는 REW 에 직접 묻지 않는다 — 단계 화면은 이 캐시를 REW·장치 목록 자리에 받는다
        # (09-23 사용자: "단계별로 이동할 때 딜레이가 굉장히 심하다", 원인은 화면 스레드의 동기 HTTP).
        from secs.wizard.rew_cache import RewCache
        self.rew_cache = RewCache(rew, devices, parent=self)
        self.rew_cache.updated.connect(self._on_rew_updated)

        self.setWindowTitle(f'{version_text()} · {t("측정 위저드")}')
        self.setWindowIcon(app_icon())
        self.resize(1280, 800)
        # Windows 11 기본 스타일은 스핀박스 ▲▼ 를 옆으로 넓게 그려 글자와 겹치고 클릭 영역이 어긋난다
        # (09-22 사용자: 6단계 제어점 표·7단계 조건 스핀박스). Fusion 은 QSS 와 같이 써도 예측대로 그린다.
        app = QtWidgets.QApplication.instance()
        if app is not None and app.style().objectName().lower() != 'fusion':
            app.setStyle('Fusion')
        # Fusion 은 Windows 다크 모드를 따라 메뉴·대화상자 바탕을 검게 칠하는데 QSS 글자는 짙은 색이라
        # 우클릭 메뉴·도움말이 안 읽혔다(09-23 사용자). 앱은 밝은 테마 하나뿐이므로 밝게 고정한다.
        if app is not None and hasattr(app.styleHints(), 'setColorScheme'):     # Qt 6.8 이상
            app.styleHints().setColorScheme(QtCore.Qt.ColorScheme.Light)
        self.setStyleSheet(QSS)
        if app is not None:
            tooltips.install(app)           # 풍선 도움말(커서를 잠시 대면 기능 설명, 09-24)
        self._build()
        self._build_shortcuts()
        self.go(1)
        self._dirty = False                 # 만드는 동안의 갱신은 변경이 아니다
        self._build_busy()

    # ──────────────────────────── 작업 중 표시 ────────────────────────────
    def _build_busy(self) -> None:
        """계산·측정이 도는 동안 보이는 표시 둘.

        - 로딩 오버레이(`secs.wizard.loading`, 09-23 사용자 요청): 화면을 덮는 긴 일 — 5단계 mdat·샘플
          불러오기. 300 ms 넘게 돌 때만 보인다.
        - 구석 패널(`secs.wizard.busy`, 작은 로고 방울 포함): 화면을 봐야 하는 작업 — 5단계 측정,
          보정 설계 계산·MIMO·XO 추천, 파일 쓰기(그래프를 가리면 안 된다).
        - 단계 안의 작은 방울(`loading.BusyDrop`, 09-24): 1단계 REW 확인·장치 목록 읽기, 5단계 스윕.
        셋 다 300 ms 넘게 도는 일에만 보인다(`loading.SHOW_DELAY_MS`).
        """
        from secs.wizard.busy import BusyPanel
        from secs.wizard.loading import LoadingOverlay
        from secs.wizard.step4_layout import position_seconds

        self.busy_panel = BusyPanel(self.stack)
        self.loading = LoadingOverlay(self.stack)

        def measure_estimate() -> float:
            # 4 · 5단계 '이 위치' 와 같은 값(반복 · 시작 지연 포함 — 10-10 코드 리뷰: 반복 4 에서 191 s vs 48 s)
            return position_seconds(self.project)

        #: (문구, 돌고 있나, 예상 초, 취소) — 오버레이로 가리는 작업.
        self._overlay_jobs = [
            ('mdat 를 불러오는 중…', lambda: self.step5.loading_mdat(), lambda: 0.0, None),
            ('샘플 데이터를 불러오는 중…', lambda: self.step5.loading_sample(), lambda: 0.0, None),
        ]
        page = self.calibration_page
        self._busy_jobs = [
            ('측정 중…', lambda: self.step5._thread is not None, measure_estimate),
            ('보정 설계 계산 중…', page.running, page.compute_total_seconds),
            ('BA 계산 중…', page.subs_running, page.subs_estimate_seconds),
            ('MIMO 계산 중…', page.mimo_running, page.mimo_estimate_seconds),
            ('XO 추천 계산 중…', page.xo_running, page.xo_estimate_seconds),
            ('파일을 내보내는 중…', page.exporting, lambda: 0.0),
            ('파일을 내보내는 중…', self.step8.exporting, lambda: 0.0),
        ]
        #: 단계 안의 작은 방울(09-24) — 화면을 덮지 않는 대기. 각 화면의 `poll_busy()` 가
        #: `working()` 을 보고 300 ms 규칙으로 켜고 끈다(1단계 REW 확인, 5단계 스윕·카운트다운).
        self._drop_pages = [self.devices_page, self.measure_page]
        self._busy_timer = QtCore.QTimer(self)
        self._busy_timer.setInterval(250)
        self._busy_timer.timeout.connect(self._poll_busy)
        self._busy_timer.start()

    def _poll_busy(self) -> None:
        self._refresh_auto()
        from secs.wizard.busy import poll

        for page in self._drop_pages:
            try:
                page.poll_busy()
            except Exception:  # noqa: BLE001 - 표시 실패가 점검을 막지 않게
                pass
        # 화면을 덮는 로딩창은 그 일을 하는 단계를 보고 있을 때만(09-25 사용자: "측정 탭에서 갑자기 저역 통합 연산을 하네") —
        # 자동 보정이 뒤에서 도는 동안에는 사이드바 '자동 보정' 과 구석 표시만.
        for (what, running, estimate, cancel), page in zip(self._overlay_jobs, OVERLAY_PAGES):
            if page != self.current_step:
                continue
            try:
                active = bool(running())
            except Exception:  # noqa: BLE001 - 상태 조회 실패는 '안 돎'
                active = False
            if active:
                try:
                    guess = float(estimate() or 0.0)
                except Exception:  # noqa: BLE001
                    guess = 0.0
                self.loading.want(what, guess, cancel)
                self.busy_panel.stop()
                return
        self.loading.clear()
        poll(self.busy_panel, self._busy_jobs)

    #: 단계 페이지 안쪽 여백(좌+우) — `_scrolled` 의 setContentsMargins(28, …, 28, …).
    PAGE_PAD = 56

    def _fit_side_columns(self) -> int:
        """모든 단계의 오른쪽 칸 폭을 지금 창 폭에 맞춘다(`style.set_side_width`). 맞춘 폭을 돌려준다."""
        from secs.wizard.style import set_side_width

        area = self.stack.widget(0)
        bar = area.verticalScrollBar().sizeHint().width() if area is not None else 0
        return set_side_width(self.stack.width() - bar - self.PAGE_PAD)

    def eventFilter(self, watched, event) -> bool:  # noqa: N802 - Qt 이름
        if watched is getattr(self, 'stack', None) and event.type() == QtCore.QEvent.Type.Resize:
            self._fit_side_columns()
        return super().eventFilter(watched, event)

    def resizeEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().resizeEvent(event)
        panel = getattr(self, 'busy_panel', None)
        if panel is not None and panel.isVisible():
            panel.place()

    # ──────────────────────────── 뼈대 ────────────────────────────
    def _build(self) -> None:
        root = QtWidgets.QWidget()
        root.setObjectName('wizardRoot')
        self.setCentralWidget(root)
        row = QtWidgets.QHBoxLayout(root)
        row.setContentsMargins(0, 0, 0, 0)
        row.setSpacing(0)
        row.addWidget(self._sidebar())

        main = QtWidgets.QVBoxLayout()
        main.setContentsMargins(0, 0, 0, 0)
        main.setSpacing(0)
        self.steps_bar = _StepsBar()
        self.steps_bar.stepClicked.connect(lambda position: self.go(FLOW[position - 1]))
        main.addWidget(self.steps_bar)

        self.stack = QtWidgets.QStackedWidget()
        self.stack.installEventFilter(self)       # 크기가 바뀌면 오른쪽 칸 폭을 다시 정한다
        cache = self.rew_cache
        self.devices_page = Step1Devices(self.project, rew=cache, devices=cache)
        self.config_page = Step2Config(self.project, devices=cache)
        self.step3 = Step3Level(self.project, levels=self.levels, devices=cache)
        self.step4 = Step4Layout(self.project, devices=cache)
        self.step5 = Step5Measure(self.project, measurer=self.measurer,
                                  project_path=self.project_path, devices=cache)
        self.calibration_page = StepCalibration(self.project)
        self.step8 = Step8Mdat(self.project, calibration=self.calibration_page)
        # 3·4·5·8단계는 번호 이름을 그대로 두고 뜻 이름을 더한다(테스트·도구가 번호 이름을 많이 쓴다).
        self.level_page, self.layout_page, self.measure_page = self.step3, self.step4, self.step5
        self.export_page = self.step8
        #: 화면 번호 순서의 단계 위젯(번호 → 위젯은 `step_widget()`·`_page_of`).
        self.steps: list[QtWidgets.QWidget] = [getattr(self, name) for name in PAGE_NAMES.values()]
        self._page_of: dict[int, QtWidgets.QWidget] = {number: getattr(self, name)
                                                       for number, name in PAGE_NAMES.items()}
        self._stack_index: dict[int, int] = {}
        for number, step in self._page_of.items():
            step.changed.connect(self._on_step_changed)
            self._stack_index[number] = self.stack.addWidget(self._scrolled(step))
        # 자동 보정(개정 9, 09-26 새 엔진): 측정이 바뀌면 보정 설계를 뒤에서 기본값으로 계산한다.
        # 테스트는 계산 시점을 직접 정한다 — 자동 보정은 켜 둔 검사(`test_user_feedback_0925`)만 본다.
        # 10-09 사용자: "계산을 눌러야만 실시돼야" — 기본은 끈다(측정 · 단계 이동으로 저절로 돌지 않는다).
        self.auto_calibrate = False
        self._auto_state = ''
        self._auto_timer = QtCore.QTimer(self)
        self._auto_timer.setSingleShot(True)
        self._auto_timer.timeout.connect(self._auto_calibrate)
        self.step5.changed.connect(self._auto_calibrate_soon)
        self.calibration_page.computed.connect(self._refresh_auto)
        # 내보내기 화면이 보고 있는 동안 보정 계산이 끝나거나(자동 보정) 시작·취소되면 바로 다시 그린다.
        self.calibration_page.changed.connect(self.step8.refresh)
        self.calibration_page.computed.connect(self.step8.refresh)
        self._set_calibration_context()
        main.addWidget(self.stack, 1)
        main.addWidget(self._nav())
        row.addLayout(main, 1)

    def _build_shortcuts(self) -> None:
        """키보드만으로 다니기(P6): Alt+←/→ 단계 이동, Ctrl+1~7 바로 가기(진행바 순서), Ctrl+S/O 저장·열기."""
        def bind(keys: str, slot) -> None:
            shortcut = QtGui.QShortcut(QtGui.QKeySequence(keys), self)
            shortcut.setContext(QtCore.Qt.ShortcutContext.WindowShortcut)
            shortcut.activated.connect(slot)

        bind('Alt+Left', self.back)
        bind('Alt+Right', self.advance)
        bind('Ctrl+S', lambda: self.quick_save())
        bind('Ctrl+Shift+S', lambda: self.save_as())
        bind('Ctrl+O', lambda: self.load_project())
        bind('F1', lambda: self.show_help())
        bind('Ctrl+M', lambda: self.open_mic_meter())
        for number in range(1, len(STEPS) + 1):
            bind(f'Ctrl+{number}', lambda n=number: self.go(FLOW[n - 1]))

    # ──────────────────────────── REW 백그라운드 점검 ────────────────────────────
    def start_rew_watch(self) -> bool:
        """창이 뜬 뒤 REW 연결을 백그라운드에서 본다. 상태가 바뀌면 화면을 다시 채운다.

        창을 빨리 띄우려고 `run_wizard` 가 만들 때는 REW 를 기다리지 않게 해 두었다
        (`RealREW.hold_offline`). 여기서 처음 실제로 물어보고, 그 뒤로 몇 초마다 본다.
        `probe()` 가 없는 백엔드(FakeREW)는 점검할 것이 없다. 읽은 값은 `self.rew_cache` 에 들어가고,
        바뀌면 `_on_rew_updated` 가 화면 스레드에서 다시 그린다.
        """
        from secs.wizard.rew_cache import is_remote

        if self._rew_watch_thread is not None or not is_remote(self.rew):
            return False

        def watch() -> None:
            while not self._rew_watch_stop.is_set():
                # REW 가 꺼져 있다고 아는 동안에는 방해할 통신이 없으니 어느 단계에서든 본다.
                if self._rew_running is not True or self.current_step not in self.REW_WATCH_SKIP_STEPS:
                    try:
                        self.rew_cache.watch_tick()
                    except Exception:  # noqa: BLE001 - 점검 실패로 점검 스레드가 죽지 않게
                        pass
                self._rew_watch_stop.wait(self.REW_WATCH_S)

        self._rew_watch_thread = threading.Thread(target=watch, name='secs-rew-watch', daemon=True)
        self._rew_watch_thread.start()
        return True

    #: REW 값(연결·장치·채널 이름)을 화면에 보이는 단계 — 캐시가 바뀌면 지금 이 단계면 다시 그린다.
    REW_VIEW_STEPS = (1, 2, 3, 4)

    @QtCore.pyqtSlot(object)
    def _on_rew_updated(self, snap) -> None:
        """캐시가 새 값을 받았다(화면 스레드). 보이는 단계만 다시 그린다 — 다른 단계는 들어갈 때 그린다."""
        self._rew_running = snap.running
        widget = self.step_widget()
        if self.current_step in self.REW_VIEW_STEPS and widget is not None and hasattr(widget, 'refresh'):
            widget.refresh()
        self._refresh_nav()
        self._refresh_sidebar()

    def _scrolled(self, widget: QtWidgets.QWidget) -> QtWidgets.QScrollArea:
        area = QtWidgets.QScrollArea()
        area.setWidgetResizable(True)
        area.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        # 세로 스크롤 막대 자리를 늘 둔다 — 단계마다 막대가 생겼다 없어지면 본문 폭이 12 px 씩
        # 달라져 오른쪽 칸이 흔들렸다(09-23 사용자: 단계마다 좌우 크기가 달라진다).
        area.setVerticalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAlwaysOn)
        holder = QtWidgets.QWidget()
        holder.setObjectName('page')
        box = QtWidgets.QVBoxLayout(holder)
        box.setContentsMargins(28, 22, 28, 22)
        box.addWidget(widget)
        area.setWidget(holder)
        return area

    # ──────────────────────────── 프로젝트 경로 ────────────────────────────
    def current_project_path(self) -> Path | None:
        """지금 쓰는 `.secs` 경로.

        5단계가 저장 위치를 묻지 않고 만들 수 있어서, 창이 들고 있는 경로보다
        5단계가 실제로 쓴 경로가 최신일 수 있다. IR wav 경로는 이 파일 기준의
        상대 경로라 보정 설계도 같은 값을 써야 한다.
        """
        if self.project_path is None and self.step5.project_path is not None:
            self.project_path = self.step5.project_path
        return self.project_path

    # ──────────────────────────── 자동 보정(개정 9) ────────────────────────────
    def _auto_calibrate_soon(self) -> None:
        """5단계 측정이 바뀌면(측정·mdat·샘플) 잠시 뒤 보정 설계를 기본값으로 뒤에서 계산한다 — 09-25 사용자: 측정만
        하면 필터 설계·XO 까지 알아서 해 주기를 바랐다. 측정이 도는 중이면 기다린다. 10-09 부터 기본은 꺼짐(`auto_calibrate`)."""
        if getattr(self, 'trial', False):       # 튜토리얼: 뒤에서 미리 계산해 두어 [계산] 은 짧게(10-10 사용자)
            QtCore.QTimer.singleShot(1500, self._prepare_tutorial_calibration)
        if not getattr(self, 'auto_calibrate', True):
            return
        self._auto_timer.start(1200)

    def _prepare_tutorial_calibration(self) -> None:
        if not getattr(self, 'trial', False) or self.step5._thread is not None:
            return
        ok, _message = self.step5.can_advance()
        if ok:
            self._set_calibration_context()
            self.calibration_page.prepare_in_background()

    def _auto_calibrate(self) -> None:
        if self.step5._thread is not None:
            self._auto_timer.start(1200)
            return
        ok, _message = self.step5.can_advance()
        if not ok:
            return
        # 위치를 다 재기 전에는 돌리지 않는다 — 위치 1 만 재고 보정 설계가 먼저 돌던 문제(10-09 사용자 지적).
        if self.step5.current_position() != -1:
            return
        page = self.calibration_page
        if page.working():                          # 이전 계산이 아직 돌면 끝난 뒤 다시(측정이 또 바뀌었을 수 있다)
            self._auto_timer.start(1200)
            return
        self._set_calibration_context()
        if page.compute():
            self._auto_state = 'calibration'
            self._refresh_auto()

    def _refresh_auto(self) -> None:
        state = getattr(self, '_auto_state', '')
        page = self.calibration_page
        if state == 'calibration' and not page.running():
            state = self._auto_state = 'done' if page.run is not None else ''
        if not state and page.run is not None and not page.working() and not page.stale():
            state = 'done'          # [계산] 을 직접 눌러 끝낸 것도(10-10 튜토리얼 AI 시험: 계산 뒤에도 '—')
        text = {'calibration': t('보정 설계 계산 중…'),
                'done': t('완료 — 내보내기로 바로 가도 됩니다')}.get(state, '—')
        if self.side_auto.text() != text:
            self.side_auto.setText(text)

    def _set_calibration_context(self) -> None:
        """보정 설계 화면에 프로젝트 경로와 IR 공급 함수를 넘긴다. 공급 함수는 워커 스레드에서 불린다 —
        창의 IR 캐시(`_ir_cache`)를 나눠 쓴다."""
        def provider():
            from secs.rewapi.session import load_project_irs
            return load_project_irs(self.project, self.current_project_path(), cache=self._ir_cache)

        self.calibration_page.set_context(self.current_project_path(), provider)

    def _sidebar(self) -> QtWidgets.QFrame:
        side = QtWidgets.QFrame()
        side.setObjectName('side')
        self.side_frame = side
        side.setFixedWidth(232)
        box = QtWidgets.QVBoxLayout(side)
        box.setContentsMargins(18, 22, 18, 18)
        box.setSpacing(self.SIDE_GAP_MIN)           # 칸 사이 나머지는 줄어드는 틈 — 맨 끝 `_shrinkable_gaps`
        # 마크 + 이름. 마크 바탕색이 사이드바와 같아 도형(흰 커튼 · 청록 막대)만 떠 보인다.
        brand_row = QtWidgets.QHBoxLayout()
        brand_row.setSpacing(10)
        mark = mark_pixmap(34, self.devicePixelRatioF())
        if mark is not None:
            mark_label = QtWidgets.QLabel()
            mark_label.setObjectName('brandMark')
            mark_label.setPixmap(mark)
            mark_label.setFixedSize(34, 34)
            brand_row.addWidget(mark_label)
        brand_row.addWidget(label(APP_NAME, 'brand'))
        brand_row.addStretch(1)
        box.addLayout(brand_row)
        box.addWidget(label('CALIBRATE', 'brandSub'))
        self.side_version = label(VERSION_LABEL, 'sideValue')      # 예: 2.0 Alpha 1.0.0 — version.py 에서 온다
        self.side_version.setToolTip(version_text())
        box.addWidget(self.side_version)

        self.side_layout = label('—', 'sideValue', wrap=True)
        self.side_meas = label('—', 'sideValue')
        self.side_auto = label('—', 'sideValue', wrap=True)
        tooltips.attach(self.side_auto, 'win.auto')
        self.side_rew = label('—', 'sideValue')
        self.side_file = label('—', 'sideValue', wrap=True)
        # 칸 이름 '자동 보정' → '보정 설계'(진행바 단계 이름과 같게 — 10-10 3차 시험 사용자 결정 4A)
        for title, widget in (('구성', self.side_layout), ('측정', self.side_meas), ('보정 설계', self.side_auto),
                              ('REW', self.side_rew), ('프로젝트', self.side_file)):
            rule = QtWidgets.QFrame()
            rule.setObjectName('sideRule')
            box.addWidget(rule)
            box.addWidget(label(title, 'sideTitle'))
            box.addWidget(widget)
        box.addStretch(1)

        save = QtWidgets.QPushButton(t('프로젝트 저장'))
        # [프로젝트 저장] = 지금 파일에 바로 저장(10-09 사용자: 누르면 매번 다른 이름으로 저장 창이 떴다), 옆 ▾ = 다른 이름으로
        save.clicked.connect(lambda: self.quick_save())       # clicked(bool) 이 path 로 들어가면 안 된다(09-22 버그)
        self.save_button = save
        save_more = QtWidgets.QToolButton()
        save_more.setText('▾')
        save_more.setPopupMode(QtWidgets.QToolButton.ToolButtonPopupMode.InstantPopup)
        save_menu = QtWidgets.QMenu(save_more)
        self.save_as_action = save_menu.addAction(t('다른 이름으로 저장…'))
        self.save_as_action.triggered.connect(lambda: self.save_as())
        save_more.setMenu(save_menu)
        tooltips.attach(save_more, 'win.save_more')
        self.save_more_button = save_more
        load = QtWidgets.QPushButton(t('프로젝트 불러오기'))
        load.clicked.connect(lambda: self.load_project())
        save.setToolTip('프로젝트 저장 (Ctrl+S)')
        load.setToolTip('프로젝트 불러오기 (Ctrl+O)')
        help_button = QtWidgets.QPushButton(t('도움말'))
        help_button.setToolTip('이 단계 따라 하기 (F1)')
        help_button.clicked.connect(lambda: self.show_help())
        tooltips.attach(help_button, 'win.help')
        tooltips.attach(save, 'win.save')
        tooltips.attach(load, 'win.load')
        self.help_button = help_button
        box.addWidget(help_button)
        # 마이크 미터 버튼은 뺐다(10-09 사용자 — Ctrl+M 은 남김). 그 자리에 튜토리얼: 샘플 데이터로 1→8 을 따로 띄운다.
        trial_button = QtWidgets.QPushButton(t('튜토리얼'))
        trial_button.clicked.connect(lambda: self.open_trial_mode())
        tooltips.attach(trial_button, 'win.trial')
        self.trial_button = trial_button
        box.addWidget(trial_button)
        # 튜토리얼 중에만 — 퀘스트를 처음부터 다시(10-09)
        self.tutorial_restart_button = QtWidgets.QPushButton(t('튜토리얼 다시'))
        self.tutorial_restart_button.clicked.connect(lambda: self.restart_tutorial())
        tooltips.attach(self.tutorial_restart_button, 'win.tutorial_restart')
        self.tutorial_restart_button.hide()
        box.addWidget(self.tutorial_restart_button)
        save_row = QtWidgets.QHBoxLayout()
        save_row.setSpacing(4)
        save_row.addWidget(save, 1)
        save_row.addWidget(save_more)
        box.addLayout(save_row)
        box.addWidget(load)

        # 언어 선택 — 고르면 저장되고 열려 있는 화면도 바로 바뀐다. 영어 화면은 10-11 부터 임시로 닫았다
        # (`i18n.ENGLISH_UI`) — 그동안은 칸을 만들지 않는다.
        if ENGLISH_UI:
            language_row = QtWidgets.QHBoxLayout()
            language_row.setSpacing(8)
            add_language_selector(self, language_row)
            tooltips.attach(self.language_combo, 'win.language')
            language_row.setStretch(1, 1)
            box.addLayout(language_row)
        self._shrinkable_gaps(box)
        language_manager().language_changed.connect(self._on_language_changed)
        return side

    trial = False
    trial_return_path: Path | None = None

    def set_trial(self, on: bool, return_path=None) -> None:
        """튜토리얼 표시(10-09 사용자: 창 하나로, 체험인 걸 확실히) — 노란 띠 · 창 제목 앞 [튜토리얼] · 사이드바 버튼이
        [튜토리얼 끝내기] 로. 튜토리얼 중에는 저장 · 내보내기를 막는다(`save_project` · 보정 설계 `export`)."""
        self.trial = bool(on)
        self.trial_return_path = Path(return_path) if return_path else None
        # 띠 대신 키 색을 분홍으로(10-09 사용자: 위 띠가 화면을 줄여 불편) — 사이드바 · 진행바 · 강조 버튼이 모두 분홍
        from secs.wizard.style import QSS, TRIAL_QSS
        self.setStyleSheet(TRIAL_QSS if self.trial else QSS)
        self.trial_button.setText(t('튜토리얼 끝내기' if self.trial else '튜토리얼'))
        tooltips.attach(self.trial_button, 'win.trial_end' if self.trial else 'win.trial')
        self.calibration_page.trial = self.trial
        self.level_page.trial = self.trial
        self.step5.set_trial(self.trial)
        self.tutorial_restart_button.setVisible(self.trial)
        # 게임식 튜토리얼(10-09 사용자: 체험 + 도움말을 합친 것) — 튜토리얼 모드가 켜지면 퀘스트를 시작한다
        if self.trial:
            from secs.wizard.tutorial_overlay import Tutorial
            if getattr(self, 'tutorial', None) is None:
                self.tutorial = Tutorial(self)
            from secs.wizard.tutorial_quests import plant_wrong_out
            plant_wrong_out(self)                # 출력 퀘스트가 고칠 틀린 출력(SW3 16, 10-11)
            QtCore.QTimer.singleShot(0, self.tutorial.start)
        elif getattr(self, 'tutorial', None) is not None:
            self.tutorial.stop()
        self._refresh_title()
        self._refresh_sidebar()

    def restart_tutorial(self) -> None:
        if getattr(self, 'tutorial', None) is not None:
            self.go(1)
            from secs.wizard.tutorial_quests import plant_wrong_out
            plant_wrong_out(self)
            self.tutorial.start()

    def _refresh_title(self) -> None:
        head = f"[{t('튜토리얼')}] " if self.trial else ''
        self.setWindowTitle(f'{head}{version_text()} · {t("측정 위저드")}')

    def _swap_to(self, project, fake: bool, project_path, trial: bool, return_path=None):
        """같은 자리에 새 창을 세우고 이 창을 닫는다(백엔드를 실행 중에 바꿔 끼우지 않고 창째 바꾼다 — 실제 REW 와 가짜
        REW 가 섞일 틈이 없다). 사용자에게는 한 창이 체험으로 바뀌는 것으로 보인다."""
        from secs.wizard import build_backend
        rew, devices, levels, measurer = build_backend(fake=fake, use_sample_dolby=fake)
        win = WizardWindow(project, rew=rew, devices=devices, levels=levels, measurer=measurer,
                           project_path=str(project_path) if project_path else None)
        win.ask_before_close = not trial
        win.set_trial(trial, return_path)
        win.setGeometry(self.geometry())
        maximized = self.isMaximized()
        # 작업 표시줄 로고(10-11 사용자: 튜토리얼을 켜면 작업 표시줄 로고가 Windows 기본 아이콘으로 바뀜): 새 창을 띄우기 전에
        # 이 창을 먼저 숨겨 두 창이 한 묶음 단추가 되는 순간을 없애고, 첫 창처럼 exe 아이콘을 직접 붙인다(바꿔 세운 창에는
        # `_pin_taskbar_icon` 이 빠져 있었다). 작업 표시줄이 단추를 다 만든 뒤 한 번 더 붙인다.
        self.hide()
        if maximized:
            win.showMaximized()
        else:
            win.show()
        from secs.wizard import _pin_taskbar_icon
        _pin_taskbar_icon(win)
        QtCore.QTimer.singleShot(400, lambda w=win: _pin_taskbar_icon(w))
        app = QtWidgets.QApplication.instance()
        if app is not None:
            app._secs_window = win               # 새 창이 지워지지 않게 붙잡아 둔다
        win.start_rew_watch()
        self.ask_before_close = False
        self.close()
        return win

    def open_trial_mode(self):
        """[튜토리얼](10-09 사용자: 별도 창 말고 지금 창을 체험으로) — 저장 안 한 변경이 있으면 먼저 묻고, 같은 창 자리에서
        샘플 데이터 · 데모 REW 체험으로 바뀐다. 튜토리얼 중이면 [튜토리얼 끝내기] 와 같다."""
        if self.trial:
            return self.end_trial_mode()
        if not self._confirm_close():
            return None
        from secs.wizard import new_project
        return self._swap_to(new_project(True), True, None, True, self.current_project_path())

    def end_trial_mode(self):
        """[튜토리얼 끝내기] — 실제 REW 로, 체험 전에 열려 있던 프로젝트(있으면)로 돌아간다."""
        from secs.wizard import new_project, open_project
        path = self.trial_return_path
        ok = path is not None and path.exists()
        return self._swap_to(open_project(path) if ok else new_project(False), False, path if ok else None, False)

    def open_mic_meter(self) -> None:
        """마이크 입력 미터 창(`mic_meter`)을 연다 — 측정 없이 마이크가 들어오는지 본다(10-07). 하나만 띄운다."""
        from secs.wizard.mic_meter import MicMeterWindow
        if getattr(self, '_mic_meter', None) is None:
            self._mic_meter = MicMeterWindow(self.levels, parent=self)
        self._mic_meter.show()
        self._mic_meter.raise_()
        self._mic_meter.activateWindow()

    #: 사이드바 칸 사이 간격(px): 넉넉하면 SIDE_GAP, 창이 낮으면 SIDE_GAP_MIN 까지 줄어든다.
    SIDE_GAP = 14
    SIDE_GAP_MIN = 4

    def _shrinkable_gaps(self, box: QtWidgets.QVBoxLayout) -> None:
        """칸 사이마다 (SIDE_GAP − SIDE_GAP_MIN) px 의 줄어드는 틈을 넣는다. 고정 간격 14 px 로는 사이드바 최소 높이가
        707 px 라 창을 1024×640 으로 줄일 수 없었다(10-05 요구 대장 R5 — 캡처가 전부 1024×707). 넉넉한 창에서는
        전과 같은 14 px, 낮은 창에서는 틈만 줄어든다. 늘어나는 빈칸(addStretch) 뒤에는 넣지 않는다(앞에 하나면 된다 —
        앞에도 없으면 낮은 창에서 프로젝트 이름이 [도움말] 버튼에 4 px 로 붙었다)."""
        for index in range(box.count() - 1, 0, -1):
            if box.itemAt(index - 1).spacerItem() is None:
                box.insertSpacerItem(index, QtWidgets.QSpacerItem(
                    0, self.SIDE_GAP - self.SIDE_GAP_MIN,
                    QtWidgets.QSizePolicy.Policy.Minimum, QtWidgets.QSizePolicy.Policy.Preferred))

    def _on_language_changed(self, _language: str) -> None:
        """언어 변경 뒤 창 제목, 모든 단계의 표시, 내비와 사이드바를 갱신한다.
        번역 위젯의 저장된 문구는 언어 관리자가 별도로 다시 번역한다.
        """
        self._refresh_title()
        if self.trial:
            self.trial_button.setText(t('튜토리얼 끝내기'))
        for step in self.steps:
            if hasattr(step, 'refresh'):
                step.refresh()
        self._refresh_nav()
        self._refresh_sidebar()
        self._sync_help()

    def _nav(self) -> QtWidgets.QFrame:
        nav = QtWidgets.QFrame()
        nav.setObjectName('nav')
        nav.setFixedHeight(56)
        box = QtWidgets.QHBoxLayout(nav)
        box.setContentsMargins(28, 10, 28, 10)
        box.setSpacing(12)
        self.prev_button = QtWidgets.QPushButton(t('← 이전'))
        set_variant(self.prev_button, 'ghost')
        self.prev_button.clicked.connect(lambda: self.back())
        self.nav_message = label('', 'navMid')
        self.nav_message.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        self.skip_button = QtWidgets.QPushButton('')
        set_variant(self.skip_button, 'skip')
        self.skip_button.clicked.connect(self.skip)
        self.next_button = QtWidgets.QPushButton(t('다음 →'))
        self.next_button.clicked.connect(self.advance)
        tooltips.attach(self.prev_button, 'win.prev')
        tooltips.attach(self.next_button, 'win.next')
        self.prev_button.setToolTip('이전 단계 (Alt+←)')
        self.next_button.setToolTip('다음 단계 (Alt+→) · 단계 바로 가기 Ctrl+1~7')
        box.addWidget(self.prev_button)
        box.addWidget(self.nav_message, 1)
        box.addWidget(self.skip_button)
        box.addWidget(self.next_button)
        return nav

    # ──────────────────────────── 도움말 ────────────────────────────
    def show_help(self) -> None:
        """현재 단계의 도움말 창(비모달). 이미 열려 있으면 앞으로 가져온다."""
        from secs.wizard.help_dialog import HelpDialog

        if getattr(self, '_help_dialog', None) is None:
            self._help_dialog = HelpDialog(self)
        self._help_dialog.set_step(self.current_step)
        self._help_dialog.show()
        self._help_dialog.raise_()
        self._help_dialog.activateWindow()

    def _sync_help(self) -> None:
        dialog = getattr(self, '_help_dialog', None)
        if dialog is not None and dialog.isVisible():
            dialog.set_step(self.current_step)

    # ──────────────────────────── 이동 ────────────────────────────
    def go(self, step: int) -> None:
        """화면 이동(목업 go()). 화면 번호 1~5 · 내보내기 8 · 보정 설계 `CAL_STEP`(9)만 받고 그 밖은 ValueError
        (6·7·10 은 2026-10-10 에 지운 옛 화면)."""
        step = int(step)
        if step not in PAGE_NAMES:
            raise ValueError(f'단계 번호는 {sorted(PAGE_NAMES)} 중 하나여야 합니다: {step}')
        if self.current_step == 3 and step != 3:
            self.step3.stop_monitor()
        # 지난번에 이 단계로 오는 데 300 ms 넘게 걸렸으면 로딩 표시를 먼저 그린다 — 이동 자체는
        # 화면 스레드 일이라 그동안 그려지지 않으므로 미리 그려 둔다(09-23 사용자: 로딩창).
        slow = getattr(self, 'loading', None) is not None and self._go_ms.get(step, 0.0) > 300.0
        if slow:
            self.loading.show_now('화면을 준비하는 중…')
        started = time.perf_counter()
        self.current_step = step
        self.stack.setCurrentIndex(self._stack_index[self.current_step])
        if self.current_step == 8:
            # 내보내기는 프로젝트 파일 위치(내보낼 폴더 기본값)만 받는다 — 결과는 보정 설계 화면에서 읽는다.
            self.step8.set_context(self.current_project_path())
        if self.current_step == CAL_STEP:
            self._set_calibration_context()
            if self.auto_calibrate:             # 결과가 없거나 낡았으면 들어올 때 계산(테스트는 직접 부른다)
                self.calibration_page.compute_if_needed()
        widget = self.step_widget()
        if widget is not None and hasattr(widget, 'refresh'):
            widget.refresh()
        if self.current_step == 3:
            self.step3.start_monitor()
        if self.current_step in self.REW_VIEW_STEPS:
            # REW 값이 오래됐으면 뒤에서 새로 읽는다(기다리지 않는다) — 끝나면 `_on_rew_updated`.
            self.rew_cache.refresh_if_stale()
        self.steps_bar.set_current(self.current_step)
        self._refresh_nav()
        self._refresh_sidebar()
        self._sync_help()
        self._go_ms[step] = (time.perf_counter() - started) * 1000.0
        if slow:
            self.loading.clear()

    def step_widget(self):
        """현재 단계의 위젯(모르는 번호면 None)."""
        return self._page_of.get(self.current_step)

    def can_advance(self) -> tuple[bool, str]:
        """현재 단계에서 '다음' 을 눌러도 되는지와 그 이유."""
        widget = self.step_widget()
        if widget is None:
            return False, ''
        if hasattr(widget, 'can_advance'):
            return widget.can_advance()
        return True, ''

    def can_advance_step(self, step: int) -> tuple[bool, str]:
        """테스트 전용 임의 단계 진행 조건 조회. 현재 단계는 바꾸지 않는다.
        화면 내비는 인자 없는 can_advance()를 사용한다.
        """
        widget = self._page_of.get(int(step))
        if widget is None:
            return False, ''
        if hasattr(widget, 'can_advance'):
            return widget.can_advance()
        return True, ''

    def next_page(self, step: int | None = None) -> int | None:
        """[다음] 이 갈 화면 번호(마지막이면 None) — 흐름(`FLOW`)의 다음 칸."""
        step = self.current_step if step is None else int(step)
        index = FLOW.index(step)
        return FLOW[index + 1] if index + 1 < len(FLOW) else None

    def previous_page(self, step: int | None = None) -> int:
        step = self.current_step if step is None else int(step)
        return FLOW[max(0, FLOW.index(step) - 1)]

    def advance(self) -> bool:
        target = self.next_page()
        if target is None:
            return False
        ok, message = self.can_advance()
        if not ok:
            self.nav_message.setText(message)
            return False
        self.go(target)
        return True

    def back(self) -> None:
        self.go(self.previous_page())

    def skip(self) -> None:
        """목업 #skip — 단계별 건너뛰기."""
        if self.current_step == 1:
            self.nav_message.setText('장치 확인을 건너뛰었습니다. REW 연결은 실제 측정에서 확인됩니다.')
            self.go(2)
        elif self.current_step == 3:
            self.step3.apply_defaults()
            self.nav_message.setText('모든 채널을 레퍼런스 구간(-30~-15 dBFS)으로 간주합니다.')
            self.go(4)
        elif self.current_step == 5:
            self.step5.load_mdat()          # 측정 대신 mdat(10-09 사용자: 샘플은 튜토리얼로만)

    # ──────────────────────────── 갱신 ────────────────────────────
    def _on_step_changed(self) -> None:
        self._dirty = True                 # 마지막 저장 뒤 바뀐 것이 있다 → 닫을 때 묻는다(09-22)
        self._refresh_nav()
        self._refresh_sidebar()

    def _refresh_nav(self) -> None:
        self.prev_button.setEnabled(self.current_step != FLOW[0])
        # 마지막 단계의 '완료' 는 영어로 'Finish' — 목록의 '완료'(Done) 와 낱말이 같아 여기서 가른다.
        finish = 'Finish' if language_manager().language == 'en' else '완료'
        self.next_button.setText(finish if self.next_page() is None else t('다음 →'))
        skip_text = SKIP_LABELS.get(self.current_step, '')
        self.skip_button.setText(skip_text)
        if skip_text:
            tooltips.attach(self.skip_button, f'win.skip{self.current_step}')
        self.skip_button.setVisible(bool(skip_text))
        ok, message = self.can_advance()
        self.next_button.setEnabled(ok)
        self.nav_message.setText('' if ok else message)

    def _refresh_sidebar(self) -> None:
        config = self.project.config
        outs = sum(len(s.bands) for s in config.speakers)
        preset = '사용자 정의' if config.preset == 'custom' else config.preset
        self.side_layout.setText(f'{preset} · 서브 {config.sub_count()} · 출력 {outs}')
        total = int(self.project.extra.get('array', 9) or 9)
        self.side_meas.setText(f'{len(self.step5.done_positions())} / {total} 위치')
        running = self.rew_cache.is_running()     # 캐시 값 — 화면은 REW 를 기다리지 않는다
        self.side_rew.setText('API 연결 (4735)' if running else t('미연결'))
        # 5단계가 저장 위치를 묻지 않고 만들므로, 어디에 만들어졌는지는 보여 줘야 한다.
        path = self.current_project_path()
        if self.trial:          # 튜토리얼 파일은 임시 폴더 — 저장된 것처럼 보이지 않게(10-10 AI 시험 2차)
            self.side_file.setText(t('튜토리얼 · 저장 안 됨'))
        else:
            self.side_file.setText(path.name if path else t('저장 안 됨'))
        self.side_file.setToolTip(str(path) if path else '')

    # ──────────────────────────── 파일 ────────────────────────────
    def quick_save(self) -> Path | None:
        """[프로젝트 저장] · Ctrl+S — 지금 프로젝트 파일이 있으면 묻지 않고 그 파일에, 없으면(처음 저장) 위치를 묻는다."""
        path = self.current_project_path()
        return self.save_project(str(path) if path else None)

    def save_as(self) -> Path | None:
        """▾ → [다른 이름으로 저장…] · Ctrl+Shift+S — 늘 위치를 묻는다."""
        return self.save_project()

    def save_project(self, path: str | None = None) -> Path | None:
        if self.trial:                          # 튜토리얼 데이터는 저장하지 않는다(10-09)
            self.nav_message.setText(t('튜토리얼에서는 저장하지 않습니다 — [튜토리얼 끝내기] 로 돌아가세요'))
            return None
        if path is None:
            path, _ = QtWidgets.QFileDialog.getSaveFileName(
                self, t('프로젝트 저장'),
                str(self.current_project_path() or '측정.secs'),
                'SECS 프로젝트 (*.secs)')
        if not path:
            return None
        target = Path(path)
        self._carry_irs(self.current_project_path(), target)
        self._spl_offsets()                     # REW SPL 오프셋을 기록에 채워 둔 뒤 저장(mdat 레벨 — 10-09)
        self.project.save(target)
        self._write_side_mdat(target)
        self._dirty = False
        self.project_path = target
        self.step5.project_path = self.project_path
        self.step5.session = None
        self._refresh_sidebar()
        return self.project_path

    def _write_side_mdat(self, target: Path) -> Path | None:
        """저장할 때 측정을 `<이름>.mdat` 로도 남긴다 — 파일 하나로 옮기고 REW 로 바로 열게(10-09 사용자 요청).
        `.irs/` 는 그대로 둔다(위치마다 바로 저장돼 측정 중 꺼져도 남는다). 실패해도 저장은 막지 않는다."""
        try:
            from secs.rewapi.session import load_project_irs
            from secs.rew_mdat.builder import MeasurementInput, build_mdat
            irs = load_project_irs(self.project, target)
            order = list(self.step5.plan().roles)
            offsets = self._spl_offsets()
            items = []
            for pos in sorted(irs):
                roles = sorted(irs[pos], key=lambda r: order.index(r) if r in order else len(order))
                for role in roles:
                    ir, rate, *rest = irs[pos][role]
                    items.append(MeasurementInput(f'{role}_P{pos}', f'SECS · 위치 {pos} · 원 측정',
                                                  int(rate), np.asarray(ir, np.float32),
                                                  float(rest[0]) if rest else 0.0,
                                                  offsets.get((role, int(pos)))))
            if not items:
                return None
            return build_mdat(items, target.with_suffix('.mdat'))
        except Exception:  # noqa: BLE001 - mdat 는 덤 — 실패해도 .secs·.irs 저장은 그대로
            return None

    def _spl_offsets(self) -> dict:
        """(role, 위치) → REW SPL 오프셋(dB). `.secs` 에 적힌 값이 먼저, 없으면(10-09 이전 측정) REW 에 uuid 로 물어본다
        (REW 가 꺼졌거나 그 측정을 지웠으면 없음 → 템플릿 값)."""
        out: dict = {}
        ask = getattr(self.measurer, 'spl_offset_db', None)
        for record in self.project.measurements or []:
            if not isinstance(record, dict):
                continue
            try:
                pos = int(record.get('pos'))
            except (TypeError, ValueError):
                continue
            stored = record.get('spl_offset') if isinstance(record.get('spl_offset'), dict) else {}
            uuids = record.get('uuid') if isinstance(record.get('uuid'), dict) else {}
            for role in (record.get('ir') or {}):
                value = stored.get(role)
                if value is None and callable(ask) and uuids.get(role):
                    try:
                        value = ask(uuids[role])
                    except Exception:  # noqa: BLE001 - REW 가 없으면 템플릿 값
                        value = None
                    if value is not None:
                        record.setdefault('spl_offset', {})[role] = float(value)   # 다음엔 묻지 않게
                if value is not None:
                    out[(str(role), pos)] = float(value)
        return out

    def _carry_irs(self, old: Path | None, new: Path) -> bool:
        """저장 위치를 옮길 때 `<프로젝트>.irs/` 를 따라가게 한다.

        측정 기록의 wav 경로는 `.secs` 가 있는 폴더 기준 상대 경로다. 파일만
        옮기면 IR 을 못 찾으므로 폴더를 복사하고 기록의 폴더 이름을 고친다.
        원본은 지우지 않는다(사용자가 옮겨 달라고 한 적은 없다).
        """
        import shutil

        if old is None or old == new:
            return False
        source, destination = old.with_suffix('.irs'), new.with_suffix('.irs')
        if source == destination or not source.is_dir():
            return False
        try:
            shutil.copytree(source, destination, dirs_exist_ok=True)
        except Exception:  # noqa: BLE001 - 복사 실패해도 .secs 저장은 막지 않는다
            return False
        for record in self.project.measurements or []:
            irs = record.get('ir') if isinstance(record, dict) else None
            if not isinstance(irs, dict):
                continue
            for role, value in list(irs.items()):
                parts = Path(str(value)).parts
                if parts and parts[0] == source.name:
                    irs[role] = '/'.join((destination.name, *parts[1:]))
        return True

    def load_project(self, path: str | None = None) -> Path | None:
        if path is None:
            path, _ = QtWidgets.QFileDialog.getOpenFileName(
                self, t('프로젝트 불러오기'), '', 'SECS 프로젝트 (*.secs)')
        if not path:
            return None
        from secs.wizard import open_project
        loaded = open_project(path)          # 바이앰프를 닫아 두었으면 단일 앰프로(10-11, `secs.wizard.BIAMP_UI`)
        self.project.__dict__.update(loaded.__dict__)
        self._dirty = False
        self.project_path = Path(path)
        self.step5.project_path = self.project_path
        self.step5.session = None
        self.step5.reload_curves()          # 불러온 측정의 그래프(10-09: 완료로만 뜨고 곡선이 비었다)
        for step in self.steps:
            if hasattr(step, 'refresh'):
                step.refresh()
        self.go(self.current_step)
        return self.project_path

    #: 닫을 때 저장 여부를 물을지. 기본 False — `run_wizard`(실제 실행)만 켠다. 테스트·캡처 도구는 모달 대화상자에 멈춘다.
    ask_before_close = False

    #: 닫기 확인 QMessageBox 는 창의 QSS 를 물려받지 않는다(따로 뜨는 최상위 창이라 —
    #: 09-22 사용자 보고: 어두운 바탕에 어두운 글자). 밝은 바탕·잉크색 글자를 직접 준다.
    _CONFIRM_CLOSE_QSS = f"""
        QMessageBox {{ background: {COLORS['panel']}; }}
        QMessageBox QLabel {{ color: {COLORS['ink']}; font-family: {FONT_UI}; font-size: 13px; }}
        QMessageBox QPushButton {{
            background: transparent; border: 1px solid {COLORS['chassis']}; color: {COLORS['chassis']};
            border-radius: 14px; padding: 6px 16px; font-family: {FONT_UI}; font-weight: 600;
            font-size: 13px; min-width: 64px;
        }}
        QMessageBox QPushButton:hover {{ background: #dde6e5; }}
        QMessageBox QPushButton[variant="accent"] {{
            background: {COLORS['accent']}; border-color: {COLORS['accent']}; color: #ffffff;
        }}
        QMessageBox QPushButton[variant="accent"]:hover {{ background: #17706b; }}
    """

    def _confirm_close(self) -> bool:
        """저장 안 한 변경이 있으면 묻는다. 계속 닫아도 되면 True."""
        if not self.ask_before_close or not getattr(self, '_dirty', False):
            return True
        box = QtWidgets.QMessageBox(self)
        box.setIcon(QtWidgets.QMessageBox.Icon.Question)
        box.setStyleSheet(self._CONFIRM_CLOSE_QSS)
        box.setWindowTitle(str(t('프로젝트 저장')))
        box.setText(str(t('저장하지 않은 변경이 있습니다. 저장할까요?')))
        save = box.addButton(str(t('저장')), QtWidgets.QMessageBox.ButtonRole.AcceptRole)
        discard = box.addButton(str(t('저장 안 함')), QtWidgets.QMessageBox.ButtonRole.DestructiveRole)
        box.addButton(str(t('취소')), QtWidgets.QMessageBox.ButtonRole.RejectRole)
        set_variant(save, 'accent')
        box.setDefaultButton(save)
        box.exec()
        clicked = box.clickedButton()
        if clicked is save:
            return self.save_project(str(self.project_path) if self.project_path else None) is not None
        return clicked is discard

    def closeEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        if not self._confirm_close():
            event.ignore()
            return
        self._rew_watch_stop.set()
        if getattr(self, '_mic_meter', None) is not None:
            self._mic_meter.close()
        self.step3.stop_monitor(wait=True)
        self.step5.cancel()
        self.step5.wait(3000)
        # 보정 설계 계산은 끊을 수 없는 daemon 스레드 — 결과만 버린다(프로세스가 끝나면 같이 끝난다).
        self.calibration_page.cancel()
        from secs.mimo_iir import abort_workers
        abort_workers()                     # 창을 닫으면 MIMO 워커도 같이 끈다(10-09: 꺼도 계속 돌았다)
        super().closeEvent(event)


__all__ = ['WizardWindow', 'SKIP_LABELS', 'PAGE_NAMES']
