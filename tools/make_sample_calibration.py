# -*- coding: utf-8 -*-
"""튜토리얼 데모 방의 보정 결과를 한 번 계산해 저장한다 → `src/secs/rewapi/sample_calibration.pkl.gz`.

튜토리얼은 보정 계산을 하지 않고 이 결과를 보여 주기만 한다(2026-10-11 사용자: "연산하는 척만 하라니까" — 데모 방이
실측 서브 3개가 되면서 뒤에서 하던 미리 계산이 길어져 [계산] 이 멈춘 것처럼 보였다). 튜토리얼과 같은 길로 계산한다:
샘플 모드 창(숨김) → 튜토리얼 켬 → 5단계 데모 데이터 → 보정 설계 화면의 설정 · IR 공급 함수로 `calibration.compute`.

엔진 · 샘플 데이터 · 기본 설정이 바뀌면 다시 돌린다(약 1~3 분, 창은 화면에 뜨지 않음):
    .venv-secs\\Scripts\\python.exe tools\\make_sample_calibration.py
`tests/test_demo_run.py` 가 파일이 읽히고 서브 3 결과가 들어 있는지 본다.
"""
from __future__ import annotations

import copy
import gzip
import os
import pickle
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'src'))


def _small(value):
    """dict · list 안의 큰 배열을 32비트로(complex128 → complex64, float64 → float32). 그대로 두면 파일이 72 MB(gzip)였다."""
    import numpy as np

    if isinstance(value, dict):
        return {k: _small(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_small(v) for v in value]
    if isinstance(value, np.ndarray) and value.size > 1000:
        if value.dtype == np.complex128:
            return value.astype(np.complex64)
        if value.dtype == np.float64:
            return value.astype(np.float32)
    return value


def _slim(run) -> None:
    """화면이 쓰지 않는 것은 빼고(결과의 채널별 전 · 후 · 합 스펙트럼 — 곡선은 `run.curves` 로 이미 뽑혀 있다),
    XO 미리 보기 · 서브 정렬 탭이 쓰는 엔진 배열(chain · H · H0 등)은 32비트로."""
    res = run.result
    for name in ('before', 'after', 'combined'):
        if isinstance(getattr(res, name, None), dict):
            setattr(res, name, {})
    eng = run.engine
    for name in ('chain', 'H', 'H0', 'design_db', 'irs'):
        if hasattr(eng, name):
            setattr(eng, name, _small(getattr(eng, name)))


def _targets(quest, win, widget) -> bool:
    try:
        target = quest.target(win)
    except Exception:  # noqa: BLE001
        return False
    return target is widget or (isinstance(target, (list, tuple)) and widget in target)


def _slim_subs(run) -> None:
    """서브만 결과는 서브 정렬 탭(`calibration.sub_combine`)만 쓴다 — 엔진 배열을 서브 채널만 남기고 32비트로."""
    _slim(run)
    eng = run.engine
    subs = set(eng.subs)
    for name in ('H', 'H0', 'irs'):
        table = getattr(eng, name, None)
        if isinstance(table, dict):
            setattr(eng, name, {p: {r: v for r, v in roles.items() if r in subs} if isinstance(roles, dict) else roles
                                for p, roles in table.items()})
    for name in ('chain', 'design_db'):
        if isinstance(getattr(eng, name, None), dict):
            setattr(eng, name, {r: v for r, v in getattr(eng, name).items() if r in subs})


def main() -> int:
    os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
    from PyQt6 import QtCore, QtWidgets

    app = QtWidgets.QApplication.instance() or QtWidgets.QApplication([])
    from secs import calibration as cal
    from secs.wizard import build_backend, new_project
    from secs.wizard.window import WizardWindow

    rew, devices, levels, measurer = build_backend(fake=True, use_sample_dolby=True)
    win = WizardWindow(new_project(True), rew=rew, devices=devices, levels=levels, measurer=measurer)
    win.ask_before_close = False
    win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    win.show()
    try:
        win.set_trial(True)
        app.processEvents()
        win.tutorial.stop()
        win.go(5)
        if win.measure_page.load_sample() < 1:
            print('데모 데이터를 못 넣었다')
            return 1
        app.processEvents()
        win._set_calibration_context()
        page = win.calibration_page
        started = time.perf_counter()
        run = cal.compute(copy.deepcopy(page.project), page.irs_provider(), copy.deepcopy(page.settings))
        print(f'계산 {time.perf_counter() - started:.1f} s · 서브 {list(run.engine.subs)}')
        # [서브만 계산] 퀘스트가 보여 줄 결과 — 인프라 서브 퀘스트를 대신 해 준 뒤(SW3 = 인프라)의 서브 정렬만
        from secs.wizard import tutorial_quests as tq
        infra = next(q for q in tq.QUESTS if q.auto is not None and _targets(q, win, page.infra_combo))
        infra.auto(win)
        started = time.perf_counter()
        subs_run = cal.compute(copy.deepcopy(page.project), page.irs_provider(), copy.deepcopy(page.settings),
                               subs_only=True)
        print(f'서브만 {time.perf_counter() - started:.1f} s · 인프라 {page.infra_state()}')
    finally:
        win.close()
    _slim(run)
    _slim_subs(subs_run)
    out = cal.demo_run_path()
    with gzip.open(out, 'wb', compresslevel=9) as fh:
        pickle.dump({'version': cal.RUN_CACHE_VERSION, 'run': run, 'subs_run': subs_run}, fh,
                    protocol=pickle.HIGHEST_PROTOCOL)
    print(f'{out} · {out.stat().st_size / 1e6:.2f} MB')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
