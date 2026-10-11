"""P2 B `secs.rewapi.callback` 검사 — REW 구독을 받는 로컬 HTTP 서버."""
from __future__ import annotations

import json
import threading
import urllib.error
import urllib.request

import pytest

from conftest import engine_choice, load_engine

pytestmark = pytest.mark.skipif(
    engine_choice() != "package",
    reason="secs.rewapi 는 패키지 엔진 전용이다 (SECS_ENGINE=package).",
)


@pytest.fixture(scope="module")
def callback_mod():
    load_engine("package")
    import secs.rewapi.callback as module  # type: ignore[import-not-found]
    return module


@pytest.fixture()
def server(callback_mod):
    srv = callback_mod.CallbackServer(keep_history=10)
    srv.start()
    yield srv
    srv.stop()


def _post(url: str, payload, timeout: float = 3.0):
    body = json.dumps(payload).encode("utf-8") if payload is not None else b""
    request = urllib.request.Request(url, data=body, method="POST",
                                     headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return response.status, response.read()


def test_server_starts_on_free_port_and_builds_urls(server):
    assert server.is_running is True
    assert server.port > 0
    assert server.url == f"http://127.0.0.1:{server.port}"
    assert server.url_for("levels") == f"{server.url}/levels"
    assert server.url_for("/levels") == f"{server.url}/levels"


def test_route_receives_json_body(server):
    got: list[dict] = []
    seen = threading.Event()

    def handler(payload) -> None:
        got.append(payload)
        seen.set()

    url = server.route("/levels", handler)
    assert url.startswith(server.url)

    status, body = _post(url, {"rms": -21.5, "peak": -6.0})
    assert status == 200 and body == b"OK"        # REW 는 OK 를 못 받으면 구독을 끊는다
    assert seen.wait(2.0)
    assert got == [{"rms": -21.5, "peak": -6.0}]


def test_query_string_and_unknown_path(server):
    levels: list[object] = []
    other: list[object] = []
    both = threading.Semaphore(0)

    def on_levels(payload) -> None:
        levels.append(payload)
        both.release()

    def on_any(payload) -> None:
        other.append(payload)
        both.release()

    server.route("/levels", on_levels)
    server.on_any(on_any)

    _post(f"{server.url}/levels?unit=dBFS", {"rms": -30.0})
    _post(f"{server.url}/unknown-path", {"a": 1})
    assert both.acquire(timeout=2.0) and both.acquire(timeout=2.0)

    assert levels == [{"rms": -30.0}]             # 쿼리스트링은 경로에서 떼어낸다
    assert other == [{"a": 1}]                    # 등록 안 된 경로는 기본 콜백으로


def test_handler_exception_does_not_kill_server(server):
    def boom(payload) -> None:
        raise RuntimeError("콜백 실패")

    server.route("/boom", boom)
    assert _post(f"{server.url}/boom", {"x": 1})[0] == 200
    assert _post(f"{server.url}/boom", {"x": 2})[0] == 200
    assert server.is_running is True


def test_history_and_get_health(server):
    seen = threading.Event()
    server.route("/levels", lambda payload: seen.set())
    _post(f"{server.url}/levels", {"rms": -12.0})
    assert seen.wait(2.0)
    assert ("/levels", {"rms": -12.0}) in server.history

    with urllib.request.urlopen(f"{server.url}/levels", timeout=3.0) as response:
        assert response.status == 200
        assert json.loads(response.read())["status"] == "ok"


def test_stop_closes_port(callback_mod):
    srv = callback_mod.CallbackServer()
    srv.start()
    url = srv.url_for("/x")
    srv.route("/x", lambda payload: None)
    assert _post(url, {"a": 1})[0] == 200
    srv.stop()
    assert srv.is_running is False
    with pytest.raises((urllib.error.URLError, OSError)):
        _post(url, {"a": 1}, timeout=1.0)


def test_context_manager(callback_mod):
    with callback_mod.CallbackServer() as srv:
        assert srv.is_running is True
        port = srv.port
    assert srv.is_running is False
    assert port > 0
