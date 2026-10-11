"""실사용 QA 캡처 도구 — 위저드 단계를 여러 창 크기로 띄워 화면을 저장하고 자동 점검한다.

REW·오디오 장치 없이 FakeREW 로 돈다(소리 안 남). **오프스크린 플랫폼을 쓰지 말 것** — 글꼴이
없어 글자가 전부 네모로 나오고 폭 계산이 틀린다. 실제 windows 플랫폼에서 돌리되 창은
`WA_DontShowOnScreen` 이라 화면에 뜨지 않고, 모니터보다 큰 크기도 잘리지 않는다.

    .venv-secs\\Scripts\\python.exe tools\\qa_capture.py --steps 2,9 --preset 2.1
    .venv-secs\\Scripts\\python.exe tools\\qa_capture.py --steps 9 --preset 9.1.6 --tag after

`--steps` 는 화면 번호다(기본 = 흐름 순서 1,2,3,4,5,9,8 — 9 = 보정 설계, 8 = 내보내기).

자동 점검(완전하지 않다 — 캡처를 눈으로도 볼 것):
  * 가로 스크롤: 단계 스크롤 영역의 가로 스크롤바 범위 > 0
  * 글자 잘림: 보이는 QLabel/QPushButton/QCheckBox/QRadioButton 의 글자 폭·높이 > 위젯 크기
  * 표 안 가로 스크롤: 화면 안 표·목록의 가로 스크롤바 범위 > 0(결과의 'inner', 2026-10-05)
  * 계산이 도는 화면(`wait()` 이 있는 단계)은 계산이 끝나기를 기다린 뒤 캡처한다.
결과: `보고서/실사용 QA <날짜>/step<N>_<프리셋>[_biamp]_<폭>x<높이>_<언어>[_<tag>].png` + 표준 출력에 표(마크다운).

추가 옵션(2026-09-24, 빌드 전 QA):
  --sample-dolby   FakeREW(use_sample_dolby=True) — 7.1.4 + 서브 3 원측정 role 에 돌비룸 실측 IR(샘플 모드와 같다)
  --help-dialog    단계마다 도움말 창(F1)도 캡처: help<N>_..._<창폭>x<창높이>.png(+ _bottom)
  --help-sizes     도움말 창 크기 목록(기본 640x720 = 기본 크기, 520x420 = 최소 크기)
  --loading        창 크기마다 로딩 오버레이(카드) 4종 + 구석 진행 패널 3종을 강제로 띄워 캡처
                   (loading<k>_... / busy<k>_...). 실제 계산이 아니라 문구·배치만 본다.
  --pages          세로로 긴 화면의 위·아래 사이도 한 화면씩(_mid1, _mid2 …)

    .venv-secs\\Scripts\\python.exe tools\\qa_capture.py --preset 7.1.4 --sample-dolby --lang en
        --sizes 1024x640,1920x1080 --help-dialog --loading --out <폴더>
"""
from __future__ import annotations

import argparse
import datetime as _dt
import os
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
os.environ.pop('QT_QPA_PLATFORM', None)      # 오프스크린 금지(위 설명)

#: CLAUDE.md "실사용 QA" 의 창 크기 세트(마지막은 극단적으로 좁은 세로).
SIZES = [(1024, 640), (1280, 800), (1920, 1080), (2560, 1440), (1024, 420)]


def build_window(preset: str, biamp: bool, positions: int, tmp: Path, sample_dolby: bool = False):
    import secs.model as model
    import secs.rewapi as rewapi
    from secs.wizard import WizardWindow

    project = model.Project(config=model.SpeakerConfig.from_preset(preset, biamp=biamp))
    # 샘플 실측은 1점뿐 — 샘플 모드(`wizard.new_project(fake=True)`)처럼 배치 1점(09-24).
    project.extra['array'] = 1 if sample_dolby else 9
    if sample_dolby and preset == '7.1.4':           # 샘플 실측은 서브 3개(10-11) — 샘플 모드 구성과 같게
        from secs.wizard import SAMPLE_SUBS
        project.config.set_sub_count(SAMPLE_SUBS)
        project.config.preset = preset
    rew = rewapi.FakeREW(delay_s=0.0, use_sample_dolby=sample_dolby)
    win = WizardWindow(project, rew, rew, rew, rew, project_path=tmp / 'qa.secs')
    win.ask_before_close = False
    return win


def measure(win, app, positions: int) -> None:
    win.go(5)
    for pos in range(1, positions + 1):
        if not win.step5.measure_position(pos):
            break
        win.step5.wait(120000)
        app.processEvents()


def settle(app, rounds: int = 6) -> None:
    for _ in range(rounds):
        app.processEvents()


def clipped_widgets(page) -> list[str]:
    """글자가 위젯보다 큰(잘렸을) 보이는 위젯의 설명 목록."""
    from PyQt6 import QtCore, QtWidgets

    found: list[str] = []
    kinds = (QtWidgets.QLabel, QtWidgets.QAbstractButton)
    for w in page.findChildren(QtWidgets.QWidget):
        if not isinstance(w, kinds) or not w.isVisible():
            continue
        text = w.text() if hasattr(w, 'text') else ''
        if not text or '<' in text:
            continue
        fm = w.fontMetrics()
        if isinstance(w, QtWidgets.QLabel) and w.wordWrap():
            need = fm.boundingRect(QtCore.QRect(0, 0, max(w.width(), 1), 10000),
                                   int(QtCore.Qt.TextFlag.TextWordWrap), text)
            if need.height() > w.height() + 2:
                found.append(f'세로 잘림 "{text[:24]}" ({need.height()}>{w.height()})')
            continue
        widest = max(fm.horizontalAdvance(line) for line in text.split('\n'))
        if widest > w.width() + 2:
            found.append(f'가로 잘림 "{text[:24]}" ({widest}>{w.width()})')
    return found


def hangul_texts(root) -> list[str]:
    """영어 화면에 남은 한글(번역 누락) — 라벨·버튼·콤보·표·목록·로그·툴팁·접미사를 훑는다."""
    import re

    from PyQt6 import QtWidgets

    hangul = re.compile(r'[가-힣]')
    seen: list[str] = []

    def note(kind: str, text) -> None:
        text = str(text or '')
        if hangul.search(text):
            entry = f'{kind}: {text[:70]!r}'
            if entry not in seen:
                seen.append(entry)

    from PyQt6 import sip

    for w in [root] + root.findChildren(QtWidgets.QWidget):
        if sip.isdeleted(w) or not w.isVisible():
            continue
        note('툴팁', w.toolTip())
        if isinstance(w, (QtWidgets.QLabel, QtWidgets.QAbstractButton)):
            note(type(w).__name__, w.text())
        elif isinstance(w, QtWidgets.QComboBox):
            for index in range(w.count()):
                note('콤보', QtWidgets.QComboBox.itemText(w, index))
        elif isinstance(w, QtWidgets.QAbstractSpinBox) and hasattr(w, 'suffix'):
            note('접미사', w.suffix())
        elif isinstance(w, QtWidgets.QTableWidget):
            for col in range(w.columnCount()):
                head = w.horizontalHeaderItem(col)
                note('표 머리', head.text() if head else '')
                for row in range(w.rowCount()):
                    item = w.item(row, col)
                    if item is not None:
                        note('표 셀', item.text())
                        note('표 셀 툴팁', item.toolTip())
        elif isinstance(w, QtWidgets.QListWidget):
            for row in range(w.count()):
                note('목록', w.item(row).text())
        elif isinstance(w, QtWidgets.QPlainTextEdit):
            for line in w.toPlainText().split('\n'):
                note('로그', line)
        elif isinstance(w, QtWidgets.QGraphicsView) and w.scene() is not None:
            for item in w.scene().items():
                if hasattr(item, 'text') and callable(item.text):
                    note('그림 글자', item.text())
                elif hasattr(item, 'toPlainText'):
                    note('그림 글자', item.toPlainText())
    return seen


def overflow_widgets(area) -> list[str]:
    """가로 스크롤의 원인 찾기 — 보이는 영역 오른쪽 끝을 넘는 글자 위젯(넘친 폭 순)."""
    from PyQt6 import QtCore, QtWidgets

    holder = area.widget()
    limit = area.viewport().width()
    rows = []
    for w in holder.findChildren(QtWidgets.QWidget):
        if not w.isVisible() or not isinstance(w, (QtWidgets.QLabel, QtWidgets.QAbstractButton,
                                                   QtWidgets.QComboBox)):
            continue
        right = w.mapTo(holder, QtCore.QPoint(w.width(), 0)).x()
        if right > limit:
            text = w.currentText() if isinstance(w, QtWidgets.QComboBox) else w.text()
            rows.append((right - limit, f'+{right - limit}px {type(w).__name__} "{text[:30]}" (폭 {w.width()})'))
    return [text for _over, text in sorted(rows, reverse=True)[:6]]


def inner_hscroll(page) -> list[str]:
    """화면 안의 표·목록이 제 안에서 가로로 넘치는 것(페이지 가로 스크롤과 따로 — 10-05 R5: 구성 단계 패치 표가
    1024 폭에서 표 안 가로 스크롤로 극성 칸을 가렸는데 페이지 점검에는 안 잡혔다). 그래프(QGraphicsView)는 뺀다."""
    from PyQt6 import QtWidgets

    found = []
    for area in page.findChildren(QtWidgets.QAbstractScrollArea):
        if isinstance(area, QtWidgets.QGraphicsView) or not area.isVisible():
            continue
        over = area.horizontalScrollBar().maximum()
        if over > 0:
            found.append(f'+{over}px {type(area).__name__} (폭 {area.width()})')
    return found


def layout_geometry(win) -> dict:
    """지금 단계의 2열 배치 위치(창 좌표, px): 제목 x·y, 본문 칸 x·y·폭, 오른쪽 칸 x·y·폭.

    `style.two_columns` 규칙(09-23)이 지켜지는지 — 같은 창 크기에서 단계마다 같아야 한다 — 를 본다.
    오른쪽 칸이 없거나(8단계) 본문 아래로 쌓였으면(좁은 창의 구성 단계) side_* 는 None.
    """
    from PyQt6 import QtCore, QtWidgets

    page = win.stack.currentWidget()

    def rect(widget):
        top_left = widget.mapTo(win, QtCore.QPoint(0, 0))
        return top_left.x(), top_left.y(), widget.width()

    heading = next((w for w in page.findChildren(QtWidgets.QLabel) if w.objectName() == 'h2'
                    and w.isVisible()), None)
    main = next((w for w in page.findChildren(QtWidgets.QWidget) if w.objectName() == 'mainColumn'), None)
    side = next((w for w in page.findChildren(QtWidgets.QWidget) if w.objectName() == 'sideColumn'), None)
    out = {'head_x': None, 'head_y': None, 'main_x': None, 'main_y': None, 'main_w': None,
           'side_x': None, 'side_y': None, 'side_w': None}
    if heading is not None:
        out['head_x'], out['head_y'], _w = rect(heading)
    if main is not None:
        out['main_x'], out['main_y'], out['main_w'] = rect(main)
    if side is not None and side.isVisible() and main is not None:
        x, y, w = rect(side)
        if y == out['main_y']:                  # 옆에 있을 때만(아래로 쌓였으면 오른쪽 칸이 아니다)
            out['side_x'], out['side_y'], out['side_w'] = x, y, w
    return out


def capture(win, app, step: int, size: tuple[int, int], out: Path, name: str,
            scan_text: bool = False, pages: bool = False) -> dict:
    win.resize(*size)
    win.go(step)
    settle(app)
    # 계산이 끝난 화면을 본다 — 계산이 끝나야 문구가 채워져 폭이 달라진다
    # (09-19: 계산을 안 기다려 가로 스크롤 118 px 를 놓쳤다).
    widget = win.step_widget()
    if widget is not None and hasattr(widget, 'wait'):
        widget.wait(180000)
        settle(app)
    # 로딩 막은 250 ms 타이머가 지운다 — 계산이 끝난 직후 캡처하면 막이 남아 화면을 가린다(09-24).
    if hasattr(win, '_poll_busy'):
        win._poll_busy()
        settle(app)
    area = win.stack.currentWidget()
    hbar = area.horizontalScrollBar() if hasattr(area, 'horizontalScrollBar') else None
    hscroll = hbar.maximum() if hbar is not None else 0
    clipped = clipped_widgets(area)
    if hscroll:
        clipped = clipped + overflow_widgets(area)
    inner = inner_hscroll(area.widget() if hasattr(area, 'widget') else area)
    path = out / f'{name}.png'
    win.grab().save(str(path))
    # 세로로 긴 화면은 아래쪽도 남긴다.
    vbar = area.verticalScrollBar() if hasattr(area, 'verticalScrollBar') else None
    if vbar is not None and vbar.maximum() > 0:
        if pages:
            # 위·아래 사이의 가운데 화면도 남긴다(한 화면 높이의 90 % 씩, 8단계처럼 긴 화면용).
            stride = max(1, int(vbar.pageStep() * 0.9))
            for k, value in enumerate(range(stride, vbar.maximum(), stride), 1):
                vbar.setValue(value)
                settle(app)
                win.grab().save(str(out / f'{name}_mid{k}.png'))
        vbar.setValue(vbar.maximum())
        settle(app)
        win.grab().save(str(out / f'{name}_bottom.png'))
        vbar.setValue(0)
    return {'name': name, 'hscroll': hscroll, 'clipped': clipped, 'inner': inner,
            'hangul': hangul_texts(win) if scan_text else [],
            'step': step, 'size': size, 'geometry': layout_geometry(win)}


def capture_help(win, app, out: Path, prefix: str, sizes) -> list[dict]:
    """도움말 창(비모달)을 지금 단계로 열어 크기마다 위·아래를 캡처한다."""
    from PyQt6 import QtCore

    rows = []
    if getattr(win, '_help_dialog', None) is None:
        from secs.wizard.help_dialog import HelpDialog
        win._help_dialog = HelpDialog(win)
    dialog = win._help_dialog
    dialog.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    win.show_help()
    for w, h in sizes:
        dialog.resize(w, h)
        settle(app)
        if hasattr(dialog, 'flush_resize'):     # 그림 폭 맞춤은 120 ms 디바운스 — 캡처 전에 끝낸다
            dialog.flush_resize()
            settle(app)
        name = f'{prefix}_{w}x{h}'
        bar = dialog.browser.verticalScrollBar()
        bar.setValue(0)
        settle(app)
        dialog.grab().save(str(out / f'{name}.png'))
        hbar = dialog.browser.horizontalScrollBar().maximum()
        clipped = clipped_widgets(dialog)
        if bar.maximum() > 0:
            bar.setValue(bar.maximum())
            settle(app)
            dialog.grab().save(str(out / f'{name}_bottom.png'))
            bar.setValue(0)
        rows.append({'name': name, 'hscroll': hbar, 'clipped': clipped, 'hangul': []})
    dialog.close()
    settle(app)
    return rows


def capture_loading(win, app, out: Path, prefix: str) -> list[dict]:
    """로딩 오버레이(카드)와 구석 진행 패널을 작업 문구마다 강제로 띄워 캡처한다.

    폴링 타이머가 '안 돎' 으로 보고 바로 지우지 않게 잠깐 멈춘다. 예상 시간·취소 버튼이 있는
    가장 긴 모양으로 그린다.
    """
    rows = []
    win._busy_timer.stop()
    try:
        for k, (what, _running, _estimate, _cancel) in enumerate(win._overlay_jobs, 1):
            win.loading.clear()
            win.loading.want(what, 12.0, lambda: None)
            win.loading._show()
            settle(app)
            name = f'loading{k}_{prefix}'
            win.grab().save(str(out / f'{name}.png'))
            rows.append({'name': name, 'hscroll': 0, 'clipped': clipped_widgets(win.loading), 'hangul': []})
            win.loading.clear()
        for k, (what, _running, _estimate) in enumerate(win._busy_jobs, 1):
            win.busy_panel.start(what, 125.0)
            settle(app)
            name = f'busy{k}_{prefix}'
            win.grab().save(str(out / f'{name}.png'))
            rows.append({'name': name, 'hscroll': 0, 'clipped': clipped_widgets(win.busy_panel), 'hangul': []})
            win.busy_panel.stop()
        settle(app)
    finally:
        win._busy_timer.start()
    return rows


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--steps', default='1,2,3,4,5,9,8')   # 화면 번호, 흐름 순서(9 = 보정 설계, 8 = 내보내기)
    ap.add_argument('--preset', default='2.1')
    ap.add_argument('--biamp', action='store_true')
    ap.add_argument('--positions', type=int, default=2, help='Fake 측정 위치 수(0 이면 측정 없이)')
    ap.add_argument('--tag', default='')
    ap.add_argument('--out', default='')
    ap.add_argument('--lang', default='ko', choices=('ko', 'en'),
                    help='화면 언어. en 이면 남은 한글(번역 누락)도 같이 보고한다')
    ap.add_argument('--sizes', default='', help='예: 1024x640,1280x800 (없으면 전체 세트)')
    ap.add_argument('--geometry', action='store_true',
                    help='단계별 2열 배치 위치 표(제목·본문·오른쪽 칸 x/y/폭)도 출력')
    ap.add_argument('--sample-dolby', action='store_true',
                    help='FakeREW 가 7.1.4 원측정 role 에 돌비룸 실측 IR 을 준다(샘플 모드와 같다)')
    ap.add_argument('--pages', action='store_true',
                    help='세로 스크롤 화면의 위·아래 사이도 한 화면씩 캡처(_mid1, _mid2 …)')
    ap.add_argument('--help-dialog', action='store_true', help='단계마다 도움말 창도 캡처')
    ap.add_argument('--help-sizes', default='640x720,520x420', help='도움말 창 크기 목록')
    ap.add_argument('--loading', action='store_true',
                    help='창 크기마다 로딩 오버레이·구석 진행 패널을 강제로 띄워 캡처')
    args = ap.parse_args(argv)

    from PyQt6 import QtCore
    from PyQt6.QtWidgets import QApplication

    # 사용자 설정(레지스트리)의 언어를 건드리지 않게 임시 ini 로 돈다.
    os.environ['SECS4_SETTINGS_PATH'] = str(Path(tempfile.mkdtemp(prefix='secs-qa-')) / 'settings.ini')
    app = QApplication.instance() or QApplication([])
    from secs.i18n import language_manager
    language_manager().set_language(args.lang, persist=False)
    sizes = [tuple(int(v) for v in s.split('x')) for s in args.sizes.split(',') if s.strip()] or SIZES
    out = Path(args.out) if args.out else ROOT / '보고서' / '05 화면 QA' / f'{_dt.date.today():%Y-%m-%d} 실사용 QA'
    out.mkdir(parents=True, exist_ok=True)
    steps = [int(s) for s in args.steps.split(',') if s.strip()]

    rows = []
    with tempfile.TemporaryDirectory() as tmp:
        win = build_window(args.preset, args.biamp, args.positions, Path(tmp),
                           sample_dolby=args.sample_dolby)
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.show()
        if args.positions > 0:
            # 샘플 FakeREW 는 위치 2 이상을 막는다(실측 1점뿐) — 1 로 자른다.
            measure(win, app, 1 if args.sample_dolby else args.positions)
        tag = f'_{args.tag}' if args.tag else ''
        label = args.preset + ('_biamp' if args.biamp else '')
        help_sizes = [tuple(int(v) for v in s.split('x')) for s in args.help_sizes.split(',') if s.strip()]
        for step in steps:
            for size in sizes:
                name = f'step{step}_{label}_{size[0]}x{size[1]}_{args.lang}{tag}'
                rows.append(capture(win, app, step, size, out, name, scan_text=args.lang == 'en', pages=args.pages))
            if args.help_dialog:
                rows.extend(capture_help(win, app, out, f'help{step}_{label}_{args.lang}{tag}', help_sizes))
        if args.loading:
            for size in sizes:
                win.resize(*size)
                settle(app)
                rows.extend(capture_loading(win, app, out, f'{label}_{size[0]}x{size[1]}_{args.lang}{tag}'))
        win.close()
        settle(app)

    print('| 캡처 | 가로 스크롤(px) | 잘림 의심 |')
    print('|---|---|---|')
    for row in rows:
        clipped = '<br>'.join(row['clipped'][:8] + [f'표 안 {text}' for text in row.get('inner', [])]) or '-'
        print(f"| {row['name']} | {row['hscroll']} | {clipped} |")
    if args.geometry:
        print('\n| 창 크기 | 단계 | 제목 x,y | 본문 x,y,폭 | 오른쪽 칸 x,y,폭 |')
        print('|---|---|---|---|---|')
        for row in sorted(rows, key=lambda r: (r['size'], r['step'])):
            g = row['geometry']
            side = f"{g['side_x']}, {g['side_y']}, {g['side_w']}" if g['side_x'] is not None else '—'
            print(f"| {row['size'][0]}x{row['size'][1]} | {row['step']} | {g['head_x']}, {g['head_y']} | "
                  f"{g['main_x']}, {g['main_y']}, {g['main_w']} | {side} |")
    bad = sum(1 for r in rows if r['hscroll'] or r['clipped'] or r.get('inner'))
    print(f'\n캡처 {len(rows)}장, 의심 {bad}장 → {out}')
    if args.lang == 'en':
        leftovers: list[str] = []
        for row in rows:
            for entry in row['hangul']:
                line = f"{row['name'].split('_')[0]} {entry}"
                if line not in leftovers:
                    leftovers.append(line)
        print(f'\n영어 화면에 남은 한글 {len(leftovers)}건')
        for line in leftovers:
            print('  ' + line)
    return 0


if __name__ == '__main__':
    sys.exit(main())
