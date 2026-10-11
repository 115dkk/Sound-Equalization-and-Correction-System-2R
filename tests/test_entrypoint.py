"""실행 진입점(`secs.__main__`). 2026-10-10 슬림화로 옛 1.41 화면(`--legacy`·`SECS 1.41.exe`·.wav/.mdat 인자)은
없어졌다 — 무엇을 넘기든 위저드가 뜨고, `--check` 는 위저드가 쓰는 모듈(`secs.SOURCE_MODULES`)을 임포트해 본다."""
from __future__ import annotations

import importlib
import importlib.util

import pytest

from conftest import load_engine


@pytest.fixture(scope="module")
def entry():
    load_engine("package")
    import secs.__main__ as module
    return module


def test_no_legacy_screen_left(entry):
    assert not hasattr(entry, 'wants_legacy')
    assert 'sounddevice' not in entry._REQUIRED          # 옛 화면의 CamillaDSP 장치 창만 썼다
    for name in ('gui', 'main', 'rew', 'worker', 'dsp_dialog', 'export_dsp', 'macos_setup'):
        assert importlib.util.find_spec(f'secs.{name}') is None, name


def test_check_imports_every_source_module(entry):
    """`--check` 와 같은 임포트 — 목록에 지운 모듈이 남아 있으면 얼린 exe 의 `--check` 가 죽는다."""
    assert 'wizard.window' in entry.SOURCE_MODULES
    for name in entry.SOURCE_MODULES:
        importlib.import_module(f'secs.{name}')
