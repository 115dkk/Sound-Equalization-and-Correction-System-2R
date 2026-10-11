"""P2 B: REW 가 없는 환경용 가짜 구현.

`FakeREW` 하나가 `interfaces.py` 의 다섯 프로토콜(RewStatus·DeviceLister·
LevelMonitor·Measurer·IRProvider)을 전부 구현한다. 위저드(C)·통합 테스트(E) 가
REW 없이 전 흐름을 돌리는 데 쓴다.

IR 은 세 층 중 하나에서 온다(우선순위 순):
1. `use_sample_dolby=True` 이고 role 이 원측정 14개(7.1.4 의 L·R·C·Ls·Rs·Lb·Rb·
   Tfl·Tfr·Tbl·Tbr + 서브 SW1 = DBA L · SW2 = DBA R · SW3 = INFRA 인프라 서브)에 있으면
   `sample_dolby_714.npz`(2026-03-10 사용자의 실제 돌비룸 측정, `tools/make_sample_dolby.py` 로
   뽑음 — 서브 3개는 2026-10-11 사용자 결정, 전에는 DBA+INFRA 합 하나를 SW1 로 썼다)의 **실측 IR**.
   실측은 위치 1 뿐이라 샘플 모드는 **위치 2 이상을 재지 않는다** — `run_position`/`ir_for` 가
   `RewError`(`SAMPLE_SINGLE_POSITION`)로 막는다(2026-09-24 사용자 결정: 변형한 가짜 위치를 채우면
   7단계 최적화 목표에 따라 결과가 인공적으로 달라진다). 실측 밖 합성 role 도 같은 규칙
   (샘플 모드 = 1점)을 따른다 — 위치 1 에서만 합성. 바이앰프 대역
   (`role:LF` 등)·이 14개 밖의 role(SW4 이상·Lw/Rw·Tml/Tmr 등)은 대상이 아니다 —
   샘플 프로젝트가 7.1.4 · 서브 3 · 바이앰프 끔이라 실측이 없다.
2. `tests/synth.py` 의 `role_measurement_set`(지연 임포트).
3. 그 파일도 없으면(배포본) 같은 성격의 자체 합성 `_fallback_ir`.

어느 쪽이든 같은 입력에 항상 같은 배열을 돌려준다(결정적). `--fake-rew` 로 여는
샘플 모드는 1 을 쓰고(`secs.wizard.build_backend`), 기존 테스트·도구는 `use_sample_dolby`
기본값이 꺼져 있어(합성 IR 을 가정한 회귀 값이 많다) 그대로 2·3 을 쓴다.
"""
from __future__ import annotations

import json
import math
import os
import sys
import threading
from pathlib import Path
from typing import Any

import numpy as np

from secs.rewapi.interfaces import (
    DeviceInfo, LevelCallback, MeasurementPlan, MeasurementResult, ProgressCallback,
    RewError, emit, progress, split_role,
)

# 목업 역할 순서(=tests/synth.py 와 같은 순서). synth 가 없을 때도 쓰인다.
_MAIN_ROLE_ORDER: tuple[str, ...] = (
    'L', 'R', 'C', 'Ls', 'Rs', 'Lb', 'Rb', 'Lw', 'Rw',
    'Tfl', 'Tfr', 'Tml', 'Tmr', 'Tbl', 'Tbr',
)
_SUB_ROLE_ORDER: tuple[str, ...] = tuple(f'SW{i}' for i in range(1, 9))

_DEFAULT_SAMPLE_RATE = 48000
_DEFAULT_DURATION_S = 1.0


# ──────────────────────────── IR 합성 ────────────────────────────
def _tests_dir() -> Path | None:
    """저장소의 `tests` 디렉터리(있으면). 환경변수 `SECS_SYNTH_DIR` 이 우선."""
    env = os.environ.get('SECS_SYNTH_DIR', '').strip()
    if env:
        p = Path(env)
        return p if (p / 'synth.py').is_file() else None
    here = Path(__file__).resolve()
    for parent in here.parents:
        candidate = parent / 'tests' / 'synth.py'
        if candidate.is_file():
            return candidate.parent
    return None


def _load_synth():
    """`tests/synth.py` 모듈(지연 임포트·경로 주입). 없으면 None."""
    if 'synth' in sys.modules:
        return sys.modules['synth']
    directory = _tests_dir()
    if directory is None:
        return None
    if str(directory) not in sys.path:
        sys.path.insert(0, str(directory))
    try:
        import synth  # type: ignore[import-not-found]
    except Exception:  # noqa: BLE001 - 배포본에는 tests 가 없다
        return None
    return synth


#: `tools/make_sample_dolby.py` 가 만드는 npz 의 모듈 캐시. 프로세스에서 한 번만 읽는다.
_SAMPLE_DOLBY_CACHE: dict | None = None


def _sample_dolby_path() -> Path:
    """샘플 npz 경로. 환경변수 `SECS_SAMPLE_DOLBY_NPZ` 가 있으면 그것부터.

    개발 트리·frozen exe 양쪽에서 `Path(__file__).parent`(=`secs/rewapi/`)가 실제
    파일 위치와 같다(`secs.wizard.branding.ASSETS` 와 같은 방식 — PyInstaller 가
    `__file__` 을 `sys._MEIPASS` 기준으로 되돌려 준다). `SECS.spec` 의 `datas` 가
    이 파일을 같은 상대 경로로 exe 에 담는다.
    """
    env = os.environ.get('SECS_SAMPLE_DOLBY_NPZ', '').strip()
    if env:
        return Path(env)
    return Path(__file__).resolve().parent / 'sample_dolby_714.npz'


def _load_sample_dolby() -> dict:
    """샘플 npz 를 읽어 `{role: (ir: float32[], sample_rate: int, t0_s: float)}` 로.

    파일이 없거나 읽기 실패면 빈 dict(부르는 쪽은 role 이 없으면 합성으로 넘어간다).
    """
    global _SAMPLE_DOLBY_CACHE
    if _SAMPLE_DOLBY_CACHE is not None:
        return _SAMPLE_DOLBY_CACHE
    path = _sample_dolby_path()
    table: dict = {}
    if path.is_file():
        try:
            with np.load(path) as data:
                roles = json.loads(str(data['meta_json'])).get('roles', [])
                sample_rate = int(json.loads(str(data['meta_json'])).get('sample_rate', _DEFAULT_SAMPLE_RATE))
                t0_values = np.asarray(data['t0'], dtype=np.float64)
                for i, role in enumerate(roles):
                    ir = np.asarray(data[f'ir_{role}'], dtype=np.float32)
                    table[role] = (ir, sample_rate, float(t0_values[i]))
        except Exception:  # noqa: BLE001 - 손상된 npz 는 합성으로 대체
            table = {}
    _SAMPLE_DOLBY_CACHE = table
    return table


def _fallback_ir(role: str, sample_rate: int, duration_s: float) -> np.ndarray:
    """synth 없이 쓰는 자체 합성 IR: 직접음 + 감쇠 반사 + 역할별 공진."""
    length = int(round(sample_rate * duration_s))
    seed = 20260916 + (abs(hash(role)) % 100000)
    rng = np.random.default_rng(seed)
    ir = np.zeros(length, dtype=np.float64)

    is_sub = role.upper().startswith('SW')
    direct = int(round(0.003 * sample_rate))
    ir[direct] = 1.0
    # 초기 반사 몇 개 + 지수 감쇠 노이즈 꼬리
    for _ in range(12):
        idx = direct + int(rng.integers(20, int(0.08 * sample_rate)))
        if idx < length:
            ir[idx] += float(rng.normal(0.0, 0.18))
    tail = rng.normal(0.0, 1.0, length)
    decay = np.exp(-np.arange(length) / (0.12 * sample_rate))
    ir += 0.04 * tail * decay

    # 역할별 룸 모드(저역 공진) 흉내: 감쇠 사인 몇 개를 더한다.
    t = np.arange(length) / sample_rate
    modes = (28.0, 43.0, 67.0) if is_sub else (52.0, 118.0, 240.0)
    for k, f0 in enumerate(modes):
        ir += 0.05 / (k + 1) * np.sin(2 * math.pi * f0 * t) * np.exp(-t / 0.25)

    if is_sub:  # 서브는 저역만 남도록 1차 저역통과를 두 번
        alpha = 1.0 - math.exp(-2 * math.pi * 120.0 / sample_rate)
        for _ in range(2):
            out = np.empty_like(ir)
            acc = 0.0
            for i, x in enumerate(ir):
                acc += alpha * (x - acc)
                out[i] = acc
            ir = out
    return ir.astype(np.float32)


def _position_variation(ir: np.ndarray, pos: int, sample_rate: int) -> np.ndarray:
    """위치마다 소량 변화: 지연 + 게인(결정적)."""
    step = max(0, int(pos) - 1)
    if step == 0:
        return np.asarray(ir, dtype=np.float32).copy()
    delay = int(round(step * 0.35e-3 * sample_rate))     # 위치당 0.35 ms
    gain = float(10.0 ** (-0.4 * step / 20.0))           # 위치당 -0.4 dB
    out = np.zeros_like(np.asarray(ir, dtype=np.float32))
    if delay < out.size:
        out[delay:] = np.asarray(ir, dtype=np.float32)[: out.size - delay]
    return (out * gain).astype(np.float32)


# ──────────────────────────── FakeREW ────────────────────────────
#: 샘플 모드(`use_sample_dolby=True`)에서 위치 2 이상을 재려 할 때의 알림 — 5단계 로그에 그대로 뜬다.
SAMPLE_SINGLE_POSITION = '샘플은 실측 1점뿐 — 위치 2 이상은 잴 수 없습니다'


class FakeREW:
    """REW 대역품. RewStatus·DeviceLister·LevelMonitor·Measurer·IRProvider."""

    def __init__(self, *,
                 delay_s: float = 0.05,
                 sample_rate: int = _DEFAULT_SAMPLE_RATE,
                 duration_s: float = _DEFAULT_DURATION_S,
                 running: bool = True,
                 version: str = '5.40 beta 135 (demo)',
                 pro: bool = True,
                 output_devices: list[DeviceInfo] | None = None,
                 input_devices: list[DeviceInfo] | None = None,
                 level_interval_s: float = 0.02,
                 t0_s: float = 0.0,
                 use_test_synth: bool = True,
                 use_sample_dolby: bool = False,
                 measure_level_dbfs: float = -20.0,
                 cal_file: str = '') -> None:
        self.delay_s = float(delay_s)
        self.sample_rate = int(sample_rate)
        self.duration_s = float(duration_s)
        #: 1단계 [설정 읽기]가 읽는 두 값 — 실제 REW 는 `GET /measure/level`·
        #: `GET /audio/input-cal` 로 준다(2026-09-23 실검증, `보고서/REW API 실검증`).
        #: `cal_file` 빈 문자열 = 파일 없음(실기 확인값과 같은 뜻).
        self._measure_level_dbfs = float(measure_level_dbfs)
        self._cal_file = str(cal_file or '')
        #: 측정 결과에 실을 IR 시간 원점. 실제 REW 는 ImpulseResponse.startTime 을 준다.
        #: 기본 0.0 이고, 음수 t0 를 시험하려면 인자로 준다.
        self.t0_s = float(t0_s)
        self._running = bool(running)
        self._version = version
        self._pro = bool(pro)
        self.level_interval_s = float(level_interval_s)
        self.use_test_synth = bool(use_test_synth)
        #: 09-24: `--fake-rew` 샘플 모드에서만 켠다(`secs.wizard.build_backend`). 기존
        #: 테스트·도구는 합성 IR 의 결정적 회귀 값을 쓰므로 기본은 꺼져 있다.
        self.use_sample_dolby = bool(use_sample_dolby)

        self._outputs = output_devices if output_devices is not None else [
            DeviceInfo(id='fake-out-0', name='Demo ASIO 16ch', channels=16, kind='output'),
            DeviceInfo(id='fake-out-1', name='Demo Speakers (2ch)', channels=2, kind='output'),
        ]
        self._inputs = input_devices if input_devices is not None else [
            DeviceInfo(id='fake-in-0', name='Demo USB Mic', channels=1, kind='input'),
            DeviceInfo(id='fake-in-1', name='Demo Interface In', channels=8, kind='input'),
        ]

        self._ir_cache: dict[str, np.ndarray] = {}
        self._by_uuid: dict[str, tuple[np.ndarray, int]] = {}
        self._counter = 0
        self._lock = threading.Lock()

        # 레벨 모니터 상태
        self._level_thread: threading.Thread | None = None
        self._level_stop = threading.Event()
        self._gen_role: str | None = None
        self._gen_output: int = 0
        self._gen_level_db: float = -20.0
        self.generator_running = False
        self.level_running = False

        # 테스트가 들여다볼 호출 기록
        self.calls: list[tuple[str, dict]] = []

    # ── RewStatus ──
    def is_running(self) -> bool:
        return self._running

    def version(self) -> str | None:
        return self._version if self._running else None

    def is_pro(self) -> bool:
        return self._pro and self._running

    def set_running(self, value: bool) -> None:
        """테스트·위저드에서 'REW 가 꺼진' 상태를 흉내낼 때."""
        self._running = bool(value)

    def measure_level_dbfs(self) -> float | None:
        """`GET /measure/level` 흉내(1단계 [설정 읽기]). 꺼져 있으면 모른다."""
        return self._measure_level_dbfs if self._running else None

    def input_calibration_file(self) -> str | None:
        """`GET /audio/input-cal` 흉내. 꺼져 있으면 None(알 수 없음), 켜져 있으면
        빈 문자열(파일 없음) 또는 경로."""
        return self._cal_file if self._running else None

    # ── DeviceLister ──
    def output_devices(self) -> list[DeviceInfo]:
        return list(self._outputs)

    def input_devices(self) -> list[DeviceInfo]:
        return list(self._inputs)

    def output_channel_names(self) -> list[str]:
        """실제 REW 의 `"N: 이름"` 꼴을 흉내 낸다(가장 채널이 많은 가짜 출력 장치 기준)."""
        count = max((d.channels for d in self._outputs), default=2)
        return [f'{n}: Demo Out {n}' for n in range(1, count + 1)]

    def input_channel_names(self) -> list[str]:
        count = max((d.channels for d in self._inputs), default=1)
        return [f'{n}: Demo In {n}' for n in range(1, count + 1)]

    # ── LevelMonitor ──
    def start(self, on_level: LevelCallback) -> None:
        self.stop()
        self._level_stop.clear()
        self.level_running = True
        self.calls.append(('level_start', {}))

        def _run() -> None:
            tick = 0
            while not self._level_stop.is_set():
                try:
                    on_level(self._level_sample(tick))
                except Exception:  # noqa: BLE001 - UI 콜백 오류로 스레드를 죽이지 않는다
                    pass
                tick += 1
                if self._level_stop.wait(self.level_interval_s):
                    break

        self._level_thread = threading.Thread(target=_run, name='fake-rew-levels', daemon=True)
        self._level_thread.start()

    def stop(self) -> None:
        self._level_stop.set()
        thread, self._level_thread = self._level_thread, None
        if thread is not None and thread.is_alive():
            thread.join(timeout=2.0)
        self.level_running = False

    def play_pink(self, role: str, output: int, level_db: float) -> None:
        self._gen_role = str(role)
        self._gen_output = int(output)
        self._gen_level_db = float(level_db)
        self.generator_running = True
        self.calls.append(('play_pink', {'role': role, 'output': int(output),
                                         'level_db': float(level_db)}))

    def stop_generator(self) -> None:
        self.generator_running = False
        self._gen_role = None
        self.calls.append(('stop_generator', {}))

    def _level_sample(self, tick: int) -> dict:
        """결정적 레벨 샘플. 재생 중이면 설정 레벨 근처, 아니면 암소음."""
        if not self.generator_running:
            dbfs = -62.0 + 1.5 * math.sin(tick * 0.7)
        else:
            base = self._gen_level_db + 2.0
            dbfs = base + 1.2 * math.sin(tick * 0.9)
        clip = dbfs > -1.0
        spl = None if not self.generator_running else 75.0 + (dbfs - self._gen_level_db)
        return {'input_dbfs': float(dbfs),
                'spl_db': None if spl is None else float(spl),
                'clip': bool(clip)}

    # ── Measurer ──
    def run_position(self, plan: MeasurementPlan, pos: int,
                     on_progress: ProgressCallback | None = None,
                     cancel: threading.Event | None = None) -> list[MeasurementResult]:
        """한 위치의 모든 role 을 순차 측정한 척한다(블로킹)."""
        results: list[MeasurementResult] = []
        cancel = cancel or threading.Event()
        self.calls.append(('run_position', {'pos': int(pos), 'roles': list(plan.roles)}))
        self._check_sample_position(pos)

        for role in plan.roles:
            if cancel.is_set():
                emit(on_progress, progress(role, pos, 'cancelled', 0.0, '측정을 취소했습니다'))
                return results
            emit(on_progress, progress(role, pos, 'sweep', 0.0,
                                       f'{role} 스윕 재생 중 (위치 {pos})'))
            if not self._sleep_sweep(role, pos, on_progress, cancel):
                emit(on_progress, progress(role, pos, 'cancelled', 0.0, '측정을 취소했습니다'))
                return results
            emit(on_progress, progress(role, pos, 'process', 90.0, f'{role} 임펄스 응답 계산'))

            ir = self.ir_for(role, pos, plan.sample_rate or self.sample_rate)
            uuid = self._new_uuid(role, pos)
            rate = int(plan.sample_rate or self.sample_rate)
            with self._lock:
                self._by_uuid[uuid] = (ir, rate)
            result = MeasurementResult(
                role=role, pos=int(pos), uuid=uuid, ir=ir, sample_rate=rate,
                name=plan.name_for(role, pos),
                spl_db=round(75.0 + 0.25 * plan.output_for(role), 3),
                t0_s=self._t0_for(role, rate),
            )
            results.append(result)
            emit(on_progress, progress(role, pos, 'done', 100.0, f'{role} 완료'))
        return results

    def _sleep_sweep(self, role: str, pos: int,
                     on_progress: ProgressCallback | None,
                     cancel: threading.Event) -> bool:
        """스윕 시간을 흉내내며 진행률을 알린다. 취소되면 False.

        첫 이벤트(0 %)만 'sweep' 이고 그 뒤 증가분은 'running' 이다 — `RealREW` 가 REW 의
        진행 문자열을 중계하는 구간과 phase 를 맞춘다(2026-09-23, 백로그 2).
        """
        steps = 5
        slice_s = max(0.0, self.delay_s) / steps
        for i in range(1, steps + 1):
            if slice_s and cancel.wait(slice_s):
                return False
            if cancel.is_set():
                return False
            emit(on_progress, progress(role, pos, 'running', 90.0 * i / steps,
                                       f'{role} 스윕 {i}/{steps}'))
        return True

    # ── IRProvider ──
    def get_ir(self, uuid: str) -> tuple[np.ndarray, int]:
        with self._lock:
            if uuid not in self._by_uuid:
                raise KeyError(f'알 수 없는 측정 uuid: {uuid!r}')
            ir, rate = self._by_uuid[uuid]
        return np.asarray(ir, dtype=np.float32).copy(), int(rate)

    # ── IR 합성 ──
    def ir_for(self, role: str, pos: int, sample_rate: int | None = None) -> np.ndarray:
        """role·pos 에 대한 결정적 IR."""
        self._check_sample_position(pos)
        rate = int(sample_rate or self.sample_rate)
        base = self._base_ir(role, rate)
        return _position_variation(base, pos, rate)

    def _check_sample_position(self, pos: int) -> None:
        """샘플 모드는 실측이 있는 위치 1 만 — 그 밖은 변형 가짜를 만들지 않고 막는다(09-24)."""
        if self.use_sample_dolby and int(pos) != 1:
            raise RewError(SAMPLE_SINGLE_POSITION)

    def _base_ir(self, role: str, sample_rate: int) -> np.ndarray:
        key = f'{role}@{sample_rate}'
        if key in self._ir_cache:
            return self._ir_cache[key]
        base_role, band = split_role(role)
        ir = None
        if self.use_sample_dolby and band is None:
            ir = self._sample_ir(base_role, sample_rate)
        if ir is None and self.use_test_synth:
            ir = self._synth_ir(base_role, sample_rate)
        if ir is None:
            ir = _fallback_ir(base_role, sample_rate, self.duration_s)
        ir = np.asarray(ir, dtype=np.float32)
        self._ir_cache[key] = ir
        return ir

    def _sample_ir(self, role: str, sample_rate: int) -> np.ndarray | None:
        """실측 npz(`sample_dolby_714.npz`)에서 role 의 IR. 없거나 레이트가 다르면 None(합성으로)."""
        table = _load_sample_dolby()
        entry = table.get(role)
        if entry is None:
            return None
        ir, rate, _t0 = entry
        if int(rate) != int(sample_rate):
            return None
        return np.asarray(ir, dtype=np.float32).copy()

    def _t0_for(self, role: str, sample_rate: int) -> float:
        """role 의 시간 원점. 샘플 실측이면 그 채널의 실제 t0(도착 시간 차이 보존),
        아니면 인스턴스 공통값(`self.t0_s`)."""
        base_role, band = split_role(role)
        if self.use_sample_dolby and band is None:
            table = _load_sample_dolby()
            entry = table.get(base_role)
            if entry is not None and int(entry[1]) == int(sample_rate):
                return float(entry[2])
        return self.t0_s

    def _synth_ir(self, role: str, sample_rate: int) -> np.ndarray | None:
        """`tests/synth.py` 의 `role_measurement_set(main_count, sub_count)` 사용."""
        synth = _load_synth()
        if synth is None or not hasattr(synth, 'role_measurement_set'):
            return None
        main_order = tuple(getattr(synth, 'MAIN_ROLE_ORDER', _MAIN_ROLE_ORDER))
        sub_order = tuple(getattr(synth, 'SUB_ROLE_ORDER', _SUB_ROLE_ORDER))
        if role in main_order:
            main_count, sub_count = main_order.index(role) + 1, 0
        elif role in sub_order:
            main_count, sub_count = 0, sub_order.index(role) + 1
        else:
            return None
        try:
            table = synth.role_measurement_set(main_count, sub_count,
                                               sample_rate, self.duration_s)
        except Exception:  # noqa: BLE001 - 합성 실패 시 자체 합성으로
            return None
        entry = table.get(role)
        if entry is None:
            return None
        return np.asarray(entry[0], dtype=np.float32)

    def _new_uuid(self, role: str, pos: int) -> str:
        with self._lock:
            self._counter += 1
            n = self._counter
        return f'fake-{role.replace(":", "_")}-P{pos}-{n:04d}'

    # ── 편의 ──
    def measurements(self) -> list[dict[str, Any]]:
        """REW 의 GET /measurements 흉내(테스트·위저드 표시용)."""
        with self._lock:
            return [{'uuid': u, 'sampleRate': r, 'samples': int(ir.size)}
                    for u, (ir, r) in self._by_uuid.items()]

    def close(self) -> None:
        self.stop()
        self.stop_generator()

    def __enter__(self) -> 'FakeREW':
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()


def fake_plan(roles: list[str], positions: list[int], **kwargs: Any) -> MeasurementPlan:
    """테스트·데모용 최소 plan(출력 채널은 1부터 순서대로)."""
    outputs = {role: i + 1 for i, role in enumerate(roles)}
    params: dict[str, Any] = {
        'output_device': 'Demo ASIO 16ch',
        'input_device': 'Demo USB Mic',
        'input_channel': 1,
    }
    params.update(kwargs)
    return MeasurementPlan(roles=list(roles), outputs=outputs,
                           positions=list(positions), **params)


__all__ = ['FakeREW', 'fake_plan']
