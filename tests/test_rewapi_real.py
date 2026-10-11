"""P2 B `secs.rewapi.rew` 검사 — `requests` 세션을 가짜로 바꿔 URL·페이로드만 본다.

REW 를 띄우지 않는다. 검증하는 것은 "어떤 경로에 무엇을 보내는가" 뿐이다.
**StubSession 의 응답은 2026-09-16 에 REW 5.40 Beta 124(API 0.9.4) 에서 읽기 전용
GET 으로 실제로 받은 본문과 doc.json 스키마를 그대로 옮긴 것이다**
(`보고서/04 실측·REW/2026-09-16 REW API 실검증.md`). 스키마가 바뀌면 여기부터 깨진다.
"""
from __future__ import annotations

import base64
import threading
from urllib.parse import urlsplit

import numpy as np
import pytest

from conftest import engine_choice, load_engine

pytestmark = pytest.mark.skipif(
    engine_choice() != "package",
    reason="secs.rewapi 는 패키지 엔진 전용이다 (SECS_ENGINE=package).",
)


@pytest.fixture(scope="module")
def rew_mod():
    load_engine("package")
    import secs.rewapi.rew as module  # type: ignore[import-not-found]
    return module


@pytest.fixture(scope="module")
def rewapi():
    load_engine("package")
    import secs.rewapi as module  # type: ignore[import-not-found]
    return module


# ── REW 흉내 세션 ─────────────────────────────────────────────────────
class _Response:
    def __init__(self, status: int = 200, payload=None, text: str = "") -> None:
        self.status_code = status
        self._payload = payload
        self.text = text

    def json(self):
        if self._payload is None:
            raise ValueError("본문이 JSON 이 아닙니다")
        return self._payload


#: `GET /audio/asio/outputs` 실응답(MOTU Gen 5, 22 채널) 중 앞 4개.
ASIO_OUTPUTS = ["1: Main Out 1", "2: Main Out 2", "3: Line Out 3", "4: Line Out 4"]


class StubSession:
    """`requests.Session` 자리에 끼우는 REW 흉내. 호출을 전부 기록한다.

    응답 본문은 실제 REW 5.40 Beta 124(API 0.9.4) 에서 받은 것과 같은 스키마다.
    """

    def __init__(self, *, ir: np.ndarray | None = None, status: dict | None = None,
                 version_payload=None, channels: list[str] | None = None,
                 driver: str = "Java", start_time: float = -0.125) -> None:
        self.calls: list[dict] = []
        #: ImpulseResponse.startTime — IR 배열 인덱스 0 의 절대 시각(초).
        #: None 이면 그 키를 아예 빼서 옛 REW 응답을 흉내낸다.
        self.start_time = None if start_time is None else float(start_time)
        self.status = status or {}
        # 실응답: {"message": "5.40 Beta 124 API 0.9.4"}
        self.version_payload = version_payload if version_payload is not None else {
            "message": "5.40 Beta 124 API 0.9.4"}
        self.channels = channels
        self.driver = driver
        self.ir = np.arange(8, dtype=np.float32) / 10.0 if ir is None else ir
        #: `GET /measurements` 는 순번 문자열 키 → MeasurementSummary 맵이다.
        self.measurements: dict[str, dict] = {}
        self._counter = 0
        self.raise_on: set[str] = set()
        #: 실측(09-18): 측정 명령은 바로 돌아오고 측정은 한참 뒤 목록에 생긴다.
        #: 0 보다 크면 `GET /measurements` 를 그만큼 받은 뒤에야 새 측정을 보여 준다.
        self.appear_after = 0
        self._pending: list[tuple[int, str, dict]] = []
        #: `/measure/level`·`/audio/input-cal` 실검증(2026-09-23) 응답용.
        self.measure_level_dbfs = -24.0
        self.cal_file_path = ""
        self.separate_cal_files = False

    # requests.Session 과 같은 시그니처
    def request(self, method, url, json=None, params=None, timeout=None):
        path = urlsplit(url).path
        self.calls.append({"method": method, "path": path, "json": json,
                           "params": params, "timeout": timeout})
        if path in self.raise_on:
            raise OSError("연결 거부")
        for key in (f"{method} {path}", path):
            if key in self.status:
                code, text = self.status[key]
                return _Response(code, None, text)
        return self._route(method, path, json)

    def _route(self, method, path, body):
        if path == "/version":
            return _Response(200, self.version_payload)
        if path == "/audio/driver" and method == "GET":
            return _Response(200, {"driver": self.driver})
        if path == "/measurements" and method == "GET":
            waiting = []
            for polls, key, summary in self._pending:
                if polls <= 1:
                    self.measurements[key] = summary
                else:
                    waiting.append((polls - 1, key, summary))
            self._pending = waiting
            return _Response(200, dict(self.measurements))
        if path.endswith("/impulse-response"):
            uuid = path.split("/")[2]
            if uuid not in self._uuids():
                return _Response(404, None, "없음")
            encoded = base64.b64encode(
                np.asarray(self.ir, dtype=">f4").tobytes()).decode("ascii")
            # ImpulseResponse: message/unit/startTime/sampleInterval/sampleRate/…/data
            # startTime = 배열 인덱스 0 의 절대 시각. 타이밍 레퍼런스가 있으면 음수다.
            body = {"message": "Completed", "unit": "percent",
                    "sampleInterval": 1 / 48000, "sampleRate": 48000.0,
                    "timingReference": "Acoustic", "timingRefTime": 0.0,
                    "delay": 0.0, "data": encoded}
            if self.start_time is not None:
                body["startTime"] = self.start_time
            return _Response(200, body)
        if path == "/measure/command" and (body or {}).get("command") == "SPL":
            self._counter += 1
            uuid = f"m{self._counter:03d}"
            summary = {"uuid": uuid, "title": f"측정 {self._counter}", "sampleRate": 48000.0}
            if self.appear_after > 0:
                self._pending.append((self.appear_after, str(self._counter), summary))
                # 실응답(09-18 실스윕): blocking 이어도 바로 이 본문으로 돌아온다.
                return _Response(202, {"message": "Starting measurement"})
            self.measurements[str(self._counter)] = summary
            # 실응답은 202 + APIResponse
            return _Response(202, {"message": "Completed"})
        if path in ("/audio/java/outputs", "/audio/asio/outputs"):
            if self.channels is None:
                return _Response(400, None, "Bad Request")
            return _Response(200, list(self.channels))
        if path in ("/audio/java/inputs", "/audio/asio/inputs"):
            return _Response(200, ["1: In 1", "2: In 2"])
        if path in ("/audio/java/output-devices", "/audio/asio/devices"):
            return _Response(200, ["MOTU Gen 5"])
        if path == "/audio/java/input-devices":
            return _Response(200, ["UMIK-1"])
        if path == "/spl-meter/1/levels":
            # SPLValues: 값 필드는 소문자 spl
            return _Response(200, {"meterNumber": 1, "weighting": "C", "filter": "Slow",
                                   "spl": 78.5, "leq": 77.0, "sel": 70.0,
                                   "elapsedTime": 3.0})
        if path == "/input-levels/last-levels":
            return _Response(200, {"unit": "dBFS", "rms": [-22.0], "peak": [-4.0],
                                   "timeSpanSeconds": 0.25})
        if path == "/measure/level" and method == "GET":
            # 실응답(2026-09-23, REW 5.40 Beta 124): Value {value, unit}.
            return _Response(200, {"value": self.measure_level_dbfs, "unit": "dBFS"})
        if path == "/audio/input-cal" and method == "GET":
            # 실응답(2026-09-23, MOTU Gen 5, 캘리브레이션 파일 없음): AudioInputCal.
            return _Response(200, {
                "currentInputSelection": "MOTU Gen 5",
                "separateCalFileForEachInput": self.separate_cal_files,
                "inputDeviceIsCWeighted": False,
                "calDataAllInputs": {"calFilePath": self.cal_file_path,
                                     "dBFSAt94dBSPL": -19.29323546142578,
                                     "fullScaleSineVrms": 1.0},
            })
        return _Response(200, {"message": "ok"})

    def _uuids(self) -> set[str]:
        return {str(m.get("uuid")) for m in self.measurements.values()}

    # 조회 도우미
    def posts(self, path: str) -> list:
        return [c["json"] for c in self.calls if c["method"] == "POST" and c["path"] == path]

    def paths(self, method: str = "POST") -> list[str]:
        return [c["path"] for c in self.calls if c["method"] == method]


def _client(rew_mod, session, **kwargs):
    return rew_mod.RealREW(session=session, measure_timeout=5.0, **kwargs)


# ── 상태 ──────────────────────────────────────────────────────────────
def test_is_running_and_version(rew_mod):
    stub = StubSession()
    client = _client(rew_mod, stub)
    assert client.is_running() is True
    assert client.version() == "5.40 Beta 124 API 0.9.4"   # APIResponse.message 실응답
    assert stub.calls[0]["path"] == "/version"
    assert stub.calls[0]["timeout"] is not None       # 모든 호출에 타임아웃


def test_is_running_false_when_rew_is_down(rew_mod):
    stub = StubSession()
    stub.raise_on.add("/version")
    client = _client(rew_mod, stub)
    assert client.is_running() is False
    assert client.version() is None
    assert client.pro_state() is None
    assert client.is_pro() is True                    # 모르면 낙관, 막히면 예외로 안다


def test_pro_state_only_comes_from_set_pro(rew_mod):
    """API 0.9.4 의 doc.json 에는 라이선스 필드가 없다 → /version 으로는 알 수 없다."""
    stub = StubSession()
    client = _client(rew_mod, stub)
    assert client.pro_state() is None
    assert stub.paths("GET") == []                    # /version 을 묻지도 않는다
    client.set_pro(False)
    assert client.pro_state() is False and client.is_pro() is False
    client.set_pro(True)
    assert client.is_pro() is True


def test_driver_is_queried_once_and_cached(rew_mod):
    stub = StubSession(driver="ASIO", channels=ASIO_OUTPUTS)
    client = _client(rew_mod, stub)
    assert client.driver() == "ASIO"
    client.output_channel_names()
    assert client.driver() == "ASIO"
    assert stub.paths("GET").count("/audio/driver") == 1


def test_pro_required_error_on_403(rew_mod, rewapi):
    stub = StubSession(status={"POST /measure/command": (403, "Pro upgrade required")})
    client = _client(rew_mod, stub)
    with pytest.raises(rewapi.RewProRequired):
        client.start_sweep()


def test_http_error_raises_rew_error(rew_mod, rewapi):
    stub = StubSession(status={"GET /measurements": (500, "서버 오류")})
    client = _client(rew_mod, stub)
    with pytest.raises(rewapi.RewError):
        client.measurements()


# ── 설정 ──────────────────────────────────────────────────────────────
def test_configure_sends_documented_payloads(rew_mod, rewapi):
    stub = StubSession()
    client = _client(rew_mod, stub)
    plan = rewapi.fake_plan(["L", "R"], [1], sweep_level_db=-18.0, sweep_length="256k")
    client.configure(plan)

    assert stub.posts("/application/blocking") == [True]
    # Value 스키마: 정수 본문이 아니다
    assert stub.posts("/audio/samplerate") == [{"value": 48000.0, "unit": "Hz"}]
    # MeasSweepConfiguration: 주파수는 int32, length 는 문자열
    assert stub.posts("/measure/sweep/configuration") == [
        {"startFrequency": 20, "endFrequency": 20000, "length": "256k"}]
    assert stub.posts("/measure/level") == [{"value": -18.0, "unit": "dBFS"}]
    assert stub.posts("/measure/sweep/repetitions") == [1]
    assert stub.posts("/measure/timing/reference") == ["Acoustic"]   # 맨 문자열


def test_sweep_length_is_mapped_to_a_rew_name(rew_mod):
    assert rew_mod.sweep_length_name("256k") == "256k"
    assert rew_mod.sweep_length_name("1m") == "1M"
    assert rew_mod.sweep_length_name(262144) == "256k"    # 샘플 수를 줘도 문자열로
    assert rew_mod.sweep_length_name(300000) == "256k"    # 가장 가까운 허용값
    assert "2M" in rew_mod.SWEEP_LENGTH_NAMES            # 검토 문서 목록에는 없던 값


def test_output_channel_uses_rew_channel_names(rew_mod):
    stub = StubSession(channels=["L", "R", "C", "SW"])    # Java 드라이버 선택지
    client = _client(rew_mod, stub)
    client.set_output_channel(3)
    assert stub.posts("/audio/java/output") == [{"output": "C"}]


def test_output_channel_matches_asio_number_prefix(rew_mod):
    stub = StubSession(driver="ASIO", channels=ASIO_OUTPUTS)
    client = _client(rew_mod, stub)
    client.set_output_channel(3)
    assert stub.posts("/audio/asio/output") == [{"output": "3: Line Out 3"}]
    assert stub.posts("/audio/java/output") == []        # 드라이버가 다른 경로는 건드리지 않는다


def test_output_channel_falls_back_to_number(rew_mod):
    stub = StubSession(channels=None)                       # 목록 조회 400
    client = _client(rew_mod, stub)
    client.set_output_channel(5)
    assert stub.posts("/audio/java/output") == [{"output": "5"}]


def test_devices_are_plain_string_lists(rew_mod):
    client = _client(rew_mod, StubSession(driver="ASIO", channels=ASIO_OUTPUTS))
    outputs = client.output_devices()
    assert [d.name for d in outputs] == ["MOTU Gen 5"]
    assert outputs[0].channels == len(ASIO_OUTPUTS)
    assert [d.name for d in client.input_devices()] == ["MOTU Gen 5"]  # ASIO 는 입·출력 공용

    java = _client(rew_mod, StubSession(channels=["L", "R"]))
    assert [d.name for d in java.input_devices()] == ["UMIK-1"]


def test_sequential_channels_uses_channel_list_object(rew_mod):
    stub = StubSession()
    client = _client(rew_mod, stub)
    client.set_sequential_channels(["1", "2"])
    assert stub.posts("/measure/sequential-channels") == [{"channels": ["1", "2"]}]


# ── 측정 ──────────────────────────────────────────────────────────────
def test_run_position_sequence_and_ir(rew_mod, rewapi):
    ir = np.linspace(-0.5, 0.5, 64, dtype=np.float32)
    stub = StubSession(ir=ir, channels=["L", "R", "SW"])
    client = _client(rew_mod, stub)
    plan = rewapi.fake_plan(["L", "R"], [1, 2])
    events: list[dict] = []

    results = client.run_position(plan, 2, events.append, threading.Event())

    assert [r.role for r in results] == ["L", "R"]
    assert [r.name for r in results] == ["L P2", "R P2"]
    assert [r.uuid for r in results] == ["m001", "m002"]
    for result in results:
        assert np.allclose(result.ir, ir)                   # base64 big-endian float32
        assert result.sample_rate == 48000
        assert result.spl_db == 78.5
        assert result.t0_s == pytest.approx(-0.125)         # ImpulseResponse.startTime
    # prefixMeasNameWithOutput 기본값(true)을 꺼야 넣은 제목이 그대로 쓰인다
    assert stub.posts("/measure/naming") == [
        {"title": "L P2", "namingOption": "Use as entered",
         "prefixMeasNameWithOutput": False},
        {"title": "R P2", "namingOption": "Use as entered",
         "prefixMeasNameWithOutput": False}]
    assert stub.posts("/measure/command") == [{"command": "SPL"}, {"command": "SPL"}]
    assert stub.posts("/audio/java/output") == [{"output": "L"}, {"output": "R"}]
    assert [c["params"] for c in stub.calls
            if c["path"].endswith("/impulse-response")] == [{"normalised": "false"},
                                                            {"normalised": "false"}]
    assert events[-1]["phase"] == "done"
    assert {e["phase"] for e in events} == {"sweep", "process", "done"}


def test_run_position_waits_until_the_measurement_appears(rew_mod, rewapi):
    """실스윕(09-18): 측정 명령은 blocking 이어도 "Starting measurement" 로 바로 돌아오고
    측정은 약 10초 뒤에야 목록에 생긴다. 예전 5초 고정 대기로는 실패했다."""
    stub = StubSession(channels=["L", "R"])
    stub.appear_after = 15            # 폴링 15번째에 나타난다(예전 한도는 20회 × 0.25 s)
    client = _client(rew_mod, stub)
    plan = rewapi.fake_plan(["L"], [1])

    assert client.sweep_wait_s(plan) > 30.0
    found = client.wait_for_new_measurement(set(), timeout_s=0.3, interval=0.01)
    assert found is None              # 아직 아무 측정도 없다

    results = client.run_position(plan, 1, None, threading.Event())
    assert [r.uuid for r in results] == ["m001"]


def test_wait_for_new_measurement_stops_on_cancel(rew_mod):
    stub = StubSession()
    client = _client(rew_mod, stub)
    cancel = threading.Event()
    cancel.set()
    assert client.wait_for_new_measurement(set(), 5.0, cancel) is None


def test_parse_progress_reads_the_real_callback_strings(rew_mod):
    """`/measure/subscribe` 콜백 본문은 맨 JSON 문자열 `"NN% 설명"` 이다(실스윕 확인)."""
    parse = rew_mod.RealREW.parse_progress
    assert parse("0% Capturing noise floor...13%") == (0.0, "Capturing noise floor...13%")
    assert parse("41% Remaining sweeps: 1, time: 4 s") == (41.0, "Remaining sweeps: 1, time: 4 s")
    assert parse("100% Measurement complete") == (100.0, "Measurement complete")
    assert parse("뭔가 다른 글") == (None, "뭔가 다른 글")
    assert parse(None) == (None, "")


def test_abort_uses_the_cancel_command(rew_mod):
    stub = StubSession()
    client = _client(rew_mod, stub)
    client.abort()
    assert stub.posts("/measure/command") == [{"command": "Cancel"}]
    assert "Cancel" in rew_mod.MEASURE_COMMANDS
    assert "Check levels" in rew_mod.MEASURE_COMMANDS   # 검토 문서와 달리 실제로 있다


def test_run_position_respects_cancel(rew_mod, rewapi):
    stub = StubSession()
    client = _client(rew_mod, stub)
    plan = rewapi.fake_plan(["L", "R"], [1])
    cancel = threading.Event()
    cancel.set()
    events: list[dict] = []

    assert client.run_position(plan, 1, events.append, cancel) == []
    assert events[-1]["phase"] == "cancelled"
    assert stub.posts("/measure/command") == []             # 스윕은 시작하지 않는다


def test_get_ir_decodes_big_endian_float32(rew_mod):
    ir = np.array([1.0, -0.25, 0.125], dtype=np.float32)
    stub = StubSession(ir=ir)
    stub.measurements["1"] = {"uuid": "abc"}
    client = _client(rew_mod, stub)
    got, rate = client.get_ir("abc")
    assert rate == 48000                     # sampleRate 는 double 로 온다
    assert np.array_equal(got, ir)


def test_impulse_response_reports_start_time_as_t0(rew_mod):
    """ImpulseResponse.startTime = IR 배열 인덱스 0 의 절대 시각(초)."""
    ir = np.array([1.0, -0.25], dtype=np.float32)
    stub = StubSession(ir=ir, start_time=-0.125)
    stub.measurements["1"] = {"uuid": "abc"}
    client = _client(rew_mod, stub)
    got, rate, t0 = client.impulse_response("abc")
    assert np.array_equal(got, ir) and rate == 48000
    assert t0 == pytest.approx(-0.125)       # 타이밍 레퍼런스가 있으면 음수


def test_missing_start_time_falls_back_to_zero(rew_mod):
    """t0 가 없는 응답(옛 REW)은 0.0 으로 본다."""
    stub = StubSession(start_time=None)
    stub.measurements["1"] = {"uuid": "abc"}
    client = _client(rew_mod, stub)
    assert client.impulse_response("abc")[2] == 0.0


def test_measurements_map_is_keyed_by_position(rew_mod):
    """`GET /measurements` 는 순번 문자열 키 → MeasurementSummary 맵이다."""
    stub = StubSession()
    stub.measurements = {"1": {"uuid": "zz", "title": "먼저"},
                         "2": {"uuid": "aa", "title": "나중"}}
    client = _client(rew_mod, stub)
    assert client.measurement_ids() == {"zz", "aa"}
    assert [m["title"] for m in client.measurements()] == ["먼저", "나중"]
    # 사전순이 아니라 목록 순서의 마지막을 새 측정으로 본다
    assert client._new_measurement_id({"zz"}) == "aa"


# ── 레벨·제너레이터 ───────────────────────────────────────────────────
def test_play_pink_uses_sine_for_subs(rew_mod):
    stub = StubSession(channels=["L", "R", "SW"])
    client = _client(rew_mod, stub)
    client.play_pink("L", 1, -20.0)
    client.play_pink("SW1", 3, -24.0)
    client.stop_generator()

    # /generator/signals 실목록은 소문자 id 다
    assert stub.posts("/generator/signal") == [{"signal": "pinknoise"}, {"signal": "sine"}]
    # 사인 주파수는 신호별 설정 엔드포인트로 넣는다(신호 본문에 넣는 게 아니다)
    assert stub.posts("/generator/signals/sine/configuration") == [{"frequency": 50.0}]
    assert stub.posts("/generator/level") == [{"value": -20.0, "unit": "dBFS"},
                                              {"value": -24.0, "unit": "dBFS"}]
    # 경로는 단수, 시작 명령은 Start 가 아니라 Play
    assert stub.posts("/generator/command") == [{"command": "Play"}, {"command": "Play"},
                                                {"command": "Stop"}]
    assert stub.posts("/generator/commands") == []


def test_level_monitor_subscribes_with_callback_url(rew_mod):
    stub = StubSession()
    client = _client(rew_mod, stub)
    got: list[dict] = []
    seen = threading.Event()
    try:
        client.start(lambda payload: (got.append(payload), seen.set()))
        subscribe = stub.posts("/input-levels/subscribe")[0]
        assert subscribe["url"].startswith("http://127.0.0.1:")
        assert subscribe["url"].endswith("/levels")
        assert subscribe["parameters"]["unit"] == "dBFS"
        assert stub.posts("/input-levels/command") == [{"command": "Start"}]

        # InputLevels 실스키마: 채널별 배열
        client._on_level_payload({"unit": "dBFS", "rms": [-22.0], "peak": [-4.0],
                                  "timeSpanSeconds": 0.25})
        assert seen.wait(1.0)
        assert got == [{"input_dbfs": -22.0, "spl_db": None, "clip": False}]
    finally:
        client.close()
    assert {"command": "Stop"} in stub.posts("/input-levels/command")
    assert stub.posts("/input-levels/unsubscribe")[0]["url"].endswith("/levels")


def test_level_payload_reads_input_levels_arrays(rew_mod):
    parse = rew_mod.RealREW._level_payload
    # /input-levels/last-levels 와 같은 모양
    body = {"unit": "dBFS", "rms": [-30.0, -12.0], "peak": [-0.5, -6.0],
            "timeSpanSeconds": 0.25}
    assert parse(body) == {"input_dbfs": -30.0, "spl_db": None, "clip": True}
    assert parse(body, 1) == {"input_dbfs": -12.0, "spl_db": None, "clip": False}
    # 단위가 SPL 이면 같은 값이 spl_db 로도 나온다
    assert parse({"unit": "SPL", "rms": [74.0], "peak": [80.0]})["spl_db"] == 74.0
    # 스칼라·다른 키 모양도 받아 준다(콜백 스키마는 문서에 없다)
    assert parse({"inputs": [{"rmsLevel": -18.0, "peakLevel": -9.0}]}) == {
        "input_dbfs": -18.0, "spl_db": None, "clip": False}
    assert parse("이상한 본문")["input_dbfs"] == -120.0


def test_last_levels_is_available_as_a_polling_fallback(rew_mod):
    client = _client(rew_mod, StubSession())
    assert client.last_levels() == {"unit": "dBFS", "rms": [-22.0], "peak": [-4.0],
                                    "timeSpanSeconds": 0.25}


def test_progress_subscription_is_unsubscribed_on_close(rew_mod):
    stub = StubSession()
    client = _client(rew_mod, stub)
    url = client.subscribe_progress(lambda payload: None)
    assert stub.posts("/measure/subscribe") == [{"url": url, "parameters": {}}]
    client.close()
    assert stub.posts("/measure/unsubscribe") == [{"url": url}]


def test_plan_from_project_is_reexported(rew_mod, rewapi):
    assert rew_mod.plan_from_project is rewapi.plan_from_project


# ── 2단계 [설정 읽기] 실값(2026-09-23) ──────────────────────────────────
def test_measure_level_dbfs_reads_the_real_shape(rew_mod):
    stub = StubSession()
    stub.measure_level_dbfs = -24.0
    client = _client(rew_mod, stub)
    assert client.measure_level_dbfs() == -24.0
    assert stub.paths("GET") == ["/measure/level"]


def test_measure_level_dbfs_is_none_when_rew_is_down(rew_mod):
    stub = StubSession()
    stub.raise_on.add("/measure/level")
    client = _client(rew_mod, stub)
    assert client.measure_level_dbfs() is None


def test_input_calibration_file_empty_means_not_loaded(rew_mod):
    """실기(MOTU Gen 5, 파일 없음) 응답: calFilePath 가 빈 문자열이다 — '파일 없음'이지 '모름'이 아니다."""
    stub = StubSession()
    stub.cal_file_path = ""
    client = _client(rew_mod, stub)
    assert client.input_calibration_file() == ""


def test_input_calibration_file_returns_the_path_when_loaded(rew_mod):
    stub = StubSession()
    stub.cal_file_path = "D:\\cal\\UMIK-1.cal"
    client = _client(rew_mod, stub)
    assert client.input_calibration_file() == "D:\\cal\\UMIK-1.cal"


def test_input_calibration_file_is_none_when_per_input_files_differ(rew_mod):
    """`separateCalFileForEachInput` 이 true 면 이 엔드포인트 하나로는 어느 파일인지 모른다(추정)."""
    stub = StubSession()
    stub.separate_cal_files = True
    client = _client(rew_mod, stub)
    assert client.input_calibration_file() is None


def test_input_calibration_file_is_none_when_rew_is_down(rew_mod):
    stub = StubSession()
    stub.raise_on.add("/audio/input-cal")
    client = _client(rew_mod, stub)
    assert client.input_calibration_file() is None
    assert rew_mod.REW_API_PORT == 4735
