# -*- coding: utf-8 -*-
"""SECS — 패키지 진입점. 제품 버전은 `secs/version.py`.

실행: python -m secs [프로젝트.secs] [--fake-rew]   (측정 위저드, Fake = REW 없이 샘플. 또는 루트의 run_secs.py)
확인: python -m secs --check      버전: python -m secs --version
언어: 처음 실행 시 한국어. 화면 오른쪽 위에서 한국어/English 전환 (자동 저장).
필요 패키지: numpy scipy soundfile PyQt6 pyqtgraph requests
준비된 환경: .venv-secs/Scripts/python.exe run_secs.py
(옛 1.41 화면 `--legacy` 와 `SECS 1.41.exe` 는 2026-10-10 슬림화에서 지웠다 — 그 인자는 무시하고 위저드를 연다.)
"""
from __future__ import annotations

import importlib
import importlib.util
import sys

from secs import SOURCE_MODULES
from secs.version import __version__, version_text

_REQUIRED = ('numpy', 'scipy', 'soundfile', 'PyQt6', 'pyqtgraph', 'requests')


def _check_dependencies() -> None:
    missing = [name for name in _REQUIRED if importlib.util.find_spec(name) is None]
    if missing:
        raise RuntimeError('Missing packages. Run: ' + sys.executable
                           + ' -m pip install ' + ' '.join(missing))


def _force_utf8_stdout() -> None:
    """cp949 콘솔에서 --help 의 한국어·em dash 가 UnicodeEncodeError 를 내지 않게 한다.

    reconfigure 가 없는 스트림(창 모드 exe 의 None, 리다이렉트된 래퍼)이면 그냥 넘어간다.
    """
    reconfigure = getattr(sys.stdout, 'reconfigure', None)
    if reconfigure is None:
        return
    try:
        reconfigure(encoding='utf-8', errors='replace')
    except (OSError, ValueError):
        pass


def main() -> int:
    _force_utf8_stdout()
    if sys.version_info < (3, 11):
        raise RuntimeError('SECS requires Python 3.11 or newer.')
    argv = sys.argv[1:]
    if '--help' in argv or '-h' in argv:
        print(__doc__)
        return 0
    if '--version' in argv:
        print(f'{version_text()} ({__version__})')
        return 0
    _check_dependencies()
    if '--check' in argv:
        import json
        for name in SOURCE_MODULES:
            importlib.import_module(f'secs.{name}')
        # 보정 설계의 워커 프로세스(spawn — XO 후보 채점·MIMO)가 얼린 exe 에서도 뜨는지(run_secs.py freeze_support).
        # 3.0.0 부터 옛 7단계 엔진의 공유 풀(`lowband.process_pool`)로 보던 것을 10-10 슬림화에서 직접 띄우는 것으로.
        pool = 'none'
        try:
            import multiprocessing
            from concurrent.futures import ProcessPoolExecutor
            with ProcessPoolExecutor(max_workers=2, mp_context=multiprocessing.get_context('spawn')) as executor:
                pool = 'ok' if executor.submit(pow, 2, 10).result(timeout=60) == 1024 else 'wrong'
        except Exception as error:  # noqa: BLE001 - 점검 결과로 보고만 한다
            pool = f'error: {error}'
        print(json.dumps({'version': __version__,          # 제품(위저드) 버전
                          'python': sys.version.split()[0],
                          'source_modules': len(SOURCE_MODULES),
                          'process_pool': pool,
                          'status': 'ok'}, ensure_ascii=False))
        return 0
    # 측정 위저드. `--fake-rew` 면 REW 없이 FakeREW 로 돈다. `--wizard` 는 예전 런처와의 호환용으로 받기만 한다.
    # 무거운 모듈을 읽기 전에 로딩 창부터(exe 는 창이 뜨기까지 2~3 초 — 09-22 사용자 요청).
    from secs.wizard.splash import show_startup_splash
    splash = show_startup_splash()
    if splash is not None:
        splash.message('계산 모듈을 읽는 중…')
        # 가장 오래 걸리는 계산 모듈은 뒤 스레드에서 — 그동안 로딩 창의 방울이 움직인다(09-24).
        splash.preload(('numpy', 'scipy.signal', 'scipy.optimize', 'scipy.fft', 'scipy.interpolate'))
    from secs.wizard import run_wizard
    return run_wizard(argv, splash=splash)


if __name__ == '__main__':
    raise SystemExit(main())
