import os
import re
import socket
import time
from pathlib import Path

import pytest

import secs2r


ROOT = Path(__file__).resolve().parents[1]


def crate_version() -> str:
    manifest = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version = "([^"]+)"$', manifest, re.MULTILINE)
    assert match is not None
    return match.group(1)


def test_import_and_version_match_crate() -> None:
    assert secs2r.__version__ == crate_version()
    assert secs2r.Engine.__module__ == "secs2r._secs2r"
    assert secs2r.Client.__module__ == "secs2r._secs2r"


def test_engine_bootstrap_is_a_real_request(tmp_path: Path) -> None:
    engine = secs2r.Engine(tmp_path / "settings.json")
    response = engine.call({"method": "bootstrap"})
    assert response["method"] == "bootstrap"
    assert response["data"]["version"] == secs2r.__version__

    events = engine.events()
    assert iter(events) is events
    assert events.dropped_events() == 0


def test_engine_api_error_has_structured_fields() -> None:
    with pytest.raises(secs2r.SecsError) as raised:
        secs2r.Engine().call({"method": "results"})

    error = raised.value
    assert error.code == "project_not_open"
    assert error.message
    assert isinstance(error.detail, str)
    assert error.detail


def test_client_closed_port_fails_quickly_with_secs_error() -> None:
    probe = socket.socket()
    probe.bind(("127.0.0.1", 0))
    port = probe.getsockname()[1]
    probe.close()

    started = time.monotonic()
    with pytest.raises(secs2r.SecsError) as raised:
        secs2r.Client(port=port, token="test").call({"method": "bootstrap"})
    elapsed = time.monotonic() - started

    assert raised.value.code == "transport_error"
    assert elapsed < 5.0


def test_oracle_project_round_trip_when_supplied(tmp_path: Path) -> None:
    project = os.environ.get("SECS_ORACLE_PROJECT")
    if not project:
        pytest.skip("SECS_ORACLE_PROJECT is not available")

    engine = secs2r.Engine()
    opened = engine.call(
        {"method": "project_open", "params": {"path": str(Path(project).resolve())}}
    )
    assert opened["method"] == "project_open"

    started = engine.call({"method": "calibrate", "params": {}})
    finished = engine.wait_job(started["data"]["job"], 120.0)
    assert finished["event"] == "finished"
    assert finished["status"] == "succeeded"

    results = engine.call({"method": "results"})
    assert results["data"]["calibration"] is not None

    exported = engine.call(
        {
            "method": "export",
            "params": {
                "kind": "camilla_dsp",
                "folder": str(tmp_path),
                "mdat_scope": None,
                "design_name": None,
                "camilla_devices": None,
            },
        }
    )
    terminal = engine.wait_job(exported["data"]["job"], 120.0)
    assert terminal["status"] == "succeeded"
    assert any(tmp_path.iterdir())


def test_python_error_carries_params_and_retryable_td160() -> None:
    with pytest.raises(secs2r.SecsError) as raised:
        secs2r.Engine().call({"method": "results"})
    error = raised.value
    assert error.code == "project_not_open"
    assert getattr(error, "params", None) == {}
    assert getattr(error, "retryable", None) is False
    assert error.message.startswith("project_not_open: ")


def _run_event_probe(source: str, client: bool) -> None:
    import subprocess
    import sys
    import textwrap

    source = textwrap.dedent(source)
    if client:
        source = source.replace("engine = secs2r.Engine()", "engine = secs2r.Client(port=port, token='test')")
        source = textwrap.dedent(r"""
            import base64, hashlib, socket, threading
            listener = socket.socket()
            listener.bind(("127.0.0.1", 0))
            listener.listen(1)
            port = listener.getsockname()[1]
            disconnected = threading.Event()
            def serve():
                connection, _ = listener.accept()
                with connection:
                    request = b""
                    while b"\r\n\r\n" not in request:
                        request += connection.recv(4096)
                    key = next(line.split(b":", 1)[1].strip() for line in request.split(b"\r\n") if line.lower().startswith(b"sec-websocket-key:"))
                    accept = base64.b64encode(hashlib.sha1(key + b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11").digest())
                    connection.sendall(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: " + accept + b"\r\n\r\n")
                    try:
                        while connection.recv(4096):
                            pass
                    except ConnectionError:
                        pass
                disconnected.set()
                listener.close()
            threading.Thread(target=serve, daemon=True).start()
        """) + source + "\nassert disconnected.wait(1), 'client socket must close'\n"
    try:
        result = subprocess.run(
            [sys.executable, "-c", textwrap.dedent(source)],
            capture_output=True, text=True, timeout=3,
        )
    except subprocess.TimeoutExpired:
        pytest.fail("blocked next() did not return within three seconds")
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("client", [False, True])
def test_events_close_ends_iteration_td150(client: bool) -> None:
    _run_event_probe('''
        import threading, time, secs2r
        engine = secs2r.Engine()
        events = engine.events()
        ended = threading.Event()
        def read():
            assert next(events, None) is None
            ended.set()
        worker = threading.Thread(target=read, daemon=True)
        worker.start()
        time.sleep(0.1)
        assert callable(getattr(events, "close", None)), "events need close()"
        events.close()
        assert ended.wait(1), "close must end a blocked next"
        assert events.closed
        assert next(events, None) is None
        events.close()
    ''', client)


@pytest.mark.parametrize("client", [False, True])
def test_events_timeout_raises_td150(client: bool) -> None:
    _run_event_probe('''
        import time, secs2r
        engine = secs2r.Engine()
        started = time.monotonic()
        events = engine.events(timeout=0.2)
        try:
            next(events)
        except TimeoutError:
            pass
        else:
            raise AssertionError("event timeout must raise TimeoutError")
        assert 0.15 <= time.monotonic() - started < 1
        assert events.closed
    ''', client)


@pytest.mark.parametrize("client", [False, True])
def test_keyboard_interrupt_reaches_a_blocked_next_td150(client: bool) -> None:
    _run_event_probe('''
        import _thread, threading, time, secs2r
        engine = secs2r.Engine()
        events = engine.events()
        timer = threading.Timer(0.2, _thread.interrupt_main)
        timer.start()
        started = time.monotonic()
        try:
            next(events)
        except KeyboardInterrupt:
            assert time.monotonic() - started < 1
        else:
            raise AssertionError("KeyboardInterrupt must propagate")
        finally:
            timer.join()
            events.close()
    ''', client)



def test_client_events_report_lagged_td161() -> None:
    import base64
    import hashlib
    import json
    import threading

    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    port = listener.getsockname()[1]

    def frame(payload: bytes) -> bytes:
        assert len(payload) < 126
        return bytes((0x81, len(payload))) + payload

    def serve() -> None:
        connection, _ = listener.accept()
        with connection:
            request = b""
            while b"\r\n\r\n" not in request:
                request += connection.recv(4096)
            key = next(
                line.split(b":", 1)[1].strip()
                for line in request.split(b"\r\n")
                if line.lower().startswith(b"sec-websocket-key:")
            )
            accept = base64.b64encode(
                hashlib.sha1(
                    key + b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
                ).digest()
            )
            connection.sendall(
                b"HTTP/1.1 101 Switching Protocols\r\n"
                b"Upgrade: websocket\r\nConnection: Upgrade\r\n"
                b"Sec-WebSocket-Accept: " + accept + b"\r\n\r\n"
            )
            payload = json.dumps({"kind": "lagged", "missed": 7}).encode()
            connection.sendall(frame(payload))
            time.sleep(0.2)
        listener.close()

    worker = threading.Thread(target=serve)
    worker.start()
    events = secs2r.Client(port=port, token="test").events(timeout=2)
    event = next(events)
    assert event["kind"] == "lagged"
    assert event["event"]["missed"] == 7
    events.close()
    worker.join(timeout=2)
    assert not worker.is_alive()
