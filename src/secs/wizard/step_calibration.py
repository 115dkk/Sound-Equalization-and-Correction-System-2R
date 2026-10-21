# -*- coding: utf-8 -*-
"""보정 설계 단계(옛 6 필터 설계 + 7 저역 통합을 하나로, 2026-09-26 — `문서/요구 대장.md` 최상위 결정 "6,7단계를 다 통합").

누가 부르나: `WizardWindow` 가 만들어 스택에 넣는다(화면 번호 `CAL_STEP` = 9, 진행바 6번째 = 측정 다음·내보내기 앞).
창이 `set_context(프로젝트 경로, IR 공급 함수)` 로 측정 IR 을 대 주고, 측정이 끝나면(자동 보정) `compute()` 를 부른다.
입력: `project`(구성·타겟·XO·`extra['calibration']` 설정 — `secs.calibration.CalibrationSettings`) + 측정 IR.
출력: `self.run`(`calibration.CalibrationRun` — 새 엔진 `CorrectionEngine` 결과·지표·곡선), `self.mimo_run`(MIMO), 내보낸 파일
(REW 단계별 mdat · 카밀라 DSP 설정). 설정은 `project.extra['calibration']` 에 바로 쓴다(.secs 에 남는다).

화면: 왼쪽 = 결과(채널 · SPL/GD 탭 · 채널 지표 줄 · 지연·RC 상한 요약) + 내보내기, 오른쪽 = 보정 모드(최소 지연 ·
저사양 · 최고사양, 엔진 `MODES` — 최고사양에서 사양 줄을 5번 누르면 숨은 초고지연 모드, 지연 30–200 ms) · RC 범위(하한 · 상한 자동/수동 · 룸 볼륨 또는 가로×세로×높이 — 자동 상한 = 슈뢰더) ·
MIMO(위치 3곳 이상, Fsiso · 하한, 명시 버튼) · BA 설정(10-05 요구 대장 R27, 서브 2개 이상: 서브마다 자동/고정/묶음 ·
값 · MIMO 서포트). 결과는 내보내기 단계(`step8_mdat`)가 `self.run`·`self.mimo_run` 으로 읽는다.
계산은 워커 스레드(보통 이하 우선순위)에서만 — 화면 스레드는 기다리지 않는다. 엔진은 끊을 수 없어 [취소] 는 결과를 버린다.
"""
from __future__ import annotations

import copy
import json
import os
import threading
import time
from pathlib import Path

import numpy as np
import pyqtgraph as pg
from PyQt6 import QtCore, QtGui
from PyQt6 import QtWidgets as _PlainQt
from secs.wizard import qt as QtWidgets

from secs import calibration as cal
from secs.model import TargetCurve, role_code, role_label
from secs.wizard.loading import run_off_thread
from secs.wizard.plot import C_CORRECTED, C_MEASURED, C_TARGET, FREQ_PAD, axis_label, freq_ticks, set_freq_range
from secs.wizard.strings import t
from secs.wizard.target_editor import TARGET_PRESETS, apply_preset, preset_of
from secs.wizard.style import (COLORS, add_heading, card, clear_layout, label, path_label, seg, set_state, set_variant,
                               two_columns)
from secs.wizard.tooltips import attach

#: 결과 탭 — (키, 문구).
TABS: tuple[tuple[str, str], ...] = (('spl', 'SPL'), ('gd', 'GD'), ('xo', 'XO'), ('subs', 'BA'))
#: MIMO 적용 곡선 색(보정 전·후·타겟과 겹치지 않는 파랑 — 6단계 XO 막대 색).
C_MIMO = '#3b86c8'
#: 결과 그래프 x 범위(Hz).
SPL_RANGE = (5.0, 20000.0)          # 10-09 사용자: X축 5 Hz 까지
GD_RANGE = (5.0, 5000.0)
GD_TICKS = (5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000)
#: XO 보기(09-27, 요구 대장 R7·R8): 메인 = 측정 자홍, 서브 = MIMO 파랑, 합 = 잉크, XO 선 = 강조색(메인을 보정 후 올리브로
#: 그렸더니 타겟 점선과 구분이 안 됐다 — 09-27 QA 캡처).
XO_RANGE_X = (5.0, 500.0)
XO_TICKS = (5, 10, 20, 30, 50, 80, 100, 150, 200, 300, 500)
C_XO_SUM = COLORS['ink']
C_XO_MAIN = C_MEASURED
#: BA 보기(10-04, 요구 대장 R9): 서브마다 색 하나(정렬 전 = 점선, 정렬 후 = 실선), 서브 합 = 잉크.
#: 색 범례는 정렬 값 표의 서브 이름 칸이 맡는다(범례 줄에 서브를 늘어놓으면 1024 폭에서 넘친다).
SUB_RANGE_X = (5.0, 300.0)
SUB_TICKS = (5, 10, 20, 30, 50, 80, 100, 150, 200, 300)
SUB_COLORS = (COLORS['measured'], COLORS['xo'], COLORS['corrected'], COLORS['warn'], COLORS['sub'], COLORS['accent'],
              COLORS['bad'], COLORS['ink2'])


#: BA 방식 고르기(R27) — (데이터, 문구). 묶음은 'bundle:<이름>'.
SUB_MODE_ITEMS: tuple[tuple[str, str], ...] = (('auto', '자동'), ('fixed', '고정')) + tuple(
    (f'bundle:{b}', f'Group {b}') for b in cal.SUB_BUNDLES)
#: 묶음 레이아웃(10-09 R30) — (데이터, 문구).
SUB_LAYOUT_ITEMS: tuple[tuple[str, str], ...] = (('values', '상대값 고정'), ('dba', 'DBA'), ('multi', '멀티서브'))


def _join(parts) -> str:
    """' · ' 로 이은 한 줄 — 조각마다 먼저 옮긴다. 통째로 넘기면 앞 조각의 `{0} … {1} ms` 패턴이 줄 전체를 삼켜
    뒤 조각이 한국어로 남았다(09-26 번역 점검). 언어를 바꾸면 창이 `refresh()` 를 불러 새 언어로 다시 만든다."""
    return ' · '.join(str(t(part)) for part in parts if part)


def _remaining_text(seconds: float) -> str:
    """남은 시간 한 조각 — 90 초 안쪽은 초('약 40초 남음'), 넘으면 분('약 3분 남음'). 다 됐는데 아직이면 '곧 끝남'."""
    s = float(seconds)
    if s <= 3.0:
        return '곧 끝남'
    if s < 90.0:
        return f'약 {s:.0f}초 남음'
    return f'약 {s / 60:.0f}분 남음'


def _legend_swatch(color: str, dashed: bool) -> _PlainQt.QLabel:
    """범례 선 견본 — 그래프 선과 같은 색 · 굵기(2 px)로 그린 22×10 그림. 점선이면 타겟처럼 끊어 그린다."""
    ratio = 2.0
    pix = QtGui.QPixmap(int(22 * ratio), int(10 * ratio))
    pix.setDevicePixelRatio(ratio)
    pix.fill(QtCore.Qt.GlobalColor.transparent)
    painter = QtGui.QPainter(pix)
    painter.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing)
    pen = QtGui.QPen(QtGui.QColor(color), 2.5)
    if dashed:
        pen.setStyle(QtCore.Qt.PenStyle.DashLine)
    painter.setPen(pen)
    painter.drawLine(QtCore.QPointF(1, 5), QtCore.QPointF(21, 5))
    painter.end()
    swatch = _PlainQt.QLabel()
    swatch.setPixmap(pix)
    swatch.setObjectName('legendSwatch')
    return swatch


def _below_normal_priority() -> None:
    """지금 스레드를 '보통 이하' 로(09-25 하이브리드 먹통 교훈 — 계산이 화면·다른 프로그램을 굶기지 않게)."""
    try:
        import sys
        if sys.platform == 'win32':
            import ctypes
            kernel = ctypes.windll.kernel32
            kernel.SetThreadPriority(kernel.GetCurrentThread(), -1)       # THREAD_PRIORITY_BELOW_NORMAL
    except Exception:  # noqa: BLE001 - 우선순위는 있으면 좋은 것
        pass



def default_export_dir(project_path) -> Path:
    """내보낼 폴더 기본값 = 프로젝트 옆 `SECS_export/<날짜> <프로젝트 이름>/`(폴더 이름 ASCII — 영어 화면 · 카밀라 GUI)(10-09 사용자: 바탕화면에 폴더가 흩어진다,
    폴더로 모아라). 프로젝트가 없으면 문서 폴더 아래 같은 꼴."""
    stamp = time.strftime('%Y-%m-%d')
    if project_path:
        p = Path(project_path)
        return p.parent / 'SECS_export' / f'{stamp} {p.stem}'
    return Path.home() / 'Documents' / 'SECS_export' / stamp

class _Bridge(QtCore.QObject):
    """워커 스레드(파이썬 daemon 스레드) → 화면 스레드 신호. 받는 쪽이 화면 스레드에 살아 대기열로 넘어간다."""

    done = QtCore.pyqtSignal(str, object, object)        # 종류, 토큰, 값
    failed = QtCore.pyqtSignal(str, object, str)          # 종류, 토큰, 문구
    progress = QtCore.pyqtSignal(str, object, int, int)   # 종류, 토큰, 끝난 수, 전체(XO 추천 후보)


class _ResultBar(QtWidgets.QWidget):
    """결과 카드 맨 위 줄 — 왼쪽 채널 고르기 + 오른쪽 결과 탭. 본문 칸이 한 줄을 못 담으면 탭을 아랫줄(왼쪽 붙임)로 내린다.

    10-05 요구 대장 R5: 1024 폭에서 이 한 줄의 최소 폭이 본문 칸(~410 px)을 넘어 가로 스크롤이 났다(한국어 25 px,
    영어 118 px — 탭 4개 'SPL · Group delay · XO · BA'). 최소 폭은 두 줄일 때 값(넓은 쪽 줄)으로 둬서
    본문 칸을 넓히지 않는다."""

    def __init__(self, left: QtWidgets.QHBoxLayout, right: QtWidgets.QHBoxLayout, parent=None) -> None:
        super().__init__(parent)
        self.left, self.right = left, right
        self.row = QtWidgets.QBoxLayout(QtWidgets.QBoxLayout.Direction.LeftToRight, self)
        self.row.setContentsMargins(0, 0, 0, 0)
        self.row.setSpacing(8)
        self.row.addLayout(left, 1)
        self.row.addLayout(right)
        # 아랫줄일 때만 늘어나 탭을 왼쪽에 붙인다(한 줄일 때 늘어나면 탭이 오른쪽 끝에서 떨어진다).
        self._tail = QtWidgets.QSpacerItem(0, 0, QtWidgets.QSizePolicy.Policy.Fixed, QtWidgets.QSizePolicy.Policy.Minimum)
        right.addItem(self._tail)

    def one_line(self) -> bool:
        return self.row.direction() == QtWidgets.QBoxLayout.Direction.LeftToRight

    def fit(self) -> None:
        """지금 폭에 맞춰 한 줄/두 줄을 고른다(크기·탭·언어가 바뀔 때)."""
        self.setMinimumWidth(max(self.left.minimumSize().width(), self.right.minimumSize().width()))
        need = self.left.sizeHint().width() + self.row.spacing() + self.right.sizeHint().width()
        one = self.width() >= need
        if one == self.one_line():
            return
        Policy = QtWidgets.QSizePolicy.Policy
        self._tail.changeSize(0, 0, Policy.Fixed if one else Policy.Expanding, Policy.Minimum)
        self.right.invalidate()
        self.row.setDirection(QtWidgets.QBoxLayout.Direction.LeftToRight if one
                              else QtWidgets.QBoxLayout.Direction.TopToBottom)

    def resizeEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().resizeEvent(event)
        self.fit()


class StepCalibration(QtWidgets.QWidget):
    """보정 설계(6·7 통합)."""

    changed = QtCore.pyqtSignal()
    #: 보정 계산이 끝났다(새 결과).
    computed = QtCore.pyqtSignal()

    def __init__(self, project, parent=None) -> None:
        super().__init__(parent)
        self.project = project
        self.settings = cal.CalibrationSettings.from_project(project)
        self.project_path: Path | None = None
        self.irs_provider = None                  # () → pos → role → (ir, sr[, t0])
        self.run: cal.CalibrationRun | None = None
        self.mimo_run: cal.MimoRun | None = None
        self.sub_run: cal.CalibrationRun | None = None   # [서브만 계산] 결과 — BA 탭만 이걸로 그린다(10-09)
        self._job: dict | None = None             # {'kind', 'token', 'thread', 'started'}
        self._error = ''
        self._mimo_error = ''
        self._cancelled = False
        self.compute_count = 0                    # 테스트·창: 실제로 끝난 보정 계산 수
        self.mimo_count = 0
        self.channel = ''
        self.tab = 'spl'
        self._ultra_unlocked = self.settings.mode == cal.ULTRA_MODE
        self._spec_clicks: list[float] = []
        self.last_files: list[str] = []
        self._exporting = False
        self._bridge = _Bridge(self)
        self._bridge.done.connect(self._on_done)
        self._bridge.failed.connect(self._on_failed)
        self._bridge.progress.connect(self._on_progress)
        self.xo_group = ''
        self.sub_role = ''                 # BA 설정 카드에서 고친 서브
        self._xo_error = ''
        self._xo_progress = (0, 0)
        self._xo_done_at = 0.0             # 마지막 후보가 끝난 시각(perf_counter) — 남은 시간 어림에 쓴다
        # MIMO·XO 추천 줄의 남은 시간을 1 초마다 고친다(보정 계산은 창의 로딩 오버레이가 센다).
        self._tick = QtCore.QTimer(self)
        self._tick.setInterval(1000)
        self._tick.timeout.connect(self._on_tick)
        self.xo_recommendation: cal.XoRecommendation | None = None
        self._xo_curves: dict = {}
        self._syncing = True               # 만드는 동안 스핀 범위 설정이 값 바뀜 신호를 내도 설정을 건드리지 않게
        self._build()
        self._syncing = False
        self.refresh()

    # ──────────────────────────── 뼈대 ────────────────────────────
    def _build(self) -> None:
        outer = QtWidgets.QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(14)
        add_heading(outer, t('6 · 보정 설계'))
        _body, main, side = two_columns(outer)
        main.addWidget(self._result_card())
        # 내보내기는 내보내기 탭(`step8_mdat`) 한 곳에서만(10-09 사용자: 두 곳에 있으면 내보내기 탭이 무의미) — 카드는 만들되
        # 화면에 넣지 않는다(내보내기 탭이 이 페이지의 `export` 를 그대로 부른다).
        self.export_card = self._export_card()
        self.export_card.setParent(self)
        self.export_card.hide()
        # 오른쪽 칸(카드 4장)이 더 길면 본문 칸이 그 높이로 늘어나는데, 남는 높이가 결과 카드의 줄들 사이로 퍼져
        # 숫자 줄과 표 사이가 크게 비었다(10-05 R5 1024×640 캡처) — 남는 높이는 카드 아래 빈칸으로.
        main.addStretch(1)
        # 오른쪽 카드는 탭에 맞춰 하나만(10-09 사용자: "SPL 탭에서는 RC, XO 탭에서는 XO, BA 에서 BA") —
        # 보정 모드·MIMO 는 늘 보인다(`_show_tab_cards`). 타겟 커브 카드는 없앴다 — SPL 그래프 빈 곳 우클릭 메뉴(`_show_target_menu`).
        side.box.addWidget(self._mode_card())
        self.rc_card = self._rc_card()
        side.box.addWidget(self.rc_card)
        self._target_handles = []
        self._target_offset = 0.0
        self.xo_card = self._xo_card()
        side.box.addWidget(self.xo_card)
        side.box.addWidget(self._sub_card())
        side.box.addWidget(self._mimo_card())
        side.box.addStretch(1)
        outer.addStretch(1)

    def _result_card(self) -> QtWidgets.QFrame:
        frame, box = card('')
        channel = QtWidgets.QHBoxLayout()
        channel.setSpacing(8)
        channel.addWidget(label('채널', 'note'))
        # 채널 이름은 번역하지 않는다(돌비 표기, CLAUDE.md '채널 이름은 plain_label') — 번역 없는 콤보.
        self.channel_combo = _PlainQt.QComboBox()
        self.channel_combo.setMinimumWidth(90)
        self.channel_combo.currentIndexChanged.connect(self._on_channel)
        attach(self.channel_combo, 'cal.channel')
        channel.addWidget(self.channel_combo)
        channel.addStretch(1)
        tabs = QtWidgets.QHBoxLayout()
        tabs.setSpacing(8)
        self.tab_buttons: dict[str, QtWidgets.QPushButton] = {}
        for key, text in TABS:
            button = QtWidgets.QPushButton(t(text))
            button.setObjectName('tab')
            button.clicked.connect(lambda _=False, k=key: self.set_tab(k))
            attach(button, f'cal.tab_{key}')
            tabs.addWidget(button)
            self.tab_buttons[key] = button
        self.result_bar = _ResultBar(channel, tabs)
        box.addWidget(self.result_bar)

        self.plot = pg.PlotWidget()
        self.plot.setBackground(COLORS['plot_bg'])
        self.plot.setMinimumHeight(300)
        self.plot.setMaximumHeight(380)
        self.plot.setMouseEnabled(x=False, y=False)
        self.plot.setMenuEnabled(False)
        item = self.plot.getPlotItem()
        item.hideButtons()
        item.vb.setMenuEnabled(False)
        item.showGrid(x=True, y=True, alpha=0.25)
        item.setLogMode(x=True, y=False)
        for name in ('left', 'bottom'):
            self.plot.getAxis(name).setTextPen(COLORS['ink2'])
            self.plot.getAxis(name).setPen(pg.mkPen(COLORS['grid']))
        attach(self.plot, 'cal.plot')
        self.plot.scene().sigMouseClicked.connect(self._on_plot_click)
        box.addWidget(self.plot)
        # XO 선 — XO 탭에서만 그래프에 올린다. 끄는 동안 곡선이 따라오고(0.2 s 안), 놓으면 설정에 들어간다.
        self.xo_line = pg.InfiniteLine(angle=90, movable=True, pen=pg.mkPen(COLORS['accent'], width=2),
                                       hoverPen=pg.mkPen(COLORS['accent'], width=4))
        self.xo_line.sigPositionChanged.connect(self._on_xo_line_moving)
        self.xo_line.sigPositionChangeFinished.connect(self._on_xo_line_done)

        legend = QtWidgets.QHBoxLayout()
        legend.setSpacing(14)
        self.legend_items: dict[str, QtWidgets.QWidget] = {}
        for key, color, text, dashed in (('before', C_MEASURED, '보정 전', False), ('after', C_CORRECTED, '보정 후', False),
                                         ('target', C_TARGET, '타겟', True), ('mimo', C_MIMO, 'MIMO 적용', False),
                                         ('xo_main', C_XO_MAIN, '메인', False), ('xo_sub', C_MIMO, '서브', False),
                                         ('xo_sum', C_XO_SUM, '합', False),
                                         ('sub_before', COLORS['ink3'], '정렬 전', True),
                                         ('sub_after', COLORS['ink3'], '정렬 후', False),
                                         ('sub_sum', C_XO_SUM, '서브 합', False)):
            holder = QtWidgets.QWidget()
            row = QtWidgets.QHBoxLayout(holder)
            row.setContentsMargins(0, 0, 0, 0)
            row.setSpacing(5)
            row.addWidget(_legend_swatch(color, dashed))
            row.addWidget(label(text, 'note'))
            legend.addWidget(holder)
            self.legend_items[key] = holder
        legend.addStretch(1)
        box.addLayout(legend)

        # 결과 숫자 줄은 'spec'(모양은 note 와 같다) — 화면의 긴 note 는 설명문으로 보고 도움말로 옮기는 규칙(09-22)이라 가른다.
        self.metrics_label = label('', 'spec', wrap=True)
        attach(self.metrics_label, 'cal.metrics')          # 지표 풀이(10-10 AI 시험: 'GD 폭' 정의가 없다)
        box.addWidget(self.metrics_label)
        self.filters_label = label('', 'spec', wrap=True)
        box.addWidget(self.filters_label)
        # BA 탭의 정렬 값 표(서브 | 게인 | 딜레이 | 극성 | 올패스) — 서브 이름 칸의 색이 곡선 색 범례.
        self.sub_values = QtWidgets.QWidget()
        self.sub_grid = QtWidgets.QGridLayout(self.sub_values)
        self.sub_grid.setContentsMargins(0, 0, 0, 0)
        # 열 사이 8 px — R27 '방식' 열이 들어와 1024 영어에서 표가 본문 칸보다 45 px 넓었다(10-05 R5 점검), 묶음 값('Bundle A')이면
        # 7 px 더(10-05 돌비 3서브 캡처).
        self.sub_grid.setHorizontalSpacing(8)
        self.sub_grid.setVerticalSpacing(3)
        attach(self.sub_values, 'cal.sub_values')
        self.sub_values.setVisible(False)
        box.addWidget(self.sub_values)
        self.sub_filters_label = label('', 'spec', wrap=True)     # 서브별 대역 필터·고정 PEQ(R27, BA 탭)
        self.sub_filters_label.setVisible(False)
        box.addWidget(self.sub_filters_label)
        # BA 경고(10-12 사용자: 경고 기능) — 서브 필터가 대역을 막았거나 게인이 비정상으로 클 때 빨간 글자
        self.sub_warn_result = label('', 'spec', wrap=True)
        self.sub_warn_result.setStyleSheet('color: %s; font-weight: 600;' % COLORS['bad'])
        self.sub_warn_result.setVisible(False)
        box.addWidget(self.sub_warn_result)
        self.summary_label = label('', 'spec', wrap=True)
        box.addWidget(self.summary_label)
        return frame

    def _export_card(self) -> QtWidgets.QFrame:
        frame, box = card('내보내기')
        path_row = QtWidgets.QHBoxLayout()
        path_row.setSpacing(8)
        path_row.addWidget(label('내보낼 폴더', 'note'))
        path_row.addStretch(1)
        self.export_path_label = path_label('')
        # 오른쪽 칸이 있는 좁은 본문(1024 폭에서 ~400 px)에서 경로 글자 폭이 최소 폭이 되어 가로 스크롤이 생겼다(09-26 QA) —
        # 크기 힌트를 무시하고 남는 폭만 받는다(넘치면 가운데를 … 로 줄이고 전체는 풍선 도움말).
        self.export_path_label.setSizePolicy(QtWidgets.QSizePolicy.Policy.Ignored, QtWidgets.QSizePolicy.Policy.Preferred)
        self.export_path_change = QtWidgets.QPushButton(t('변경…'))
        set_variant(self.export_path_change, 'ghost', 'sm')
        self.export_path_change.clicked.connect(lambda: self.choose_export_folder())
        attach(self.export_path_change, 'cal.folder_change')
        self.export_path_open = QtWidgets.QPushButton(t('폴더 열기'))
        set_variant(self.export_path_open, 'ghost', 'sm')
        self.export_path_open.clicked.connect(self.open_export_folder)
        attach(self.export_path_open, 'cal.folder_open')
        path_row.addWidget(self.export_path_change)
        path_row.addWidget(self.export_path_open)
        box.addLayout(path_row)
        # 경로는 따로 한 줄 — 1024 폭에서 버튼 옆에 두면 'C:…hi' 까지 줄었다(09-26 캡처).
        box.addWidget(self.export_path_label)
        buttons = QtWidgets.QHBoxLayout()
        buttons.setSpacing(8)
        self.export_buttons: dict[str, QtWidgets.QPushButton] = {}
        for key, text, variant in (('mdat', 'REW mdat', 'accent'), ('camilla', 'CamillaDSP 내보내기', 'ghost')):
            button = QtWidgets.QPushButton(t(text))
            set_variant(button, variant, 'sm')
            button.clicked.connect(lambda _=False, k=key: self.export(k, open_after=True))
            attach(button, f'cal.export_{key}')
            buttons.addWidget(button)
            self.export_buttons[key] = button
        buttons.addStretch(1)
        box.addLayout(buttons)
        self.export_note = label('', 'note', wrap=True)
        box.addWidget(self.export_note)
        return frame

    def _mode_card(self) -> QtWidgets.QFrame:
        frame, box = card('보정 모드')
        keys = cal.MODE_KEYS + (cal.ULTRA_MODE,)
        self.mode_seg, self.mode_buttons = seg(tuple((key, cal.mode_label(key)) for key in keys),
                                               self.set_mode, vertical=True)
        for key, button in self.mode_buttons.items():
            attach(button, f'cal.mode_{key}')
        box.addWidget(self.mode_seg)
        self.mode_spec = label('', 'spec', wrap=True)
        # 숨은 버튼(09-27 사용자 이스터에그): 최고사양 모드에서 이 사양 줄을 연달아 5번 누르면 초고지연 모드가 열린다.
        self.mode_spec.installEventFilter(self)
        box.addWidget(self.mode_spec)
        self.ultra_row = QtWidgets.QWidget()
        ultra = QtWidgets.QHBoxLayout(self.ultra_row)
        ultra.setContentsMargins(0, 0, 0, 0)
        ultra.setSpacing(8)
        ultra.addWidget(label('지연 한도', 'note'))
        self.ultra_spin = self._spin(*cal.ULTRA_MS_RANGE, 10.0, ' ms')
        self.ultra_spin.valueChanged.connect(self._on_ultra_ms)
        attach(self.ultra_spin, 'cal.ultra_ms')
        ultra.addWidget(self.ultra_spin)
        ultra.addStretch(1)
        box.addWidget(self.ultra_row)
        row = QtWidgets.QHBoxLayout()
        row.setSpacing(8)
        self.compute_button = QtWidgets.QPushButton(t('계산'))
        set_variant(self.compute_button, 'accent', 'sm')
        self.compute_button.clicked.connect(lambda: self.compute_clicked())
        attach(self.compute_button, 'cal.compute')
        self.cancel_button = QtWidgets.QPushButton(t('계산 취소'))
        set_variant(self.cancel_button, 'ghost', 'sm')
        self.cancel_button.clicked.connect(self.cancel)
        attach(self.cancel_button, 'cal.cancel')
        row.addWidget(self.compute_button)
        row.addWidget(self.cancel_button)
        row.addStretch(1)
        box.addLayout(row)
        self.status = label('', 'note', wrap=True)
        box.addWidget(self.status)
        return frame

    @staticmethod
    def _spin(lo: float, hi: float, step: float, suffix: str, decimals: int = 0) -> QtWidgets.QDoubleSpinBox:
        spin = QtWidgets.QDoubleSpinBox()
        spin.setRange(lo, hi)
        spin.setDecimals(decimals)
        spin.setSingleStep(step)
        spin.setSuffix(suffix)
        spin.setKeyboardTracking(False)            # 칠 때마다가 아니라 다 치고 나서 한 번
        spin.setMinimumWidth(96)
        return spin

    def _rc_card(self) -> QtWidgets.QFrame:
        frame, box = card('RC 범위')
        grid = QtWidgets.QGridLayout()
        grid.setHorizontalSpacing(8)
        grid.setVerticalSpacing(6)
        grid.addWidget(label('하한', 'note'), 0, 0)
        self.rc_lo_spin = self._spin(10.0, 200.0, 5.0, ' Hz')
        self.rc_lo_spin.valueChanged.connect(self._on_rc_lo)
        attach(self.rc_lo_spin, 'cal.rc_lo')
        grid.addWidget(self.rc_lo_spin, 0, 2)
        grid.addWidget(label('상한', 'note'), 1, 0)
        self.rc_hi_auto = QtWidgets.QCheckBox(t('자동'))
        self.rc_hi_auto.toggled.connect(self._on_rc_hi_auto)
        attach(self.rc_hi_auto, 'cal.rc_hi_auto')
        grid.addWidget(self.rc_hi_auto, 1, 1)
        self.rc_hi_spin = self._spin(*cal.RC_HI_RANGE, 50.0, ' Hz')
        self.rc_hi_spin.valueChanged.connect(self._on_rc_hi)
        attach(self.rc_hi_spin, 'cal.rc_hi')
        grid.addWidget(self.rc_hi_spin, 1, 2)
        grid.addWidget(label('룸 볼륨', 'note'), 2, 0)
        self.volume_spin = self._spin(0.0, 100000.0, 10.0, ' m³')
        self.volume_spin.setSpecialValueText(str(t('없음')))
        self.volume_spin.valueChanged.connect(self._on_volume)
        attach(self.volume_spin, 'cal.volume')
        grid.addWidget(self.volume_spin, 2, 2)
        # 치수(가로×세로×높이 m) — 셋 다 넣으면 부피 칸이 곱으로 채워진다. 좁은 오른쪽 칸(300 px)에 들어가게 한 줄 따로.
        grid.addWidget(label('치수', 'note'), 3, 0)
        dims = QtWidgets.QHBoxLayout()
        dims.setSpacing(3)
        self.dim_spins: list[QtWidgets.QDoubleSpinBox] = []
        for k, key in enumerate(('cal.dim_w', 'cal.dim_d', 'cal.dim_h')):
            if k:
                dims.addWidget(_PlainQt.QLabel('×'))
            spin = QtWidgets.QDoubleSpinBox()
            spin.setRange(0.0, cal.ROOM_DIM_RANGE[1])
            spin.setDecimals(1)
            spin.setSingleStep(0.1)
            spin.setSpecialValueText('–')          # 0 = 비움(글자가 없어 번역할 것도 없다)
            spin.setKeyboardTracking(False)
            spin.setButtonSymbols(QtWidgets.QAbstractSpinBox.ButtonSymbols.NoButtons)
            spin.setMinimumWidth(44)
            spin.setAlignment(QtCore.Qt.AlignmentFlag.AlignRight)
            spin.valueChanged.connect(self._on_dims)
            attach(spin, key)
            dims.addWidget(spin, 1)
            self.dim_spins.append(spin)
        dims.addWidget(_PlainQt.QLabel('m'))
        grid.addLayout(dims, 3, 1, 1, 2)
        # 부스트 상한(10-09 사용자) — RC · XO PEQ · 서브 버스 PEQ 의 올림을 모두 이 안으로(헤드룸)
        grid.addWidget(label('부스트 상한', 'note', wrap=True), 4, 0)
        self.boost_spin = self._spin(*cal.BOOST_RANGE, 1.0, ' dB', 1)
        self.boost_spin.valueChanged.connect(self._on_boost)
        attach(self.boost_spin, 'cal.boost_max')
        grid.addWidget(self.boost_spin, 4, 2)
        grid.setColumnStretch(1, 1)
        box.addLayout(grid)
        self.rc_note = label('', 'spec', wrap=True)
        box.addWidget(self.rc_note)
        return frame

    def _xo_card(self) -> QtWidgets.QFrame:
        frame, box = card('XO (크로스오버)')
        grid = QtWidgets.QGridLayout()
        grid.setHorizontalSpacing(8)
        grid.setVerticalSpacing(6)
        grid.addWidget(label('그룹', 'note'), 0, 0)
        # 그룹 키(front·surround…)는 모델 키 그대로 — 번역 없는 콤보.
        self.xo_group_combo = _PlainQt.QComboBox()
        self.xo_group_combo.currentIndexChanged.connect(self._on_xo_group)
        attach(self.xo_group_combo, 'cal.xo_group')
        grid.addWidget(self.xo_group_combo, 0, 2)
        grid.addWidget(label('XO', 'note'), 1, 0)
        self.xo_spin = self._spin(*cal.XO_RANGE, 5.0, ' Hz')
        self.xo_spin.valueChanged.connect(self._on_xo_hz)
        attach(self.xo_spin, 'cal.xo_hz')
        grid.addWidget(self.xo_spin, 1, 2)
        # XO PEQ(10-09 사용자: 따로 정하게) — 켜기 · 하한 · 상한(0 = 자동: XO ½ … XO × 4, RC 상한으로 자르지 않음 — 10-09 결정), 모든 그룹 공통
        self.xo_peq_check = QtWidgets.QCheckBox(t('XO PEQ'))
        self.xo_peq_check.toggled.connect(self._on_xo_peq)
        attach(self.xo_peq_check, 'cal.xo_peq_on')
        grid.addWidget(self.xo_peq_check, 2, 0, 1, 2)
        grid.addWidget(label('XO PEQ 하한', 'note', wrap=True), 3, 0)
        self.xo_peq_lo_spin = self._small_spin(*cal.XO_PEQ_RANGE, 0, ' Hz', 'cal.xo_peq_lo')
        self.xo_peq_lo_spin.setSpecialValueText(str(t('자동')))
        self.xo_peq_lo_spin.valueChanged.connect(lambda _v: self._on_xo_peq())
        grid.addWidget(self.xo_peq_lo_spin, 3, 2)
        grid.addWidget(label('XO PEQ 상한', 'note', wrap=True), 4, 0)
        self.xo_peq_hi_spin = self._small_spin(*cal.XO_PEQ_RANGE, 0, ' Hz', 'cal.xo_peq_hi')
        self.xo_peq_hi_spin.setSpecialValueText(str(t('자동')))
        self.xo_peq_hi_spin.valueChanged.connect(lambda _v: self._on_xo_peq())
        grid.addWidget(self.xo_peq_hi_spin, 4, 2)
        grid.setColumnStretch(1, 1)
        box.addLayout(grid)
        self.xo_rec_label = label('', 'spec', wrap=True)
        box.addWidget(self.xo_rec_label)
        self.xo_reason = label('', 'note', wrap=True)
        box.addWidget(self.xo_reason)
        self.xo_button = QtWidgets.QPushButton(t('XO 추천 계산'))
        set_variant(self.xo_button, 'ghost', 'sm')
        self.xo_button.clicked.connect(lambda: self.recommend_xo())
        attach(self.xo_button, 'cal.xo_recommend')
        row = QtWidgets.QHBoxLayout()
        row.addWidget(self.xo_button)
        # 카드마다 취소(10-09 사용자: 보정 설계에 취소 버튼이 없다 — 위 모드 카드의 것은 스크롤 밖이었다)
        self.xo_cancel_button = QtWidgets.QPushButton(t('계산 취소'))
        set_variant(self.xo_cancel_button, 'ghost', 'sm')
        self.xo_cancel_button.clicked.connect(self.cancel)
        attach(self.xo_cancel_button, 'cal.cancel')
        row.addWidget(self.xo_cancel_button)
        row.addStretch(1)
        box.addLayout(row)
        self.xo_status = label('', 'note', wrap=True)
        box.addWidget(self.xo_status)
        return frame

    def _sub_card(self) -> QtWidgets.QFrame:
        """BA 설정(10-05 요구 대장 R27 — DBA 처럼 배치로 감쇠를 만든 사람이 BA 값을 직접 정하게). 오른쪽 칸(300 px)에
        서브 8개 줄을 펼 수 없어 서브를 하나 골라 고치고, 아래 요약 줄이 모든 서브의 방식을 보인다. 자동이면 값 칸은 꺼진다
        (넣어 둔 값은 남는다). 메인 추가 지연은 고정 서브가 있을 때만 보인다(`CalibrationSettings.sub_main_delay_ms`).
        고정 대역 필터(HP·LP 주파수 + 종류, LR 24 기본)·고정 PEQ 3(10-05 R27 추가 — INFRA 로우패스 등)는 방식과 상관없이 늘 켜져 있다."""
        frame, box = card('BA (Bass Alignment) 설정')
        self.sub_card = frame
        self._infra_rows(box)
        grid = QtWidgets.QGridLayout()
        grid.setHorizontalSpacing(8)
        grid.setVerticalSpacing(6)
        grid.addWidget(label('서브', 'note'), 0, 0)
        # 채널 이름은 번역하지 않는다(돌비 표기 — `role_code`) — 번역 없는 콤보.
        self.sub_pick = _PlainQt.QComboBox()
        self.sub_pick.currentIndexChanged.connect(self._on_sub_pick)
        attach(self.sub_pick, 'cal.sub_pick')
        grid.addWidget(self.sub_pick, 0, 1, 1, 2)
        grid.addWidget(label('방식', 'note'), 1, 0)
        self.sub_mode_combo = QtWidgets.QComboBox()
        for key, text in SUB_MODE_ITEMS:
            self.sub_mode_combo.addItem(text, key)
        self.sub_mode_combo.currentIndexChanged.connect(self._on_sub_mode)
        attach(self.sub_mode_combo, 'cal.sub_mode')
        grid.addWidget(self.sub_mode_combo, 1, 1, 1, 2)
        grid.addWidget(label('게인', 'note'), 5, 0)
        self.sub_gain_spin = self._spin(*cal.SUB_GAIN_RANGE, 0.5, ' dB', 2)
        self.sub_gain_spin.valueChanged.connect(lambda v: self._on_sub_value(gain_db=float(v)))
        attach(self.sub_gain_spin, 'cal.sub_gain')
        grid.addWidget(self.sub_gain_spin, 5, 2)
        grid.addWidget(label('딜레이', 'note'), 6, 0)
        self.sub_delay_spin = self._spin(*cal.SUB_DELAY_RANGE, 0.1, ' ms', 2)
        self.sub_delay_spin.valueChanged.connect(lambda v: self._on_sub_value(delay_ms=float(v)))
        attach(self.sub_delay_spin, 'cal.sub_delay')
        grid.addWidget(self.sub_delay_spin, 6, 2)
        grid.addWidget(label('극성', 'note'), 7, 0)
        self.sub_pol_combo = QtWidgets.QComboBox()
        self.sub_pol_combo.addItem('정상', 1)
        self.sub_pol_combo.addItem('반전', -1)
        self.sub_pol_combo.currentIndexChanged.connect(
            lambda _i: self._on_sub_value(polarity=int(self.sub_pol_combo.currentData() or 1)))
        attach(self.sub_pol_combo, 'cal.sub_polarity')
        grid.addWidget(self.sub_pol_combo, 7, 1, 1, 2)     # 두 칸 — 영어 'Non-inverted' 가 1024 폭 한 칸에서 잘렸다(10-05 캡처)
        # 올패스 두 개(f · Q) — f 0 = 없음. 좁은 칸이라 단추 없는 작은 칸 둘.
        self.sub_ap_spins: list[tuple[QtWidgets.QDoubleSpinBox, QtWidgets.QDoubleSpinBox]] = []
        for k in range(2):
            grid.addWidget(label(f'올패스 {k + 1}', 'note'), 8 + k, 0)
            f_spin = QtWidgets.QDoubleSpinBox()
            f_spin.setRange(0.0, cal.SUB_AP_F_RANGE[1])
            f_spin.setDecimals(0)
            f_spin.setSuffix(' Hz')
            f_spin.setSpecialValueText('— Hz')          # 비었지만 주파수 칸인 것이 보이게(10-05 리드)
            f_q = QtWidgets.QDoubleSpinBox()
            f_q.setRange(*cal.SUB_AP_Q_RANGE)
            f_q.setDecimals(2)
            f_q.setPrefix('Q ')
            for spin, key in ((f_spin, 'cal.sub_ap_f'), (f_q, 'cal.sub_ap_q')):
                spin.setKeyboardTracking(False)
                spin.setButtonSymbols(QtWidgets.QAbstractSpinBox.ButtonSymbols.NoButtons)
                spin.setMinimumWidth(60)
                spin.setAlignment(QtCore.Qt.AlignmentFlag.AlignRight)
                spin.valueChanged.connect(lambda _v: self._on_sub_allpass())
                attach(spin, key)
            grid.addWidget(f_spin, 8 + k, 1)
            grid.addWidget(f_q, 8 + k, 2)
            self.sub_ap_spins.append((f_spin, f_q))
        # 고정 대역 필터: HP · LP 한 줄씩 — 주파수 칸 + 종류 칸(끔 · LR 24 기본 …). 끔이면 주파수 칸이 꺼진다(10-05 리드: 켜졌는지·
        # 어느 칸이 주파수인지 보이게). PEQ 3밴드(주파수 · Q · dB, 주파수 비우면 없음).
        self.sub_band_combos: dict[str, QtWidgets.QComboBox] = {}
        for k, kind in enumerate(('hp', 'lp')):
            grid.addWidget(label(kind.upper(), 'note'), 10 + k, 0)
            spin = self._small_spin(0.0, cal.SUB_FILTER_RANGE[1], 0, ' Hz', f'cal.sub_{kind}', zero_text=True)
            spin.valueChanged.connect(lambda _v, kd=kind: self._on_sub_band_hz(kd))
            combo = QtWidgets.QComboBox()
            combo.addItem('끔', 'off')
            for typ in cal.SUB_BAND_TYPES:
                combo.addItem(cal.band_type_text(typ), typ)
            combo.currentIndexChanged.connect(lambda _i, kd=kind: self._on_sub_band_type(kd))
            attach(combo, f'cal.sub_{kind}_type')
            grid.addWidget(spin, 10 + k, 1)
            grid.addWidget(combo, 10 + k, 2)
            setattr(self, f'sub_{kind}_spin', spin)
            self.sub_band_combos[kind] = combo
        self.sub_peq_spins: list[tuple] = []
        for k in range(cal.SUB_PEQ_BANDS):
            grid.addWidget(label(f'PEQ {k + 1}', 'note'), 12 + k, 0)
            f_spin = self._small_spin(0.0, cal.SUB_AP_F_RANGE[1], 0, ' Hz', 'cal.sub_peq_f', zero_text=True)
            q_spin = self._small_spin(*cal.SUB_AP_Q_RANGE, 2, '', 'cal.sub_peq_q', prefix='Q ')
            g_spin = self._small_spin(*cal.SUB_PEQ_GAIN_RANGE, 1, ' dB', 'cal.sub_peq_gain')
            pair = QtWidgets.QHBoxLayout()
            pair.setSpacing(4)
            pair.addWidget(q_spin, 1)
            pair.addWidget(g_spin, 1)
            grid.addWidget(f_spin, 12 + k, 1)
            grid.addLayout(pair, 12 + k, 2)
            self.sub_peq_spins.append((f_spin, q_spin, g_spin))
        for spin in (w for row in self.sub_peq_spins for w in row):
            spin.valueChanged.connect(lambda _v: self._on_sub_peq())
        row = 12 + cal.SUB_PEQ_BANDS
        # 줄바꿈 라벨 — 영어 'Main extra delay' 가 1024 폭의 이름 칸(95 px)보다 3 px 길어 잘렸다(10-05 캡처).
        self.sub_main_label = label('메인 추가 지연', 'note', wrap=True)
        grid.addWidget(self.sub_main_label, row, 0)
        self.sub_main_spin = self._spin(*cal.SUB_MAIN_DELAY_RANGE, 0.1, ' ms', 2)
        self.sub_main_spin.valueChanged.connect(self._on_sub_main_delay)
        attach(self.sub_main_spin, 'cal.sub_main_delay')
        grid.addWidget(self.sub_main_spin, row, 2)
        self._layout_rows(grid, 2)                  # R30 레이아웃 줄 — 방식 바로 아래(2–4행)
        grid.setColumnStretch(1, 1)
        box.addLayout(grid)
        self.sub_mimo_check = QtWidgets.QCheckBox(t('MIMO 서포트로 쓰기'))
        self.sub_mimo_check.toggled.connect(self._on_sub_mimo)
        attach(self.sub_mimo_check, 'cal.sub_mimo')
        box.addWidget(self.sub_mimo_check)
        self.sub_take_button = QtWidgets.QPushButton(t('결과 값 가져오기'))
        set_variant(self.sub_take_button, 'ghost', 'sm')
        self.sub_take_button.clicked.connect(lambda: self.take_sub_values())
        attach(self.sub_take_button, 'cal.sub_take')
        self.sub_compute_button = QtWidgets.QPushButton(t('서브만 계산'))
        set_variant(self.sub_compute_button, 'accent', 'sm')
        self.sub_compute_button.clicked.connect(lambda: self.compute_subs())
        attach(self.sub_compute_button, 'cal.sub_compute')
        row = QtWidgets.QHBoxLayout()
        row.addWidget(self.sub_compute_button)
        row.addWidget(self.sub_take_button)
        row.addStretch(1)
        box.addLayout(row)
        self.sub_summary = label('', 'spec', wrap=True)
        box.addWidget(self.sub_summary)
        self.sub_warn_label = label('', 'spec', wrap=True)          # 설정 경고(계산 전에도) — `cal.sub_filter_warnings`
        self.sub_warn_label.setStyleSheet('color: %s; font-weight: 600;' % COLORS['bad'])
        self.sub_warn_label.setVisible(False)
        box.addWidget(self.sub_warn_label)
        return frame

    def _infra_rows(self, box: QtWidgets.QVBoxLayout) -> None:
        """인프라 XO(10-09 사용자: "인프라 서브가 있으면 XO 하기가 어렵다") — 인프라 서브 하나와 XO 주파수·종류만 고르면
        인프라에는 LP, 나머지 서브에는 같은 주파수·종류의 HP 를 넣는다(서브별 고정 HP·LP 칸에 그대로 써서 엔진은 그대로)."""
        grid = QtWidgets.QGridLayout()
        grid.setHorizontalSpacing(8)
        grid.setVerticalSpacing(6)
        grid.addWidget(label('인프라 서브', 'note'), 0, 0)
        self.infra_combo = _PlainQt.QComboBox()
        self.infra_combo.currentIndexChanged.connect(lambda _i: self._on_infra())
        attach(self.infra_combo, 'cal.infra_sub')
        grid.addWidget(self.infra_combo, 0, 1, 1, 2)
        self.infra_xo_label = label('인프라 XO', 'note')
        grid.addWidget(self.infra_xo_label, 1, 0)
        self.infra_hz_spin = self._small_spin(*cal.SUB_FILTER_RANGE, 0, ' Hz', 'cal.infra_xo')
        self.infra_hz_spin.valueChanged.connect(lambda _v: self._on_infra())
        grid.addWidget(self.infra_hz_spin, 1, 1)
        self.infra_type_combo = QtWidgets.QComboBox()
        for typ in cal.SUB_BAND_TYPES:
            self.infra_type_combo.addItem(cal.band_type_text(typ), typ)
        self.infra_type_combo.currentIndexChanged.connect(lambda _i: self._on_infra())
        attach(self.infra_type_combo, 'cal.infra_type')
        grid.addWidget(self.infra_type_combo, 1, 2)
        # 저역 서브를 대역 합 레벨에 맞춰 올리는 상한(10-09 사용자 결정: 끝까지 맞추되 헤드룸에 맞게 줄일 수 있게)
        self.infra_boost_label = label('인프라 최대 부스트', 'note', wrap=True)
        grid.addWidget(self.infra_boost_label, 2, 0)
        self.infra_boost_spin = self._spin(*cal.INFRA_BOOST_RANGE, 1.0, ' dB', 1)
        self.infra_boost_spin.valueChanged.connect(self._on_infra_boost)
        attach(self.infra_boost_spin, 'cal.infra_boost')
        grid.addWidget(self.infra_boost_spin, 2, 2)
        grid.setColumnStretch(1, 1)
        box.addLayout(grid)
        self.infra_note = label('', 'spec', wrap=True)
        box.addWidget(self.infra_note)

    def infra_state(self) -> tuple[str, float, str]:
        """지금 설정에서 읽은 인프라 XO — (인프라 서브, Hz, 종류). 인프라 = LP 만 있고 다른 서브는 모두 HP 가 있는 서브 하나.
        없으면 ('', 0, 기본 종류)."""
        roles = self.sub_roles()
        lows = [r for r in roles if (e := self.settings.sub(r))['lp_hz'] and not e['hp_hz']]
        if lows and all(self.settings.sub(o)['hp_hz'] for o in roles if o not in lows):
            e = self.settings.sub(lows[0])                # 필터를 나눠 쓰는 묶음이면 묶음째 인프라(10-12)
            return lows[0], float(e['lp_hz']), str(e['lp_type'])
        return '', 0.0, cal.SUB_BAND_TYPES[0]

    def set_infra(self, role: str, hz: float = 25.0, typ: str = 'LR24') -> None:
        """인프라 XO 를 정한다 — role 이 빈 글자면 끈다(전에 인프라 XO 로 넣은 LP·HP 를 지운다). 화면 조작과 같은 길."""
        before, old_hz, _ = self.infra_state()
        roles = self.sub_roles()
        if not role:
            if before:
                for r in roles:
                    if r == before:
                        self.settings.set_sub(r, lp_hz=0.0)
                    elif self.settings.sub(r)['hp_hz'] == old_hz:
                        self.settings.set_sub(r, hp_hz=0.0)
                self._changed_setting()
            return
        hz = float(min(max(hz, cal.SUB_FILTER_RANGE[0]), cal.SUB_FILTER_RANGE[1]))
        # 인프라가 필터를 나눠 쓰는 묶음의 서브면 묶음째 인프라 — 한 서브씩 고치면 묶음 공유가 LP · HP 를 서로 옮겨 적어
        # 둘 다 HP + LP 가 됐고, 인프라를 다른 서브로 옮겨도 그 LP 가 남았다(10-12 돌비룸: 묶음 SW1 · SW2 에 LP 25 Hz → BA +51 dB).
        e = self.settings.sub(role)
        group = set(self.settings.bundle_members(e['bundle'])) if e['mode'] == 'bundle' else set()
        group = (group if self.settings.layout(e['bundle']) != 'values' else set()) | {role}
        for r in roles:
            if r in group:
                self.settings.set_sub(r, lp_hz=hz, lp_type=typ, hp_hz=0.0)
            else:
                # 인프라 XO 아래 · 근처의 LP 는 위 대역을 막는다 — 전에 인프라로 넣은 LP 로 보고 지운다
                lp = self.settings.sub(r)['lp_hz']
                clear = r == before or (lp and lp < 2.0 * max(hz, old_hz or hz))
                self.settings.set_sub(r, hp_hz=hz, hp_type=typ, **({'lp_hz': 0.0} if clear else {}))
        self._changed_setting()

    def _on_boost(self, value: float) -> None:
        if self._syncing:
            return
        self.settings.boost_max_db = float(value)
        self._changed_setting()

    def _on_xo_peq(self, *_args) -> None:
        if self._syncing:
            return
        self.settings.xo_peq_on = bool(self.xo_peq_check.isChecked())
        self.settings.xo_peq_lo = float(self.xo_peq_lo_spin.value())
        self.settings.xo_peq_hi = float(self.xo_peq_hi_spin.value())
        self._changed_setting()

    def _on_infra_boost(self, value: float) -> None:
        if self._syncing:
            return
        self.settings.infra_boost_max_db = float(value)
        self._changed_setting()

    def _on_infra(self) -> None:
        if self._syncing:
            return
        role = str(self.infra_combo.currentData() or '')
        _, hz, _ = self.infra_state()
        hz = self.infra_hz_spin.value() if self.infra_hz_spin.value() else (hz or 25.0)
        self.set_infra(role, hz, str(self.infra_type_combo.currentData() or cal.SUB_BAND_TYPES[0]))

    def _refresh_infra(self, roles: list[str]) -> None:
        items = [('', t('없음'))] + [(r, r) for r in roles]
        if [(self.infra_combo.itemData(i), self.infra_combo.itemText(i)) for i in range(self.infra_combo.count())] != items:
            self.infra_combo.clear()
            for role, text in items:
                self.infra_combo.addItem(text, role)
        role, hz, typ = self.infra_state()
        self.infra_combo.setCurrentIndex(max(0, self.infra_combo.findData(role)))
        for w in (self.infra_xo_label, self.infra_hz_spin, self.infra_type_combo, self.infra_note,
                  self.infra_boost_label, self.infra_boost_spin):
            w.setVisible(bool(role))
        self.infra_boost_spin.setValue(float(self.settings.infra_boost_max_db))
        if role:
            self.infra_hz_spin.setValue(hz)
            self.infra_type_combo.setCurrentIndex(max(0, self.infra_type_combo.findData(typ)))
            others = ', '.join(r for r in roles if r != role)
            self.infra_note.setText(t('{infra}: {hz:g} Hz 아래만 · {others}: {hz:g} Hz 위만').format(
                infra=role, hz=hz, others=others))

    def _layout_rows(self, grid: QtWidgets.QGridLayout, row: int) -> None:
        """묶음 레이아웃 줄(10-09 R30) — 묶음일 때만 보인다: 레이아웃(상대값 고정 · DBA · 멀티서브, 묶음마다), DBA 줄(앞·뒤,
        서브마다), 뒤 지연 보정(ms, 묶음마다 — 자동 전파 지연에 더함). 레이아웃 묶음은 필터 칸을 고치면 묶음 전부에 옮겨진다."""
        self.sub_layout_label = label('레이아웃', 'note')
        grid.addWidget(self.sub_layout_label, row, 0)
        self.sub_layout_combo = QtWidgets.QComboBox()
        for key, text in SUB_LAYOUT_ITEMS:
            self.sub_layout_combo.addItem(text, key)
        self.sub_layout_combo.currentIndexChanged.connect(self._on_sub_layout)
        attach(self.sub_layout_combo, 'cal.sub_layout')
        grid.addWidget(self.sub_layout_combo, row, 1, 1, 2)
        self.sub_row_label = label('DBA 줄', 'note')
        grid.addWidget(self.sub_row_label, row + 1, 0)
        self.sub_row_combo = QtWidgets.QComboBox()
        self.sub_row_combo.addItem('앞', 'front')
        self.sub_row_combo.addItem('뒤', 'rear')
        self.sub_row_combo.currentIndexChanged.connect(self._on_sub_row)
        attach(self.sub_row_combo, 'cal.sub_dba_row')
        grid.addWidget(self.sub_row_combo, row + 1, 1, 1, 2)
        self.sub_trim_label = label('뒤 지연 보정', 'note', wrap=True)
        grid.addWidget(self.sub_trim_label, row + 2, 0)
        self.sub_trim_spin = self._spin(*cal.DBA_TRIM_RANGE, 0.1, ' ms', 2)
        self.sub_trim_spin.valueChanged.connect(self._on_sub_trim)
        attach(self.sub_trim_spin, 'cal.sub_dba_trim')
        grid.addWidget(self.sub_trim_spin, row + 2, 2)

    def _on_sub_layout(self, _index: int) -> None:
        if self._syncing or not self.sub_role:
            return
        e = self.settings.sub(self.sub_role)
        if e['mode'] != 'bundle':
            return
        self.settings.set_layout(e['bundle'], kind=str(self.sub_layout_combo.currentData() or 'values'))
        self._changed_setting()

    def _on_sub_row(self, _index: int) -> None:
        if self._syncing or not self.sub_role:
            return
        self.settings.set_sub(self.sub_role, dba_row=str(self.sub_row_combo.currentData() or 'front'))
        self._changed_setting()

    def _on_sub_trim(self, value: float) -> None:
        if self._syncing or not self.sub_role:
            return
        e = self.settings.sub(self.sub_role)
        if e['mode'] == 'bundle':
            self.settings.set_layout(e['bundle'], rear_trim_ms=float(value))
            self._changed_setting()

    @staticmethod
    def _small_spin(lo: float, hi: float, decimals: int, suffix: str, key: str, zero_text: bool = False,
                    prefix: str = '') -> QtWidgets.QDoubleSpinBox:
        """BA 설정 카드의 좁은 칸(단추 없음) — 올패스·고정 필터. zero_text 면 주파수 칸이라 0 에 '— Hz'(비었지만 주파수 칸인 것이
        보이게 — 10-05 리드: '없음' 만 보여 무슨 칸인지 몰랐다)."""
        spin = QtWidgets.QDoubleSpinBox()
        spin.setRange(lo, hi)
        spin.setDecimals(decimals)
        spin.setSuffix(suffix)
        spin.setPrefix(prefix)
        spin.setKeyboardTracking(False)
        spin.setButtonSymbols(QtWidgets.QAbstractSpinBox.ButtonSymbols.NoButtons)
        spin.setMinimumWidth(56)
        spin.setAlignment(QtCore.Qt.AlignmentFlag.AlignRight)
        if zero_text:
            spin.setSpecialValueText('— Hz')
        attach(spin, key)
        return spin

    def _mimo_card(self) -> QtWidgets.QFrame:
        frame, box = card('MIMO (저역 감쇠)')
        self.mimo_check = QtWidgets.QCheckBox(t('MIMO 사용'))
        self.mimo_check.toggled.connect(self._on_mimo_on)
        attach(self.mimo_check, 'cal.mimo_on')
        box.addWidget(self.mimo_check)
        grid = QtWidgets.QGridLayout()
        grid.setHorizontalSpacing(8)
        grid.setVerticalSpacing(6)
        fsiso_name = label('서포트 주파수 상한', 'note')      # 10-11 사용자: 'Fsiso 상한' → '서포트 주파수 상한'
        attach(fsiso_name, 'cal.fsiso')
        grid.addWidget(fsiso_name, 0, 0, 1, 2)          # 0 행 1 열은 비어 있다 — 긴 이름이 1024 폭에서 잘리지 않게 두 칸에 걸침
        self.fsiso_spin = self._spin(*cal.FSISO_RANGE, 5.0, ' Hz')
        self.fsiso_spin.valueChanged.connect(self._on_fsiso)
        attach(self.fsiso_spin, 'cal.fsiso')
        grid.addWidget(self.fsiso_spin, 0, 2)
        grid.addWidget(label('하한', 'note'), 1, 0)
        self.mimo_low_auto = QtWidgets.QCheckBox(t('자동'))
        self.mimo_low_auto.toggled.connect(self._on_mimo_low_auto)
        attach(self.mimo_low_auto, 'cal.mimo_low_auto')
        grid.addWidget(self.mimo_low_auto, 1, 1)
        self.mimo_low_spin = self._spin(15.0, cal.FSISO_RANGE[1], 5.0, ' Hz')
        self.mimo_low_spin.valueChanged.connect(self._on_mimo_low)
        attach(self.mimo_low_spin, 'cal.mimo_low')
        grid.addWidget(self.mimo_low_spin, 1, 2)
        # 입력당 서포트 상한(10-05 R28 — 테시라 DSP 예산으로 입력당 4갈래가 기준). 0 = 제한 없음.
        grid.addWidget(label('입력당 최대', 'note'), 2, 0)
        self.mimo_max_spin = QtWidgets.QSpinBox()
        self.mimo_max_spin.setRange(*cal.MIMO_MAX_SUPPORTS_RANGE)
        self.mimo_max_spin.setSuffix(str(t(' 서포트')))
        self.mimo_max_spin.setKeyboardTracking(False)
        self.mimo_max_spin.setMinimumWidth(96)
        self.mimo_max_spin.valueChanged.connect(self._on_mimo_max)
        attach(self.mimo_max_spin, 'cal.mimo_max')
        grid.addWidget(self.mimo_max_spin, 2, 2)
        # 서포트 필터(10-12 사용자: 체크박스 'FIR 사용', 서포트당 8,000탭). 끄면 IIR(PEQ 9 + 올패스 3).
        grid.addWidget(label('서포트 필터', 'note'), 3, 0)
        self.mimo_fir_check = QtWidgets.QCheckBox(t('FIR 사용'))
        self.mimo_fir_check.toggled.connect(self._on_mimo_fir)
        attach(self.mimo_fir_check, 'cal.mimo_fir')
        grid.addWidget(self.mimo_fir_check, 3, 1, 1, 2)
        grid.setColumnStretch(1, 1)
        box.addLayout(grid)
        self.mimo_reason = label('', 'note', wrap=True)
        box.addWidget(self.mimo_reason)
        self.mimo_button = QtWidgets.QPushButton(t('MIMO 계산'))
        set_variant(self.mimo_button, 'ghost', 'sm')
        self.mimo_button.clicked.connect(lambda: self.compute_mimo())
        attach(self.mimo_button, 'cal.mimo_compute')
        row = QtWidgets.QHBoxLayout()
        row.addWidget(self.mimo_button)
        # 카드마다 취소(10-09 사용자: 보정 설계에 취소 버튼이 없다 — 위 모드 카드의 것은 스크롤 밖이었다)
        self.mimo_cancel_button = QtWidgets.QPushButton(t('계산 취소'))
        set_variant(self.mimo_cancel_button, 'ghost', 'sm')
        self.mimo_cancel_button.clicked.connect(self.cancel)
        attach(self.mimo_cancel_button, 'cal.cancel')
        row.addWidget(self.mimo_cancel_button)
        row.addStretch(1)
        box.addLayout(row)
        self.mimo_status = label('', 'note', wrap=True)
        box.addWidget(self.mimo_status)
        return frame

    # ──────────────────────────── 문맥 ────────────────────────────
    def set_context(self, project_path=None, irs_provider=None) -> None:
        """창이 넘긴다: 프로젝트 파일 위치(내보낼 폴더 기본값)와 측정 IR 을 읽는 함수(워커 스레드에서 부른다)."""
        self.project_path = Path(project_path) if project_path else None
        if irs_provider is not None:
            self.irs_provider = irs_provider
        if self.run is None and self._job is None:
            self.restore_cached_run()
        self.refresh()

    def restore_cached_run(self) -> bool:
        """프로젝트 옆에 저장된 지난 계산 결과를 되살린다(설정이 바뀌었으면 화면이 '낡음' 으로 알린다). 되살렸으면 True."""
        cached = cal.load_run_cache(self.project_path)
        if cached is None:
            return False
        self.run, self.mimo_run = cached
        self._fill_channels()
        return True

    def _save_cache(self) -> None:
        """뒤 스레드에서 쓴다 — 엔진 스펙트럼까지 들어 수백 MB 라 화면이 멈추면 안 된다."""
        path, run, mimo = self.project_path, self.run, self.mimo_run

        def work() -> None:
            try:
                cal.save_run_cache(path, run, mimo)
            except Exception:  # noqa: BLE001 - 캐시는 덤 — 못 써도 화면은 그대로
                pass
        self._cache_thread = threading.Thread(target=work, name='secs-calibration-cache', daemon=True)
        self._cache_thread.start()

    def positions(self) -> list[int]:
        return cal.measured_positions(self.project)

    def mimo_available(self) -> tuple[bool, str]:
        """위치 수는 계산 결과가 있으면 그 엔진이 쓴 위치, 없으면 기록된 측정 위치."""
        count = len(self.run.engine.positions) if self.run is not None else len(self.positions())
        return cal.mimo_availability(count)

    # ──────────────────────────── 설정 바꾸기 ────────────────────────────
    def _changed_setting(self) -> None:
        self.settings.store(self.project)
        self.refresh()
        self.changed.emit()

    def set_target_preset(self, key: str) -> None:
        """타겟 프리셋(플랫·하만·시네마)을 넣는다 — 결과가 낡았다고만 보이고 계산은 하지 않는다(10-09 사용자 결정)."""
        if apply_preset(self.project, key):
            self._target_edited()

    # ──────────────────────────── 타겟 커브: SPL 그래프 위 편집(10-09) ────────────────────────────
    def save_target(self, path: str | None = None) -> str | None:
        """타겟 커브를 JSON 으로 저장(옛 6단계와 같은 꼴)."""
        if path is None:
            path, _ = QtWidgets.QFileDialog.getSaveFileName(self, t('타겟 커브 저장'), '타겟.json', 'JSON (*.json)')
        if not path:
            return None
        Path(path).write_text(json.dumps(self.project.target.to_dict(), ensure_ascii=False, indent=2), encoding='utf-8')
        return str(path)

    def load_target(self, path: str | None = None) -> bool:
        if path is None:
            path, _ = QtWidgets.QFileDialog.getOpenFileName(self, t('타겟 커브 불러오기'), '', 'JSON (*.json)')
        if not path:
            return False
        try:
            points = TargetCurve.from_dict(json.loads(Path(path).read_text(encoding='utf-8'))).points
            if len(points) < 2:
                raise ValueError('points < 2')
        except Exception:  # noqa: BLE001 - 잘못된 파일은 무시(타겟 그대로)
            return False
        self.project.target.points = list(points)
        self._target_edited()
        return True

    def set_target_point(self, index: int, freq: float, db: float) -> None:
        """제어점 하나를 옮긴다(Hz · 타겟 dB). 순서가 바뀌면 주파수로 다시 정렬한다."""
        points = [list(p) for p in self.project.target.points]
        if not 0 <= index < len(points):
            return
        points[index] = [float(np.clip(freq, 10.0, 24000.0)), float(np.clip(db, -30.0, 30.0))]
        self.project.target.points = sorted((float(f), float(d)) for f, d in points)
        self.project.bc['target_preset'] = ''          # 손으로 고쳤다 = 사용자 지정
        self._target_edited()

    def add_target_point(self, freq: float, db: float) -> None:
        points = list(self.project.target.points) + [(float(freq), float(db))]
        self.project.target.points = sorted((float(f), float(d)) for f, d in points)
        self.project.bc['target_preset'] = ''          # 손으로 고쳤다 = 사용자 지정
        self._target_edited()

    def remove_target_point(self, index: int) -> None:
        points = list(self.project.target.points)
        if len(points) <= 2 or not 0 <= index < len(points):
            return                                       # 타겟은 점 2개 이상
        del points[index]
        self.project.target.points = points
        self.project.bc['target_preset'] = ''          # 손으로 고쳤다 = 사용자 지정
        self._target_edited()

    def _draw_target_handles(self, offset: float) -> None:
        """SPL 탭: 타겟 제어점을 끌 수 있는 손잡이로 올린다. `offset` = 화면 곡선 − 타겟(dB, 레벨 기준)."""
        self._target_offset = float(offset)
        self._target_handles = []
        for index, (freq, db) in enumerate(sorted(self.project.target.points)):
            handle = pg.TargetItem(pos=(np.log10(float(freq)), float(db) + offset), size=11, movable=True,
                                   pen=pg.mkPen(C_TARGET, width=2), brush=pg.mkBrush(COLORS['plot_bg']))
            handle.setZValue(20)
            handle.setToolTip(f'{freq:g} Hz · {db:+.1f} dB')
            handle.sigPositionChangeFinished.connect(lambda item, i=index: self._on_handle_moved(i, item))
            handle.mouseClickEvent = lambda ev, i=index: self._on_handle_click(i, ev)
            self.plot.addItem(handle)
            self._target_handles.append(handle)

    def _on_handle_moved(self, index: int, item) -> None:
        pos = item.pos()
        self.set_target_point(index, 10 ** float(pos.x()), float(pos.y()) - self._target_offset)

    def _on_handle_click(self, index: int, ev) -> None:
        if ev.button() == QtCore.Qt.MouseButton.RightButton:
            ev.accept()
            self.remove_target_point(index)

    def target_menu(self) -> QtWidgets.QMenu:
        """SPL 그래프 빈 곳 우클릭 메뉴(10-09 사용자: 타겟 커브 카드를 없애고 우클릭으로) — 프리셋 3개(지금 것에 체크, 손으로
        고쳤으면 '사용자 지정' 에 체크) · 저장… · 불러오기…. 테스트가 띄우지 않고 항목을 누를 수 있게 메뉴만 만들어 돌려준다."""
        menu = _PlainQt.QMenu(self)
        head = menu.addAction(str(t('타겟 커브')))
        head.setEnabled(False)
        preset = preset_of(self.project)
        for key, text in TARGET_PRESETS:
            action = menu.addAction(str(t(text)))
            action.setCheckable(True)
            action.setChecked(key == preset)
            action.setData(key)
            action.triggered.connect(lambda _=False, k=key: self.set_target_preset(k))
        custom = menu.addAction(str(t('사용자 지정')))
        custom.setCheckable(True)
        custom.setChecked(not preset)
        custom.setData('custom')
        custom.setEnabled(False)                     # 표시만 — 점을 끌거나 불러오면 켜진다
        menu.addSeparator()
        menu.addAction(str(t('타겟 저장…')), lambda: self.save_target())
        menu.addAction(str(t('타겟 불러오기…')), lambda: self.load_target())
        return menu

    def _on_plot_click(self, ev) -> None:
        """그래프 클릭 — 빈 곳 우클릭 = 타겟 메뉴, 빈 곳 더블클릭 = 점 추가(SPL 탭만). 점 우클릭은 손잡이가 먼저 받아 지운다."""
        if self.tab != 'spl':
            return
        if ev.button() == QtCore.Qt.MouseButton.RightButton:
            if ev.isAccepted():                          # 점 위 우클릭(손잡이가 지웠다)
                return
            ev.accept()
            self.target_menu().exec(QtGui.QCursor.pos())
            return
        self._on_plot_double_click(ev)

    def _on_plot_double_click(self, ev) -> None:
        if self.tab != 'spl' or not ev.double():
            return
        point = self.plot.getPlotItem().vb.mapSceneToView(ev.scenePos())
        ev.accept()
        self.add_target_point(10 ** float(point.x()), float(point.y()) - self._target_offset)

    def _target_edited(self) -> None:
        self.refresh()
        self.changed.emit()

    def set_mode(self, key: str) -> None:
        allowed = cal.MODE_KEYS + ((cal.ULTRA_MODE,) if self._ultra_unlocked else ())
        if key not in allowed or key == self.settings.mode:
            self._refresh_controls()
            return
        self.settings.mode = key
        self._changed_setting()

    ULTRA_CLICKS = 5                   # 숨은 버튼: 최고사양 모드에서 사양 줄을 이만큼 연달아(`ULTRA_CLICK_S` 안에) 누르면
    ULTRA_CLICK_S = 3.0

    def eventFilter(self, obj, event) -> bool:  # noqa: N802 - Qt 이름
        # 빠르게 누르면 두 번째 누름은 MouseButtonDblClick 으로 온다 — 그것도 센다(10-11 사용자: 빨리 누르면 초고지연이 안 열렸다)
        if obj is getattr(self, 'mode_spec', None) and event.type() in (QtCore.QEvent.Type.MouseButtonPress,
                                                                         QtCore.QEvent.Type.MouseButtonDblClick):
            self.spec_clicked()
        return super().eventFilter(obj, event)

    def spec_clicked(self) -> None:
        """사양 줄 클릭 — 최고사양 모드에서 5번 연달아 누르면 초고지연 모드를 열고 고른다(이스터에그, 09-27)."""
        if self.settings.mode != 'linear' or self._ultra_unlocked:
            self._spec_clicks = []
            return
        now = time.monotonic()
        self._spec_clicks = [c for c in self._spec_clicks if now - c <= self.ULTRA_CLICK_S] + [now]
        if len(self._spec_clicks) >= self.ULTRA_CLICKS:
            self._spec_clicks = []
            self._ultra_unlocked = True
            self.set_mode(cal.ULTRA_MODE)

    def _on_ultra_ms(self, value: float) -> None:
        if self._syncing:
            return
        self.settings.ultra_ms = float(value)
        self._changed_setting()

    def _on_rc_lo(self, value: float) -> None:
        if self._syncing:
            return
        self.settings.rc_lo = float(value)
        self._changed_setting()

    def _on_rc_hi_auto(self, on: bool) -> None:
        if self._syncing:
            return
        self.settings.rc_hi = None if on else float(self.rc_hi_spin.value())
        self._changed_setting()

    def _on_rc_hi(self, value: float) -> None:
        if self._syncing or self.settings.rc_hi is None:
            return
        self.settings.rc_hi = float(value)
        self._changed_setting()

    def _on_volume(self, value: float) -> None:
        if self._syncing:
            return
        self.settings.room_volume_m3 = float(value) if value > 0 else None
        # 부피를 직접 고쳤다 — 치수의 곱과 다르면 치수를 지운다(맞지 않는 치수가 저장돼 남지 않게).
        made = cal.room_volume(self.settings.room_dims_m)
        if made is None or self.settings.room_volume_m3 is None or abs(made - self.settings.room_volume_m3) > 0.5:
            self.settings.room_dims_m = None
        self._changed_setting()

    def _on_dims(self, _value: float) -> None:
        """가로·세로·높이 — 셋 다 있으면 부피 = 곱(부피 칸도 바뀐다). 하나라도 비우면 부피는 그대로 둔다."""
        if self._syncing:
            return
        dims = cal.room_dims([spin.value() for spin in self.dim_spins])
        self.settings.room_dims_m = dims
        if dims is not None:
            self.settings.room_volume_m3 = min(round(cal.room_volume(dims), 1), 100000.0)
        self._changed_setting()

    def set_room(self, volume_m3: float | None = None, dims_m=None) -> None:
        """룸 볼륨 또는 치수를 화면 조작과 같은 길로 넣는다(테스트·도구). `dims_m` 을 주면 부피는 곱으로."""
        if dims_m is not None:
            for spin, value in zip(self.dim_spins, dims_m):
                spin.setValue(float(value))
            return
        self.volume_spin.setValue(float(volume_m3 or 0.0))

    def _on_mimo_on(self, on: bool) -> None:
        if self._syncing:
            return
        self.settings.mimo_on = bool(on)
        self._changed_setting()

    def _on_fsiso(self, value: float) -> None:
        if self._syncing:
            return
        self.settings.fsiso = float(value)
        self._changed_setting()

    def _on_mimo_low_auto(self, on: bool) -> None:
        if self._syncing:
            return
        self.settings.mimo_low = None if on else float(self.mimo_low_spin.value())
        self._changed_setting()

    def _on_mimo_low(self, value: float) -> None:
        if self._syncing or self.settings.mimo_low is None:
            return
        self.settings.mimo_low = float(value)
        self._changed_setting()

    def _on_mimo_max(self, value: int) -> None:
        if self._syncing:
            return
        self.settings.mimo_max_supports = int(value)
        self._changed_setting()

    def _on_mimo_fir(self, on: bool) -> None:
        if self._syncing:
            return
        self.settings.mimo_fir = bool(on)
        self._changed_setting()

    # ──────────────────────────── BA 설정(R27) ────────────────────────────
    def sub_roles(self) -> list[str]:
        """구성의 서브 역할(측정 전에도 고를 수 있게 구성 기준)."""
        return [str(getattr(s, 'role', s)) for s in self.project.config.groups().get('sub', [])]

    def set_sub_role(self, role: str) -> None:
        index = self.sub_pick.findData(role)
        if index >= 0:
            self.sub_pick.setCurrentIndex(index)

    def set_sub_mode(self, role: str, mode: str) -> None:
        """방식을 바꾼다 — mode = 'auto' · 'fixed' · 'bundle:A'(화면 조작과 같은 길: 저장되고 결과가 낡는다)."""
        kind, _, bundle = mode.partition(':')
        values = {'mode': kind}
        if bundle:
            values['bundle'] = bundle
        self.settings.set_sub(role, **values)
        self._changed_setting()

    def _on_sub_pick(self, _index: int) -> None:
        if self._syncing:
            return
        self.sub_role = str(self.sub_pick.currentData() or '')
        self._refresh_controls()

    def _on_sub_mode(self, _index: int) -> None:
        if self._syncing or not self.sub_role:
            return
        self.set_sub_mode(self.sub_role, str(self.sub_mode_combo.currentData() or 'auto'))

    def _on_sub_value(self, **values) -> None:
        """게인·딜레이·극성 칸 하나 — 바뀐 칸만 넣는다(나머지 칸의 표시 반올림이 [결과 값 가져오기]의 값을 깎지 않게)."""
        if self._syncing or not self.sub_role:
            return
        self.settings.set_sub(self.sub_role, **values)
        self._changed_setting()

    def _on_sub_allpass(self) -> None:
        if self._syncing or not self.sub_role:
            return
        allpass = [[f.value(), q.value()] for f, q in self.sub_ap_spins if f.value() > 0]
        self.settings.set_sub(self.sub_role, allpass=allpass)
        self._changed_setting()

    def _on_sub_band_type(self, kind: str) -> None:
        """HP·LP 종류 칸 — 끔이면 주파수 0, 종류를 고르면 그 종류로 켠다(주파수가 비어 있으면 `SUB_FILTER_DEFAULT_HZ`).
        방식과 상관없이 저장되고 결과가 낡는다."""
        if self._syncing or not self.sub_role:
            return
        typ = str(self.sub_band_combos[kind].currentData() or 'off')
        if typ == 'off':
            self.settings.set_sub(self.sub_role, **{f'{kind}_hz': 0.0})
        else:
            hz = self.settings.sub(self.sub_role)[f'{kind}_hz'] or cal.SUB_FILTER_DEFAULT_HZ[kind]
            self.settings.set_sub(self.sub_role, **{f'{kind}_hz': hz, f'{kind}_type': typ})
        self._changed_setting()

    def _on_sub_band_hz(self, kind: str) -> None:
        if self._syncing or not self.sub_role:
            return
        self.settings.set_sub(self.sub_role, **{f'{kind}_hz': getattr(self, f'sub_{kind}_spin').value()})
        self._changed_setting()

    def _on_sub_peq(self) -> None:
        """고정 PEQ 칸 — 주파수가 빈 밴드는 없는 것."""
        if self._syncing or not self.sub_role:
            return
        peq = [[f.value(), q.value(), g.value()] for f, q, g in self.sub_peq_spins if f.value() > 0]
        self.settings.set_sub(self.sub_role, peq=peq)
        self._changed_setting()

    def _on_sub_mimo(self, on: bool) -> None:
        if self._syncing or not self.sub_role:
            return
        self.settings.set_sub(self.sub_role, mimo_support=bool(on))
        self._changed_setting()

    def _on_sub_main_delay(self, value: float) -> None:
        if self._syncing:
            return
        self.settings.sub_main_delay_ms = float(value)
        self._changed_setting()

    def take_sub_values(self) -> bool:
        """[결과 값 가져오기] — 마지막 결과의 서브 값을 모든 서브의 수동 칸에(방식은 그대로, `take_sub_values`)."""
        if self.run is None or not self.run.result.subs:
            return False
        self.settings.take_sub_values(self.run)
        self._changed_setting()
        return True

    def _refresh_sub_controls(self) -> None:
        """BA 설정 카드(`_refresh_controls` 안 — 이미 `_syncing`)."""
        roles = self.sub_roles()
        self.sub_card.setVisible(len(roles) >= 2 and self.tab == 'subs')   # 탭에 맞춰(`_show_tab_cards`)
        # 서브는 측정 이름(SW1·SW2…)으로 — LFE 1·2·3 은 측정 파일·REW 이름과 달라 헷갈린다(10-09 사용자).
        shown = [(r, r) for r in roles]
        if [(self.sub_pick.itemText(i), self.sub_pick.itemData(i)) for i in range(self.sub_pick.count())] != shown:
            self.sub_pick.clear()
            for text, role in shown:
                self.sub_pick.addItem(text, role)
                self.sub_pick.setItemData(self.sub_pick.count() - 1, role_label(role), QtCore.Qt.ItemDataRole.ToolTipRole)
        self._refresh_infra(roles)
        if self.sub_role not in roles:
            self.sub_role = roles[0] if roles else ''
        if not self.sub_role:
            return
        self.sub_pick.setCurrentIndex(max(0, self.sub_pick.findData(self.sub_role)))
        e = self.settings.sub(self.sub_role)
        key = f"bundle:{e['bundle']}" if e['mode'] == 'bundle' else e['mode']
        self.sub_mode_combo.setCurrentIndex(max(0, self.sub_mode_combo.findData(key)))
        layout = self.settings.layout(e['bundle']) if e['mode'] == 'bundle' else 'values'
        manual = e['mode'] != 'auto' and layout == 'values'     # 레이아웃 묶음(R30)은 값을 엔진이 정한다
        self.sub_layout_label.setVisible(e['mode'] == 'bundle')
        self.sub_layout_combo.setVisible(e['mode'] == 'bundle')
        self.sub_layout_combo.setCurrentIndex(max(0, self.sub_layout_combo.findData(layout)))
        for w in (self.sub_row_label, self.sub_row_combo, self.sub_trim_label, self.sub_trim_spin):
            w.setVisible(layout == 'dba')
        self.sub_row_combo.setCurrentIndex(0 if e['dba_row'] == 'front' else 1)
        if layout == 'dba':
            self.sub_trim_spin.setValue(float(self.settings.bundle_layouts[e['bundle']]['rear_trim_ms']))
        self.sub_gain_spin.setValue(float(e['gain_db']))
        self.sub_delay_spin.setValue(float(e['delay_ms']))
        self.sub_pol_combo.setCurrentIndex(0 if e['polarity'] > 0 else 1)
        for k, (f_spin, q_spin) in enumerate(self.sub_ap_spins):
            f0, q = e['allpass'][k] if k < len(e['allpass']) else (0.0, 0.71)
            f_spin.setValue(float(f0))
            q_spin.setValue(float(q))
        for widget in (self.sub_gain_spin, self.sub_delay_spin, self.sub_pol_combo,
                       *[w for pair in self.sub_ap_spins for w in pair]):
            widget.setEnabled(manual)
        for kind, combo in self.sub_band_combos.items():
            hz = float(e[f'{kind}_hz'])
            spin = getattr(self, f'sub_{kind}_spin')
            spin.setValue(hz)
            spin.setEnabled(hz > 0)                         # 끔이면 주파수 칸도 꺼진다 — 종류 칸에서 켠다
            combo.setCurrentIndex(max(0, combo.findData(e[f'{kind}_type'] if hz > 0 else 'off')))
        for k, (f_spin, q_spin, g_spin) in enumerate(self.sub_peq_spins):
            f0, q, g = e['peq'][k] if k < len(e['peq']) else (0.0, 1.0, 0.0)
            f_spin.setValue(float(f0))
            q_spin.setValue(float(q))
            g_spin.setValue(float(g))
        self.sub_mimo_check.setChecked(bool(e['mimo_support']))
        fixed = any(self.settings.sub(r)['mode'] == 'fixed' for r in roles)
        self.sub_main_label.setVisible(fixed)
        self.sub_main_spin.setVisible(fixed)
        self.sub_main_spin.setValue(float(self.settings.sub_main_delay_ms))
        self.sub_take_button.setEnabled(self.run is not None and bool(self.run.result.subs) and self._job is None)
        self.sub_compute_button.setEnabled(self._job is None and self.irs_provider is not None and bool(self.positions()))
        warns = cal.sub_filter_warnings(self.settings, roles)
        self.sub_warn_label.setText('\n'.join(warns))
        self.sub_warn_label.setVisible(bool(warns))
        if self._job is not None and self._job['kind'] == 'subs':
            self.sub_summary.setText(_join(['BA 계산 중…', _remaining_text(self.subs_estimate_seconds() - self.elapsed_seconds())]))
            return
        words = dict(SUB_MODE_ITEMS)
        self.sub_summary.setText(_join(
            f"{r} {t(words[self._sub_mode_key(r)])}" + self._sub_layout_text(r) + self._sub_filter_text(r)
            + ('' if self.settings.sub(r)['mimo_support'] else f" ({t('MIMO 제외')})") for r in roles))

    def _sub_filter_text(self, role: str) -> str:
        """요약 줄의 고정 출력 필터 조각(' HP 25 · LP 40 · PEQ 2' 꼴, 없으면 빈 글자)."""
        e = self.settings.sub(role)
        parts = [f"{kind.upper()} {e[kind + '_hz']:g}" + ('' if e[kind + '_type'] == cal.SUB_BAND_TYPES[0]
                                                             else f" {cal.band_type_text(e[kind + '_type'])}")
                 for kind in ('hp', 'lp') if e[kind + '_hz']] + ([f"PEQ {len(e['peq'])}"] if e['peq'] else [])
        return f" [{' '.join(parts)}]" if parts else ''

    def _sub_layout_text(self, role: str) -> str:
        """요약 줄의 레이아웃 조각(' DBA 뒤' · ' 멀티', R30 — 상대값 고정 묶음·자동·고정이면 빈 글자)."""
        e = self.settings.sub(role)
        layout = self.settings.layout(e['bundle']) if e['mode'] == 'bundle' else 'values'
        if layout == 'dba':
            return ' ' + t('DBA 뒤' if e['dba_row'] == 'rear' else 'DBA 앞')
        return ' ' + t('멀티') if layout == 'multi' else ''

    def _sub_mode_key(self, role: str) -> str:
        e = self.settings.sub(role)
        return f"bundle:{e['bundle']}" if e['mode'] == 'bundle' else e['mode']

    # ──────────────────────────── XO ────────────────────────────
    def xo_group_names(self) -> list[str]:
        """XO 가 있는 그룹(구성에 서브가 있을 때 메인 그룹 전부)."""
        groups = self.project.config.groups()
        if not groups.get('sub'):
            return []
        return [str(g) for g in groups if g != 'sub']

    def xo_of(self, group: str) -> float:
        """그룹 XO — 고른 값 → 옛 7단계 값 → 80 Hz(엔진 입력 `calibration.engine_inputs` 와 같은 순서)."""
        return float(self.settings.xo.get(group) or (self.project.xo or {}).get(group) or cal.DEFAULT_XO_HZ)

    def set_xo(self, group: str, hz: float) -> None:
        """그룹 XO 를 바꾼다(화면 조작과 같은 길 — 저장되고 결과가 낡는다, XO 보기는 바로 따라온다)."""
        hz = float(round(min(max(float(hz), cal.XO_RANGE[0]), cal.XO_RANGE[1])))
        if group not in self.xo_group_names() or self.settings.xo.get(group) == hz:
            return
        self.settings.xo[group] = hz
        self._changed_setting()

    def set_xo_group(self, group: str) -> None:
        index = self.xo_group_combo.findData(group)
        if index >= 0:
            self.xo_group_combo.setCurrentIndex(index)

    def _on_xo_group(self, _index: int) -> None:
        if self._syncing:
            return
        self.xo_group = str(self.xo_group_combo.currentData() or '')
        self._refresh_controls()
        if self.tab == 'xo':
            self._refresh_result()

    def _on_xo_hz(self, value: float) -> None:
        if self._syncing or not self.xo_group:
            return
        self.set_xo(self.xo_group, value)

    def _on_xo_line_moving(self) -> None:
        """XO 선을 끄는 중 — 곡선만 다시(설정은 놓을 때)."""
        if self._syncing or self.tab != 'xo':
            return
        hz = float(np.clip(10 ** self.xo_line.value(), *cal.XO_RANGE))
        self._draw_xo(hz, update=True)
        self._syncing = True
        try:
            self.xo_spin.setValue(round(hz))
        finally:
            self._syncing = False

    def _on_xo_line_done(self) -> None:
        if self._syncing or not self.xo_group:
            return
        self.set_xo(self.xo_group, 10 ** self.xo_line.value())

    def xo_running(self) -> bool:
        return self._job is not None and self._job['kind'] == 'xo'

    def recommend_xo(self) -> bool:
        """[XO 추천 계산] — 후보(`cal.XO_CANDIDATES`)마다 보정을 끝까지 계산해 그룹별로 고른다(후보 수 × 보정 한 번).
        끝나면 추천값을 설정에 넣고 그 XO 로 보정을 다시 계산한다."""
        if self._job is None and self._demo_mode():
            return self._fake_compute()                    # 튜토리얼은 XO 추천(수 분)도 계산하지 않는다(10-11)
        if self._job is not None or self.irs_provider is None or not self.positions() or not self.xo_group_names():
            if not self.xo_group_names():
                self._xo_error = '서브가 없어 XO 가 없습니다'
            self._refresh_status()
            return False
        project = copy.deepcopy(self.project)
        settings = copy.deepcopy(self.settings)
        provider = self.irs_provider
        bridge = self._bridge
        holder: dict = {}

        def work():
            return cal.recommend_xo(project, provider(), settings,
                                    progress=lambda k, n: bridge.progress.emit('xo', holder.get('token'), k, n))

        started = self._start('xo', work)
        if started:
            holder['token'] = self._job['token']
        return started

    @QtCore.pyqtSlot(str, object, int, int)
    def _on_progress(self, kind: str, token, done: int, total: int) -> None:
        if self._mine(kind, token):
            if kind == 'compute':
                self._compute_progress = (done, total)
                self._compute_done_at = time.perf_counter()
            elif kind == 'mimo':
                self._mimo_progress = (done, total)
                self._mimo_done_at = time.perf_counter()
            else:
                self._xo_progress = (done, total)
                self._xo_done_at = time.perf_counter()
            self._refresh_status()

    def _on_tick(self) -> None:
        if self._job is None:
            self._tick.stop()
            return
        if self._job['kind'] in ('xo', 'mimo', 'compute'):
            self._refresh_status()

    def elapsed_seconds(self) -> float:
        """지금 도는 계산의 경과 초(돌지 않으면 0)."""
        return time.perf_counter() - self._job['started'] if self._job is not None else 0.0

    def mimo_estimate_seconds(self) -> float:
        """MIMO 계산 예상 초. 돌고 있고 입력이 하나라도 끝났으면 실제 속도로(끝난 입력당 경과 × 남은 라운드),
        아니면 전에 같은 엔진으로 잰 값, 그것도 없으면 구성 어림(10-09: 초고지연에서 어림 750 s 를 넘겨 924 s 에도 돌았다)."""
        done, total = getattr(self, '_mimo_progress', (0, 0))
        if self.mimo_running() and done and total:
            import math
            per_round = (self._mimo_done_at - self._job['started']) / max(1, math.ceil(done / max(cal.MIMO_WORKERS, 1)))
            return per_round * math.ceil(total / max(cal.MIMO_WORKERS, 1))
        fir = bool(self.settings.mimo_fir)
        if (self.mimo_run is not None and getattr(self.mimo_run, 'elapsed_s', 0.0)
                and (cal.mimo_kind(self.mimo_run) == 'FIR') == fir):
            return float(self.mimo_run.elapsed_s)
        return cal.estimate_mimo_seconds(self.project, len(self.positions()), fir=fir)

    def xo_remaining_seconds(self) -> float:
        """XO 추천 남은 초 — 후보가 하나라도 끝났으면 실제 속도(경과 ÷ 끝난 후보)로, 아니면 구성 어림에서 경과를 뺀다.
        병렬이라 후보가 몰려 끝나므로 '끝난 후보당 경과'로 센다(10-09 사용자: 예상과 실제가 크게 다르다)."""
        if self._job is None or self._job['kind'] != 'xo':
            return 0.0
        done, total = self._xo_progress
        elapsed = self.elapsed_seconds()
        if done and total:
            if done >= total:
                return 0.0
            per = (self._xo_done_at - self._job['started']) / done
            return max(0.0, per * total - elapsed)
        total = total or len(cal.XO_CANDIDATES)
        return max(0.0, cal.estimate_xo_seconds(self.project, self.settings.mode, total) - elapsed)

    def compute_remaining_seconds(self) -> float:
        """보정 계산 남은 초 — 단계가 하나라도 끝났으면 실제 속도(끝난 단계당 경과)로, 아니면 어림에서 경과를 뺀다.
        [계산] 아래 줄과 창 구석 알림이 둘 다 이 값을 쓴다(10-10 AI 시험: 17 s vs 23 s 로 서로 달랐다)."""
        if self._job is None or self._job['kind'] != 'compute':
            return 0.0
        done, total = getattr(self, '_compute_progress', (0, 0))
        elapsed = self.elapsed_seconds()
        if done and total:
            if done >= total:
                return 0.0
            per = (self._compute_done_at - self._job['started']) / done
            return max(0.0, per * total - elapsed)
        return max(0.0, self.estimate_seconds() - elapsed)

    def compute_total_seconds(self) -> float:
        """보정 계산 전체 예상 초(경과 + 남은) — 창 구석 표시용."""
        return self.elapsed_seconds() + self.compute_remaining_seconds() if self.running() else self.estimate_seconds()

    def xo_estimate_seconds(self) -> float:
        """XO 추천 전체 예상 초(경과 + 남은) — 창 구석 표시용."""
        return self.elapsed_seconds() + self.xo_remaining_seconds() if self.xo_running() else 0.0

    def _on_channel(self, _index: int) -> None:
        if self._syncing:
            return
        self.channel = str(self.channel_combo.currentData() or '')
        # XO 탭: 고른 채널의 그룹으로 XO 카드를 맞춘다 — 그래프 · XO 값이 그 채널 것이 되게(10-09).
        if self.run is not None:
            group = next((g for g, rs in self.run.engine.groups.items() if self.channel in rs), '')
            if group and group != self.xo_group and self.xo_group_combo.findData(group) >= 0:
                self.set_xo_group(group)
                return                                   # _on_xo_group 이 다시 그린다
        self._refresh_result()

    def set_channel(self, role: str) -> None:
        index = self.channel_combo.findData(role)
        if index >= 0:
            self.channel_combo.setCurrentIndex(index)

    def set_tab(self, key: str) -> None:
        self.tab = key if key in dict(TABS) else 'spl'
        self._refresh_result()

    # ──────────────────────────── 계산 ────────────────────────────
    def running(self) -> bool:
        """보정 계산이 도는 중(MIMO 는 `mimo_running`)."""
        return self._job is not None and self._job['kind'] == 'compute'

    def subs_running(self) -> bool:
        """[서브만 계산] 이 도는 중(10-12 사용자: 꽤 걸리는데 구석에 예상 시간이 안 뜬다 — 구석 알림 목록에 없었다)."""
        return self._job is not None and self._job['kind'] == 'subs'

    def subs_estimate_seconds(self) -> float:
        """[서브만 계산] 전체 예상 초 — 전에 잰 값(`sub_run.elapsed_s`), 없으면 구성 어림."""
        if self.sub_run is not None and getattr(self.sub_run, 'elapsed_s', 0.0):
            return float(self.sub_run.elapsed_s)
        return cal.estimate_subs_seconds(self.project)

    def mimo_running(self) -> bool:
        return self._job is not None and self._job['kind'] == 'mimo'

    def working(self) -> bool:
        return self._job is not None

    def stale(self) -> bool:
        """결과가 지금 설정·측정과 다르다(다시 계산해야 맞다) — 측정을 다시 했거나 다른 프로젝트를 열었을 때도.
        튜토리얼은 저장해 둔 데모 결과를 보여 주는 척만 하므로 낡음을 따지지 않는다(10-11)."""
        if self._demo_mode():
            return False
        return self.run is not None and (self.run.settings.engine_key() != self.settings.engine_key()
                                         or self.run.signature != cal.measurement_signature(self.project)
                                         or (self.run.target_key is not None
                                             and self.run.target_key != cal.target_key(self.project)))

    def mimo_stale(self) -> bool:
        return self.mimo_run is not None and self.mimo_run.key() != self.settings.mimo_key()

    def estimate_seconds(self) -> float:
        if self.run is not None and self.run.settings.mode == self.settings.mode:
            return float(self.run.elapsed_s)
        return cal.estimate_seconds(self.project, self.settings.mode)

    def compute(self) -> bool:
        """보정 계산을 워커 스레드에서 시작한다. 시작했으면 True(돌고 있거나 측정이 없으면 False).
        튜토리얼은 계산하지 않고 저장해 둔 데모 결과를 짧게 보여 준다(`_fake_compute`, 10-11)."""
        if self._job is not None:
            return False
        if self._demo_mode():
            return self._fake_compute()
        if self.irs_provider is None or not self.positions():
            self._error = '측정이 없습니다 — 5단계에서 측정하세요'
            self._refresh_status()
            return False
        project = copy.deepcopy(self.project)             # 계산 도중 화면에서 값을 바꿔도 이 계산은 시작 값으로
        settings = copy.deepcopy(self.settings)
        provider = self.irs_provider
        bridge = self._bridge
        holder: dict = {}
        self._compute_progress = (0, 0)
        started = self._start('compute', lambda: cal.compute(
            project, provider(), settings,
            progress=lambda k, n: bridge.progress.emit('compute', holder.get('token'), k, n)))
        if started:
            holder['token'] = self._job['token']
        return started

    def needs_first_xo(self) -> bool:
        """이 프로젝트에서 XO 추천을 한 번도 안 했나(XO 가 있는 그룹이 있고 어느 그룹에도 추천값이 없다)."""
        if getattr(self, 'trial', False):       # 튜토리얼은 XO 추천(수 분)을 건너뛰고 기본 XO 로 바로 계산(10-10 AI 시험: 10명 모두 여기서 멈춤)
            return False
        groups = self.xo_group_names()
        return bool(groups) and not any(self.settings.xo_rec.get(g) for g in groups)

    def compute_subs(self) -> bool:
        """[서브만 계산] — BA까지만 다시 풀어 BA 탭을 바로 그린다(메인 RC·MIMO·내보내기 결과는 그대로)."""
        if self._job is None and self._demo_mode():
            return self._fake_compute('subs')              # 튜토리얼은 저장해 둔 BA 결과를 보여 주기만(10-11)
        if self._job is not None or self.irs_provider is None or not self.positions():
            return False
        project = copy.deepcopy(self.project)
        settings = copy.deepcopy(self.settings)
        provider = self.irs_provider
        return self._start('subs', lambda: cal.compute(project, provider(), settings, subs_only=True))

    # ──── 튜토리얼: 미리 계산해 두고 [계산] 은 짧게 보여 주기(10-10 사용자: "연산하는 척만 하고 빠르게") ────
    #: 튜토리얼 [계산] 의 보여 주기 길이 — 12 칸 × 0.25 s ≈ 3 s(미리 계산이 아직이면 끝날 때까지 마지막 칸에서 기다린다)
    FAKE_STEPS, FAKE_STEP_MS = 12, 250

    def _demo_mode(self) -> bool:
        """튜토리얼이고 저장해 둔 데모 결과가 있다 — 이때는 보정 계산을 하지 않는다(10-11 사용자: "연산하는 척만 하라니까")."""
        return bool(getattr(self, 'trial', False)) and cal.load_demo_run() is not None

    def _use_demo_run(self) -> None:
        """저장해 둔 데모 결과를 지금 설정의 '미리 계산' 으로 둔다(설정 · 측정이 달라도 다시 계산하지 않는다)."""
        key = self._prepare_key()
        self._prepared, self._prepared_key = (key, copy.deepcopy(cal.load_demo_run())), key

    def _prepare_key(self) -> tuple:
        """미리 계산이 아직 맞는지 가르는 키 — 설정 · 측정 · 타겟(10-10 코드 리뷰: 설정만 보면 타겟을 끈 뒤에도 옛 결과를 넘겼다)."""
        return (self.settings.engine_key(), cal.measurement_signature(self.project), cal.target_key(self.project))

    def prepare_in_background(self) -> bool:
        """튜토리얼 전용: 데모 측정이 들어오면 뒤에서 조용히 보정 계산을 해 둔다(화면 상태는 그대로 — `run` 은 [계산] 때
        넘긴다). 이미 돌고 있거나 같은 설정으로 해 둔 것이 있으면 다시 하지 않는다."""
        if self.irs_provider is None or not self.positions():
            return False
        if self._demo_mode():                   # 계산하지 않고 저장해 둔 데모 결과(10-11)
            self._use_demo_run()
            return True
        key = self._prepare_key()
        thread = getattr(self, '_prepare_thread', None)
        if (thread is not None and thread.is_alive()) or getattr(self, '_prepared_key', None) == key:
            return False
        project, settings, provider = copy.deepcopy(self.project), copy.deepcopy(self.settings), self.irs_provider
        self._prepared, self._prepared_key = None, key

        def work() -> None:
            _below_normal_priority()
            try:
                self._prepared = (key, cal.compute(project, provider(), settings))
            except Exception:  # noqa: BLE001 - 못 하면 [계산] 이 실제로 계산한다
                self._prepared, self._prepared_key = None, None
        self._prepare_thread = threading.Thread(target=work, name='secs-calibration-prepare', daemon=True)
        self._prepare_thread.start()
        return True

    def _fake_compute(self, kind: str = 'compute') -> bool:
        """튜토리얼 [계산] · [서브만 계산](`kind='subs'`): 진행 칸만 짧게 올리고 미리 계산한 결과를 넘긴다. 데모 결과 파일이
        있으면 설정이 바뀌어도 그것을 넘긴다(계산하지 않음, 10-11). 파일이 없을 때만 예전처럼 미리 계산이 없거나 설정이
        바뀌면 실제 계산."""
        if self._demo_mode():
            self._use_demo_run()
        key = self._prepare_key()
        if getattr(self, '_prepared_key', None) != key:
            return self.compute()
        value = None
        if kind == 'subs':
            value = copy.deepcopy(cal.load_demo_subs_run())
            if value is None:
                return False
        token = object()
        self._job = {'kind': kind, 'token': token, 'thread': None, 'started': time.perf_counter(), 'fake': True}
        self._error = ''
        self._cancelled = False
        self._compute_progress = (0, self.FAKE_STEPS)
        step = {'k': 0}
        timer = QtCore.QTimer(self)
        timer.setInterval(self.FAKE_STEP_MS)

        def tick() -> None:
            if not self._mine(kind, token):
                timer.stop(); timer.deleteLater()
                return
            prepared = getattr(self, '_prepared', None)
            ready = prepared is not None and prepared[0] == key
            thread = getattr(self, '_prepare_thread', None)
            if not ready and (thread is None or not thread.is_alive()):
                timer.stop(); timer.deleteLater()             # 미리 계산이 실패했다 — 실제로 계산
                self._job = None
                self.compute()
                return
            if step['k'] < self.FAKE_STEPS - 1 or ready:
                step['k'] += 1
            self._compute_progress = (step['k'], self.FAKE_STEPS)
            self._compute_done_at = time.perf_counter()      # 남은 시간 표시가 쓰는 값(실제 계산의 진행 신호와 같게)
            self._refresh_status()
            if step['k'] >= self.FAKE_STEPS and ready:
                timer.stop(); timer.deleteLater()
                self._on_done(kind, token, prepared[1] if value is None else value)
        timer.timeout.connect(tick)
        timer.start()
        self._tick.start()
        self.refresh()
        self.changed.emit()
        return True

    def compute_clicked(self) -> bool:
        """[계산] 버튼. XO 추천을 한 번도 안 한 프로젝트면 추천부터 돌리고, 끝나면 `_on_done('xo')` 가 추천 XO 를 넣고 보정을
        계산한다(10-09 사용자: "XO 추천도 첫 계산에 자동으로"). 저절로 도는 건 이 클릭 안에서만 — 창의 자동 보정
        (`compute_if_needed`)은 XO 추천을 부르지 않는다(사용자: 계산을 눌러야만). 진행·남은 시간·취소는 XO 추천 그대로."""
        if self._job is None and getattr(self, 'trial', False):
            return self._fake_compute()
        if self._job is None and self.needs_first_xo() and self.irs_provider is not None and self.positions():
            return self.recommend_xo()
        return self.compute()

    def compute_if_needed(self) -> bool:
        """결과가 없거나 낡았으면 계산한다(창이 들어올 때·측정 뒤 자동 보정)."""
        if self._job is not None or (self.run is not None and not self.stale()):
            return False
        return self.compute()

    def compute_mimo(self) -> bool:
        """MIMO(무겁다 — [MIMO 계산] 버튼으로만). 보정 결과가 있고 위치가 3곳 이상일 때만."""
        ok, reason = self.mimo_available()
        if self._job is not None or self.run is None or not ok or not self.settings.mimo_on:
            if not ok:
                self._mimo_error = reason
            self._refresh_status()
            return False
        run = self.run
        fsiso, low = float(self.settings.fsiso), self.settings.mimo_low
        exclude = self.settings.excluded_supports()
        cap = int(self.settings.mimo_max_supports)
        fir = bool(self.settings.mimo_fir)
        bridge = self._bridge
        holder: dict = {}
        self._mimo_progress = (0, 0)
        started = self._start('mimo', lambda: cal.compute_mimo(
            run, fsiso, low, workers=cal.MIMO_WORKERS, exclude=exclude, max_supports=cap, fir=fir,
            progress=lambda k, n: bridge.progress.emit('mimo', holder.get('token'), k, n)))
        if started:
            holder['token'] = self._job['token']
        return started

    def _start(self, kind: str, work) -> bool:
        token = object()
        bridge = self._bridge

        def target() -> None:
            _below_normal_priority()
            try:
                value = work()
            except Exception as error:  # noqa: BLE001 - 계산 실패는 화면 문구로
                try:
                    bridge.failed.emit(kind, token, str(error) or type(error).__name__)
                except RuntimeError:            # 창이 먼저 닫혔다
                    pass
                return
            try:
                bridge.done.emit(kind, token, value)
            except RuntimeError:
                pass

        thread = threading.Thread(target=target, name=f'secs-calibration-{kind}', daemon=True)
        self._job = {'kind': kind, 'token': token, 'thread': thread, 'started': time.perf_counter()}
        if kind in ('compute', 'subs'):
            self._error = ''
        elif kind == 'mimo':
            self._mimo_error = ''
        else:
            self._xo_error = ''
            self._xo_progress = (0, 0)
        self._cancelled = False
        self._tick.start()
        thread.start()
        self.refresh()
        self.changed.emit()
        return True

    def cancel(self) -> None:
        """돌고 있는 계산의 결과를 버린다(엔진은 도중에 못 끊는다 — 뒤에서 끝난 뒤 버려진다)."""
        if self._job is None:
            return
        if self._job.get('kind') in ('mimo', 'xo'):
            # MIMO·XO 추천은 워커 프로세스에서 돈다 — 결과만 버리면 프로세스가 계속 CPU 를 잡는다(10-09). 바로 끈다.
            from secs.mimo_iir import abort_workers
            abort_workers()
        self._job = None
        self._cancelled = True
        self.refresh()
        self.changed.emit()

    def wait(self, timeout_ms: int = 20000) -> bool:
        """돌고 있는 계산이 끝나 결과가 화면에 올 때까지 이벤트 루프를 돌린다(창 닫기·테스트·QA 도구)."""
        timer = QtCore.QElapsedTimer()
        timer.start()
        app = QtWidgets.QApplication.instance()
        while self._job is not None and timer.elapsed() < timeout_ms:
            if app is not None:
                app.processEvents(QtCore.QEventLoop.ProcessEventsFlag.AllEvents, 20)
            time.sleep(0.005)
        return self._job is None

    def _mine(self, kind: str, token) -> bool:
        return self._job is not None and self._job['kind'] == kind and self._job['token'] is token

    @QtCore.pyqtSlot(str, object, object)
    def _on_done(self, kind: str, token, value) -> None:
        if not self._mine(kind, token):
            return                                          # 취소 뒤 늦게 온 결과
        self._job = None
        if kind == 'subs':
            self.sub_run = value
            self.refresh()
            self.changed.emit()
            return
        if kind == 'compute':
            self.run = value
            self.sub_run = None                             # 전체 계산이 서브도 다시 풀었다
            self.mimo_run = None                             # 새 보정 위의 MIMO 는 다시 계산해야 맞다
            self.compute_count += 1
            self._error = ''
            # 계산 전에 [내보내기]를 눌러 남은 '계산 결과가 없습니다' 를 지운다(10-09 QA: 계산이 끝난 뒤에도 남아 있었다).
            # 지난 내보내기의 '저장했습니다' 줄도 새 결과와는 무관하니 같이 지운다.
            self.export_note.setText('')
            self._fill_channels()
        elif kind == 'xo':
            # 추천값을 설정에 넣고(사용자가 이어서 고칠 수 있다) 그 XO 로 보정을 다시 계산한다.
            self.xo_recommendation = value
            self.settings.xo.update(value.best)
            self.settings.xo_rec = dict(value.best)
            self.settings.store(self.project)
            self.refresh()
            self.changed.emit()
            self.compute()
            return
        else:
            self.mimo_run = value
            self.mimo_count += 1
            self._mimo_error = ''
        self._save_cache()                                  # 다음에 불러올 때 다시 계산하지 않게(10-09)
        self.refresh()
        self.changed.emit()
        if kind == 'compute':
            self.computed.emit()

    @QtCore.pyqtSlot(str, object, str)
    def _on_failed(self, kind: str, token, message: str) -> None:
        if not self._mine(kind, token):
            return
        self._job = None
        if kind in ('compute', 'subs'):
            self._error = f'계산 실패: {message}'
        elif kind == 'xo':
            self._xo_error = f'XO 추천 실패: {message}'
        else:
            self._mimo_error = f'MIMO 실패: {message}'
        self.refresh()
        self.changed.emit()

    # ──────────────────────────── 그리기 ────────────────────────────
    _syncing = False

    def refresh(self) -> None:
        """project 값으로 다시 채운다(불러오기·언어 전환·단계 진입)."""
        stored = cal.CalibrationSettings.from_project(self.project)
        if (self.project.extra or {}).get('calibration') is not None:
            self.settings = stored
        if self.run is None:
            self._fill_channels()                       # 계산 전에도 측정된 채널을 고를 수 있게(미리보기)
        self._refresh_controls()
        self._refresh_status()
        self._refresh_export_path()
        self._refresh_result()

    def _refresh_controls(self) -> None:
        s = self.settings
        self._syncing = True
        try:
            self._ultra_unlocked = self._ultra_unlocked or s.mode == cal.ULTRA_MODE
            for key, button in self.mode_buttons.items():
                set_state(button, 'on' if key == s.mode else '')
            self.mode_buttons[cal.ULTRA_MODE].setVisible(self._ultra_unlocked)
            self.ultra_row.setVisible(s.mode == cal.ULTRA_MODE)
            self.ultra_spin.setValue(float(s.ultra_ms))
            self.mode_spec.setText(_join(cal.mode_spec(s.mode, s.ultra_ms, s.mimo_fir).split(' · ')))
            self.rc_lo_spin.setValue(float(s.rc_lo))
            self.rc_hi_auto.setChecked(s.rc_hi is None)
            shown_hi = s.rc_hi if s.rc_hi is not None else (self.run.rc_hi if self.run is not None else 500.0)
            self.rc_hi_spin.setValue(float(shown_hi))
            self.rc_hi_spin.setEnabled(s.rc_hi is not None)
            self.volume_spin.setSpecialValueText(str(t('없음')))      # 언어를 바꾸면 다시(스핀 특수 문구는 번역 위젯이 안 옮긴다)
            self.volume_spin.setValue(float(s.room_volume_m3 or 0.0))
            self.boost_spin.setValue(float(s.boost_max_db))
            self.xo_peq_check.setChecked(bool(s.xo_peq_on))
            self.xo_peq_lo_spin.setValue(float(s.xo_peq_lo))
            self.xo_peq_hi_spin.setValue(float(s.xo_peq_hi))
            for spin in (self.xo_peq_lo_spin, self.xo_peq_hi_spin):
                spin.setSpecialValueText(str(t('자동')))     # 언어를 바꾸면 다시(부피 칸과 같은 이유)
                spin.setEnabled(bool(s.xo_peq_on))
            if s.room_dims_m is not None:
                for spin, value in zip(self.dim_spins, s.room_dims_m):
                    spin.setValue(float(value))
            elif all(spin.value() > 0 for spin in self.dim_spins):
                # 온전한 치수가 칸에 있는데 설정에는 없다 = 부피를 직접 고쳤거나 다른 프로젝트를 열었다 → 비운다.
                # 한두 칸만 넣은 중간 상태는 그대로 둔다(칠 때마다 지우면 셋째 칸까지 갈 수 없다).
                for spin in self.dim_spins:
                    spin.setValue(0.0)
            ok, reason = self.mimo_available()
            self.mimo_check.setChecked(bool(s.mimo_on) and ok)
            self.mimo_check.setEnabled(ok)
            on = ok and s.mimo_on
            self.fsiso_spin.setValue(float(s.fsiso))
            self.fsiso_spin.setEnabled(on)
            self.mimo_low_auto.setChecked(s.mimo_low is None)
            self.mimo_low_auto.setEnabled(on)
            self.mimo_low_spin.setValue(float(s.mimo_low if s.mimo_low is not None else 20.0))
            self.mimo_low_spin.setEnabled(on and s.mimo_low is not None)
            # 스핀 특수 문구·접미사는 번역 위젯이 안 옮긴다 — 언어를 바꾸면 여기서 다시
            self.mimo_max_spin.setSpecialValueText(str(t('제한 없음')))
            self.mimo_max_spin.setSuffix(str(t(' 서포트')))
            self.mimo_max_spin.setValue(int(s.mimo_max_supports))
            self.mimo_max_spin.setEnabled(on)
            self.mimo_fir_check.setChecked(bool(s.mimo_fir))
            self.mimo_fir_check.setEnabled(on)
            self.mimo_reason.setText(reason)
            self.mimo_reason.setVisible(not ok)
            self.mimo_button.setEnabled(on and self.run is not None and self._job is None)
            self._refresh_sub_controls()
            groups = self.xo_group_names()
            # 그룹 키는 모델의 한국어 이름('프론트'…) — 보이는 글자만 번역(언어를 바꾸면 다시 채운다, 09-27 영어 QA).
            shown = [(str(t(g)), g) for g in groups]
            if [(self.xo_group_combo.itemText(i), self.xo_group_combo.itemData(i))
                    for i in range(self.xo_group_combo.count())] != shown:
                self.xo_group_combo.clear()
                for text, group in shown:
                    self.xo_group_combo.addItem(text, group)
            if self.xo_group not in groups:
                self.xo_group = groups[0] if groups else ''
            if self.xo_group:
                self.xo_group_combo.setCurrentIndex(max(0, self.xo_group_combo.findData(self.xo_group)))
                self.xo_spin.setValue(self.xo_of(self.xo_group))
            has = bool(groups)
            self.xo_group_combo.setEnabled(has)
            self.xo_spin.setEnabled(has)
            rec = self.settings.xo_rec.get(self.xo_group)
            self.xo_rec_label.setText(f'추천 {rec:g} Hz' if rec else '추천 전')
            self.xo_rec_label.setVisible(has)
            self.xo_reason.setText('서브가 없어 XO 가 없습니다')
            self.xo_reason.setVisible(not has)
            self.xo_button.setEnabled(has and self._job is None and self.irs_provider is not None
                                      and bool(self.positions()))
            self.compute_button.setEnabled(self._job is None)
            # 취소는 늘 보이고, 돌고 있는 계산이 있을 때만 눌린다(숨기면 '취소 버튼이 없다' — 10-09).
            kind = self._job['kind'] if self._job is not None else ''
            self.cancel_button.setEnabled(bool(kind))
            self.xo_cancel_button.setEnabled(kind == 'xo')
            self.mimo_cancel_button.setEnabled(kind == 'mimo')
            for button in self.export_buttons.values():
                button.setEnabled(self.run is not None and self._job is None)
        finally:
            self._syncing = False

    def _refresh_status(self) -> None:
        if self.running():
            done, total = getattr(self, '_compute_progress', (0, 0))
            head = f'보정 계산 중… ({done}/{total})' if done and total else '보정 계산 중…'
            text = _join([head, _remaining_text(self.compute_remaining_seconds())])
        elif self.xo_running():
            # [계산] 이 XO 추천부터 돌릴 때(첫 계산, 10-09) — 모드 카드에서도 진행이 보이게(XO 카드는 XO 탭에서만 보인다)
            done, total = self._xo_progress
            head = f'XO 추천 계산 중… ({done}/{total})' if total else 'XO 추천 계산 중…'
            text = _join([head, _remaining_text(self.xo_remaining_seconds())])
        elif self._error:
            text = self._error
        elif self._cancelled and self.run is None:
            text = '취소했습니다'
        elif self.run is None:
            text = '계산 대기 중' if self.positions() else '측정이 없습니다 — 5단계에서 측정하세요'
        elif self.run.signature != cal.measurement_signature(self.project):
            text = '측정이 바뀌었습니다 — [계산]을 누르세요'
        elif self.stale():
            text = '설정이 바뀌었습니다 — [계산]을 누르세요'
        else:
            text = '계산 완료'
        self.status.setText(text)
        # MIMO 줄
        ok, _reason = self.mimo_available()
        if not ok or not self.settings.mimo_on:
            mimo = ''
        elif self.mimo_running():
            done, total = getattr(self, '_mimo_progress', (0, 0))
            head = f'MIMO 계산 중… ({done}/{total})' if total else 'MIMO 계산 중…'
            mimo = _join([head, _remaining_text(self.mimo_estimate_seconds() - self.elapsed_seconds())])
        elif self._mimo_error:
            mimo = self._mimo_error
        elif self.run is None:
            mimo = '먼저 [계산]으로 보정을 계산하세요'
        elif self.mimo_run is None:
            mimo = 'MIMO 계산 전'
        elif self.mimo_stale():
            mimo = 'MIMO 설정이 바뀌었습니다 — [MIMO 계산]을 누르세요'
        else:
            total = sum(self.mimo_run.supports(r) for r in self.mimo_run.iir)
            cap = int(self.mimo_run.max_supports)
            budget = cal.mimo_fir_budget_text(self.mimo_run, int(getattr(self.project.dsp, 'fir_budget', 0) or 0))
            mimo = _join(('MIMO 계산 완료', f'{cal.mimo_kind(self.mimo_run)} 서포트 {total}개')
                         + ((f'입력당 서포트 ≤ {cap}',) if cap else ()) + ((budget,) if budget else ()))
        self.mimo_status.setText(mimo)
        self.mimo_status.setVisible(bool(mimo))
        # XO 줄
        if self.xo_running():
            done, total = self._xo_progress
            head = f'XO 추천 계산 중… ({done}/{total})' if total else 'XO 추천 계산 중…'
            xo = _join([head, _remaining_text(self.xo_remaining_seconds())])
        elif self._xo_error:
            xo = self._xo_error
        elif self.xo_recommendation is not None:
            xo = _join(['XO 추천 완료'] + [f'{t(g)} {hz:g} Hz' for g, hz in self.xo_recommendation.best.items()])
        else:
            xo = ''
        self.xo_status.setText(xo)
        self.xo_status.setVisible(bool(xo))

    def _preview(self) -> dict:
        """계산 전 미리보기: 청취 위치(위치 1) 측정의 SPL — role → dB(`cal.SPL_GRID`). 10-09 사용자: 자동 계산을 끈 뒤
        불러온 프로젝트의 보정 설계가 텅 비어 '아무것도 안 뜬다'. 위치 1 wav 만 읽고 (경로·수정 시각)으로 기억한다."""
        from secs.rewapi.session import IrSaver
        record = next((r for r in (self.project.measurements or [])
                       if isinstance(r, dict) and str(r.get('pos')) == '1'), None)
        irs = record.get('ir') if isinstance(record, dict) and isinstance(record.get('ir'), dict) else {}
        base = self.project_path.parent if self.project_path is not None else Path('.')
        key = (str(base), tuple(sorted((str(k), str(v)) for k, v in irs.items())))
        if getattr(self, '_preview_key', None) == key:
            return self._preview_cache
        out = {}
        for role, rel in irs.items():
            try:
                path = Path(rel) if Path(rel).is_absolute() else base / rel
                ir, rate = IrSaver.load_path(path)
                ir = np.asarray(ir, float).ravel()
                n = 1 << int(np.ceil(np.log2(max(ir.size, 65536))))
                f = np.fft.rfftfreq(n, 1.0 / float(rate))
                out[str(role)] = cal.spl_db(np.fft.rfft(ir, n), f)
            except Exception:  # noqa: BLE001 - 못 읽는 wav 는 미리보기에서 빼기만
                continue
        self._preview_key, self._preview_cache = key, out
        return out

    def _fill_channels(self) -> None:
        if self.run is not None:
            roles = self.run.roles
        else:                                            # 계산 전 — 측정된 채널(구성 순서)
            have = self._preview()
            roles = [sp.role for sp in self.project.config.speakers if sp.role in have]
        self._syncing = True
        try:
            self.channel_combo.clear()
            for role in roles:
                self.channel_combo.addItem(role_code(role), role)
                self.channel_combo.setItemData(self.channel_combo.count() - 1, role_label(role),
                                               QtCore.Qt.ItemDataRole.ToolTipRole)
            if self.channel not in roles:
                self.channel = roles[0] if roles else ''
            if self.channel:
                self.channel_combo.setCurrentIndex(max(0, self.channel_combo.findData(self.channel)))
        finally:
            self._syncing = False

    def show_mimo(self) -> bool:
        """MIMO 곡선을 그리고 내보내나 — 켰고, 계산했고, 지금 보정 위의 것일 때."""
        return bool(self.settings.mimo_on and self.mimo_run is not None and self.run is not None
                    and self.mimo_available()[0])

    def sub_count(self) -> int:
        """서브 수 — 계산했으면 엔진이 쓴 서브(측정된 것), 아니면 구성."""
        if self.run is not None:
            return len(self.run.engine.subs)
        return len(self.project.config.groups().get('sub', []))

    def _show_tab_cards(self) -> None:
        """오른쪽 설정 카드를 탭에 맞춘다(10-09 사용자): SPL·GD = RC 범위, XO = XO, BA = BA 설정. 보정 모드·MIMO
        카드와 내보내기는 늘 보인다. 숨기기만 하니 값·계산 상태는 그대로다."""
        self.rc_card.setVisible(self.tab in ('spl', 'gd'))
        self.xo_card.setVisible(self.tab == 'xo')
        self.sub_card.setVisible(self.tab == 'subs' and len(self.sub_roles()) >= 2)

    def _draw_rc_band(self, x_range: tuple[float, float]) -> None:
        """SPL·GD 탭: RC 하한·상한 세로선 + 바깥 대역 옅은 음영 + 값 라벨(10-09 사용자: "RC 상한과 하한이 명확히 보였으면").
        값은 계산 결과가 있으면 그 계산이 쓴 값(자동 상한 = 슈뢰더 등 `run.rc_hi`), 없으면 설정(자동 상한은 계산 전엔 모른다 — 선 없음)."""
        self.rc_lines = {}
        run = self.run
        lo = float(run.settings.rc_lo) if run is not None else float(self.settings.rc_lo)
        hi = float(run.rc_hi) if run is not None else self.settings.rc_hi
        shade = QtGui.QColor(COLORS['ink3'])
        shade.setAlpha(28)
        x0, x1 = np.log10(x_range[0]), np.log10(x_range[1])
        span = (x1 - x0) * FREQ_PAD                     # 축 양 끝 여유(`set_freq_range`)까지 음영
        x0, x1 = x0 - span, x1 + span
        for key, hz, text, edge in (('lo', lo, 'RC 하한 {0} Hz', (x0, None)), ('hi', hi, 'RC 상한 {0} Hz', (None, x1))):
            if hz is None or not x_range[0] < hz < x_range[1]:
                continue
            at = np.log10(hz)
            region = pg.LinearRegionItem(values=(edge[0] if edge[0] is not None else at, edge[1] if edge[1] is not None else at),
                                         movable=False, brush=pg.mkBrush(shade), pen=pg.mkPen(None))
            region.setAcceptedMouseButtons(QtCore.Qt.MouseButton.NoButton)
            for line in region.lines:
                line.setPen(pg.mkPen(None))
            region.setZValue(-10)
            self.plot.addItem(region)
            line = pg.InfiniteLine(pos=at, angle=90, movable=False,
                                   pen=pg.mkPen(COLORS['ink2'], width=1, style=QtCore.Qt.PenStyle.DashLine),
                                   label=str(t(text.format(f'{hz:.0f}'))),
                                   # 하한 라벨은 아래, 상한은 위 — 20·186 Hz 처럼 가까우면 같은 높이에서 겹쳤다(10-09 돌비 캡처)
                                   labelOpts={'position': 0.06 if key == 'lo' else 0.95, 'color': COLORS['ink2'],
                                              'anchors': [(0, 1), (0, 1)] if key == 'lo' else [(0, 0), (0, 0)],
                                              'fill': pg.mkBrush(COLORS['plot_bg'])})
            line.setAcceptedMouseButtons(QtCore.Qt.MouseButton.NoButton)
            line.setZValue(-5)
            self.plot.addItem(line)
            self.rc_lines[key] = (hz, line)

    def _refresh_result(self) -> None:
        # BA 탭은 서브가 2개 이상일 때만(1개면 결합할 것이 없다 — R9).
        multi = self.sub_count() >= 2
        self.tab_buttons['subs'].setVisible(multi)
        if self.tab == 'subs' and not multi:
            self.tab = 'spl'
        self._show_tab_cards()
        for key, button in self.tab_buttons.items():
            set_state(button, 'on' if key == self.tab else '')
        self.result_bar.fit()                                   # 탭 수·글자(언어)가 바뀌면 한 줄 폭도 바뀐다
        self.plot.clear()
        self.rc_lines: dict = {}                                # 키 → (Hz, 선) — SPL·GD 탭에서 `_draw_rc_band` 가 채운다
        self.sub_values.setVisible(False)
        self.sub_filters_label.setVisible(False)
        self.sub_warn_result.setVisible(False)
        self.filters_label.setVisible(self.tab != 'subs')      # BA 에선 비는 줄 — 숫자 줄과 표 사이 빈칸이 됐다
        run = self.run
        role = self.channel
        has = run is not None and role in run.curves
        self.channel_combo.setEnabled(self.tab != 'subs')      # BA 는 채널과 상관없다
        self.legend_items['mimo'].setVisible(has and self.tab == 'spl' and self.show_mimo()
                                            and role in self.mimo_run.curves)
        self.legend_items['target'].setVisible(self.tab in ('spl', 'xo', 'subs'))
        for key in ('before', 'after'):
            self.legend_items[key].setVisible(self.tab in ('spl', 'gd'))
        for key in ('xo_main', 'xo_sub', 'xo_sum'):
            self.legend_items[key].setVisible(self.tab == 'xo')
        for key in ('sub_before', 'sub_after', 'sub_sum'):
            self.legend_items[key].setVisible(self.tab == 'subs')
        self._xo_curves = {}
        # 10-10 사용자 결정(1A): 마이크 상대 레벨(−35 ~ −75 dB)이 dB SPL 처럼 읽혔다 → SPL 은 타겟 기준으로 옮겨 그린다.
        # XO · BA 탭도 같은 기준(10-10 3차 시험: 두 탭만 마이크 절대값 −40 ~ −75 라 SPL 탭과 기준이 달랐다).
        if self.tab == 'subs':
            axis_label(self.plot, 'left', t('dB (타겟 기준)'))
            self.plot.getAxis('bottom').setTicks(freq_ticks(SUB_TICKS))
            set_freq_range(self.plot, SUB_RANGE_X)
        elif self.tab == 'xo':
            axis_label(self.plot, 'left', t('dB (타겟 기준)'))
            self.plot.getAxis('bottom').setTicks(freq_ticks(XO_TICKS))
            set_freq_range(self.plot, XO_RANGE_X)
            self._draw_xo()
        elif self.tab == 'spl':
            axis_label(self.plot, 'left', t('dB (타겟 기준)'))
            self.plot.getAxis('bottom').setTicks(freq_ticks())
            set_freq_range(self.plot, SPL_RANGE)
            self._draw_rc_band(SPL_RANGE)
        else:
            axis_label(self.plot, 'left', 'ms')
            self.plot.getAxis('bottom').setTicks(freq_ticks(GD_TICKS))
            set_freq_range(self.plot, GD_RANGE)
            self._draw_rc_band(GD_RANGE)
        if not has:
            self.metrics_label.setText('')
            self.filters_label.setText('')
            self.summary_label.setText('')
            self.rc_note.setText('')
            self.rc_note.setVisible(False)
            if run is None and self.tab == 'spl':
                curve = self._preview().get(role)
                if curve is not None:                      # 계산 전: 청취 위치 측정 + 타겟만
                    x = cal.SPL_GRID
                    target = np.asarray(self.project.target.sample(x), float)
                    level = float(np.median(curve[(x >= 200) & (x <= 2000)]) - np.median(target[(x >= 200) & (x <= 2000)]))
                    self.plot.plot(x, curve - level, pen=pg.mkPen(C_MEASURED, width=2))
                    self.plot.plot(x, target, pen=pg.mkPen(C_TARGET, width=2, style=QtCore.Qt.PenStyle.DashLine))
                    self._draw_target_handles(0.0)
                    mid = float(np.median(curve[(x >= 20) & (x <= 20000)])) - level
                    self.plot.setYRange(mid - 30.0, mid + 15.0, padding=0)
                    self.summary_label.setText(t('계산 전 — 청취 위치 측정만 보입니다. [계산]을 누르면 보정 결과가 나옵니다.'))
            elif run is None and self.tab == 'gd':
                # 빈 그래프에 SPL 눈금(10 … −25)이 남았다(10-10 3차 시험) — GD 눈금(ms)과 안내 한 줄
                self.plot.setYRange(-10.0, 100.0, padding=0)
                self.summary_label.setText(t('계산 전 — [계산] 을 누르면 GD 가 나옵니다.'))
            elif run is None and self.tab == 'xo':
                self.plot.setYRange(-30.0, 12.0, padding=0)
                self.summary_label.setText(t('계산 전 — [계산] 을 누르면 메인 · 서브 · 합 곡선이 나옵니다.'))
            return
        curves = run.curves[role]
        if self.tab in ('xo', 'subs'):
            pass
        elif self.tab == 'spl':
            x = cal.SPL_GRID
            # 곡선은 계산 때의 레벨(타겟 + offset)에 있다 — offset 을 빼 타겟이 제 값에 오게 그린다(1A)
            # 타겟은 지금 프로젝트 값으로 그린다(끌어 고치면 바로 따라온다) — 레벨 기준은 계산 때의 것.
            shown = np.asarray(self.project.target.sample(x), float)
            offset = self._level_offset(run)
            self.plot.plot(x, curves['spl_before'] - offset, pen=pg.mkPen(C_MEASURED, width=2))
            self.plot.plot(x, curves['spl_after'] - offset, pen=pg.mkPen(C_CORRECTED, width=2))
            self.plot.plot(x, shown, pen=pg.mkPen(C_TARGET, width=2, style=QtCore.Qt.PenStyle.DashLine))
            self._draw_target_handles(0.0)
            if self.show_mimo() and role in self.mimo_run.curves:
                self.plot.plot(x, self.mimo_run.curves[role]['spl_mimo'] - offset, pen=pg.mkPen(C_MIMO, width=2))
            band = (x >= 20) & (x <= 20000)
            mid = float(np.median(run.target[band])) - offset
            self.plot.setYRange(mid - 30.0, mid + 15.0, padding=0)
        else:
            x = cal.GD_GRID
            self.plot.plot(x, curves['gd_before'], pen=pg.mkPen(C_MEASURED, width=2))
            self.plot.plot(x, curves['gd_after'], pen=pg.mkPen(C_CORRECTED, width=2))
            both = np.concatenate([curves['gd_before'], curves['gd_after']])
            both = both[np.isfinite(both)]
            hi = float(np.clip(np.percentile(both, 99) + 10.0, 30.0, 400.0)) if both.size else 100.0
            lo = float(np.clip(np.percentile(both, 1) - 5.0, -50.0, -5.0)) if both.size else -10.0
            self.plot.setYRange(lo, hi, padding=0)
        m0, m1 = run.metrics[role]['before'], run.metrics[role]['after']
        self.metrics_label.setText(_join((
            f'±1 dB 안(20–200 Hz) {m0["in1_20"]:.0f} % → {m1["in1_20"]:.0f} %',       # 앞 숫자에도 %(3차 시험)
            # 10-10 사용자 결정(2A): '1/24 GD 최대' 는 뺐다 — 딥 근처 한 점의 뾰족값이라 GD 폭이 좋아져도
            # 보정 뒤 커져 나빠진 것처럼 읽혔다(데모 L 121 → 154 ms @146 Hz, GD 폭 39.9 → 16.2 ms).
            f'GD 폭(20–100 Hz) {m0["gd_20"]:.1f} → {m1["gd_20"]:.1f} ms')))
        info = run.filters(role)
        parts = [f'PEQ {info["peq"]}개', f'FIR {info["fir_taps"]:,}탭' if info['fir_taps'] else 'FIR 없음']
        if info['xo_hz']:
            parts.append(f'XO {info["xo_hz"]:g} Hz')
        if self.show_mimo():
            parts.append(f'MIMO 서포트 {self.mimo_run.supports(role)}개')
        self.filters_label.setText(_join(parts))
        if self.tab == 'subs':
            self._draw_subs()
        rc = cal.rc_pieces(run)
        # RC 상한은 RC 카드에만 — 요약 줄에도 넣었더니 한 화면에 같은 말이 두 번 나왔다(10-10 AI 시험 6 · 7 · 9번).
        summary = [f'전체 지연 {run.latency_ms:.1f} ms'] + list(run.notes)
        self.summary_label.setText(_join(summary))
        # RC 칸(좁다)에는 조각마다 한 줄 — 엔진 노트('RC 상한 … Hz (…)')가 첫 줄, 자동이면 무엇으로 정했는지가 다음 줄.
        self.rc_note.setText('\n'.join(str(t(part)) for part in rc))
        self.rc_note.setVisible(True)

    def _level_offset(self, run) -> float:
        """계산 곡선의 레벨 − 타겟(dB) — 이만큼 빼서 그리면 타겟이 제 값(타겟 레벨 = 0 dB)에 온다(10-10 사용자 결정 1A).
        SPL · XO · BA 탭이 같은 값을 쓴다(3차 시험: XO · BA 탭만 마이크 절대값이었다). 타겟은 지금 프로젝트
        값과 견준다 — 계산 뒤 타겟을 끌어 고치면 SPL 탭 타겟이 바로 따라오는 것과 같은 기준."""
        if run is None:
            return 0.0
        if getattr(run, 'settings_target', None) is not None:
            return float(np.median(run.target - np.asarray(run.settings_target, float)))
        shown = np.asarray(self.project.target.sample(cal.SPL_GRID), float)
        return float(np.median(run.target - shown))

    def _draw_subs(self) -> None:
        """BA 보기: 서브마다 정렬 전(점선)·후(실선), 서브 합 전·후(잉크), 평탄도·좌석 편차 줄, 정렬 값 표.
        값은 마지막 보정 계산 그대로(`calibration.sub_combine`) — 시트·내보내기에 나가는 값과 같다."""
        source = self.sub_run if self.sub_run is not None else self.run
        view = cal.sub_combine(source) if source is not None else None
        if view is None:
            return
        x = view['grid']
        dash = QtCore.Qt.PenStyle.DashLine
        offset = self._level_offset(source)          # SPL 탭과 같은 타겟 기준(보이는 높이만 — 정렬 값 · 점수는 그대로)
        # 타겟 커브(10-11 사용자: "서브정렬에도 타겟 커브를 넣어줘") — SPL 탭처럼 지금 프로젝트 타겟을 그대로(곡선이 그 기준으로 옮겨짐)
        self.plot.plot(x, np.asarray(self.project.target.sample(x), float),
                       pen=pg.mkPen(C_TARGET, width=2, style=QtCore.Qt.PenStyle.DashLine))
        for k, role in enumerate(view['subs']):
            color = SUB_COLORS[k % len(SUB_COLORS)]
            self.plot.plot(x, view['before'][role] - offset, pen=pg.mkPen(color, width=1, style=dash))
            self.plot.plot(x, view['after'][role] - offset, pen=pg.mkPen(color, width=2))
        self.plot.plot(x, view['sum_before'] - offset, pen=pg.mkPen(C_XO_SUM, width=2, style=dash))
        self.plot.plot(x, view['sum_after'] - offset, pen=pg.mkPen(C_XO_SUM, width=3))
        lo, hi = view['band']
        band = (x >= lo) & (x <= hi)
        top = float(max(np.max(view['sum_before'][band]), np.max(view['sum_after'][band]))) - offset
        self.plot.setYRange(top - 40.0, top + 6.0, padding=0)
        flat, seat = view['flat'], view['seat']
        self.metrics_label.setText(_join((
            f'서브 합 평탄도({lo:.0f}–{hi:.0f} Hz) {flat[0]:.1f} → {flat[1]:.1f} dB',
            f'좌석 편차 {seat[0]:.1f} → {seat[1]:.1f} dB' if seat else '좌석 편차 — 측정 위치 1곳')))
        self.filters_label.setText('')
        clear_layout(self.sub_grid)
        for col, head in enumerate(('서브', '방식', '게인', '딜레이', '극성', '올패스')):
            self.sub_grid.addWidget(label(head, 'note'), 0, col)
        for row, value in enumerate(view['values'], 1):
            name = _PlainQt.QLabel(f'━ {value["role"]}')       # 채널 이름은 번역하지 않는다
            name.setStyleSheet(f'color: {SUB_COLORS[(row - 1) % len(SUB_COLORS)]}; font-weight: 700;')
            name.setToolTip(role_label(value['role']))
            allpass = ', '.join(f'{f0:.0f} Hz Q {q:.1f}' for f0, q in value['allpass'])
            cells = (cal.sub_mode_text(value['mode']), f'{value["gain_db"]:+.1f} dB', f'{value["delay_ms"]:.2f} ms',
                     '정상' if value['polarity'] > 0 else '반전', allpass or '없음')
            self.sub_grid.addWidget(name, row, 0)
            for col, text in enumerate(cells, 1):
                # 방식·올패스 칸은 줄바꿈('Bundle A', '60 Hz Q 0.8' 둘) — 좁은 1024 영어에서 표가 본문 칸을 넓히지 않게.
                self.sub_grid.addWidget(label(text, 'spec', wrap=col in (1, len(cells))), row, col)
        self.sub_grid.setColumnStretch(6, 1)
        # 서브별 대역 필터·고정 PEQ(R27) — 표 아래 따로 한 줄(필터가 있는 서브만). 표 열로 넣으면 1024 영어에서 넘치고, 표 칸을
        # 가로지르는 줄바꿈 라벨도 표를 6 px 넓혔다(10-05 캡처).
        fixed = [f'{r} {text}' for r, text in ((r, cal.sub_filter_text(s)) for r, s in self.run.result.subs.items())
                 if text]
        lines = [t(x) for x in view.get('layouts') or []]       # R30 레이아웃 묶음 줄
        text = (f"{t('고정 필터')}: " + ' · '.join(fixed)) if fixed else ''
        text = '\n'.join(lines + ([text] if text else []))
        self.sub_filters_label.setText(text)
        self.sub_filters_label.setVisible(bool(text))
        warns = cal.sub_filter_warnings(source.settings, view['subs']) + cal.sub_gain_warnings(source)
        self.sub_warn_result.setText('\n'.join(warns))
        self.sub_warn_result.setVisible(bool(warns))
        self.sub_values.setVisible(True)

    def _draw_xo(self, hz: float | None = None, update: bool = False) -> None:
        """XO 보기: 고른 그룹의 메인 평균·서브·합·타겟(청취 위치) + XO 선. `update` 면 곡선 데이터만 바꾼다(끄는 중)."""
        run, group = self.run, self.xo_group
        if run is None or not group or group not in run.engine.groups or not run.engine.subs:
            return
        hz = float(hz if hz is not None else self.xo_of(group))
        view = cal.xo_preview(run, group, hz, role=self.channel)
        x = view['grid']
        offset = self._level_offset(run)            # SPL 탭과 같은 타겟 기준 — 곡선 모양 · XO · 점수는 그대로, 보이는 높이만
        if update and self._xo_curves:
            for key in ('main', 'sub', 'sum'):
                self._xo_curves[key].setData(x, view[key] - offset)
            return
        self._xo_curves = {
            'target': self.plot.plot(x, view['target'] - offset,
                                     pen=pg.mkPen(C_TARGET, width=2, style=QtCore.Qt.PenStyle.DashLine)),
            'main': self.plot.plot(x, view['main'] - offset, pen=pg.mkPen(C_XO_MAIN, width=2)),
            'sub': self.plot.plot(x, view['sub'] - offset, pen=pg.mkPen(C_MIMO, width=2)),
            'sum': self.plot.plot(x, view['sum'] - offset, pen=pg.mkPen(C_XO_SUM, width=2))}
        self._syncing = True
        try:
            self.xo_line.setBounds((np.log10(cal.XO_RANGE[0]), np.log10(cal.XO_RANGE[1])))
            self.xo_line.setValue(np.log10(hz))
        finally:
            self._syncing = False
        self.plot.addItem(self.xo_line)
        mid = float(np.median(view['target'])) - offset
        self.plot.setYRange(mid - 30.0, mid + 12.0, padding=0)

    # ──────────────────────────── 내보내기 ────────────────────────────
    def export_base_dir(self) -> Path:
        """8단계와 같은 내보낼 폴더(프로젝트에 고른 값 → 프로젝트 파일 옆 → 문서 폴더)."""
        stored = (self.project.extra.get('export') or {}).get('folder')
        if stored:
            return Path(stored)
        return default_export_dir(self.project_path)

    def _refresh_export_path(self) -> None:
        self.export_path_label.set_full_text(str(self.export_base_dir()))

    def choose_export_folder(self, folder=None) -> Path | None:
        """[변경…] — 폴더를 고른다. 테스트(pytest)는 대화상자를 띄우지 않는다(모달에 멈춘다) — `folder` 를 준다."""
        if folder is None:
            if os.environ.get('PYTEST_CURRENT_TEST'):
                return None
            folder = QtWidgets.QFileDialog.getExistingDirectory(self, t('내보낼 폴더 선택'), str(self.export_base_dir()))
            if not folder:
                return None
        self.project.extra['export'] = {**(self.project.extra.get('export') or {}), 'folder': str(folder)}
        self._refresh_export_path()
        self.changed.emit()
        return Path(folder)

    def open_export_folder(self) -> None:
        folder = self.export_base_dir()
        folder.mkdir(parents=True, exist_ok=True)
        if not os.environ.get('PYTEST_CURRENT_TEST'):
            QtGui.QDesktopServices.openUrl(QtCore.QUrl.fromLocalFile(str(folder)))

    def exporting(self) -> bool:
        return self._exporting

    def export(self, kind: str, folder=None, open_after: bool = False) -> list[str]:
        """`kind` = 'mdat'(REW 단계별) | 'camilla'(카밀라 DSP 설정). `<folder 또는 내보낼 폴더>/SECS_calibration_<모드>_<날짜시간>/`
        에 쓰고 쓴 파일을 돌려준다. 버튼으로 내보내면 그 폴더를 연다(pytest 에서는 안 연다 — 8단계와 같은 규칙)."""
        if getattr(self, 'trial', False):           # 튜토리얼는 파일을 쓰지 않는다(10-09)
            self.export_note.setText('튜토리얼에서는 내보내지 않습니다')
            return []
        run, mimo = self.run, (self.mimo_run if self.show_mimo() else None)
        if run is None:
            self.export_note.setText('계산 결과가 없습니다 — 먼저 [계산]을 누르세요')
            return []
        base = Path(folder) if folder else self.export_base_dir()
        # 폴더 이름은 ASCII(카밀라 GUI 가 CP949 로 읽는다 — 설정 파일 안은 이스케이프하지만 폴더 이름도 되도록 안전하게).
        stamp = time.strftime('%Y%m%d-%H%M%S')
        # 폴더 이름에 무엇이 들었는지(10-09 사용자: 어느 게 RC 고 어느 게 MIMO 인지 써 놔라) — ASCII 만(카밀라 GUI 가 CP949 로 읽는다).
        what = 'RC_BA_MIMO' if mimo is not None else 'RC_BA'
        target = base / f'SECS_{kind}_{what}_{run.settings.mode}_{stamp}'

        def work() -> list[str]:
            if kind == 'mdat':
                return [str(cal.export_mdat(run, target, mimo, cal.project_spl_offset(self.project)))]
            if kind == 'camilla':
                return [str(p) for p in cal.export_camilladsp(run, target / 'camilladsp', mimo)]
            raise ValueError(kind)

        self._exporting = True
        try:
            files = run_off_thread(work, name='secs-calibration-export')
        except Exception as error:  # noqa: BLE001 - 파일 오류는 문구로
            self.export_note.setText(f'내보내기 실패: {error}')
            return []
        finally:
            self._exporting = False
        self.last_files = files
        self.export_note.setText(f'{len(files)}개 파일을 저장했습니다 → {target}')
        if open_after and files and not os.environ.get('PYTEST_CURRENT_TEST'):
            opened = target / 'camilladsp' if kind == 'camilla' else target
            QtGui.QDesktopServices.openUrl(QtCore.QUrl.fromLocalFile(str(opened if opened.is_dir() else target)))
        return files

    # ──────────────────────────── 진행 조건 ────────────────────────────
    def can_advance(self) -> tuple[bool, str]:
        return True, ''          # 내보내기 단계는 결과 없이도 열린다(열면 '계산 결과가 없습니다' 를 보인다)


__all__ = ['StepCalibration', 'TABS']
