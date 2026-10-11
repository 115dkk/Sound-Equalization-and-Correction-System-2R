"""F1 도움말 내보내기 「CamillaDSP」 탭의 그림 11장 만들기 (2026-10-10).

사용자 요청: "다른 사람이 윈도우나 맥에서 카밀라 GUI 를 통해 설정을 할 텐데, 아무것도 없는 상태에서 뭘 어떻게 넣어야 라우팅이
되고 세팅이 되는지 … 내보내기 도움말에 카밀라DSP 탭 하나 만들어서 자세히 써 줘. 그림 많이 넣고." 읽는 사람은 대충 훑어보므로
그림이 본문이다. 키는 `tutorial_text.CAMILLA_CAPTIONS` 와 같다(계약 그림 27개 밖이라 `make_tutorial_shots.KEYS` 에 넣지 않는다):

  cam_kit        준비물 넷(엔진 · GUI · 루프백 · 인터페이스)
  cam_path       소리 길 — 윈도우 · 맥 두 줄(플레이어 → 루프백 → CamillaDSP → 인터페이스 → 스피커)
  cam_folder     카밀라 폴더(bin · gui · configs · coeffs)
  cam_export     SECS 가 내보낸 폴더 → 카밀라 폴더(configs → configs, coeffs → coeffs, ../coeffs 상대 경로)
  cam_start      켜는 순서 — 카밀라 창 · GUI 창 · 브라우저 주소
  cam_files      GUI Files 탭 — wav 먼저(Filters ⬆) · yml(Configs ⬆) · Valid · ★
  cam_devices    GUI Devices 탭 — samplerate · chunksize · 캡처 = 루프백 · 재생 = 인터페이스 · Rate adjust
  cam_order      내보낸 설정 파일 맨 위의 순서표(진짜 `export_camilladsp.howto_lines` 출력, 5.1 예)
  cam_route_in   Mixers 행렬 route_in — 플레이어가 LFE 를 4번째로 보낼 때
  cam_route_out  Mixers 행렬 route_out — 인터페이스 출력 4 = 서브, 5 · 6 = 서라운드, out 8
  cam_check      GUI 왼쪽 패널 — Apply to DSP · State · 미터 · Clipped samples · Save to file
  cam_latency    모드별 지연과 영상 싱크

GUI 화면은 그대로 베끼지 않고 단순화해 그렸다(QPainter) — 칸 이름은 CamillaGUI 4.1.0 프런트 소스에서 읽은 것
(`문서/조사/CamillaDSP GUI 적용 조사 2026-10-10.md` 3 · 5 · 6절). 버전에 따라 이름 · 자리가 조금 다를 수 있다고 탭에 적었다.
채널 예시는 바뀌지 않을 이름만(L R C Ls Rs SW1 · LFE).

    .venv-secs\\Scripts\\python.exe tools\\make_camilla_help_shots.py            # ko·en 전부
    .venv-secs\\Scripts\\python.exe tools\\make_camilla_help_shots.py --lang en --keys cam_devices

`make_mdat_help_shots.py` 와 같은 원칙: 오프스크린 플랫폼 금지(글꼴), 창은 띄우지 않는다(QImage 에만 그림). 출력은
`src/secs/wizard/assets/tutorial/<key>_<lang>.png` + `MANIFEST.json` 에 키 추가(다른 키는 그대로).
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
os.environ.pop('QT_QPA_PLATFORM', None)      # 오프스크린 금지(글꼴이 없다)

import make_mdat_help_shots as M             # noqa: E402 - 경로 설정 뒤에(그리기 도우미 · 색 · 저장을 같이 쓴다)

OUT_DIR = M.OUT_DIR
KEYS = ('cam_kit', 'cam_path', 'cam_folder', 'cam_export', 'cam_start', 'cam_files', 'cam_devices', 'cam_order',
        'cam_route_in', 'cam_route_out', 'cam_check', 'cam_latency')
INK, INK2, INK3, ACCENT, LINE, HEAD_BG, SEL_BG, PANEL = M.INK, M.INK2, M.INK3, M.ACCENT, M.LINE, M.HEAD_BG, M.SEL_BG, M.PANEL
GREEN, AMBER, CONSOLE, CONSOLE_INK = '#2f9e44', '#e08a00', '#1f2a29', '#e8f1ef'

#: 5.1 예시(바뀌지 않을 이름만) — SECS 입력은 LFE 가 맨 뒤, 플레이어는 LFE 가 4번째(윈도우 · 대부분의 플레이어).
SECS_IN = ['L', 'R', 'C', 'Ls', 'Rs', 'LFE']
SECS_OUT = ['L', 'R', 'C', 'Ls', 'Rs', 'SW1']
PLAYER = ['L', 'R', 'C', 'LFE', 'Ls', 'Rs']
ROUTE_IN = {0: 0, 1: 1, 2: 2, 3: 4, 4: 5, 5: 3}          # SECS 입력 k ← 플레이어 채널
ROUTE_OUT = {0: 0, 1: 1, 2: 2, 3: 4, 4: 5, 5: 3}         # SECS 출력 k → 인터페이스 출력(0부터): Ls 5 · Rs 6 · SW1 4
N_IFACE = 8

TEXT = {
    'ko': {
        'kit': [('CamillaDSP', '소리를 처리하는 엔진', 'camilladsp 4.1.3'),
                ('CamillaGUI', '브라우저로 여는 설정 화면', 'camillagui_backend 4.1.0'),
                ('루프백', '플레이어 소리를 받는 가상 장치', '윈도우 VB-CABLE · 맥 BlackHole'),
                ('오디오 인터페이스', '스피커로 나가는 장치', '지금 쓰는 그대로')],
        'win': '윈도우', 'mac': '맥',
        'player': ('플레이어', '→ 루프백'), 'dsp': ('CamillaDSP', ''), 'iface': ('인터페이스', ''), 'spk': ('스피커', ''),
        'inside': 'CamillaDSP 안:', 'secs': 'SECS 보정', 'rate': '루프백 · 인터페이스 모두 48 kHz',
        'tree_root': 'camilladsp', 'tree_where': '윈도우 사용자 폴더 · 맥 ~/',
        'tree': [('bin', '엔진(camilladsp)을 풀어 넣음'), ('gui', 'GUI(camillagui_backend)를 풀어 넣음'),
                 ('configs', '설정 yml'), ('coeffs', 'FIR wav'), ('statefile.yml', '카밀라가 만듦')],
        'must': '꼭 미리 만들기',
        'export_head': 'SECS 가 내보낸 폴더', 'home_head': '카밀라 폴더', 'more': '… 채널마다 하나',
        'check_only': 'SECS 검사용',
        'eng': '카밀라 엔진 — 이 창은 열어 둡니다', 'gui': 'GUI 서버', 'browser': '브라우저',
        'run_in': 'camilladsp 폴더에서', 'win_p': '윈도우>', 'mac_p': '맥 $',
        'none': '(비움)', 'win_only': '윈도우만', 'your_iface': 'Speakers (내 인터페이스)',
        'player_order': '플레이어가 보내는 순서', 'secs_in': 'SECS 입력', 'secs_out': 'SECS 출력',
        'iface_out': '인터페이스', 'out_n': '출력 {0}', 'silent': '무음',
        'axis_in': '가로 = 들어오는 채널 (Input)', 'axis_out': '세로 = 나가는 채널 (Output)',
        'modes': [('최소 지연', 0, 0), ('저지연', 10, 10), ('표준', 30, 30), ('초고지연', 30, 200)],
        'buffer': '+ 카밀라 버퍼', 'video': '영상 플레이어 › 오디오 싱크', 'video_val': '−200 ms',
        'video_note': '초고지연 200 ms 일 때', 'order_title': 'secs_mixed.yml 맨 위',
    },
    'en': {
        'kit': [('CamillaDSP', 'The engine that processes sound', 'camilladsp 4.1.3'),
                ('CamillaGUI', 'Settings page in the browser', 'camillagui_backend 4.1.0'),
                ('Loopback', 'Virtual device that takes the player', 'Windows VB-CABLE · Mac BlackHole'),
                ('Audio interface', 'The device that feeds the speakers', 'the one you use now')],
        'win': 'Windows', 'mac': 'Mac',
        'player': ('Player', '→ loopback'), 'dsp': ('CamillaDSP', ''), 'iface': ('Interface', ''),
        'spk': ('Speakers', ''),
        'inside': 'Inside CamillaDSP:', 'secs': 'SECS correction', 'rate': '48 kHz on loopback and interface',
        'tree_root': 'camilladsp', 'tree_where': 'Windows user folder · Mac ~/',
        'tree': [('bin', 'unzip the engine (camilladsp) here'), ('gui', 'unzip the GUI (camillagui_backend) here'),
                 ('configs', 'config yml'), ('coeffs', 'FIR wav'), ('statefile.yml', 'made by CamillaDSP')],
        'must': 'create first',
        'export_head': 'Folder exported by SECS', 'home_head': 'camilladsp folder', 'more': '… one per channel',
        'check_only': 'SECS check only',
        'eng': 'CamillaDSP engine — keep this window open', 'gui': 'GUI server', 'browser': 'Browser',
        'run_in': 'in the camilladsp folder', 'win_p': 'Windows>', 'mac_p': 'Mac $',
        'none': '(empty)', 'win_only': 'Windows only', 'your_iface': 'Speakers (your interface)',
        'player_order': 'Order the player sends', 'secs_in': 'SECS input', 'secs_out': 'SECS output',
        'iface_out': 'Interface', 'out_n': 'Out {0}', 'silent': 'silent',
        'axis_in': 'Across = channels coming in (Input)', 'axis_out': 'Down = channels going out (Output)',
        'modes': [('Minimum latency', 0, 0), ('Low latency', 10, 10), ('Standard', 30, 30), ('Ultra latency', 30, 200)],
        'buffer': '+ CamillaDSP buffer', 'video': 'Video player › audio sync', 'video_val': '−200 ms',
        'video_note': 'with ultra latency 200 ms', 'order_title': 'top of secs_mixed.yml',
    },
}


# ──────────────────────────── 그리기 도우미(mdat 도구 것 + 몇 개) ────────────────────────────
def _qt():
    from PyQt6 import QtCore, QtGui

    return QtCore, QtGui


def font(px: int, bold: bool = False, latin: bool = False):
    return M.make_font(px, bold, latin)


def mono(px: int, bold: bool = False):
    _core, gui = _qt()
    f = gui.QFont()
    f.setFamilies(['Consolas', 'Cascadia Mono', 'Menlo', 'Courier New'])
    f.setPixelSize(px)
    f.setBold(bold)
    f.setStyleHint(gui.QFont.StyleHint.Monospace)
    return f


def tw(f, text: str) -> float:
    return M.text_width(f, text)


def text(p, x, y_mid, s, f, color=INK) -> float:
    return M.draw_text(p, x, y_mid, s, f, color)


def center(p, x, y, w, h, s, f, color=INK) -> None:
    core, gui = _qt()
    p.setFont(f)
    p.setPen(gui.QColor(color))
    p.drawText(core.QRectF(x, y, w, h), int(core.Qt.AlignmentFlag.AlignCenter), s)


def box(p, x, y, w, h, fill='#ffffff', border=LINE, radius=6, width=1.2) -> None:
    M.rounded(p, x, y, w, h, fill, border, radius, width)


def line(p, x0, y0, x1, y1, color=LINE, width=1.4, dashed=False) -> None:
    core, gui = _qt()
    pen = gui.QPen(gui.QColor(color), width)
    pen.setCapStyle(core.Qt.PenCapStyle.RoundCap)
    if dashed:
        pen.setStyle(core.Qt.PenStyle.DashLine)
    p.setPen(pen)
    p.drawLine(core.QPointF(x0, y0), core.QPointF(x1, y1))


def arrow_to(p, x0, y0, x1, y1, color=INK2, width=2.2, dashed=False, head=11) -> None:
    """(x0,y0) → (x1,y1) 화살표(아무 방향)."""
    import math

    core, gui = _qt()
    ang = math.atan2(y1 - y0, x1 - x0)
    bx, by = x1 - head * 0.8 * math.cos(ang), y1 - head * 0.8 * math.sin(ang)
    line(p, x0, y0, bx, by, color, width, dashed)
    p.setPen(core.Qt.PenStyle.NoPen)
    p.setBrush(gui.QColor(color))
    pts = [core.QPointF(x1, y1),
           core.QPointF(x1 - head * math.cos(ang - 0.45), y1 - head * math.sin(ang - 0.45)),
           core.QPointF(x1 - head * math.cos(ang + 0.45), y1 - head * math.sin(ang + 0.45))]
    p.drawPolygon(gui.QPolygonF(pts))


def folder_icon(p, x, y, color=ACCENT, w=24, h=18) -> None:
    core, gui = _qt()
    path = gui.QPainterPath()
    path.moveTo(x, y + 3)
    path.lineTo(x + w * 0.38, y + 3)
    path.lineTo(x + w * 0.48, y)
    path.lineTo(x + w, y)
    path.lineTo(x + w, y + h)
    path.lineTo(x, y + h)
    path.closeSubpath()
    p.setPen(gui.QPen(gui.QColor(color), 1.4))
    p.setBrush(gui.QColor(SEL_BG))
    p.drawPath(path)


def file_icon(p, x, y, color=INK2, w=16, h=20) -> None:
    core, gui = _qt()
    path = gui.QPainterPath()
    path.moveTo(x, y)
    path.lineTo(x + w - 5, y)
    path.lineTo(x + w, y + 5)
    path.lineTo(x + w, y + h)
    path.lineTo(x, y + h)
    path.closeSubpath()
    p.setPen(gui.QPen(gui.QColor(color), 1.3))
    p.setBrush(gui.QColor('#ffffff'))
    p.drawPath(path)


def star(p, cx, cy, r, color=AMBER, filled=True) -> None:
    import math

    core, gui = _qt()
    pts = []
    for i in range(10):
        rr = r if i % 2 == 0 else r * 0.45
        a = -math.pi / 2 + i * math.pi / 5
        pts.append(core.QPointF(cx + rr * math.cos(a), cy + rr * math.sin(a)))
    p.setPen(gui.QPen(gui.QColor(color), 1.4))
    p.setBrush(gui.QColor(color) if filled else core.Qt.BrushStyle.NoBrush)
    p.drawPolygon(gui.QPolygonF(pts))


def upload_icon(p, x, y, s=20, color=INK) -> None:
    line(p, x + s / 2, y + s * 0.78, x + s / 2, y + s * 0.18, color, 2.2)
    line(p, x + s / 2, y + s * 0.15, x + s * 0.22, y + s * 0.42, color, 2.2)
    line(p, x + s / 2, y + s * 0.15, x + s * 0.78, y + s * 0.42, color, 2.2)
    line(p, x + s * 0.12, y + s * 0.95, x + s * 0.88, y + s * 0.95, color, 2.2)


def check_icon(p, x, y, s=18, color=GREEN) -> None:
    line(p, x + s * 0.1, y + s * 0.55, x + s * 0.4, y + s * 0.85, color, 3)
    line(p, x + s * 0.4, y + s * 0.85, x + s * 0.95, y + s * 0.15, color, 3)


def magnifier(p, x, y, s=18, color=INK2) -> None:
    core, gui = _qt()
    p.setPen(gui.QPen(gui.QColor(color), 2))
    p.setBrush(core.Qt.BrushStyle.NoBrush)
    p.drawEllipse(core.QPointF(x + s * 0.42, y + s * 0.42), s * 0.3, s * 0.3)
    line(p, x + s * 0.64, y + s * 0.64, x + s * 0.95, y + s * 0.95, color, 2.4)


def speaker_icon(p, x, y, s=26, color=INK2) -> None:
    core, gui = _qt()
    p.setPen(gui.QPen(gui.QColor(color), 1.6))
    p.setBrush(gui.QColor('#ffffff'))
    p.drawRoundedRect(core.QRectF(x, y, s * 0.7, s), 3, 3)
    p.setBrush(core.Qt.BrushStyle.NoBrush)
    p.drawEllipse(core.QPointF(x + s * 0.35, y + s * 0.66), s * 0.2, s * 0.2)
    p.drawEllipse(core.QPointF(x + s * 0.35, y + s * 0.24), s * 0.09, s * 0.09)


def field(p, x, y, w, label, value, lab_w=None, h=30, value_font=None, hint=None) -> tuple[float, float, float, float]:
    """'라벨 [값]' 한 칸 — 값 상자 사각형을 돌려준다."""
    lf = font(15, latin=True)
    lab_w = lab_w if lab_w is not None else tw(lf, label) + 10
    text(p, x, y + h / 2, label, lf, INK2)
    vx = x + lab_w
    box(p, vx, y, w - lab_w, h, '#ffffff', LINE, radius=4)
    vf = value_font or font(15, latin=True)
    if value:
        text(p, vx + 8, y + h / 2, value, vf, INK if not hint else INK3)
    return (vx, y, w - lab_w, h)


def checkbox(p, x, y, on=True, s=18) -> None:
    box(p, x, y, s, s, ACCENT if on else '#ffffff', ACCENT if on else LINE, radius=3)
    if on:
        check_icon(p, x + 2, y + 2, s - 4, '#ffffff')


def group(p, x, y, w, h, title) -> None:
    box(p, x, y, w, h, '#ffffff', LINE, radius=6)
    f = font(15, bold=True, latin=True)
    box(p, x + 12, y - 10, tw(f, title) + 12, 20, '#ffffff', None, radius=2)
    text(p, x + 18, y, title, f, INK)


def tab_strip(p, x, y, active: str) -> float:
    """CamillaGUI 탭 줄(4.1.0 순서) — 오른쪽 끝 x."""
    f = font(15, latin=True)
    for name in ('Title', 'Devices', 'Filters', 'Mixers', 'Processors', 'Pipeline', 'Files', 'Shortcuts'):
        w = tw(f, name) + 18
        on = name == active
        box(p, x, y, w, 28, SEL_BG if on else '#ffffff', ACCENT if on else LINE, radius=4, width=1.4 if on else 1.0)
        center(p, x, y, w, 28, name, font(15, bold=on, latin=True), INK if on else INK2)
        x += w + 4
    return x


def canvas(width: int, height: int):
    return M.new_canvas(width, height)


def done(p, image, width, height, marks):
    for number, rect in enumerate(marks, 1):
        M.mark(p, rect, number)
    M.frame(p, width, height)
    p.end()
    return image, marks


# ──────────────────────────── 그림들 ────────────────────────────
def draw_kit(lang: str):
    t = TEXT[lang]
    width, cw, ch, gap = 720, 330, 112, 20
    height = 20 + 2 * ch + gap + 20
    image, p = canvas(width, height)
    marks = []
    for i, (name, role, files) in enumerate(t['kit']):
        x = 20 + (i % 2) * (cw + gap)
        y = 20 + (i // 2) * (ch + gap)
        box(p, x, y, cw, ch, '#f7faf9', LINE, radius=8)
        if i == 2:
            folder_icon(p, x + 18, y + 20, ACCENT, 26, 20)
        elif i == 3:
            speaker_icon(p, x + 20, y + 16, 30)
        else:
            box(p, x + 18, y + 18, 26, 26, ACCENT, None, radius=5)
            center(p, x + 18, y + 18, 26, 26, 'C' if i == 0 else 'G', font(16, bold=True, latin=True), '#ffffff')
        text(p, x + 58, y + 31, name, font(21, bold=True, latin=lang == 'en' or i < 2), INK)
        text(p, x + 20, y + 66, role, font(17), INK2)
        text(p, x + 20, y + 92, files, font(15, latin=True), INK3)
        marks.append([x + 4, y + 4, cw - 8, ch - 8])
    return done(p, image, width, height, marks)


def draw_path(lang: str):
    t = TEXT[lang]
    width = 800
    lane_h, top = 92, 18
    height = top + 2 * lane_h + 96
    image, p = canvas(width, height)
    lane_font = font(19, bold=True, latin=lang == 'en')
    x0, gap = 18 + max(tw(lane_font, t['win']), tw(lane_font, t['mac'])) + 16, 24
    bw = (width - 16 - x0 - 4 * gap) / 5
    cols = [x0 + k * (bw + gap) for k in range(5)]
    lanes = [(t['win'], ['VB-CABLE', 'Wasapi']), (t['mac'], ['BlackHole 16ch', 'CoreAudio'])]
    for li, (os_name, (loop, api)) in enumerate(lanes):
        y = top + li * lane_h
        text(p, 18, y + 34, os_name, lane_font, INK)
        labels = [(t['player'][0], t['player'][1]), (loop, 'loopback'), ('CamillaDSP', api), (t['iface'][0], ''),
                  (t['spk'][0], '')]
        for k, (a, b) in enumerate(labels):
            x = cols[k]
            fill = SEL_BG if k == 2 else '#ffffff'
            box(p, x, y, bw, 68, fill, ACCENT if k == 2 else LINE, radius=8, width=1.6 if k == 2 else 1.2)
            if k == 4:
                speaker_icon(p, x + bw / 2 - 30, y + 8, 28)
                speaker_icon(p, x + bw / 2 + 6, y + 8, 28)
                center(p, x, y + 40, bw, 24, a, font(14, bold=True, latin=lang == 'en'), INK)
                continue
            fa = font(16 if len(a) < 12 else 14, bold=True, latin=k in (1, 2))
            center(p, x, y + 6, bw, 30, a, fa, INK)
            if b:
                center(p, x, y + 34, bw, 24, b, font(13, latin=True), INK2)
            if k < 4:
                M.arrow(p, x + bw + 3, x + bw + gap - 2, y + 34)
    # CamillaDSP 안: route_in → SECS 보정 → route_out
    y = top + 2 * lane_h + 6
    fx = font(16, bold=True, latin=True)
    parts = [t['inside'], 'route_in', '→', t['secs'], '→', 'route_out']
    fonts = [font(15, latin=lang == 'en') if s == t['inside'] else (fx if s != '→' else font(17, bold=True)) for s in parts]
    w = sum(tw(f, s) + 10 for f, s in zip(fonts, parts)) + 24
    x = cols[2] + bw / 2 - w / 2
    box(p, x, y, w, 48, '#f7faf9', ACCENT, radius=8, width=1.4)
    cx = x + 14
    for s, f in zip(parts, fonts):
        cx = text(p, cx, y + 24, s, f, INK2 if s in (t['inside'], '→') else INK) + 10
    # 48 kHz 표시
    rf = font(14, bold=True, latin=lang == 'en')
    pw = tw(rf, t['rate']) + 18
    box(p, width - pw - 18, y + 62, pw, 24, '#fff5e6', AMBER, radius=12)
    center(p, width - pw - 18, y + 62, pw, 24, t['rate'], rf, INK)
    marks = [[cols[1] - 5, top - 5, bw + 10, lane_h + 68 + 10], [cols[2] - 5, top - 5, bw + 10, lane_h + 68 + 10],
             [cols[3] - 5, top - 5, bw + 10, lane_h + 68 + 10]]
    return done(p, image, width, height, marks)


def draw_folder(lang: str):
    t = TEXT[lang]
    width, row_h, top = 700, 46, 22
    rows = t['tree']
    height = top + (len(rows) + 1) * row_h + 18
    image, p = canvas(width, height)
    folder_icon(p, 28, top + 12, ACCENT, 28, 21)
    text(p, 66, top + 22, t['tree_root'], font(21, bold=True, latin=True), INK)
    text(p, 210, top + 22, t['tree_where'], font(15, latin=lang == 'en'), INK3)
    nf, df = font(19, bold=True, latin=True), font(16, latin=lang == 'en')
    for i, (name, desc) in enumerate(rows):
        y = top + (i + 1) * row_h
        line(p, 42, y - row_h + 34, 42, y + 22, LINE, 1.6)
        line(p, 42, y + 22, 66, y + 22, LINE, 1.6)
        if name.endswith('.yml'):
            file_icon(p, 74, y + 11)
        else:
            folder_icon(p, 72, y + 13)
        text(p, 106, y + 22, name, nf, INK)
        text(p, 260, y + 22, desc, df, INK2)
        if name in ('configs', 'coeffs'):
            star(p, 244, y + 22, 8)
    y_conf = top + 3 * row_h
    mf = font(15, bold=True, latin=lang == 'en')
    mw = tw(mf, t['must']) + 20
    box(p, width - mw - 30, y_conf + 30, mw, 26, '#fff5e6', AMBER, radius=13)
    center(p, width - mw - 30, y_conf + 30, mw, 26, t['must'], mf, INK)
    marks = [[60, top + row_h + 2, width - 90, 2 * row_h - 4], [60, top + 3 * row_h + 2, width - 90, 2 * row_h - 4]]
    return done(p, image, width, height, marks)


def draw_export(lang: str):
    t = TEXT[lang]
    width, row_h, top = 760, 36, 52
    tree = [(0, 'camilladsp', 'dir', ''), (1, 'configs', 'dir', ''), (2, 'secs_mixed.yml', 'file', ''),
            (1, 'coeffs', 'dir', ''), (2, 'secs_mixed_L_fir.wav', 'file', ''), (2, 'secs_mixed_R_fir.wav', 'file', ''),
            (2, t['more'], 'more', ''), (1, 'in.raw', 'file', t['check_only'])]
    height = top + len(tree) * row_h + 26
    image, p = canvas(width, height)
    text(p, 22, 26, t['export_head'], font(18, bold=True), INK)
    text(p, 22 + tw(font(18, bold=True), t['export_head']) + 12, 26, 'SECS_camilla_…_mixed_…', font(14, latin=True), INK3)
    nf = font(16, bold=True, latin=True)
    ys = {}
    for i, (depth, name, kind, note) in enumerate(tree):
        y = top + i * row_h
        x = 26 + depth * 28
        if kind == 'dir':
            folder_icon(p, x, y + 9)
        elif kind == 'file':
            file_icon(p, x + 3, y + 7)
        text(p, x + 32, y + 18, name, nf if kind != 'more' else font(15, latin=lang == 'en'),
             INK if kind != 'more' else INK3)
        if note:
            text(p, x + 32 + tw(nf, name) + 12, y + 18, note, font(14, latin=lang == 'en'), INK3)
        ys[name] = y + 18
    # 오른쪽: 카밀라 폴더
    rx = 520
    text(p, rx, 26, t['home_head'], font(18, bold=True), INK)
    box(p, rx - 10, top - 4, 228, 4 * row_h, '#f7faf9', LINE, radius=8)
    folder_icon(p, rx, top + 9)
    text(p, rx + 32, top + 18, 'camilladsp', nf, INK)
    home = {}
    for j, name in enumerate(('configs', 'coeffs')):
        y = top + (j + 1) * row_h + (row_h * 0.5 if j else 0)
        folder_icon(p, rx + 28, y + 9)
        text(p, rx + 60, y + 18, name, nf, INK)
        home[name] = y + 18
    for name in ('configs', 'coeffs'):
        sx = 26 + 28 + 32 + tw(nf, name) + 14
        arrow_to(p, sx, ys[name], rx + 22, home[name], ACCENT, 2.6)
    # ../coeffs 상대 경로: 카밀라 폴더 안에서 configs → coeffs(점선) — 설정 안 FIR 경로가 가리키는 곳
    ax = rx + 60 + tw(nf, 'configs') + 16
    arrow_to(p, ax, home['configs'], ax, home['coeffs'] - 12, INK3, 1.6, dashed=True, head=9)
    text(p, ax + 8, (home['configs'] + home['coeffs']) / 2 - 2, '../coeffs/', font(14, latin=True), INK2)
    marks = [[20, ys['configs'] - 16, 470, 2 * row_h - 8], [20, ys['coeffs'] - 14, 470, 4 * row_h - 6],
             [44, ys['in.raw'] - 15, 300 if lang == 'ko' else 330, row_h - 6]]
    return done(p, image, width, height, marks)


def draw_start(lang: str):
    t = TEXT[lang]
    width = 760
    mf = mono(15)
    blocks = [
        (t['eng'], [(t['win_p'], r'bin\camilladsp.exe -p1234 -w -s statefile.yml -g-20'),
                    (t['mac_p'], 'bin/camilladsp -p1234 -w -s statefile.yml -g-20')]),
        (t['gui'], [(t['win_p'], r'gui\camillagui_backend\camillagui_backend.exe'),
                    (t['mac_p'], 'cd gui/camillagui_backend && ./camillagui_backend')]),
    ]
    bh, gap, top = 108, 18, 18
    height = top + 2 * (bh + gap) + 84 + 18
    image, p = canvas(width, height)
    marks = []
    for i, (title, rows) in enumerate(blocks):
        y = top + i * (bh + gap)
        box(p, 20, y, width - 40, bh, CONSOLE, None, radius=8)
        box(p, 20, y, width - 40, 30, '#33413f', None, radius=8)
        text(p, 36, y + 15, title, font(15, bold=True, latin=lang == 'en'), CONSOLE_INK)
        if i == 0:
            text(p, width - 40 - tw(font(13, latin=lang == 'en'), t['run_in']) - 10, y + 15, t['run_in'],
                 font(13, latin=lang == 'en'), '#9fb5b2')
        for j, (prompt, cmd) in enumerate(rows):
            yy = y + 52 + j * 30
            text(p, 36, yy, prompt, font(14, bold=True, latin=lang == 'en'), '#9fb5b2')
            text(p, 36 + (84 if lang == 'ko' else 92), yy, cmd, mf, CONSOLE_INK)
        marks.append([24, y + 4, width - 48, bh - 8])
    y = top + 2 * (bh + gap)
    box(p, 20, y, width - 40, 84, '#ffffff', LINE, radius=8)
    text(p, 36, y + 22, t['browser'], font(15, bold=True, latin=lang == 'en'), INK2)
    box(p, 36, y + 42, width - 72, 30, '#f2f6f5', LINE, radius=15)
    text(p, 54, y + 57, 'http://localhost:5005/gui/index.html', mono(16, bold=True), INK)
    marks.append([30, y + 38, width - 60, 38])
    return done(p, image, width, height, marks)


def draw_files(lang: str):
    width = 760
    height = 344
    image, p = canvas(width, height)
    tab_strip(p, 18, 14, 'Files')
    hf, rf = font(17, bold=True, latin=True), font(15, latin=True)
    marks = []

    def table(y, title, rows, configs: bool):
        box(p, 18, y, width - 36, 34 + 34 * len(rows), '#ffffff', LINE, radius=6)
        box(p, 19, y + 1, width - 38, 33, HEAD_BG, None, radius=5)
        text(p, 32, y + 17, title, hf, INK)
        ux = width - 220
        upload_icon(p, ux, y + 7, 20)
        text(p, ux + 28, y + 17, 'Upload files', rf, INK)
        up = [ux - 6, y + 3, 140, 28]
        cells = []
        for k, row in enumerate(rows):
            yy = y + 34 + 34 * k
            line(p, 19, yy, width - 19, yy, '#dde6e4', 1)
            if configs:
                star(p, 40, yy + 17, 9, AMBER, filled=False)
                file_icon(p, 60, yy + 7, INK3, 14, 18)
                text(p, 66 + 24, yy + 17, '↗', font(16, bold=True, latin=True), INK2)
                text(p, 112, yy + 17, row[0], rf, INK)
                text(p, 300, yy + 17, row[1], rf, INK2)
                check_icon(p, 470, yy + 8, 18)
                text(p, 494, yy + 17, 'Valid', font(14, latin=True), INK3)
                cells = [[30, yy + 5, 22, 24], [464, yy + 4, 80, 26]]
            else:
                file_icon(p, 36, yy + 7, INK3, 14, 18)
                text(p, 60, yy + 17, row[0], rf, INK)
                text(p, 300, yy + 17, row[1], rf, INK3)
        return up, cells

    up_c, cells = table(64, 'Configs', [('secs_mixed.yml', 'SECS 2 mixed (9.6 ms)')], True)
    up_f, _ = table(152, 'Filters', [('secs_mixed_L_fir.wav', '48000 Hz'), ('secs_mixed_R_fir.wav', '48000 Hz'),
                                     ('secs_mixed_C_fir.wav', '48000 Hz'), ('…', '')], False)
    marks = [up_f, up_c, cells[1], cells[0]]
    return done(p, image, width, height, marks)


def draw_devices(lang: str):
    t = TEXT[lang]
    width = 760
    height = 470
    image, p = canvas(width, height)
    tab_strip(p, 18, 14, 'Devices')
    marks = []
    y = 70
    group(p, 18, y, 340, 58, 'Buffers')
    field(p, 32, y + 16, 150, 'samplerate', '48000')
    field(p, 196, y + 16, 150, 'chunksize', '1024')
    marks.append([24, y + 8, 330, 44])
    group(p, 376, y, 366, 58, 'Rate adjust')
    checkbox(p, 392, y + 22)
    text(p, 418, y + 31, 'enable_rate_adjust', font(15, latin=True), INK)
    y2 = y + 76
    group(p, 376, y2, 366, 58, 'Resampling')
    field(p, 392, y2 + 16, 220, 'type', 'AsyncSinc')
    text(p, 622, y2 + 31, t['win_only'], font(14, bold=True, latin=lang == 'en'), AMBER)
    marks.append([382, y + 6, 354, y2 + 52 - y])
    for k, (title, dev, ch) in enumerate((('Capture device', 'CABLE Output (VB-Audio Virtual Cable)', '6'),
                                         ('Playback device', t['your_iface'], '8'))):
        gy = 230 + k * 118
        group(p, 18, gy, width - 36, 100, title)
        field(p, 32, gy + 18, 210, 'type', 'Wasapi  ▾', lab_w=48)
        field(p, 256, gy + 18, 130, 'channels', ch)
        field(p, 400, gy + 18, 160, 'format', t['none'], hint=True)
        r = field(p, 32, gy + 58, width - 110, 'device', dev, lab_w=60)
        magnifier(p, r[0] + r[2] + 10, gy + 63, 20)
        marks.append([24, gy + 8, width - 48, 86])
    marks = [marks[0], marks[2], marks[3], marks[1]]
    return done(p, image, width, height, marks)


def order_lines() -> list[str]:
    """진짜 내보내기 머리 주석에서 순서표 부분만(5.1 예) — 표 뒤의 긴 영어 줄은 그림을 넓혀 글자가 작아지므로 뺀다."""
    from secs.export_camilladsp import howto_lines

    lines = howto_lines(SECS_IN, SECS_OUT, 9.6, 'mixed')
    start = next(i for i, s in enumerate(lines) if s.startswith('# 3)'))
    end = next(i for i, s in enumerate(lines) if i > start and not (s[4:7].strip().isdigit() or 'input (capture' in s))
    return lines[start:end]


def draw_order(lang: str):
    t = TEXT[lang]
    lines = order_lines()
    mf = mono(15)
    cw = tw(mf, 'x' * 40) / 40
    width = int(min(900, 40 + cw * max(len(s) for s in lines) + 30))
    lh, top = 24, 44
    height = top + lh * len(lines) + 20
    image, p = canvas(width, height)
    text(p, 22, 22, t['order_title'], font(16, bold=True, latin=lang == 'en'), INK2)
    box(p, 14, top - 6, width - 28, lh * len(lines) + 12, '#f7faf9', LINE, radius=6)
    rows = []
    for i, s in enumerate(lines):
        y = top + i * lh + lh / 2
        text(p, 26, y, s, mf, INK if s[4:7].strip().isdigit() else INK2)
        if s[4:7].strip().isdigit():
            rows.append(y)
    head = next(s for s in lines if 'input (capture' in s)
    xi, xo = 26 + cw * head.index('input'), 26 + cw * head.index('output')
    y0, y1 = rows[0] - lh / 2 - lh, rows[-1] + lh / 2
    marks = [[xi - 6, y0, cw * 31, y1 - y0 + 2], [xo - 6, y0, cw * 33, y1 - y0 + 2]]
    return done(p, image, width, height, marks)


def matrix(p, x, y, cols: list[str], rows: list[str], links: dict[int, int], changed: set[int], cell=50,
           col_title='', row_title='', row_names=None):
    """CamillaGUI Mixers 행렬 모양: 가로 = 입력(Input) 번호, 세로 = 출력(Output) 번호. links = {출력: 입력}.
    돌려주는 것: (열 머리 사각형, 바뀐 행 사각형, 오른쪽 끝 x, 아래 끝 y)."""
    nf = font(15, bold=True, latin=True)
    lf = font(15, bold=True, latin=True)
    row_w = 110
    gx, gy = x + row_w, y + 58
    text(p, gx + 22, y + 6, col_title, font(14, latin=False), INK3)     # 번호 배지(왼쪽 위)와 안 겹치게
    for c, name in enumerate(cols):
        cx = gx + c * cell
        center(p, cx, y + 20, cell, 18, str(c), nf, INK2)
        center(p, cx, y + 38, cell, 18, name, lf, ACCENT)
    for r, name in enumerate(rows):
        ry = gy + r * cell
        center(p, x, ry, 30, cell, str(r), nf, INK2)
        text(p, x + 34, ry + cell / 2, name, lf, INK if not row_names else INK)
        if row_names:
            text(p, x + 34, ry + cell / 2 + 15, row_names[r], font(12, latin=True), INK3)
        for c in range(len(cols)):
            cx = gx + c * cell
            on = links.get(r) == c
            box(p, cx + 3, ry + 3, cell - 6, cell - 6, (AMBER if r in changed else ACCENT) if on else '#ffffff',
                None if on else '#dde6e4', radius=5, width=1)
            center(p, cx + 3, ry + 3, cell - 6, cell - 6, '0 dB' if on else '+', font(12 if on else 16, bold=on, latin=True),
                   '#ffffff' if on else '#c3cfcd')
    right, bottom = gx + len(cols) * cell, gy + len(rows) * cell
    head = [gx - 4, y + 16, len(cols) * cell + 8, 44]
    ch = sorted(changed)
    moved = [x - 4, gy + ch[0] * cell, right - x + 8, (ch[-1] - ch[0] + 1) * cell] if ch else None
    return head, moved, right, bottom


def mixer_head(p, x, y, name, n_in, n_out, width):
    box(p, x, y, width, 40, HEAD_BG, LINE, radius=6)
    text(p, x + 14, y + 20, name, font(17, bold=True, latin=True), INK)
    f = font(15, latin=True)
    ix = x + 160
    text(p, ix, y + 20, 'in', f, INK2)
    box(p, ix + 22, y + 7, 44, 26, '#ffffff', LINE, radius=4)
    center(p, ix + 22, y + 7, 44, 26, str(n_in), font(15, bold=True, latin=True), INK)
    text(p, ix + 82, y + 20, 'out', f, INK2)
    box(p, ix + 112, y + 7, 44, 26, '#ffffff', LINE, radius=4)
    center(p, ix + 112, y + 7, 44, 26, str(n_out), font(15, bold=True, latin=True), INK)
    return [ix + 106, y + 3, 56, 34]


def draw_route_in(lang: str):
    t = TEXT[lang]
    width = 600
    cell = 50
    height = 64 + 58 + 6 * cell + 66
    image, p = canvas(width, height)
    mixer_head(p, 16, 14, 'route_in', 6, 6, width - 32)
    head, moved, right, bottom = matrix(p, 24, 64, PLAYER, SECS_IN, ROUTE_IN, {3, 4, 5}, cell,
                                        col_title=t['player_order'])
    text(p, 24, bottom + 20, t['axis_in'], font(14, latin=lang == 'en'), INK3)
    text(p, 24, bottom + 42, t['axis_out'] + ' = ' + t['secs_in'], font(14, latin=lang == 'en'), INK3)
    marks = [head, moved]
    return done(p, image, width, height, marks)


def draw_route_out(lang: str):
    t = TEXT[lang]
    width = 600
    cell = 46
    rows = [str(k + 1) for k in range(N_IFACE)]
    names = [t['out_n'].format(k + 1) for k in range(N_IFACE)]
    height = 64 + 58 + N_IFACE * cell + 66
    image, p = canvas(width, height)
    out_box = mixer_head(p, 16, 14, 'route_out', 6, N_IFACE, width - 32)
    links = {ROUTE_OUT[k]: k for k in range(len(SECS_OUT))}       # 행 = 인터페이스 출력, 값 = SECS 출력
    changed = {ROUTE_OUT[k] for k in (3, 4, 5)}
    head, moved, right, bottom = matrix(p, 24, 64, SECS_OUT, names, links, changed, cell, col_title=t['secs_out'])
    for r in range(len(SECS_OUT), N_IFACE):
        text(p, right + 10, 64 + 58 + r * cell + cell / 2, t['silent'], font(13, latin=lang == 'en'), INK3)
    text(p, 24, bottom + 20, t['axis_in'] + ' = ' + t['secs_out'], font(14, latin=lang == 'en'), INK3)
    text(p, 24, bottom + 42, t['axis_out'] + ' = ' + t['iface_out'], font(14, latin=lang == 'en'), INK3)
    marks = [out_box, moved]
    del rows
    return done(p, image, width, height, marks)


def draw_check(lang: str):
    width, height = 560, 560
    image, p = canvas(width, height)
    px, pw = 20, 360
    box(p, px, 16, pw, height - 32, PANEL, LINE, radius=8)
    # 볼륨
    text(p, px + 16, 40, 'Volume', font(15, bold=True, latin=True), INK)
    line(p, px + 92, 40, px + pw - 80, 40, LINE, 4)
    line(p, px + 92, 40, px + 190, 40, ACCENT, 4)
    box(p, px + 184, 31, 12, 18, '#ffffff', ACCENT, radius=3)
    text(p, px + pw - 70, 40, '-20 dB', font(14, latin=True), INK2)
    # 미터
    mtop = 64
    meters = []
    for k, (label, n, levels) in enumerate((('In', 6, [0.7, 0.68, 0.5, 0.3, 0.35, 0.2]),
                                           ('Out', 6, [0.66, 0.64, 0.55, 0.4, 0.42, 0.85]))):
        y = mtop + k * 64
        text(p, px + 16, y + 26, label, font(15, bold=True, latin=True), INK)
        for c in range(n):
            by = y + 6 + c * 8
            box(p, px + 60, by, pw - 80, 6, '#e3ebe9', None, radius=2)
            col = GREEN if levels[c] < 0.8 else AMBER
            box(p, px + 60, by, (pw - 80) * levels[c], 6, col, None, radius=2)
        meters.append([px + 8, y, pw - 16, 56])
    # CamillaDSP 상자
    by0 = mtop + 140
    box(p, px + 10, by0, pw - 20, 176, '#ffffff', LINE, radius=6)
    text(p, px + 22, by0 + 18, 'CamillaDSP', font(16, bold=True, latin=True), INK)
    rows = [('State', 'RUNNING', GREEN), ('Capt. samplerate', '48000', INK), ('Rate adjust', '1.0001', INK),
            ('Clipped samples', '0', INK), ('Buffer level', '1012', INK), ('DSP load', '9 %', INK)]
    rects = {}
    for i, (k, v, c) in enumerate(rows):
        y = by0 + 44 + i * 21
        text(p, px + 22, y, k, font(14, latin=True), INK2)
        text(p, px + 200, y, v, font(14, bold=True, latin=True), c)
        rects[k] = [px + 16, y - 11, pw - 40, 22]
    # Config 상자
    cy0 = by0 + 190
    box(p, px + 10, cy0, pw - 20, 140, '#ffffff', LINE, radius=6)
    text(p, px + 22, cy0 + 18, 'Config', font(16, bold=True, latin=True), INK)
    text(p, px + 90, cy0 + 18, 'secs_mixed.yml', font(14, latin=True), INK2)
    bf = font(14, bold=True, latin=True)
    buttons = {}
    for i, name in enumerate(('Fetch from DSP', 'Apply to DSP', 'Save to file', 'Apply and save', 'Stop processing')):
        bx = px + 22 + (i % 2) * 158
        byy = cy0 + 38 + (i // 2) * 34
        on = name in ('Apply to DSP', 'Save to file')
        box(p, bx, byy, 150, 28, ACCENT if on else '#ffffff', ACCENT if on else LINE, radius=5)
        center(p, bx, byy, 150, 28, name, bf, '#ffffff' if on else INK2)
        buttons[name] = [bx - 3, byy - 3, 156, 34]
    # 오른쪽 옆: 한 채널씩 확인
    sx = px + pw + 30
    for k in range(3):
        speaker_icon(p, sx + k * 44, 120, 34, ACCENT if k == 1 else INK3)
    text(p, sx - 6, 176, 'L  ·  R  ·  C …', font(15, bold=True, latin=True), INK2)
    marks = [buttons['Apply to DSP'], rects['State'], [meters[0][0], meters[0][1], meters[0][2], 120],
             rects['Clipped samples'], buttons['Save to file']]
    return done(p, image, width, height, marks)


def draw_latency(lang: str):
    t = TEXT[lang]
    width, row_h, top = 700, 46, 24
    modes = t['modes']
    height = top + len(modes) * row_h + 128
    image, p = canvas(width, height)
    x0, scale = 190, 1.9            # px / ms (0 … 200 ms → 380 px, 뒤에 '30 – 200 ms' 글자 자리)
    lf = font(17, bold=True, latin=lang == 'en')
    for i, (name, lo, hi) in enumerate(modes):
        y = top + i * row_h
        text(p, 22, y + 18, name, lf, INK)
        w = max(4.0, hi * scale)
        box(p, x0, y + 6, w, 24, ACCENT if lo == hi else SEL_BG, ACCENT, radius=4)
        if lo != hi:
            box(p, x0, y + 6, lo * scale, 24, ACCENT, None, radius=4)
        label = f'{hi:g} ms' if lo == hi else f'{lo:g} – {hi:g} ms'
        text(p, x0 + w + 10, y + 18, label, font(16, bold=True, latin=True), INK2)
    # 버퍼(모드와 따로 더해짐)
    y = top + len(modes) * row_h
    box(p, x0, y + 4, 60, 20, '#ffffff', INK3, radius=4)
    for k in range(6):
        line(p, x0 + 6 + k * 10, y + 22, x0 + 14 + k * 10, y + 6, INK3, 1.2)
    text(p, x0 + 70, y + 14, t['buffer'] + ' (chunksize)', font(14, latin=lang == 'en'), INK2)
    # 영상 플레이어
    vy = y + 44
    box(p, 22, vy, width - 44, 56, '#ffffff', LINE, radius=8)
    box(p, 36, vy + 14, 40, 28, CONSOLE, None, radius=4)
    core, gui = _qt()
    p.setBrush(gui.QColor('#ffffff'))
    p.setPen(core.Qt.PenStyle.NoPen)
    p.drawPolygon(gui.QPolygonF([core.QPointF(51, vy + 20), core.QPointF(51, vy + 36), core.QPointF(64, vy + 28)]))
    text(p, 92, vy + 28, t['video'], font(17, bold=True, latin=lang == 'en'), INK)
    vf = font(20, bold=True, latin=True)
    vx = width - 44 - tw(vf, t['video_val']) - 160
    box(p, vx - 10, vy + 12, tw(vf, t['video_val']) + 20, 32, '#fff5e6', AMBER, radius=5)
    text(p, vx, vy + 28, t['video_val'], vf, INK)
    text(p, vx + tw(vf, t['video_val']) + 20, vy + 28, t['video_note'], font(13, latin=lang == 'en'), INK3)
    marks = [[vx - 14, vy + 8, tw(vf, t['video_val']) + 28, 40]]
    return done(p, image, width, height, marks)


JOBS = {'cam_kit': draw_kit, 'cam_path': draw_path, 'cam_folder': draw_folder, 'cam_export': draw_export,
        'cam_start': draw_start, 'cam_files': draw_files, 'cam_devices': draw_devices, 'cam_order': draw_order,
        'cam_route_in': draw_route_in, 'cam_route_out': draw_route_out, 'cam_check': draw_check,
        'cam_latency': draw_latency}


# ──────────────────────────── 저장 · MANIFEST ────────────────────────────
def run_language(lang: str, out: Path, keys: set[str]) -> dict:
    entries = {}
    for key in KEYS:
        if key not in keys:
            continue
        image, marks = JOBS[key](lang)
        name = f'{key}_{lang}.png'
        size, dims = M.save(image, out / name)
        entries[key] = {'file': name, 'crop': [0, 0, dims[0], dims[1]],
                        'marks': [[round(v) for v in m] for m in marks], 'size': list(dims), 'bytes': size, 'drawn': True}
        print(f'  {name:26s} {dims[0]}x{dims[1]}  {size / 1024:6.1f} KB  marks={len(marks)}')
    return entries


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

    os.environ['SECS4_SETTINGS_PATH'] = str(Path(tempfile.mkdtemp(prefix='secs-cam-')) / 'settings.ini')
    app = QApplication.instance() or QApplication([])           # noqa: F841 - 글꼴 · QImage 그리기에 필요
    manifest_path = out / 'MANIFEST.json'
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    shots = manifest['shots']
    for lang in [s.strip() for s in args.lang.split(',') if s.strip()]:
        print(f'[{lang}]')
        for key, entry in run_language(lang, out, keys).items():
            old = shots.get(key, {})
            by_lang = {**old.get('by_lang', {}), lang: {k: v for k, v in entry.items() if k != 'file'}}
            files = {code: old[code] for code in ('ko', 'en') if code in old}
            files[lang] = entry['file']
            shots[key] = {**{code: files[code] for code in ('ko', 'en') if code in files},
                          'crop': entry['crop'], 'marks': entry['marks'], 'by_lang': by_lang}
    manifest['generated'] = f'{_dt.date.today():%Y-%m-%d}'
    manifest_path.write_text(M.base._manifest_text(manifest), encoding='utf-8')
    return 0


if __name__ == '__main__':
    sys.exit(main())
