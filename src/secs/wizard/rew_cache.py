# -*- coding: utf-8 -*-
"""REW 상태 캐시 — 화면 스레드는 REW 에 묻지 않고 이 캐시만 읽는다(2026-09-23).

사용자 보고: "각 단계별로 이동할 때 딜레이가 굉장히 심하다"(실제 exe + 실제 REW). FakeREW 로 재면
`go(n)` 이 4~50 ms 였는데, 실제 REW 에서는 단계 진입·새로 그리기·다음 버튼 판정마다 화면 스레드가
HTTP 를 불렀다 — 장치 단계 한 번 들어갈 때 10~14회(`/version` ×6 …), 요청마다 REW 응답 시간만큼
화면이 멈췄다(`tools/slow_rew_profile.py`, `보고서/05 화면 QA/2026-09-23 반응 속도 측정.md` "실제 REW 지연 흉내").

그래서 이렇게 나눈다.
- **값 읽기**: `RewCache` 가 REW 상태(켜짐·버전·Pro)·장치/채널 목록·캘리브레이션·REW 프로세스 여부를
  `RewSnapshot` 하나로 들고, 화면은 같은 이름의 메서드(`is_running()`·`output_devices()` …)로 그 값을
  읽는다 — 단계 화면 코드는 예전 REW 객체 자리에 캐시를 받을 뿐 바뀌지 않는다.
- **새로 읽기**: 워커 스레드에서만(`request_refresh()`) — 창의 백그라운드 점검(`WizardWindow.start_rew_watch`),
  유효 시간(`TTL_S`)이 지난 채 단계에 들어갈 때, 사용자가 [다시 확인]·[설정 읽기]를 누를 때.
  끝나면 `updated` 신호(화면 스레드)로 알린다.
- **보내기**(레벨 모니터 시작·정지, 제너레이터): `SerialWorker` 한 줄 대기열 — 순서는 지키고 화면은 안 막는다.

원격이 아닌 백엔드(FakeREW·테스트 흉내 — `probe()` 가 없다)는 프로세스 안이라 즉시 답하므로
캐시를 거치지 않고 그대로 읽는다(`live`). 이 구분은 창의 백그라운드 점검과 같은 기준이다.
"""
from __future__ import annotations

import queue
import threading
import time
from dataclasses import dataclass, field
from typing import Any, Callable

from PyQt6 import QtCore


def is_remote(backend: Any) -> bool:
    """네트워크 너머의 REW 인가(`RealREW` 처럼 `probe()` 가 있다). 아니면 프로세스 안의 흉내다."""
    return backend is not None and hasattr(backend, 'probe')


@dataclass(frozen=True)
class RewSnapshot:
    """한 번 읽은 REW 의 모습. `running=None` 은 아직 한 번도 못 물어봤다는 뜻."""

    running: bool | None = None
    version: str | None = None
    pro: bool = False
    outputs: tuple = ()
    inputs: tuple = ()
    output_names: tuple = ()
    input_names: tuple = ()
    #: None = 알 수 없음, '' = 파일 없음(확정), 그 밖 = 경로.
    calibration: str | None = None
    #: REW 프로세스가 떠 있는가(API 가 답하지 않을 때만 의미가 있다 — tasklist 는 느리다).
    process_running: bool = False
    exe_path: str | None = None
    #: [설정 읽기] 때만 읽는다. None = 안 읽었거나 모름.
    level_dbfs: float | None = None
    stamp: float = field(default=0.0, compare=False)


class RewCache(QtCore.QObject):
    """REW 상태·장치 목록 캐시 겸 화면용 REW 대역. 화면 스레드에서 만든다."""

    #: 새로 읽은 값이 이전과 다를 때(또는 요청자가 기다릴 때) — 화면 스레드에서 받는다.
    updated = QtCore.pyqtSignal(object)
    _deliver = QtCore.pyqtSignal(object, object)       # (스냅숏, 콜백) — 워커 → 화면 스레드

    #: 이보다 오래된 값이면 단계에 들어갈 때 뒤에서 새로 읽는다(초).
    TTL_S = 15.0

    def __init__(self, rew: Any, devices: Any = None, parent=None) -> None:
        super().__init__(parent)
        self.backend = rew
        self.devices = devices if devices is not None else rew
        self.live = not is_remote(rew)
        self._snap = RewSnapshot()
        self._lock = threading.Lock()
        self._busy = False
        self._waiters: list[Callable[[RewSnapshot], None]] = []
        self._want_level = False
        self._deliver.connect(self._on_deliver)

    # ──────────────────────────── 화면이 읽는 값(REW 대역) ────────────────────────────
    def snapshot(self) -> RewSnapshot:
        return self.read_live() if self.live else self._snap

    def age_s(self) -> float:
        return time.monotonic() - self._snap.stamp if self._snap.stamp else float('inf')

    def is_running(self) -> bool:
        if self.live:
            return bool(_call(self.backend, 'is_running', False))
        return self._snap.running is True

    def version(self) -> str | None:
        return _call(self.backend, 'version', None) if self.live else self._snap.version

    def is_pro(self) -> bool:
        return bool(_call(self.backend, 'is_pro', False)) if self.live else self._snap.pro

    def output_devices(self) -> list:
        return list(_call(self.devices, 'output_devices', []) or []) if self.live else list(self._snap.outputs)

    def input_devices(self) -> list:
        return list(_call(self.devices, 'input_devices', []) or []) if self.live else list(self._snap.inputs)

    def output_channel_names(self) -> list[str]:
        if self.live:
            return [str(n) for n in (_call(self.devices, 'output_channel_names', []) or [])]
        return list(self._snap.output_names)

    def input_channel_names(self) -> list[str]:
        if self.live:
            return [str(n) for n in (_call(self.devices, 'input_channel_names', []) or [])]
        return list(self._snap.input_names)

    def input_calibration_file(self) -> str | None:
        return _call(self.backend, 'input_calibration_file', None) if self.live else self._snap.calibration

    def measure_level_dbfs(self) -> float | None:
        return _call(self.backend, 'measure_level_dbfs', None) if self.live else self._snap.level_dbfs

    def process_running(self) -> bool:
        """REW 프로세스가 떠 있는가. 원격이면 마지막으로 읽은 값(tasklist 는 워커가 돌린다)."""
        if self.live:
            from secs.wizard import step1_devices as step_devices
            return bool(step_devices.rew_process_running())
        return self._snap.process_running

    def can_launch(self) -> bool:
        return hasattr(self.backend, 'launch')

    def launch(self, wait_s: float = 20.0) -> bool:
        """REW 를 띄운다 — 오래 걸리니 **워커 스레드에서만** 부른다(장치 단계가 그렇게 한다)."""
        ok = bool(self.backend.launch(wait_s=wait_s))
        if not self.live:
            self._store(self.read_live())
        return ok

    # ──────────────────────────── 새로 읽기 ────────────────────────────
    def read_live(self, level: bool = False) -> RewSnapshot:
        """REW 에 실제로 물어 스냅숏을 만든다. 원격이면 **워커 스레드에서만** 부른다."""
        from secs.wizard import step1_devices as step_devices   # 테스트가 이 모듈의 함수를 바꿔 끼운다

        backend, devices = self.backend, self.devices
        if is_remote(backend):
            try:
                running = bool(backend.probe())
            except Exception:  # noqa: BLE001 - 점검 실패는 '꺼짐'
                running = False
        else:
            running = bool(_call(backend, 'is_running', False))
        exe = step_devices.find_rew_exe()
        if not running:
            return RewSnapshot(running=False, process_running=bool(step_devices.rew_process_running()),
                               exe_path=exe, stamp=time.monotonic())
        return RewSnapshot(
            running=True,
            version=_call(backend, 'version', None),
            pro=bool(_call(backend, 'is_pro', False)),
            outputs=tuple(_call(devices, 'output_devices', []) or ()),
            inputs=tuple(_call(devices, 'input_devices', []) or ()),
            output_names=tuple(str(n) for n in (_call(devices, 'output_channel_names', []) or ())),
            input_names=tuple(str(n) for n in (_call(devices, 'input_channel_names', []) or ())),
            calibration=_call(backend, 'input_calibration_file', None),
            process_running=True,
            exe_path=exe,
            level_dbfs=_call(backend, 'measure_level_dbfs', None) if level else self._snap.level_dbfs,
            stamp=time.monotonic())

    def _store(self, snap: RewSnapshot) -> bool:
        """새 값을 넣는다. 이전과 달랐으면 True."""
        with self._lock:
            changed = snap != self._snap
            self._snap = snap
        return changed

    def watch_tick(self) -> bool:
        """창의 백그라운드 점검 한 번(워커 스레드). 평소엔 `/version` 하나만 묻고, 켜짐 상태가
        바뀌었거나 값이 `TTL_S` 보다 오래됐거나 장치 목록이 비어 있으면(REW 가 막 떠서 오디오가
        아직 준비 전) 전부 새로 읽는다. 새로 읽었으면 True."""
        if self.live:
            return False
        try:
            running = bool(self.backend.probe())
        except Exception:  # noqa: BLE001
            running = False
        snap = self._snap
        stale = running and (self.age_s() >= self.TTL_S or not snap.outputs or not snap.inputs)
        if running == snap.running and not stale:
            return False
        if self._busy:
            return False
        self.refresh_blocking()
        return True

    def refresh_blocking(self, level: bool = False) -> bool:
        """워커 스레드용: 새로 읽어 넣고, 바뀌었으면 `updated` 를 낸다. 바뀌었는지 돌려준다."""
        snap = self.read_live(level=level)
        changed = self._store(snap)
        if changed:
            self._deliver.emit(snap, None)
        return changed

    def request_refresh(self, then: Callable[[RewSnapshot], None] | None = None,
                        level: bool = False) -> None:
        """뒤에서 새로 읽는다. 끝나면 `then(스냅숏)` 을 화면 스레드에서 부른다(바뀌었으면 `updated` 도).

        원격이 아니면(프로세스 안 흉내) 바로 읽고 바로 부른다 — 기다릴 것이 없다.
        이미 읽는 중이면 새 스레드를 만들지 않고 그 결과를 같이 받는다.
        """
        if self.live:
            snap = self.read_live(level=level)
            if self._store(snap):
                self.updated.emit(snap)
            if then is not None:
                then(snap)
            return
        with self._lock:
            if then is not None:
                self._waiters.append(then)
            self._want_level = self._want_level or level
            if self._busy:
                return
            self._busy = True

        def work() -> None:
            while True:
                with self._lock:
                    want_level, self._want_level = self._want_level, False
                snap = self.read_live(level=want_level)
                changed = self._store(snap)
                with self._lock:
                    waiters, self._waiters = self._waiters, []
                    again = self._want_level
                    if not again:
                        self._busy = False
                self._deliver.emit(snap, (changed, waiters))
                if not again:
                    return

        threading.Thread(target=work, name='secs-rew-refresh', daemon=True).start()

    def refresh_if_stale(self, ttl_s: float | None = None) -> bool:
        """값이 유효 시간보다 오래됐으면 뒤에서 새로 읽기 시작한다. 시작했으면 True."""
        # 한 번도 안 읽었으면 첫 읽기는 창의 백그라운드 점검 몫이다(창을 만드는 동안 REW 에 가지 않는다 — 09-20).
        if self.live or self._busy or not self._snap.stamp:
            return False
        if self.age_s() < float(ttl_s if ttl_s is not None else self.TTL_S):
            return False
        self.request_refresh()
        return True

    def busy(self) -> bool:
        return self._busy

    @QtCore.pyqtSlot(object, object)
    def _on_deliver(self, snap: RewSnapshot, extra) -> None:
        if extra is None:
            self.updated.emit(snap)
            return
        changed, waiters = extra
        if changed:
            self.updated.emit(snap)
        for callback in waiters:
            try:
                callback(snap)
            except Exception:  # noqa: BLE001 - 한 요청자의 오류가 다른 요청자를 막지 않게
                pass


def _call(target: Any, name: str, default):
    """`target.name()` — 없거나 실패하면 `default`. 옛 흉내(메서드 없음)도 받아 준다."""
    method = getattr(target, name, None) if target is not None else None
    if method is None:
        return default
    try:
        return method()
    except Exception:  # noqa: BLE001 - REW 가 답하지 않으면 모른다고 본다
        return default


class SerialWorker:
    """REW 로 보내는 명령(레벨 모니터·제너레이터)을 한 줄로 세워 워커 스레드에서 차례로 보낸다.

    `inline=True`(프로세스 안 흉내)면 그 자리에서 바로 부른다. 실패는 삼키고 `on_error` 로 알린다.
    """

    def __init__(self, inline: bool = False, name: str = 'secs-rew-send') -> None:
        self.inline = bool(inline)
        self._name = name
        self._queue: queue.Queue = queue.Queue()
        self._thread: threading.Thread | None = None

    def submit(self, func: Callable, *args, on_error: Callable[[Exception], None] | None = None) -> None:
        if self.inline:
            try:
                func(*args)
            except Exception as error:  # noqa: BLE001
                if on_error is not None:
                    on_error(error)
            return
        self._queue.put((func, args, on_error))
        if self._thread is None or not self._thread.is_alive():
            self._thread = threading.Thread(target=self._run, name=self._name, daemon=True)
            self._thread.start()

    def _run(self) -> None:
        while True:
            try:
                func, args, on_error = self._queue.get(timeout=2.0)
            except queue.Empty:
                return
            try:
                func(*args)
            except Exception as error:  # noqa: BLE001 - REW 명령 실패로 워커가 죽지 않게
                if on_error is not None:
                    try:
                        on_error(error)
                    except Exception:  # noqa: BLE001
                        pass

    def drain(self, timeout_s: float = 3.0) -> bool:
        """대기열이 빌 때까지 기다린다(창을 닫을 때). 비었으면 True."""
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            if self._queue.empty() and (self._thread is None or not self._thread.is_alive()
                                        or self._queue.unfinished_tasks == 0):
                return True
            time.sleep(0.01)
        return self._queue.empty()


__all__ = ['RewCache', 'RewSnapshot', 'SerialWorker', 'is_remote']
