"""P2 B: 실제 REW API 구현(`RealREW`).

**2026-09-16 실검증 완료.** REW 5.40 Beta 124(API 0.9.4) 를 `-api` 로 띄우고
`localhost:4735/doc.json`(Swagger 2.0, 379 경로) 과 읽기 전용 GET 응답으로
아래를 모두 확정했다. 표는 `보고서/04 실측·REW/2026-09-16 REW API 실검증.md`.

확정된 것(예전 "미확인" 12곳)
- 장치 목록: 드라이버에 따라 경로가 갈린다. `GET /audio/driver` → `{"driver":"ASIO"|"Java"}`.
  ASIO 는 `/audio/asio/devices`(입·출력 공용), Java 는 `/audio/{output,input}-devices`.
  모두 그냥 문자열 배열이다. 드라이버가 안 맞는 경로는 400 을 준다.
- 출력 채널: 선택지는 `/audio/asio/outputs`(`"17: Optical Out 3"` 처럼 1-기준 번호 접두)
  또는 `/audio/java/outputs`(`"L"`,`"R"`,`"L+R"`), 설정은 `POST .../output {"output": 문자열}`.
- `/audio/samplerate` 는 정수가 아니라 `{"value":48000.0,"unit":"Hz"}`(Value).
- `/measure/sweep/configuration` 의 `length` 는 샘플 수가 아니라 **문자열**("256k").
  허용값은 `/measure/sweep/configuration/sweep-lengths` = 64k~4M.
- `/measure/timing/reference` 는 맨 JSON 문자열. 선택지 = None/Loopback/Loopback cal/Acoustic/Wired.
- 측정 취소 명령은 `Cancel`(`/measure/commands` 실목록으로 확인).
- 제너레이터: 경로가 `/generator/command`(단수)이고 명령은 `Play`/`Stop`.
  신호 이름은 소문자 id(`pinknoise`,`sine`), 사인 주파수는 `/generator/signals/sine/configuration`.
- `/input-levels/command` 는 Start/Stop, 구독 본문은 Subscriber `{url, parameters}`.
  레벨 본문은 InputLevels `{unit, rms[], peak[], timeSpanSeconds}` — **채널별 배열**이다.
- SPL 값 필드는 소문자 `spl`(SPLValues). 경로는 `/spl-meter/{id}/levels`.
- `/version` 은 APIResponse `{"message":"5.40 Beta 124 API 0.9.4"}`. Pro 표시 필드는
  doc.json 어디에도 없다 → `pro_state()` 는 사용자가 알려준 값만 근거로 쓴다.
- `/measure/sequential-channels` 본문은 ChannelList `{"channels":[...]}`.

2026-09-18 실스윕으로 확정(예전 "추정" 2곳) — `보고서/04 실측·REW/2026-09-18 REW 실스윕/REW 실스윕 2026-09-18.md`
- `/measure/subscribe` 콜백 본문은 **맨 JSON 문자열** `"NN% 설명"` 이다
  (`"0% Capturing noise floor...13%"` … `"100% Measurement complete"`, 약 0.17 s 간격).
  `RealREW.parse_progress` 가 읽는다.
- `/input-levels/subscribe` 콜백 본문은 GET `/input-levels/last-levels` 와 같은
  InputLevels `{unit, rms[], peak[], timeSpanSeconds}` 다. 배열에는 **선택된 입력
  하나**만 온다(길이 1). `timeSpanSeconds` 는 요청값(0.25)이 아니라 0.1707 로 왔다.
- **측정 명령은 blocking 을 켜도 바로 돌아온다**(`{"message":"Starting measurement"}`,
  `process-result` 도 "There is no process"). 끝났는지는 새 측정이 `/measurements` 에
  생기는 것으로만 안다 → `wait_for_new_measurement`. 256k·48 kHz 에서 약 10 s.
- 스윕 도중 `close()`(= `/generator/command Stop`·구독 해제)를 부르자 **측정이 1 kHz
  위가 빈 채로 저장됐다**. Stop 이 스윕 재생을 끊은 것으로 추정(따로 재현은 안 함).
  측정 중에는 `close()` 를 부르지 말 것.
- `/input-levels/unsubscribe` 는 모니터 Stop 뒤에 부르면 400 "No subscription found".

참고: `/measure/commands` 에 **`Check levels`** 가 실제로 있다. 검토 문서의 "전용 명령
없음" 은 틀렸다. 위저드 3단계는 제너레이터+입력 레벨 조합 대신 이 명령을 쓸 수 있다
(스피커로 소리가 나가므로 이 모듈에서는 호출하지 않는다).

2026-09-23 실검증(1단계 [설정 읽기] 백로그, 사용자 PC 의 REW 5.40 Beta 124 에 GET 만 보냄)
- `GET /measure/level` → Value `{"value":-24.0,"unit":"dBFS"}`. `set_level()` 이 쓰는
  것과 같은 모양이라 그대로 읽으면 된다 → `measure_level_dbfs()`.
- `GET /audio/input-cal` → AudioInputCal. 캘리브레이션 파일 없음 상태(MOTU Gen 5)에서
  `calDataAllInputs.calFilePath` 가 빈 문자열로 왔다 — "없음"과 "모름"을 가르는 확정값.
  `separateCalFileForEachInput` 이 true 인 경우(입력마다 다른 파일)는 실기로 못 봤다
  (추정) → `input_calibration_file()` 은 그때 None(알 수 없음)을 돌려준다.
- Pro 라이선스처럼 REW API 에 캘리브레이션을 읽는 별도 엔드포인트는 이것 하나뿐이고
  doc.json 에 다른 후보는 없었다(`/audio/output-cal` 은 스피커 보정용으로 별개).

Qt 의존 없음. `requests` 가 없어도 이 모듈은 임포트되고, 세션을 직접 주지 않은 채
`RealREW()` 를 만들 때 비로소 `RewError` 가 난다(패키지 `__init__` 의 지연 로딩 때문).
"""
from __future__ import annotations

import base64
import os
import subprocess
import sys
import threading
import time
from typing import Any, Callable, Sequence

import numpy as np

from secs.rewapi.callback import CallbackServer
from secs.rewapi.interfaces import (
    DeviceInfo, LevelCallback, MeasurementPlan, MeasurementResult, ProgressCallback,
    RewError, RewProRequired, emit, plan_from_project, progress, split_role,
    sweep_length_samples,
)

try:  # requests 없이도 secs.rewapi 임포트가 되도록(패키지 __init__ 은 지연 로딩)
    import requests as _requests
except ImportError:  # pragma: no cover - 환경 문제
    _requests = None  # type: ignore[assignment]

#: REW API 기본 포트(`roomeqwizard.exe -api`).
REW_API_PORT = 4735

_IS_MAC = sys.platform == 'darwin'
_IS_WINDOWS = sys.platform == 'win32'
# 콘솔 없는(--windowed) exe 에서 하위 프로세스를 띄울 때 CMD 창이 뜨지 않도록.
_NO_CONSOLE: dict[str, Any] = (
    {'creationflags': subprocess.CREATE_NO_WINDOW} if _IS_WINDOWS else {})
# REW 설치 경로 후보(`find_rew_exe`). 1단계 [REW API 모드로 실행] 이 `RealREW.launch` 로 띄운다(09-22).
_REW_WIN_PATHS = (
    'C:\\Program Files\\REW\\roomeqwizard.exe',
    'C:\\Program Files (x86)\\REW\\roomeqwizard.exe',
    'C:\\Program Files\\REW\\REW.exe',
    'C:\\Program Files (x86)\\REW\\REW.exe',
)
_REW_MAC_PATH = '/Applications/REW/REW.app'

#: REW 가 받아들이는 스윕 길이 문자열(`/measure/sweep/configuration/sweep-lengths` 실측).
SWEEP_LENGTH_NAMES: tuple[str, ...] = ('64k', '128k', '256k', '512k', '1M', '2M', '4M')

#: 측정 명령(`/measure/commands` 실측). 'Check levels' 도 실제로 있다.
MEASURE_COMMANDS: tuple[str, ...] = (
    'SPL', 'Filtered SPL', 'Impedance', 'Impedance open cal', 'Impedance short cal',
    'Impedance ref cal', 'Check levels', 'Cancel')

#: 타이밍 레퍼런스 선택지(`/measure/timing/reference/choices` 실측).
TIMING_REFERENCES: tuple[str, ...] = ('None', 'Loopback', 'Loopback cal', 'Acoustic', 'Wired')

#: 제너레이터 신호 id(`/generator/signals` 실측, 소문자).
SIGNAL_PINK_NOISE = 'pinknoise'
SIGNAL_SINE = 'sine'

#: 연결에 실패한 뒤 이 시간(초) 동안은 다시 시도하지 않고 바로 실패한다. REW 를 막 켠 사용자가
#: [다시 확인]을 눌렀을 때 오래 기다리지 않게 짧게 둔다.
DOWN_FAST_FAIL_S = 2.5
#: 연결 자체에 주는 시간(초). 주소는 127.0.0.1 — `localhost` 는 Windows 가 IPv6(::1)를 먼저 시도해 새 연결마다
#: 이 시간만큼 늦었다(09-27 실측: 첫 요청 622 ms → 2 ms; 연결을 매번 새로 여는 도구는 요청마다 2 s).
CONNECT_TIMEOUT_S = 0.6
#: `probe()` 가 "꺼져 있음"을 확인한 뒤 화면 쪽 요청을 바로 실패시키는 시간(초). 점검 주기보다 길어야 한다.
PROBE_HOLD_S = 6.0
#: 장치·채널 목록 캐시 수명(초). 화면 한 번 그리는 동안만 같으면 된다.
LIST_CACHE_S = 2.0

# 드라이버별 오디오 경로. `GET /audio/driver` 로 고른다. 반대 드라이버 경로는 400 을 준다.
_AUDIO_PATHS: dict[str, dict[str, str]] = {
    'asio': {
        'output_devices': '/audio/asio/devices',
        'input_devices': '/audio/asio/devices',
        'output_choices': '/audio/asio/outputs',
        'input_choices': '/audio/asio/inputs',
        'output_set': '/audio/asio/output',
        'ref_output_set': '/audio/asio/ref-output',          # {"output": "17: 이름"}
    },
    'java': {
        'output_devices': '/audio/java/output-devices',
        'input_devices': '/audio/java/input-devices',
        'output_choices': '/audio/java/outputs',
        'input_choices': '/audio/java/inputs',
        'output_set': '/audio/java/output',
        'ref_output_set': '/audio/java/ref-output-channel',  # {"channel": "R"}
        'ref_output_choices': '/audio/java/ref-output-channels',
    },
}


def find_rew_exe() -> str | None:
    """REW 실행 파일 경로(없으면 None) — 알려진 설치 경로(`_REW_WIN_PATHS`·맥 앱)만 본다."""
    if _IS_MAC:
        return _REW_MAC_PATH if os.path.exists(_REW_MAC_PATH) else None
    if _IS_WINDOWS:
        for path in _REW_WIN_PATHS:
            if os.path.isfile(path):
                return path
    return None


_REW_PROCESS_NAMES = ('roomeqwizard.exe', 'rew.exe')


def rew_process_running() -> bool:
    """REW 프로세스가 떠 있는지(Windows: tasklist). API 가 응답하지 않는데 프로세스가 있으면
    "-api 없이 켜져 있다"는 뜻이라 1단계가 다른 안내를 낸다(09-22 사용자: 연결해 놔도 안 된다)."""
    if not _IS_WINDOWS:
        return False
    try:
        out = subprocess.run(['tasklist', '/FO', 'CSV', '/NH'], capture_output=True, text=True,
                             timeout=5.0, **_NO_CONSOLE).stdout.lower()
    except Exception:  # noqa: BLE001 - 못 알아내면 모른다고 본다
        return False
    return any(name in out for name in _REW_PROCESS_NAMES)


def _b64_be_f32(encoded: str) -> np.ndarray:
    """REW 의 base64 big-endian float32 → numpy(기존 REWClient 와 동일)."""
    return np.frombuffer(base64.b64decode(encoded), dtype='>f4').astype(np.float32)


def _as_float(value: Any, default: float | None = None) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def _first_float(value: Any, index: int = 0) -> float | None:
    """InputLevels 의 `rms`/`peak` 은 채널별 배열이다. 스칼라도 받아 준다."""
    if isinstance(value, (list, tuple)):
        if not value:
            return None
        return _as_float(value[index] if 0 <= index < len(value) else value[0])
    return _as_float(value)


def sweep_length_name(length: str | int) -> str:
    """`'256k'`·`262144` → REW 가 받는 길이 문자열. 허용값 중 가장 가까운 것으로 맞춘다."""
    if isinstance(length, str):
        text = length.strip()
        for name in SWEEP_LENGTH_NAMES:
            if text.lower() == name.lower():
                return name
        samples = sweep_length_samples(text)
    else:
        samples = int(length)
    return min(SWEEP_LENGTH_NAMES,
               key=lambda name: abs(sweep_length_samples(name) - samples))


def _index_key(key: str) -> tuple[int, Any]:
    """`/measurements` 의 키("1","2",…)를 숫자로 정렬하되 숫자가 아니면 뒤로 보낸다."""
    try:
        return (0, int(str(key)))
    except ValueError:
        return (1, str(key))


def channel_name_for(names: Sequence[str], channel: int) -> str | None:
    """ASIO 채널 목록(`"17: Optical Out 3"`)에서 1-기준 번호에 맞는 문자열을 찾는다.

    번호 접두가 없는 Java 목록(`"L"`,`"R"`)이면 순서로 인덱싱한다.
    """
    prefix = f'{int(channel)}:'
    for name in names:
        if str(name).strip().startswith(prefix):
            return str(name)
    if 1 <= int(channel) <= len(names):
        return str(names[int(channel) - 1])
    return None


class RealREW:
    """실제 REW(`-api` 모드)와 이야기하는 구현.

    `interfaces.py` 의 RewStatus·DeviceLister·LevelMonitor·Measurer·IRProvider 를
    모두 만족한다. 모든 HTTP 호출에 타임아웃이 있다.
    """

    def __init__(self, host: str = '127.0.0.1', port: int = REW_API_PORT, *,
                 timeout: float = 5.0, measure_timeout: float = 180.0,
                 session: Any = None, callback_host: str = '127.0.0.1') -> None:
        if session is None and _requests is None:
            raise RewError('requests 모듈이 없어 REW API 를 쓸 수 없습니다')
        self.host = host
        self.port = int(port)
        self.base = f'http://{host}:{int(port)}'
        self.timeout = float(timeout)
        self.measure_timeout = float(measure_timeout)
        self._sess = session if session is not None else _requests.Session()
        self._callback_host = callback_host
        self._callback: CallbackServer | None = None
        self._level_cb: LevelCallback | None = None
        self._level_url: str | None = None
        self._progress_url: str | None = None
        self._lock = threading.Lock()
        self._pro_hint: bool | None = None
        self._driver: str | None = None
        self._down_until = 0.0      # 이 시각(monotonic)까지는 연결을 시도하지 않고 바로 실패
        #: 장치·채널 목록의 짧은 캐시 {키: (만료 시각, 값)} — 화면 한 번 그릴 때 줄마다 묻는다(09-22 속도).
        self._list_cache: dict[str, tuple[float, Any]] = {}
        #: 입력 레벨 콜백에서 읽을 채널 인덱스(0-기준). InputLevels 는 채널별 배열이다.
        self.level_index = 0

    # ──────────────────────── HTTP ────────────────────────
    def url(self, path: str) -> str:
        return f'{self.base}{path if path.startswith("/") else "/" + path}'

    def _request(self, method: str, path: str, *, json_body: Any = None,
                 params: dict | None = None, timeout: float | None = None) -> Any:
        # REW 가 꺼져 있으면 연결 거부에도 요청 하나가 시간이 걸린다(예전 localhost 는 IPv6 → IPv4 순으로 2~4 s).
        # 화면은 한 번 그릴 때 수십 번 묻기 때문에, 방금 연결에 실패했으면 잠깐은 바로 실패시킨다
        # (09-20: REW 없이 exe 를 켜면 창이 몇 분 동안 안 떴다).
        if time.monotonic() < self._down_until:
            raise RewError(f'REW 연결 실패({method} {path}): 방금 연결에 실패했습니다')
        try:
            read_timeout = float(timeout if timeout is not None else self.timeout)
            # (연결, 읽기) — REW 는 같은 PC 라 연결은 즉시 되거나 안 된다. 읽기만 길게 기다린다.
            response = self._sess.request(
                method, self.url(path), json=json_body, params=params,
                timeout=(min(CONNECT_TIMEOUT_S, read_timeout), read_timeout))
        except Exception as exc:  # noqa: BLE001 - requests 예외 종류가 넓다
            self._down_until = time.monotonic() + DOWN_FAST_FAIL_S
            raise RewError(f'REW 연결 실패({method} {path}): {exc}') from exc
        self._down_until = 0.0
        status = int(getattr(response, 'status_code', 0))
        if status in (401, 402, 403):
            raise RewProRequired(
                f'REW 가 거부했습니다({status} {method} {path}). REW Pro 업그레이드가 필요합니다.')
        if status >= 400:
            text = (getattr(response, 'text', '') or '')[:200]
            if 'pro' in text.lower() and path.startswith('/measure'):
                raise RewProRequired(f'REW Pro 업그레이드가 필요합니다({status}): {text}')
            raise RewError(f'REW 오류 {status} ({method} {path}): {text}')
        try:
            return response.json()
        except Exception:  # noqa: BLE001 - 본문이 없거나 JSON 이 아닐 수 있다
            return getattr(response, 'text', None)

    def get(self, path: str, params: dict | None = None,
            timeout: float | None = None) -> Any:
        return self._request('GET', path, params=params, timeout=timeout)

    def post(self, path: str, json_body: Any = None,
             timeout: float | None = None) -> Any:
        return self._request('POST', path, json_body=json_body, timeout=timeout)

    # ──────────────────────── 화면을 막지 않기(백그라운드 점검) ────────────────────────
    def hold_offline(self, seconds: float) -> None:
        """이 시간 동안은 어떤 요청도 네트워크에 가지 않고 바로 실패한다.

        위저드가 창을 만들 때 쓴다 — 창이 뜨기 전에는 REW 를 기다리지 않고, 뜬 뒤에
        `probe()` 가 백그라운드에서 실제로 물어본다(2026-09-20 사용자: "rew 연결은 프로그램
        열고 안에서 해도 되잖아").
        """
        self._down_until = time.monotonic() + max(0.0, float(seconds))

    def probe(self) -> bool:
        """REW 가 켜져 있는지 **실제로** 물어본다(빠른 실패 창을 무시). 백그라운드 스레드용.

        꺼져 있으면 `PROBE_HOLD_S` 동안 화면 쪽 요청이 바로 실패하게 해 두므로, 주기적으로
        부르는 동안에는 REW 가 꺼져 있어도 화면이 멈추지 않는다. 켜져 있으면 창을 걷는다.
        """
        self._down_until = 0.0
        try:
            self.get('/version', timeout=1.0)
        except RewError:
            self._down_until = time.monotonic() + PROBE_HOLD_S
            return False
        return True

    # ──────────────────────── RewStatus 상태·버전·Pro ────────────────────────
    def is_running(self) -> bool:
        try:
            self.get('/version', timeout=min(self.timeout, 2.0))
            return True
        except RewError:
            return False

    def version(self) -> str | None:
        """`GET /version` → APIResponse. 실응답: `{"message":"5.40 Beta 124 API 0.9.4"}`."""
        try:
            data = self.get('/version', timeout=min(self.timeout, 2.0))
        except RewError:
            return None
        if isinstance(data, dict):
            message = data.get('message')
            return str(message) if message else str(data)
        return None if data is None else str(data)

    def pro_state(self) -> bool | None:
        """Pro 라이선스 여부. 모르면 None.

        API 0.9.4 의 doc.json 에는 라이선스를 알려주는 필드·엔드포인트가 없다(전수 확인).
        그래서 `set_pro()` 로 사용자가 알려준 값만 근거가 된다.
        """
        return self._pro_hint

    def is_pro(self) -> bool:
        """모를 때는 True 로 본다. 실제로 막혀 있으면 측정 시 RewProRequired 가 난다."""
        state = self.pro_state()
        return True if state is None else bool(state)

    def set_pro(self, value: bool | None) -> None:
        """사용자가 확인해 준 Pro 여부를 고정한다."""
        self._pro_hint = value

    # ──────────────────────── REW 실행 ────────────────────────
    def launch(self, wait_s: float = 20.0) -> bool:
        """REW 를 `-api` 로 띄우고 응답할 때까지 기다린다(1단계 [REW API 모드로 실행]).

        REW 는 뜨는 데 10~20 초 걸린다. 실행 파일이 없거나 시간 안에 응답이 없으면 False.
        """
        exe = find_rew_exe()
        try:
            if _IS_MAC:
                subprocess.Popen(['open', '-a', _REW_MAC_PATH, '--args', '-api'])
            elif exe is not None:
                from secs.proc import launch_external   # exe 의 DLL 폴더를 REW 에 물려주지 않는다(09-23)
                launch_external([str(exe), '-api'])
            else:
                return False
        except Exception:  # noqa: BLE001
            return False
        deadline = time.time() + float(wait_s)
        while time.time() < deadline:
            if self.probe():          # 빠른 실패 창을 무시하고 실제로 묻는다
                return True
            time.sleep(0.5)
        return False

    # ──────────────────────── 드라이버 ────────────────────────
    def driver(self) -> str:
        """`GET /audio/driver` → `'ASIO'` 또는 `'Java'`. 한 번만 묻고 캐시한다."""
        if self._driver is None:
            try:
                data = self.get('/audio/driver')
            except RewError:
                return 'Java'
            name = data.get('driver') if isinstance(data, dict) else data
            self._driver = str(name or 'Java')
        return self._driver

    def _audio_path(self, key: str) -> str:
        """드라이버에 맞는 `/audio/...` 경로. 반대쪽 경로는 REW 가 400 으로 거부한다."""
        family = 'asio' if self.driver().strip().lower() == 'asio' else 'java'
        return _AUDIO_PATHS[family][key]

    # ──────────────────────── DeviceLister 장치 목록 ────────────────────────
    def output_devices(self) -> list[DeviceInfo]:
        return self._devices('output')

    def input_devices(self) -> list[DeviceInfo]:
        return self._devices('input')

    def _cached(self, key: str, fetch):
        """`LIST_CACHE_S` 동안 같은 목록을 다시 묻지 않는다. 실패(빈 값)는 기억하지 않는다."""
        now = time.monotonic()
        hit = self._list_cache.get(key)
        if hit is not None and hit[0] > now:
            return hit[1]
        value = fetch()
        if value:
            self._list_cache[key] = (now + LIST_CACHE_S, value)
        return value

    def _devices(self, kind: str) -> list[DeviceInfo]:
        """실응답은 그냥 문자열 배열이다(ASIO: `["MOTU Gen 5"]`).

        ASIO 는 장치 하나가 입·출력을 겸하므로 두 경로가 같다.
        """
        def fetch():
            try:
                data = self.get(self._audio_path(f'{kind}_devices'))
            except RewError:
                return []
            return self._parse_devices(data, kind)
        return list(self._cached(f'devices:{kind}', fetch))

    def _parse_devices(self, data: Any, kind: str) -> list[DeviceInfo]:
        if isinstance(data, dict):     # 방어: 단일 장치를 Device 객체로 주는 경우
            name = data.get('device') or data.get('name')
            data = [name] if name else []
        if not isinstance(data, (list, tuple)):
            return []
        channels = self._channel_count(kind)
        out: list[DeviceInfo] = []
        for index, item in enumerate(data):
            if isinstance(item, str):
                out.append(DeviceInfo(id=item, name=item, channels=channels, kind=kind))
            elif isinstance(item, dict):
                name = str(item.get('name') or item.get('device') or item.get('id') or index)
                count = item.get('channels') or item.get('channelCount')
                out.append(DeviceInfo(
                    id=str(item.get('id') or name), name=name,
                    channels=int(count) if count else channels,
                    kind=kind, extra=dict(item)))
        return out

    def _channel_count(self, kind: str) -> int:
        try:
            names = self._channel_names(kind)
        except RewError:
            return 2
        return max(2, len(names))

    def _channel_names(self, kind: str) -> list[str]:
        def fetch():
            data = self.get(self._audio_path(f'{kind}_choices'))
            if isinstance(data, dict):     # 방어: ChannelList 모양으로 올 경우
                data = data.get('channels') or data.get('values') or []
            return [str(x) for x in data] if isinstance(data, (list, tuple)) else []
        return list(self._cached(f'channels:{kind}', fetch))

    # ──────────────────────── 채널 선택 ────────────────────────
    def output_channel_names(self) -> list[str]:
        """측정 출력 선택지.

        ASIO: `"1: Main Out 1"` … `"22: Optical Out 8"` (1-기준 번호 접두).
        Java: `"L"`, `"R"`, `"L+R"` 같은 채널 이름.
        """
        try:
            return self._channel_names('output')
        except RewError:
            return []

    def input_channel_names(self) -> list[str]:
        """입력 채널 선택지(ASIO: `"3: Mic/Line/Inst In 1"` …). 화면이 번호 대신 이 이름을 보여 준다(09-22)."""
        try:
            return self._channel_names('input')
        except RewError:
            return []

    def set_output_channel(self, channel: int | str) -> None:
        """측정 출력 채널 선택 — `POST /audio/{asio,java}/output {"output": 문자열}`.

        번호를 주면 목록에서 `"N: 이름"` 접두로 찾고, 접두가 없으면 1-기준 순서로 고른다.
        목록을 못 받으면 번호를 문자열로 그대로 보낸다.
        """
        value = str(channel)
        if isinstance(channel, int):
            names = self.output_channel_names()
            value = channel_name_for(names, channel) or str(channel)
        self.post(self._audio_path('output_set'), {'output': value})

    def set_timing_ref_output(self, channel: int | str) -> None:
        """타이밍 레퍼런스 신호가 나갈 출력 채널(doc.json 2026-09-18 확인).

        ASIO 는 `POST /audio/asio/ref-output {"output": "17: 이름"}`(Output),
        Java 는 `POST /audio/java/ref-output-channel {"channel": "R"}`(OutputChannel,
        선택지 `/audio/java/ref-output-channels`). 번호는 측정 출력과 같은 규칙으로 찾는다.
        """
        value = str(channel)
        asio = self.driver().strip().lower() == 'asio'
        if isinstance(channel, int):
            if asio:
                names = self.output_channel_names()
            else:
                try:
                    data = self.get(self._audio_path('ref_output_choices'))
                except RewError:
                    data = []
                names = [str(x) for x in data] if isinstance(data, (list, tuple)) else []
            value = channel_name_for(names, channel) or str(channel)
        self.post(self._audio_path('ref_output_set'),
                  {'output': value} if asio else {'channel': value})

    # ──────────────────────── 측정 설정 ────────────────────────
    def set_blocking(self, value: bool = True) -> None:
        """POST /application/blocking.

        이름과 달리 **측정 명령은 이걸 켜도 바로 돌아온다**(2026-09-18 실스윕:
        `{"message":"Starting measurement"}`, 소요 0.0 s). 스윕이 끝났는지는
        `wait_for_new_measurement` 로만 안다.
        """
        self.post('/application/blocking', bool(value))

    def set_sample_rate(self, rate: int) -> None:
        """`POST /audio/samplerate` — 본문은 Value `{value, unit}`(정수 아님)."""
        self.post('/audio/samplerate', {'value': float(rate), 'unit': 'Hz'})

    def set_sweep(self, start_hz: float, end_hz: float, length: str | int) -> None:
        """MeasSweepConfiguration — 주파수는 int32, `length` 는 `"256k"` 같은 문자열."""
        self.post('/measure/sweep/configuration', {
            'startFrequency': int(round(float(start_hz))),
            'endFrequency': int(round(float(end_hz))),
            'length': sweep_length_name(length),
        })

    def set_level(self, dbfs: float) -> None:
        self.post('/measure/level', {'value': float(dbfs), 'unit': 'dBFS'})

    def set_repetitions(self, n: int) -> None:
        """이미 같은 값이면 보내지 않는다 — 타이밍 레퍼런스가 Acoustic 이면 REW 는 1 을 보내도
        400 "Multiple sweeps are not allowed…" 로 거절한다(10-09 위치 2 측정 실패)."""
        try:
            if int(self.get('/measure/sweep/repetitions')) == int(n):
                return
        except (RewError, TypeError, ValueError):
            pass
        self.post('/measure/sweep/repetitions', int(n))

    def set_timing_reference(self, reference: str = 'acoustic') -> None:
        """본문은 맨 JSON 문자열. 선택지는 `TIMING_REFERENCES`."""
        text = str(reference).strip()
        name = next((choice for choice in TIMING_REFERENCES
                     if choice.lower() == text.lower()), None)
        if name is None and text.lower().startswith('acoustic'):
            name = 'Acoustic'
        self.post('/measure/timing/reference', name or text)

    def set_naming(self, title: str) -> None:
        """MeasurementNaming.

        `prefixMeasNameWithOutput` 은 REW 기본이 true 라 이름 앞에 출력 채널이 붙는다.
        SECS 는 이름으로 role·위치를 되찾으므로 꺼서 넣은 제목 그대로 쓰게 한다.
        """
        self.post('/measure/naming', {'title': str(title),
                                      'namingOption': 'Use as entered',
                                      'prefixMeasNameWithOutput': False})

    def set_notes(self, notes: str) -> None:
        self.post('/measure/notes', str(notes))

    def set_sequential_channels(self, channels: Sequence[str]) -> None:
        """ChannelList `{"channels":[...]}` (테스트·도구 전용).

        선택지는 `/measure/sequential-choices`. ASIO 에서는 채널 이름이 아니라
        출력 채널 **번호 문자열**(`"1"`…`"22"`) 이 온다. SECS 는 role 마다 출력을
        바꿔 가며 한 번씩 재므로 제품 경로에서는 쓰지 않는다.
        """
        self.post('/measure/sequential-channels',
                  {'channels': [str(c) for c in channels]})

    def configure(self, plan: MeasurementPlan) -> None:
        """plan 의 측정 파라미터를 REW 에 밀어 넣는다(역할별 설정은 제외)."""
        self.set_blocking(True)
        self.set_sample_rate(plan.sample_rate)
        self.set_sweep(plan.start_frequency, plan.end_frequency, plan.sweep_length)
        self.set_level(plan.sweep_level_db)
        # 순서: REW 는 타이밍이 Acoustic 인 동안 반복 POST 를 400 으로 거절한다(10-09 실측) — 반복을 늘릴 때는
        # 타이밍(루프백 · 없음)을 먼저, 반복 1 로 돌아갈 때(음향)는 반복을 먼저(10-10 코드 리뷰).
        if int(plan.repetitions or 1) > 1:
            self.set_timing_reference(plan.timing_reference)
            self.set_repetitions(plan.repetitions)
        else:
            self.set_repetitions(plan.repetitions)
            self.set_timing_reference(plan.timing_reference)
        if (plan.timing_ref_output is not None
                and str(plan.timing_reference).strip().lower() != 'none'):
            self.set_timing_ref_output(int(plan.timing_ref_output))

    # ──────────────────────── 측정 목록 ────────────────────────
    def measurements_map(self) -> dict[str, dict]:
        """`GET /measurements` → `{"1": MeasurementSummary, ...}`.

        키는 REW 목록에서의 1-기준 순번 문자열이고, 값에 `uuid`·`title`·`date` 가 있다.
        """
        data = self.get('/measurements', timeout=max(self.timeout, 10.0))
        if isinstance(data, dict):
            return {str(k): v for k, v in data.items() if isinstance(v, dict)}
        if isinstance(data, list):
            return {str(i + 1): m for i, m in enumerate(data) if isinstance(m, dict)}
        return {}

    def measurements(self) -> list[dict]:
        """측정 요약 목록(REW 목록 순서)."""
        items = self.measurements_map()
        return [items[key] for key in sorted(items, key=_index_key)]

    def measurement_ids(self) -> set[str]:
        out: set[str] = set()
        for m in self.measurements():
            uuid = m.get('uuid') or m.get('id')
            if uuid:
                out.add(str(uuid))
        return out

    # ──────────────────────── 측정 명령 ────────────────────────
    def start_sweep(self, timeout: float | None = None) -> Any:
        """POST /measure/command {'command': 'SPL'} — 스윕 시작만 시킨다.

        `set_blocking(True)` 여도 **바로 돌아온다**(2026-09-18 실스윕). 부른 쪽은
        `wait_for_new_measurement` 로 새 측정이 REW 목록에 생기기를 기다려야 한다.
        """
        return self.post('/measure/command', {'command': 'SPL'},
                         timeout=timeout if timeout is not None else self.measure_timeout)

    def abort(self) -> None:
        """측정 취소 — `/measure/commands` 실목록에 있는 이름은 `Cancel` 이다."""
        try:
            self.post('/measure/command', {'command': 'Cancel'}, timeout=self.timeout)
        except RewError:
            pass

    # ──────────────────────── 결과 읽기(IRProvider) ────────────────────────
    def impulse_response(self, uuid: str) -> tuple[np.ndarray, int, float]:
        """`(IR, 샘플레이트, t0)`. t0 는 배열 인덱스 0 의 절대 시각(초).

        ImpulseResponse 스키마의 `startTime` 이 그 값이다(doc.json 확인).
        타이밍 레퍼런스를 쓰면 t=0 앞에 여유 구간이 있어 음수로 온다.
        WAV 에는 담기지 않으니 `.secs` 에 따로 남겨야 한다.
        """
        data = self.get(f'/measurements/{uuid}/impulse-response',
                        params={'normalised': 'false'}, timeout=max(self.timeout, 15.0))
        if not isinstance(data, dict) or 'data' not in data:
            raise RewError(f'임펄스 응답 응답 형식이 예상과 다릅니다: {uuid}')
        return (_b64_be_f32(data['data']),
                int(data.get('sampleRate', 48000)),
                _as_float(data.get('startTime'), 0.0) or 0.0)

    def get_ir(self, uuid: str) -> tuple[np.ndarray, int]:
        """IRProvider 프로토콜. t0 까지 필요하면 `impulse_response()` 를 쓸 것."""
        ir, rate, _t0 = self.impulse_response(uuid)
        return ir, rate

    def spl_offset_db(self, uuid: str) -> float | None:
        """`GET /measurements/{uuid}` → `splOffsetdB`(IR 진폭을 dB SPL 로 바꾸는 값, 10-09 실측 116.12)."""
        try:
            data = self.get(f'/measurements/{uuid}', timeout=min(self.timeout, 5.0))
        except RewError:
            return None
        return _as_float(data.get('splOffsetdB')) if isinstance(data, dict) else None

    def snr_db(self, uuid: str) -> float | None:
        """`GET /measurements/{uuid}` → MeasurementSummary.signalToNoisedB(실스윕 확인)."""
        try:
            data = self.get(f'/measurements/{uuid}', timeout=min(self.timeout, 5.0))
        except RewError:
            return None
        if not isinstance(data, dict):
            return None
        return _as_float(data.get('signalToNoisedB'))

    def measure_level_dbfs(self) -> float | None:
        """`GET /measure/level` → Value `{value, unit}`(2026-09-23 실검증, 1단계 [설정 읽기]).

        실기(REW 5.40 Beta 124, 캘리브레이션 없음 상태)에서 `{"value":-24.0,"unit":"dBFS"}`
        로 왔다. `set_level()` 이 같은 모양으로 쓰는 값을 그대로 읽어 온다.
        """
        try:
            data = self.get('/measure/level', timeout=min(self.timeout, 3.0))
        except RewError:
            return None
        if not isinstance(data, dict):
            return None
        return _as_float(data.get('value'))

    def input_calibration_file(self) -> str | None:
        """`GET /audio/input-cal` → AudioInputCal.calDataAllInputs.calFilePath(2026-09-23 실검증).

        실기(REW 5.40 Beta 124, MOTU Gen 5, 캘리브레이션 파일 없음)에서
        `{"calDataAllInputs":{"calFilePath":"",...},"separateCalFileForEachInput":false,...}`
        로 왔다 — 빈 문자열이 "파일 없음"이라는 확정값이다. `separateCalFileForEachInput`
        이 true 면 입력마다 파일이 다를 수 있어 이 엔드포인트 하나로는 알 수 없다
        (추정 — 그 경우는 실기로 확인 못 했다) → None(알 수 없음)으로 본다.
        """
        try:
            data = self.get('/audio/input-cal', timeout=min(self.timeout, 3.0))
        except RewError:
            return None
        if not isinstance(data, dict):
            return None
        if data.get('separateCalFileForEachInput'):
            return None
        cal = data.get('calDataAllInputs')
        if not isinstance(cal, dict):
            return None
        return str(cal.get('calFilePath') or '')

    def spl(self, meter: int = 1) -> float | None:
        """`GET /spl-meter/{id}/levels` → SPLValues. 값 필드는 소문자 `spl`.

        미터가 열려 있지 않으면 REW 가 -180 을 주므로 그때는 None 으로 본다.
        """
        try:
            data = self.get(f'/spl-meter/{int(meter)}/levels',
                            timeout=min(self.timeout, 3.0))
        except RewError:
            return None
        if not isinstance(data, dict):
            return None
        for key in ('spl', 'SPL'):     # 대문자는 옛 응답 방어용
            value = _as_float(data.get(key))
            if value is not None:
                return None if value <= -179.0 else value
        return None

    # ──────────────────────── Measurer 한 위치 측정 ────────────────────────
    def run_position(self, plan: MeasurementPlan, pos: int,
                     on_progress: ProgressCallback | None = None,
                     cancel: threading.Event | None = None) -> list[MeasurementResult]:
        """한 위치의 모든 role 을 순차 측정(블로킹). 취소되면 그때까지의 결과만 반환."""
        cancel = cancel or threading.Event()
        results: list[MeasurementResult] = []
        self.configure(plan)

        # REW 의 진행 문자열("NN% 설명")을 화면 진행률로 넘긴다. 스윕 구간은 0~90 %
        # (90 부터는 IR 수신). 이미 누가 구독 중이면(실측 도구) 건드리지 않는다.
        # 진행 콜백은 REW 가 부르는 별도 스레드에서 온다. 스윕이 끝난 뒤 늦게 도착한 'running' 이
        # 'done' 뒤에 끼지 않게, 역할을 비우는 것과 전달을 같은 잠금 아래에서 한다(09-23 게이트에서 발견).
        current: dict[str, Any] = {'role': '', 'lock': threading.Lock()}
        own_subscription = False
        if on_progress is not None and self._progress_url is None:
            def relay(body: Any) -> None:
                percent, message = self.parse_progress(body)
                if percent is None:
                    return
                with current['lock']:
                    if not current['role']:
                        return
                    emit(on_progress, progress(current['role'], pos, 'running',
                                               min(max(percent, 0.0), 100.0) * 0.9, message))
            try:
                self.subscribe_progress(relay)
                own_subscription = True
            except (RewError, OSError):
                self._progress_url = None       # 진행률은 없어도 측정은 된다
        try:
            return self._run_roles(plan, pos, on_progress, cancel, results, current)
        finally:
            if own_subscription:
                self.unsubscribe_progress()

    def _run_roles(self, plan: MeasurementPlan, pos: int,
                   on_progress: ProgressCallback | None, cancel: threading.Event,
                   results: list[MeasurementResult],
                   current: dict[str, Any]) -> list[MeasurementResult]:
        lock = current.get('lock') or threading.Lock()
        for role in plan.roles:
            with lock:
                current['role'] = role
            if cancel.is_set():
                emit(on_progress, progress(role, pos, 'cancelled', 0.0, '측정을 취소했습니다'))
                return results
            name = plan.name_for(role, pos)
            emit(on_progress, progress(role, pos, 'sweep', 0.0, f'{role} 스윕 (위치 {pos})'))

            self.set_output_channel(plan.output_for(role))
            self.set_naming(name)
            base_role, band = split_role(role)
            self.set_notes(f'SECS {base_role}{"/" + band if band else ""} P{pos} '
                           f'({plan.timing_reference} timing)')

            before = self.measurement_ids()
            try:
                self.start_sweep()
            except RewProRequired:
                raise
            except RewError:
                if cancel.is_set():
                    emit(on_progress, progress(role, pos, 'cancelled', 0.0, '측정을 취소했습니다'))
                    return results
                raise
            if cancel.is_set():
                self.abort()
                emit(on_progress, progress(role, pos, 'cancelled', 0.0, '측정을 취소했습니다'))
                return results

            # 실측(2026-09-18): blocking 을 켜도 측정 명령은 "Starting measurement" 로
            # 바로 돌아온다. 스윕이 끝났는지는 새 측정이 목록에 생기는 것으로만 안다.
            uuid = self.wait_for_new_measurement(before, self.sweep_wait_s(plan), cancel)
            if cancel.is_set():
                self.abort()
                emit(on_progress, progress(role, pos, 'cancelled', 0.0, '측정을 취소했습니다'))
                return results
            with lock:
                current['role'] = ''            # 스윕 끝 — 이 뒤의 늦은 진행 콜백은 버린다
            if uuid is None:
                raise RewError(f'{name}: 새 측정을 찾지 못했습니다')
            emit(on_progress, progress(role, pos, 'process', 90.0, f'{role} 임펄스 응답 수신'))
            ir, rate, t0 = self.impulse_response(uuid)
            results.append(MeasurementResult(role=role, pos=int(pos), uuid=uuid, ir=ir,
                                             sample_rate=rate, name=name, spl_db=self.spl(),
                                             t0_s=t0, snr_db=self.snr_db(uuid),
                                             spl_offset_db=self.spl_offset_db(uuid)))
            emit(on_progress, progress(role, pos, 'done', 100.0, f'{role} 완료'))
        return results

    # ──────────────────────── 스윕 완료 대기 ────────────────────────
    @staticmethod
    def sweep_wait_s(plan: MeasurementPlan) -> float:
        """스윕 한 번을 기다릴 상한(초) = 스윕 길이 × 반복 + 여유 30 s.

        256k·48 kHz 실측(2026-09-18): 잡음 바닥 캡처 3 s + 스윕 7 s + 처리 = 9.9 s.
        """
        try:
            sweep = sweep_length_samples(plan.sweep_length) / float(plan.sample_rate)
        except Exception:  # noqa: BLE001 - 모르는 길이 표기면 가장 긴 4M 로 본다
            sweep = 4_194_304 / 48000.0
        return sweep * max(1, int(plan.repetitions)) + 30.0

    def wait_for_new_measurement(self, before: set[str], timeout_s: float,
                                 cancel: threading.Event | None = None,
                                 interval: float = 0.5) -> str | None:
        """`before` 에 없던 측정이 REW 목록에 생길 때까지 기다린다. 없으면 None."""
        deadline = time.monotonic() + max(1.0, float(timeout_s))
        while time.monotonic() < deadline:
            if cancel is not None and cancel.is_set():
                return None
            uuid = self._new_measurement_id(before, tries=1)
            if uuid is not None:
                return uuid
            time.sleep(interval)
        return None

    def _new_measurement_id(self, before: set[str], tries: int = 20,
                            interval: float = 0.25) -> str | None:
        """`before` 에 없는 uuid 를 목록에서 찾는다. 없으면 None.

        제품 경로는 `wait_for_new_measurement` 가 `tries=1` 로 부르고 재시도는 바깥
        루프가 맡는다(취소를 그때그때 보기 위해).
        여럿이면 REW 목록의 마지막 것(가장 큰 순번 키)을 고른다. uuid 를 사전순으로
        고르면 순서가 뒤집힐 수 있다.
        """
        for _ in range(max(1, tries)):
            items = self.measurements_map()
            added = [(key, str(m.get('uuid') or m.get('id') or ''))
                     for key, m in items.items()
                     if str(m.get('uuid') or m.get('id') or '') not in before]
            added = [(key, uuid) for key, uuid in added if uuid]
            if added:
                return max(added, key=lambda item: _index_key(item[0]))[1]
            time.sleep(interval)
        return None

    # ──────────────────────── 콜백 서버 ────────────────────────
    def _ensure_callback(self) -> CallbackServer:
        if self._callback is None:
            self._callback = CallbackServer(host=self._callback_host)
            self._callback.start()
        elif not self._callback.is_running:
            self._callback.start()
        return self._callback

    # ──────────────────────── LevelMonitor 입력 레벨·제너레이터 ────────────────────────
    def start(self, on_level: LevelCallback) -> None:
        """입력 레벨 모니터 시작. REW 가 콜백 서버 URL 로 POST 한다.

        구독 본문은 Subscriber `{url, parameters}`, 명령은 Start/Stop(실목록 확인).
        단위 선택지는 `/input-levels/units` = SPL/dBFS/dBu/dBV/dBW/V/W.
        콜백 본문은 `/input-levels/last-levels` 와 같은 InputLevels 다(2026-09-18
        실스윕 확인). 배열에는 선택된 입력 하나만 오고, `timeSpanSeconds` 는 여기서
        요청한 0.25 가 아니라 0.1707 로 왔다(2초에 11건).
        """
        self._level_cb = on_level
        server = self._ensure_callback()
        self._level_url = server.route('/levels', self._on_level_payload)
        self.post('/input-levels/subscribe', {
            'url': self._level_url,
            'parameters': {'unit': 'dBFS', 'timeSpanSeconds': 0.25},
        })
        self.post('/input-levels/command', {'command': 'Start'})

    def last_levels(self) -> dict:
        """`GET /input-levels/last-levels` → InputLevels (테스트·도구 전용).

        구독(`start`)이 막혔을 때의 폴링 대체 경로다. 제품 화면은 구독만 쓴다.
        모니터링 전에는 빈 객체 `{}` 가 온다.
        """
        try:
            data = self.get('/input-levels/last-levels', timeout=min(self.timeout, 3.0))
        except RewError:
            return {}
        return data if isinstance(data, dict) else {}

    def stop(self) -> None:
        for path, body in (('/input-levels/command', {'command': 'Stop'}),
                           ('/input-levels/unsubscribe', {'url': self._level_url})):
            if body.get('url') is None and path.endswith('unsubscribe'):
                continue
            try:
                self.post(path, body)
            except RewError:
                pass
        self._level_cb = None
        self._level_url = None
        if self._callback is not None and self._progress_url is None:
            self._callback.stop()
            self._callback = None

    def _on_level_payload(self, payload: Any) -> None:
        callback = self._level_cb
        if callback is None:
            return
        try:
            callback(self._level_payload(payload, self.level_index))
        except Exception:  # noqa: BLE001 - UI 콜백 오류를 서버로 되돌리지 않는다
            pass

    @staticmethod
    def _level_payload(payload: Any, index: int = 0) -> dict:
        """InputLevels → `{'input_dbfs','spl_db','clip'}`.

        실제 모양은 `{"unit":"dBFS","rms":[…],"peak":[…],"timeSpanSeconds":0.1707}` 로
        **채널별 배열**이다(실스윕에서는 선택된 입력 하나뿐이었다).
        `index` 로 마이크가 물린 입력을 고른다.
        단위가 SPL 이면 같은 값을 `spl_db` 에도 넣는다. 스칼라 본문도 받아 준다.
        """
        rms: float | None = None
        peak: float | None = None
        spl: float | None = None
        unit = ''
        data = payload
        if isinstance(data, dict):
            inputs = data.get('inputs') or data.get('levels')
            if isinstance(inputs, list) and inputs and isinstance(inputs[0], dict):
                data = inputs[0]
            if isinstance(data, dict):
                unit = str(data.get('unit') or '')
                rms = _first_float(data.get('rms', data.get('rmsLevel', data.get('level'))),
                                   index)
                peak = _first_float(data.get('peak', data.get('peakLevel')), index)
                spl = _first_float(data.get('spl', data.get('SPL')), index)
        if spl is None and unit.strip().upper() == 'SPL':
            spl = rms if rms is not None else peak
        return {
            'input_dbfs': rms if rms is not None else (peak if peak is not None else -120.0),
            'spl_db': spl,
            'clip': bool(peak is not None and peak > -1.0),
        }

    def play_pink(self, role: str, output: int, level_db: float) -> None:
        """레벨 확인 신호 재생. 서브는 50 Hz 사인, 그 외 핑크 노이즈.

        신호 id 는 소문자(`/generator/signals` 실목록), 명령 경로는 `/generator/command`
        (단수)이고 시작 명령은 `Play` 다. 사인 주파수는 신호별 설정 엔드포인트로 넣는다.
        """
        self.set_output_channel(int(output))
        base_role, _band = split_role(role)
        is_sub = base_role.upper().startswith('SW')
        signal = SIGNAL_SINE if is_sub else SIGNAL_PINK_NOISE
        self.post('/generator/signal', {'signal': signal})
        if is_sub:
            self.post('/generator/signals/sine/configuration', {'frequency': 50.0})
        self.post('/generator/level', {'value': float(level_db), 'unit': 'dBFS'})
        self.post('/generator/command', {'command': 'Play'})

    def stop_generator(self) -> None:
        try:
            self.post('/generator/command', {'command': 'Stop'})
        except RewError:
            pass

    # ──────────────────────── 진행률 구독 ────────────────────────
    def subscribe_progress(self, handler: Callable[[Any], None]) -> str:
        """`POST /measure/subscribe` — 본문은 Subscriber `{url, parameters}`.

        콜백 본문은 doc.json 에 없지만 실스윕(2026-09-18)에서 맨 JSON 문자열
        `"NN% 설명"` 으로 확인했다(약 0.17 s 간격) — `parse_progress` 가 읽는다.
        """
        server = self._ensure_callback()
        self._progress_url = server.route('/measure-progress', handler)
        self.post('/measure/subscribe', {'url': self._progress_url, 'parameters': {}})
        return self._progress_url

    @staticmethod
    def parse_progress(body: Any) -> tuple[float | None, str]:
        """측정 진행 콜백 본문 → `(퍼센트, 설명)`.

        실스윕(2026-09-18) 확인: 본문은 객체가 아니라 맨 JSON 문자열이다 —
        `"0% Capturing noise floor...13%"`, `"41% Remaining sweeps: 1, time: 4 s"`,
        …, `"100% Measurement complete"`. 모양이 다르면 퍼센트는 None.
        """
        text = '' if body is None else str(body).strip()
        head, _, rest = text.partition('%')
        try:
            return float(head), rest.strip()
        except ValueError:
            return None, text

    def unsubscribe_progress(self) -> None:
        """`POST /measure/unsubscribe`. 안 부르면 REW 에 죽은 구독자가 남는다."""
        if self._progress_url is None:
            return
        try:
            self.post('/measure/unsubscribe', {'url': self._progress_url})
        except RewError:
            pass
        self._progress_url = None

    def process_result(self) -> dict:
        """`GET /measurements/process-result` → ProcessResult (테스트·도구 전용).

        `{processName, message, results}`. 측정 중에도 "There is no process" 가 오므로
        (2026-09-18) 완료 판정에는 쓸 수 없다 — `tools/rew_live_*.py` 의 기록용이다.
        """
        try:
            data = self.get('/measurements/process-result')
        except RewError:
            return {}
        return data if isinstance(data, dict) else {}

    # ──────────────────────── 수명 ────────────────────────
    def close(self) -> None:
        """제너레이터 Stop · 구독 해제 · 콜백 서버 종료.

        **측정 중에는 부르지 말 것**: 2026-09-18 실스윕에서 스윕 41 % 지점에 불렀더니
        측정이 1.25 kHz 위가 빈 채로 저장됐다.
        """
        self.stop_generator()
        self.unsubscribe_progress()
        try:
            self.stop()
        except RewError:
            pass
        if self._callback is not None:
            self._callback.stop()
            self._callback = None

    def __enter__(self) -> 'RealREW':
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()


__all__ = ['RealREW', 'REW_API_PORT', 'plan_from_project', 'find_rew_exe',
           'sweep_length_name', 'channel_name_for', 'SWEEP_LENGTH_NAMES',
           'MEASURE_COMMANDS', 'TIMING_REFERENCES',
           'SIGNAL_PINK_NOISE', 'SIGNAL_SINE']
