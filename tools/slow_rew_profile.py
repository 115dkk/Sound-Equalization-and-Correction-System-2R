# -*- coding: utf-8 -*-
"""느린 REW 흉내 — 실제 REW 에서만 단계 이동이 느린 원인을 찾는 도구(2026-09-23 사용자 보고:
"각 단계별로 이동할 때 딜레이가 굉장히 심하다").

FakeREW 는 메서드 호출이 즉시 끝나서 화면 스레드가 REW 를 부르는지 드러나지 않는다. 이 도구는
**로컬 가짜 HTTP 서버**(요청마다 `--delay-ms`, 기본 150 ms 쉬고 REW 실응답 모양을 돌려준다)를 띄우고
진짜 `RealREW` 를 그 포트에 붙여 창을 만든다. `RealREW._request` 를 감싸 요청마다
(단계, 메서드, 경로, 화면 스레드인지, 걸린 ms)를 기록한 뒤, 1→8 정방향과 8→1 되돌아가기의
`go(n)` 시간과 단계별로 화면 스레드에서 나간 REW 요청 목록을 마크다운 표로 낸다.

**실제 REW(localhost:4735)에는 아무 요청도 보내지 않는다** — 서버는 빈 포트(OS 가 고른 번호)에 뜬다.
측정 데이터는 FakeREW 샘플(`step5.load_sample`)로 채운다(측정기는 FakeREW 그대로).

    .venv-secs\\Scripts\\python.exe tools\\slow_rew_profile.py [--delay-ms 150] [--preset 2.1] [--out FILE.md]
"""
from __future__ import annotations

import argparse
import http.server
import io
import json
import os
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
os.environ.setdefault('SECS4_SETTINGS_PATH', str(Path(tempfile.mkdtemp(prefix='secs-slowrew-')) / 'settings.ini'))

#: REW 5.40 실응답 모양(`보고서/04 실측·REW/2026-09-16 REW API 실검증.md`) — 화면이 읽는 GET 만.
_GET = {
    '/version': {'message': '5.40 Beta 124 API 0.9.4 (slow fake)'},
    '/audio/driver': {'driver': 'ASIO'},
    '/audio/asio/devices': ['MOTU Gen 5'],
    '/audio/asio/outputs': [f'{n}: Out {n}' for n in range(1, 23)],
    '/audio/asio/inputs': [f'{n}: Mic/Line In {n}' for n in range(1, 11)],
    '/audio/input-cal': {'calDataAllInputs': {'calFilePath': ''}, 'separateCalFileForEachInput': False},
    '/measure/level': {'value': -24.0, 'unit': 'dBFS'},
    '/measurements': {},
}


def start_server(delay_s: float) -> tuple[http.server.ThreadingHTTPServer, int]:
    """요청마다 `delay_s` 쉬고 답하는 가짜 REW. (서버, 포트)."""

    class Handler(http.server.BaseHTTPRequestHandler):
        def _reply(self, body) -> None:
            time.sleep(delay_s)
            data = json.dumps(body).encode('utf-8')
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self) -> None:  # noqa: N802
            self._reply(_GET.get(self.path.split('?')[0], {}))

        def do_POST(self) -> None:  # noqa: N802
            length = int(self.headers.get('Content-Length') or 0)
            if length:
                self.rfile.read(length)
            self._reply({'message': 'ok'})

        def log_message(self, *_args) -> None:
            return

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, name='slow-rew-server', daemon=True).start()
    return server, server.server_address[1]


class CallLog:
    """RealREW 요청 기록 — (라벨, 메서드, 경로, 화면 스레드?, ms)."""

    def __init__(self) -> None:
        self.label = ''
        self.rows: list[tuple[str, str, str, bool, float]] = []
        self._lock = threading.Lock()

    def add(self, method: str, path: str, gui: bool, ms: float) -> None:
        with self._lock:
            self.rows.append((self.label, method, path, gui, ms))

    def gui_calls(self, label: str) -> list[str]:
        return [f'{m} {p}' for (lb, m, p, gui, _ms) in self.rows if lb == label and gui]


def logged_rew(port: int, log: CallLog):
    from secs.rewapi.rew import RealREW

    class LoggedREW(RealREW):
        def _request(self, method, path, **kwargs):
            gui = threading.current_thread() is threading.main_thread()
            t0 = time.perf_counter()
            try:
                return super()._request(method, path, **kwargs)
            finally:
                log.add(method, path, gui, (time.perf_counter() - t0) * 1000.0)

    return LoggedREW(host='127.0.0.1', port=port)


def pump(app, seconds: float = 0.0) -> None:
    end = time.perf_counter() + seconds
    app.processEvents()
    while time.perf_counter() < end:
        app.processEvents()
        time.sleep(0.005)


def summarize(calls: list[str]) -> str:
    if not calls:
        return '없음'
    counts: dict[str, int] = {}
    for call in calls:
        counts[call] = counts.get(call, 0) + 1
    return ', '.join(f'{k} ×{v}' if v > 1 else k for k, v in counts.items())


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--delay-ms', type=float, default=150.0)
    parser.add_argument('--preset', default='2.1')
    parser.add_argument('--settle', type=float, default=0.4,
                        help='go() 뒤 이벤트 루프를 돌리는 시간(초) — 백그라운드 결과가 돌아오는 것까지 본다')
    parser.add_argument('--out')
    args = parser.parse_args(argv)

    os.environ.pop('QT_QPA_PLATFORM', None)
    from PyQt6 import QtCore, QtWidgets

    import secs.model as model
    from secs.rewapi.fake import FakeREW
    from secs.wizard.strings import FLOW, PAGE_TITLES
    from secs.wizard.window import WizardWindow

    app = QtWidgets.QApplication.instance() or QtWidgets.QApplication([])
    server, port = start_server(args.delay_ms / 1000.0)
    log = CallLog()
    rew = logged_rew(port, log)
    measurer = FakeREW(delay_s=0.0)
    project = model.Project(config=model.SpeakerConfig.from_preset(args.preset, biamp=False))
    tmp = Path(tempfile.mkdtemp(prefix='secs-slowrew-proj-'))

    log.label = '창 만들기'
    t0 = time.perf_counter()
    win = WizardWindow(project, rew=rew, devices=rew, levels=rew, measurer=measurer,
                       project_path=tmp / 'slow.secs')
    create_ms = (time.perf_counter() - t0) * 1000.0
    win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    win.resize(1280, 800)
    win.show()
    win.start_rew_watch()
    pump(app, 1.5)          # 백그라운드 점검이 한 바퀴 돌아 캐시를 채울 시간
    step5 = getattr(win, 'measure_page', None) or win.step5
    step5.load_sample(3)
    pump(app, 0.2)

    rows: list[tuple[str, float, list[str]]] = []

    def visit(step: int, tag: str) -> None:
        label = f'{tag} go({step}) {PAGE_TITLES.get(step, step)}'
        log.label = label
        t = time.perf_counter()
        win.go(step)
        ms = (time.perf_counter() - t) * 1000.0
        log.label = label + ' (뒤 이벤트)'
        pump(app, args.settle)
        widget = win.step_widget()
        if hasattr(widget, 'wait'):
            widget.wait(120000)
        rows.append((label, ms, log.gui_calls(label) + log.gui_calls(label + ' (뒤 이벤트)')))

    for step in FLOW:                       # 진행바 순서(09-26: 5 → 보정 설계 → 내보내기)
        visit(step, '정방향')
    for step in reversed(FLOW[:-1]):
        visit(step, '되돌아가기')

    out = io.StringIO()
    out.write(f'느린 REW 흉내: 요청마다 {args.delay_ms:.0f} ms · 프리셋 {args.preset} · 측정 3곳(샘플)\n\n')
    out.write(f'창 만들기 {create_ms:.0f} ms, 그동안 화면 스레드 REW 요청 {len(log.gui_calls("창 만들기"))}회\n\n')
    out.write('| 이동 | go() ms | 화면 스레드 REW 요청(go + 뒤 이벤트 {:.1f} s) |\n|---|---|---|\n'.format(args.settle))
    for label, ms, calls in rows:
        out.write(f'| {label} | {ms:.0f} | {len(calls)}회: {summarize(calls)} |\n')
    worst = max(ms for _l, ms, _c in rows)
    total_gui = sum(len(c) for _l, _m, c in rows)
    bg = sum(1 for (_lb, _m, _p, gui, _ms) in log.rows if not gui)
    out.write(f'\n최대 go() {worst:.0f} ms · 이동 중 화면 스레드 요청 합 {total_gui}회 · 백그라운드 요청(전체) {bg}회\n')
    text = out.getvalue()
    sys.stdout.buffer.write(text.encode('utf-8'))
    if args.out:
        Path(args.out).write_text(text, encoding='utf-8')
    win.close()
    server.shutdown()
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
