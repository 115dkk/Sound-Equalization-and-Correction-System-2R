# -*- coding: utf-8 -*-
"""SECS 실행 진입점 (소스/PyInstaller 공용).

실행: .venv-secs\\Scripts\\python.exe run_secs.py [프로젝트.secs] [--fake-rew]
확인: .venv-secs\\Scripts\\python.exe run_secs.py --check

src/ 를 sys.path 에 넣어 `secs` 패키지를 임포트한 뒤 `secs.__main__.main()` 을 부른다.
얼린(frozen) exe 에서는 secs 가 번들에 들어 있으므로 경로 추가는 무시된다.
"""
from __future__ import annotations

import os
import sys

_SRC = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'src')
if os.path.isdir(_SRC) and _SRC not in sys.path:
    sys.path.insert(0, _SRC)

from secs.__main__ import main

if __name__ == '__main__':
    # 7단계 저역 설계가 워커 프로세스(spawn)를 쓴다 — 얼린 exe 에서 자식이 창을 또 띄우지 않게(2026-09-24).
    import multiprocessing

    multiprocessing.freeze_support()
    raise SystemExit(main())
