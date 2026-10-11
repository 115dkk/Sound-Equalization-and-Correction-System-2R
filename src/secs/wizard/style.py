# -*- coding: utf-8 -*-
"""위저드 공용 스타일과 위젯 — WizardWindow는 QSS를 적용하고 단계 화면은 도우미를 호출한다.
목업의 색·글꼴 토큰으로 QSS 문자열을 만들며, 문구·상태·레이아웃을 받은 도우미는
라벨·카드·버튼 묶음을 만들거나 기존 위젯의 스타일과 배치를 갱신한다.
색은 UI 목업/secs-ui-mockup.html을 기준으로 한다.
"""
from __future__ import annotations

from PyQt6 import QtCore
from PyQt6 import QtWidgets as _PlainQt
from secs.wizard import qt as QtWidgets

# ──────────────────────────── 디자인 토큰 (목업 :root) ────────────────────────────
COLORS: dict[str, str] = {
    'chassis': '#1e3736',
    'chassis2': '#234d4a',
    'marker': '#30e3da',
    'ground': '#e6eeed',
    'panel': '#f2f6f5',
    'paper': '#d5e0df',
    'grid': '#b9c8c6',
    'line': '#acbcba',
    'ink': '#1b2a29',
    'ink2': '#4f6462',
    'ink3': '#7a8e8c',
    'accent': '#1e8c86',
    'accent_ink': '#ffffff',
    'measured': '#a0258a',
    'corrected': '#7e861c',
    'target': '#8c781d',
    'xo': '#3d73c2',
    'sub': '#6a32a6',
    'ok': '#1f8a3f',
    'warn': '#a8561f',
    'bad': '#a63a25',
    'ok_bg': '#ddf1e3',
    'warn_bg': '#f6e6d6',
    'bad_bg': '#f5dad5',
    'radius': '4px',
    # 목업의 파생색(계산값이 아니라 목업 CSS 에 그대로 적힌 값들)
    'sel_bg': '#dff0ee',
    'head_bg': '#e9efee',
    'meter_bg': '#c9d5d3',
    'plot_bg': '#d9dde3',
    'side_text': '#dbe9e7',
    'side_dim': '#b9d0cd',
    'side_key': '#8fb0ac',
    'side_rule': '#3a5a57',
    'step_off': '#4b6a67',
    'step_done': '#8fb0ac',
    'step_text': '#aac6c2',
}

#: 5단계 측정 곡선 색 — 메인 채널(구성 순서). 처음 색은 목업 chColor() 그대로. 10-10 3차 시험: 8색이라 12채널에서
#: L/Rtf · R/Ltr · C/Rtr · Ls/SW1 이 같은 색이었고 검정(Ltf)은 튜토리얼 막 아래 범례에서 안 보였다 → 16색, 모두 중간 밝기
#: (그래프 바탕 #d9dde3 · 카드 바탕 · 튜토리얼 막에 덮인 카드에서 대비 2.4 이상), 앞 12색은 서로 ΔE 20 이상.
CHANNEL_COLORS: tuple[str, ...] = (
    '#c23f9b', '#6f7d12', '#1f6fb8', '#7d4cb0',
    '#c25e00', '#18875a', '#c62f2f', '#07808c',
    '#8a5524', '#4f5fd6', '#6b7f8c', '#9c7700',
    '#e0558c', '#3f9a2c', '#2e95d3', '#a06fc8',
)
#: 서브 곡선 색 — 메인과 겹치지 않는 진한 색 + 굵은 선(`SUB_LINE_WIDTH`)이라 눈에 띈다(3차 시험: SW1 이 Ls 와 같은 색).
SUB_LINE_COLORS: tuple[str, ...] = (
    '#1c2a8c', '#8e1b1b', '#005f56', '#4e342e', '#6a1b9a', '#33691e', '#263238', '#880e4f',
)
SUB_LINE_WIDTH = 3.0


def channel_colors(roles) -> dict[str, str]:
    """역할(측정 계획의 'L' · 'L:LF' · 'SW1' …) → 곡선 색. 메인은 순서대로 `CHANNEL_COLORS`, 서브는 `SUB_LINE_COLORS`."""
    out: dict[str, str] = {}
    mains = subs = 0
    for role in roles:
        if is_sub_role(role):
            out[role] = SUB_LINE_COLORS[subs % len(SUB_LINE_COLORS)]
            subs += 1
        else:
            out[role] = CHANNEL_COLORS[mains % len(CHANNEL_COLORS)]
            mains += 1
    return out


def is_sub_role(role: str) -> bool:
    """서브 역할인가('SW1' · 'SW2:SUB' …)."""
    from secs.model import role_group

    return role_group(str(role).split(':')[0]) == 'sub'

#: 목업 --font-ui / --font-body / --font-mono. 설치되지 않은 환경을 위해 폴백을 붙인다.
FONT_UI = '"Barlow", "IBM Plex Sans KR", "Malgun Gothic", "Segoe UI", sans-serif'
FONT_BODY = '"IBM Plex Sans KR", "Malgun Gothic", "Barlow", "Segoe UI", sans-serif'
FONT_MONO = '"IBM Plex Mono", "Consolas", "Malgun Gothic", monospace'

#: 목업 .band 배경색(대역 칩).
BAND_COLORS: dict[str, tuple[str, str]] = {
    'LF': ('#e6e0f2', '#c9bde3'),
    'MF': ('#e8eed6', '#c9d3a8'),
    'HF': ('#dbeaf5', '#b7cfe6'),
    'SUB': ('#e6e0f2', '#c9bde3'),
    'FULL': ('#e9efee', '#acbcba'),
}

#: 목업 BAND_KO.
BAND_KO: dict[str, str] = {'FULL': '풀레인지', 'LF': 'LF', 'MF': 'MF', 'HF': 'HF', 'SUB': '서브'}


_TEMPLATE = """
QWidget { font-family: @font_body; font-size: 14px; color: @ink; }
QWidget#wizardRoot { background: @ground; }

/* ── 사이드바(목업 .side) ── */
QFrame#side { background: @chassis; }
QFrame#side QLabel { color: @side_dim; font-size: 12.5px; }
QLabel#brand { font-family: @font_ui; font-weight: 700; font-size: 24px; color: #ffffff; }
QLabel#brandSub { font-family: @font_ui; font-size: 12px; color: #9fc0bc; letter-spacing: 1px; }
QLabel#sideKey { color: @side_key; }
QLabel#sideValue { color: #ffffff; }
QLabel#sideTitle { font-family: @font_ui; font-size: 13px; font-weight: 600; color: #ffffff; }
QFrame#sideRule { background: @side_rule; max-height: 1px; min-height: 1px; border: 0; }
QFrame#side QPushButton {
    background: transparent; border: 1px solid @side_rule; border-radius: @radius;
    color: @side_text; padding: 5px 10px; font-size: 12.5px;
}
QFrame#side QPushButton:hover { background: @chassis2; border-color: @step_done; }
/* [프로젝트 저장] 옆 ▾(다른 이름으로 저장) — 사이드바 버튼과 같은 색(10-10 사용자: 혼자 밝은 회색이었다) */
QFrame#side QToolButton {
    background: transparent; border: 1px solid @side_rule; border-radius: @radius;
    color: @side_text; padding: 5px 6px; font-size: 12.5px;
}
QFrame#side QToolButton:hover { background: @chassis2; border-color: @step_done; }
QFrame#side QToolButton::menu-indicator { image: none; width: 0; }

/* ── 상단 진행바(목업 .steps / .step) ── */
QFrame#stepsBar { background: @chassis2; }
QPushButton#step {
    background: transparent; border: 0; color: @step_text;
    font-family: @font_ui; font-size: 12.5px; padding: 0 0 6px 0; text-align: center;
}
QPushButton#step[state="done"] { color: @side_text; }
QPushButton#step[state="cur"] { color: #ffffff; font-weight: 600; }
QPushButton#step:hover { color: #ffffff; }

/* ── 본문(목업 .page / .card / .head) ── */
QWidget#page { background: @ground; }
QScrollArea { background: @ground; border: 0; }
QScrollArea > QWidget > QWidget { background: @ground; }
QFrame#card { background: @panel; border: 1px solid @line; border-radius: @radius; }
QLabel#h2 { font-family: @font_ui; font-size: 22px; font-weight: 600; color: @ink; }
QLabel#h3 { font-family: @font_ui; font-size: 15px; font-weight: 600; color: @ink2; }
QLabel#lead { color: @ink2; font-size: 14px; }
QLabel#note { color: @ink3; font-size: 12.5px; }
QLabel#spec { color: @ink3; font-size: 12.5px; }   /* 사양·결과 숫자 줄(설명문이 아니다 — 보정 설계 09-26) */
QLabel#help {
    background: @panel; border: 1px solid @line; border-radius: @radius;
    padding: 10px 14px; font-size: 13px; color: @ink2;
}
QLabel#helpBad {
    background: @bad_bg; border: 1px solid @bad; border-radius: @radius;
    padding: 10px 14px; font-size: 13px; color: @bad;
}
QLabel#num { font-family: @font_mono; }

/* ── 버튼(목업 .btn) ── */
QPushButton {
    background: @chassis; border: 1px solid @chassis; color: #ffffff;
    border-radius: 14px; padding: 8px 18px;
    font-family: @font_ui; font-weight: 600; font-size: 14px;
}
QPushButton:hover { background: @chassis2; }
QPushButton:disabled { background: @chassis; color: #9fb4b2; border-color: @chassis; }
QPushButton[variant="ghost"] { background: transparent; color: @chassis; border-color: @chassis; }
QPushButton[variant="ghost"]:hover { background: #dde6e5; }
QPushButton[variant="accent"] { background: @accent; border-color: @accent; }
QPushButton[variant="accent"]:hover { background: #17706b; }
QPushButton[variant="skip"] {
    background: transparent; border: 1px dashed @ink3; color: @ink2; font-weight: 500;
}
QPushButton[variant="skip"]:hover { background: #dde6e5; color: @ink; }
QPushButton[size="sm"] { padding: 5px 12px; font-size: 13px; }
/* 변형 버튼의 꺼진 모습 — 속성 선택자가 위의 :disabled 보다 우선해 꺼진 [계산 취소]가 켜진 것처럼 보였다(10-09 QA) */
QPushButton[variant="ghost"]:disabled { background: transparent; color: #a9b8b6; border-color: #c4cfcd; }
QPushButton[variant="accent"]:disabled { background: #8fbcb9; border-color: #8fbcb9; color: #eef5f4; }

/* 프리셋 버튼(목업 .preset) */
QPushButton#preset {
    background: #ffffff; border: 1px solid @line; border-radius: @radius;
    color: @ink2; font-family: @font_ui; font-weight: 600; font-size: 14px;
    padding: 8px 12px; text-align: center;
}
QPushButton#preset[state="on"] { border-color: @accent; color: @accent; background: @sel_bg; }

/* 배치 선택 버튼(목업 .arr button) */
QPushButton#arrPick {
    background: #ffffff; border: 1px solid @line; border-radius: @radius;
    color: @ink; padding: 8px 10px; text-align: left; font-family: @font_ui; font-weight: 600;
}
QPushButton#arrPick[state="on"] { border-color: @accent; background: @sel_bg; }
QPushButton#arrPick:hover { border-color: @ink3; }
QPushButton#arrPick[state="on"]:hover { border-color: @accent; }
QLabel#arrName { font-family: @font_ui; font-weight: 700; font-size: 14px; color: @ink; }
QLabel#arrSub { font-size: 12px; color: @ink3; }
QLabel#arrCount { font-family: @font_mono; font-weight: 700; font-size: 12.5px; color: @accent; }
QLabel#arrUnit { font-size: 12px; color: @ink2; }

/* 3단계 채널 줄 구분선(목업 .lvl border-bottom) */
QFrame#rowRule { background: @line; border: 0; max-height: 1px; min-height: 1px; }

/* 스텝퍼(목업 .stp) · 아이콘 버튼(목업 .iconbtn) */
QPushButton#stp {
    background: @chassis; border: 1px solid @chassis; color: #ffffff;
    border-radius: 13px; min-width: 26px; max-width: 26px; min-height: 26px; max-height: 26px;
    padding: 0; font-family: @font_ui; font-weight: 700; font-size: 15px;
}
QPushButton#stp:disabled { background: #7e918f; border-color: #7e918f; }
QPushButton#iconbtn {
    background: #ffffff; border: 1px solid @chassis; color: @chassis;
    border-radius: 15px; min-width: 30px; max-width: 30px; min-height: 30px; max-height: 30px;
    padding: 0; font-size: 12px;
}
QPushButton#iconbtn:hover { background: @chassis; color: #ffffff; }
QPushButton#iconbtn[state="on"] { background: @accent; border-color: @accent; color: #ffffff; }   /* 3단계 재생 중(■) */
QPushButton#pol {
    background: #ffffff; border: 1px solid @line; border-radius: 3px; color: @ink;
    padding: 2px 10px; font-size: 12px; font-weight: 400; min-width: 36px;
}   /* 고정폭 글꼴에는 한글이 없어 '정상'·'반전' 이 대체 글꼴로 겹쳐 그려졌다(09-23 사용자) */
QPushButton#pol[state="inv"] { background: @warn_bg; border-color: @warn; color: @warn; }

/* ── 알약 배지(목업 .pill) · 칩(.chip) · 대역(.band) ── */
QLabel#pill {
    border-radius: 9px; padding: 2px 9px; font-family: @font_ui; font-weight: 600; font-size: 12px;
    background: #e1e4ea; color: @ink2;
}
QLabel#pill[tone="ok"] { background: @ok_bg; color: @ok; }
QLabel#pill[tone="warn"] { background: @warn_bg; color: @warn; }
QLabel#pill[tone="bad"] { background: @bad_bg; color: @bad; }
QLabel#chip {
    border: 1px solid @line; border-radius: 3px; background: #ffffff;
    font-family: @font_mono; font-size: 12px; padding: 1px 7px;
}

/* ── 입력 위젯 ── */
QComboBox, QSpinBox, QDoubleSpinBox, QLineEdit {
    background: #ffffff; border: 1px solid @line; border-radius: @radius;
    padding: 3px 6px; color: @ink; font-size: 13px;
}
QComboBox:focus, QSpinBox:focus, QDoubleSpinBox:focus, QLineEdit:focus { border-color: @accent; }
/* 꺼진 칸은 흐리게 — 켜진 칸과 똑같이 보여 '안 쓰는 값'인지 알 수 없었다(10-09 사용자) */
QComboBox:disabled, QSpinBox:disabled, QDoubleSpinBox:disabled, QLineEdit:disabled {
    background: @paper; color: @ink3; border-color: @paper;
}
QComboBox QAbstractItemView {
    background: #ffffff; color: @ink; selection-background-color: @sel_bg; selection-color: @ink;
    border: 1px solid @line;
}
/* 우클릭 메뉴·도움말·작은 대화상자: 시스템 다크 모드와 상관없이 밝은 바탕(09-23 사용자 — 검은 바탕에 짙은 글자) */
QMenu {
    background: #ffffff; color: @ink; border: 1px solid @line; padding: 4px 0; font-size: 13px;
}
QMenu::item { padding: 5px 22px 5px 26px; background: transparent; }
QMenu::item:selected { background: @sel_bg; color: @ink; }
QMenu::item:disabled { color: @ink2; }
QMenu::separator { height: 1px; background: @line; margin: 4px 8px; }
QDialog { background: @panel; color: @ink; }
QTextBrowser { background: #ffffff; color: @ink; }
QToolTip { background: #ffffff; color: @ink; border: 1px solid @line; padding: 3px 6px; }
QCheckBox { color: @ink2; font-size: 13px; spacing: 6px; }
QSlider::groove:horizontal { height: 4px; background: @meter_bg; border-radius: 2px; }
QSlider::sub-page:horizontal { background: @accent; border-radius: 2px; }
QSlider::handle:horizontal {
    width: 14px; height: 14px; border-radius: 7px; background: @chassis; margin: -5px 0;
}

/* ── 표(목업 table.patch) ── */
QTableWidget {
    background: #ffffff; border: 1px solid @line; border-radius: @radius;
    gridline-color: @line; font-size: 13px; color: @ink;
}
QTableWidget::item { padding: 3px 6px; }
QTableWidget::item:selected { background: @sel_bg; color: @ink; }
QHeaderView::section {
    background: @head_bg; color: @ink2; border: 0; border-bottom: 1px solid @line;
    padding: 5px 8px; font-family: @font_ui; font-weight: 600; font-size: 12px;
}

/* ── 목록(목업 .poslist) · 로그(.sweeplog) ── */
QListWidget {
    background: #ffffff; border: 1px solid @line; border-radius: @radius;
    font-size: 13px; color: @ink; outline: 0;
}
QListWidget::item { border-bottom: 1px solid @head_bg; padding: 5px 8px; }
QListWidget::item:selected { background: @sel_bg; color: @ink; }
QPlainTextEdit#log {
    background: #ffffff; border: 1px solid @line; border-radius: @radius;
    font-family: @font_mono; font-size: 12px; color: @ink2; padding: 6px 8px;
}

/* ── 진행바(목업 .prog) ── */
QProgressBar {
    background: @meter_bg; border: 0; border-radius: 2px; max-height: 8px; min-height: 8px;
    text-align: center; color: transparent;
}
QProgressBar::chunk { background: @accent; border-radius: 2px; }

/* ── 하단 내비(목업 .nav) ── */
QFrame#nav { background: #e8eaee; border-top: 1px solid @line; }
QLabel#navMid { color: @ink2; font-size: 13px; }

/* ── 6단계(목업 .tabs / .seg / .grp / .ptrow / .snap) ── */
QPushButton#tab {
    background: #ffffff; border: 1px solid @line; border-bottom: 0;
    border-top-left-radius: @radius; border-top-right-radius: @radius;
    border-bottom-left-radius: 0; border-bottom-right-radius: 0;
    color: @ink2; padding: 5px 12px; font-family: @font_ui; font-weight: 600; font-size: 13px;
}
QPushButton#tab[state="on"] { background: @chassis; color: #ffffff; border-color: @chassis; }
QFrame#seg { background: #ffffff; border: 1px solid @line; border-radius: 13px; }
QPushButton#segBtn {
    background: transparent; border: 0; border-radius: 12px; color: @ink2;
    padding: 4px 12px; font-family: @font_ui; font-weight: 600; font-size: 12.5px;
}
QPushButton#segBtn[state="on"] { background: @chassis; color: #ffffff; }
QPushButton#segBtn:disabled { color: @ink3; background: transparent; }
QFrame#grp { background: #ffffff; border: 1px solid @line; border-radius: @radius; }
QFrame#grp[state="on"] { border-color: @accent; }
QLabel#grpName { font-family: @font_ui; font-size: 12px; color: @ink3; }
QPushButton#chChip {
    background: @head_bg; border: 1px solid transparent; border-radius: 3px;
    color: @ink; font-family: @font_mono; font-weight: 400; font-size: 12px; padding: 1px 7px;
}
QPushButton#chChip[state="on"] { background: @chassis; color: #ffffff; }
QFrame#ptrow { background: transparent; border-radius: 3px; }
QFrame#ptrow[state="sel"] { background: @sel_bg; }
QPushButton#snapBtn {
    background: #ffffff; border: 1px solid @line; border-radius: 3px; color: @ink;
    font-family: @font_mono; font-weight: 400; font-size: 12px; padding: 2px 8px;
}
QPushButton#snapBtn[state="on"] { background: @accent; color: #ffffff; border-color: @accent; }

/* ── 키보드 포커스(P6) — 크기가 흔들리지 않게 테두리 굵기는 두고 색만 바꾼다 ── */
QPushButton:focus { border-color: @marker; }
QPushButton[variant="ghost"]:focus, QPushButton[variant="skip"]:focus {
    border: 1px solid @accent; background: #dde6e5; color: @ink;
}
QPushButton#preset:focus, QPushButton#arrPick:focus, QPushButton#pol:focus, QPushButton#tab:focus,
QPushButton#chChip:focus, QPushButton#snapBtn:focus, QPushButton#iconbtn:focus { border-color: @accent; }
QPushButton#stp:focus { border-color: @marker; }
QPushButton#segBtn:focus { border: 1px solid @accent; padding: 3px 11px; }
QPushButton#step:focus { color: @marker; text-decoration: underline; }
QCheckBox:focus { color: @ink; text-decoration: underline; }
QSlider::handle:horizontal:focus { background: @accent; }
QTableWidget:focus, QListWidget:focus, QPlainTextEdit#log:focus { border-color: @accent; }
"""


def _substitute(template: str, colors: dict | None = None) -> str:
    colors = colors or COLORS
    text = template
    for key in sorted(colors, key=len, reverse=True):
        text = text.replace('@' + key, colors[key])
    text = text.replace('@font_ui', FONT_UI)
    text = text.replace('@font_body', FONT_BODY)
    text = text.replace('@font_mono', FONT_MONO)
    return text


#: 위저드 창 전체에 적용하는 QSS.
QSS = _substitute(_TEMPLATE)

#: 체험 모드 키 색(10-09 사용자: 위에 띠를 두지 말고 키 색을 분홍으로 — 띠가 화면을 줄여 불편했다).
TRIAL_COLORS = {**COLORS, 'accent': '#d6338a', 'marker': '#ff8ccb', 'chassis': '#4a1d36', 'chassis2': '#62284a'}
TRIAL_QSS = _substitute(_TEMPLATE, TRIAL_COLORS).replace('#17706b', '#ad2570')


# ──────────────────────────── 작은 도우미 ────────────────────────────
def repolish(widget: QtWidgets.QWidget) -> None:
    """동적 프로퍼티(state/tone/variant)를 바꾼 뒤 스타일을 다시 적용한다."""
    style = widget.style()
    if style is not None:
        style.unpolish(widget)
        style.polish(widget)
    widget.update()


def set_variant(widget: QtWidgets.QWidget, variant: str | None = None,
                size: str | None = None) -> QtWidgets.QWidget:
    """버튼에 목업의 .btn 변형(ghost/accent/skip)과 크기(sm)를 준다."""
    if variant:
        widget.setProperty('variant', variant)
    if size:
        widget.setProperty('size', size)
    repolish(widget)
    return widget


def set_state(widget: QtWidgets.QWidget, state: str) -> None:
    widget.setProperty('state', state)
    repolish(widget)


def band_style(band: str) -> str:
    """대역 칩(.band)의 인라인 스타일."""
    bg, border = BAND_COLORS.get(band, BAND_COLORS['FULL'])
    return (f'background:{bg};border:1px solid {border};border-radius:3px;'
            f'font-family:{FONT_MONO};font-size:11.5px;padding:1px 6px;color:{COLORS["ink"]};')


# ──────────────────────────── 목업 조각 위젯 ────────────────────────────
class WrapLabel(QtWidgets.QLabel):
    """줄바꿈 라벨 — 폭이 정해진 뒤의 실제 높이를 최소 높이로 잡는다.

    Qt 레이아웃은 줄바꿈 QLabel 의 높이를 폭이 정해지기 전에 어림해서, 좁은 칸에서는
    글자가 위아래로 잘린다(6단계 분해능 안내에서 확인, 09-18). 크기가 바뀔 때마다
    `heightForWidth` 로 다시 맞춘다.
    """

    def _fit(self) -> None:
        if not self.wordWrap() or self.width() <= 0:
            return
        height = self.heightForWidth(self.width())
        if height > 0 and height != self.minimumHeight():
            self.setMinimumHeight(height)

    def resizeEvent(self, event) -> None:  # noqa: N802 - Qt 규약
        super().resizeEvent(event)
        self._fit()

    def setText(self, text: str) -> None:  # noqa: N802
        super().setText(text)
        self._fit()


def label(text: str, name: str = '', wrap: bool = False) -> QtWidgets.QLabel:
    """objectName 으로 QSS 규칙(h2/h3/lead/note/help/num)을 고르는 라벨."""
    item = WrapLabel(text) if wrap else QtWidgets.QLabel(text)
    if name:
        item.setObjectName(name)
    item.setWordWrap(wrap)
    return item


def plain_label(text: str, name: str = '') -> _PlainQt.QLabel:
    """번역 위젯을 거치지 않는 라벨 — 채널 이름(Left · Center · Subwoofer 1)처럼 언어와 상관없이
    그대로 보여야 하는 글자용. 번역 라벨에 넣으면 옛 GUI 카탈로그가 'Center' 를 '센터'로 되돌린다(09-22)."""
    item = _PlainQt.QLabel(text)
    if name:
        item.setObjectName(name)
    return item


class ElidedLabel(_PlainQt.QLabel):
    """한 줄 라벨 — 폭이 모자라면 가운데를 `…` 로 줄이고 전체 글자는 풍선 도움말로 둔다.

    채널 이름처럼 화면 언어와 무관한 글자용이라 번역 위젯을 쓰지 않는다(`plain_label` 과 같은 이유).
    크기 힌트는 줄이기 전 전체 글자 폭이다 — 줄인 글자로 힌트를 주면 창을 다시 넓혀도 칸이
    커지지 않는다. 3단계 채널 이름이 좁은 칸에서 2~3줄로 접혀 행 높이가 제각각이었다(2026-09-24).
    """

    def __init__(self, text: str = '', parent=None) -> None:
        super().__init__(parent)
        self._full = ''
        self.setWordWrap(False)
        self.setMinimumWidth(40)
        self.set_full_text(text)

    def sizeHint(self) -> QtCore.QSize:  # noqa: N802 - Qt 이름
        hint = super().sizeHint()
        margins = self.contentsMargins()
        return QtCore.QSize(self.fontMetrics().horizontalAdvance(self._full) + 4
                            + margins.left() + margins.right(), hint.height())

    def minimumSizeHint(self) -> QtCore.QSize:  # noqa: N802 - Qt 이름
        return QtCore.QSize(self.minimumWidth(), super().minimumSizeHint().height())

    def changeEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().changeEvent(event)
        if event.type() == QtCore.QEvent.Type.FontChange:
            self._elide()                  # 스타일시트 글꼴이 붙은 뒤 글자 폭을 다시 잰다

    def set_full_text(self, text: str) -> None:
        self._full = str(text)
        self.setToolTip(self._full)
        self._elide()

    def full_text(self) -> str:
        return self._full

    def _elide(self) -> None:
        metrics = self.fontMetrics()
        width = max(self.width(), 40)
        elided = metrics.elidedText(self._full, QtCore.Qt.TextElideMode.ElideMiddle, width)
        _PlainQt.QLabel.setText(self, elided)

    def resizeEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().resizeEvent(event)
        self._elide()


class ElidedPathLabel(ElidedLabel):
    """가운데를 줄인 한 줄 경로 표시(2026-09-24, 8단계 내보낼 폴더 줄).

    파일 경로는 채널 이름처럼 화면 언어와 무관해 `plain_label` 과 같은 이유로 번역 위젯을
    쓰지 않는다. 폭이 좁으면 가운데를 `…` 로 줄이고, 전체 경로는 풍선 도움말로 그대로 둔다
    (`test_qa_layout_*` 가 1024 폭에서 가로 스크롤이 없는지 본다).
    """

    def __init__(self, text: str = '', parent=None) -> None:
        super().__init__(text, parent)
        self.setMinimumWidth(0)
        # 고정폭 글꼴(Consolas) — 한국어 Windows 기본 글꼴(맑은 고딕)은 백슬래시를 '₩' 로
        # 그려 경로가 'C:₩Users₩…' 로 보였다(2026-09-24 실사용 QA).
        self.setStyleSheet(f'font-family:{FONT_MONO};')

    # 경로 줄은 전과 같이 QLabel 기본 크기 힌트(줄인 글자 기준)를 쓴다 — 8단계 배치가 이 값에 맞춰져 있다.
    def sizeHint(self) -> QtCore.QSize:  # noqa: N802 - Qt 이름
        return _PlainQt.QLabel.sizeHint(self)

    def minimumSizeHint(self) -> QtCore.QSize:  # noqa: N802 - Qt 이름
        return _PlainQt.QLabel.minimumSizeHint(self)


def path_label(text: str = '', name: str = '') -> ElidedPathLabel:
    """`ElidedPathLabel` + objectName(QSS 는 label() 과 같은 규칙을 쓴다)."""
    item = ElidedPathLabel(text)
    if name:
        item.setObjectName(name)
    return item


class _ColumnFitter(QtCore.QObject):
    """표 크기가 바뀌면 `fit_table_columns` 를 다시 부른다(한 이벤트 루프 뒤 — 열 폭이 먼저
    정해진 다음 행 높이를 잰다)."""

    def __init__(self, table, spread: bool) -> None:
        super().__init__(table)
        self._table, self._spread, self._queued = table, spread, False

    def eventFilter(self, obj, event) -> bool:  # noqa: N802 - Qt 이름
        if obj is self._table and event.type() == QtCore.QEvent.Type.Resize and not self._queued:
            self._queued = True
            QtCore.QTimer.singleShot(0, self._run)
        return False

    def _run(self) -> None:
        self._queued = False
        try:
            _fit_columns(self._table, self._spread)
        except RuntimeError:               # 표가 이미 지워졌다
            pass


def _fit_columns(table, spread: bool) -> None:
    header = table.horizontalHeader()
    header.setStretchLastSection(False)
    header.setSectionResizeMode(QtWidgets.QHeaderView.ResizeMode.Interactive)
    table.resizeColumnsToContents()
    count = table.columnCount()
    if count:
        extra = table.viewport().width() - sum(table.columnWidth(c) for c in range(count))
        if extra > 0:                      # 남는 폭은 나눠 준다(모자라면 표 안 가로 스크롤)
            if spread:
                share, rest = divmod(extra, count)
                for c in range(count):
                    table.setColumnWidth(c, table.columnWidth(c) + share + (1 if c < rest else 0))
            else:
                table.setColumnWidth(count - 1, table.columnWidth(count - 1) + extra)
    table.resizeRowsToContents()           # 열 폭이 정해진 뒤에 행 높이


def fit_table_columns(table, spread: bool = False) -> None:
    """표 열 폭 = 내용의 최소 폭(말줄임·줄바꿈 없음). 남는 폭은 마지막 열(`spread` 면 모든 열)에
    나눠 주고, 모자라면 표 안에서만 가로 스크롤한다(페이지는 가로 스크롤하지 않는다).

    2026-09-24 실사용 QA: 7단계 매트릭스(Stretch 로 15열 이상 → '✓ -…', 'R:LI')와 8단계 시트
    (PEQ 열이 남는 폭만 받아 ~70px, 행 높이 ~700px)가 잘렸다. 처음 부르면 크기 변화 감시를
    붙이고, 내용을 새로 채운 뒤에도 다시 부른다."""
    table.setWordWrap(False)
    table.setHorizontalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAsNeeded)
    table.setHorizontalScrollMode(QtWidgets.QAbstractItemView.ScrollMode.ScrollPerPixel)
    if getattr(table, '_secs_fitter', None) is None:
        table._secs_fitter = _ColumnFitter(table, spread)
        table.installEventFilter(table._secs_fitter)
    _fit_columns(table, spread)


def pill(text: str, tone: str = 'na') -> QtWidgets.QLabel:
    """목업 .pill — ok / warn / bad / na."""
    item = QtWidgets.QLabel(text)
    item.setObjectName('pill')
    item.setProperty('tone', tone)
    item.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
    item.setSizePolicy(QtWidgets.QSizePolicy.Policy.Maximum,
                       QtWidgets.QSizePolicy.Policy.Fixed)
    return item


def chip(text: str) -> QtWidgets.QLabel:
    """목업 .chip — 모노스페이스 역할 칩. 역할 ID 는 화면용 돌비 약어(`role_code`)로 보인다."""
    from secs.model import role_code
    item = _PlainQt.QLabel(role_code(text))      # 역할 코드는 번역하지 않는다
    item.setObjectName('chip')
    item.setSizePolicy(QtWidgets.QSizePolicy.Policy.Maximum,
                       QtWidgets.QSizePolicy.Policy.Fixed)
    _fit_chip(item)
    return item


def _fit_chip(item) -> None:
    """칩 글자가 잘리지 않게(표 칸의 'Rrs' 가 'Rr' 로 보였다 — 09-22) 글자 폭 + 여백을 최소 폭으로."""
    item.setMinimumWidth(item.fontMetrics().horizontalAdvance(item.text()) + 22)


def band_chip(band: str) -> QtWidgets.QLabel:
    item = QtWidgets.QLabel(BAND_KO.get(band, band))
    item.setStyleSheet(band_style(band))
    item.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
    item.setSizePolicy(QtWidgets.QSizePolicy.Policy.Maximum,
                       QtWidgets.QSizePolicy.Policy.Fixed)
    _fit_chip(item)
    return item


def card(title: str = '') -> tuple[QtWidgets.QFrame, QtWidgets.QVBoxLayout]:
    """목업 .card — 패널색 배경 + 얇은 테두리. (프레임, 내부 세로 레이아웃)."""
    frame = QtWidgets.QFrame()
    frame.setObjectName('card')
    box = QtWidgets.QVBoxLayout(frame)
    box.setContentsMargins(16, 14, 16, 14)
    box.setSpacing(10)
    if title:
        box.addWidget(label(title, 'h3'))
    return frame, box


# ──────────────────────────── 단계 공통 2열 배치(본문 + 오른쪽 칸) ────────────────────────────
# 2026-09-23 사용자: "단계마다 가운데 영역과 오른쪽 영역의 크기가 계속 달라진다 — 깔끔하게 정렬".
# 전에는 단계마다 3:1, 3:2, 10:24, 최소 300·최대 400 이 제각각이라 단계를 넘길 때 오른쪽 칸이
# 흔들렸다. 이제 규칙은 하나다: 오른쪽 칸 폭 = 창(본문 영역) 폭만의 함수, 본문은 나머지를 채움,
# 두 칸의 위쪽은 제목 바로 아래 같은 y. 오른쪽 칸이 없는 단계(8)는 본문이 전체 폭.

#: 본문과 오른쪽 칸 사이 간격, 칸 안 카드 사이 간격(px).
COLUMN_GAP = 16
CARD_GAP = 12
#: 오른쪽 칸 폭 = 본문 영역 폭 × SIDE_RATIO 를 [SIDE_MIN, SIDE_MAX] 로 자른 값.
#: 1024 폭 창에서 SIDE_MIN(본문이 배치도 362 px 를 담을 수 있게), 넓어지면 SIDE_MAX 에서 멈춘다
#: (구성 단계의 스피커 배치도 340 px 카드가 들어가는 폭).
SIDE_MIN = 300
SIDE_MAX = 400
SIDE_RATIO = 0.4

_side_columns: list = []          # 살아 있는 SideColumn(창이 크기를 바꿀 때 한꺼번에 맞춘다)
_side_width = SIDE_MIN


def side_width_for(content_width: int) -> int:
    """본문 영역 폭(단계 페이지 안쪽, 여백 뺀 값) → 오른쪽 칸 폭."""
    return int(max(SIDE_MIN, min(SIDE_MAX, round(float(content_width) * SIDE_RATIO))))


class SideColumn(QtWidgets.QWidget):
    """오른쪽 칸. 폭은 모든 단계가 같은 값(`set_side_width`)이고, 카드는 `box` 에 위에서부터 쌓는다."""

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self.setObjectName('sideColumn')
        self.box = QtWidgets.QVBoxLayout(self)
        self.box.setContentsMargins(0, 0, 0, 0)
        self.box.setSpacing(CARD_GAP)
        self._stacked = False
        _side_columns.append(self)
        self.destroyed.connect(lambda *_a, me=id(self): _forget(me))
        self.apply_width(_side_width)

    def apply_width(self, width: int) -> None:
        if self._stacked:
            self.setMinimumWidth(0)
            self.setMaximumWidth(16777215)
        else:
            self.setFixedWidth(int(width))

    def set_stacked(self, on: bool) -> None:
        """좁아서 본문 아래로 쌓일 때(구성 단계) 폭 고정을 푼다."""
        self._stacked = bool(on)
        self.apply_width(_side_width)


def _forget(ident: int) -> None:
    _side_columns[:] = [column for column in _side_columns if id(column) != ident]


def set_side_width(content_width: int) -> int:
    """창이 크기를 바꿀 때 부른다 — 모든 단계의 오른쪽 칸을 같은 폭으로 맞추고 그 폭을 돌려준다."""
    global _side_width
    _side_width = side_width_for(content_width)
    for column in list(_side_columns):
        try:
            column.apply_width(_side_width)
        except RuntimeError:          # 이미 지워진 C++ 객체
            _forget(id(column))
    return _side_width


def two_columns(outer: QtWidgets.QBoxLayout, side: bool = True):
    """단계 제목 아래에 본문(+오른쪽 칸)을 놓는다. (배치 레이아웃, 본문 세로 레이아웃, 오른쪽 칸|None).

    본문 칸은 남는 폭을 모두 쓰고, 오른쪽 칸은 `SideColumn`(모든 단계 같은 폭). 두 칸 모두 위에서부터
    카드를 쌓는다. 배치 레이아웃은 QBoxLayout 이라 좁은 창에서 세로로 바꿀 수 있다(구성 단계).
    """
    body = QtWidgets.QBoxLayout(QtWidgets.QBoxLayout.Direction.LeftToRight)
    body.setSpacing(COLUMN_GAP)
    main_holder = QtWidgets.QWidget()
    main_holder.setObjectName('mainColumn')
    main = QtWidgets.QVBoxLayout(main_holder)
    main.setContentsMargins(0, 0, 0, 0)
    main.setSpacing(CARD_GAP)
    body.addWidget(main_holder, 1)
    column = None
    if side:
        column = SideColumn()
        body.addWidget(column, 0)
    outer.addLayout(body)
    return body, main, column


def add_heading(box: QtWidgets.QBoxLayout, title: str, lead: str = '') -> None:
    """목업 .head — 단계 제목(+ 있으면 한 줄 머리말)을 레이아웃에 바로 넣는다.

    09-22 부터 단계 설명은 화면에 깔지 않는다(순서는 [도움말](F1) 튜토리얼, 뜻은 풍선 도움말) — 머리말은 비운다.
    줄바꿈 라벨은 중간에 위젯을 한 겹 끼우면 높이(heightForWidth)가 전달되지
    않아 글이 겹친다. 그래서 라벨을 레이아웃에 직접 붙인다.
    """
    box.addWidget(label(title, 'h2'))
    if lead:
        lead_label = label(lead, 'lead', wrap=True)
        lead_label.setMaximumWidth(900)
        box.addWidget(lead_label)


def clear_layout(box: QtWidgets.QLayout) -> None:
    """레이아웃을 비운다. 부모에서 떼지 않으면 지워지기 전까지 유령으로 남는다."""
    while box.count():
        item = box.takeAt(0)
        widget = item.widget()
        if widget is not None:
            widget.setParent(None)
            widget.deleteLater()
            continue
        child = item.layout()
        if child is not None:
            clear_layout(child)


def help_box(text: str, bad: bool = False) -> QtWidgets.QLabel:
    """목업 .help — 안내 상자."""
    item = label(text, 'helpBad' if bad else 'help', wrap=True)
    return item


def seg(items, on_click, vertical: bool = False
        ) -> tuple[QtWidgets.QFrame, dict[str, QtWidgets.QPushButton]]:
    """목업 .seg 모양의 버튼 묶음을 만들고 클릭한 키를 on_click에 넘긴다.

    items는 (키, 문구) 쌍들이다. (프레임, 키별 버튼 사전)을 반환한다.
    선택 상태와 배타 선택 정책은 호출자가 관리한다. vertical=True이면 세로로 쌓는다.
    """
    frame = QtWidgets.QFrame()
    frame.setObjectName('seg')
    frame.setSizePolicy(QtWidgets.QSizePolicy.Policy.Preferred if vertical
                        else QtWidgets.QSizePolicy.Policy.Maximum,
                        QtWidgets.QSizePolicy.Policy.Fixed)
    box = QtWidgets.QVBoxLayout(frame) if vertical else QtWidgets.QHBoxLayout(frame)
    box.setContentsMargins(1, 1, 1, 1)
    box.setSpacing(0)
    buttons: dict[str, QtWidgets.QPushButton] = {}
    for key, text in items:
        button = QtWidgets.QPushButton(text)
        button.setObjectName('segBtn')
        button.clicked.connect(lambda _=False, k=key: on_click(k))
        box.addWidget(button)
        buttons[key] = button
    return frame, buttons


__all__ = ['COLORS', 'CHANNEL_COLORS', 'SUB_LINE_COLORS', 'SUB_LINE_WIDTH', 'channel_colors', 'is_sub_role',
           'BAND_COLORS', 'BAND_KO', 'QSS',
           'FONT_UI', 'FONT_BODY', 'FONT_MONO',
           'repolish', 'set_variant', 'set_state', 'band_style',
           'label', 'plain_label', 'ElidedLabel', 'ElidedPathLabel', 'path_label', 'fit_table_columns',
           'pill', 'chip', 'band_chip', 'card', 'add_heading', 'clear_layout',
           'help_box', 'seg']
