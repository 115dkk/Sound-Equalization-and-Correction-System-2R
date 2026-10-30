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
