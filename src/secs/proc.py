"""외부 프로그램(REW 등)을 띄울 때 SECS exe 의 흔적을 물려주지 않는다.

누가 부름: `secs.rewapi.rew.RealREW.launch`(위저드의 [REW API 모드로 실행]).
입력: 실행할 명령 목록. 출력: 띄운 `subprocess.Popen`.

왜(09-23): PyInstaller exe 는 자기 `_internal` 폴더를 DLL 검색 경로(SetDllDirectory)와 PATH 에 넣는다.
SECS 에서 [REW API 모드로 실행] 으로 띄운 REW 가 이걸 물려받아 우리 `VCRUNTIME140.dll` 을 쥐고 있었고,
그 뒤로는 REW 를 끄기 전까지 새 빌드가 `dist\\SECS` 를 지우지 못했다(액세스 거부). 개발 실행
(`python run_secs.py`)에서는 할 일이 없다.
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

_IS_WINDOWS = sys.platform.startswith('win')
#: exe 가 자기 것으로 넣는 환경 변수 — 외부 프로그램에는 필요 없다.
_OWN_VARS = ('PYTHONHOME', 'PYTHONPATH', 'QT_PLUGIN_PATH', 'QML2_IMPORT_PATH', 'TCL_LIBRARY', 'TK_LIBRARY')


def _bundle_dir() -> str | None:
    return getattr(sys, '_MEIPASS', None) if getattr(sys, 'frozen', False) else None


def clean_env(bundle: str | None = None) -> dict[str, str]:
    """PATH 에서 exe 폴더를 뺀 환경. 번들이 아니면 지금 환경 그대로."""
    env = dict(os.environ)
    bundle = bundle if bundle is not None else _bundle_dir()
    if not bundle:
        return env
    root = os.path.normcase(os.path.abspath(bundle))
    parts = [p for p in env.get('PATH', '').split(os.pathsep)
             if p and not os.path.normcase(os.path.abspath(p)).startswith(root)]
    env['PATH'] = os.pathsep.join(parts)
    for key in _OWN_VARS:
        env.pop(key, None)
    return env


def launch_external(args: list[str], **kwargs) -> subprocess.Popen:
    """외부 프로그램을 깨끗한 환경·자기 폴더를 작업 폴더로 띄운다(Windows 는 콘솔 창 없이)."""
    bundle = _bundle_dir()
    kwargs.setdefault('env', clean_env(bundle))
    exe = Path(args[0])
    if exe.is_absolute():
        kwargs.setdefault('cwd', str(exe.parent))      # 작업 폴더가 dist\SECS 면 그 폴더도 잠긴다
    if _IS_WINDOWS:
        kwargs.setdefault('creationflags', subprocess.CREATE_NO_WINDOW)
    if not (_IS_WINDOWS and bundle):
        return subprocess.Popen(args, **kwargs)
    import ctypes

    kernel32 = ctypes.windll.kernel32
    kernel32.SetDllDirectoryW(None)                     # 자식에게 _internal 을 물려주지 않게 잠깐 풀었다가
    try:
        return subprocess.Popen(args, **kwargs)
    finally:
        kernel32.SetDllDirectoryW(bundle)               # 우리 프로세스의 DLL 검색은 원래대로


__all__ = ['clean_env', 'launch_external']
