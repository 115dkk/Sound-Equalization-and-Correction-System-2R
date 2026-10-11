"""로고 파일이 제자리에 있고 창·사이드바·빌드 spec 에 걸려 있는지 (2026-09-20)."""
from __future__ import annotations

from pathlib import Path

import pytest

from test_wizard_common import (PRESET, engine, fake, make_window,  # noqa: F401
                                project, qapp)
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)

ROOT = Path(__file__).resolve().parents[1]


def test_logo_assets_exist_and_ico_has_every_size(engine):
    from secs.wizard import branding

    for name in ('logo_b6.svg', 'logo_b6_small.svg', 'app.ico', 'app_256.png'):
        assert (branding.ASSETS / name).is_file(), name
    data = (branding.ASSETS / 'app.ico').read_bytes()
    assert data[:4] == b'\x00\x00\x01\x00'                      # ICO 머리
    count = int.from_bytes(data[4:6], 'little')
    sizes = {data[6 + 16 * i] or 256 for i in range(count)}     # 0 은 256 px
    assert {16, 32, 48, 256} <= sizes


def test_window_and_sidebar_show_the_logo(qapp, project, fake, tmp_path):
    from PyQt6 import QtWidgets

    win = make_window(project, fake, tmp_path / "w.secs")
    try:
        assert not win.windowIcon().isNull()
        mark = win.findChild(QtWidgets.QLabel, 'brandMark')
        assert mark is not None and not mark.pixmap().isNull()
        assert mark.width() == 34 and mark.height() == 34
    finally:
        win.close()
        qapp.processEvents()


def test_build_spec_bundles_the_icon():
    spec = (ROOT / 'SECS.spec').read_text(encoding='utf-8')
    assert "ICON = os.path.join(SRC, 'secs', 'wizard', 'assets', 'app.ico')" in spec
    assert "make_exe(f'{APP_NAME} {VERSION_LABEL}', icon=ICON," in spec        # 2.x exe 에 로고
    assert 'version=version_file(' in spec                                   # 작업 관리자에 이름·버전
    assert spec.count("'assets'") >= 4                           # datas 에 ico·png 둘 다
