"""외부 프로그램 실행 환경(09-23): exe 로 띄운 REW 가 우리 _internal DLL 폴더를 물려받아 빌드를 잠갔다."""
from __future__ import annotations

import os
import subprocess

import pytest

from test_wizard_common import engine  # noqa: F401  (src 를 경로에 넣는다)
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


@pytest.fixture
def proc(engine):  # noqa: F811
    from secs import proc as module
    return module


def test_clean_env_drops_the_bundle_from_path(tmp_path, monkeypatch, proc):
    bundle = tmp_path / 'SECS' / '_internal'
    other = tmp_path / 'Other'
    monkeypatch.setenv('PATH', os.pathsep.join([str(bundle), str(bundle / 'PyQt6'), str(other)]))
    monkeypatch.setenv('QT_PLUGIN_PATH', str(bundle / 'plugins'))
    env = proc.clean_env(str(bundle))
    assert env['PATH'].split(os.pathsep) == [str(other)]
    assert 'QT_PLUGIN_PATH' not in env


def test_clean_env_is_untouched_outside_a_bundle(monkeypatch, proc):
    monkeypatch.setenv('PATH', 'a' + os.pathsep + 'b')
    assert proc.clean_env(None)['PATH'] == 'a' + os.pathsep + 'b'


def test_launch_external_runs_in_the_programs_own_folder(tmp_path, monkeypatch, proc):
    seen = {}

    def fake_popen(args, **kwargs):
        seen.update(kwargs, args=args)
        return object()

    monkeypatch.setattr(subprocess, 'Popen', fake_popen)
    exe = tmp_path / 'REW' / 'roomeqwizard.exe'
    proc.launch_external([str(exe), '-api'])
    assert seen['args'] == [str(exe), '-api']
    assert seen['cwd'] == str(exe.parent)
    assert 'env' in seen
