"""실사용 QA B — 5단계 몽키 테스트(2026-09-23, 사용자 지시: "각종 버튼들 다 눌러보고
사이즈 줄여도 보고 늘려도 보고 ui 망가지는거 없는지 기능 문제 없는지 철저하게").

2026-10-10 슬림화: 옛 6·7·8′ 단계(필터 설계·저역 통합·시트/Tesira/검증) 크롤은 그 화면과 함께 뺐다 —
남은 것은 5단계 측정 크롤과 5 · 보정 설계 · 내보내기 화면의 언어 전환·창 크기 왕복이다. 아래 설명의 6~8단계
조작은 옛 판 기록이다.

FakeREW·가짜 Tesira·시뮬레이션 검증으로 실장비 없이 5~8단계의 모든 버튼·체크·드롭다운·
슬라이더·스핀박스·그래프 드래그(제어점·커튼·XO)·우클릭 메뉴·더블클릭을 조작하고, 순서 밖
조작(측정 중 취소·단계 이동·재측정, 계산 중 값 연타, 위상 모드·FIR 자동 분배, 7단계
경로 매트릭스 on/off 로 IIR/하이브리드/FIR 전환, 저장→불러오기, 언어 전환)을 시도한다.

각 조작 뒤 (1) 예외(sys.excepthook)·Qt 경고(qInstallMessageHandler), (2) UI 스레드
하트비트(계산 중 processEvents 간격 > 200 ms), (3) 가로 스크롤·글자 잘림
(`tools.qa_capture`), (4) NaN·비물리적 딜레이(>100 ms)·게인 폭주를 점검한다. 발견은
`보고서/실사용 QA 2026-09-23 B.md` 표로, 스크린샷은 `보고서/실사용 QA 2026-09-23/B_*.png`.

    .venv-secs\\Scripts\\python.exe tools\\qa_monkey_b.py

src\\secs 는 고치지 않는다 — 이 도구는 찾기만 한다(수정은 리드가 모은다).
"""
from __future__ import annotations

import argparse
import datetime as _dt
import itertools
import os
import sys
import tempfile
import traceback
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
sys.path.insert(0, str(Path(__file__).resolve().parent))
os.environ['QT_QPA_PLATFORM'] = 'windows'   # 실제 windows 플랫폼(오프스크린은 글꼴이 없어 폭이 틀린다)
os.environ['SECS4_SETTINGS_PATH'] = str(Path(tempfile.mkdtemp(prefix='secs-qa-b-')) / 'settings.ini')

import qa_capture  # noqa: E402 - sys.path 조작 뒤 임포트


# ──────────────────────────── 기록 ────────────────────────────
@dataclass
class Issue:
    screen: str
    size: str
    lang: str
    action: str
    symptom: str
    repro: str
    severity: str          # 상 | 중 | 하
    suspect: str = ''


ISSUES: list[Issue] = []
EXCEPTIONS: list[str] = []
QT_WARNINGS: list[str] = []
_CTX = {'screen': '?', 'size': '?', 'lang': 'ko', 'action': '?'}


def note(screen: str = None, size: str = None, lang: str = None, action: str = None) -> None:
    if screen is not None:
        _CTX['screen'] = screen
    if size is not None:
        _CTX['size'] = size
    if lang is not None:
        _CTX['lang'] = lang
    if action is not None:
        _CTX['action'] = action


def record(symptom: str, severity: str = '중', repro: str = '', suspect: str = '') -> None:
    issue = Issue(_CTX['screen'], _CTX['size'], _CTX['lang'], _CTX['action'],
                  symptom, repro or _CTX['action'], severity, suspect)
    ISSUES.append(issue)
    print(f'[발견 {severity}] {issue.screen} · {issue.size} · {issue.lang} · {issue.action} → {symptom}')


def _excepthook(etype, value, tb) -> None:
    text = ''.join(traceback.format_exception(etype, value, tb))
    EXCEPTIONS.append(f"{_CTX['screen']}/{_CTX['action']}: {text}")
    record(f'예외 발생: {etype.__name__}: {value}', '상', suspect=text.strip().splitlines()[-2] if len(text.splitlines()) > 1 else '')
    print('[EXCEPTHOOK]', text[-800:])


sys.excepthook = _excepthook


def install_qt_message_handler() -> None:
    from PyQt6 import QtCore

    def handler(mode, _context, message) -> None:
        text = str(message)
        QT_WARNINGS.append(f"{_CTX['screen']}/{_CTX['action']}: {text}")
        if 'QWindowsWindow::setGeometry' in text or 'Populating font family aliases' in text:
            return                                   # 알려진 무해 경고(창 크기 극단값)
        record(f'Qt 경고: {text}', '하')

    QtCore.qInstallMessageHandler(handler)


MENU_LOG: list[list[str]] = []


def install_dialog_patches():
    """QFileDialog·QMenu·QMessageBox 를 자동 응답으로 바꾼다(모달이 스크립트를 막지 않게)."""
    from PyQt6 import QtWidgets

    tmp_dir = Path(tempfile.mkdtemp(prefix='secs-qa-b-dialog-'))

    def _save(*_args, **_kwargs):
        return (str(tmp_dir / 'qa_target.json'), '')

    def _open_cancel(*_args, **_kwargs):
        return ('', '')

    QtWidgets.QFileDialog.getSaveFileName = staticmethod(_save)
    QtWidgets.QFileDialog.getOpenFileName = staticmethod(_open_cancel)

    def _flatten(menu) -> list:
        out = []
        for action in menu.actions():
            if action.isSeparator():
                continue
            sub = action.menu()
            if sub is not None:
                out.extend(_flatten(sub))
            elif action.isEnabled():
                out.append(action)
        return out

    counter = itertools.count()

    def _exec(self, *_args, **_kwargs):
        actions = _flatten(self)
        MENU_LOG.append([a.text() for a in actions])
        if not actions:
            return None
        chosen = actions[next(counter) % len(actions)]
        chosen.trigger()
        return chosen

    QtWidgets.QMenu.exec = _exec

    def _msgbox_exec(self):
        record('예상 밖 QMessageBox 가 떴다(자동으로 닫음)', '중')
        self.done(0)
        return 0

    QtWidgets.QMessageBox.exec = _msgbox_exec
    return tmp_dir


# ──────────────────────────── 도우미 ────────────────────────────
def settle(app, rounds: int = 4) -> None:
    for _ in range(rounds):
        app.processEvents()


def pump_with_heartbeat(app, predicate, timeout_ms: int = 60000, tick_ms: int = 40) -> int:
    """`predicate()` 가 참인 동안 이벤트를 돌리며 한 처리 사이 최대 간격(ms)을 돌려준다."""
    from PyQt6 import QtCore

    timer = QtCore.QElapsedTimer()
    timer.start()
    max_gap = 0
    last = timer.elapsed()
    while predicate() and timer.elapsed() < timeout_ms:
        app.processEvents(QtCore.QEventLoop.ProcessEventsFlag.AllEvents, tick_ms)
        now = timer.elapsed()
        max_gap = max(max_gap, now - last)
        last = now
        QtCore.QThread.msleep(1)
    return max_gap


def check_heartbeat(gap_ms: int, tag: str) -> None:
    # 이 측정은 배경 스레드 결과가 도착한 순간 뒤처리(표·그래프 갱신)까지 한 tick 에
    # 잡힐 수 있어 300~400 ms 는 흔히 오탐이다(09-23 확인) — 뚜렷한 정지만 '중', 나머지는
    # '하' 로 남겨 재확인 대상임을 표시한다. 초 단위(≥1000 ms)면 진짜 멈춤으로 본다.
    if gap_ms >= 1000:
        record(f'UI 스레드가 {gap_ms} ms 동안 멈춤(>=1 s) · {tag}', '상')
    elif gap_ms > 400:
        record(f'UI 스레드 하트비트 지연 {gap_ms} ms (>400 ms) · {tag}', '중')
    elif gap_ms > 200:
        record(f'UI 스레드 하트비트 지연 {gap_ms} ms (>200 ms, 재확인 필요) · {tag}', '하')


def _mouse_event(kind, pos, button, buttons):
    """`QMouseEvent` 를 만든다. pyqtgraph 의 `GraphicsView`(QAbstractScrollArea)에는
    `QTest.mouse*` 가 안 먹는다 — 뷰포트 이원화 때문에 `_drag` 상태가 반영 안 되는 것을
    이 스크립트가 직접 확인했다(09-23, `tools/qa_monkey_b.py` 개발 중 발견). 위젯의
    이벤트 메서드를 직접 불러 실제 클릭·드래그와 같은 코드 경로를 통과시킨다."""
    from PyQt6 import QtCore
    from PyQt6.QtGui import QMouseEvent

    point = QtCore.QPointF(pos)
    return QMouseEvent(kind, point, point, button, buttons, QtCore.Qt.KeyboardModifier.NoModifier)


def drag(widget, app, start, end) -> None:
    """제어점·커튼·XO 드래그(위젯의 mouse*Event 를 직접 불러서, 위 `_mouse_event` 참고)."""
    from PyQt6 import QtCore

    button = QtCore.Qt.MouseButton.LeftButton
    widget.mousePressEvent(_mouse_event(QtCore.QEvent.Type.MouseButtonPress, start, button, button))
    settle(app)
    mid = QtCore.QPoint((start.x() + end.x()) // 2, (start.y() + end.y()) // 2)
    widget.mouseMoveEvent(_mouse_event(QtCore.QEvent.Type.MouseMove, mid,
                                       QtCore.Qt.MouseButton.NoButton, button))
    settle(app)
    widget.mouseMoveEvent(_mouse_event(QtCore.QEvent.Type.MouseMove, end,
                                       QtCore.Qt.MouseButton.NoButton, button))
    settle(app)
    widget.mouseReleaseEvent(_mouse_event(QtCore.QEvent.Type.MouseButtonRelease, end, button,
                                          QtCore.Qt.MouseButton.NoButton))
    settle(app)


def check_layout(area, win, out: Path, tag: str) -> None:
    clipped = qa_capture.clipped_widgets(area)
    hbar = area.horizontalScrollBar() if hasattr(area, 'horizontalScrollBar') else None
    hscroll = hbar.maximum() if hbar is not None else 0
    if hscroll:
        clipped = clipped + qa_capture.overflow_widgets(area)
    if clipped or hscroll:
        path = out / f'{tag}.png'
        win.grab().save(str(path))
        record(f'가로 스크롤 {hscroll}px, 잘림 의심 {clipped[:5]}', '중',
               suspect=f'스크린샷 {path.name}')


def check_hangul(win, lang: str) -> None:
    if lang != 'en':
        return
    leftovers = qa_capture.hangul_texts(win)
    if leftovers:
        record(f'영어 화면에 남은 한글 {len(leftovers)}건: {leftovers[:6]}', '하')


# ──────────────────────────── 창 만들기 ────────────────────────────
def build_window(preset: str, biamp: bool, tmp: Path, delay_s: float = 0.0):
    import secs.model as model
    import secs.rewapi as rewapi
    from secs.wizard import WizardWindow

    project = model.Project(config=model.SpeakerConfig.from_preset(preset, biamp=biamp))
    project.extra['array'] = 9
    rew = rewapi.FakeREW(delay_s=delay_s)
    win = WizardWindow(project, rew, rew, rew, rew, project_path=tmp / 'qa_b.secs')
    win.ask_before_close = False
    return win


# ──────────────────────────── 5단계 ────────────────────────────
def exercise_step5(win, app, out: Path, *, lang: str, positions: int) -> None:
    note(screen='5측정', lang=lang)
    win.go(5)
    page = win.step5

    note(action='측정 전 [다시 측정] 눌러 보기(순서 밖)')
    try:
        page.redo_last()
    except Exception:
        record(f'측정 전 다시 측정 예외: {traceback.format_exc(limit=2)}', '중')
    settle(app)

    note(action='위치 1 측정 시작 후 즉시 취소')
    page.measure_position(1)
    gap = pump_with_heartbeat(app, lambda: page._thread is not None, 4000)
    check_heartbeat(gap, '측정 대기')
    page.cancel()
    if not page.wait(8000):
        record('취소 뒤에도 측정 스레드가 끝나지 않음', '상', suspect='step5_measure.cancel/wait')
    settle(app)

    note(action='위치 1 측정 중 보정 설계로 이동(순서 밖)')
    page.measure_position(1)
    try:
        win.go(9)
    except Exception:
        record(f'측정 중 단계 이동 예외: {traceback.format_exc(limit=2)}', '상')
    win.go(5)
    page.wait(15000)
    settle(app)

    note(action=f'위치 1~{positions} 측정')
    for pos in range(1, positions + 1):
        if not page.measure_position(pos):
            record(f'위치 {pos} 측정 시작 실패', '중')
            continue
        gap = pump_with_heartbeat(app, lambda: page._thread is not None, 20000)
        check_heartbeat(gap, f'위치 {pos} 측정')
        page.wait(20000)
        settle(app)
    if len(page.done_positions()) < positions:
        record(f'측정 완료 위치 {len(page.done_positions())}/{positions}', '중')

    note(action='같은 위치 다시 측정(재측정)')
    page.measure_position(1)
    page.wait(20000)
    settle(app)

    note(action='mdat 불러오기 버튼(파일 대화상자는 취소로 자동 응답)')
    try:
        page.load_mdat()
    except Exception:
        record(f'mdat 불러오기 예외: {traceback.format_exc(limit=2)}', '상')
    settle(app)

    for size in qa_capture.SIZES:
        note(size=f'{size[0]}x{size[1]}')
        win.resize(*size)
        settle(app)
        check_layout(win.stack.currentWidget(), win, out, f'B_step5_{lang}_{size[0]}x{size[1]}')
    win.resize(1280, 800)   # 다음 단계 크롤이 정상 크기에서 시작하게(09-23 이 스크립트에서 잔여 극단 크기 발견)
    note(size='1280x800')
    settle(app)
    check_hangul(win, lang)


# ──────────────────────────── 언어 전환 ────────────────────────────
def exercise_language_roundtrip(win, app, out: Path) -> None:
    from secs.i18n import language_manager

    note(action='언어 전환(en) 후 5·보정 설계·내보내기 재방문')
    language_manager().set_language('en', persist=False)
    settle(app)
    for step in (5, 9, 8):
        win.go(step)
        settle(app)
        check_layout(win.stack.currentWidget(), win, out, f'B_lang_en_step{step}')
        check_hangul(win, 'en')
    note(action='언어 되돌리기(ko)')
    language_manager().set_language('ko', persist=False)
    settle(app)
    for step in (5, 9, 8):
        win.go(step)
        settle(app)


# ──────────────────────────── 창 크기 왕복 ────────────────────────────
def resize_roundtrip(win, app, out: Path, lang: str) -> None:
    note(action='창 크기 왕복(늘렸다 줄였다)')
    sizes = qa_capture.SIZES + list(reversed(qa_capture.SIZES[:-1]))
    for step in (5, 9, 8):
        win.go(step)
        for size in sizes:
            note(size=f'{size[0]}x{size[1]}')
            win.resize(*size)
            settle(app)
        check_layout(win.stack.currentWidget(), win, out, f'B_roundtrip_step{step}_{lang}_end')


# ──────────────────────────── 실행 단위 ────────────────────────────
def run_pass(preset: str, biamp: bool, positions: int, out: Path, *, deep: bool) -> None:
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance() or QApplication([])
    with tempfile.TemporaryDirectory(prefix='secs-qa-b-run-') as tmp:
        tmp_path = Path(tmp)
        from PyQt6 import QtCore

        win = build_window(preset, biamp, tmp_path, delay_s=0.35 if deep else 0.0)
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.show()
        win.resize(1280, 800)
        settle(app)

        tag = f'{preset}{"biamp" if biamp else ""}'

        def guarded(fn, *args, **kwargs) -> None:
            """이 스크립트 자체의 버그가 나머지 단계 크롤을 막지 않게 한 단계 단위로 감싼다."""
            try:
                fn(*args, **kwargs)
            except Exception:  # noqa: BLE001
                record(f'크롤 스크립트가 처리 못한 예외로 이 단계를 중단함: {traceback.format_exc(limit=6)}',
                       '상', suspect='tools/qa_monkey_b.py')

        guarded(exercise_step5, win, app, out, lang='ko', positions=positions)
        if deep:
            guarded(exercise_language_roundtrip, win, app, out)
            guarded(resize_roundtrip, win, app, out, 'ko')
        win.close()
        settle(app)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--out', default='')
    args = ap.parse_args(argv)

    install_qt_message_handler()
    install_dialog_patches()

    out = Path(args.out) if args.out else ROOT / '보고서' / '05 화면 QA' / f'{_dt.date.today():%Y-%m-%d} 실사용 QA'
    out.mkdir(parents=True, exist_ok=True)

    print('=== 1회차: 9.1.6 바이앰프, 측정 3위치(깊은 패스) ===')
    run_pass('9.1.6', True, 3, out, deep=True)

    print('=== 2회차: 2.1, 측정 1위치 ===')
    run_pass('2.1', False, 1, out, deep=False)

    print(f'\n예외 {len(EXCEPTIONS)}건, Qt 경고 {len(QT_WARNINGS)}건, 발견 {len(ISSUES)}건')
    print('\n| 화면 | 크기 | 언어 | 조작 | 증상 | 심각도 |')
    print('|---|---|---|---|---|---|')
    for issue in ISSUES:
        print(f'| {issue.screen} | {issue.size} | {issue.lang} | {issue.action} | '
              f'{issue.symptom} | {issue.severity} |')
    return 0


if __name__ == '__main__':
    sys.exit(main())
