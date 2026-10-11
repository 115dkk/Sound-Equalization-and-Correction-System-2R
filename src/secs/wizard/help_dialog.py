# -*- coding: utf-8 -*-
"""도움말 창(F1) — 현재 단계의 **따라 하기 튜토리얼**을 화면 그림과 함께 보여 준다(2026-09-24).

누가 부름: `WizardWindow.show_help()`(사이드바 [도움말] · F1)가 하나를 만들어 두고 단계가 바뀌면
`set_step()` 으로 내용만 바꾼다. 모달이 아니라 열어 둔 채로 화면을 만질 수 있다. `set_step(0)` 은
'처음이라면'(샘플 데이터로 1→7 한 바퀴)만 그린다.
입력: `tutorial_text`(문안 · 그림 캡션)와 `assets/tutorial/<key>_<ko|en>.png`(+ `MANIFEST.json`,
`tools/make_tutorial_shots.py` 가 만든다). 출력: 창 하나. 문구는 그릴 때 `t()` 로 옮기므로 언어를
바꾸면 `set_step()` 을 다시 부르면 된다(창이 `_sync_help()` 로 부른다).

사용자 요청(09-24): "기능 설명은 커서를 대는 풍선 도움말로 대체하고, 도움말(F1)은 튜토리얼에 가깝게,
사진을 최대한 많이." 계약은 `문서/워커 과제/도움말 튜토리얼 계약 2026-09-24.md` 3절. 기능 하나하나의
뜻은 풍선 도움말(`tooltips.TIPS`)로 갔고 이 창은 순서만 안내한다(옛 기능 설명표 `help_text` 는 10-10 에 지움).

탭 = 따라 하기 / 확인할 것 / 문제 해결 / 실제 REW(1단계에만 맨 앞에 '처음이라면') + 그림 위주 덧붙임 탭(5단계 'mdat 불러오기',
내보내기 'CamillaDSP' — `tut.EXTRA_TABS` · `tut.PICTURE_TABS`, 10-10). 탭마다 문서를 새로
그린다 — 검색(Ctrl+F)은 지금 탭 안에서만 찾는다.

09-24 가독성 재작업에서 배운 것을 그대로 쓴다:
- **줄 간격은 CSS 대신 `QTextBlockFormat.setLineHeight(px, FixedHeight)`** 로 블록마다 직접 건다
  (`_apply_metrics`). CSS `line-height` 는 Qt 가 글꼴 기본 줄높이(맑은 고딕 1.33em)에 곱해 실제 1.93배가
  됐다. 단 **그림 블록은 고정 줄높이를 걸지 않는다** — 고정하면 그림 줄이 22 px 로 잘려 아래 글과 겹친다.
- **낱말 단위 줄바꿈**: 어절마다 `<span style="white-space:nowrap">`(`_keepall`). 긴 영문 토큰(URL 등,
  한글 없이 18자 초과)은 감싸지 않아 창보다 길면 정상적으로 접힌다.
- 탭 줄은 창보다 넓으면 탭을 줄이지 않고 가로로 스크롤한다(`_TabStrip`).
- style 속성은 홑따옴표 — `FONT_UI`·`FONT_BODY` 가 겹따옴표로 감싼 글꼴 목록이라 style="…" 안에 넣으면
  태그가 깨져 본문이 통째로 빈다.

그림: `QTextDocument.addResource(ImageResource, QUrl('tut:<key>'), QImage)` 로 등록하고 `<img>` 로 건다.
폭 = min(원본 폭, 뷰포트 폭 − 여백). 창 크기가 바뀌면 120 ms 디바운스 뒤 다시 그리고 스크롤 위치(비율)를
지킨다. 그림을 누르면(`<a href="tut:<key>">` → `anchorClicked`) 큰 보기 창(`ImageViewer`, 화면 90 % 안에
맞춤, Esc·클릭으로 닫힘)이 뜬다. 그림 파일이 없으면 회색 자리 상자를 그려 넣는다(예외 없음).
"""
from __future__ import annotations

import html
import json
import re
import weakref
from pathlib import Path

from PyQt6 import QtCore, QtGui, QtWidgets

from secs.i18n import language_manager
from secs.wizard import tutorial_text as tut
from secs.wizard.branding import ASSETS
from secs.wizard.strings import PAGE_TITLES, STEPS, bar_position, t
from secs.wizard.style import COLORS, FONT_BODY, FONT_UI, seg, set_state, set_variant
from secs.wizard.tooltips import attach

#: 튜토리얼 그림 폴더(exe 에는 `SECS.spec` datas 로 같은 상대 경로에 들어간다).
TUTORIAL_DIR = ASSETS / 'tutorial'

# ──── 규격(px) — (줄 간격 | None = 고정 안 함, 위 여백, 아래 여백, 왼쪽 들여쓰기) ────
_GOAL = (22, 0, 8, 0)
_HINT = (20, 0, 14, 0)
_ACTION = (22, 16, 2, 0)      # "1  … 를 누릅니다."
_SEE = (22, 2, 8, 22)         # "→ … 가 보이면 다음"
_IMAGE = (None, 4, 4, 22)     # 그림 블록 — 고정 줄높이 금지(위 docstring)
_CAP_TITLE = (18, 0, 2, 22)
_CAP_ITEM = (18, 0, 0, 22)
_CAP_LAST = (18, 0, 6, 22)
_ITEM = (22, 10, 0, 0)        # 확인할 것 · 실제 REW 한 줄
_SYMPTOM = (22, 16, 2, 0)
_FIX = (22, 0, 4, 18)
_HEAD = (22, 18, 6, 0)
_NOTE = (20, 18, 0, 0)
_START = (24, 0, 2, 0)        # mdat 불러오기 탭 맨 위 한 줄
_SECTION = (22, 20, 4, 0)     # 구역 머리말
_START_BODY = (22, 0, 6, 0)
_LINE = (22, 2, 4, 22)        # 번호 머리말 아래 짧은 줄(그림과 같은 들여쓰기)
_ROW = (22, 3, 0, 22)
_NCAP_TITLE = (22, 8, 3, 22)  # mdat 불러오기 탭 그림 제목 · 번호 설명(본문 크기)
_NCAP_ITEM = (22, 0, 0, 22)
_NCAP_LAST = (22, 0, 4, 22)

_IMAGE_SIDE_MARGIN = 22 + 16  # 들여쓰기 + 문서 여백 · 스크롤바 여유
_CIRCLED = '①②③④⑤⑥⑦⑧⑨⑩'


# ──────────────────────────── 한국어 낱말 단위 줄바꿈(keep-all) ────────────────────────────
_WS_RE = re.compile(r'(\s+)')
_HANGUL_RE = re.compile(r'[가-힣]')
_LONG_TOKEN = 18  # 이보다 긴 한글 없는 토큰(URL 등)은 감싸지 않아 넘치지 않고 접힌다


def _keepall(text: str) -> str:
    """어절마다 `white-space:nowrap` 으로 감싸 낱말 안에서 줄이 끊기지 않게 한다."""
    out = []
    for piece in _WS_RE.split(text):
        if not piece:
            continue
        if piece.isspace():
            out.append(piece)
            continue
        esc = html.escape(piece)
        if len(piece) > _LONG_TOKEN and not _HANGUL_RE.search(piece):
            out.append(esc)
        else:
            out.append(f'<span style="white-space:nowrap">{esc}</span>')
    return ''.join(out)


def _apply_metrics(document: QtGui.QTextDocument, specs: list[tuple]) -> None:
    """블록마다 줄 간격·문단 간격·들여쓰기를 px 로 직접 건다(줄 간격 None = 그림 블록, 고정 안 함).

    CSS `line-height` 는 Qt 리치 텍스트에서 글꼴 기본 줄높이에 곱해져 믿을 수 없다(2026-09-23 UI 감사 A2).
    `QTextBlockFormat.setLineHeight(px, FixedHeight)` 만 실제 px 를 지킨다.
    """
    fixed = QtGui.QTextBlockFormat.LineHeightTypes.FixedHeight.value
    proportional = QtGui.QTextBlockFormat.LineHeightTypes.ProportionalHeight.value
    cursor = QtGui.QTextCursor(document)
    block = document.begin()
    for lh, sb, sa, indent in specs:
        if not block.isValid():
            break
        fmt = QtGui.QTextBlockFormat()
        if lh is None:
            fmt.setLineHeight(100.0, proportional)
        else:
            fmt.setLineHeight(float(lh), fixed)
        fmt.setTopMargin(float(sb))
        fmt.setBottomMargin(float(sa))
        fmt.setLeftMargin(float(indent))
        cursor.setPosition(block.position())
        cursor.mergeBlockFormat(fmt)
        block = block.next()


# ──────────────────────────── 그림 ────────────────────────────
_MANIFEST: dict | None = None
_ORIGINALS: dict[tuple[str, str], QtGui.QImage | None] = {}


def _manifest() -> dict:
    global _MANIFEST
    if _MANIFEST is None:
        try:
            _MANIFEST = json.loads((TUTORIAL_DIR / 'MANIFEST.json').read_text(encoding='utf-8'))
        except (OSError, ValueError):
            _MANIFEST = {}
    return _MANIFEST


def _language() -> str:
    return 'en' if language_manager().language == 'en' else 'ko'


def image_path(key: str, lang: str | None = None) -> Path | None:
    """그림 키 → 파일 경로(MANIFEST 에 없거나 'missing' 이거나 파일이 없으면 None)."""
    lang = lang or _language()
    manifest = _manifest()
    if key in (manifest.get('missing') or {}):
        return None
    entry = (manifest.get('shots') or {}).get(key) or {}
    name = entry.get(lang) or f'{key}_{lang}.png'
    path = TUTORIAL_DIR / name
    return path if path.is_file() else None


def original_image(key: str, lang: str | None = None) -> QtGui.QImage | None:
    """원본 그림(프로세스에서 한 번만 읽는다). 없거나 못 읽으면 None."""
    lang = lang or _language()
    cache_key = (key, lang)
    if cache_key not in _ORIGINALS:
        path = image_path(key, lang)
        image = QtGui.QImage(str(path)) if path is not None else None
        _ORIGINALS[cache_key] = image if image is not None and not image.isNull() else None
    return _ORIGINALS[cache_key]


def placeholder_image(width: int, key: str) -> QtGui.QImage:
    """그림이 없을 때 쓰는 회색 자리 상자(글자: '그림 준비 중' + 키)."""
    width = max(120, int(width))
    height = max(90, width * 9 // 16)
    image = QtGui.QImage(width, height, QtGui.QImage.Format.Format_ARGB32)
    image.fill(QtGui.QColor(COLORS['head_bg']))
    painter = QtGui.QPainter(image)
    try:
        painter.setPen(QtGui.QPen(QtGui.QColor(COLORS['line']), 2, QtCore.Qt.PenStyle.DashLine))
        painter.drawRect(1, 1, width - 3, height - 3)
        painter.setPen(QtGui.QColor(COLORS['ink3']))
        font = painter.font()
        font.setPixelSize(14)
        painter.setFont(font)
        painter.drawText(QtCore.QRect(0, 0, width, height), int(QtCore.Qt.AlignmentFlag.AlignCenter),
                         f'{t("그림 준비 중")}\n{key}')
    finally:
        painter.end()
    return image


def display_image(key: str, width: int, dpr: float = 1.0) -> tuple[QtGui.QImage, int, int, bool]:
    """(표시용 그림, 표시 폭, 표시 높이, 실제 그림인지). 표시 폭 = min(원본 폭, width).

    화면 배율(dpr)만큼 큰 그림을 미리 부드럽게 줄여 둔다 — Qt 문서가 그림을 늘이고 줄이면 계단이 진다.
    """
    original = original_image(key)
    if original is None:
        image = placeholder_image(width, key)
        return image, image.width(), image.height(), False
    shown_w = max(80, min(original.width(), int(width)))
    shown_h = max(1, round(original.height() * shown_w / original.width()))
    pixels_w = min(original.width(), max(1, round(shown_w * dpr)))
    if pixels_w == original.width():
        image = original
    else:
        image = original.scaledToWidth(pixels_w, QtCore.Qt.TransformationMode.SmoothTransformation)
    return image, shown_w, shown_h, True


# ──────────────────────────── 탭 내용 → HTML ────────────────────────────
class _Page:
    """한 탭의 HTML 조각 · 블록 규격 · 그림 목록을 모은다(블록 하나 = `<p>` 하나)."""

    def __init__(self, width: int, dpr: float) -> None:
        self.width = width
        self.dpr = dpr
        self.parts: list[str] = []
        self.specs: list[tuple] = []
        self.images: dict[str, QtGui.QImage] = {}
        ink, ink2, ink3 = COLORS['ink'], COLORS['ink2'], COLORS['ink3']
        self.body = f'font-family:{FONT_BODY}; font-size:14px; color:{ink};'
        self.muted = f'font-family:{FONT_BODY}; font-size:13px; color:{ink2};'
        self.small = f'font-family:{FONT_BODY}; font-size:12px; color:{ink2};'
        self.small_title = f'font-family:{FONT_UI}; font-weight:600; font-size:12px; color:{ink3};'
        self.strong = f'font-family:{FONT_UI}; font-weight:600; font-size:14px; color:{ink};'
        self.head = f'font-family:{FONT_UI}; font-weight:600; font-size:15px; color:{ink2};'
        self.number = f'font-family:{FONT_UI}; font-weight:700; color:{COLORS["accent"]};'
        self.lead = f'font-family:{FONT_UI}; font-weight:700; font-size:17px; color:{COLORS["accent"]};'
        self.section = f'font-family:{FONT_UI}; font-weight:700; font-size:15px; color:{COLORS["ink"]};'
        self.red = f'font-family:{FONT_UI}; font-weight:700; color:#D8352A;'     # 그림 속 빨간 번호와 같은 색

    def p(self, inner: str, style: str, spec: tuple) -> None:
        self.parts.append(f"<p style='{style}'>{inner}</p>")
        self.specs.append(spec)

    def text(self, text: str, style: str, spec: tuple) -> None:
        self.p(_keepall(str(t(text))), style, spec)

    def image(self, key: str, big: bool = False) -> None:
        """그림 + 캡션(그림 제목 · 번호 설명). 같은 그림이 두 번 나와도 자원은 하나.

        big = 본문 크기 캡션(mdat 불러오기 탭 — 대충 읽는 사람도 번호 뜻을 놓치지 않게)."""
        image, shown_w, shown_h, _real = display_image(key, self.width, self.dpr)
        self.images[key] = image
        tip = html.escape(str(t('그림을 누르면 크게 봅니다.')))
        self.p(f"<a href='tut:{key}'><img src='tut:{key}' width='{shown_w}' height='{shown_h}' "
               f"title='{tip}'></a>", self.body, _IMAGE)
        title, legend = tut.caption_of(key)
        if title:
            if big:
                self.text(title, self.strong, _NCAP_TITLE if legend else _NCAP_LAST)
            else:
                self.text(title, self.small_title, _CAP_TITLE if legend else _CAP_LAST)
        for i, item in enumerate(legend):
            mark = _CIRCLED[i] if i < len(_CIRCLED) else f'({i + 1})'
            last = i == len(legend) - 1
            if big:
                self.p(f"<span style='{self.red}'>{mark}</span> {_keepall(str(t(item)))}", self.body,
                       _NCAP_LAST if last else _NCAP_ITEM)
            else:
                self.p(f'{mark} {_keepall(str(t(item)))}', self.small, _CAP_LAST if last else _CAP_ITEM)

    def html(self) -> str:
        return f"<div style='{self.body}'>{''.join(self.parts)}</div>"


def _intro_page(page: _Page) -> None:
    intro = tut.INTRO
    page.text(intro['title'], page.head, (22, 0, 8, 0))
    for line in intro['body']:
        page.text(line, page.body, (22, 0, 8, 0))
    page.image(intro['img'])
    page.text('샘플 데이터로 1→7 한 번에', page.head, _HEAD)
    for line in intro['route']:
        text = str(t(line))
        number, _sep, rest = text.partition(' ')
        if number.isdigit():
            page.p(f"<span style='{page.number}'>{html.escape(number)}</span>&nbsp;&nbsp;{_keepall(rest)}",
                   page.body, (22, 6, 0, 0))
        else:       # '끝 — …' · 'Done — …' 처럼 번호가 없는 줄은 첫 낱말을 번호처럼 굵게 그리지 않는다(10-10 공개 전 점검)
            page.p(_keepall(text), page.body, (22, 6, 0, 0))
    for line in intro['outro']:
        page.text(line, page.muted, (20, 14, 0, 0))


def _steps_page(page: _Page, step: int) -> None:
    entry = tut.TUTORIAL[step]
    page.text(entry['goal'], page.body, _GOAL)
    page.text(tut.HINT, page.muted, _HINT)
    for i, action in enumerate(entry['steps'], 1):
        page.p(f"<span style='{page.number}'>{i}</span>&nbsp;&nbsp;"
               f"<span style='{page.strong}'>{_keepall(str(t(action['do'])))}</span>", page.body, _ACTION)
        page.p(f"→ {_keepall(str(t(action['see'])))}", page.muted, _SEE)
        if action['img']:
            page.image(action['img'])
    if step != 1:
        page.text('처음 쓰신다면 1단계 도움말의 "처음이라면" 탭부터 보세요.', page.small, _NOTE)


def _list_page(page: _Page, items: list[str]) -> None:
    for item in items:
        page.p(f"·&nbsp;{_keepall(str(t(item)))}", page.body, _ITEM)


def _trouble_page(page: _Page, pairs: list[tuple[str, str]]) -> None:
    for symptom, fix in pairs:
        page.p(f"<span style='{page.strong}'>{_keepall(str(t(symptom)))}</span>", page.body, _SYMPTOM)
        page.p(f"→ {_keepall(str(t(fix)))}", page.body, _FIX)


def tabs_for(step: int) -> list[tuple[str, str]]:
    """단계의 탭 (키, 한국어 이름). 0 = 처음이라면만, 1 = 처음이라면 + 4개, 2~8 = 4개, 그 밖 = 없음."""
    if step == 0:
        return [('intro', tut.TAB_INTRO)]
    if step not in tut.TUTORIAL:
        return []
    tabs = list(tut.TABS) + list(tut.EXTRA_TABS.get(step, ()))     # 5단계 + mdat 불러오기, 8단계 + CamillaDSP(10-10)
    if step == 1:
        tabs.insert(0, ('intro', tut.TAB_INTRO))
    return tabs


def _picture_page(page: _Page, names: dict) -> None:
    """그림이 본문인 덧붙임 탭(10-10 — 5단계 mdat 불러오기 · 내보내기 CamillaDSP). 맨 위 굵은 한 줄 뒤로 구역마다
    머리말 + 그림 + 한두 줄. 사용자: "열심히 안 보고 대충 읽으니 글로 어렵게 쓰면 안 돼." 문안 모양은 `tut.PICTURE_TABS`."""
    first, *rest = names['start']
    page.p(f"<span style='{page.lead}'>{_keepall(str(t(first)))}</span>", page.body, _START)
    for line in rest:
        page.text(line, page.body, _START_BODY)
    for section in names['sections']:
        # 번호는 붙이지 않는다 — 그림 속 빨간 ①②③ 과 헷갈린다.
        page.p(f"<span style='{page.section}'>{_keepall(str(t(section['head'])))}</span>", page.body, _SECTION)
        for line in section['before']:
            page.text(line, page.body, _LINE)
        if section['img']:
            page.image(section['img'], big=True)
        for name, text in section['rows']:
            page.p(f"<span style='{page.strong}'>{html.escape(name)}</span>&nbsp;&nbsp;—&nbsp;&nbsp;"
                   f"{_keepall(str(t(text)))}", page.body, _ROW)
        bullets = not section['img'] and not section['rows']
        for line in section['after']:
            if bullets:
                page.p(f"·&nbsp;{_keepall(str(t(line)))}", page.body, _LINE)
            elif section['rows']:         # 못 알아보는 이름 다음 줄 = 할 일 한 줄
                page.p(f"<span style='{page.strong}'>→&nbsp;{_keepall(str(t(line)))}</span>", page.body, _LINE)
            else:
                page.text(line, page.body, _LINE)


def build_page(step: int, tab: str, width: int = 600, dpr: float = 1.0) -> _Page:
    """(단계, 탭) → `_Page`(HTML · 블록 규격 · 그림). 모르는 단계·탭이면 안내 한 줄."""
    page = _Page(max(120, int(width)), dpr)
    entry = tut.TUTORIAL.get(step)
    if tab == 'intro' and step in (0, 1):
        _intro_page(page)
    elif entry is None:
        page.text('이 단계의 도움말이 아직 없습니다.', page.body, _GOAL)
    elif tab == 'steps':
        _steps_page(page, step)
    elif tab == 'checks':
        _list_page(page, entry['checks'])
    elif tab == 'trouble':
        _trouble_page(page, entry['trouble'])
    elif tab == 'real':
        _list_page(page, entry['real'])
    elif tab in tut.PICTURE_TABS and any(key == tab for key, _l in tut.EXTRA_TABS.get(step, ())):
        _picture_page(page, tut.PICTURE_TABS[tab][0])
    else:
        page.text('이 단계의 도움말이 아직 없습니다.', page.body, _GOAL)
    return page


def render_html(step: int, tab: str | None = None, width: int = 600) -> str:
    """(단계, 탭) → HTML(탭을 안 주면 그 단계의 첫 탭). 그림 자원은 `HelpDialog` 가 문서에 등록한다."""
    tabs = tabs_for(int(step))
    key = tab or (tabs[0][0] if tabs else 'steps')
    return build_page(int(step), key, width).html()


# ──────────────────────────── 탭 줄 ────────────────────────────
class _TabStrip(QtWidgets.QScrollArea):
    """탭 줄 — 창보다 넓으면 탭을 줄이지 않고 가로로 스크롤한다.

    전에는 탭 버튼이 창 폭에 맞춰 눌려 글자 양쪽이 잘렸다(빌드 전 QA 2026-09-24, 640×720 영어 ·
    520×420 한/영). 넘칠 때만 가로 스크롤바만큼 높이를 늘리고, 휠을 굴리면 가로로 움직인다.
    """

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self.setWidgetResizable(True)
        self.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        self.setVerticalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setHorizontalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        self.setSizePolicy(QtWidgets.QSizePolicy.Policy.Expanding, QtWidgets.QSizePolicy.Policy.Fixed)
        # 스크롤바는 얇은 막대만(화살표 없음) — 탭 줄 아래에 굵은 기본 스크롤바가 붙으면 본문처럼 보였다.
        self.setStyleSheet(
            'QScrollArea, QScrollArea > QWidget > QWidget { background: transparent; }'
            'QScrollBar:horizontal { height: 6px; background: transparent; margin: 0; border: 0; }'
            f'QScrollBar::handle:horizontal {{ background: {COLORS["line"]}; border-radius: 3px; min-width: 32px; }}'
            'QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal { width: 0; border: 0; }'
            'QScrollBar::add-page:horizontal, QScrollBar::sub-page:horizontal { background: transparent; }')
        inner = QtWidgets.QWidget()
        self.row = QtWidgets.QHBoxLayout(inner)
        self.row.setContentsMargins(0, 0, 0, 0)
        self.row.setSpacing(6)
        self.setWidget(inner)

    def overflowing(self) -> bool:
        return self.widget().minimumSizeHint().width() > self.viewport().width()

    def sync_height(self) -> None:
        inner = self.widget()
        height = inner.sizeHint().height()
        if self.overflowing():
            height += self.horizontalScrollBar().sizeHint().height() + 2
        if self.height() != height:
            self.setFixedHeight(height)

    def resizeEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        super().resizeEvent(event)
        self.sync_height()

    def wheelEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        bar = self.horizontalScrollBar()
        delta = event.angleDelta().y() or event.angleDelta().x()
        if bar.maximum() > 0 and delta:
            bar.setValue(bar.value() - delta // 2)
            event.accept()
            return
        super().wheelEvent(event)


# ──────────────────────────── 큰 보기 창 ────────────────────────────
class ImageViewer(QtWidgets.QDialog):
    """그림 하나를 크게(화면 90 % 안에 맞춰) 보여 준다. 누르거나 Esc 로 닫힌다."""

    def __init__(self, key: str, parent=None) -> None:
        super().__init__(parent)
        self.setObjectName('helpImageViewer')
        self.key = key
        # 부모가 화면 밖 그리기(테스트·qa_capture)면 따라간다 — 사용자 화면에 튀어나오지 않게.
        hidden = QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen
        if parent is not None and parent.window().testAttribute(hidden):
            self.setAttribute(hidden, True)
        title, legend = tut.caption_of(key)
        self.setWindowTitle(str(t(title)) if title else str(t('큰 그림')))
        self.setStyleSheet('QDialog#helpImageViewer { background: #ffffff; }')

        screen = (parent.screen() if parent is not None else None) or QtGui.QGuiApplication.primaryScreen()
        area = screen.availableGeometry() if screen is not None else QtCore.QRect(0, 0, 1280, 800)
        caption_h = 28 + 18 * len(legend)
        max_w = int(area.width() * 0.9) - 24
        max_h = int(area.height() * 0.9) - 24 - caption_h

        original = original_image(key)
        if original is None:
            pixmap = QtGui.QPixmap.fromImage(placeholder_image(min(max_w, 640), key))
        else:
            # 논리 크기는 화면 90 % 안에 맞추고, 화소는 화면 배율만큼(원본 한도) 남겨 흐리지 않게 한다.
            scale = min(1.0, max_w / original.width(), max_h / original.height())
            logical_w = max(1.0, original.width() * scale)
            dpr = float(screen.devicePixelRatio()) if screen is not None else 1.0
            pixels_w = min(original.width(), max(1, round(logical_w * dpr)))
            image = original if pixels_w == original.width() else original.scaledToWidth(
                pixels_w, QtCore.Qt.TransformationMode.SmoothTransformation)
            pixmap = QtGui.QPixmap.fromImage(image)
            pixmap.setDevicePixelRatio(pixels_w / logical_w)
        box = QtWidgets.QVBoxLayout(self)
        box.setContentsMargins(12, 12, 12, 10)
        box.setSpacing(6)
        self.picture = QtWidgets.QLabel()
        self.picture.setPixmap(pixmap)
        self.picture.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        attach(self.picture, 'win.help_viewer')
        box.addWidget(self.picture)
        lines = [f'{_CIRCLED[i] if i < len(_CIRCLED) else i + 1} {t(item)}' for i, item in enumerate(legend)]
        lines.append(str(t('닫으려면 누르거나 Esc')))
        self.caption = QtWidgets.QLabel('\n'.join(lines))
        self.caption.setWordWrap(True)
        self.caption.setStyleSheet(f'font-size:12px; color:{COLORS["ink2"]};')
        box.addWidget(self.caption)

    def mousePressEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        self.accept()


# ──────────────────────────── 창 ────────────────────────────
class HelpDialog(QtWidgets.QDialog):
    """단계별 따라 하기 튜토리얼(비모달)."""

    RESIZE_DEBOUNCE_MS = 120

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self.setObjectName('helpDialog')
        self.setWindowFlag(QtCore.Qt.WindowType.Tool, True)      # 주 창 위에 떠 있되 작업 표시줄에 따로 안 뜬다
        self.setMinimumSize(520, 420)
        self.resize(640, 720)
        self.step = -1
        self.tab = ''
        self.viewer: ImageViewer | None = None
        self._tab_buttons: dict[str, QtWidgets.QPushButton] = {}
        self._image_keys: list[str] = []
        self._rendered_width = 0

        box = QtWidgets.QVBoxLayout(self)
        box.setContentsMargins(16, 14, 16, 12)
        box.setSpacing(8)

        head = QtWidgets.QHBoxLayout()
        head.setSpacing(10)
        self.title_label = QtWidgets.QLabel()
        self.title_label.setStyleSheet(
            f'font-family:{FONT_UI}; font-weight:600; font-size:16px; color:{COLORS["ink"]};')
        self.chip_label = QtWidgets.QLabel()
        self.chip_label.setStyleSheet(
            f'font-family:{FONT_UI}; font-size:12px; color:{COLORS["ink2"]};'
            f'border:1px solid {COLORS["line"]}; border-radius:3px; padding:1px 7px;')
        self.search_edit = QtWidgets.QLineEdit()
        self.search_edit.setFixedWidth(180)
        self.search_edit.setClearButtonEnabled(True)
        self.search_edit.textChanged.connect(self._search)
        self.search_edit.returnPressed.connect(self._search_next)
        attach(self.search_edit, 'win.help_search')
        head.addWidget(self.title_label)
        head.addWidget(self.chip_label)
        head.addStretch(1)
        head.addWidget(self.search_edit)
        box.addLayout(head)

        self.tabs_frame = QtWidgets.QFrame()
        frame_box = QtWidgets.QVBoxLayout(self.tabs_frame)
        frame_box.setContentsMargins(0, 0, 0, 0)
        self.tabs_strip = _TabStrip()
        frame_box.addWidget(self.tabs_strip)
        self.tabs_layout = self.tabs_strip.row
        box.addWidget(self.tabs_frame)

        self.browser = QtWidgets.QTextBrowser()
        self.browser.setOpenLinks(False)                 # tut: 링크는 큰 보기로(아래), 이동하지 않는다
        self.browser.setOpenExternalLinks(False)
        self.browser.anchorClicked.connect(self._on_anchor)
        self.browser.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        # 어절이 줄에 다 안 들어갈 때만 글자 단위로 접는다 — 명시해서 Qt 버전이 바뀌어도 틀어지지 않게.
        self.browser.setWordWrapMode(QtGui.QTextOption.WrapMode.WrapAtWordBoundaryOrAnywhere)
        self.browser.setHorizontalScrollBarPolicy(QtCore.Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.browser.setStyleSheet(
            f'QTextBrowser {{ background:#ffffff; border:none; border-top:1px solid {COLORS["line"]}; }}')
        self.browser.setViewportMargins(6, 12, 6, 12)
        self.browser.viewport().installEventFilter(self)
        box.addWidget(self.browser, 1)

        foot = QtWidgets.QHBoxLayout()
        self.hint_label = QtWidgets.QLabel()
        self.hint_label.setStyleSheet(f'font-size:12px; color:{COLORS["ink2"]};')
        close = QtWidgets.QPushButton(t('닫기'))
        set_variant(close, 'ghost', 'sm')
        close.clicked.connect(self.close)
        attach(close, 'win.help_close')
        foot.addWidget(self.hint_label, 1)
        foot.addWidget(close)
        box.addLayout(foot)

        self._resize_timer = QtCore.QTimer(self)
        self._resize_timer.setSingleShot(True)
        self._resize_timer.setInterval(self.RESIZE_DEBOUNCE_MS)
        self._resize_timer.timeout.connect(self._rerender_for_width)

        find_key = QtGui.QShortcut(QtGui.QKeySequence.StandardKey.Find, self)
        find_key.activated.connect(self._focus_search)

    # ──── 검색(지금 탭 안에서만) ────
    def _focus_search(self) -> None:
        self.search_edit.setFocus(QtCore.Qt.FocusReason.ShortcutFocusReason)
        self.search_edit.selectAll()

    def _search(self, text: str) -> None:
        cursor = self.browser.textCursor()
        cursor.movePosition(QtGui.QTextCursor.MoveOperation.Start)
        self.browser.setTextCursor(cursor)
        if text:
            self.browser.find(text)

    def keyPressEvent(self, event):  # noqa: N802 - Qt 이름
        """Enter 는 창을 닫지 않는다 — QDialog 는 Enter 로 기본 버튼을 눌러 닫혔다(10-10 사용자: 검색 칸에서 Enter 를 누르자
        도움말이 닫혔다). 검색 칸의 Enter 는 그 칸이 이미 '다음 결과' 로 처리했다. Esc 는 그대로 닫기."""
        if event.key() in (QtCore.Qt.Key.Key_Return, QtCore.Qt.Key.Key_Enter):
            event.accept()
            return
        super().keyPressEvent(event)

    def _search_next(self) -> None:
        text = self.search_edit.text()
        if not text:
            return
        if not self.browser.find(text):
            cursor = self.browser.textCursor()
            cursor.movePosition(QtGui.QTextCursor.MoveOperation.Start)
            self.browser.setTextCursor(cursor)
            self.browser.find(text)

    # ──── 그림 폭 · 다시 그리기 ────
    def image_width(self) -> int:
        """지금 뷰포트에 넣을 그림 최대 폭(원본보다 크면 원본 폭이 이긴다 — `display_image`)."""
        margin = 2 * self.browser.document().documentMargin()
        return max(160, int(self.browser.viewport().width() - margin - _IMAGE_SIDE_MARGIN))

    def eventFilter(self, obj, event) -> bool:  # noqa: N802 - Qt 이름
        if obj is self.browser.viewport() and event.type() == QtCore.QEvent.Type.Resize:
            if self._image_keys and self.image_width() != self._rendered_width:
                self._resize_timer.start()
        return super().eventFilter(obj, event)

    def showEvent(self, event) -> None:  # noqa: N802 - Qt 이름
        # 처음 보일 때는 기다리지 않고 바로 맞춘다 — 보이기 전의 뷰포트 폭은 믿을 수 없다(QA 09-24:
        # 1단계 첫 그림이 창보다 넓게 그려졌다).
        super().showEvent(event)
        self.flush_resize()

    def flush_resize(self) -> None:
        """기다리던 폭 맞춤을 지금 한다(보일 때 · 캡처 도구가 크기를 바꾼 직후)."""
        self._resize_timer.stop()
        self._rerender_for_width()

    def _rerender_for_width(self) -> None:
        """창 크기가 바뀐 뒤 그림 폭을 새로 맞춰 다시 그린다. 스크롤 위치는 비율로 지킨다."""
        if self.image_width() == self._rendered_width:
            return
        bar = self.browser.verticalScrollBar()
        ratio = bar.value() / bar.maximum() if bar.maximum() > 0 else 0.0
        self._render()
        bar = self.browser.verticalScrollBar()
        bar.setValue(round(ratio * bar.maximum()))

    def _render(self) -> None:
        if not self.isVisible():
            self.layout().activate()          # 숨은 창도 지금 크기로 뷰포트 폭을 정해 둔다
        width = self.image_width()
        dpr = float(self.devicePixelRatioF() or 1.0)
        page = build_page(self.step, self.tab, width, dpr)
        document = self.browser.document()
        # setHtml 전에 자원을 걸어 두어야 첫 배치에서 그림 크기를 안다(뒤에서 한 번 더 — clear 가 지울 수 있다).
        for key, image in page.images.items():
            document.addResource(QtGui.QTextDocument.ResourceType.ImageResource.value, QtCore.QUrl(f'tut:{key}'), image)
        self.browser.setHtml(page.html())
        for key, image in page.images.items():
            document.addResource(QtGui.QTextDocument.ResourceType.ImageResource.value, QtCore.QUrl(f'tut:{key}'), image)
        _apply_metrics(document, page.specs)
        self._image_keys = list(page.images)
        self._rendered_width = width
        if self.search_edit.text():
            self._search(self.search_edit.text())

    # ──── 그림 큰 보기 ────
    def _on_anchor(self, url: QtCore.QUrl) -> None:
        if url.scheme() == 'tut':
            self.show_image(url.path() or url.toString().split(':', 1)[-1])

    def show_image(self, key: str) -> ImageViewer:
        if self.viewer is not None:
            self.viewer.close()
        self.viewer = ImageViewer(key, self)
        self.viewer.open()
        return self.viewer

    # ──── 탭 ────
    def _build_tabs(self, tabs: list[tuple[str, str]]) -> None:
        # setParent(None) 로 화면 트리에서 즉시 떼어낸다 — deleteLater() 만 걸면 다음 이벤트
        # 루프까지 옛 버튼이 그대로 그려져 있어 단계를 옮길 때마다 탭이 겹쳐 보였다(2026-09-24).
        while self.tabs_layout.count():
            item = self.tabs_layout.takeAt(0)
            widget = item.widget()
            if widget is not None:
                widget.setParent(None)
                widget.deleteLater()
        self._tab_buttons = {}
        if len(tabs) < 2:
            self.tabs_frame.setVisible(False)
            return
        self.tabs_frame.setVisible(True)
        # 클릭 콜백이 self.set_tab 을 강하게 쥐면 창 → 탭 버튼 → 람다 → 창 순환이 생겨, 닫은 창이 GC 때 아무 때나 지워진다
        # (10-10: 다른 창을 그리는 중에 지워져 테스트가 abort) — 약한 참조로 끊는다.
        weak_pick = weakref.WeakMethod(self.set_tab)

        def pick(key: str) -> None:
            method = weak_pick()
            if method is not None:
                method(key)

        frame, buttons = seg([(key, str(t(label))) for key, label in tabs], pick)
        self.tabs_layout.addWidget(frame)
        self.tabs_layout.addStretch(1)
        # 보이는 창에 레이아웃으로 넣은 위젯은 다음 이벤트 루프에서야 보이게 된다 — 그 전에 잰 높이는 0 이라
        # 창을 연 채로 단계를 옮기면 탭 줄이 통째로 사라졌다(QA 09-24). 지금 보이게 해 두고 잰다.
        frame.show()
        self._tab_buttons = buttons
        for button in buttons.values():
            # 글자 폭 밑으로는 줄지 않게 — 모자라면 탭 줄이 가로로 스크롤된다(_TabStrip).
            button.ensurePolished()
            button.setMinimumWidth(button.sizeHint().width())
            attach(button, 'win.help_tabs')
        self.tabs_strip.horizontalScrollBar().setValue(0)
        self.tabs_strip.sync_height()

    def set_tab(self, key: str) -> None:
        """탭(intro · steps · checks · trouble · real)을 바꿔 그린다. 맨 위부터 보인다."""
        keys = [k for k, _label in tabs_for(self.step)]
        if key not in keys:
            key = keys[0] if keys else 'steps'
        self.tab = key
        for name, button in self._tab_buttons.items():
            set_state(button, 'on' if name == key else '')
            if name == key:
                self.tabs_strip.ensureWidgetVisible(button, 24, 0)
        self._render()
        self.browser.verticalScrollBar().setValue(0)

    def tab_keys(self) -> list[str]:
        return [key for key, _label in tabs_for(self.step)]

    # ──── 단계 ────
    def set_step(self, step: int) -> None:
        """단계 번호(0 = 처음이라면, 1~8)의 튜토리얼을 그린다. 모르는 단계는 안내 한 줄.

        같은 단계를 다시 부르면(언어 전환 등) 보던 탭을 지킨다. 단계가 바뀌면 그 단계의 첫 탭
        (1단계는 '처음이라면', 나머지는 '따라 하기')부터 보인다.
        """
        step = int(step)
        same = step == self.step
        self.step = step
        # step = 창의 화면 번호(1~5 · 내보내기 8 · 보정 설계 9). 제목은 화면 이름, 칩은 진행바 위치(모르는 번호는 제목·칩 없음).
        title = PAGE_TITLES.get(step, '')
        if step == 0:
            heading = str(t(tut.TAB_INTRO))
            self.setWindowTitle(f'{t("도움말")} · {heading}')
            self.title_label.setText(heading)
        else:
            self.setWindowTitle(f'{t("도움말")} · {bar_position(step)} · {t(title)}' if title else str(t('도움말')))
            self.title_label.setText(str(t(title)) if title else str(t('도움말')))
        self.chip_label.setText(f'{bar_position(step)} / {len(STEPS)}' if title else '')
        self.chip_label.setVisible(bool(title))
        self.search_edit.setPlaceholderText(str(t('도움말 검색')))
        tabs = tabs_for(step)
        self._build_tabs(tabs)
        keys = [key for key, _label in tabs]
        keep = self.tab if same and self.tab in keys else (keys[0] if keys else 'steps')
        scroll = self.browser.verticalScrollBar().value() if same else 0
        self.set_tab(keep)
        if same:
            self.browser.verticalScrollBar().setValue(scroll)
        self.hint_label.setText(str(t('그림을 누르면 크게 봅니다.')))


__all__ = ['HelpDialog', 'ImageViewer', 'render_html', 'build_page', 'tabs_for', 'image_path',
           'original_image', 'placeholder_image', 'display_image', 'TUTORIAL_DIR']
