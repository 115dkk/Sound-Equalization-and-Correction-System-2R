# -*- coding: utf-8 -*-
"""4단계 측정 위치 배치 — 목업 `step4()`.

배치 선택(1/9/13/17 — 1 위치는 09-24 추가), 목업 `arrPoints` 좌표를 그대로 그린 배치도, 측정 옵션.

`WizardWindow` 가 만들고 `refresh()`·`can_advance()` 를 부른다. 바뀌면 `changed` 를
낸다. 출력은 `project.extra['array']`(위치 수) — 5단계가 이 값으로 측정 계획의 위치
목록을 만든다. 위치 수가 달라지면 기존 측정과 번호가 어긋나므로
`project.measurements` 를 비운다. `project.extra['start_delay']`(측정 시작 지연)은
5단계 `Step5Measure.measure_position` 이 읽어 그만큼 센 뒤 스윕을 시작한다(09-22).

타이밍 레퍼런스(음향/루프백/없음 + 기준 스피커·출력)도 여기서 고른다 — 2026-09-23 단계 순서를
1 장치 → 2 구성으로 바꾸며 장치 단계에서 옮겼다(기준 스피커 목록은 구성 뒤에야 확정된다).
값은 `project.extra['devices']` 의 타이밍 키에 둔다(`wizard.timing`, `.secs` 뜻은 그대로).

`position_seconds()`/`format_estimate()` 는 위치 한 곳의 예상 소요를 계산·표시용
문구로 바꾼다 — 5단계가 현재 위치의 남은 시간을 같은 값으로 보여줄 때도 쓴다(09-23).
"""
from __future__ import annotations

from PyQt6 import QtCore, QtGui
from secs.wizard import qt as QtWidgets

from secs.model import role_code
from secs.wizard.channels import output_channel_names, output_label
from secs.wizard.layout_map import ARRANGEMENTS, ArrayLayoutView, arr_points
from secs.wizard.step1_devices import ChannelCombo
from secs.wizard.timing import TIMING_CHOICES, default_ref_role, measurement_roles
from secs.wizard.strings import t
from secs.wizard.tooltips import attach, attach_all, attach_cells, attach_header
from secs.wizard.style import COLORS, add_heading, card, label, plain_label, set_state, two_columns


#: 목업 step4 의 측정 시작 지연 선택지.
START_DELAYS = ('끄기', '5 초', '10 초', '15 초')

#: 스윕 한 번의 어림 시간에 더하는 REW 처리·전환 시간(초). 실스윕 09-18: 256k 스윕 5.5 s + 약 2.5 s.
SWEEP_OVERHEAD_S = 2.5
#: Acoustic 기준일 때 채널 하나의 처리 여유(초) — 10-09 실측 163 s / 14채널에서.
ACOUSTIC_OVERHEAD_S = 0.7


def start_delay_seconds(text: str) -> int:
    """'5 초' → 5, '끄기' → 0. 저장값(`project.extra['start_delay']`)도 같은 꼴이다."""
    digits = ''.join(ch for ch in str(text or '') if ch.isdigit())
    return int(digits) if digits else 0


def sweep_seconds(project) -> float:
    """스윕 한 번의 어림 시간(초) — 1단계 스윕 길이·샘플레이트에서."""
    from secs.rewapi.interfaces import sweep_length_samples

    devices = dict(project.extra.get('devices', {}) or {})
    rate = float(devices.get('sample_rate') or 48000)
    try:
        samples = sweep_length_samples(str(devices.get('sweep_length') or '256k'))
    except Exception:  # noqa: BLE001 - 모르는 길이는 기본 256k
        samples = 262144
    sweep = samples / max(rate, 1.0)
    # Acoustic 타이밍 기준은 채널마다 기준 스윕을 한 번 더 낸다 — 스윕이 두 배, 처리 여유는 짧다.
    # 10-09 돌비룸 실측(14채널 · 256k · 48 kHz): 위치 하나 163 s = 채널당 11.6 s = 스윕 5.46 s × 2 + 0.7 s.
    # 예전 어림(스윕 1번 + 2.5 s)은 1분 51초였다.
    stored = str(devices.get('timing_reference') or 'acoustic').lower()
    if stored.startswith('acoustic'):
        return 2.0 * sweep + ACOUSTIC_OVERHEAD_S
    return sweep + SWEEP_OVERHEAD_S


def position_seconds(project) -> float:
    """위치 한 곳의 예상 소요(초) = 채널 수 × 스윕 시간 × 반복 수 + 시작 지연.

    이 화면의 "예상 소요"와 5단계의 위치별 안내가 같은 값을 쓴다(2026-09-23 사용자
    지시). 전에는 `_refresh_estimate` 가 전체 합계만 보여줘 "위치 하나에 14분 20초
    걸리나" 하고 오해했다 — 실제 체감 대기시간은 위치당 값이다.
    """
    bands = sum(len(s.bands) for s in project.config.speakers)
    delay = start_delay_seconds(project.extra.get('start_delay', ''))
    devices = dict(project.extra.get('devices', {}) or {})
    try:
        repetitions = int(devices.get('repetitions', 1) or 1)
        if repetitions not in (1, 2, 4, 8):           # 5단계 콤보 · plan() 과 같은 정규화(10-10 코드 리뷰)
            repetitions = 1
    except (TypeError, ValueError):
        repetitions = 1
    if str(devices.get('timing_reference') or 'acoustic').lower().startswith('acoustic'):
        repetitions = 1
    return bands * sweep_seconds(project) * repetitions + delay


def format_estimate(seconds: float) -> str:
    """초 → '약 N 분 M 초' / '약 M 초'. 4·5단계가 같이 쓴다(en_g4 에 두 패턴 등록됨)."""
    minutes, rest = divmod(int(round(seconds)), 60)
    return f'약 {minutes} 분 {rest} 초' if minutes else f'약 {rest} 초'


# ──────────────────────────── 배치 선택 카드 ────────────────────────────
class MiniArrayMap(QtWidgets.QWidget):
    """배치 선택 카드의 작은 도식 — 아래 큰 배치도(`ArrayLayoutView`)와 같은 좌표계(cm, 원점 =
    청취 위치, y 앞 +)로 `arr_points(n)` 의 점을 줄여 그린다. 1번(청취 위치)은 액센트 색.

    카드들이 같은 축척을 쓴다(점 범위 ±80 cm × -60~75 cm 를 칸에 맞춤) — 배치가 넓어지는 것이
    카드끼리 그대로 비교된다. 소파·의자 실루엣과 지름 1 m 점선 원도 큰 배치도와 같이 둔다.
    """

    #: 그림에 담는 범위(cm). 17 위치의 가장 먼 점(±70, -55~70) + 점 반지름 여유.
    SPAN_X = (-82.0, 82.0)
    SPAN_Y = (-62.0, 76.0)

    def __init__(self, count: int, parent=None) -> None:
        super().__init__(parent)
        self._count = int(count)
        self.setFixedHeight(56)
        self.setMinimumWidth(60)
        self.setSizePolicy(QtWidgets.QSizePolicy.Policy.Expanding,
                           QtWidgets.QSizePolicy.Policy.Fixed)
        self.setAttribute(QtCore.Qt.WidgetAttribute.WA_TransparentForMouseEvents)

    def paintEvent(self, _event) -> None:  # noqa: N802 - Qt 이름
        painter = QtGui.QPainter(self)
        painter.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing)
        width, height = self.width(), self.height()
        painter.setPen(QtCore.Qt.PenStyle.NoPen)
        painter.setBrush(QtGui.QColor(COLORS['paper']))
        painter.drawRoundedRect(QtCore.QRectF(0, 0, width, height), 3, 3)
        (x0, x1), (y0, y1) = self.SPAN_X, self.SPAN_Y
        k = min((width - 8) / (x1 - x0), (height - 6) / (y1 - y0))
        cx = width / 2.0 - (x0 + x1) / 2.0 * k
        cy = height / 2.0 + (y0 + y1) / 2.0 * k

        def to_screen(x_cm: float, y_cm: float) -> QtCore.QPointF:
            return QtCore.QPointF(cx + x_cm * k, cy - y_cm * k)

        def rect(x_cm: float, y_cm: float, w_cm: float, h_cm: float) -> QtCore.QRectF:
            top_left = to_screen(x_cm, y_cm)                  # (x, y) = 왼쪽 위(앞쪽) 모서리
            return QtCore.QRectF(top_left.x(), top_left.y(), w_cm * k, h_cm * k)

        # 의자/소파 — 큰 배치도와 같은 치수(cm)
        edge = QtGui.QPen(QtGui.QColor('#8fa3a1'))
        edge.setWidthF(1.0)
        painter.setPen(edge)
        painter.setBrush(QtGui.QColor('#eef2f1'))
        seat = (-95.0, 190.0) if self._count >= 13 else (-30.0, 60.0)
        painter.drawRoundedRect(rect(seat[0], 28, seat[1], 56), 2, 2)
        painter.setBrush(QtGui.QColor('#dfe6e4'))
        painter.drawRoundedRect(rect(seat[0], -16, seat[1], 12), 2, 2)
        # 지름 1 m 안내 원
        dashed = QtGui.QPen(QtGui.QColor('#8fa3a1'))
        dashed.setStyle(QtCore.Qt.PenStyle.DashLine)
        painter.setPen(dashed)
        painter.setBrush(QtCore.Qt.BrushStyle.NoBrush)
        painter.drawEllipse(to_screen(0, 0), 50 * k, 50 * k)
        # 측정 점 — 1번(청취 위치)이 맨 위에 오게 뒤에서부터 그린다
        outline = QtGui.QPen(QtGui.QColor(COLORS['chassis']))
        outline.setWidthF(1.0)
        points = arr_points(self._count)
        for index in range(len(points) - 1, -1, -1):
            point = points[index]
            if index == 0:
                painter.setPen(QtCore.Qt.PenStyle.NoPen)
                painter.setBrush(QtGui.QColor(COLORS['accent']))
                painter.drawEllipse(to_screen(point.x, point.y), 4.2, 4.2)
            else:
                painter.setPen(outline)
                painter.setBrush(QtGui.QColor('#ffffff' if point.h == 0 else COLORS['chassis']))
                painter.drawEllipse(to_screen(point.x, point.y), 2.6, 2.6)
        painter.end()


class ArrangementCard(QtWidgets.QPushButton):
    """배치 선택 카드(목업 .arr button) — 미니 도식 + 이름(굵게) + 부제 + 'N 위치'.

    2026-09-24 사용자: 세 버튼이 글자 길이에 따라 폭이 제각각이고 굵은 글자 세 줄뿐이라 투박했다.
    이제 카드는 모두 같은 폭(행을 등분 — 09-24 부터 4장, 최대 `MAX_WIDTH`)이고, 좁으면 줄바꿈 없이 함께 줄어든다.
    QPushButton 이라 누르기·키보드·`set_state('on')` 선택 표시(QSS `arrPick`)는 전과 같다 —
    안의 위젯은 마우스를 통과시킨다.
    """

    MAX_WIDTH = 230
    #: 4장(09-24 1점 추가) + 사이 8 px 가 1024 창의 본문 칸(≈437 px)에 가로 스크롤 없이 들어가는 폭.
    MIN_WIDTH = 86

    def __init__(self, arrangement, parent=None) -> None:
        super().__init__('', parent)
        self.setObjectName('arrPick')
        self.count = arrangement.count
        name, _dot, sub = arrangement.name.partition(' · ')
        self.setToolTip(arrangement.name)
        box = QtWidgets.QVBoxLayout(self)
        box.setContentsMargins(6, 8, 6, 7)          # 4장이 1024 폭 한 줄에 들어가게 좌우 6(09-24)
        box.setSpacing(2)
        self.map = MiniArrayMap(arrangement.count)
        box.addWidget(self.map)
        box.addSpacing(4)
        self.name_label = label(name, 'arrName')
        self.sub_label = label(sub, 'arrSub')
        count_row = QtWidgets.QHBoxLayout()
        count_row.setContentsMargins(0, 0, 0, 0)
        count_row.setSpacing(4)
        self.count_label = label(str(arrangement.count), 'arrCount')
        count_row.addWidget(self.count_label)
        # 단위는 번역 라벨이 아니라 직접 — 영어는 1 일 때 단수(10-10 AI 시험: "1 positions")
        self.unit_label = plain_label('', 'arrUnit')
        count_row.addWidget(self.unit_label)
        self._set_unit()
        from secs.i18n import language_manager
        language_manager().language_changed.connect(lambda _lang: self._set_unit())
        count_row.addStretch(1)
        box.addWidget(self.name_label)
        box.addWidget(self.sub_label)
        box.addLayout(count_row)
        for child in self.findChildren(QtWidgets.QWidget):
            child.setAttribute(QtCore.Qt.WidgetAttribute.WA_TransparentForMouseEvents)
            if isinstance(child, QtWidgets.QLabel):
                child.setMinimumWidth(0)
                child.setSizePolicy(QtWidgets.QSizePolicy.Policy.Ignored,
                                    QtWidgets.QSizePolicy.Policy.Fixed)
        # 폭은 글자와 무관하게 행을 나눠 갖는다(Ignored) — 카드가 늘 같은 크기.
        self.setSizePolicy(QtWidgets.QSizePolicy.Policy.Ignored,
                           QtWidgets.QSizePolicy.Policy.Fixed)
        self.setMinimumWidth(self.MIN_WIDTH)
        self.setMaximumWidth(self.MAX_WIDTH)


    def _set_unit(self) -> None:
        from secs.i18n import language_manager
        english = language_manager().language != 'ko'
        try:
            self.unit_label.setText(('position' if self.count == 1 else 'positions') if english else '위치')
        except RuntimeError:          # 카드가 지워진 뒤 언어 신호가 온 경우
            pass
    def sizeHint(self) -> QtCore.QSize:  # noqa: N802 - Qt 이름
        layout = self.layout()
        return QtCore.QSize(self.MAX_WIDTH, layout.sizeHint().height() if layout else 120)

    def minimumSizeHint(self) -> QtCore.QSize:  # noqa: N802 - Qt 이름
        layout = self.layout()
        return QtCore.QSize(self.MIN_WIDTH, layout.minimumSize().height() if layout else 120)


class Step4Layout(QtWidgets.QWidget):
    """4 · 측정 위치 배치."""

    changed = QtCore.pyqtSignal()

    def __init__(self, project, devices=None, parent=None) -> None:
        super().__init__(parent)
        self.devices = devices          # REW 채널 이름(기준 출력 표시용). 없으면 번호로.
        self.project = project
        self._building = False
        self._build()
        self.refresh()

    def _build(self) -> None:
        outer = QtWidgets.QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(14)
        add_heading(outer, t('4 · 배치'))

        body, main, side = two_columns(outer)      # 단계 공통 2열(style.two_columns, 09-23 정렬)

        left_card, left_box = card(t('배치 선택'))
        picks = QtWidgets.QHBoxLayout()
        picks.setSpacing(8)
        self._picks: dict[int, QtWidgets.QPushButton] = {}
        picks.addStretch(0)          # 넓은 창에서 카드 묶음을 아래 배치도처럼 가운데에(양쪽 여백)
        for count, arrangement in ARRANGEMENTS.items():
            button = ArrangementCard(arrangement)
            button.clicked.connect(lambda _=False, n=count: self.set_count(n))
            attach(button, 's4.arrangement_single' if count == 1 else 's4.arrangement')
            picks.addWidget(button, 1)
            self._picks[count] = button
        picks.addStretch(0)
        left_box.addLayout(picks)
        self.view = attach(ArrayLayoutView(), 's4.map')
        left_box.addWidget(self.view, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)
        left_box.addStretch(1)
        main.addWidget(left_card)

        right_card, right_box = card(t('측정 옵션'))
        grid = QtWidgets.QGridLayout()
        grid.setHorizontalSpacing(12)
        grid.setVerticalSpacing(6)
        self.delay_combo = QtWidgets.QComboBox()
        self.delay_combo.addItems(START_DELAYS)
        attach(self.delay_combo, 's4.start_delay')
        self.delay_combo.setCurrentIndex(0)      # 기본 끄기 — 켜면 [이 위치 측정] 뒤 그만큼 센 다음 스윕(09-22)
        self.delay_combo.currentIndexChanged.connect(lambda _=0: (self._store(), self._refresh_estimate()))
        self.sweeps_label = label('—', 'num', wrap=True)
        # 주 표시 = 위치당 시간, 작은 글자 = 전체 합계(09-23) — 한 칸에 두 줄.
        self.estimate_label = label('—', 'num', wrap=True)
        self.estimate_sub_label = label('', 'note', wrap=True)
        estimate_holder = QtWidgets.QWidget()
        estimate_box = QtWidgets.QVBoxLayout(estimate_holder)
        estimate_box.setContentsMargins(0, 0, 0, 0)
        estimate_box.setSpacing(0)
        estimate_box.addWidget(self.estimate_label)
        estimate_box.addWidget(self.estimate_sub_label)
        self._options_grid = grid
        self._options_rows = [(label(name), widget) for name, widget in (
            ('측정 시작 지연', self.delay_combo),
            ('위치당 스윕', self.sweeps_label),
            ('예상 소요', estimate_holder),
            ('타이밍 레퍼런스', self._timing_widgets()),
            ('레퍼런스 출력', self._ref_holder))]
        self._options_stacked: bool | None = None
        self._options_card = right_card
        self._place_options(False)
        right_card.installEventFilter(self)
        right_box.addLayout(grid)
        right_box.addStretch(1)
        side.box.addWidget(right_card)
        outer.addStretch(1)

    #: 측정 옵션 카드가 이보다 좁으면 이름을 값 위로 올린다(px).
    OPTIONS_STACK_BELOW = 400

    def _place_options(self, stacked: bool) -> None:
        """측정 옵션 칸 배치 — 넓으면 이름 | 값 두 열, 좁으면 이름 위·값 아래 한 열.

        1024 영어에서 오른쪽 칸(≈290 px)을 두 열로 나누면 값 열이 ≈125 px 뿐이라 콤보 글자가
        잘리고 예상 소요가 다섯 줄로 접혔다(빌드 전 QA 2026-09-24).
        """
        if stacked == self._options_stacked:
            return
        self._options_stacked = stacked
        grid = self._options_grid
        for name_label, widget in self._options_rows:
            grid.removeWidget(name_label)
            grid.removeWidget(widget)
        top = QtCore.Qt.AlignmentFlag.AlignTop
        for row, (name_label, widget) in enumerate(self._options_rows):
            # 한 열일 때 행 사이 틈은 이름 라벨 위 여백으로 준다(이름과 값은 붙인다).
            name_label.setContentsMargins(0, 8 if stacked and row else 0, 0, 0)
            if stacked:
                grid.addWidget(name_label, row * 2, 0, 1, 2, top)
                grid.addWidget(widget, row * 2 + 1, 0, 1, 2)
            else:
                # 값이 두 줄(예상 소요)이어도 이름이 첫 줄과 나란히 오게 위쪽 정렬(09-23 QA A)
                grid.addWidget(name_label, row, 0, top)
                grid.addWidget(widget, row, 1, top)
        grid.setColumnStretch(0, 1 if stacked else 0)
        grid.setColumnStretch(1, 0 if stacked else 1)
        grid.setVerticalSpacing(2 if stacked else 6)

    def eventFilter(self, watched, event) -> bool:  # noqa: N802 - Qt 이름
        # 카드 폭은 창 크기에 맞춘 오른쪽 칸 폭(style.set_side_width)을 따른다 — 카드 자체의 크기
        # 변화를 본다(단계 화면의 resizeEvent 는 오른쪽 칸 폭이 정해지기 전에 올 수 있다).
        if watched is self._options_card and event.type() == QtCore.QEvent.Type.Resize:
            width = self._options_card.width()
            if width > 0:
                self._place_options(width < self.OPTIONS_STACK_BELOW)
        return super().eventFilter(watched, event)

    # ──────────────────────────── 상태 ────────────────────────────
    def count(self) -> int:
        stored = int(self.project.extra.get('array', 9) or 9)
        return stored if stored in ARRANGEMENTS else 9

    def set_count(self, count: int) -> None:
        if int(count) == self.count() and 'array' in self.project.extra:
            self.refresh()               # 같은 배치를 다시 누른 것 — 측정 기록을 지우지 않는다
            return                       # (실사용 QA 09-19, QA-B: 재클릭에 기록이 사라졌다)
        self.project.extra['array'] = int(count)
        self.project.measurements.clear()
        self.refresh()
        self.changed.emit()

    def points(self):
        """고른 배치의 마이크 위치 목록(테스트 전용). 화면은 `arr_points` 를 직접 쓴다."""
        return arr_points(self.count())

    def _store(self) -> None:
        self.project.extra['start_delay'] = self.delay_combo.currentText()
        self.changed.emit()

    # ──────────────────────────── 타이밍 레퍼런스 ────────────────────────────
    def _timing_widgets(self) -> QtWidgets.QWidget:
        """종류 콤보를 돌려주고, 기준 출력 칸(`self._ref_holder`)도 만든다(2026-09-18 사용자 지시)."""
        self.timing_combo = QtWidgets.QComboBox()
        for key, text in TIMING_CHOICES:
            self.timing_combo.addItem(text, key)
        self.timing_combo.currentIndexChanged.connect(self._timing_changed)
        self.timing_role_combo = QtWidgets.QComboBox()       # 음향: 기준음을 낼 스피커
        self.timing_role_combo.currentIndexChanged.connect(self._store_timing)
        self.timing_channel = ChannelCombo()                 # 루프백: 되돌린 출력 채널(이름으로 고른다)
        self.timing_channel.valueChanged.connect(self._store_timing)
        self.timing_none = label('레퍼런스 출력 없음', 'note')
        attach(self.timing_combo, 's4.timing')
        attach(self.timing_role_combo, 's4.timing_speaker')
        attach(self.timing_channel, 's4.timing_channel')
        for combo in (self.timing_combo, self.timing_role_combo):
            # 긴 항목이 카드 최소 폭을 밀지 않게 — 좁은 창에서 가로 스크롤 방지.
            combo.setSizeAdjustPolicy(
                QtWidgets.QComboBox.SizeAdjustPolicy.AdjustToMinimumContentsLengthWithIcon)
            combo.setMinimumContentsLength(10)
        self._ref_holder = QtWidgets.QWidget()
        row = QtWidgets.QHBoxLayout(self._ref_holder)
        row.setContentsMargins(0, 0, 0, 0)
        for widget in (self.timing_role_combo, self.timing_channel, self.timing_none):
            row.addWidget(widget, 1)
        return self.timing_combo

    def _refresh_timing(self) -> None:
        """저장된 타이밍 선택과 기준 스피커 목록(구성이 바뀌면 따라간다)을 다시 채운다."""
        stored = dict(self.project.extra.get('devices', {}) or {})
        kind = str(stored.get('timing_reference') or 'acoustic').lower()
        index = self.timing_combo.findData(kind)
        self.timing_combo.blockSignals(True)
        self.timing_combo.setCurrentIndex(index if index >= 0 else 0)
        self.timing_combo.blockSignals(False)

        roles = measurement_roles(self.project)
        wanted = str(stored.get('timing_ref_role') or '')
        if wanted not in {role for role, _out in roles}:
            wanted = default_ref_role(self.project)
        names = output_channel_names(self.devices) if self.devices is not None else []
        self.timing_role_combo.blockSignals(True)
        self.timing_role_combo.clear()
        for role, out in roles:
            self.timing_role_combo.addItem(f'{role_code(role)} · {output_label(self.devices, out, names)}', role)
        self.timing_role_combo.setCurrentIndex(max(self.timing_role_combo.findData(wanted), 0))
        self.timing_role_combo.blockSignals(False)

        self.timing_channel.blockSignals(True)
        channels = max((d.channels for d in (self.devices.output_devices() if self.devices is not None else [])),
                       default=8)
        self.timing_channel.set_names(names, fallback='Out {0}', count=channels)
        if stored.get('timing_ref_output'):
            self.timing_channel.setValue(int(stored['timing_ref_output']))
        self.timing_channel.blockSignals(False)
        self._show_timing_widgets()

    def _show_timing_widgets(self) -> None:
        kind = str(self.timing_combo.currentData() or 'acoustic')
        self.timing_role_combo.setVisible(kind == 'acoustic')
        self.timing_channel.setVisible(kind == 'loopback')
        self.timing_none.setVisible(kind == 'none')

    def _timing_changed(self, *_args) -> None:
        self._show_timing_widgets()
        self._store_timing()

    def _store_timing(self, *_args, emit: bool = True) -> None:
        """타이밍 키만 고친다 — 같은 사전의 장치 값(1단계)은 그대로 둔다."""
        devices = dict(self.project.extra.get('devices', {}) or {})
        devices.update({
            'timing_reference': str(self.timing_combo.currentData() or 'acoustic'),
            'timing_ref_role': str(self.timing_role_combo.currentData() or ''),
            'timing_ref_output': int(self.timing_channel.value()),
        })
        self.project.extra['devices'] = devices
        if emit and not self._building:
            self.changed.emit()

    def refresh(self) -> None:
        self._building = True
        try:
            self._refresh_timing()
            self._store_timing(emit=False)
        finally:
            self._building = False
        count = self.count()
        for key, button in self._picks.items():
            set_state(button, 'on' if key == count else '')
        done = sorted({int(m.get('pos', 0)) - 1 for m in self.project.measurements})
        self.view.set_state(count, [p for p in done if p >= 0], -1)
        bands = sum(len(s.bands) for s in self.project.config.speakers)
        self.sweeps_label.setText(f'{bands} 채널 순차')
        # 콤보를 저장된 값으로 맞춘다 — 안 하면 프로젝트를 불러온 직후 콤보는 '끄기'인데
        # 예상 소요(project.extra 값을 쓴다)는 저장된 지연을 반영해 서로 어긋난다(09-23).
        stored_delay = str(self.project.extra.get('start_delay') or '')
        if stored_delay in START_DELAYS and stored_delay != self.delay_combo.currentText():
            self.delay_combo.blockSignals(True)
            self.delay_combo.setCurrentIndex(self.delay_combo.findText(stored_delay))
            self.delay_combo.blockSignals(False)
        self._refresh_estimate()
        self.project.extra.setdefault('array', count)
        self.project.extra.setdefault('start_delay', self.delay_combo.currentText())

    def _refresh_estimate(self) -> None:
        """예상 소요 — 주 표시는 위치당 시간, 작은 글자는 전체 합계(2026-09-23 사용자 지시).

        전에는 합계만 보여줘 "위치 하나에 14분 20초"로 오해했다. 위치당 값 = 채널 수 ×
        스윕 한 번 + 시작 지연(`position_seconds`, 4·5단계 공용). 지연·스윕 길이를 바꾸면
        따라간다(09-22).
        """
        count = self.count()
        per_position = position_seconds(self.project)
        self.estimate_label.setText(f'위치당 {format_estimate(per_position)}')
        self.estimate_sub_label.setText(
            f'{count} 위치 합계 {format_estimate(count * per_position)} · 마이크 이동 시간 제외')

    def can_advance(self) -> tuple[bool, str]:
        if not self.project.config.speakers:
            return False, '2단계에서 스피커를 먼저 구성하세요.'
        return True, ''


__all__ = ['Step4Layout', 'START_DELAYS', 'position_seconds', 'format_estimate',
           'start_delay_seconds', 'sweep_seconds']
