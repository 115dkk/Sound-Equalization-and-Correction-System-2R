"""F1 도움말 「mdat 불러오기」 탭의 그림 4장 만들기 (2026-10-10).

사용자 요청: "이걸 보는 사람들은 열심히 안 보고 대충대충 읽기 때문에 글로 어렵게 쓰면 안 돼. 최대한 사진 위주로."
그림 넷 — 키는 `tutorial_text.NAMES_CAPTIONS` 와 같다(계약 그림 27개와는 따로라 `make_tutorial_shots.KEYS` 에 넣지 않는다):

  mdat_names     REW 측정 목록처럼 그린 그림 (L R C SW1 / L_P2 R_P2 …) — QPainter 로 직접 그림
  mdat_dialog    mdat 를 고르면 뜨는 창 — 진짜 창(`MdatAssignDialog`)을 샘플 이름으로 열어 `grab()` 한 뒤 번호 표시만 얹음
  mdat_files     위치마다 파일을 따로 불러오는 예 (위치1.mdat → 위치 1, 위치2.mdat → 위치 2) — 직접 그림
  mdat_channels  채널 이름표 (프론트 … 서브) — 직접 그림. 표의 정본은 아래 `CHANNEL_ROWS`

    .venv-secs\\Scripts\\python.exe tools\\make_mdat_help_shots.py            # ko·en 전부
    .venv-secs\\Scripts\\python.exe tools\\make_mdat_help_shots.py --lang en

qa_capture · make_tutorial_shots 와 같은 원칙: 오프스크린 플랫폼 금지(글꼴이 없다) — 실제 windows 플랫폼에서 돌리되
창에는 `WA_DontShowOnScreen` 을 걸어 화면에 절대 뜨지 않는다. 출력은 `src/secs/wizard/assets/tutorial/<key>_<lang>.png`
+ `MANIFEST.json` 에 키 추가(다른 키는 건드리지 않는다). 그림 폭 680~700 px — 도움말 창(기본 640)에서 0.8 배쯤으로 줄어도 읽히게
글자를 크게(18~26 px) 잡았다.
"""
from __future__ import annotations

import argparse
import datetime as _dt
import json
import os
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
sys.path.insert(0, str(ROOT / 'tools'))
os.environ.pop('QT_QPA_PLATFORM', None)      # 오프스크린 금지(위 설명)

import make_tutorial_shots as base           # noqa: E402 - 경로 설정 뒤에 불러온다

OUT_DIR = base.OUT_DIR
KEYS = ('mdat_names', 'mdat_dialog', 'mdat_files', 'mdat_channels')

# ── 색(앱 팔레트 `style.COLORS` 와 같은 값) ──
INK, INK2, INK3 = '#1b2a29', '#4f6462', '#7a8e8c'
ACCENT, LINE, HEAD_BG, SEL_BG, PANEL = '#1e8c86', '#acbcba', '#e9efee', '#dff0ee', '#f2f6f5'
RED = '#D8352A'
SWATCH = ['#d8352a', '#2e6fd6', '#1f8a3f', '#a0258a', '#e08a00', '#00a3a3', '#7b5cd6', '#8a6d3b']

#: 채널 이름표 — (한국어 구분, 영어 구분, 이 이름을 쓰세요, 이것도 됩니다). 이름은 번역하지 않는다.
CHANNEL_ROWS = (
    ('프론트', 'Front', ['L', 'R', 'C'], 'FL · FR · FC · CEN'),
    ('사이드 서라운드', 'Side surround', ['Ls', 'Rs'], 'SL · SR'),
    ('리어 서라운드', 'Rear surround', ['Lrs', 'Rrs'], 'SBL · SBR · BL · BR'),
    ('와이드', 'Wide', ['Lw', 'Rw'], 'FWL · FWR'),
    ('탑', 'Top', ['Ltf', 'Rtf', 'Ltm', 'Rtm', 'Ltr', 'Rtr'], 'TFL · TFR · TML · TMR · TRL · TRR'),
    ('하이트', 'Height', ['Lfh', 'Rfh', 'Lrh', 'Rrh'], 'FHL · FHR · RHL · RHR'),
    ('서브', 'Subs', ['SW1', 'SW2', '…', 'SW8'], 'SUB · LFE · Sub2 · LFE2 …'),
)

TEXT = {
    'ko': {
        'list_head': 'REW 측정 목록', 'pos1': '위치 1', 'pos1_sub': ['채널 이름만'],
        'pos2': '위치 2', 'pos2_sub': ['채널 이름 + ', '_P2'],
        'file1': '위치1.mdat', 'file2': '위치2.mdat', 'button': 'mdat 불러오기',
        'legend_use': '이 이름을 쓰세요', 'legend_alt': '이것도 됩니다',
    },
    'en': {
        'list_head': 'REW measurements', 'pos1': 'Position 1', 'pos1_sub': ['channel name only'],
        'pos2': 'Position 2', 'pos2_sub': ['channel name + ', '_P2'],
        'file1': 'Position1.mdat', 'file2': 'Position2.mdat', 'button': 'Load mdat',
        'legend_use': 'Use this name', 'legend_alt': 'Also accepted',
    },
}


# ──────────────────────────── 그리기 도우미(QPainter) ────────────────────────────
def _qt():
    from PyQt6 import QtCore, QtGui

    return QtCore, QtGui


def make_font(px: int, bold: bool = False, latin: bool = False):
    """앱 글꼴(`style.FONT_UI` · `FONT_BODY`)과 같은 순서의 후보. latin = 이름·숫자는 Barlow 먼저."""
    _core, gui = _qt()
    font = gui.QFont()
    font.setFamilies((['Barlow', 'IBM Plex Sans KR', 'Malgun Gothic', 'Segoe UI'] if latin
                      else ['IBM Plex Sans KR', 'Malgun Gothic', 'Barlow', 'Segoe UI']))
    font.setPixelSize(px)
    font.setBold(bold)
    return font


def text_width(font, text: str) -> float:
    _core, gui = _qt()
    return gui.QFontMetricsF(font).horizontalAdvance(text)


def new_canvas(width: int, height: int):
    _core, gui = _qt()
    image = gui.QImage(width, height, gui.QImage.Format.Format_ARGB32)
    image.fill(gui.QColor('#ffffff'))
    painter = gui.QPainter(image)
    painter.setRenderHints(gui.QPainter.RenderHint.Antialiasing | gui.QPainter.RenderHint.TextAntialiasing
                           | gui.QPainter.RenderHint.SmoothPixmapTransform)
    return image, painter


def draw_text(painter, x: float, y_mid: float, text: str, font, color: str) -> float:
    """왼쪽 x, 세로 가운데 y_mid 에 한 줄. 끝 x 를 돌려준다."""
    core, gui = _qt()
    painter.setFont(font)
    painter.setPen(gui.QColor(color))
    width = text_width(font, text) + 4
    painter.drawText(core.QRectF(x, y_mid - 20, width, 40),
                     int(core.Qt.AlignmentFlag.AlignLeft | core.Qt.AlignmentFlag.AlignVCenter), text)
    return x + width - 4


def draw_segments(painter, x: float, y_mid: float, parts, font) -> float:
    """[(글자, 색), …] 를 이어서 그린다."""
    for text, color in parts:
        x = draw_text(painter, x, y_mid, text, font, color)
    return x


def rounded(painter, x, y, w, h, fill: str | None, border: str | None, radius: float = 6, width: float = 1.0) -> None:
    core, gui = _qt()
    painter.setBrush(gui.QColor(fill) if fill else core.Qt.BrushStyle.NoBrush)
    painter.setPen(gui.QPen(gui.QColor(border), width) if border else core.Qt.PenStyle.NoPen)
    painter.drawRoundedRect(core.QRectF(x, y, w, h), radius, radius)


def chip(painter, x: float, y: float, parts, font, h: float = 30, pad: float = 10,
         fill: str = SEL_BG, border: str = ACCENT) -> float:
    """이름 칩 하나. parts = [(글자, 색), …]. 오른쪽 끝 x 를 돌려준다."""
    width = sum(text_width(font, text) for text, _c in parts) + 2 * pad
    rounded(painter, x, y, width, h, fill, border, radius=6, width=1.2)
    draw_segments(painter, x + pad, y + h / 2, parts, font)
    return x + width


def arrow(painter, x0: float, x1: float, y: float, color: str = INK2) -> None:
    core, gui = _qt()
    pen = gui.QPen(gui.QColor(color), 2.4)
    pen.setCapStyle(core.Qt.PenCapStyle.RoundCap)
    painter.setPen(pen)
    painter.drawLine(core.QPointF(x0, y), core.QPointF(x1 - 4, y))
    painter.setBrush(gui.QColor(color))
    painter.drawPolygon(gui.QPolygonF([core.QPointF(x1, y), core.QPointF(x1 - 12, y - 7), core.QPointF(x1 - 12, y + 7)]))


def mark(painter, rect, number: int) -> list[float]:
    """빨간 둥근 테두리 + 번호 원(`make_tutorial_shots.draw_marks` 와 같은 모양). 사각형 [x, y, w, h] 를 돌려준다."""
    core, gui = _qt()
    x, y, w, h = rect
    painter.setBrush(core.Qt.BrushStyle.NoBrush)
    painter.setPen(gui.QPen(gui.QColor(RED), 3))
    painter.drawRoundedRect(core.QRectF(x, y, w, h), 7, 7)
    radius = 11
    cx, cy = max(x - 4, radius + 1), max(y - 4, radius + 1)
    painter.setPen(core.Qt.PenStyle.NoPen)
    painter.setBrush(gui.QColor('#ffffff'))
    painter.drawEllipse(core.QPointF(cx, cy), radius + 1, radius + 1)
    painter.setBrush(gui.QColor(RED))
    painter.drawEllipse(core.QPointF(cx, cy), radius, radius)
    painter.setFont(make_font(14, bold=True, latin=True))
    painter.setPen(gui.QColor('#ffffff'))
    painter.drawText(core.QRectF(cx - radius, cy - radius, 2 * radius, 2 * radius),
                     int(core.Qt.AlignmentFlag.AlignCenter), str(number))
    return [float(x), float(y), float(w), float(h)]


def frame(painter, width: int, height: int) -> None:
    rounded(painter, 0.5, 0.5, width - 1, height - 1, None, LINE, radius=4, width=1)


def to_pil(image):
    """QImage → Pillow RGB."""
    from PIL import Image

    _core, gui = _qt()
    rgb = image.convertToFormat(gui.QImage.Format.Format_RGB888)
    return Image.frombuffer('RGB', (rgb.width(), rgb.height()), bytes(rgb.constBits().asstring(rgb.sizeInBytes())),
                            'raw', 'RGB', rgb.bytesPerLine(), 1)


# ──────────────────────────── 그림 1 · REW 측정 목록 ────────────────────────────
def draw_names(lang: str):
    text = TEXT[lang]
    width, row_h, gap = 680, 38, 14
    top = 16 + 36 + 10
    height = top + 8 * row_h + gap + 10 + 16
    image, p = new_canvas(width, height)
    panel_x, panel_w = 16, 300
    rounded(p, panel_x, 16, panel_w, height - 32, '#ffffff', LINE, radius=5, width=1.2)
    rounded(p, panel_x + 1, 17, panel_w - 2, 35, HEAD_BG, None, radius=4)
    draw_text(p, panel_x + 14, 16 + 18, text['list_head'], make_font(17, bold=True), INK)
    names = ['L', 'R', 'C', 'SW1', 'L_P2', 'R_P2', 'C_P2', 'SW1_P2']
    name_font = make_font(21, bold=True, latin=True)
    group_top: list[float] = []
    for index, name in enumerate(names):
        y = top + index * row_h + (gap if index >= 4 else 0)
        if index in (0, 4):
            group_top.append(y)
        if index % 2 == 1 and False:
            rounded(p, panel_x + 6, y, panel_w - 12, row_h, '#f7faf9', None, radius=3)
        rounded(p, panel_x + 18, y + row_h / 2 - 8, 16, 16, SWATCH[index], None, radius=3)
        body, _sep, tail = name.partition('_')
        parts = [(body, INK)] + ([('_' + tail, ACCENT)] if tail else [])
        draw_segments(p, panel_x + 46, y + row_h / 2, parts, name_font)
    marks = []
    for number, y0 in enumerate(group_top, 1):
        rect = [panel_x + 8, y0 - 2, panel_w - 16, 4 * row_h + 4]
        marks.append(mark(p, rect, number))
    # 오른쪽: 묶음마다 "위치 N" 큰 글씨 + 한 줄
    big, small = make_font(27, bold=True), make_font(20)
    for index, (y0, label, sub) in enumerate(zip(group_top, (text['pos1'], text['pos2']),
                                                  (text['pos1_sub'], text['pos2_sub']))):
        mid = y0 + 2 * row_h
        bracket_x = panel_x + panel_w + 14
        p.setPen(_qt()[1].QPen(_qt()[1].QColor(LINE), 2.4))
        p.drawLine(_qt()[0].QPointF(bracket_x, y0 + 2), _qt()[0].QPointF(bracket_x, y0 + 4 * row_h - 4))
        p.drawLine(_qt()[0].QPointF(bracket_x - 8, y0 + 2), _qt()[0].QPointF(bracket_x, y0 + 2))
        p.drawLine(_qt()[0].QPointF(bracket_x - 8, y0 + 4 * row_h - 4), _qt()[0].QPointF(bracket_x, y0 + 4 * row_h - 4))
        draw_text(p, bracket_x + 22, mid - 18, label, big, INK)
        parts = [(sub[0], INK2)] + ([(sub[1], ACCENT)] if len(sub) > 1 else [])
        draw_segments(p, bracket_x + 22, mid + 18, parts, make_font(20, bold=len(sub) > 1, latin=True))
        del small, index
        small = make_font(20)
    frame(p, width, height)
    p.end()
    return image, marks


# ──────────────────────────── 그림 3 · 위치마다 파일 따로 ────────────────────────────
def draw_files(lang: str):
    text = TEXT[lang]
    core, gui = _qt()
    width, card_h, gap = 700, 108, 14
    height = 16 + card_h + gap + card_h + 16
    image, p = new_canvas(width, height)
    chip_font = make_font(18, bold=True, latin=True)
    name_font = make_font(21, bold=True)
    pill_font = make_font(16, bold=True)
    marks = []
    for index, (file_name, chips, result) in enumerate((
            (text['file1'], ['L', 'R', 'C', 'SW1'], text['pos1']),
            (text['file2'], ['L_P2', 'R_P2', 'C_P2', 'SW1_P2'], text['pos2']))):
        y = 16 + index * (card_h + gap)
        rounded(p, 16, y, width - 32, card_h, '#f7faf9', LINE, radius=8, width=1.2)
        # 파일 아이콘(접힌 모서리)
        ix, iy = 36, y + 15
        path = gui.QPainterPath()
        path.moveTo(ix, iy)
        path.lineTo(ix + 14, iy)
        path.lineTo(ix + 24, iy + 10)
        path.lineTo(ix + 24, iy + 30)
        path.lineTo(ix, iy + 30)
        path.closeSubpath()
        p.setBrush(gui.QColor('#ffffff'))
        p.setPen(gui.QPen(gui.QColor(INK2), 1.8))
        p.drawPath(path)
        p.drawLine(core.QPointF(ix + 14, iy), core.QPointF(ix + 14, iy + 10))
        p.drawLine(core.QPointF(ix + 14, iy + 10), core.QPointF(ix + 24, iy + 10))
        draw_text(p, ix + 38, iy + 15, file_name, name_font, INK)
        # 이름 칩
        cx = 36
        for name in chips:
            body, _sep, tail = name.partition('_')
            parts = [(body, INK)] + ([('_' + tail, ACCENT)] if tail else [])
            cx = chip(p, cx, y + 60, parts, chip_font, h=32) + 8
        draw_text(p, cx, y + 76, '…', make_font(22, bold=True), INK3)
        # 화살표 + [mdat 불러오기] 버튼 + 결과 상자
        mid = y + card_h / 2
        arrow(p, 384, 550, mid)
        pill_w = max(124.0, text_width(pill_font, text['button']) + 26)
        pill_x = 474 - pill_w / 2
        rounded(p, pill_x, mid - 17, pill_w, 34, '#ffffff', INK2, radius=6, width=1.6)
        draw_text(p, pill_x + (pill_w - text_width(pill_font, text['button'])) / 2, mid, text['button'], pill_font, INK)
        box_x, box_w = 558, 112
        rounded(p, box_x, mid - 28, box_w, 56, ACCENT, None, radius=8)
        label_font = make_font(21 if lang == 'ko' else 19, bold=True)
        draw_text(p, box_x + (box_w - text_width(label_font, result)) / 2, mid, result, label_font, '#ffffff')
        marks.append([pill_x - 5, mid - 22, pill_w + 10, 44])
    for number, rect in enumerate(marks, 1):
        mark(p, rect, number)
    frame(p, width, height)
    p.end()
    return image, marks


# ──────────────────────────── 그림 4 · 채널 이름표 ────────────────────────────
def draw_channels(lang: str):
    text = TEXT[lang]
    width, row_h = 680, 62
    top = 58
    height = top + len(CHANNEL_ROWS) * row_h + 14
    image, p = new_canvas(width, height)
    chip_font = make_font(18, bold=True, latin=True)
    alt_font = make_font(16, latin=True)
    group_font = make_font(19, bold=True)
    # 위쪽 범례 한 줄: 칩 = 이 이름을 쓰세요, 회색 = 이것도 됩니다
    x = chip(p, 28, 12, [('Ltf', INK)], chip_font, h=30) + 10
    x = draw_text(p, x, 27, text['legend_use'], make_font(17), INK2) + 34
    x = draw_text(p, x, 27, 'TFL', make_font(17, latin=True), INK3) + 10
    draw_text(p, x, 27, text['legend_alt'], make_font(17), INK2)
    p.setPen(_qt()[1].QPen(_qt()[1].QColor(LINE), 1.2))
    for index, (ko, en, mains, alt) in enumerate(CHANNEL_ROWS):
        y = top + index * row_h
        if index == 0:
            p.drawLine(_qt()[0].QPointF(16, y - 4), _qt()[0].QPointF(width - 16, y - 4))
        p.setPen(_qt()[1].QPen(_qt()[1].QColor(LINE), 1.2))
        p.drawLine(_qt()[0].QPointF(16, y + row_h - 4), _qt()[0].QPointF(width - 16, y + row_h - 4))
        draw_text(p, 28, y + 28, ko if lang == 'ko' else en, group_font, INK)
        cx = 214
        for name in mains:
            if name == '…':
                draw_text(p, cx + 2, y + 22, '…', make_font(22, bold=True), INK3)
                cx += 28
                continue
            cx = chip(p, cx, y + 7, [(name, INK)], chip_font, h=30) + 8
        draw_text(p, 218, y + 49, alt, alt_font, INK2)
    frame(p, width, height)
    p.end()
    return image, []


# ──────────────────────────── 그림 2 · 진짜 창 ────────────────────────────
#: 창에 넣는 샘플 측정 이름과 서브로 보이는지. 마지막 둘(L EQ · DBA L)은 일부러 못 알아보는 이름.
DIALOG_TITLES = ('L', 'R', 'C', 'Ls', 'Rs', 'SW1', 'L EQ', 'DBA L')
DIALOG_SUB = (False, False, False, False, False, True, False, False)


def capture_dialog(shooter):
    """진짜 `MdatAssignDialog` 를 샘플 이름으로 열어 잡는다. (QImage, 강조 사각형 4개)."""
    core, _gui = _qt()
    from PyQt6 import QtWidgets

    from secs.wizard.step5_measure import MdatAssignDialog, auto_assign

    win = shooter.win
    plan = win.measure_page.plan()
    initial = auto_assign(list(DIALOG_TITLES), plan.roles, list(DIALOG_SUB))
    details = ['저역만(서브로 보임)' if sub else '전대역' for sub in DIALOG_SUB]
    # 실제 창처럼 위치 칸까지(10-11: 줄마다 위치를 고른다) — 샘플은 모두 위치 1, 배치 9곳
    dialog = MdatAssignDialog(list(DIALOG_TITLES), plan.roles, initial, details, win,
                              positions=[1] * len(DIALOG_TITLES), position_count=9)
    dialog.setAttribute(core.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    # 실제 창은 목록 칸 최소 높이가 8 줄 분이라 8 줄이면 아래가 비어 보인다 — 그림에서는 줄 수만큼으로 줄인다.
    scroll = dialog.findChildren(QtWidgets.QScrollArea)[0]
    scroll.setMinimumHeight(0)
    scroll.widget().adjustSize()
    scroll.setMinimumHeight(scroll.widget().sizeHint().height() + 4)
    dialog.resize(max(680, dialog.minimumWidth()), dialog.sizeHint().height())
    dialog.show()
    shooter.settle()
    holder = dialog.findChildren(QtWidgets.QScrollArea)[0].widget()
    grid = holder.layout()

    def rect_of(widget):
        top_left = widget.mapTo(dialog, core.QPoint(0, 0))
        return core.QRect(top_left, widget.size())

    labels = [grid.itemAtPosition(row, 0).widget() for row in range(len(DIALOG_TITLES))]
    names = [rect_of(label) for label in labels]
    text_w = max(label.fontMetrics().horizontalAdvance(label.text()) for label in labels)   # 칸은 넓어도 글자 폭만 표시
    combos = [rect_of(combo) for combo in dialog.combos]
    matched = [i for i, role in enumerate(initial) if role]
    unmatched = [i for i, role in enumerate(initial) if not role]
    print('  창 자동 채움:', dict(zip(DIALOG_TITLES, initial)))
    ok = rect_of(dialog.buttons.button(QtWidgets.QDialogButtonBox.StandardButton.Ok))
    first_name = names[0]
    name_col = core.QRect(first_name.x() - 4, names[0].y() - 2, text_w + 16,
                          names[-1].bottom() - names[0].y() + 5)

    def union(rects, pad=2):
        out = rects[0]
        for r in rects[1:]:
            out = out.united(r)
        return out.adjusted(-pad, -pad, pad, pad)

    rects = [name_col, union([combos[i] for i in matched]), union([combos[i] for i in unmatched]), ok.adjusted(-3, -3, 3, 3)]
    pixmap = dialog.grab()
    image = pixmap.toImage()
    logical_w = dialog.width()
    dialog.close()
    # 화면 배율이 2 배인 PC 에서는 grab 이 물리 화소로 나온다 — 논리 폭의 1.5 배(≤ 900)로 고르게 맞춘다.
    target = min(base.MAX_WIDTH, round(logical_w * 1.5))
    image = image.scaledToWidth(target, core.Qt.TransformationMode.SmoothTransformation)
    image.setDevicePixelRatio(1.0)          # 배율이 남으면 QPainter 가 좌표를 또 2 배로 그린다
    factor = target / logical_w
    return image, [[r.x() * factor, r.y() * factor, r.width() * factor, r.height() * factor] for r in rects]


# ──────────────────────────── 저장 · MANIFEST ────────────────────────────
def save(image, path: Path):
    from PIL import ImageDraw  # noqa: F401 - Pillow 가 있는지 먼저 확인

    pil = to_pil(image)
    size = base.save_png(pil, path)
    return size, pil.size


def run_language(app, lang: str, out: Path, tmp: Path, keys: set[str]) -> dict:
    """언어 하나의 그림을 만든다. {키: MANIFEST 항목(언어별)}."""
    entries = {}
    jobs = {'mdat_names': draw_names, 'mdat_files': draw_files, 'mdat_channels': draw_channels}
    for key in KEYS:
        if key not in keys:
            continue
        if key == 'mdat_dialog':
            continue
        image, marks = jobs[key](lang)
        name = f'{key}_{lang}.png'
        size, dims = save(image, out / name)
        entries[key] = {'file': name, 'crop': [0, 0, dims[0], dims[1]],
                        'marks': [[round(v) for v in m] for m in marks], 'size': list(dims), 'bytes': size, 'drawn': True}
        print(f'  {name:26s} {dims[0]}x{dims[1]}  {size / 1024:6.1f} KB  marks={len(marks)}')
    if 'mdat_dialog' in keys:
        shooter = base.Shooter(app, lang, set(), out, tmp)
        shooter.build()
        try:
            image, marks = capture_dialog(shooter)
        finally:
            shooter.win.close()
        core, gui = _qt()
        painter = gui.QPainter(image)
        painter.setRenderHints(gui.QPainter.RenderHint.Antialiasing | gui.QPainter.RenderHint.TextAntialiasing)
        for number, rect in enumerate(marks, 1):
            mark(painter, rect, number)
        frame(painter, image.width(), image.height())
        painter.end()
        name = f'mdat_dialog_{lang}.png'
        size, dims = save(image, out / name)
        entries['mdat_dialog'] = {'file': name, 'crop': [0, 0, dims[0], dims[1]],
                                  'marks': [[round(v) for v in m] for m in marks], 'size': list(dims), 'bytes': size}
        print(f'  {name:26s} {dims[0]}x{dims[1]}  {size / 1024:6.1f} KB  marks={len(marks)}')
    return entries


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--lang', default='ko,en')
    ap.add_argument('--keys', default='', help='쉼표 목록(없으면 넷 다)')
    ap.add_argument('--out', default='', help=f'출력 폴더(기본 {OUT_DIR.relative_to(ROOT)})')
    args = ap.parse_args(argv)
    keys = {k.strip() for k in args.keys.split(',') if k.strip()} or set(KEYS)
    unknown = keys - set(KEYS)
    if unknown:
        ap.error('모르는 키: ' + ', '.join(sorted(unknown)))
    out = Path(args.out) if args.out else OUT_DIR
    out.mkdir(parents=True, exist_ok=True)

    from PyQt6.QtWidgets import QApplication

    os.environ['SECS4_SETTINGS_PATH'] = str(Path(tempfile.mkdtemp(prefix='secs-mdat-')) / 'settings.ini')
    app = QApplication.instance() or QApplication([])
    from secs.i18n import language_manager

    manifest_path = out / 'MANIFEST.json'
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    shots = manifest['shots']
    for lang in [s.strip() for s in args.lang.split(',') if s.strip()]:
        print(f'[{lang}]')
        language_manager().set_language(lang, persist=False)
        with tempfile.TemporaryDirectory(prefix='secs-mdat-') as tmp:
            entries = run_language(app, lang, out, Path(tmp), keys)
        for key, entry in entries.items():
            old = shots.get(key, {})
            by_lang = {**old.get('by_lang', {}), lang: {k: v for k, v in entry.items() if k != 'file'}}
            files = {code: old[code] for code in ('ko', 'en') if code in old}
            files[lang] = entry['file']
            shots[key] = {**{code: files[code] for code in ('ko', 'en') if code in files},
                          'crop': entry['crop'], 'marks': entry['marks'], 'by_lang': by_lang}
    manifest['generated'] = f'{_dt.date.today():%Y-%m-%d}'
    manifest_path.write_text(base._manifest_text(manifest), encoding='utf-8')
    return 0


if __name__ == '__main__':
    sys.exit(main())
