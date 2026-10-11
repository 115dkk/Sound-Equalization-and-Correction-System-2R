"""도움말 튜토리얼 그림 만들기 — 위저드 1→8단계를 샘플 데이터로 실제로 눌러 가며 화면을 잘라 강조 표시를 그린다.

부르는 곳: 사람이 손으로(화면·문구가 바뀌었을 때 다시 뽑는다). 계약은
`문서/워커 과제/도움말 튜토리얼 계약 2026-09-24.md` 1절(키 표) — 키·강조 번호 순서는 그 표를 따른다.
입력: 없음(FakeREW 데모 방 샘플 = 7.1.4 + 서브 3, 창 1280×800). 출력:
`src/secs/wizard/assets/tutorial/<key>_<lang>.png` + `MANIFEST.json`(자르기·강조 사각형은 창 논리 좌표 px).

    .venv-secs\\Scripts\\python.exe tools\\make_tutorial_shots.py                 # ko·en 전부
    .venv-secs\\Scripts\\python.exe tools\\make_tutorial_shots.py --lang en --keys s1_rew,s8_files

qa_capture 와 같은 원칙: **오프스크린 플랫폼 금지**(글꼴이 없다) — 실제 windows 플랫폼에서 돌리되 모든 창에
`WA_DontShowOnScreen` 을 걸어 화면에는 절대 뜨지 않는다. 확대 그림은 스크롤 없이 한 번에 담으려고 잠깐
창 높이를 페이지 높이만큼 늘려 찍는다(폭은 1280 그대로라 배치는 같다). 전체 그림(`*_overview`)은 1280×800
그대로 — 단 `s8_overview` 는 시트·내보내기·검증을 한 장에 담으려고 페이지 전체 높이로 찍는다.

그림 규격(계약): 폭 ≤ 900 px(LANCZOS 축소, 확대는 안 함), 빨간(#D8352A) 둥근 테두리 3 px + 번호 원(지름 22 px,
빨간 바탕 흰 숫자) — 축소한 뒤에 그려 굵기가 줄지 않는다. 팔레트 PNG(`quantize`), 파일당 120 KB 목표.
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

OUT_DIR = ROOT / 'src' / 'secs' / 'wizard' / 'assets' / 'tutorial'
WINDOW = (1280, 800)
MAX_WIDTH = 900
PAD = 12            # 자르기 여백(논리 px)
MARK_PAD = 4        # 강조 테두리를 위젯 바깥으로 띄우는 여백(논리 px)
RED = (0xD8, 0x35, 0x2A)
TARGET_BYTES = 120 * 1024
FONT_DIR = Path(os.environ.get('WINDIR', r'C:\Windows')) / 'Fonts'

#: 계약 1절 표의 키(순서 = 흐름 순서).
KEYS: tuple[str, ...] = (
    's1_overview', 's1_rew', 's1_mic',
    's2_overview', 's2_preset', 's2_patch', 's2_dsp',
    's3_overview', 's3_channel', 's3_master',
    's4_overview', 's4_pick', 's4_options',
    's5_before', 's5_running', 's5_after1', 's5_done', 's5_mdat',
    's9_overview', 's9_after', 's9_spl', 's9_xo', 's9_subs',
    's8_overview', 's8_export', 's8_files',
    'side_nav',
)


# ──────────────────────────── 그림 처리(Pillow, Qt 없음) ────────────────────────────
def _font(size: int, bold: bool = True):
    from PIL import ImageFont

    for name in (('malgunbd.ttf', 'arialbd.ttf') if bold else ('malgun.ttf', 'arial.ttf')):
        path = FONT_DIR / name
        if path.is_file():
            return ImageFont.truetype(str(path), size)
    return ImageFont.load_default()


def _parts(mark):
    """강조 하나 → 조각 목록. (x, y, w, h) 하나이거나 그런 것의 목록(같은 번호로 테두리 여럿)."""
    return list(mark) if mark and isinstance(mark[0], (list, tuple)) else [mark]


def draw_marks(image, marks, badge_left: bool = False):
    """축소가 끝난 그림 위에 번호 강조를 그린다. marks 는 그림 px 좌표(x, y, w, h) — 또는 그런 조각의
    목록(2026-09-24: 6단계 커튼처럼 한 번호가 떨어진 두 구역을 가리킬 때). 번호 원은 첫 조각에만."""
    from PIL import ImageDraw

    draw = ImageDraw.Draw(image)
    width, height = image.size
    for x, y, w, h in [part for mark in marks for part in _parts(mark)]:
        x0, y0 = max(1.0, x), max(1.0, y)
        x1, y1 = min(width - 2.0, x + w), min(height - 2.0, y + h)
        if x1 <= x0 or y1 <= y0:
            continue            # 그림 밖(shot() 이 경고를 찍는다)
        draw.rounded_rectangle((x0, y0, x1, y1), radius=7, outline=RED, width=3)
    font = _font(14)
    radius = 11
    for number, mark in enumerate(marks, 1):
        x, y, w, h = _parts(mark)[0]
        if x >= width or y >= height or x + w <= 0 or y + h <= 0:
            continue
        # 번호 원은 테두리 왼쪽 위 모서리에 걸친다 — 위젯 안 글자 첫머리를 덜 가리게 모서리보다 조금
        # 바깥(왼쪽 위)으로 민다. 그림 밖으로 나가면 안쪽으로 민다.
        cx = min(max(x - radius - 6 if badge_left else x - 4, radius + 1), width - radius - 2)
        cy = min(max(y + h / 2 if badge_left else y - 4, radius + 1), height - radius - 2)
        draw.ellipse((cx - radius - 1, cy - radius - 1, cx + radius + 1, cy + radius + 1), fill=(255, 255, 255))
        draw.ellipse((cx - radius, cy - radius, cx + radius, cy + radius), fill=RED)
        draw.text((cx, cy), str(number), fill=(255, 255, 255), font=font, anchor='mm')
    return image


def save_png(image, path: Path) -> int:
    """팔레트 PNG 로 저장 — 목표 크기를 넘으면 색 수를 줄여 다시. 쓴 바이트 수."""
    from PIL import Image

    rgb = image.convert('RGB')
    for colors in (256, 160, 96, 64):
        palette = rgb.quantize(colors=colors, method=Image.Quantize.MEDIANCUT,
                               dither=Image.Dither.NONE)
        palette.save(path, optimize=True)
        size = path.stat().st_size
        if size <= TARGET_BYTES:
            break
    return size


def finish(image, marks_px: list[tuple[float, float, float, float]], path: Path,
           badge_left: bool = False) -> tuple[int, tuple[int, int]]:
    """크롭된 원본 해상도 그림 → 폭 ≤ 900 축소 → 강조 → 저장. (바이트, 최종 크기)."""
    from PIL import Image

    scale = min(1.0, MAX_WIDTH / image.width)
    if scale < 1.0:
        image = image.resize((round(image.width * scale), round(image.height * scale)), Image.LANCZOS)
    marks = [[(x * scale, y * scale, w * scale, h * scale) for x, y, w, h in _parts(mark)]
             for mark in marks_px]
    draw_marks(image, marks, badge_left)
    return save_png(image, path), image.size


def draw_tree(lines: list[tuple[int, str, bool]], marks: list[int], title: str):
    """폴더 트리를 글자 그림으로. lines = (깊이, 이름, 폴더인가). marks = 강조할 줄 번호(0-기준) 순서."""
    from PIL import Image, ImageDraw

    font = _font(17, bold=False)
    bold = _font(17)
    head = _font(18)
    line_h, left, top, indent = 30, 24, 60, 30
    width = 820
    height = top + line_h * len(lines) + 24
    image = Image.new('RGB', (width, height), (246, 248, 247))
    draw = ImageDraw.Draw(image)
    draw.rectangle((0, 0, width - 1, 42), fill=(34, 74, 74))
    draw.text((left, 21), title, fill=(255, 255, 255), font=head, anchor='lm')
    rects = []
    for index, (depth, name, folder) in enumerate(lines):
        y = top + index * line_h
        x = left + depth * indent
        if depth:
            # 가지 선: 부모 줄에서 내려와 이 줄로 꺾인다
            px = left + (depth - 1) * indent + 8
            draw.line((px, y - line_h / 2 + 4, px, y + line_h / 2 - 2), fill=(150, 160, 160), width=1)
            draw.line((px, y + line_h / 2 - 2, x - 4, y + line_h / 2 - 2), fill=(150, 160, 160), width=1)
        text = f'{name}{"/" if folder else ""}'
        draw.text((x, y + line_h / 2 - 2), text, fill=(30, 40, 40) if folder else (60, 70, 70),
                  font=bold if folder else font, anchor='lm')
        box = draw.textbbox((x, y + line_h / 2 - 2), text, font=bold if folder else font, anchor='lm')
        rects.append((box[0] - 8, y + 1, box[2] - box[0] + 16, line_h - 4))
    marks_px = [rects[i] for i in marks]
    return image, marks_px


# ──────────────────────────── Qt 쪽 ────────────────────────────
class Shooter:
    """창 하나로 흐름을 걸으며 키마다 찍는다."""

    def __init__(self, app, lang: str, keys: set[str], out: Path, tmp: Path) -> None:
        self.app, self.lang, self.keys, self.out, self.tmp = app, lang, keys, out, tmp
        self.manifest: dict[str, dict] = {}
        self.missing: dict[str, str] = {}
        self.win = None

    # ── 좌표 도우미 ──
    def rect(self, widget, local=None):
        """위젯(또는 위젯 안 사각형 local)의 창 논리 좌표 QRect."""
        from PyQt6 import QtCore

        if local is None:
            local = QtCore.QRect(0, 0, widget.width(), widget.height())
        top_left = widget.mapTo(self.win, local.topLeft())
        return QtCore.QRect(top_left, local.size())

    @staticmethod
    def union(*rects):
        out = rects[0]
        for r in rects[1:]:
            out = out.united(r)
        return out

    @staticmethod
    def card_of(widget):
        w = widget
        while w is not None and w.objectName() != 'card':
            w = w.parentWidget()
        return w if w is not None else widget

    def content(self, card, pad: int = 14):
        """카드의 실제 내용 범위(창 좌표) — 확대 그림에서 카드가 옆 칸 높이만큼 늘어난 빈 아래쪽을 뺀다."""
        from PyQt6 import QtWidgets

        rects = [self.rect(w) for w in card.findChildren(QtWidgets.QWidget)
                 if w.isVisible() and w.width() > 0 and w.height() > 0]
        whole = self.rect(card)
        if not rects:
            return whole
        bottom = max(r.bottom() for r in rects) + pad
        return _qrect(whole.x(), whole.y(), whole.width(), min(whole.height(), bottom - whole.y() + 1))

    def item_rect(self, view, local):
        """아이템 뷰(표·목록) 뷰포트 안 사각형 → 창 좌표."""
        return self.rect(view.viewport(), local)

    def plot_rect(self, plot, x0, x1, y0, y1):
        """pyqtgraph 뷰 좌표(로그 x 그대로) 사각형 → 창 좌표."""
        from PyQt6 import QtCore

        vb = plot.getPlotItem().vb
        a = plot.mapFromScene(vb.mapViewToScene(QtCore.QPointF(x0, y0)))
        b = plot.mapFromScene(vb.mapViewToScene(QtCore.QPointF(x1, y1)))
        local = QtCore.QRect(a, b).normalized()
        # 그래프 칸(뷰박스) 밖으로 나가지 않게
        box = plot.mapFromScene(vb.sceneBoundingRect()).boundingRect()
        return self.rect(plot, local.intersected(box))

    # ── 상태 ──
    def settle(self, rounds: int = 8) -> None:
        for _ in range(rounds):
            self.app.processEvents()
        if hasattr(self.win, '_poll_busy'):
            self.win._poll_busy()
            for _ in range(3):
                self.app.processEvents()

    def page(self):
        return self.win.stack.currentWidget()

    def tall(self):
        """창 높이를 페이지 전체가 스크롤 없이 들어가게 늘린다(폭은 그대로). 되돌리는 함수를 돌려준다."""
        area = self.page()
        chrome = self.win.height() - area.viewport().height()
        need = area.widget().sizeHint().height() + chrome + 4
        if need > self.win.height():
            self.win.resize(WINDOW[0], need)
            self.settle()
        return lambda: (self.win.resize(*WINDOW), self.settle())

    def want(self, *keys) -> bool:
        return any(k in self.keys for k in keys)

    def shot(self, key: str, crop, marks, *, pad: int = PAD) -> None:
        """지금 창을 찍어 crop(창 좌표 QRect, None=창 전체)으로 자르고 marks 를 그려 저장."""
        from PIL import Image
        from PyQt6 import QtCore

        if key not in self.keys:
            return
        self.settle()
        pixmap = self.win.grab()
        dpr = pixmap.devicePixelRatio() or 1.0
        qimage = pixmap.toImage().convertToFormat(pixmap.toImage().Format.Format_RGB888)
        raw = Image.frombuffer('RGB', (qimage.width(), qimage.height()),
                               bytes(qimage.constBits().asstring(qimage.sizeInBytes())),
                               'raw', 'RGB', qimage.bytesPerLine(), 1)
        full = QtCore.QRect(0, 0, self.win.width(), self.win.height())
        if crop is None:
            crop = full
        else:
            crop = crop.adjusted(-pad, -pad, pad, pad).intersected(full)
        box = (round(crop.x() * dpr), round(crop.y() * dpr),
               round((crop.x() + crop.width()) * dpr), round((crop.y() + crop.height()) * dpr))
        image = raw.crop(box)
        marks_px = []
        groups = [list(m) if isinstance(m, (list, tuple)) else [m] for m in marks]
        for number, group in enumerate(groups, 1):
            parts = []
            for m in group:
                if not crop.intersects(m) or not m.isValid():
                    print(f'  ! {key}: 강조 {number} 이(가) 그림 밖 {m.getRect()} (자르기 {crop.getRect()})')
                m = m.adjusted(-MARK_PAD, -MARK_PAD, MARK_PAD, MARK_PAD)
                parts.append(((m.x() - crop.x()) * dpr, (m.y() - crop.y()) * dpr,
                              m.width() * dpr, m.height() * dpr))
            marks_px.append(parts)
        # MANIFEST 에는 번호마다 사각형 하나(조각이 여럿이면 둘러싼 사각형) — 번호 수가 곧 캡션 수다.
        marks = [self.union(*g) for g in groups]
        name = f'{key}_{self.lang}.png'
        size, dims = finish(image, marks_px, self.out / name)
        self.record(key, name, [crop.x(), crop.y(), crop.width(), crop.height()],
                    [[m.x(), m.y(), m.width(), m.height()] for m in marks], size, dims,
                    window=[self.win.width(), self.win.height()])

    def record(self, key, name, crop, marks, size, dims, window=None) -> None:
        entry = {'file': name, 'crop': crop, 'marks': marks, 'size': list(dims), 'bytes': size}
        if window and list(window) != list(WINDOW):
            entry['window'] = list(window)
        self.manifest[key] = entry
        print(f'  {name:28s} {dims[0]}x{dims[1]}  {size / 1024:6.1f} KB  marks={len(marks)}')

    # ── 흐름 ──
    def build(self):
        from PyQt6 import QtCore

        import secs.model as model
        import secs.rewapi as rewapi
        from secs.wizard import WizardWindow

        from secs.wizard import new_project

        # 샘플 모드 그대로(09-24 master: 샘플은 실측 1점 — 배치 '단일 · 청취 위치 1점', FakeREW 는
        # 위치 2 이상을 막는다).
        project = new_project(fake=True)
        rew = rewapi.FakeREW(delay_s=0.0, use_sample_dolby=True)
        win = WizardWindow(project, rew, rew, rew, rew, project_path=self.tmp / 'sample.secs')
        win.ask_before_close = False
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.resize(*WINDOW)
        win.show()
        self.win = win
        self.settle()

    def run(self) -> None:
        self.build()
        win = self.win
        try:
            self.step1()
            self.step2()
            self.step3()
            self.step4()
            self.step5()
            self.step9()
            self.step8()
            self.sidebar()
            self.subs_shot()
        finally:
            win.close()
            self.settle()

    def step1(self) -> None:
        win, s = self.win, self.win.devices_page
        win.go(1)
        self.settle()
        rew_card, mic_card = self.card_of(s.retry_button), self.card_of(s.input_combo)
        self.shot('s1_overview', None, [self.rect(rew_card), self.rect(mic_card), self.rect(win.next_button)])
        self.shot('s1_rew', self.content(rew_card),
                  [self.rect(s.retry_button), self.rect(s.status_pill), self.rect(s.read_button)])
        self.shot('s1_mic', self.content(mic_card),
                  [self.rect(s.input_combo), self.rect(s.input_channel), self.rect(s.cal_pill),
                   self.rect(s.sweep_combo)])

    def step2(self) -> None:
        win, s = self.win, self.win.config_page
        win.go(2)
        self.settle()
        # 1280 폭에서 배치도·DSP 칸은 시트 아래로 내려간다 — 전체 그림도 페이지 전체 높이로 찍는다.
        restore = self.tall()
        # 바이앰프 칸은 `secs.wizard.BIAMP_UI` 가 켜졌을 때만 있다(10-11 부터 닫음 — 없으면 None).
        presets = self.union(*[self.rect(b) for b in s._preset_buttons.values()],
                             self.rect(s._sub_minus), self.rect(s._sub_plus),
                             *([self.rect(s._biamp)] if s._biamp is not None else []))
        patch_card = self.card_of(s.table)
        self.shot('s2_overview', None, [presets, self.rect(patch_card), self.content(s._left_card)])
        add_button = next(b for b in patch_card.findChildren(type(s.csv_button))
                          if b.text().startswith('+'))
        self.shot('s2_preset', self.union(presets, self.rect(add_button)),
                  [self.rect(s._preset_buttons['7.1.4']),
                   self.union(self.rect(s._sub_minus), self.rect(s._sub_plus)), self.rect(add_button)])
        # 패치 시트 위 8행 — 출력(4) · 극성(5) · 크기(1) 열
        table = s.table
        rows = min(8, table.rowCount())
        bottom = table.rowViewportPosition(rows - 1) + table.rowHeight(rows - 1)
        header = table.horizontalHeader()
        head_rect = self.rect(header)

        def column(col):
            x = table.columnViewportPosition(col)
            body = self.item_rect(table, _qrect(x, 0, table.columnWidth(col), bottom))
            return self.union(body, _qrect(body.x(), head_rect.y(), body.width(), head_rect.height()))

        crop = self.union(head_rect, self.item_rect(table, _qrect(0, 0, table.viewport().width(), bottom)))
        self.shot('s2_patch', crop, [column(4), column(5), column(1)])
        dsp_card = self.card_of(s.dsp_profile)
        self.shot('s2_dsp', self.content(dsp_card),
                  [self.rect(s.dsp_profile),
                   self.union(self.rect(s.dsp_taps), self.rect(s.dsp_iir))])
        restore()

    def step3(self) -> None:
        win, s = self.win, self.win.level_page
        win.go(3)
        self.settle()
        # 샘플 흐름에는 레벨 값이 없다 — 한 채널을 실제로 재생해 막대를 채운다(FakeREW 레벨).
        role = next(iter(s._rows))
        speaker = s.project.config.speakers[0]
        try:
            s.toggle_play(role, speaker.bands[0].out if speaker.bands else 1)
            timer = _elapsed()
            while timer.elapsed() < 1500:
                self.app.processEvents()
        except Exception as error:  # noqa: BLE001 - 그림 도구: 재생이 안 돼도 빈 막대로 찍는다
            print('  (레벨 재생 실패:', error, ')')
        list_card, master_card = self.card_of(s._rows[role][0]), self.card_of(s.master)
        self.shot('s3_overview', None, [self.rect(list_card), self.rect(master_card)])
        meter, value, play = s._rows[role]
        from secs.wizard.step3_level import METER_MAX, METER_MIN, TARGET_HIGH, TARGET_LOW

        span = METER_MAX - METER_MIN
        zone = _qrect(int((TARGET_LOW - METER_MIN) / span * meter.width()), 0,
                      int((TARGET_HIGH - TARGET_LOW) / span * meter.width()), meter.height())
        # 자르기 = 머리글(채널 · 레벨 · 값) + 첫 채널 줄. 여백을 작게 해 다음 줄이 비치지 않게.
        card = self.rect(list_card)
        play_rect = self.rect(play)
        top = play_rect.y() - 28            # 머리글 줄 + 구분선
        row = _qrect(card.x() + 10, top, card.width() - 20, play_rect.bottom() + 6 - top)
        self.shot('s3_channel', row, [self.rect(play), self.rect(meter, zone), self.rect(value)], pad=2)
        self.shot('s3_master', self.content(master_card), [self.rect(s.master), self.rect(s.sequence_button)])
        if hasattr(s, 'stop_monitor'):
            s.stop_monitor()

    def step4(self) -> None:
        win, s = self.win, self.win.layout_page
        win.go(4)
        self.settle()
        pick_card, options_card = self.card_of(s.view), self.card_of(s.timing_combo)
        picks = self.union(*[self.rect(b) for b in s._picks.values()])
        self.shot('s4_overview', None, [picks, self.rect(options_card)])
        restore = self.tall()
        picks = self.union(*[self.rect(b) for b in s._picks.values()])
        center = s.view.marker_center(0)
        marker = self.rect(s.view, _qrect(center.x() - 16, center.y() - 16, 32, 32))
        self.shot('s4_pick', self.content(pick_card), [marker, picks])
        self.shot('s4_options', self.content(options_card),
                  [self.rect(s.timing_combo), self.rect(s._ref_holder)])
        restore()

    def step5(self) -> None:
        win, s = self.win, self.win.measure_page
        win.go(5)
        self.settle()
        self.shot('s5_before', None, [self.rect(s.position_title), self.rect(s.measure_button),
                                      self.rect(s.positions)])
        # 측정 대신 mdat 불러오기(10-09) — 오른쪽 카드의 [mdat 불러오기] 와 측정 지연
        self.shot('s5_mdat', self.union(self.rect(s.delay_combo), self.rect(s.mdat_button)),
                  [self.rect(s.mdat_button), self.rect(s.delay_combo)], pad=40)
        # 측정 중 — FakeREW 스윕을 잠깐 느리게 해 진행 막대가 도는 동안 찍는다.
        running = self.want('s5_running')
        if running:
            win.rew.delay_s = 1.0
        s.measure_position(1)
        if running:
            timer = _elapsed()
            while timer.elapsed() < 30000 and s.progress.value() < 35:
                self.app.processEvents()
            # 진행 표시 = 창 구석 진행 패널(경과·예상 시간). 안 보이면 위치 목록 아래 진행 막대.
            panel = getattr(win, 'busy_panel', None)
            target = panel if panel is not None and panel.isVisible() else s.progress
            self.shot('s5_running', None, [self.rect(target)])
        s.wait(180000)
        win.rew.delay_s = 0.0
        self.settle()
        first = s.positions.item(0)
        # ① 그래프 — 아래 내비 줄([← 이전])까지 덮지 않게 보이는 부분만(스크롤 영역에 잘린 채 보인다).
        plot = self.rect(s.plot).intersected(self.rect(self.page().viewport()))
        self.shot('s5_after1', None, [plot,
                                      self.item_rect(s.positions, s.positions.visualItemRect(first)),
                                      self.rect(s.measure_button)])
        for pos in range(2, s.position_count() + 1):
            if not s.measure_position(pos):
                break
            s.wait(180000)
            self.app.processEvents()
        self.settle()
        self.shot('s5_done', None, [self.rect(s.positions), self.rect(win.next_button)])

    def _calibrate(self, page) -> None:
        """보정 설계 [계산] — 첫 계산의 XO 추천은 건너뛴다(그림에는 결과만 필요, 화면은 같다)."""
        page.settings.xo_rec = {g: page.xo_of(g) for g in page.xo_group_names()}
        page.settings.store(page.project)
        page.compute()
        page.wait(900000)
        self.settle()

    def step9(self) -> None:
        win, s = self.win, self.win.calibration_page
        win.go(9)
        self.settle()
        self._calibrate(s)
        s.set_tab('spl')
        self.settle()
        mode_card = self.card_of(s.compute_button)
        tabs = self.union(*[self.rect(b) for b in s.tab_buttons.values() if b.isVisible()])   # 숨은 '서브 정렬' 탭(서브 1개)이 상자를 오른쪽 카드까지 늘렸다(10-10)
        self.shot('s9_overview', None, [self.rect(mode_card), tabs, self.rect(s.plot), self.rect(s.rc_card)])
        self.shot('s9_after', None, [self.rect(s.status), self.rect(s.plot)])
        restore = self.tall()
        self.shot('s9_spl', self.content(s.rc_card),
                  [self.union(self.rect(s.rc_lo_spin), self.rect(s.rc_hi_spin), self.rect(s.rc_hi_auto)),
                   self.union(self.rect(s.volume_spin), *[self.rect(d) for d in s.dim_spins]),
                   self.rect(s.boost_spin)])
        restore()
        s.set_tab('xo')
        self.settle()
        restore = self.tall()
        crop = self.union(self.rect(s.channel_combo), self.rect(s.plot), self.content(s.xo_card))
        self.shot('s9_xo', crop,
                  [self.rect(s.channel_combo), self.rect(s.xo_spin),
                   self.union(self.rect(s.xo_peq_check), self.rect(s.xo_peq_lo_spin), self.rect(s.xo_peq_hi_spin)),
                   self.rect(s.xo_button)])
        restore()
        s.set_tab('spl')
        self.settle()

    def step8(self) -> None:
        win, s = self.win, self.win.export_page
        win.go(8)
        self.settle()
        summary = self.union(self.rect(s.source_label), self.rect(s.roles_label), self.rect(s.rc_label))
        folder = self.union(self.rect(s.export_path_label), self.rect(s.folder_change), self.rect(s.folder_open))
        buttons = self.union(self.rect(s.export_button), self.rect(s.camilla_button))
        self.shot('s8_overview', None, [summary, folder, buttons])
        files = s.export(kind='mdat') + s.export(kind='camilla')
        self.settle()
        restore = self.tall()
        card = self.content(self.card_of(s.export_button))
        self.shot('s8_export', card, [self.rect(s.export_button), self.rect(s.camilla_button),
                                      self.rect(s.export_note), self.rect(s.folder_open)])
        restore()
        if 's8_files' in self.keys:
            self.files_tree(files)

    def subs_shot(self) -> None:
        """서브 결합 탭 — 인프라 XO 를 넣고 다시 계산해야 해서 따로 창을 만들어 찍는다. 샘플은 실측 서브 3개(SW1 DBA L ·
        SW2 DBA R · SW3 INFRA, 10-11 — 전에는 서브 1개라 SW2 · SW3 을 SW1 에서 합성했다)."""
        if 's9_subs' not in self.keys:
            return
        from PyQt6 import QtCore

        import secs.rewapi as rewapi
        from secs.wizard import WizardWindow, new_project

        project = new_project(fake=True)
        rew = rewapi.FakeREW(delay_s=0.0, use_sample_dolby=True)
        win = WizardWindow(project, rew, rew, rew, rew, project_path=self.tmp / 'subs.secs')
        win.ask_before_close = False
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.resize(*WINDOW)
        win.show()
        main, self.win = self.win, win
        try:
            self.settle()
            win.go(5)
            win.measure_page.load_sample()
            self.settle()
            win.go(9)
            s = win.calibration_page
            self.settle()
            s.set_infra('SW3', 25.0, 'LR24')
            self._calibrate(s)
            s.set_tab('subs')
            s.set_sub_role('SW1')
            self.settle()
            restore = self.tall()
            infra = self.union(self.rect(s.infra_combo), self.rect(s.infra_hz_spin), self.rect(s.infra_type_combo),
                               self.rect(s.infra_boost_spin))
            crop = self.union(self.rect(s.plot), self.content(s.sub_card),
                              *[self.rect(b) for b in s.tab_buttons.values() if b.isVisible()])
            self.shot('s9_subs', crop, [infra, self.rect(s.sub_mode_combo), self.rect(s.sub_compute_button),
                                        self.rect(s.plot)])
            restore()
        finally:
            self.win = main
            win.close()
            self.settle()

    def files_tree(self, files: list[str]) -> None:
        """내보낸 폴더 트리 — REW mdat · 카밀라 DSP 두 번 내보낸 결과(실제 파일 이름)."""
        if not files:
            self.missing['s8_files'] = '내보내기 결과 파일이 없음'
            return
        exports = sorted({p for f in files for p in Path(f).parents if p.name.startswith('SECS_')
                          and p.parent.parent.name == 'SECS_export'})
        base = exports[0].parent
        lines: list[tuple[int, str, bool]] = [(0, f'SECS_export/{base.name}', True)]
        marks = {}
        fir_dirs = ('fir', 'coeffs')           # 10-10 부터 카밀라 내보내기 = configs/ · coeffs/(CamillaGUI 폴더 모양)
        for run in exports:
            lines.append((1, run.name, True))
            # 경로 순서(깊이 우선) — 폴더 바로 아래에 그 안의 파일이 온다(configs/secs_x.yml 이 coeffs 아래로 섞이지 않게)
            for item in sorted(run.rglob('*'), key=lambda p: p.relative_to(run).parts):
                rel = item.relative_to(run)
                if any(part in fir_dirs for part in rel.parts[:-1]):
                    continue                    # FIR 폴더 안은 아래에서 두 개만
                depth = 1 + len(rel.parts)
                if item.is_dir():
                    if item.name in fir_dirs:
                        marks.setdefault('fir', len(lines))
                    lines.append((depth, item.name, True))
                    if item.name in fir_dirs:
                        wavs = sorted(x.name for x in item.iterdir())
                        for name in wavs[:2]:
                            lines.append((depth + 1, name, False))
                        if len(wavs) > 2:
                            more = len(wavs) - 2
                            lines.append((depth + 1, f'… 외 {more}개' if self.lang == 'ko' else f'… {more} more', False))
                elif not item.name.endswith('.raw'):
                    if item.suffix == '.mdat':
                        marks.setdefault('mdat', len(lines))
                    if item.suffix == '.yml':
                        marks.setdefault('yml', len(lines))
                    lines.append((depth, item.name, False))
        order = [marks.get('mdat', 1), marks.get('yml', 1), marks.get('fir', 1)]
        title = '내보낸 폴더' if self.lang == 'ko' else 'Exported folder'
        image, marks_px = draw_tree(lines, order, title)
        name = f's8_files_{self.lang}.png'
        size, dims = finish(image, marks_px, self.out / name, badge_left=True)
        # 창 캡처가 아니라 그린 그림 — crop 은 그림 전체, marks 는 그림 px(drawn: true 로 표시).
        self.record('s8_files', name, [0, 0, dims[0], dims[1]], [list(map(round, m)) for m in marks_px],
                    size, dims)
        self.manifest['s8_files']['drawn'] = True

    def sidebar(self) -> None:
        win = self.win
        win.go(1)           # 본문이 가장 단순한 화면(사이드바 값은 흐름을 다 돈 뒤라 1/1 위치)
        self.settle()
        side = win.help_button.parentWidget()
        while side is not None and side.objectName() != 'side':
            side = side.parentWidget()
        from PyQt6 import QtWidgets

        combo = next((c for c in side.findChildren(QtWidgets.QComboBox)), None)
        language = self.rect(combo) if combo is not None else self.rect(win.help_button)
        if combo is not None:
            for w in side.findChildren(QtWidgets.QLabel):
                if w.isVisible() and abs(self.rect(w).center().y() - language.center().y()) < 12 \
                        and self.rect(w).right() < language.x():
                    language = language.united(self.rect(w))
        # 체험 모드 띠가 보이는 상태로(10-09: 체험은 같은 창 + 노란 띠)
        win.set_trial(True)
        self.settle()
        save = self.union(self.rect(win.save_button), self.rect(win.save_more_button))
        bar = self.rect(win.steps_bar)
        steps = _qrect(bar.x(), bar.y(), bar.width() // 2, bar.height())
        crop = self.union(self.rect(side), steps)
        self.shot('side_nav', crop, [_qrect(steps.x() + 4, steps.y() + 4, steps.width() - 8, steps.height() - 8),
                                     self.rect(win.help_button), self.rect(win.trial_button), save])
        win.set_trial(False)
        self.settle()
        _ = language


def _qrect(x, y, w, h):
    from PyQt6 import QtCore

    return QtCore.QRect(int(x), int(y), int(w), int(h))


def _elapsed():
    from PyQt6 import QtCore

    timer = QtCore.QElapsedTimer()
    timer.start()
    return timer


def _manifest_text(manifest: dict) -> str:
    """MANIFEST.json — 사람이 훑어보기 쉽게 키 하나당 한 줄(값은 그대로 JSON)."""
    def one(value) -> str:
        return json.dumps(value, ensure_ascii=False, separators=(', ', ': '))

    lines = ['{']
    for name in ('generated', 'window', 'regenerate'):
        lines.append(f' {one(name)}: {one(manifest[name])},')
    for section in ('shots', 'missing'):
        items = list(manifest[section].items())
        lines.append(f' {one(section)}: {{')
        for index, (key, value) in enumerate(items):
            lines.append(f'  {one(key)}: {one(value)}' + (',' if index < len(items) - 1 else ''))
        lines.append(' }' + (',' if section == 'shots' else ''))
    lines.append('}')
    return '\n'.join(lines) + '\n'


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--lang', default='ko,en')
    ap.add_argument('--keys', default='', help='쉼표 목록(없으면 전부)')
    ap.add_argument('--out', default='', help=f'출력 폴더(기본 {OUT_DIR.relative_to(ROOT)})')
    args = ap.parse_args(argv)
    keys = {k.strip() for k in args.keys.split(',') if k.strip()} or set(KEYS)
    unknown = keys - set(KEYS)
    if unknown:
        ap.error('모르는 키: ' + ', '.join(sorted(unknown)))
    out = Path(args.out) if args.out else OUT_DIR
    out.mkdir(parents=True, exist_ok=True)

    from PyQt6.QtWidgets import QApplication

    # 사용자 설정(레지스트리)의 언어를 건드리지 않게 임시 ini 로 돈다.
    os.environ['SECS4_SETTINGS_PATH'] = str(Path(tempfile.mkdtemp(prefix='secs-tut-')) / 'settings.ini')
    app = QApplication.instance() or QApplication([])
    from secs.i18n import language_manager

    manifest_path = out / 'MANIFEST.json'
    try:
        manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    except (OSError, ValueError):
        manifest = {}
    shots = manifest.get('shots', {})
    missing = dict(manifest.get('missing', {}))
    for lang in [s.strip() for s in args.lang.split(',') if s.strip()]:
        print(f'[{lang}]')
        language_manager().set_language(lang, persist=False)
        with tempfile.TemporaryDirectory(prefix='secs-tut-') as tmp:
            shooter = Shooter(app, lang, keys, out, Path(tmp))
            shooter.run()
        for key, entry in shooter.manifest.items():
            old = shots.get(key, {})
            by_lang = {**old.get('by_lang', {}), lang: entry}
            files = {code: old[code] for code in ('ko', 'en') if code in old}
            files[lang] = entry.pop('file')
            # 계약 모양(ko·en·crop·marks) + 언어별 상세(영어는 글자 폭이 달라 강조 위치가 조금 다르다)
            shots[key] = {**{code: files[code] for code in ('ko', 'en') if code in files},
                          'crop': entry['crop'], 'marks': entry['marks'], 'by_lang': by_lang}
            missing.pop(key, None)
        for key, why in shooter.missing.items():
            missing[key] = why
    for key in KEYS:
        if key not in shots and key not in missing:
            missing[key] = '만들지 않음'
    manifest = {'generated': f'{_dt.date.today():%Y-%m-%d}', 'window': list(WINDOW),
                'regenerate': r'.venv-secs\Scripts\python.exe tools\make_tutorial_shots.py',
                # KEYS 밖의 키(F1 「mdat 불러오기」 그림 mdat_* — `make_mdat_help_shots.py` 가 만든다)는 그대로 둔다.
                'shots': {**{k: shots[k] for k in KEYS if k in shots},
                          **{k: v for k, v in shots.items() if k not in KEYS}},
                'missing': {k: missing[k] for k in KEYS if k in missing}}
    manifest_path.write_text(_manifest_text(manifest), encoding='utf-8')
    total = sum(p.stat().st_size for p in out.glob('*.png'))
    print(f'\n그림 {len(list(out.glob("*.png")))}장, 합계 {total / 1024 / 1024:.2f} MB → {out}')
    if manifest['missing']:
        print('없음:', manifest['missing'])
    return 0


if __name__ == '__main__':
    sys.exit(main())
