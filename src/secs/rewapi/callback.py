"""P2 B: REW 구독(subscribe)을 받는 로컬 HTTP 콜백 서버.

REW 는 입력 레벨·측정 진행 같은 실시간 값을 폴링으로 주지 않고, 클라이언트가
알려 준 URL 로 HTTP POST 를 보낸다(검토 문서 "구독 공통"). 그래서 SECS 안에
작은 HTTP 서버를 스레드로 띄우고 경로별 파이썬 콜백으로 넘긴다.

Qt 의존 금지 — C 가 이 콜백을 Qt 시그널로 감싼다.
"""
from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Callable

Handler = Callable[[Any], None]

_MAX_BODY = 4 * 1024 * 1024     # 콜백 본문 상한(4 MB)


class _RequestHandler(BaseHTTPRequestHandler):
    server_version = 'SECS-rewapi/1.0'
    protocol_version = 'HTTP/1.1'

    # BaseHTTPRequestHandler 는 stderr 로 한 줄씩 찍는다 — 조용히.
    def log_message(self, fmt: str, *args: Any) -> None:  # noqa: A003
        pass

    @property
    def _owner(self) -> 'CallbackServer':
        return self.server.owner  # type: ignore[attr-defined]

    def _path(self) -> str:
        path = self.path.split('?', 1)[0]
        return path if path.startswith('/') else '/' + path

    def _reply(self, code: int = 200, body: bytes = b'OK') -> None:
        self.send_response(code)
        self.send_header('Content-Type', 'text/plain; charset=utf-8')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except OSError:
            pass

    def do_POST(self) -> None:  # noqa: N802 - http.server 규약
        try:
            length = int(self.headers.get('Content-Length') or 0)
        except ValueError:
            length = 0
        raw = self.rfile.read(min(length, _MAX_BODY)) if length > 0 else b''
        try:
            payload: Any = json.loads(raw.decode('utf-8')) if raw else None
        except (UnicodeDecodeError, json.JSONDecodeError):
            payload = raw
        # REW 는 OK 응답을 못 받으면 구독을 취소한다 → 콜백보다 응답을 먼저.
        self._reply()
        self._owner._dispatch(self._path(), payload)

    def do_GET(self) -> None:  # noqa: N802
        self._reply(200, b'{"status":"ok"}')

    def do_PUT(self) -> None:  # noqa: N802
        self.do_POST()


class CallbackServer:
    """경로별 콜백을 받는 로컬 HTTP 서버(스레드).

    사용:
        cb = CallbackServer()
        cb.route('/levels', on_levels)
        cb.start()
        rew.post('/input-levels/subscribe', {'url': cb.url_for('/levels')})
        ...
        cb.stop()
    """

    def __init__(self, host: str = '127.0.0.1', port: int = 0, *,
                 keep_history: int = 0) -> None:
        self.host = host
        self._requested_port = int(port)
        self._server: ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None
        self._routes: dict[str, Handler] = {}
        self._any: Handler | None = None
        self._lock = threading.Lock()
        self.keep_history = int(keep_history)
        self.history: list[tuple[str, Any]] = []

    # ── 등록 ──
    def route(self, path: str, handler: Handler) -> str:
        """경로에 콜백을 건다. 반환값은 REW 에 넘길 전체 URL(서버 시작 후 유효)."""
        key = path if path.startswith('/') else '/' + path
        with self._lock:
            self._routes[key] = handler
        return self.url_for(key) if self.is_running else key

    def on_any(self, handler: Handler | None) -> None:
        """경로가 등록되지 않은 POST 를 받을 기본 콜백."""
        self._any = handler

    # ── 수명 ──
    @property
    def is_running(self) -> bool:
        return self._server is not None

    @property
    def port(self) -> int:
        if self._server is None:
            return self._requested_port
        return int(self._server.server_address[1])

    @property
    def url(self) -> str:
        return f'http://{self.host}:{self.port}'

    def url_for(self, path: str) -> str:
        key = path if path.startswith('/') else '/' + path
        return f'{self.url}{key}'

    def start(self) -> str:
        """서버를 띄우고 기본 URL 을 돌려준다(이미 떠 있으면 그대로)."""
        if self._server is not None:
            return self.url
        server = ThreadingHTTPServer((self.host, self._requested_port), _RequestHandler)
        server.daemon_threads = True
        server.owner = self  # type: ignore[attr-defined]
        self._server = server
        self._thread = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': 0.05},
                                        name='secs-rew-callback', daemon=True)
        self._thread.start()
        return self.url

    def stop(self) -> None:
        server, self._server = self._server, None
        thread, self._thread = self._thread, None
        if server is not None:
            server.shutdown()
            server.server_close()
        if thread is not None and thread.is_alive():
            thread.join(timeout=2.0)

    def __enter__(self) -> 'CallbackServer':
        self.start()
        return self

    def __exit__(self, *exc: object) -> None:
        self.stop()

    # ── 수신 ──
    def _dispatch(self, path: str, payload: Any) -> None:
        if self.keep_history:
            with self._lock:
                self.history.append((path, payload))
                if len(self.history) > self.keep_history:
                    del self.history[:-self.keep_history]
        with self._lock:
            handler = self._routes.get(path)
        if handler is None:
            handler = self._any
        if handler is None:
            return
        try:
            handler(payload)
        except Exception:  # noqa: BLE001 - 콜백 오류로 서버 스레드를 죽이지 않는다
            pass


__all__ = ['CallbackServer']
