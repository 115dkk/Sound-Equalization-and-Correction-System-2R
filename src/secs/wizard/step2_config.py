# -*- coding: utf-8 -*-
"""2단계 구성(2026-09-23 전에는 1단계) — 목업 `step1()`(출력 패치 시트)을 Qt 로 옮긴 것.

프리셋 · 서브 스텝퍼 · 바이앰프 기본값 · 스피커 표(역할/크기/구동/대역/측정 출력/극성)
· 배치도 · 선택 스피커 상세 · DSP 프로파일 카드 · 검증 메시지. 바이앰프 칸과 구동(앰프 구성) · 대역 열은
`secs.wizard.BIAMP_UI` 가 켜졌을 때만 만든다(2026-10-11 사용자: 바이앰프 잠시 끔 — 코드는 그대로).

`WizardWindow` 가 만들고 `refresh()`(모델 → 화면)와 `can_advance()`(다음으로 갈 수
있는지와 그 이유)를 부른다. 값이 바뀌면 `changed` 를 낸다. 입력이자 출력은
`project.config`(스피커·출력 채널·측정 장치)와 `project.dsp`(DSP 프로파일 카드) —
모든 편집은 즉시 거기에 반영된다. 프리셋 교체와 서브 수 변경은 채널 구성이 달라지는
일이라 `project.measurements` 를 비운다.
"""
from __future__ import annotations

from PyQt6 import QtCore
from PyQt6 import QtWidgets as _PlainQt
from secs.wizard import qt as QtWidgets

from secs.i18n import language_manager
from secs.model import (PRESETS, ROLES, Band, DspProfile, Speaker, SpeakerConfig, role_code,
                        default_bands, role_group, role_kind, role_label)
from secs.wizard.channels import output_channel_names, output_label
from secs.wizard.layout_map import RoomLayoutView
from secs.wizard.strings import t
from secs.wizard.tooltips import attach, attach_all, attach_cells, attach_header
from secs.wizard.style import (COLORS, FONT_MONO, add_heading, band_chip, card, chip,
                               clear_layout, help_box, label, pill, plain_label, repolish,
                               set_state, set_variant, two_columns)

#: 목업 step1 의 그룹 표시 순서(10-10: 탑 · 하이트를 나눔 — `model.ROLES` 순서대로 탑이 먼저).
GROUP_ORDER = ('프론트', '센터', '서브', '서라운드', '백', '와이드', '탑', '하이트', '기타')

#: 종류(`role_kind`) → 상세 칸의 알약 글자.
KIND_PILLS = {'main': '메인', 'sub': '서브', 'top': '탑', 'height': '하이트'}

#: 목업 DEVICES 의 pre — 장치 이름/id 에 이 낱말이 있으면 그 접두어를 쓴다(앞에서부터 먼저 맞는 것).
#: Dante Virtual Soundcard → 'Tx', Tesira AVB → 'AVB', 그 밖(MOTU·Realtek 등) → 'Out'.
CHANNEL_PREFIXES: tuple[tuple[str, str], ...] = (
    ('dante', 'Tx'), ('dvs', 'Tx'),
    ('avb', 'AVB'), ('tesira', 'AVB'),
)
DEFAULT_CHANNEL_PREFIX = 'Out'


def channel_prefix(device: str) -> str:
    """장치 이름 → 출력 채널 접두어(목업 DEVICES[].pre)."""
    text = str(device or '').lower()
    for keyword, prefix in CHANNEL_PREFIXES:
        if keyword in text:
            return prefix
    return DEFAULT_CHANNEL_PREFIX


def channel_name(device: str, out: int) -> str:
    """목업 chName(o) — `접두어 번호`."""
    return f'{channel_prefix(device)} {int(out)}'


#: 목업 DSP 프로파일 카드의 FIR 탭 선택지.
FIR_TAP_CHOICES = (512, 1024, 2048, 4096, 8192, 16384, 32768, 65536)
# DSP 프로파일 콤보에 보이는 짧은 이름(09-24). 전체 이름(`DspProfile.name`)은 툴팁 — 오른쪽 칸 폭에 맞춘다.
_DSP_SHORT_NAMES = {
    'tesira': 'Tesira SERVER-IO',
    'minidsp': 'miniDSP Flex/2x4HD',
    'camilla': 'CamillaDSP/BruteFIR',
}



def fir_tap_label(taps: int) -> str:
    """채널당 FIR 탭 콤보 항목 — '2,048 탭 (지연 21.3 ms)'. 지연 = 선형위상 FIR 의 지연(탭/2, 48 kHz).

    09-24 사용자: 전에는 '2,048 · 21.3 ms' 로 단위·이름이 없어 아래 요약 줄의 창 길이(≈ 43 ms)와
    헷갈렸다. 번역은 '{0} 탭 (지연 {1} ms)' 패턴(en_g5)이 한다.
    """
    # 10-10 사용자 결정(3A): 선형 위상일 때의 최대 지연(N/2)임을 밝힌다 — 보정 모드의 '지연 10 ms 이내'(실제 설계 지연)와
    # 뜻이 달라 보였다(AI 시험 2차 7번: 구성 '682.7 ms' vs 모드 '10 ms').
    return f'{int(taps):,} 탭 (선형 위상 최대 {(int(taps) - 1) / 48000 / 2 * 1000:.1f} ms)'


#: 이 폭보다 좁으면 세로로 쌓는다(가로 배치의 최소 폭 ≈ 1,007 px).
_WIDE_LAYOUT_MIN = 1010

#: 바이앰프일 때만 뜻이 있는 표 열 — 2 앰프 구성(싱글 · 바이 · 트라이), 3 대역(FULL · LF · HF · SUB 칩).
_BIAMP_COLUMNS = (2, 3)
#: 남는 폭을 나눠 받는 열(채널 · 크기 · 측정 출력)과 열마다 더 받는 상한(px). 극성 · × 는 내용 폭 그대로.
#: 10-11 사용자: "바이앰프 빠진만큼 폭 열 조정해줘" — 열 두 개가 빠지자 표 오른쪽이 비어 보였다.
_SPREAD_COLUMNS, _SPREAD_MAX = (0, 1, 4), 160


def biamp_ui() -> bool:
    """화면에서 바이앰프를 고를 수 있는지(`secs.wizard.BIAMP_UI`, 10-11 부터 끔). 부를 때 읽는다 — 시험이 켤 수 있게."""
    import secs.wizard

    return bool(secs.wizard.BIAMP_UI)


def _v_centered(widget):
    """바이앰프처럼 여러 행에 걸친 칸에서 드롭다운이 세로로 늘어나지 않게 한 줄 높이로 가운데 둔다."""
    holder = QtWidgets.QWidget()
    box = QtWidgets.QVBoxLayout(holder)
    box.setContentsMargins(0, 0, 0, 0)
    box.addStretch(1)
    box.addWidget(widget)
    box.addStretch(1)
    return holder


class Step2Config(QtWidgets.QWidget):
    """1 · 시스템 구성."""

    changed = QtCore.pyqtSignal()

    def __init__(self, project, devices=None, parent=None) -> None:
        super().__init__(parent)
        self.project = project
        self.devices = devices
        self._selected = -1
        self._building = False
        #: 이 화면이 바이앰프 칸 · 열을 가졌는가 — 만들 때 한 번 읽어 칸 · 표 · 프리셋 · 스피커 추가가 같은 값을 쓴다.
        self._biamp_ui = biamp_ui()
        self._build()
        self.refresh()

    # ──────────────────────────── 뼈대 ────────────────────────────
    def _build(self) -> None:
        outer = QtWidgets.QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(14)
        add_heading(outer, t('2 · 구성'))

        # 상단: 프리셋 + 서브 스텝퍼 + 바이앰프 옵션 (목업 .cfgtop, 바이앰프 칸은 `BIAMP_UI` 일 때만)
        top = QtWidgets.QBoxLayout(QtWidgets.QBoxLayout.Direction.LeftToRight)
        top.setSpacing(16)
        self._top = top
        self._presets_row = QtWidgets.QHBoxLayout()
        self._presets_row.setSpacing(8)
        self._preset_buttons: dict[str, QtWidgets.QPushButton] = {}
        for name in list(PRESETS) + ['custom']:
            text = '빈 구성' if name == 'custom' else name
            sub = '직접 추가' if name == 'custom' else f'{len(PRESETS[name])} 스피커'
            button = QtWidgets.QPushButton(f'{text}\n{sub}')
            button.setObjectName('preset')
            button.setMinimumWidth(84)
            button.clicked.connect(lambda _=False, key=name: self._apply_preset(key))
            attach(button, 's2.preset_custom' if name == 'custom' else 's2.preset')
            self._preset_buttons[name] = button
            self._presets_row.addWidget(button)
        self._presets_row.addStretch(1)
        top.addLayout(self._presets_row, 1)

        right_top = QtWidgets.QVBoxLayout()
        right_top.setSpacing(4)
        stepper = QtWidgets.QHBoxLayout()
        stepper.setSpacing(6)
        stepper.addWidget(label('서브우퍼'))
        self._sub_minus = QtWidgets.QPushButton('−')
        self._sub_minus.setObjectName('stp')
        self._sub_minus.setToolTip('서브 하나 줄이기')
        self._sub_minus.clicked.connect(lambda: self._set_sub_count(-1))
        self._sub_count = label('0', 'num')
        self._sub_count.setMinimumWidth(20)
        self._sub_count.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        self._sub_plus = QtWidgets.QPushButton('+')
        self._sub_plus.setObjectName('stp')
        self._sub_plus.setToolTip('서브 하나 늘리기')
        self._sub_plus.clicked.connect(lambda: self._set_sub_count(+1))
        attach_all((self._sub_minus, self._sub_plus), 's2.sub_count')
        stepper.addWidget(self._sub_minus)
        stepper.addWidget(self._sub_count)
        stepper.addWidget(self._sub_plus)
        stepper.addStretch(1)
        right_top.addLayout(stepper)
        self._right_top = right_top
        # 바이앰프 칸 — 10-11 부터 닫았다(`secs.wizard.BIAMP_UI`). 닫혀 있으면 만들지 않는다(None).
        self._biamp = None
        if self._biamp_ui:
            self._biamp = QtWidgets.QCheckBox('메인 스피커는 바이앰프(LF/HF)로 시작')
            self._biamp.toggled.connect(self._on_biamp)
            attach(self._biamp, 's2.biamp')
            right_top.addWidget(self._biamp, 0, QtCore.Qt.AlignmentFlag.AlignRight)
        top.addLayout(right_top)

        # 단계 공통 2열(09-23 정렬): 본문 = 프리셋 줄 + 출력 패치 시트, 오른쪽 칸 = 배치도 · DSP 프로파일.
        # 목업은 배치도가 왼쪽이었지만 모든 단계의 오른쪽 칸을 같은 자리·폭으로 맞추려고 옮겼다.
        body, main, side = two_columns(outer)
        main.addLayout(top)
        self._body, self._side = body, side

        left_card, left_box = card(t('배치도'))
        self._left_card = left_card
        self.room = RoomLayoutView()
        self.room.speaker_clicked.connect(self._select_speaker)
        attach(self.room, 's2.room')
        left_box.addWidget(self.room, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)
        self._detail = QtWidgets.QVBoxLayout()
        self._detail.setSpacing(4)
        left_box.addLayout(self._detail)
        left_box.addWidget(self._dsp_card())
        left_box.addStretch(1)
        side.box.addWidget(left_card)

        right_card, right_box = card(t('출력 패치 시트'))
        right_box.addLayout(self._device_row())
        self.table = QtWidgets.QTableWidget(0, 7)
        self.table.setHorizontalHeaderLabels(
            ['채널', '크기', '앰프 구성', '대역', '측정 출력', '극성', ''])
        self.table.verticalHeader().setVisible(False)
        self.table.setSelectionMode(QtWidgets.QAbstractItemView.SelectionMode.NoSelection)
        self.table.setEditTriggers(QtWidgets.QAbstractItemView.EditTrigger.NoEditTriggers)
        self.table.setShowGrid(False)
        header = self.table.horizontalHeader()
        # 열 폭은 `_fit_columns` 가 셀 위젯을 재서 정한다 — ResizeToContents 는 QSS 글꼴이 붙기 전의
        # 폭으로 재어 드롭다운 오른쪽(글자·화살표)이 잘렸다(09-23 사용자).
        for column in range(7):
            header.setSectionResizeMode(column, QtWidgets.QHeaderView.ResizeMode.Fixed)
        header.setStretchLastSection(False)     # × 열은 단추 폭으로 고정하고 남는 폭은 `_spread_columns` 가 나눈다(10-11)
        attach_header(self.table, ['s2.role', 's2.size', 's2.drive', 's2.band', 's2.out', 's2.polarity', ''])
        # 앰프 구성 · 대역 열은 바이앰프일 때만 뜻이 있다 — 닫혀 있으면 열을 숨기고 칸 위젯도 만들지 않는다(10-11).
        for column in _BIAMP_COLUMNS:
            self.table.setColumnHidden(column, not self._biamp_ui)
        self.table.setMinimumHeight(320)
        self.table.viewport().installEventFilter(self)       # 표 폭이 바뀌면 남는 폭을 다시 나눈다(`_spread_columns`)
        self.table.setHorizontalScrollMode(
            QtWidgets.QAbstractItemView.ScrollMode.ScrollPerPixel)
        right_box.addWidget(self.table, 1)

        # 요약은 두 줄(글자 줄 · 상태 알약 줄) — 한 줄이면 영어에서 최소 폭이 716 px 라 1024 창에서
        # 페이지 전체가 가로로 28 px 밀렸다(빌드 전 QA 2026-09-24).
        self._summary = QtWidgets.QVBoxLayout()
        self._summary.setSpacing(6)
        right_box.addLayout(self._summary)

        actions = QtWidgets.QHBoxLayout()
        actions.setSpacing(8)
        add_button = QtWidgets.QPushButton(t('+ 스피커 추가'))
        set_variant(add_button, 'accent', 'sm')
        add_button.clicked.connect(self._add_speaker)
        renumber_button = QtWidgets.QPushButton(t('출력 번호 자동 정렬'))
        set_variant(renumber_button, 'ghost', 'sm')
        renumber_button.clicked.connect(self._renumber)
        csv_button = QtWidgets.QPushButton(t('패치 시트 CSV'))
        set_variant(csv_button, 'ghost', 'sm')
        csv_button.clicked.connect(self._export_csv)
        self.csv_button = csv_button
        attach(add_button, 's2.add')
        attach(renumber_button, 's2.renumber')
        attach(csv_button, 's2.csv')
        for button in (add_button, renumber_button, csv_button):
            actions.addWidget(button)
        actions.addStretch(1)
        right_box.addLayout(actions)

        self._messages = help_box('')
        right_box.addWidget(self._messages)
        main.addWidget(right_card)
        outer.addStretch(1)

    # ──────────────────────────── 좁은 창 ────────────────────────────
    def resizeEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().resizeEvent(event)
        self._apply_width(self._available_width())

    def _scroll_viewport(self):
        parent = self.parentWidget()
        while parent is not None:
            if isinstance(parent, QtWidgets.QAbstractScrollArea):
                return parent.viewport()
            parent = parent.parentWidget()
        return None

    def _available_width(self) -> int:
        """쓸 수 있는 폭. 스크롤 영역 안에서는 내 폭이 최소 폭 밑으로 안 줄어들어(영어처럼 글자가
        길면 넓은 배치에 갇힌다 — 실사용 QA 09-20) 보이는 영역의 폭을 기준으로 삼는다."""
        viewport = self._scroll_viewport()
        if viewport is None:
            return self.width()
        margins = self.parentWidget().layout().contentsMargins() if self.parentWidget().layout() else None
        pad = (margins.left() + margins.right()) if margins is not None else 0
        return min(self.width(), viewport.width() - pad)

    def showEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().showEvent(event)
        viewport = self._scroll_viewport()
        if viewport is not None and not getattr(self, '_watching_viewport', False):
            self._watching_viewport = True
            viewport.installEventFilter(self)
        if getattr(self, '_fit_pending', False):       # 숨어 있을 때 다시 만든 표는 보일 때 잰다
            self._fit_pending = False
            self._fit_columns()

    def eventFilter(self, watched, event) -> bool:  # noqa: N802 - Qt 이름
        if event.type() == QtCore.QEvent.Type.Resize:
            if watched is self.table.viewport():
                self._spread_columns()
            else:
                self._apply_width(self._available_width())
        return super().eventFilter(watched, event)

    def _apply_width(self, width: int) -> None:
        """좁으면 상단 줄과 본문을 세로로 쌓는다(1024 폭에서 가로 스크롤이 생겼다 — 실사용 QA 09-19)."""
        narrow = width < _WIDE_LAYOUT_MIN
        direction = (QtWidgets.QBoxLayout.Direction.TopToBottom if narrow
                     else QtWidgets.QBoxLayout.Direction.LeftToRight)
        if self._body.direction() == direction:
            return
        self._body.setDirection(direction)
        self._top.setDirection(direction)
        self._side.set_stacked(narrow)          # 좁으면 배치도가 표 아래로 — 그땐 전체 폭
        side = (QtCore.Qt.AlignmentFlag.AlignLeft if narrow
                else QtCore.Qt.AlignmentFlag.AlignRight)
        if self._biamp is not None:
            self._right_top.setAlignment(self._biamp, side)

    # ──────────────────────────── 장치 줄 · DSP 카드 ────────────────────────────
    def _device_row(self) -> QtWidgets.QHBoxLayout:
        row = QtWidgets.QHBoxLayout()
        row.setSpacing(12)
        row.addWidget(label('측정 오디오 장치'))
        self.device_combo = QtWidgets.QComboBox()
        self.device_combo.setMinimumWidth(240)
        # 긴 장치 이름은 자리가 있으면 다 보이게 늘리고, 좁으면 툴팁으로 전체를 본다(실사용 QA 09-24).
        self.device_combo.setSizeAdjustPolicy(QtWidgets.QComboBox.SizeAdjustPolicy.AdjustToContents)
        self.device_combo.currentIndexChanged.connect(self._on_device)
        attach(self.device_combo, 's2.device')
        row.addWidget(self.device_combo)
        self.device_pill = pill('', 'ok')
        row.addWidget(self.device_pill)
        row.addStretch(1)
        return row

    def _dsp_card(self) -> QtWidgets.QFrame:
        frame, box = card(t('DSP 프로파일 · 필터 자원'))
        grid = QtWidgets.QGridLayout()
        grid.setHorizontalSpacing(10)
        grid.setVerticalSpacing(4)
        self.dsp_profile = QtWidgets.QComboBox()
        # 오른쪽 칸은 폭이 고정이라 긴 이름('Biamp Tesira (SERVER-IO)')이 1920 폭에서도 잘렸다(실사용 QA 09-24)
        # → 콤보에는 짧은 이름, 전체 이름은 항목·콤보 툴팁. 저장·시트는 키와 preset.name 그대로라 영향 없다.
        self.dsp_profile.setSizeAdjustPolicy(QtWidgets.QComboBox.SizeAdjustPolicy.AdjustToContents)
        for key, preset in DspProfile.presets().items():
            short = _DSP_SHORT_NAMES.get(key, preset.name)
            self.dsp_profile.addItem(short, key)
            if short != preset.name:
                self.dsp_profile.setItemData(self.dsp_profile.count() - 1, preset.name,
                                             QtCore.Qt.ItemDataRole.ToolTipRole)
        self.dsp_profile.currentIndexChanged.connect(self._on_profile)
        self.dsp_taps = QtWidgets.QComboBox()
        for taps in FIR_TAP_CHOICES:
            self.dsp_taps.addItem(fir_tap_label(taps), taps)
        self.dsp_taps.currentIndexChanged.connect(self._on_taps)
        self.dsp_budget = QtWidgets.QSpinBox()
        self.dsp_budget.setRange(0, 4_194_304)
        self.dsp_budget.setSingleStep(1024)
        # 단위(09-24 사용자: "단위도 같이 적어 줘") — 표시 형식만, 저장 값은 그대로 정수.
        # 접미사는 `_refresh_dsp` 가 현재 언어로 단다(언어를 바꾸면 refresh 가 다시 단다).
        self.dsp_budget.setGroupSeparatorShown(True)
        self.dsp_budget.valueChanged.connect(self._on_budget)
        self.dsp_iir = QtWidgets.QSpinBox()
        self.dsp_iir.setRange(0, 64)
        self.dsp_iir.valueChanged.connect(self._on_iir)
        # 혼합위상의 위상 올패스 예산(09-23, 개정 7) — Tesira 는 PEQ 와 별도의 All-Pass 블록.
        self.dsp_allpass = QtWidgets.QSpinBox()
        self.dsp_allpass.setRange(0, 64)
        self.dsp_allpass.valueChanged.connect(self._on_allpass)
        # 베이스 매니지먼트 경로 지연 수(09-24, 설계안 개정 6 C1) — 채널별 서브 경로 지연 자리. 0 이면 공통 하나.
        self.dsp_bm_delays = QtWidgets.QSpinBox()
        self.dsp_bm_delays.setRange(0, 64)
        self.dsp_bm_delays.valueChanged.connect(self._on_bm_delays)
        names: dict[int, QtWidgets.QWidget] = {}        # 이름 칸에도 같은 풀이(10-10 AI 시험: '경로 지연' 풀이 없음)
        for row, (text, widget) in enumerate((
                ('장치', self.dsp_profile), ('채널당 FIR 탭', self.dsp_taps),
                ('총 FIR 탭 예산', self.dsp_budget), ('채널당 IIR 밴드', self.dsp_iir),
                ('채널당 올패스 밴드', self.dsp_allpass),
                # 두 줄 — 가장 긴 이름이 이름 칸 폭을 정해 한국어 값 칸이 5 px 모자랐다(09-24 QA).
                ('베이스 매니지먼트\n경로 지연', self.dsp_bm_delays))):
            name = label(text)
            names[id(widget)] = name
            grid.addWidget(name, row, 0)
            grid.addWidget(widget, row, 1)
        # 남는 폭은 값 칸으로(이름 칸은 글자 폭만) — 값에 단위('2,048 탭 (지연 21.3 ms)')를 넣으며 400 px
        # 오른쪽 칸에서 콤보 글자가 잘렸다(09-24 QA). 영어 이름도 짧게('FIR taps / ch', en_g1).
        grid.setColumnStretch(1, 1)
        for widget, key in ((self.dsp_profile, 's2.dsp_profile'), (self.dsp_taps, 's2.fir_taps'),
                            (self.dsp_budget, 's2.fir_budget'), (self.dsp_iir, 's2.iir_bands'),
                            (self.dsp_allpass, 's2.allpass_bands'), (self.dsp_bm_delays, 's2.bm_delays')):
            attach(widget, key)
            attach(names[id(widget)], key)
        box.addLayout(grid)
        self.dsp_note = label('', 'note', wrap=True)
        attach(self.dsp_note, 's2.fir_note')
        box.addWidget(self.dsp_note)
        return frame

    # ──────────────────────────── 갱신 ────────────────────────────
    @property
    def config(self) -> SpeakerConfig:
        return self.project.config

    def refresh(self) -> None:
        """모델 → 화면. 목업 render() 에 해당한다."""
        self._names_cache = None      # REW 채널 이름은 refresh 마다 새로
        self._devices_cache = None    # 장치 목록도 — 한 번 그리는 동안엔 한 번만 묻는다
        self._building = True
        try:
            self._refresh_top()
            self._refresh_devices()
            self._refresh_dsp()
            self._rebuild_table()
            self._refresh_summary()
            self._refresh_detail()
            self.room.set_speakers(self.config.speakers, self._selected)
        finally:
            self._building = False
            self._devices_cache = None

    def _refresh_top(self) -> None:
        for name, button in self._preset_buttons.items():
            set_state(button, 'on' if self.config.preset == name else '')
        count = self.config.sub_count()
        self._sub_count.setText(str(count))
        self._sub_minus.setEnabled(count > 0)
        self._sub_plus.setEnabled(count < 8)
        if self._biamp is not None:
            self._biamp.setChecked(bool(self.config.biamp_default))

    # ──────────────────────────── 장치 조회 ────────────────────────────
    def _output_devices(self) -> list:
        if self.devices is None:
            return []
        # refresh() 동안엔 한 번 읽은 목록을 쓴다 — 표가 채널마다 device_label() 을 불러
        # 실제 REW 면 같은 HTTP 조회를 수백 번 했다(반응 속도 09-23).
        cached = getattr(self, '_devices_cache', None)
        if cached is not None:
            return cached
        try:
            devices = list(self.devices.output_devices())
        except Exception:  # noqa: BLE001 - 장치 조회 실패는 화면에서만 알린다
            devices = []
        if self._building:
            self._devices_cache = devices
        return devices

    def device_label(self) -> str:
        """현재 고른 측정 오디오 장치의 이름(접두어 판정용)."""
        data = self.device_combo.currentData()
        for device in self._output_devices():
            if device.id == data:
                return f'{device.id} {device.name}'
        return str(self.config.device or '')

    def channel_name(self, out: int) -> str:
        """출력 번호의 표시 이름 — REW 가 준 풀 네임("11: Phones Out 1"), 없으면 장치 접두어 + 번호."""
        names = self._channel_names_cache()
        if names:
            return output_label(self.devices, out, names)
        return channel_name(self.device_label(), out)

    def _channel_names_cache(self) -> list[str]:
        """REW 채널 이름은 화면 한 번 그릴 때 한 번만 묻는다(표가 줄마다 부른다)."""
        if getattr(self, '_names_cache', None) is None:
            self._names_cache = output_channel_names(self.devices) if self.devices is not None else []
        return self._names_cache

    def device_channels(self) -> int:
        data = self.device_combo.currentData()
        for device in self._output_devices():
            if device.id == data:
                return int(device.channels)
        return 0

    def _refresh_devices(self) -> None:
        devices = self._output_devices()
        self.device_combo.blockSignals(True)
        self.device_combo.clear()
        if devices:
            for device in devices:
                # 이름에 채널 수가 있으면 덧붙이지 않는다('Demo ASIO 16ch · 16ch' — 10-10 3차 시험, 1단계와 같게)
                tag = f'{device.channels}ch'
                text = device.name if tag.lower() in device.name.lower() else f'{device.name} · {tag}'
                self.device_combo.addItem(text, device.id)
            wanted = self.config.device
            index = max(0, next((i for i, d in enumerate(devices)
                                 if d.name == wanted or d.id == wanted), 0))
            self.device_combo.setCurrentIndex(index)
            self.config.device = devices[index].name
            self.device_combo.setToolTip(self.device_combo.currentText())
        else:
            # 장치 목록은 REW 에서 온다 — 연결 전이면 왜 비었는지 말해 준다(09-22 사용자: "아무것도 인식이 안 된다").
            try:
                connected = bool(self.devices.is_running()) if hasattr(self.devices, 'is_running') else False
            except Exception:  # noqa: BLE001
                connected = False
            empty = '장치 목록을 읽을 수 없음' if connected else 'REW 연결 후 표시됨 (1단계)'
            self.device_combo.addItem(self.config.device or empty, '')
        self.device_combo.blockSignals(False)

        channels = self.device_channels()
        over = channels and any(b.out > channels
                                for s in self.config.speakers for b in s.bands)
        if not channels:
            self.device_pill.setText('채널 수 미확인')
            self.device_pill.setProperty('tone', 'na')
        elif over:
            self.device_pill.setText(f'채널 {channels}개 초과')
            self.device_pill.setProperty('tone', 'bad')
        else:
            self.device_pill.setText(f'채널 {channels}개 · REW /audio 로 전송')
            self.device_pill.setProperty('tone', 'ok')
        repolish(self.device_pill)

    def _refresh_dsp(self) -> None:
        dsp = self.project.dsp
        index = self.dsp_profile.findData(dsp.profile)
        self.dsp_profile.setCurrentIndex(index if index >= 0 else 0)
        self.dsp_profile.setToolTip(str(self.dsp_profile.itemData(
            self.dsp_profile.currentIndex(), QtCore.Qt.ItemDataRole.ToolTipRole) or ''))
        tap_index = self.dsp_taps.findData(int(dsp.fir_taps))
        if tap_index < 0:
            self.dsp_taps.addItem(fir_tap_label(int(dsp.fir_taps)), int(dsp.fir_taps))
            tap_index = self.dsp_taps.count() - 1
        self.dsp_taps.setCurrentIndex(tap_index)
        # 단위 접미사(09-24 사용자: "단위도 같이 적어 줘") — 표시 형식만, 저장 값은 그대로 정수.
        # 경로 지연은 시간이 아니라 '지연 자리의 개수'(model.DspProfile.bm_path_delays)라 '곳'(영어 'slots' —
        # 10-10 AI 시험: 'BM path delays 64 ea' 가 어색했다).
        for spin, unit in ((self.dsp_budget, ' 탭'), (self.dsp_iir, ' 밴드'),
                           (self.dsp_allpass, ' 밴드'), (self.dsp_bm_delays, ' 곳')):
            spin.setSuffix(str(t(unit)))
        self.dsp_budget.setValue(int(dsp.fir_budget))
        self.dsp_iir.setValue(int(dsp.iir_bands))
        self.dsp_allpass.setValue(int(getattr(dsp, 'allpass_bands', 16)))
        self.dsp_bm_delays.setValue(int(getattr(dsp, 'bm_path_delays', 16)))
        # 숫자에 이름을 붙인다(09-24 UI 감사: 콤보의 '21.3 ms' 와 이 줄의 '43 ms' 가 이름 없이 나란히
        # 있어 헷갈렸다). 콤보 = 선형위상 지연(탭/2), 이 줄 = 창 길이(탭/fs). 뜻은 풍선 도움말.
        self.dsp_note.setText(t('창 길이 ≈ {0} ms · 분해능 ≈ {1} Hz').format(
            f'{dsp.fir_taps / 48000 * 1000:.0f}', f'{48000 / max(1, dsp.fir_taps):.1f}'))

    # ──────────────────────────── 표 ────────────────────────────
    def _rebuild_table(self) -> None:
        config = self.config
        usage: dict[int, list[int]] = {}
        for index, speaker in enumerate(config.speakers):
            for band in speaker.bands:
                usage.setdefault(band.out, []).append(index)
        self._duplicates = sorted(out for out, who in usage.items() if len(who) > 1)

        groups: dict[str, list[int]] = {}
        for index, speaker in enumerate(config.speakers):
            groups.setdefault(role_group(speaker.role), []).append(index)
        # role_group 은 서브를 'sub' 으로 돌려주므로 표시 이름으로 바꿔 목업 순서대로 정렬한다.
        ordered: list[tuple[str, list[int]]] = []
        for name in GROUP_ORDER:
            key = 'sub' if name == '서브' else name
            if key in groups:
                ordered.append((name, groups[key]))
        shown = {name for name, _ in ordered}
        for key, members in groups.items():
            name = '서브' if key == 'sub' else key
            if name not in shown:
                ordered.append((name, members))

        # 표에 보이는 것이 그대로면 다시 만들지 않는다. 단계를 오가기만 해도 `go()` 가 refresh()
        # 를 불러 9.1.6 바이앰프에서 매번 130~160 ms 가 들었다(반응 속도 측정 09-23).
        channels = len(self._channel_names_cache()) or self.device_channels() or 64
        structure, values = self._table_signature(channels)
        if (structure, values) == getattr(self, '_table_key', None):
            return
        if (structure == (getattr(self, '_table_key', None) or (None,))[0]
                and self._same_out_lists(values, channels)):
            # 모양은 그대로이고 출력 번호·극성만 바뀌었다 — 셀 위젯을 다시 만들지 않고 값만 바꾼다.
            self._table_key = (structure, values)
            self._update_band_cells()
            self._fit_now_or_later()
            return
        self._table_key = (structure, values)

        rows = sum(1 + sum(len(config.speakers[i].bands) for i in members)
                   for _, members in ordered)
        # 셀 위젯은 행을 0 으로 줄여야 확실히 지워진다(남으면 유령으로 겹쳐 보인다).
        self.table.clearSpans()
        self.table.clearContents()
        self.table.setRowCount(0)
        self.table.setRowCount(rows)
        self._table_combos: list = []
        self._band_cells: list = []
        row = 0
        for name, members in ordered:
            self.table.setSpan(row, 0, 1, 7)
            group_label = label(f'  {name} 그룹 · {len(members)}개')
            group_label.setStyleSheet(
                f'background:{COLORS["head_bg"]};color:{COLORS["ink2"]};font-size:12px;')
            self.table.setCellWidget(row, 0, group_label)
            self.table.setRowHeight(row, 26)
            row += 1
            for index in members:
                row = self._add_speaker_rows(row, index, channels)
        self.table.setMinimumHeight(min(520, max(240, rows * 34 + 40)))
        # 셀 위젯은 표에 들어간 뒤에야 표의 글꼴(QSS)을 받는다. 그 전에 잰 폭으로 열이 정해져
        # 드롭다운 글자가 잘렸다 → 붙인 뒤 다시 재서 최소 폭을 못박는다(실사용 QA 09-19, 09-23).
        self._fit_now_or_later()

    def _fit_now_or_later(self) -> None:
        """보이는 중이면 지금 한 번, 숨어 있으면(다른 단계를 보는 중) 보일 때 한 번 잰다.
        전에는 매번 지금 + 다음 이벤트 루프에서 두 번 쟀고, 빠른 연속 편집이면 밀린 호출이 쌓였다."""
        if self.table.isVisible():
            self._fit_pending = False
            self._fit_columns()
        else:
            self._fit_pending = True

    def _table_signature(self, channels: int) -> tuple[tuple, tuple]:
        """(표 모양, 대역별 (출력, 극성)). 모양 = 스피커·대역 구성, 채널 이름, 언어."""
        names = tuple(self._channel_names_cache())
        speakers = tuple((s.role, s.size, tuple(b.band for b in s.bands))
                         for s in self.config.speakers)
        values = tuple((int(b.out), b.pol) for s in self.config.speakers for b in s.bands)
        # 이름이 없으면 채널 이름은 장치 접두어 + 번호다(channel_name) — 장치도 모양에 든다.
        device = '' if names else self.device_label()
        return (speakers, names, int(channels), device, language_manager().language), values

    def _same_out_lists(self, values: tuple, channels: int) -> bool:
        """출력 드롭다운의 목록이 새 값에서도 그대로인가 — 범위 밖 번호는 그 드롭다운에만
        '(범위 밖)' 항목이 붙으므로, 범위 밖 여부나 그 번호가 바뀌면 다시 만들어야 한다."""
        cells = getattr(self, '_band_cells', None)
        if not cells or len(cells) != len(values):
            return False
        for cell, (out, _pol) in zip(cells, values):
            extra = cell[6]
            if (out > channels) != (extra is not None) or (extra is not None and out != extra):
                return False
        return True

    def _update_band_cells(self) -> None:
        """대역 행의 출력 드롭다운·중복 표시·극성 버튼을 지금 값으로 맞춘다(`_add_speaker_rows` 와 같은 모양)."""
        duplicates = getattr(self, '_duplicates', ())
        for cell in self._band_cells:
            index, offset, combo, box, polarity = cell[:5]
            band = self.config.speakers[index].bands[offset]
            found = combo.findData(band.out)
            blocked = combo.blockSignals(True)
            combo.setCurrentIndex(found if found >= 0 else 0)
            combo.blockSignals(blocked)
            marker = cell[5]
            if band.out in duplicates and marker is None:
                marker = pill('중복', 'bad')
                box.addWidget(marker)
            elif band.out not in duplicates and marker is not None:
                box.removeWidget(marker)
                marker.deleteLater()
                marker = None
            cell[5] = marker
            polarity.setText('정상' if band.pol == '+' else '반전')
            polarity.setProperty('state', 'inv' if band.pol == '-' else None)
            repolish(polarity)

    def _output_model(self, channels: int):
        """출력 드롭다운 목록(1…channels) — 한 번 만들어 같은 목록의 모든 드롭다운이 나눠 쓴다.

        9.1.6 바이앰프는 드롭다운 25개 × 채널 수만큼 addItem 을 870번 했다(09-23 측정).
        번역 콤보(`secs.i18n.QComboBox`)의 addItem 과 같은 모양(표시 글자 · 번호 · 원문)으로 채운다.
        """
        from PyQt6 import QtGui
        from secs.i18n import QComboBox as _LocalizedCombo, translate

        labels = tuple(self.channel_name(number) for number in range(1, channels + 1))
        key = (labels, language_manager().language)
        cached = getattr(self, '_out_model', None)
        if cached is not None and cached[0] == key:
            return cached[1]
        model = QtGui.QStandardItemModel(self)
        user, source_role = QtCore.Qt.ItemDataRole.UserRole, _LocalizedCombo._SOURCE_ROLE
        for number, text in enumerate(labels, start=1):
            item = QtGui.QStandardItem(translate(text))
            item.setData(number, user)
            item.setData(text, source_role)
            model.appendRow(item)
        if cached is not None:
            cached[1].deleteLater()          # 옛 드롭다운은 표를 비울 때 이미 지우기 예약됨
        self._out_model = (key, model)
        return model

    def _fit_columns(self) -> None:
        """각 열 = 머리글과 그 열 셀 위젯(그룹 머리 행 제외) 중 가장 넓은 것 + 여백."""
        table = self.table
        # 표에 넣은 드롭다운만 잰다 — findChildren 은 지우기 예약된 옛 드롭다운까지 훑었다.
        for combo in getattr(self, '_table_combos', None) or table.findChildren(_PlainQt.QComboBox):
            combo.ensurePolished()
            # +6: QSS 안쪽 여백을 넣은 sizeHint 는 글자와 화살표 사이가 2~4 px 뿐이라 붙어 보였다(09-24 QA).
            combo.setMinimumWidth(combo.sizeHint().width() + 6)
        header = table.horizontalHeader()
        fitted: dict[int, int] = {}
        for column in range(table.columnCount() - 1):          # 마지막(×) 열은 늘어나는 열
            width = header.sectionSizeFromContents(column).width()
            for row in range(table.rowCount()):
                if table.columnSpan(row, column) > 1:
                    continue
                widget = table.cellWidget(row, column)
                if widget is not None:
                    widget.ensurePolished()
                    width = max(width, widget.sizeHint().width(), widget.minimumSizeHint().width())
                    if widget.objectName() == 'pol':
                        # 극성 버튼: sizeHint 가 QSS 여백(좌우 10 px)을 덜 쳐서 영어 'Non-inverted' 끝이
                        # 잘렸다(09-24 QA). 누르면 글자가 바뀌니 두 글자 중 넓은 쪽 + 여백으로 못박는다.
                        metrics = widget.fontMetrics()
                        need = max(metrics.horizontalAdvance(str(t(text))) for text in ('정상', '반전')) + 26
                        widget.setMinimumWidth(need)
                        width = max(width, need)
            # 열 여백 8 px(전에는 12) — 1024 창에서 표 폭이 모자라 극성 열이 밀려났다(09-24 QA).
            # 그래도 넘치면 표 안에서만 가로로 스크롤된다(페이지는 밀리지 않는다).
            header.resizeSection(column, width + 8)
            fitted[column] = width + 8
        self._column_need = fitted
        self._spread_columns()

    def _spread_columns(self) -> None:
        """내용 폭(`_fit_columns`)을 채우고 남는 표 폭을 채널 · 크기 · 측정 출력 열에 고르게 나눈다(열마다 `_SPREAD_MAX`
        까지 — 아주 넓은 창에서 드롭다운이 지나치게 길어지지 않게). 그래도 남는 폭은 표 오른쪽 빈칸."""
        need = getattr(self, '_column_need', None)
        if not need:
            return
        table, header = self.table, self.table.horizontalHeader()
        last = table.columnCount() - 1
        shown = [c for c in range(last) if not table.isColumnHidden(c)]
        grow = [c for c in _SPREAD_COLUMNS if c in shown]
        last_need = 48                          # × 단추(지름 32) + 여백. 늘어나는 열로 두면 기본 폭 100 밑으로 안 줄어 넘쳤다
        spare = table.viewport().width() - sum(need.get(c, 0) for c in shown) - last_need
        each = min(_SPREAD_MAX, max(0, spare) // len(grow)) if grow else 0
        for column in shown:
            header.resizeSection(column, need.get(column, 0) + (each if column in grow else 0))
        header.resizeSection(last, last_need)

    def _add_speaker_rows(self, row: int, index: int, channels: int | None = None) -> int:
        speaker = self.config.speakers[index]
        combos = getattr(self, '_table_combos', None)
        if combos is None:
            combos = self._table_combos = []
        count = max(1, len(speaker.bands))
        kind = role_kind(speaker.role)

        # 채널 = 역할 드롭다운 하나. 전에는 배지·이름 칸과 역할 칸이 같은 것을 두 번 보였다(09-23 사용자).
        role_combo = QtWidgets.QComboBox()
        role_combo.setSizeAdjustPolicy(QtWidgets.QComboBox.SizeAdjustPolicy.AdjustToContents)
        for role in ROLES:
            role_combo.addItem(f'{role_code(role)} · {role_label(role)}', role)
        role_combo.addItem('사용자 정의', 'custom')
        found = role_combo.findData(speaker.role)
        role_combo.setCurrentIndex(found if found >= 0 else role_combo.count() - 1)
        role_combo.currentIndexChanged.connect(
            lambda _=0, i=index, combo=role_combo: self._on_role(i, combo))
        attach(role_combo, 's2.role')
        self.table.setCellWidget(row, 0, _v_centered(role_combo))
        combos.append(role_combo)

        # 앰프 구성 · 대역 칸(열 2 · 3)은 바이앰프 칸이 있을 때만 만든다(닫혀 있으면 열도 숨어 있다 — 10-11).
        if kind == 'sub':
            self.table.setCellWidget(row, 1, pill('서브', 'na'))
            if self._biamp_ui:
                self.table.setCellWidget(row, 2, pill('1채널', 'na'))
        else:
            size_combo = QtWidgets.QComboBox()
            size_combo.setSizeAdjustPolicy(QtWidgets.QComboBox.SizeAdjustPolicy.AdjustToContents)
            size_combo.addItem('소형 · XO 적용', 'small')
            size_combo.addItem('대형 · 풀레인지', 'large')
            size_combo.setCurrentIndex(0 if speaker.size != 'large' else 1)
            size_combo.currentIndexChanged.connect(
                lambda _=0, i=index, combo=size_combo: self._on_size(i, combo))
            attach(size_combo, 's2.size')
            self.table.setCellWidget(row, 1, _v_centered(size_combo))
            combos.append(size_combo)

            if self._biamp_ui:
                drive_combo = QtWidgets.QComboBox()
                drive_combo.setSizeAdjustPolicy(QtWidgets.QComboBox.SizeAdjustPolicy.AdjustToContents)
                drive_combo.addItem('싱글앰프', 'FULL')
                drive_combo.addItem('바이앰프', 'BI')
                drive_combo.addItem('트라이앰프', 'TRI')
                drive_combo.setCurrentIndex({1: 0, 2: 1, 3: 2}.get(count, 0))
                drive_combo.currentIndexChanged.connect(
                    lambda _=0, i=index, combo=drive_combo: self._on_drive(i, combo))
                attach(drive_combo, 's2.drive')
                self.table.setCellWidget(row, 2, _v_centered(drive_combo))
                combos.append(drive_combo)

        for column in (0, 1, 2):
            if count > 1:
                self.table.setSpan(row, column, count, 1)

        remove = QtWidgets.QPushButton('×')
        remove.setObjectName('iconbtn')
        remove.setToolTip('스피커 제거')
        remove.clicked.connect(lambda _=False, i=index: self._remove_speaker(i))
        attach(remove, 's2.remove')
        self.table.setCellWidget(row, 6, remove)
        if count > 1:
            self.table.setSpan(row, 6, count, 1)

        if channels is None:
            channels = len(self._channel_names_cache()) or self.device_channels() or 64
        for offset, band in enumerate(speaker.bands):
            current = row + offset
            if self._biamp_ui:
                self.table.setCellWidget(current, 3, attach(band_chip(band.band), 's2.band'))

            out_holder = QtWidgets.QWidget()
            out_box = QtWidgets.QHBoxLayout(out_holder)
            out_box.setContentsMargins(0, 0, 0, 0)
            out_box.setSpacing(4)
            out_combo = QtWidgets.QComboBox()
            # '17: Optical Out 3' 같은 풀 네임이 들어간다. 상한은 열이 남는 폭을 받아 넓어져도(`_spread_columns`) 칸을
            # 채우게 210 + `_SPREAD_MAX` — 210 그대로면 넓힌 열 가운데에 드롭다운이 떠 보였다(10-11).
            out_combo.setMaximumWidth(210 + _SPREAD_MAX)
            out_combo.setMinimumContentsLength(12)
            if band.out > channels:
                # 범위 밖 항목이 하나 더 붙는 드롭다운만 따로 채운다(공용 목록에 넣으면 모두에 보인다).
                for number in range(1, channels + 1):
                    out_combo.addItem(self.channel_name(number), number)
                out_combo.addItem(f'{self.channel_name(band.out)} (범위 밖)', band.out)
            else:
                out_combo.setModel(self._output_model(channels))
            combos.append(out_combo)
            out_index = out_combo.findData(band.out)
            out_combo.setCurrentIndex(out_index if out_index >= 0 else 0)
            out_combo.currentIndexChanged.connect(
                lambda _=0, i=index, b=offset, combo=out_combo: self._on_out(i, b, combo))
            attach(out_combo, 's2.out')
            out_box.addWidget(out_combo)
            marker = None
            if band.out in getattr(self, '_duplicates', ()):
                marker = pill('중복', 'bad')
                out_box.addWidget(marker)
            self.table.setCellWidget(current, 4, out_holder)

            polarity = QtWidgets.QPushButton('정상' if band.pol == '+' else '반전')
            polarity.setObjectName('pol')
            if band.pol == '-':
                polarity.setProperty('state', 'inv')
            polarity.setToolTip('극성 반전')
            polarity.clicked.connect(lambda _=False, i=index, b=offset: self._toggle_polarity(i, b))
            attach(polarity, 's2.polarity')
            self.table.setCellWidget(current, 5, polarity)
            self.table.setRowHeight(current, 34)
            cells = getattr(self, '_band_cells', None)
            if cells is not None:
                cells.append([index, offset, out_combo, out_box, polarity, marker,
                              band.out if band.out > channels else None])
        return row + count

    # ──────────────────────────── 요약 · 검증 메시지 · 상세 ────────────────────────────
    def _refresh_summary(self) -> None:
        clear_layout(self._summary)
        config = self.config
        total = sum(len(s.bands) for s in config.speakers)
        highest = max((b.out for s in config.speakers for b in s.bands), default=0)
        counts = QtWidgets.QHBoxLayout()
        counts.setSpacing(14)
        counts.addWidget(label(f'{len(config.speakers)} 스피커'))
        counts.addWidget(
            label(f'{total} 측정 출력 · 최대 {self.channel_name(highest)}'))
        counts.addStretch(1)
        self._summary.addLayout(counts)
        pills = QtWidgets.QHBoxLayout()
        pills.setSpacing(8)
        duplicates = getattr(self, '_duplicates', [])
        pills.addWidget(
            pill(f'출력 중복 {len(duplicates)}건' if duplicates else '출력 중복 없음',
                 'bad' if duplicates else 'ok'))
        has_sub = any(role_kind(s.role) == 'sub' for s in config.speakers)
        pills.addWidget(pill('서브 별도 출력' if has_sub else '서브 없음',
                             'ok' if has_sub else 'na'))
        pills.addStretch(1)
        self._summary.addLayout(pills)

        # 정상일 때는 문구를 깔지 않는다 — 오류가 있을 때만 보인다(09-22 사용자 지시).
        messages = config.validate()
        if messages:
            self._messages.setObjectName('helpBad')
            self._messages.setText('구성 오류 ' + ' · '.join(messages[:4])
                                   + ('' if len(messages) <= 4 else f' 외 {len(messages) - 4}건'))
            self._messages.setVisible(True)
        else:
            self._messages.setText('')
            self._messages.setVisible(False)
        repolish(self._messages)

    def _refresh_detail(self) -> None:
        clear_layout(self._detail)
        if not (0 <= self._selected < len(self.config.speakers)):
            self._detail.addWidget(label('배치도나 표에서 스피커를 고르면 여기에 연결 요약이 나옵니다.',
                                         'note', wrap=True))
            return
        speaker = self.config.speakers[self._selected]
        top = QtWidgets.QHBoxLayout()
        top.setSpacing(8)
        top.addWidget(plain_label(role_label(speaker.role), 'h3'))
        top.addWidget(chip(speaker.role))
        top.addWidget(pill(KIND_PILLS.get(role_kind(speaker.role), '메인')))
        top.addStretch(1)
        holder = QtWidgets.QWidget()
        holder.setLayout(top)
        self._detail.addWidget(holder)
        for band in speaker.bands:
            text = (f'{band.band} → {self.channel_name(band.out)} → '
                    f'DSP 입력 {band.out} → 앰프'
                    + ('  [반전]' if band.pol == '-' else ''))
            line = label(text)
            line.setStyleSheet(f'font-family:{FONT_MONO};font-size:12.5px;')
            self._detail.addWidget(line)
        # 자세한 뜻은 도움말(F1) 배치도 항목 — 여기는 40자 안의 짧은 상태 문구만(09-22).
        if len(speaker.bands) > 1:
            note = '대역마다 필터를 따로 계산합니다.'
        elif role_kind(speaker.role) == 'sub':
            note = '서브는 LFE와 소형 스피커의 저역을 합산해 받습니다.'
        else:
            note = '풀레인지 한 출력으로 구동합니다.'
        self._detail.addWidget(label(note, 'note', wrap=True))

    # ──────────────────────────── 편집 ────────────────────────────
    def _touch(self) -> None:
        # 표 칸을 고쳐도 스크롤 자리는 그대로(10-09 사용자: 채널이 많아 내려가서 출력을 고르면 맨 위로 튀어 다시 내려야
        # 했다) — 다시 그리기 전 자리를 기억해 두고, 다시 그린 뒤와 늦게 오는 배치 갱신 뒤에 되돌린다.
        # 표(QTableWidget)도 제 스크롤이 있다 — 행을 다시 만들면 맨 위로 튀었다(10-10 사용자: 출력 채널을 바꾸거나 중복이
        # 생겼다 없어지면 스크롤이 움직였다). 페이지 스크롤과 표 스크롤을 둘 다 지킨다.
        bars = [b for b in (self._scroll_bar(), self.table.verticalScrollBar(), self.table.horizontalScrollBar())
                if b is not None]
        kept = [(b, b.value()) for b in bars]
        self.refresh()
        self.changed.emit()
        if kept:
            def restore(kept=kept):
                for bar, keep in kept:
                    try:
                        bar.setValue(min(keep, bar.maximum()))
                    except RuntimeError:          # 창이 먼저 닫혔다
                        pass
            restore()
            QtCore.QTimer.singleShot(0, restore)
            QtCore.QTimer.singleShot(60, restore)

    def _scroll_bar(self):
        parent = self.parentWidget()
        while parent is not None:
            if isinstance(parent, QtWidgets.QAbstractScrollArea):
                return parent.verticalScrollBar()
            parent = parent.parentWidget()
        return None

    def _biamp_default(self) -> bool:
        """새 스피커를 바이앰프로 만들지 — 칸이 닫혀 있으면(10-11) 저장된 값이 켜져 있어도 끔(되돌릴 칸이 없다)."""
        return bool(self.config.biamp_default) and self._biamp_ui

    def _apply_preset(self, key: str) -> None:
        if key == 'custom':
            self.project.config = SpeakerConfig(speakers=[], device=self.config.device,
                                                preset='custom',
                                                biamp_default=self._biamp_default())
        else:
            self.project.config = SpeakerConfig.from_preset(
                key, device=self.config.device, biamp=self._biamp_default())
        self.project.measurements.clear()
        self._selected = -1
        self._touch()

    def _set_sub_count(self, delta: int) -> None:
        self.config.set_sub_count(self.config.sub_count() + delta)
        self.project.measurements.clear()
        self._touch()

    def _on_biamp(self, checked: bool) -> None:
        if self._building:
            return
        self.config.biamp_default = bool(checked)
        for speaker in self.config.speakers:
            if role_kind(speaker.role) == 'main':
                self._set_band_set(speaker, 'BI' if checked else 'FULL')
        self.config.renumber()
        self._touch()

    @staticmethod
    def _set_band_set(speaker: Speaker, key: str) -> None:
        """목업 setBandSet: 구동 방식에 맞춰 대역 목록을 다시 만든다."""
        if role_kind(speaker.role) == 'sub':
            return
        first = speaker.bands[0].out if speaker.bands else 1
        names = {'FULL': ['FULL'], 'BI': ['LF', 'HF'], 'TRI': ['LF', 'MF', 'HF']}[key]
        speaker.bands = [Band(band=name, out=first + i, pol='+') for i, name in enumerate(names)]

    def _on_role(self, index: int, combo: QtWidgets.QComboBox) -> None:
        if self._building:
            return
        if not self._live(combo, index, None):
            return
        speaker = self.config.speakers[index]
        speaker.role = str(combo.currentData())
        if role_kind(speaker.role) == 'sub':
            speaker.bands = [Band('SUB', speaker.bands[0].out if speaker.bands else 1, '+')]
            speaker.size = 'sub'
        elif speaker.bands and speaker.bands[0].band == 'SUB':
            speaker.bands = [Band('FULL', speaker.bands[0].out, '+')]
            speaker.size = 'small'
        self.config.preset = 'custom'
        self._touch()

    def _on_size(self, index: int, combo: QtWidgets.QComboBox) -> None:
        if self._building:
            return
        if not self._live(combo, index, None):
            return
        self.config.speakers[index].size = str(combo.currentData())
        self._touch()

    def _on_drive(self, index: int, combo: QtWidgets.QComboBox) -> None:
        if self._building:
            return
        if not self._live(combo, index, None):
            return
        self._set_band_set(self.config.speakers[index], str(combo.currentData()))
        self.config.preset = 'custom'
        self._touch()

    def _on_out(self, index: int, band_index: int, combo: QtWidgets.QComboBox) -> None:
        if self._building:
            return
        if not self._live(combo, index, band_index):
            return
        self.config.speakers[index].bands[band_index].out = int(combo.currentData())
        self._touch()

    def _live(self, combo, index: int, band_index: int | None = None) -> bool:
        """표를 다시 만든 뒤에도 잠깐 살아 있는 옛 드롭다운의 신호는 버린다.

        옛 위젯이 가리키는 (스피커, 대역) 번호는 새 구성에서 없거나 다른 스피커일 수 있다 —
        그대로 쓰면 IndexError 가 나거나 엉뚱한 스피커에 값이 들어갔다(09-23 스트레스 점검).
        """
        try:
            if not self.table.isAncestorOf(combo):
                return False
        except RuntimeError:          # 이미 지워진 C++ 위젯
            return False
        if not (0 <= index < len(self.config.speakers)):
            return False
        return band_index is None or 0 <= band_index < len(self.config.speakers[index].bands)

    def _toggle_polarity(self, index: int, band_index: int) -> None:
        band = self.config.speakers[index].bands[band_index]
        band.pol = '-' if band.pol == '+' else '+'
        self._touch()

    def _add_speaker(self) -> None:
        used = set(self.config.roles())
        role = next((r for r in ROLES if r not in used), 'custom')
        bands = default_bands(role, self.config.next_free_out(), self._biamp_default())
        if len(bands) > 1:
            bands[1].out = bands[0].out + 1
        self.config.speakers.append(
            Speaker(role=role, size='sub' if role_kind(role) == 'sub' else 'small', bands=bands))
        self.config.preset = 'custom'
        self._selected = len(self.config.speakers) - 1
        self._touch()

    def _remove_speaker(self, index: int) -> None:
        if 0 <= index < len(self.config.speakers):
            self.config.speakers.pop(index)
            self.config.preset = 'custom'
            self._selected = -1
            self._touch()

    def _renumber(self) -> None:
        self.config.renumber()
        self._touch()

    def _select_speaker(self, index: int) -> None:
        self._selected = index
        self.refresh()

    def _on_device(self, _index: int) -> None:
        if self._building:
            return
        self.config.device = self.device_combo.currentText().split(' · ')[0]
        self._touch()

    def _on_profile(self, _index: int) -> None:
        if self._building:
            return
        key = str(self.dsp_profile.currentData())
        preset = DspProfile.presets().get(key)
        if preset is not None and key != 'custom':
            self.project.dsp = preset
        else:
            self.project.dsp.profile = key
        self._touch()

    def _on_taps(self, _index: int) -> None:
        if self._building:
            return
        self.project.dsp.fir_taps = int(self.dsp_taps.currentData())
        self.project.dsp.profile = 'custom'
        self._touch()

    def _on_budget(self, value: int) -> None:
        if self._building:
            return
        self.project.dsp.fir_budget = int(value)
        self.project.dsp.profile = 'custom'
        self._touch()

    def _on_iir(self, value: int) -> None:
        if self._building:
            return
        self.project.dsp.iir_bands = int(value)
        self.project.dsp.profile = 'custom'
        self._touch()

    def _on_allpass(self, value: int) -> None:
        if self._building:
            return
        self.project.dsp.allpass_bands = int(value)
        self.project.dsp.profile = 'custom'
        self._touch()

    def _on_bm_delays(self, value: int) -> None:
        if self._building:
            return
        self.project.dsp.bm_path_delays = int(value)
        self.project.dsp.profile = 'custom'
        self._touch()

    # ──────────────────────────── 패치 시트 CSV ────────────────────────────
    def _export_csv(self) -> None:
        """패치 시트 CSV 저장(목업의 toast 대신 실제 파일 저장 대화상자)."""
        path, _ = QtWidgets.QFileDialog.getSaveFileName(
            self, str(t('패치 시트 CSV 저장')), 'secs_patch_sheet.csv', 'CSV (*.csv)')
        if not path:
            return
        lines = ['역할,이름,크기,대역,측정 출력,극성']
        for speaker in self.config.speakers:
            for band in speaker.bands:
                lines.append(f'{speaker.role},{role_label(speaker.role)},{speaker.size},'
                             f'{band.band},{band.out},{band.pol}')
        with open(path, 'w', encoding='utf-8-sig') as handle:
            handle.write('\n'.join(lines) + '\n')

    # ──────────────────────────── 진행 조건 ────────────────────────────
    def can_advance(self) -> tuple[bool, str]:
        if not self.config.speakers:
            return False, '스피커를 한 대 이상 추가하세요.'
        messages = self.config.validate()
        if messages:
            return False, messages[0]
        return True, ''


__all__ = ['Step2Config', 'GROUP_ORDER', 'FIR_TAP_CHOICES',
           'CHANNEL_PREFIXES', 'channel_prefix', 'channel_name']
