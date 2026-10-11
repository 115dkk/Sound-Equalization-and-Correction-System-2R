# -*- coding: utf-8 -*-
"""5단계 측정 — 목업 `step5()`.

위치 목록(대기/진행/완료) · 현재 위치 안내(4단계 `position_seconds()` 와 같은 값으로
이 위치의 예상 소요도 보여준다, 2026-09-23) · 시작/취소 · 진행바·타이머 ·
측정 직후 Before 곡선(1/12 옥타브 스무딩) · mdat 불러오기 · 데모 데이터.
측정은 `QThread` 워커에서 `MeasurementSession.run_position` 을 돌리고, 결과는
`.secs` 와 `<프로젝트>.irs/<role>_P<pos>.wav` 로 자동 저장된다.

`WizardWindow` 가 `measurer` 와 `.secs` 경로를 주며 만들고 `refresh()`·
`can_advance()` 를 부르며, 창을 닫을 때 `cancel()`/`wait()` 로 워커를 거둔다. 바뀌면
`changed` 를 낸다. 입력은 `project.config`(측정할 role 과 출력 채널) ·
`project.extra['devices']`(1단계) · `project.extra['array']`(4단계)이고, 출력은
`project.measurements`(위치당 한 항목)와 `.irs/` 의 wav 다. 6·7단계가 이 둘을 읽는다.
"""
from __future__ import annotations

import re
import threading
from datetime import datetime
from pathlib import Path

import numpy as np
import pyqtgraph as pg
from PyQt6 import QtCore, QtGui
from PyQt6 import QtWidgets as _PlainQt
from secs.wizard import qt as QtWidgets
from scipy.ndimage import gaussian_filter1d

from secs.model import role_code
from secs.rewapi.interfaces import LOW_SNR_DB, MeasurementPlan
from secs.rewapi.session import MeasurementSession
from secs.wizard.plot import octave_smoothing_sigma as _octave_smoothing_sigma
from secs.wizard.layout_map import ARRANGEMENTS, ArrayLayoutView, arr_points
from secs.wizard.loading import BusyDrop, run_off_thread
from secs.wizard.axis_range import AxisControl
from secs.wizard.plot import axis_label, freq_ticks
from secs.wizard.legend import PositionChips, position_pen, position_z
from secs.wizard.step1_devices import DEFAULT_SAMPLE_RATE, fill_sweep_combo
from secs.wizard.step4_layout import START_DELAYS, format_estimate, position_seconds, start_delay_seconds
from secs.wizard.strings import t
from secs.wizard.tooltips import attach, attach_all, attach_cells, attach_header
from secs.wizard.style import (COLORS, SUB_LINE_WIDTH, ElidedLabel, add_heading, card, channel_colors, clear_layout,
                               is_sub_role, label, set_variant, two_columns)


#: 목업 F — 15 Hz ~ 22 kHz 로그 240점.
FREQS = np.logspace(np.log10(5.0), np.log10(22000.0), 280)   # 10-09 사용자: X축 5 Hz 까지

#: 표시용 스무딩(계약: 1/12 옥타브, `plot.octave_smoothing_sigma`).
SMOOTHING_OCTAVE_FRACTION = 12.0

#: 저장 위치를 묻지 않고 쓰는 기본 폴더(`%USERPROFILE%\\Documents\\SECS`).
DEFAULT_PROJECT_DIR = Path.home() / 'Documents' / 'SECS'


#: 파일 이름 앞머리 — 실측과 Fake 샘플을 이름으로 가른다.
REAL_PREFIX = '측정'
SAMPLE_PREFIX = '샘플'

#: REW 가 받는 스윕 반복 선택지. 음향 타이밍 레퍼런스에서는 1만 허용한다.
REPETITIONS = (1, 2, 4, 8)


# ──────────────────────────── 측정 제목 → 역할 배정 ────────────────────────────
#: 위치 목록 줄의 상태 글자('완료'·'대기' 번역)를 담는 item data 역할.
_STATE_ROLE = int(QtCore.Qt.ItemDataRole.UserRole) + 1


class _PositionDelegate(_PlainQt.QStyledItemDelegate):
    """위치 목록 한 줄 = 왼쪽 '번호 이름'(넘치면 … 로 줄임) + 오른쪽 끝에 붙인 상태.

    전에는 '번호 이름    상태' 를 한 문자열로 넣어 1024 영어에서 'Done' 이 'Don' 으로 잘리고
    가로 스크롤바가 생겼다(빌드 전 QA 2026-09-24). 상태는 항상 오른쪽 끝에 온전히 보인다.
    """

    GAP = 10

    def paint(self, painter, option, index) -> None:  # noqa: D401 - Qt 이름
        opt = _PlainQt.QStyleOptionViewItem(option)
        self.initStyleOption(opt, index)
        main_text = opt.text
        state = str(index.data(_STATE_ROLE) or '')
        opt.text = ''
        widget = opt.widget
        style = widget.style() if widget is not None else _PlainQt.QApplication.style()
        style.drawControl(_PlainQt.QStyle.ControlElement.CE_ItemViewItem, opt, painter, widget)
        rect = style.subElementRect(_PlainQt.QStyle.SubElement.SE_ItemViewItemText, opt, widget)
        brush = index.data(QtCore.Qt.ItemDataRole.ForegroundRole)
        color = brush.color() if isinstance(brush, QtGui.QBrush) else opt.palette.color(
            QtGui.QPalette.ColorRole.Text)
        painter.save()
        painter.setFont(opt.font)
        painter.setPen(color)
        metrics = QtGui.QFontMetrics(opt.font)
        align = QtCore.Qt.AlignmentFlag.AlignVCenter
        state_w = metrics.horizontalAdvance(state) if state else 0
        if state:
            painter.drawText(rect, align | QtCore.Qt.AlignmentFlag.AlignRight, state)
        room = max(0, rect.width() - (state_w + self.GAP if state else 0))
        painter.drawText(QtCore.QRect(rect.left(), rect.top(), room, rect.height()),
                         align | QtCore.Qt.AlignmentFlag.AlignLeft,
                         metrics.elidedText(main_text, QtCore.Qt.TextElideMode.ElideRight, room))
        painter.restore()


def _tokens(text: str) -> list[str]:
    return re.findall(r'[A-Z0-9]+', text.upper())


def _starts_with_token(text: str, token: str) -> bool:
    """`text` 가 `token` 으로 시작하되 그 뒤가 이어지는 낱말이 아니어야 한다.

    `'Ls Fin'` 이 role `'L'` 에 걸리지 않게 하는 경계 검사다.
    """
    token = token.upper()
    if not text.startswith(token):
        return False
    return not text[len(token):][:1].isalnum()


def match_role(title: str, roles, used=None) -> str | None:
    """REW 측정 제목 → 측정 계획의 role. 못 고르면 None.

    바이앰프 구성의 role 은 `'L:LF'` 처럼 대역이 붙어 있어서 제목(`'L Fin'`)과
    그대로 비교하면 언제나 어긋났다. 기본 role(`':'` 앞)로 먼저 자르고, 같은
    기본 role 이 여럿이면 제목에 적힌 대역(LF·MF·HF·SUB)으로 고른다. 대역이
    없으면 아직 쓰지 않은 것을 순서대로 준다. 긴 role 을 먼저 보므로 `'Ls'` 가
    `'L'` 에 먹히지 않는다.
    """
    text = str(title or '').strip().upper()
    if not text:
        return None
    used = set(used or ())

    bases: dict[str, list[str]] = {}
    for role in roles:
        bases.setdefault(role.split(':')[0].upper(), []).append(role)
    matched = [base for base in bases if _starts_with_token(text, base)]
    if not matched:
        return None
    base = max(matched, key=len)          # 'LS' 가 'L' 을 이긴다
    candidates = bases[base]
    if len(candidates) == 1:
        return candidates[0]

    rest = set(_tokens(text[len(base):]))
    for role in candidates:               # 제목에 대역이 적혀 있으면 그 대역
        band = role.split(':')[1].upper() if ':' in role else ''
        if band and band in rest:
            return role
    for role in candidates:               # 아니면 아직 안 쓴 첫 후보
        if role not in used:
            return role
    return candidates[0]


_POS_TAG = re.compile(r'^(.*?)[\s_\-]*P(\d+)$', re.IGNORECASE)
_TRAILING_NUMBER = re.compile(r'^(.*\S)\s+(\d+)$')


def split_position(title: str) -> tuple[str, int]:
    """mdat 측정 제목 → (채널 부분, 위치 번호). 10-09 사용자: 위치별 mdat 를 불러오게.

    `L_P2`·`L P2`·`L-P2` → ('L', 2) — SECS 가 저장하는 mdat 이름. `L 2` → ('L', 2). 번호가 없으면 위치 1.
    띄어 쓴 번호는 서브도 위치다(`SW 2` → ('SW', 2) = SW1 의 위치 2) — 붙여 쓴 번호(`SW2`)만 서브 번호.
    10-11 사용자 · 커뮤니티: 서브 1개로 `SW 2` … `SW 9` 를 잰 9점 mdat 이 SW2 로 읽혀 [건너뜀] 이 되고 위치 2~9 가
    계속 '대기' 였다(전에는 `LFE 2` · `SW 2` 를 SW2 의 위치 1 로 읽었다).
    """
    from secs.model import role_from_title

    text = str(title or '').strip()
    match = _POS_TAG.match(text)
    if match and match.group(1).strip():
        return match.group(1).strip(), max(1, int(match.group(2)))
    match = _TRAILING_NUMBER.match(text)
    if match:
        base = match.group(1).strip()
        if role_from_title(base) is not None:
            return base, max(1, int(match.group(2)))
    return text, 1


def looks_like_sub(ir, sample_rate: int) -> bool:
    """IR 이 서브(저역만 나는 측정)처럼 보이는가 — 20~100 Hz 대비 500 Hz~5 kHz 가 30 dB↓.

    실측 v2 의 서브 합 `LF` 는 −55 dB, 전대역 `L 1` 은 −2 dB 였다. 제목이 role 과
    안 맞을 때(`LF`) 메인 채널에 서브를 넣는 사고를 막는 데 쓴다.
    """
    samples = np.asarray(ir, dtype=float).ravel()
    if samples.size < 64 or not np.any(samples):
        return False
    size = 1 << int(np.ceil(np.log2(max(samples.size, 8192))))
    power = np.abs(np.fft.rfft(samples, size)) ** 2
    freqs = np.fft.rfftfreq(size, 1.0 / float(sample_rate))
    low = power[(freqs >= 20.0) & (freqs < 100.0)]
    high = power[(freqs >= 500.0) & (freqs < 5000.0)]
    if low.size == 0 or high.size == 0:
        return False
    return bool(10.0 * np.log10((np.mean(high) + 1e-30) / (np.mean(low) + 1e-30)) < -30.0)


def auto_assign(titles, roles, sub_flags=None) -> list[str | None]:
    """측정 제목 → role 자동 배정. 못 정하거나 겹치면 None(건너뜀).

    1) 제목이 role 과 맞으면 그 role(`match_role`). 이미 채운 role 이면 건너뛴다 —
       보정 후 참고 측정(`L Fin`)이 원측정(`L 1`)을 덮어쓰면 안 된다.
    2) 제목으로 못 고르면 아직 빈 role 을 순서대로 주되, 서브처럼 보이는 측정은
       서브 role 에만, 아닌 측정은 메인 role 에만 넣는다.
    """
    titles = list(titles)
    roles = list(roles)
    # 제목이 채널 이름이면 그 제안을 그대로 쓴다(돌비 약어·별칭 포함, 같은 역할은 처음 측정만).
    # 순서대로 채우는 아래 규칙은 제목이 전혀 안 맞을 때만 — 09-23 돌비룸 mdat 에서 `DBA L`→L,
    # `LRS`→Tfr 처럼 12건을 잘못 넣었다. 비워 둔 측정은 사람이 배정 창에서 고른다.
    from secs.mdat_loader import suggest_assignment
    flags = list(sub_flags) if sub_flags is not None else [None] * len(titles)
    suggested = suggest_assignment(titles, roles)
    if any(suggested):
        # 이름이 안 맞는 서브 측정(`LF` 등)은 빈 서브 role 과 개수가 딱 맞을 때만 채운다 —
        # 후보가 더 많으면(돌비룸: DBA L·DBA R·INFRA·L LF …) 어느 것인지 몰라 사람에게 맡긴다.
        free_subs = [r for r in roles if r not in suggested and r.split(':')[0].upper().startswith('SW')]
        loose = [i for i, (role, is_sub) in enumerate(zip(suggested, flags)) if role is None and is_sub]
        if loose and len(loose) <= len(free_subs):
            for index, role in zip(loose, free_subs):
                suggested[index] = role
        return suggested
    used: set[str] = set()
    out: list[str | None] = []
    for title, is_sub in zip(titles, flags):
        role = match_role(str(title or ''), roles, used)
        if role is None:
            def fits(item: str) -> bool:
                if is_sub is None:
                    return True
                return item.split(':')[0].upper().startswith('SW') == bool(is_sub)
            role = next((item for item in roles if item not in used and fits(item)), None)
        if role is None or role in used:
            out.append(None)
            continue
        used.add(role)
        out.append(role)
    return out


# ──────────────────────────── mdat 역할 배정 창 ────────────────────────────
class MdatAssignDialog(QtWidgets.QDialog):
    """mdat 측정 → (위치, role) 배정 확인 창. 자동 배정을 보여 주고 사람이 고친다.

    10-11 사용자: "mdat 불러온 측정치를 임의로 위치로 채널을 지정할 수 있게" — `position_count` 를 주면 줄마다
    위치 드롭다운이 붙는다(이름의 `P2` · `_P3` 에서 읽은 값이 기본). 채널 이름은 2단계 표와 같은
    돌비 약어 · 이름(`Ltr · …`)으로 보인다 — 내부 ID(`Tbl`)로 보여서 다른 채널로 들어간 것처럼 보였다.
    """

    SKIP = '건너뜀'

    def __init__(self, titles, roles, initial, details=None, parent=None,
                 positions=None, position_count=None) -> None:
        super().__init__(parent)
        self.setWindowTitle('측정 역할 배정')
        titles = list(titles)
        self.roles = list(roles)
        self.combos: list[QtWidgets.QComboBox] = []
        self.pos_combos: list[QtWidgets.QComboBox] = []
        # 측정마다의 청취 위치. 같은 역할은 위치가 다르면 겹쳐도 된다 — 10-11 사용자: 다점 mdat
        # (L P1 … L P9)가 전부 '두 번 배정' 으로 막혔다.
        self._initial_positions = [int(p) for p in positions] if positions is not None else [1] * len(titles)
        count = int(position_count or 0)
        show_positions = count > 1 or any(p != 1 for p in self._initial_positions)
        self.setMinimumWidth(680 if show_positions else 560)
        # 창 배경을 본문과 같은 밝은 면으로 — 어두운 바탕에서는 안내·경고가 안 읽혔다.
        self.setStyleSheet(f'QDialog {{ background: {COLORS["panel"]}; }}')

        outer = QtWidgets.QVBoxLayout(self)
        outer.setSpacing(10)
        # 09-22: 자세한 안내는 도움말(F1) 5단계 "건너뛰기: mdat 불러오기" 항목에 있다 — 여기는 짧게.
        outer.addWidget(label('측정마다 넣을 위치와 스피커를 고르세요.' if show_positions
                              else '측정마다 배정할 스피커를 고르세요.', 'note', wrap=True))

        scroll = QtWidgets.QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        holder = QtWidgets.QWidget()
        grid = QtWidgets.QGridLayout(holder)
        grid.setHorizontalSpacing(12)
        grid.setVerticalSpacing(6)
        details = list(details) if details is not None else [''] * len(titles)
        for row, (title, role, detail, pos) in enumerate(zip(titles, initial, details, self._initial_positions)):
            name = label(str(title) or f'{t("측정")} {row + 1}')
            name.setToolTip(str(title))
            grid.addWidget(name, row, 0)
            grid.addWidget(label(str(detail), 'note'), row, 1)
            if show_positions:
                where = QtWidgets.QComboBox()
                for spot in range(1, count + 1):
                    where.addItem(f'위치 {spot}', spot)
                if where.findData(pos) < 0:          # 이름의 위치가 배치보다 크면 — 고르지 않으면 건너뛴다
                    where.addItem(f'위치 {pos} · 배치 밖', pos)
                where.setCurrentIndex(max(where.findData(pos), 0))
                where.currentIndexChanged.connect(self._validate)
                attach(where, 'mdat.pos')
                grid.addWidget(where, row, 2)
                self.pos_combos.append(where)
            combo = QtWidgets.QComboBox()
            combo.addItem(self.SKIP, None)
            for item in self.roles:
                combo.addItem(role_text(item), item)
            combo.setCurrentIndex(max(combo.findData(role), 0))
            combo.currentIndexChanged.connect(self._validate)
            attach(combo, 'mdat.role')
            grid.addWidget(combo, row, 3)
            self.combos.append(combo)
        grid.setColumnStretch(0, 1)
        grid.setRowStretch(len(self.combos), 1)
        scroll.setWidget(holder)
        # 경고 상자가 나타나도 목록이 눌리지 않게 높이를 확보한다(8줄까지, 넘으면 세로 스크롤).
        scroll.setHorizontalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setMinimumHeight(min(len(self.combos), 8) * 46 + 20)
        outer.addWidget(scroll, 1)

        self.warning = label('', 'helpBad', wrap=True)
        outer.addWidget(self.warning)
        self.buttons = QtWidgets.QDialogButtonBox(
            QtWidgets.QDialogButtonBox.StandardButton.Ok
            | QtWidgets.QDialogButtonBox.StandardButton.Cancel)
        self.buttons.button(QtWidgets.QDialogButtonBox.StandardButton.Ok).setText('불러오기')
        self.buttons.button(QtWidgets.QDialogButtonBox.StandardButton.Cancel).setText('취소')
        self.buttons.accepted.connect(self.accept)
        self.buttons.rejected.connect(self.reject)
        outer.addWidget(self.buttons)
        self._validate()

    def assignment(self) -> list[str | None]:
        return [combo.currentData() for combo in self.combos]

    def positions(self) -> list[int]:
        """줄마다 고른 위치(위치 칸이 없으면 이름에서 읽은 값 그대로)."""
        if not self.pos_combos:
            return list(self._initial_positions)
        return [int(combo.currentData()) for combo in self.pos_combos]

    def problems(self) -> str:
        chosen = [(role, pos) for role, pos in zip(self.assignment(), self.positions()) if role]
        doubles = sorted({item for item in chosen if chosen.count(item) > 1}, key=lambda item: (item[1], item[0]))
        if doubles:
            several = len({pos for _role, pos in chosen}) > 1
            names = [f'{role_code(role)} P{pos}' if several else role_code(role) for role, pos in doubles]
            return '같은 역할이 두 번 배정됐습니다: ' + ', '.join(names)
        if not chosen:
            return '불러올 측정이 없습니다. 하나 이상 역할을 고르세요.'
        return ''

    def _validate(self, *_args) -> None:
        text = self.problems()
        self.warning.setText(text)
        self.warning.setVisible(bool(text))
        ok = self.buttons.button(QtWidgets.QDialogButtonBox.StandardButton.Ok)
        ok.setEnabled(not text)


class MdatFilesDialog(QtWidgets.QDialog):
    """위치별 mdat 파일 관리(10-11 사용자: "위치별 mdat 를 구분해서 넣어도 된다며, 그 기능").

    위치마다 불러온 파일 · 채널 수, [불러오기…](파일 속 측정을 모두 그 위치로 — 그 파일에 든 채널만 바뀐다) ·
    [비우기](그 위치만). 모두 지우는 버튼은 두지 않는다(10-11 사용자: "왜 모든 데이터를 지우는데").
    """

    def __init__(self, page, parent=None) -> None:
        super().__init__(parent or page)
        self.page = page
        self.setWindowTitle('mdat 파일 관리')
        self.setMinimumWidth(620)
        self.setStyleSheet(f'QDialog {{ background: {COLORS["panel"]}; }}')
        self.rows: dict[int, tuple] = {}
        outer = QtWidgets.QVBoxLayout(self)
        outer.setSpacing(10)
        outer.addWidget(label('위치마다 mdat 파일을 따로 넣을 수 있습니다. [불러오기] 한 파일의 측정은 모두 그 위치로 들어가고, '
                              '그 파일에 든 채널만 바뀝니다.', 'note', wrap=True))
        scroll = QtWidgets.QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        scroll.setHorizontalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        holder = QtWidgets.QWidget()
        self.grid = QtWidgets.QGridLayout(holder)
        self.grid.setHorizontalSpacing(12)
        self.grid.setVerticalSpacing(6)
        scroll.setWidget(holder)
        scroll.setMinimumHeight(min(page.position_count(), 9) * 40 + 40)
        outer.addWidget(scroll, 1)
        buttons = QtWidgets.QDialogButtonBox(QtWidgets.QDialogButtonBox.StandardButton.Close)
        buttons.button(QtWidgets.QDialogButtonBox.StandardButton.Close).setText('닫기')
        buttons.rejected.connect(self.reject)
        outer.addWidget(buttons)
        self.rebuild()

    def rebuild(self) -> None:
        clear_layout(self.grid)
        self.rows.clear()
        page = self.page
        sources = page.mdat_sources()
        recorded = page._recorded_roles()
        total = len(page.plan().roles)
        for column, head in enumerate(('위치', '파일', '채널')):
            self.grid.addWidget(label(head, 'note'), 0, column)
        for pos in range(1, page.position_count() + 1):
            have = len(recorded.get(pos, []))
            text = sources.get(pos) or ('직접 측정' if have else '—')
            name = ElidedLabel(text)
            name.setMinimumWidth(0)
            name.setSizePolicy(_PlainQt.QSizePolicy.Policy.Ignored, _PlainQt.QSizePolicy.Policy.Preferred)
            channels = label(f'{have} / {total}', 'note')
            load = QtWidgets.QPushButton('불러오기…')
            set_variant(load, 'skip', 'sm')
            load.clicked.connect(lambda _c=False, spot=pos: self._load(spot))
            attach(load, 'files.load')
            empty = QtWidgets.QPushButton('비우기')
            set_variant(empty, 'skip', 'sm')
            empty.setEnabled(have > 0)
            empty.clicked.connect(lambda _c=False, spot=pos: self._clear(spot))
            attach(empty, 'files.clear')
            row = pos
            self.grid.addWidget(label(f'위치 {pos}'), row, 0)
            self.grid.addWidget(name, row, 1)
            self.grid.addWidget(channels, row, 2)
            self.grid.addWidget(load, row, 3)
            self.grid.addWidget(empty, row, 4)
            self.rows[pos] = (name, channels, load, empty)
        self.grid.setColumnStretch(1, 1)
        self.grid.setRowStretch(page.position_count() + 1, 1)

    def _load(self, pos: int) -> None:
        self.page.load_mdat(position=pos)
        self.rebuild()

    def _clear(self, pos: int) -> None:
        self.page.clear_position(pos)
        self.rebuild()


def role_text(item: str) -> str:
    """배정 창의 채널 이름 — 2단계 표와 같은 꼴 'Ltr · 이름'. 멀티앰프 키(`L:LF`)는 대역을 뒤에 붙인다."""
    from secs.model import role_label

    base, _, band = str(item).partition(':')
    text = f'{role_code(base)} · {role_label(base)}'
    return f'{text} · {band}' if band else text


def _record_pos(record) -> int:
    try:
        return int(record.get('pos', -1)) if isinstance(record, dict) else -1
    except (TypeError, ValueError):
        return -1


def _record_roles(record) -> list[str]:
    irs = record.get('ir') if isinstance(record, dict) else None
    return [str(role) for role, path in irs.items() if path] if isinstance(irs, dict) else []


def _role_source(record, role: str) -> str:
    """그 측정을 불러온 mdat 파일 이름('' = 직접 잰 측정). 위치 항목의 `source` = {role: 파일 이름}."""
    source = record.get('source')
    if isinstance(source, dict):
        return str(source.get(role) or '')
    return ''


def position_spans(numbers) -> str:
    """[1, 2, 3, 5] → '1–3, 5'."""
    spans: list[list[int]] = []
    for number in sorted(set(int(n) for n in numbers)):
        if spans and number == spans[-1][1] + 1:
            spans[-1][1] = number
        else:
            spans.append([number, number])
    return ', '.join(str(a) if a == b else f'{a}–{b}' for a, b in spans)


def source_summary(sources: dict[int, str]) -> str:
    """위치 → 파일 이름 → 'a.mdat (위치 1–9) · 직접 측정 (위치 10)'."""
    groups: dict[str, list[int]] = {}
    for pos in sorted(sources):
        groups.setdefault(sources[pos] or '직접 측정', []).append(pos)
    return ' · '.join(f'{name} (위치 {position_spans(spots)})' for name, spots in groups.items())


# ──────────────────────────── 저장 경로 · 표시용 곡선 ────────────────────────────
def default_project_path(prefix: str = REAL_PREFIX, now: datetime | None = None) -> Path:
    """묻지 않고 쓸 기본 `.secs` 경로. 폴더가 없으면 만든다.

    저장 위치 창은 취소할 수 있어서, 취소하면 측정이 조용히 시작되지 않았다
    (D 보고). 이제 경로가 없으면 여기에 자동으로 만들고, 사용자는 사이드바의
    [프로젝트 저장] 으로 나중에 옮긴다. 실측은 `측정 …`, Fake 샘플은 `샘플 …`.
    """
    stamp = (now or datetime.now()).strftime('%Y%m%d-%H%M')
    DEFAULT_PROJECT_DIR.mkdir(parents=True, exist_ok=True)
    return DEFAULT_PROJECT_DIR / f'{prefix} {stamp}.secs'


def magnitude_db(ir: np.ndarray, sample_rate: int) -> np.ndarray:
    """IR → `FREQS` 위의 1/12 옥타브 스무딩 크기응답(dB, 중역 기준 0 dB)."""
    samples = np.asarray(ir, dtype=float).ravel()
    if samples.size == 0:
        return np.zeros_like(FREQS)
    size = 1 << int(np.ceil(np.log2(max(samples.size, 8192))))
    spectrum = np.fft.rfft(samples, size)
    freqs = np.fft.rfftfreq(size, 1.0 / float(sample_rate))
    magnitude = 20.0 * np.log10(np.maximum(np.abs(spectrum), 1e-12))
    resampled = np.interp(np.log10(FREQS), np.log10(np.maximum(freqs, 1e-6)), magnitude)
    sigma = _octave_smoothing_sigma(FREQS, SMOOTHING_OCTAVE_FRACTION)
    smoothed = gaussian_filter1d(resampled, sigma=sigma, mode='nearest')
    mid = (FREQS >= 200.0) & (FREQS <= 2000.0)
    reference = float(np.mean(smoothed[mid])) if mid.any() else float(np.mean(smoothed))
    return smoothed - reference


# ──────────────────────────── 측정 워커 ────────────────────────────
class _MeasureWorker(QtCore.QObject):
    """워커 스레드에서 한 위치를 측정한다."""

    progress = QtCore.pyqtSignal(dict)
    finished = QtCore.pyqtSignal(int, list)
    failed = QtCore.pyqtSignal(str)

    def __init__(self, session, plan: MeasurementPlan, pos: int,
                 cancel: threading.Event) -> None:
        super().__init__()
        self.session = session
        self.plan = plan
        self.pos = int(pos)
        self.cancel = cancel

    @QtCore.pyqtSlot()
    def run(self) -> None:
        try:
            results = self.session.run_position(self.plan, self.pos,
                                                self.progress.emit, self.cancel)
        except Exception as error:  # noqa: BLE001 - 측정 실패는 화면에 문구로 알린다
            self.failed.emit(str(error))
            return
        self.finished.emit(self.pos, list(results))


_LEGEND_COLUMNS = 6       # 9.1.6·바이앰프에서 1024 폭 가로 스크롤이 생기지 않게(09-26 게이트) — 설명은 따로 한 줄


class Step5Measure(QtWidgets.QWidget):
    """5 · 측정."""

    changed = QtCore.pyqtSignal()

    def __init__(self, project, measurer=None, project_path=None, parent=None, devices=None) -> None:
        super().__init__(parent)
        self.project = project
        self.measurer = measurer
        self._mdat_busy = False      # mdat·샘플을 워커 스레드에서 읽는 중(창이 로딩 오버레이를 띄운다)
        self._offload_kind = ''      # 그 일의 종류: 'mdat' | 'sample' — 오버레이 문구를 가른다
        #: 사용자가 배치도·목록에서 고른 위치(1-기준). None 이면 다음 안 잰 위치(09-23 사용자 지시).
        self._selected: int | None = None
        # 채널 이름은 창의 REW 캐시에서 읽는다 — 측정 중 진행 표시마다 REW 에 묻지 않게(09-23).
        self.devices = devices if devices is not None else measurer
        self.project_path = Path(project_path) if project_path else None
        self.session: MeasurementSession | None = None
        self._thread: QtCore.QThread | None = None
        self._worker: _MeasureWorker | None = None
        self._cancel = threading.Event()
        self._magnitudes: dict[tuple[str, int], np.ndarray] = {}
        self._elapsed = QtCore.QElapsedTimer()
        self._countdown = QtCore.QTimer(self)          # 4단계 "측정 시작 지연" 카운트다운(1 초마다)
        self._countdown.setInterval(1000)
        self._countdown.timeout.connect(self._countdown_tick)
        self._countdown_left = 0
        self._countdown_pos = 0
        self._tick = QtCore.QTimer(self)
        self._tick.setInterval(250)
        self._tick.timeout.connect(self._update_elapsed)
        self._build()
        self.refresh()

    # ──────────────────────────── 뼈대 ────────────────────────────
    def _build(self) -> None:
        outer = QtWidgets.QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(14)
        add_heading(outer, t('5 · 측정'))

        body, main, side = two_columns(outer)      # 단계 공통 2열(style.two_columns, 09-23 정렬)

        left_card, left_box = card('')
        self.position_title = label('위치 — / —', 'h3')
        # 10-11 사용자: "측정 위치 표시가 너무 작은데" → "세로 배치이되 그래프를 끌 수 있게" — 그래프 · 범례 · 위치 칩을 숨기면
        # 배치도가 그 높이를 받아 커진다(`fit_height`). 가로 배치(배치도를 그래프 옆에)는 사용자가 거절했다.
        self._plot_hidden = False
        self.plot_toggle = QtWidgets.QPushButton(t('그래프 숨기기'))
        set_variant(self.plot_toggle, 'ghost', 'sm')
        self.plot_toggle.setCheckable(True)
        self.plot_toggle.toggled.connect(self.set_plot_hidden)
        attach(self.plot_toggle, 's5.plot_toggle')
        # 4단계 "위치당" 과 같은 값 — 이 위치를 재는 데 걸릴 어림 시간(09-23 사용자 지시).
        self.position_estimate = label('', 'note')
        # 버튼은 제목 · 어림 시간 두 줄에 걸쳐 오른쪽에 — 제목 줄에만 두면 줄이 17 px 높아져 배치도가 더 줄었다.
        header = QtWidgets.QGridLayout()
        header.setContentsMargins(0, 0, 0, 0)
        header.setVerticalSpacing(left_box.spacing())
        header.addWidget(self.position_title, 0, 0)
        header.addWidget(self.position_estimate, 1, 0)
        header.addWidget(self.plot_toggle, 0, 1, 2, 1,
                         QtCore.Qt.AlignmentFlag.AlignRight | QtCore.Qt.AlignmentFlag.AlignVCenter)
        header.setColumnStretch(0, 1)
        left_box.addLayout(header)
        self.view = ArrayLayoutView()
        # 마커를 누르면(또는 ←/→) 그 위치를 고른다 — [이 위치 측정] 이 그 위치를 잰다(09-23 사용자 지시).
        self.view.set_selectable(True)
        attach(self.view, 's5.map')
        self.view.position_clicked.connect(lambda index: self.select_position(index + 1))
        # 위에 붙인다 — 그래프를 숨기면 남는 높이가 배치도 위아래로 갈라져 제목과 배치도 사이가 비었다
        left_box.addWidget(self.view, 0, QtCore.Qt.AlignmentFlag.AlignHCenter | QtCore.Qt.AlignmentFlag.AlignTop)
        self.plot = pg.PlotWidget()
        self.plot.setBackground(COLORS['plot_bg'])
        self.plot.setMinimumHeight(260)
        attach(self.plot, 's5.plot')
        self.plot.setLogMode(x=True, y=False)
        self.plot.showGrid(x=True, y=True, alpha=0.25)
        self.plot.setMouseEnabled(x=False, y=False)
        self.plot.getPlotItem().hideButtons()
        self._default_range()
        self.plot.getAxis('bottom').setTicks(freq_ticks())      # 6단계와 같은 고정 눈금(기본 로그 눈금은 '5·10¹6·10¹…' 로 겹쳤다)
        self.axis_control = AxisControl(self.plot, 'measure', on_change=self._default_range)
        for axis in ('left', 'bottom'):
            self.plot.getAxis(axis).setTextPen(COLORS['ink2'])
            self.plot.getAxis(axis).setPen(pg.mkPen(COLORS['grid']))
        # 축 이름은 축 선 색을 칠한 뒤에 — setPen 이 이름 색을 옅은 격자색으로 덮는다(10-10 3차 시험: 'dB' · 'Hz' 가 안 보였다)
        axis_label(self.plot, 'left', 'dB')
        axis_label(self.plot, 'bottom', 'Hz')
        left_box.addWidget(self.plot, 1)
        # 범례는 4칸 격자 — 한 줄로 늘어놓으면 1024 폭에서 카드 최소 폭을 민다(실사용 QA 09-19).
        self._legend_holder = QtWidgets.QWidget()
        self.legend = QtWidgets.QGridLayout(self._legend_holder)
        self.legend.setContentsMargins(0, 0, 0, 0)
        self.legend.setHorizontalSpacing(12)
        self.legend.setVerticalSpacing(2)
        left_box.addWidget(self._legend_holder)
        # 위치 칩 — 위치별 곡선 켜고 끄기·커서로 강조(09-24 사용자: 정축(위치 1) 이 어느 선인지 모르겠다).
        self.position_chips = PositionChips()
        self.position_chips.toggled.connect(lambda *_a: self._draw())
        self.position_chips.hovered.connect(lambda _p: self._draw())
        left_box.addWidget(self.position_chips)
        main.addWidget(left_card)
        self._left_card = left_card

        right_card, right_box = card(t('위치 목록'))
        self._right_card = right_card
        self.positions = QtWidgets.QListWidget()
        self.positions.setMaximumHeight(self.LIST_MAX)
        # 줄 글자는 대리 그리기가 칸 폭에 맞춰 줄인다 — 가로 스크롤바를 두지 않는다(09-24 QA).
        self.positions.setHorizontalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self._position_delegate = _PositionDelegate(self.positions)
        self.positions.setItemDelegate(self._position_delegate)
        self.positions.currentRowChanged.connect(self._on_row)     # 목록 줄을 눌러도 같은 선택
        attach_cells(self.positions, 's5.positions')
        right_box.addWidget(self.positions)
        # 10-11 사용자: "불러오기 한 파일이 여러개인데 무슨 파일이 불러오기 된 건지도 모르겠어" — 위치별 mdat 이름 한 줄
        # (넘치면 가운데를 줄이고 전체는 풍선 도움말).
        self.source_note = ElidedLabel('')
        self.source_note.setObjectName('note')
        self.source_note.setMinimumWidth(0)
        self.source_note.setSizePolicy(_PlainQt.QSizePolicy.Policy.Ignored, _PlainQt.QSizePolicy.Policy.Fixed)
        self.source_note.setVisible(False)
        right_box.addWidget(self.source_note)
        self.progress = QtWidgets.QProgressBar()
        self.progress.setRange(0, 100)
        self.progress.setValue(0)
        right_box.addWidget(self.progress)

        buttons = QtWidgets.QHBoxLayout()
        buttons.setSpacing(8)
        self.measure_button = QtWidgets.QPushButton(t('이 위치 측정'))
        set_variant(self.measure_button, 'accent')
        self.measure_button.clicked.connect(self.measure_current)
        self.cancel_button = QtWidgets.QPushButton(t('측정 취소'))
        set_variant(self.cancel_button, 'ghost', 'sm')
        self.cancel_button.clicked.connect(self.cancel)
        self.cancel_button.setEnabled(False)
        self.redo_button = QtWidgets.QPushButton(t('마지막 위치 다시'))
        set_variant(self.redo_button, 'ghost', 'sm')
        self.redo_button.clicked.connect(self.redo_last)
        attach(self.measure_button, 's5.measure')
        attach(self.cancel_button, 's5.cancel')
        attach(self.redo_button, 's5.redo')
        # 주 버튼은 한 줄, 보조 버튼 둘은 아랫줄 — 셋을 한 줄에 두면 1024 폭에서 넘친다.
        right_box.addWidget(self.measure_button)
        buttons.addWidget(self.cancel_button)
        buttons.addWidget(self.redo_button)
        buttons.addStretch(1)
        right_box.addLayout(buttons)
        # 4단계와 같은 값(`project.extra['start_delay']`)을 여기서도 고른다(10-09 사용자 요청).
        # 버튼 줄에 같이 두면 1024 폭에서 글씨가 잘려(10-09 캡처) 따로 한 줄.
        options_grid = QtWidgets.QGridLayout()
        options_grid.setHorizontalSpacing(8)
        options_grid.setVerticalSpacing(6)
        self.delay_combo = QtWidgets.QComboBox()
        self.delay_combo.addItems([t(x) for x in START_DELAYS])
        attach(self.delay_combo, 's4.start_delay')
        self.delay_combo.currentIndexChanged.connect(self._on_delay)
        self.sweep_combo = QtWidgets.QComboBox()
        fill_sweep_combo(self.sweep_combo, int(
            (self.project.extra.get('devices', {}) or {}).get('sample_rate')
            or DEFAULT_SAMPLE_RATE))
        attach(self.sweep_combo, 's5.sweep')
        self.sweep_combo.currentIndexChanged.connect(self._on_sweep)
        self.repeat_combo = QtWidgets.QComboBox()
        for repetitions in REPETITIONS:
            self.repeat_combo.addItem(str(repetitions), repetitions)
        attach(self.repeat_combo, 's5.repeat')
        self.repeat_combo.currentIndexChanged.connect(self._on_repeat)
        self.repeat_note = label('음향 타이밍 레퍼런스는 반복 1', 'note', wrap=True)
        repeat_holder = QtWidgets.QWidget()
        repeat_row = QtWidgets.QHBoxLayout(repeat_holder)
        repeat_row.setContentsMargins(0, 0, 0, 0)
        repeat_row.setSpacing(8)
        repeat_row.addWidget(self.repeat_combo)
        repeat_row.addWidget(self.repeat_note, 1)
        for row, (name, widget) in enumerate((
                ('측정 시작 지연', self.delay_combo),
                ('스윕 길이', self.sweep_combo),
                ('스윕 반복', repeat_holder))):
            options_grid.addWidget(QtWidgets.QLabel(t(name)), row, 0)
            options_grid.addWidget(widget, row, 1)
        options_grid.setColumnStretch(1, 1)
        right_box.addLayout(options_grid)

        # 건너뛰기 버튼 둘은 세로로 — 가로로 두면 오른쪽 카드 최소 폭이 392 px 가 되어 넘친다.
        skips = QtWidgets.QVBoxLayout()
        skips.setSpacing(6)
        self.mdat_button = QtWidgets.QPushButton(t('mdat 불러오기'))
        set_variant(self.mdat_button, 'skip', 'sm')
        # clicked 는 checked(bool)를 넘긴다 — 그대로 연결하면 path=False 가 되어
        # 파일 창도 안 뜨고 조용히 끝났다(09-18 발견).
        self.mdat_button.clicked.connect(lambda _checked=False: self.load_mdat())
        self.sample_button = QtWidgets.QPushButton(t('데모 데이터'))
        set_variant(self.sample_button, 'skip', 'sm')
        self.sample_button.clicked.connect(lambda: self.load_sample())   # clicked(bool) 이 positions 로 들어가면 안 된다
        attach(self.mdat_button, 's5.mdat')
        attach(self.sample_button, 's5.sample')
        # 10-11 사용자: "위치별 mdat 를 구분해서 넣어도 된다며, 그 기능" — 위치마다 파일을 넣고 빼는 창.
        self.files_button = QtWidgets.QPushButton(t('파일 관리'))
        set_variant(self.files_button, 'skip', 'sm')
        self.files_button.clicked.connect(lambda _checked=False: self.manage_files())
        attach(self.files_button, 's5.files')
        mdat_row = QtWidgets.QHBoxLayout()
        mdat_row.setSpacing(6)
        mdat_row.addWidget(self.mdat_button)
        mdat_row.addWidget(self.files_button)
        mdat_row.addStretch(1)
        skips.addLayout(mdat_row)
        # 샘플 데이터는 체험 모드에서만 보인다(10-09 사용자: 평소엔 빼고, 체험은 데모 데이터 없이는 못 나아가니 체험에만) —
        # `set_trial` 이 켠다.
        skips.addWidget(self.sample_button, 0, QtCore.Qt.AlignmentFlag.AlignLeft)
        self.sample_button.setVisible(False)
        right_box.addLayout(skips)

        # 경과 줄 왼쪽 작은 방울 — 스윕·카운트다운 동안 돈다(09-24 사용자: "측정 등 딜레이를 느낄 지점엔
        # 애니메이션"). 로그·그래프를 가리지 않게 목록 카드 안 경과 줄에 두고, 자리는 늘 지킨다(로그가 안 흔들림).
        self.busy_drop = BusyDrop(keep_space=True)
        self.elapsed_label = label('', 'note')
        elapsed_row = QtWidgets.QHBoxLayout()
        elapsed_row.setSpacing(10)
        elapsed_row.addWidget(self.busy_drop, 0, QtCore.Qt.AlignmentFlag.AlignVCenter)
        elapsed_row.addWidget(self.elapsed_label, 1, QtCore.Qt.AlignmentFlag.AlignVCenter)
        right_box.addLayout(elapsed_row)
        self.log = QtWidgets.QPlainTextEdit()
        self.log.setObjectName('log')
        self.log.setReadOnly(True)
        self.log.setFixedHeight(self.LOG_H)
        right_box.addWidget(self.log)
        right_box.addStretch(1)      # 왼쪽 카드가 더 길 때 남는 높이가 제목 위아래로 퍼지지 않게
        side.box.addWidget(right_card)
        outer.addStretch(1)

    # ──────────────────────────── 계획 · 세션 ────────────────────────────
    def position_count(self) -> int:
        count = int(self.project.extra.get('array', 9) or 9)
        return count if count in ARRANGEMENTS else 9

    def plan(self) -> MeasurementPlan:
        """구성 + 장치 설정 → `MeasurementPlan`(멀티앰프는 `역할:대역` 으로 확장)."""
        roles: list[str] = []
        outputs: dict[str, int] = {}
        from secs.model import ordered_speakers
        # 서브는 맨 뒤로 몰아서 잰다 — 10-09 사용자 지적. 그 밖은 역할 순서(10-10: 구성에 나중에 넣은 서브도 SW1 바로
        # 뒤). 측정 순서는 이 plan 이 정한다.
        speakers = ordered_speakers(self.project.config.speakers, subs_last=True)
        for speaker in speakers:
            if len(speaker.bands) <= 1:
                out = speaker.bands[0].out if speaker.bands else 1
                roles.append(speaker.role)
                outputs[speaker.role] = int(out)
            else:
                for band in speaker.bands:
                    key = f'{speaker.role}:{band.band}'
                    roles.append(key)
                    outputs[key] = int(band.out)
        devices = dict(self.project.extra.get('devices', {}) or {})
        from secs.wizard.timing import timing_settings  # 4단계(측정 옵션)에서 고른 값
        timing_reference, timing_ref_output = timing_settings(self.project)
        return MeasurementPlan(
            roles=roles,
            outputs=outputs,
            positions=list(range(1, self.position_count() + 1)),
            output_device=str(devices.get('output', self.project.config.device or '')),
            input_device=str(devices.get('input', '')),
            input_channel=int(devices.get('input_channel', 1) or 1),
            sweep_length=str(devices.get('sweep_length', '256k') or '256k'),
            timing_reference=timing_reference,
            timing_ref_output=timing_ref_output,
            repetitions=(1 if timing_reference == 'acoustic'
                         else self._stored_repetitions(devices)),
        )

    def ensure_project_path(self, prefix: str = REAL_PREFIX) -> Path:
        """`.secs` 경로. 없으면 묻지 않고 기본 폴더에 만든다.

        취소할 수 있는 창을 띄우면 취소했을 때 측정이 시작되지 않은 채로 화면이
        그대로 남는다. 그래서 여기서는 묻지 않고 만들고, 옮기는 것은 사이드바의
        [프로젝트 저장] 에 맡긴다. `prefix` 로 실측과 샘플의 이름을 가른다.
        """
        if self.project_path is not None:
            return self.project_path
        if getattr(self, 'trial', False):        # 튜토리얼은 임시 폴더에만(2차 AI 시험: '저장 없음' 인데 문서 폴더에 생겼다)
            import tempfile
            folder = Path(tempfile.gettempdir()) / 'SECS tutorial'     # 영어 화면에 한글 경로가 보였다(2차 시험 t06)
            folder.mkdir(parents=True, exist_ok=True)
            self.project_path = folder / f"tutorial {datetime.now().strftime('%Y%m%d-%H%M')}.secs"
        else:
            self.project_path = default_project_path(prefix)
        try:
            self.project.save(self.project_path)
        except Exception as error:  # noqa: BLE001 - 저장 실패는 문구로만 알린다
            self._append(f'프로젝트를 만들지 못했습니다: {error}')
        else:
            if getattr(self, 'trial', False):
                # 경로는 숨긴다 — 사이드바는 '튜토리얼 · 저장 안 됨' 인데 로그에 임시 폴더 경로가 찍혀 저장된 줄 알았다(10-10 3차 시험)
                self._append('튜토리얼 임시 프로젝트 — 저장되지 않습니다')
            else:
                self._append(f'프로젝트를 새로 만들었습니다: {self.project_path}')
        self.changed.emit()      # 사이드바에 경로를 띄운다
        return self.project_path

    def ensure_session(self, prefix: str = REAL_PREFIX) -> MeasurementSession | None:
        if self.measurer is None:
            return None
        path = self.ensure_project_path(prefix)
        if self.session is None or self.session.project_path != path:
            self.session = MeasurementSession(self.project, self.measurer, project_path=path)
        return self.session

    # ──────────────────────────── 갱신 ────────────────────────────
    def done_positions(self) -> list[int]:
        """모든 role 을 다 잰 위치 번호(1부터).

        `.secs` 의 `measurements` 는 개정 1 모양(위치당 한 항목, `ir` 가 role 사전)이다.
        """
        needed = set(self.plan().roles)
        done: set[int] = set()
        for record in self.project.measurements or []:
            if not isinstance(record, dict):
                continue
            try:
                pos = int(record.get('pos'))
            except (TypeError, ValueError):
                continue
            irs = record.get('ir') if isinstance(record.get('ir'), dict) else {}
            if (irs and needed <= set(irs)) or bool(record.get('done')):
                done.add(pos)
        return sorted(done)

    def current_position(self) -> int:
        """[이 위치 측정] 이 잴 위치 번호: 고른 위치가 있으면 그것(이미 쟀으면 다시 잰다),
        없으면 다음 안 잰 위치. 다 끝났고 고른 것도 없으면 -1."""
        count = self.position_count()
        if self._selected is not None and 1 <= self._selected <= count:
            return self._selected
        done = set(self.done_positions())
        for pos in range(1, count + 1):
            if pos not in done:
                return pos
        return -1

    def select_position(self, pos: int) -> bool:
        """배치도 마커·목록 줄로 위치를 고른다. 측정(카운트다운 포함) 중이면 무시한다. 골랐으면 True."""
        pos = int(pos)
        if self._thread is not None or self._countdown_left > 0 or self._mdat_busy:
            self.refresh()               # 목록 줄이 움직였으면 되돌린다
            return False
        if not 1 <= pos <= self.position_count():
            return False
        self._selected = pos
        self.refresh()
        return True

    def _on_row(self, row: int) -> None:
        if row >= 0 and row + 1 != self.current_position():
            self.select_position(row + 1)

    def _on_delay(self, index: int) -> None:
        if 0 <= index < len(START_DELAYS):
            self.project.extra['start_delay'] = START_DELAYS[index]
            self._refresh_position_estimate()
            self.changed.emit()

    @staticmethod
    def _stored_repetitions(devices: dict) -> int:
        """저장된 반복을 화면 선택지 안의 정수로 정규화한다."""
        try:
            value = int(devices.get('repetitions', 1) or 1)
        except (TypeError, ValueError):
            value = 1
        return value if value in REPETITIONS else 1

    def _store_measure_option(self, key: str, value) -> None:
        devices = dict(self.project.extra.get('devices', {}) or {})
        devices[key] = value
        self.project.extra['devices'] = devices
        self._refresh_position_estimate()
        self.changed.emit()

    def _on_sweep(self, _index: int) -> None:
        self._store_measure_option(
            'sweep_length', str(self.sweep_combo.currentData() or '256k'))

    def _on_repeat(self, _index: int) -> None:
        self._store_measure_option(
            'repetitions', int(self.repeat_combo.currentData() or 1))

    def _refresh_position_estimate(self) -> None:
        current = self.current_position()
        self.position_estimate.setText(
            f'이 위치 {format_estimate(position_seconds(self.project))}' if current > 0 else '')

    # ──────────────────────────── 낮은 창 맞춤 ────────────────────────────
    #: 목록 · 로그 기본 높이(px)와 낮은 창(< FIT_BELOW)에서의 높이.
    LIST_MAX, LOG_H, FIT_BELOW = 300, 112, 900
    #: 창 높이에서 배치도 · 그래프 말고 세로로 드는 높이(px) — 머리 · 진행바 · 제목 · 범례 · 칩 · 아래 버튼 줄(1280×800 실측).
    #: 낮은 창에서는 그래프 PLOT_LOW px 를 먼저 주고 남는 높이를 배치도에 — 1280×800 에서 배치도 약 0.5 배,
    #: 그래프 주파수 축까지 한 화면에 든다(10-10 AI 시험 7 · 9 · 10번).
    FIXED_H, PLOT_LOW = 402, 250

    #: 넘친 만큼 줄일 때의 바닥(px) — 목록 2줄 · 로그 3줄 · 그래프.
    LIST_MIN, LOG_MIN, PLOT_MIN = 60, 52, 190

    #: 그래프를 숨기면 FIXED_H 에서 범례 · 칩 몫(GRAPH_EXTRA px)도 빠지고, 배치도는 카드 폭 안에서 최대 MAP_MAX 배까지.
    GRAPH_EXTRA, MAP_MAX = 110, 1.6

    def fit_height(self) -> None:
        """창 높이에 맞춰 배치도 배율 · 목록 · 로그 높이를 정한다(창이 크면 원래 크기).

        고정값(FIXED_H)으로 먼저 나눈 뒤, 실제로 넘친 만큼(스크롤 막대 범위)을 더 줄인다(`_trim_overflow`) — 영어 범례가
        길거나(9.1.6 은 범례 3줄) 튜토리얼에서 [데모 데이터] 단추가 오른쪽 칸에 더해지면 FIXED_H 가 모자라 범례 마지막 줄
        'Bold line = …' 이 반쯤 잘렸다(10-10 3차 시험)."""
        top = self.window()
        height = top.height() if top is not None else 1080
        if height != getattr(self, '_trim_for', None):       # 창 높이가 바뀌면 다시 잰다
            self._trim_for, self._trim = height, {'left': 0, 'right': 0}
        trim = self._trim
        base = self.view.scene().sceneRect().height()
        low = height < self.FIT_BELOW
        if self._plot_hidden:
            # 그래프를 껐으면 배치도가 그 높이를 받는다 — 카드 폭(오른쪽 카드 · 간격을 뺀 페이지 폭) 안에서.
            room = height - self.FIXED_H + self.GRAPH_EXTRA - trim['left']
            card_w = self.width() - self._right_card.width() - 16 - 40
            scene_w = self.view.scene().sceneRect().width()
            self.view.set_view_scale(min(self.MAP_MAX, room / base if base else 1.0,
                                         card_w / scene_w if scene_w else 1.0))
            self._fit_right(low)
            return
        # 페이지는 스크롤 영역이 '선호 높이'로 잡는다(줄바꿈 글자가 있어 heightForWidth) — 그래프 선호 480 px 가 그대로
        # 들어가 낮은 창에서 넘쳤다. 낮은 창이면 그래프 최대 높이를 남는 만큼으로(최소 PLOT_LOW).
        # 왼쪽이 넘치면 배치도부터 줄이고(그림), 배치도가 더 못 줄면 그래프가 나머지를 받는다
        self.view.set_view_scale(min(1.0, (height - self.FIXED_H - self.PLOT_LOW - trim['left']) / base if base else 1.0))
        map_h = self.view.height()
        plot_low = self.PLOT_MIN if trim['left'] else self.PLOT_LOW
        self.plot.setMinimumHeight(plot_low if low else 260)
        self.plot.setMaximumHeight(max(plot_low, height - self.FIXED_H - map_h - trim['left']) if low else 16777215)
        self._fit_right(low)

    def set_plot_hidden(self, hidden: bool) -> None:
        """[그래프 숨기기] — 그래프 · 범례 · 위치 칩을 숨기고 배치도를 키운다. 다시 누르면 되돌린다."""
        hidden = bool(hidden)
        if self.plot_toggle.isChecked() != hidden:
            self.plot_toggle.setChecked(hidden)          # toggled 가 다시 여기로 온다
            return
        self._plot_hidden = hidden
        for widget in (self.plot, self._legend_holder, self.position_chips):
            widget.setVisible(not hidden)
        self.plot_toggle.setText(t('그래프 보기') if hidden else t('그래프 숨기기'))
        self._trim_for = None                              # 넘침을 다시 잰다
        self.fit_height()

    def plot_hidden(self) -> bool:
        return self._plot_hidden

    def _fit_right(self, low: bool) -> None:
        """오른쪽 칸(목록 · 로그) 높이 — 낮은 창이면 줄이고 넘친 만큼 더 줄인다."""
        trim = self._trim
        cut = trim['right']
        list_h = max(self.LIST_MIN, 112 - cut)                     # 3줄 남짓 — 나머지는 목록 스크롤
        log_h = max(self.LOG_MIN, 72 - max(0, cut - (112 - list_h)))
        self.positions.setMaximumHeight(list_h if low else self.LIST_MAX)
        self.log.setFixedHeight(log_h if low else self.LOG_H)
        if low:
            QtCore.QTimer.singleShot(0, self._trim_overflow)

    def _trim_overflow(self) -> None:
        """낮은 창에서 단계 스크롤 영역이 넘치면 넘친 만큼 더 줄인다 — 긴 쪽 칸(왼쪽 = 그래프 · 배치도, 오른쪽 = 목록 · 로그)만."""
        area = self.parentWidget()
        while area is not None and not isinstance(area, _PlainQt.QScrollArea):
            area = area.parentWidget()
        if area is None or not self.isVisible():
            return
        # 앞서 줄인 높이가 레이아웃에 들어간 뒤에 잰다 — 안 그러면 옛 넘침으로 두 번 줄였다. 레이아웃 요청은 위젯 →
        # 부모로 한 단씩 올라가니 몇 번 비운다.
        for _ in range(8):
            _PlainQt.QApplication.sendPostedEvents(None, QtCore.QEvent.Type.LayoutRequest.value)
        over = area.verticalScrollBar().maximum()
        if over <= 0:
            return
        left = self._left_card.sizeHint().height()
        right = self._right_card.sizeHint().height()
        side = 'right' if right > left else 'left'
        before = dict(self._trim)
        self._trim[side] = min(self._trim[side] + over, 200)
        if self._trim != before:
            self.fit_height()

    def resizeEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().resizeEvent(event)
        self.fit_height()

    def showEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().showEvent(event)
        self.fit_height()

    def refresh(self) -> None:
        stored = str(self.project.extra.get('start_delay') or START_DELAYS[0])
        index = START_DELAYS.index(stored) if stored in START_DELAYS else 0
        if self.delay_combo.currentIndex() != index:
            self.delay_combo.blockSignals(True)
            self.delay_combo.setCurrentIndex(index)
            self.delay_combo.blockSignals(False)
        options_enabled = self._thread is None and self._countdown_left <= 0
        self.delay_combo.setEnabled(options_enabled)
        devices = dict(self.project.extra.get('devices', {}) or {})
        try:
            sample_rate = int(devices.get('sample_rate') or DEFAULT_SAMPLE_RATE)
        except (TypeError, ValueError):
            sample_rate = DEFAULT_SAMPLE_RATE
        fill_sweep_combo(
            self.sweep_combo, sample_rate, str(devices.get('sweep_length') or '256k'))
        self.sweep_combo.setEnabled(options_enabled)
        from secs.wizard.timing import timing_settings
        acoustic = timing_settings(self.project)[0] == 'acoustic'
        repetitions = 1 if acoustic else self._stored_repetitions(devices)
        repeat_index = self.repeat_combo.findData(repetitions)
        self.repeat_combo.blockSignals(True)
        self.repeat_combo.setCurrentIndex(max(0, repeat_index))
        self.repeat_combo.blockSignals(False)
        self.repeat_combo.setEnabled(options_enabled and not acoustic)
        self.repeat_note.setVisible(acoustic)
        if acoustic and self._stored_repetitions(devices) != 1:
            devices['repetitions'] = 1
            self.project.extra['devices'] = devices
        count = self.position_count()
        done = self.done_positions()
        current = self.current_position()
        if current <= 0 and len(done) >= count:      # 다 잰 뒤 '위치 — / 1' 이 어색했다(10-10 AI 시험)
            self.position_title.setText(f'위치 {count} / {count} · 모두 완료')
        else:
            self.position_title.setText(f'위치 {current if current > 0 else "—"} / {count}')
        # format_estimate() 가 이미 '약 …' 으로 시작한다 — '약' 을 두 번 넣지 않는다.
        self._refresh_position_estimate()
        self.view.set_state(count, [p - 1 for p in done],
                            current - 1 if current > 0 else -1)

        self.positions.clear()
        points = arr_points(count)
        sources = self.mdat_sources()
        summary = source_summary(sources) if any(sources.values()) else ''
        self.source_note.set_full_text(f'불러온 파일: {summary}' if summary else '')
        self.source_note.setVisible(bool(summary))
        for index in range(count):
            number = index + 1
            state = '완료' if number in done else '대기' if number == current else ''
            item = QtWidgets.QListWidgetItem(f'{number:02d}  {t(points[index].text)}')
            if state:
                item.setData(_STATE_ROLE, str(t(state)))
            source = sources.get(number)
            item.setToolTip(f'{number:02d}  {t(points[index].text)}' + (f' · {t(state)}' if state else '')
                            + (f' · {source}' if source else ''))
            if number in done:
                item.setForeground(QtCore.Qt.GlobalColor.darkGreen)
            self.positions.addItem(item)
        self.positions.blockSignals(True)
        if current > 0:
            self.positions.setCurrentRow(current - 1)
        self.positions.blockSignals(False)

        running = self._thread is not None
        self.measure_button.setEnabled(bool(current > 0 and not running))
        self.measure_button.setText(t('이 위치 다시 측정') if current in done
                                    else t('이 위치 측정') if current > 0 else t('모든 위치 완료'))
        self.cancel_button.setEnabled(running)
        self.redo_button.setEnabled(bool(done and not running))
        self.mdat_button.setEnabled(not running)
        self.files_button.setEnabled(not running)
        self.sample_button.setEnabled(not running)
        self.poll_busy()
        if not self.log.toPlainText():
            self.log.setPlainText(
                f'불러온 위치 {len(done)}개 · 채널 {len(self.plan().roles)}'
                if done else '대기 중. REW blocking 모드로 채널별 SPL 스윕을 보냅니다.')
        self._refresh_legend()
        self._draw()

    def _refresh_legend(self) -> None:
        clear_layout(self.legend)
        # 모든 채널(09-25 사용자: "7.1.4 에 서브 3개인데 5.1 기준으로 태그가 붙어 있네" — 처음 6개만 보였다).
        roles = list(self.plan().roles)
        colors = channel_colors(roles)
        for index, role in enumerate(roles):
            swatch = label('━ ' + role_code(role), 'note')
            # 서브는 굵게(그래프에서도 굵은 선 — 3차 시험: SW1 이 Ls 와 같은 색이었다)
            swatch.setStyleSheet(f'color:{colors[role]};' + (' font-weight: 700;' if is_sub_role(role) else ''))
            self.legend.addWidget(swatch, index // _LEGEND_COLUMNS, index % _LEGEND_COLUMNS)
        count = len(roles)
        self.legend.addWidget(label('굵은 선 = 위치 1(청취 위치)', 'note'),
                              (count + _LEGEND_COLUMNS - 1) // _LEGEND_COLUMNS, 0, 1, _LEGEND_COLUMNS)
        self.legend.setColumnStretch(_LEGEND_COLUMNS, 1)
        self.position_chips.set_count(max((pos for _role, pos in self._magnitudes), default=0))

    def _default_range(self) -> None:
        self.plot.setYRange(-35, 20)
        self.plot.setXRange(np.log10(5), np.log10(22000))
        if getattr(self, 'axis_control', None) is not None:
            self.axis_control.apply()

    def reload_curves(self) -> int:
        """저장된 측정(`.irs/`)에서 그래프 곡선을 다시 만든다 — 프로젝트를 불러올 때. 읽은 곡선 수."""
        self._magnitudes.clear()
        try:
            from secs.rewapi.session import load_project_irs
            irs = load_project_irs(self.project, self.project_path)
        except Exception:  # noqa: BLE001 - 일부 wav 가 없어도 화면은 연다
            irs = {}
        for pos, roles in irs.items():
            for role, value in roles.items():
                try:
                    ir, rate = value[0], value[1]
                    self._magnitudes[(role, int(pos))] = magnitude_db(ir, int(rate))
                except Exception:  # noqa: BLE001
                    continue
        self.refresh()
        return len(self._magnitudes)

    def _draw(self) -> None:
        """위치별 곡선 — 위치 1(청취 위치)은 굵고 진한 채널 색, 나머지는 가늘고 옅은 같은 색.

        09-24 사용자 요청으로 목업의 '마지막 위치만 채널색, 이전은 회색' 을 바꿨다: 다점 측정에서 어느 선이
        정축인지 알 수 없었다. 위치 칩으로 끈 위치는 그리지 않고, 칩에 커서를 대면 그 위치만 진하다.
        """
        self.plot.clear()
        if not self._magnitudes:
            return
        # 모든 채널을 그린다 — 전엔 앞 6채널만 그려 범례의 Ltf · SW1 … 곡선이 그래프에 없었다(10-10 3차 시험)
        roles = list(self.plan().roles)
        colors = channel_colors(roles)
        chips = getattr(self, 'position_chips', None)
        hover = chips.hover if chips is not None else 0
        for (role, pos), curve in self._magnitudes.items():
            if role not in colors or (chips is not None and not chips.visible(pos)):
                continue
            width = SUB_LINE_WIDTH if is_sub_role(role) else 2.0
            item = self.plot.plot(FREQS, curve, pen=position_pen(colors[role], pos, hover, width=width))
            item.setZValue(position_z(pos, hover))       # 위치 1(강조 중이면 그 위치)을 위로

    # ──────────────────────────── 로그 · 경과 시간 ────────────────────────────
    def _append(self, text: str) -> None:
        self.log.appendPlainText(str(t(text)))      # 영어 화면에서도 로그를 옮긴다(09-23 리뷰)

    def _update_elapsed(self) -> None:
        seconds = self._elapsed.elapsed() // 1000
        self.elapsed_label.setText(f'경과 {seconds // 60}:{seconds % 60:02d}')

    # ──────────────────────────── 측정 ────────────────────────────
    def measure_current(self) -> None:
        self.measure_position(self.current_position())

    def measure_position(self, pos: int, *, after_delay: bool = False) -> bool:
        """한 위치 측정을 시작한다(비동기). 시작했으면 True.

        4단계 "측정 시작 지연"이 켜져 있으면 그만큼 초를 센 뒤(마이크 자리로 갈 시간) 스윕을 시작한다
        (09-22: 값만 저장되고 쓰이지 않았다). `after_delay` 는 카운트다운 타이머가 부를 때.
        """
        if pos <= 0 or self._thread is not None or (self._countdown_left > 0 and not after_delay):
            return False
        delay = start_delay_seconds(self.project.extra.get('start_delay', ''))
        if delay > 0 and not after_delay:
            self._countdown_left = delay
            self._countdown_pos = pos
            self.log.clear()
            self._append(f'위치 {pos}: 마이크를 {pos} 번 위치로 옮기세요. {delay} 초 뒤 시작합니다.')
            self._countdown.start()
            self.refresh()
            return True
        session = self.ensure_session()
        if session is None:
            self._append('측정할 수 없습니다: REW 측정기 또는 저장 경로가 없습니다.')
            return False
        plan = self.plan()
        problems = plan.validate()
        if problems:
            self._append('측정 계획 오류: ' + ' · '.join(problems))
            return False

        self._cancel = threading.Event()
        self.log.clear()
        if pos in self.done_positions():
            # 확인 없이 다시 잰다 — 같은 위치의 채널 기록은 새 측정으로 바뀐다(09-23 사용자 지시).
            self._append(f'위치 {pos} 다시 측정 — 기존 기록을 새 측정으로 바꿉니다.')
        self._append(f'위치 {pos}: 마이크를 {pos} 번 위치로 옮기고 시작합니다.')
        self.progress.setValue(0)
        self._elapsed.start()
        self._tick.start()

        worker = _MeasureWorker(session, plan, pos, self._cancel)
        thread = QtCore.QThread(self)
        worker.moveToThread(thread)
        thread.started.connect(worker.run)
        worker.progress.connect(self._on_progress)
        worker.finished.connect(self._on_finished)
        worker.failed.connect(self._on_failed)
        worker.finished.connect(thread.quit)
        worker.failed.connect(thread.quit)
        thread.finished.connect(self._on_thread_done)
        self._worker, self._thread = worker, thread
        thread.start()
        self.refresh()
        return True

    def cancel(self) -> None:
        if self._countdown_left > 0:           # 아직 세는 중이면 그냥 멈춘다
            self._countdown.stop()
            self._countdown_left = 0
            self._append('시작 전에 취소했습니다.')
            self.refresh()
            return
        self._cancel.set()
        self._append('취소 요청을 보냈습니다. 진행 중인 스윕이 끝나면 멈춥니다.')

    def _countdown_tick(self) -> None:
        self._countdown_left -= 1
        if self._countdown_left > 0:
            self._append(f'{self._countdown_left} 초 뒤 시작…')
            return
        self._countdown.stop()
        self.measure_position(self._countdown_pos, after_delay=True)

    def wait(self, timeout_ms: int = 15000) -> bool:
        """진행 중인 측정 스레드가 끝날 때까지 이벤트 루프를 돌린다.

        테스트가 측정 완료를 기다릴 때와 `window.closeEvent` 가 창을 닫기 전에
        워커를 거둘 때 쓴다. 제한 시간 안에 끝났으면 True.
        """
        loop_timer = QtCore.QElapsedTimer()
        loop_timer.start()
        app = QtWidgets.QApplication.instance()
        while self._thread is not None and loop_timer.elapsed() < timeout_ms:
            if app is not None:
                app.processEvents(QtCore.QEventLoop.ProcessEventsFlag.AllEvents, 20)
        return self._thread is None

    @QtCore.pyqtSlot(dict)
    def _on_progress(self, payload: dict) -> None:
        role = str(payload.get('role', ''))
        phase = str(payload.get('phase', ''))
        percent = float(payload.get('percent', 0.0))
        pos = int(payload.get('pos', 0))
        plan_roles = self.plan().roles
        try:
            index = plan_roles.index(role)
        except ValueError:
            index = 0
        overall = (index + percent / 100.0) / max(1, len(plan_roles)) * 100.0
        self.progress.setValue(int(overall))
        if phase in ('sweep', 'cancelled') and payload.get('message'):
            if phase == 'cancelled' or percent in (0.0, 90.0):
                from secs.wizard.channels import output_label

                out = self.plan().outputs.get(role)
                where = output_label(self.devices, out) if out else '?'
                self._append(f'POST /measure/command {{"command":"SPL"}} · {role_code(role)} ({where})')
        if phase == 'done':
            self._append(f'{role} 완료 (위치 {pos})')

    @QtCore.pyqtSlot(int, list)
    def _on_finished(self, pos: int, results: list) -> None:
        for result in results:
            try:
                self._magnitudes[(result.role, int(result.pos))] = magnitude_db(
                    result.ir, int(result.sample_rate))
            except Exception:  # noqa: BLE001 - 곡선 계산 실패가 측정을 망치지 않게
                continue
        noisy = [(r.role, float(r.snr_db)) for r in results
                 if getattr(r, 'snr_db', None) is not None and float(r.snr_db) < LOW_SNR_DB]
        if noisy:
            listing = ', '.join(f'{role} {snr:.0f} dB' for role, snr in noisy)
            self._append(f'⚠ S/N 이 낮습니다({listing}, 권장 {LOW_SNR_DB:.0f} dB 이상). '
                         '스윕 레벨이나 마이크 게인을 올리거나 방을 조용히 한 뒤 이 위치를 다시 재세요.')
        if self._cancel.is_set():
            self._append(f'위치 {pos} 측정을 취소했습니다 (수신 {len(results)}개).')
        else:
            self._append(f'위치 {pos} 완료 · IR {len(results)}개 저장 '
                         f'({self.project_path.stem}.irs/)' if self.project_path
                         else f'위치 {pos} 완료 · IR {len(results)}개')
            self.progress.setValue(100)

    @QtCore.pyqtSlot(str)
    def _on_failed(self, message: str) -> None:
        self._append('측정 실패: ' + message)

    @QtCore.pyqtSlot()
    def _on_thread_done(self) -> None:
        self._tick.stop()
        thread = self._thread
        self._thread = None
        self._worker = None
        if thread is not None:
            thread.deleteLater()
        self._selected = None            # 잰 뒤에는 다시 '다음 안 잰 위치' 로
        self.refresh()
        self.changed.emit()

    def redo_last(self) -> None:
        done = self.done_positions()
        if not done:
            return
        last = done[-1]
        self.project.measurements[:] = [m for m in self.project.measurements
                                        if int(m.get('pos', -1)) != last]
        for key in [k for k in self._magnitudes if k[1] == last]:
            self._magnitudes.pop(key, None)
        if self.session is not None:
            self.session.save()
        self._append(f'위치 {last} 기록을 지웠습니다. 다시 측정하세요.')
        self.refresh()
        self.changed.emit()

    # ──────────────────────────── 건너뛰기 ────────────────────────────
    def set_trial(self, on: bool) -> None:
        """체험 모드에서만 [데모 데이터] 를 보인다(10-09). 튜토리얼이 만드는 파일은 임시 폴더로(`ensure_project_path`)."""
        self.trial = bool(on)
        self.sample_button.setVisible(bool(on))

    def load_sample(self, positions: int | None = None) -> int:
        """데모 데이터 — 위치 1 을 돌비룸 실측으로 즉시 채운다. 채운 위치 수를 돌려준다.

        2026-09-24 사용자 결정("샘플은 1점 측정만 했으니까 1점만"): [데모 데이터] 버튼
        (`positions=None`)은 고른 배치와 상관없이 **위치 1 만** 채우고 나머지는 비워 둔다 —
        실측을 변형한 가짜 위치를 채우면 7단계 최적화 목표에 따라 결과가 인공적으로 달라졌다.
        `positions` 를 2 이상으로 주는 것은 테스트·개발 도구 전용이다: 그때는 실측 대신
        테스트용 합성 FakeREW(다점 변형 포함)로 그만큼 채운다.
        """
        from secs.rewapi.fake import FakeREW

        session = self.ensure_session(SAMPLE_PREFIX)
        if session is None:      # 측정기가 없어도 샘플은 들어와야 한다
            session = self.session = MeasurementSession(
                self.project, FakeREW(delay_s=0.0),
                project_path=self.ensure_project_path(SAMPLE_PREFIX))
        plan = self.plan()
        count = int(positions) if positions else 1
        # 샘플 = 데모 방 7.1.4 + 서브 3(SW1 DBA L · SW2 DBA R · SW3 INFRA) 실측(위치 1, 10-11 사용자 결정). 그 밖의
        # 역할(바이앰프 대역 · SW4 이상 · 와이드 등)만 합성.
        fake = FakeREW(delay_s=0.0, use_sample_dolby=count == 1)
        if count == 1:
            # 앞서 잰 다른 위치가 남으면 실측 샘플과 섞인다 — 샘플은 1점 그대로 둔다.
            self.project.measurements[:] = [m for m in self.project.measurements
                                            if int(m.get('pos', -1)) == 1]
            for key in [k for k in self._magnitudes if k[1] != 1]:
                self._magnitudes.pop(key, None)

        def fill() -> tuple[int, dict]:
            curves, done = {}, 0
            for pos in range(1, count + 1):
                for result in fake.run_position(plan, pos, None, threading.Event()):
                    session.record(result, plan)
                    curves[(result.role, pos)] = magnitude_db(result.ir, result.sample_rate)
                done += 1
            return done, curves

        # 위치 × 채널 wav 쓰기에 1~2 초 걸린다(9.1.6·9위치) — 워커에서 돌리고 그동안 로딩 표시가 돈다(09-24).
        filled, curves = self._off_thread(fill, kind='sample')
        self._magnitudes.update(curves)
        if count == 1:      # '돌비룸' 은 설명 없는 이름이었다 → '데모 방'(10-10 3차 시험)
            self._append(f'샘플 측정(데모 방 7.1.4 · 서브 3 실측) {filled}개 위치 × {len(plan.roles)}채널을 불러왔습니다.')
            if self.position_count() > 1:
                self._append(f'샘플은 실측 1점뿐 — 위치 2~{self.position_count()} 은 비워 둡니다.')
        else:
            self._append(f'합성 측정(테스트용) {filled}개 위치 × {len(plan.roles)}채널을 불러왔습니다.')
        self.refresh()
        self.changed.emit()
        return filled

    def load_mdat(self, path: str | None = None, assignment=None,
                  ask: bool | None = None, position: int | None = None) -> int:
        """mdat 를 읽어 위치 1 의 측정으로 기록한다. 기록한 측정 수를 돌려준다.

        `assignment` 는 측정 순서대로의 role 목록(None = 건너뜀). 안 주면 자동 배정
        (`auto_assign`)이고, `ask` 가 참이면 역할 배정 창으로 확인받는다. 버튼으로
        부르면(`path` 없음) 기본으로 묻고, 경로를 주는 코드 호출은 묻지 않는다.
        """
        from secs.mdat_loader import load_mdat_project

        if ask is None:
            ask = path is None and assignment is None
        if path is None:
            path, _ = QtWidgets.QFileDialog.getOpenFileName(
                self, 'REW 프로젝트 불러오기', '', 'REW 프로젝트 (*.mdat)')
        if not path:
            return 0
        session = self.ensure_session()
        if session is None:
            return 0

        def parse():
            loaded = load_mdat_project(path)
            infos = list(loaded.measurements)
            irs = [loaded.ir_provider.get_ir(info.uuid) for info in infos]
            return infos, irs, [None if ir is None else looks_like_sub(ir[0], int(ir[1])) for ir in irs]

        try:
            # 큰 mdat 은 몇 초 걸린다 — 워커 스레드에서 읽고 그동안 화면(로딩 오버레이)은 돈다(09-23).
            infos, irs, sub_flags = self._off_thread(parse)
        except Exception as error:  # noqa: BLE001 - 잘못된 파일은 문구로만 알린다
            self._append(f'mdat 를 읽지 못했습니다: {error}')
            return 0

        from secs.rewapi.interfaces import MeasurementResult

        plan = self.plan()
        raw_titles = [str(getattr(info, 'title', '') or '') for info in infos]
        split = [split_position(title) for title in raw_titles]
        titles = [base for base, _pos in split]          # 자동 배정은 채널 부분으로
        positions = [pos for _base, pos in split]
        count = self.position_count()

        if position is not None:
            # 파일 관리 창의 위치 줄 [불러오기] — 파일 속 측정은 이름의 위치와 상관없이 모두 그 위치로.
            positions = [int(position)] * len(titles)
        elif not any(base != title.strip() for base, title in zip(titles, raw_titles)):
            # 이름에 위치가 없으면(`L`, `R` …) 지금 고른 위치 · 다음 안 잰 위치로 — 도움말의 '두 번 누르면 위치 1, 2 에
            # 차례로' 대로. 전에는 늘 위치 1 이라 두 번째 파일이 첫 파일을 덮었다(10-11).
            spot = self.current_position()
            positions = [spot if spot > 0 else 1] * len(titles)
        if assignment is None:
            # 위치마다 따로 배정한다 — 한꺼번에 하면 '같은 역할은 처음 측정만' 규칙에 위치 2 이상이 다 빠진다.
            assignment = [None] * len(titles)
            for spot in sorted(set(positions)):
                index = [i for i, pos in enumerate(positions) if pos == spot]
                picked = auto_assign([titles[i] for i in index], plan.roles, [sub_flags[i] for i in index])
                for i, role in zip(index, picked):
                    assignment[i] = role
            if ask:
                details = ['읽을 수 없음' if ir is None
                           else ('저역만(서브로 보임)' if flag else '전대역')
                           for ir, flag in zip(irs, sub_flags)]
                dialog = MdatAssignDialog(raw_titles, plan.roles, assignment, details, self,
                                          positions=positions, position_count=count)
                if dialog.exec() != QtWidgets.QDialog.DialogCode.Accepted:
                    self._append('mdat 불러오기를 취소했습니다.')
                    return 0
                assignment = dialog.assignment()
                positions = dialog.positions()
        assignment = list(assignment) + [None] * (len(infos) - len(list(assignment)))

        chosen = []
        used: set[tuple[str, int]] = set()
        for info, title, ir, role, pos in zip(infos, raw_titles, irs, assignment, positions):
            if ir is None:
                continue
            if not role or (role, pos) in used or role not in plan.roles:
                # 같은 위치·role 의 뒤 측정(예: 보정 후 참고 측정 'L Fin')이 앞 측정을
                # 덮어쓰지 않게 한다. 실측 v2 mdat 에서 확인된 문제.
                self._append(f'건너뜀: "{title}"')
                continue
            if pos > count:
                self._append(f'건너뜀: "{title}" — 위치 {pos} 는 배치({count} 곳) 밖입니다.')
                continue
            used.add((role, pos))
            chosen.append((info, title, ir, role, pos))

        # 새 파일에 든 위치 · 채널만 바뀌고 나머지 측정은 그대로 둔다(10-11 사용자: "실수로 위치 1개만 다시 불러올 수
        # 있는데 … 다 날리고 다시 하라는거야?"). 지우는 것은 파일 관리 창의 위치별 [비우기] 뿐.
        name = Path(path).name

        def save() -> dict:
            curves = {}
            for info, title, (samples, rate), role, pos in chosen:
                # REW 의 IR 시간 원점. WAV 에 담을 곳이 없어 `.secs` 의 't0' 만이 진실이다.
                t0 = float(getattr(info, 'start_time_s', 0.0) or 0.0)
                result = MeasurementResult(role=role, pos=pos, uuid=info.uuid, ir=samples,
                                           sample_rate=int(rate), name=title or role,
                                           t0_s=t0)
                entry = session.record(result, plan)
                if not isinstance(entry.get('source'), dict):
                    entry['source'] = {}
                entry['source'][role] = name
                curves[(role, pos)] = magnitude_db(samples, int(rate))
            if chosen:
                session.save()                   # 파일 이름까지 저장
            return curves

        curves = self._off_thread(save)          # wav 쓰기도 워커에서
        self._magnitudes.update(curves)
        saved = len(curves)
        spots = sorted({pos for _role, pos in curves})
        where = ', '.join(str(pos) for pos in spots) or '—'
        self._append(f'mdat 에서 측정 {saved}개를 위치 {where} 로 불러왔습니다: {name}')
        self.refresh()
        self.changed.emit()
        return saved

    def _recorded_roles(self) -> dict[int, list[str]]:
        """위치 → 기록이 있는 role 목록."""
        out: dict[int, list[str]] = {}
        for record in self.project.measurements or []:
            pos = _record_pos(record)
            roles = _record_roles(record)
            if pos > 0 and roles:
                out.setdefault(pos, []).extend(roles)
        return out

    def mdat_sources(self) -> dict[int, str]:
        """기록이 있는 위치 → 불러온 mdat 파일 이름('a.mdat + b.mdat', 직접 잰 채널이 섞이면 '직접 측정' 도, 모두 직접이면 '')."""
        out: dict[int, str] = {}
        for record in self.project.measurements or []:
            pos = _record_pos(record)
            roles = _record_roles(record)
            if pos <= 0 or not roles:
                continue
            names: list[str] = []
            for role in roles:
                source = _role_source(record, role)
                if source not in names:
                    names.append(source)
            out[pos] = ' + '.join(n or '직접 측정' for n in names) if any(names) else ''
        return out

    def clear_position(self, pos: int) -> None:
        """한 위치의 측정 기록을 지운다(파일 관리 창 [비우기]). 다른 위치는 그대로."""
        pos = int(pos)
        self.project.measurements[:] = [m for m in (self.project.measurements or []) if _record_pos(m) != pos]
        for key in [k for k in self._magnitudes if k[1] == pos]:
            self._magnitudes.pop(key, None)
        if self.session is not None:
            self.session.save()
        self._append(f'위치 {pos} 기록을 지웠습니다.')
        self.refresh()
        self.changed.emit()

    def manage_files(self) -> None:
        """[파일 관리] — 위치별 mdat 파일 창."""
        MdatFilesDialog(self).exec()

    def loading_mdat(self) -> bool:
        """mdat 를 읽거나 쓰는 중인가(창의 로딩 오버레이가 본다)."""
        return self._mdat_busy and self._offload_kind == 'mdat'

    def loading_sample(self) -> bool:
        """데모 데이터를 채우는 중인가(창의 로딩 오버레이가 본다, 09-24)."""
        return self._mdat_busy and self._offload_kind == 'sample'

    def working(self) -> bool:
        """스윕·카운트다운이 도는 중인가(경과 줄 옆 방울 — 창의 250 ms 점검이 본다)."""
        return self._thread is not None or self._countdown_left > 0

    def poll_busy(self) -> None:
        self.busy_drop.set_busy(self.working())

    def _off_thread(self, func, kind: str = 'mdat'):
        """`func()` 를 워커 스레드에서 돌린다(`loading.run_off_thread` — 동기 호출처럼 보이고 화면은
        계속 그려져 로딩 오버레이가 움직인다). 예외는 그대로 다시 낸다."""
        self._mdat_busy, self._offload_kind = True, kind
        try:
            return run_off_thread(func, name=f'secs-{kind}')
        finally:
            self._mdat_busy, self._offload_kind = False, ''

    # ──────────────────────────── 진행 조건 ────────────────────────────
    def can_advance(self) -> tuple[bool, str]:
        if self._thread is not None:
            return False, '측정이 진행 중입니다.'
        if not self.project.measurements:
            return False, '측정이 없습니다. 측정하거나 건너뛰기를 누르세요.'
        return True, ''


__all__ = ['Step5Measure', 'magnitude_db', 'FREQS', 'default_project_path',
           'DEFAULT_PROJECT_DIR', 'REAL_PREFIX', 'SAMPLE_PREFIX', 'match_role']
