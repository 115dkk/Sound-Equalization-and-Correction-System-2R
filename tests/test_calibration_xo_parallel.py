"""XO 추천 병렬(2026-10-09 사용자: "너무 오래 걸려, 멀티코어 못 해?") — 후보마다 워커 프로세스(spawn).

'동작 → 변화' 로 본다:
- 같은 입력이면 병렬 결과(표·추천·후보 순서)가 직렬과 완전히 같고, 진행 콜백은 후보마다 불린다.
- [계산 취소]·창 닫기가 부르는 `abort_workers()` 가 XO 워커 풀도 바로 끈다(꺼도 계속 연산하던 문제).
- 남은 시간 문구.
"""
from __future__ import annotations

import threading
import time

from test_calibration_step import _project, synth  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def test_parallel_recommendation_equals_serial(synth):  # noqa: F811
    from secs import calibration as cal

    case, irs = synth
    project = _project(case, 2)
    settings = cal.CalibrationSettings(mode='zero')
    serial = cal.recommend_xo(project, irs, settings, candidates=(60.0, 120.0), workers=1)
    seen = []
    parallel = cal.recommend_xo(project, irs, settings, candidates=(60.0, 120.0), workers=2,
                                progress=lambda k, n: seen.append((k, n)))
    assert seen == [(1, 2), (2, 2)]
    assert parallel.candidates == serial.candidates
    assert parallel.table == serial.table                      # 숫자까지 비트 단위로 같다
    assert list(parallel.table) == list(serial.table)
    assert all(list(parallel.table[g]) == list(serial.table[g]) for g in serial.table)
    assert parallel.best == serial.best


def test_abort_workers_stops_the_xo_pool(synth):  # noqa: F811
    from secs import calibration as cal
    from secs import mimo_iir

    case, irs = synth
    project = _project(case, 2)
    outcome: dict = {}

    def run():
        try:
            outcome['value'] = cal.recommend_xo(project, irs, cal.CalibrationSettings(mode='zero'),
                                                candidates=(60.0, 80.0, 100.0, 120.0), workers=2)
        except Exception as error:  # noqa: BLE001 - 꺼진 풀은 BrokenProcessPool
            outcome['error'] = error

    thread = threading.Thread(target=run, daemon=True)
    thread.start()
    deadline = time.monotonic() + 60
    procs = []
    while time.monotonic() < deadline:
        pools = list(mimo_iir._ACTIVE_POOLS)
        procs = [p for pool in pools for p in (getattr(pool, '_processes', None) or {}).values()]
        if procs:
            break
        time.sleep(0.05)
    assert procs, 'XO 워커 풀이 _ACTIVE_POOLS 에 들어가야 [계산 취소]가 끈다'
    assert mimo_iir.abort_workers() >= 1
    thread.join(30)
    assert not thread.is_alive(), '워커를 끈 뒤 추천 스레드가 바로 끝나야 한다'
    assert 'error' in outcome and 'value' not in outcome
    for p in procs:
        p.join(10)
        assert not p.is_alive()
    assert not mimo_iir._ACTIVE_POOLS


def test_remaining_text(engine):  # noqa: F811
    from secs.wizard.step_calibration import _remaining_text

    assert _remaining_text(2) == '곧 끝남'
    assert _remaining_text(40) == '약 40초 남음'
    assert _remaining_text(300) == '약 5분 남음'
