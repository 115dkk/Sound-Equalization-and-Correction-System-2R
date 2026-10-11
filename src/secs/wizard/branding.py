# -*- coding: utf-8 -*-
"""로고 — 창·작업 표시줄 아이콘과 사이드바 마크.

파일은 `assets/` 에 있다: `logo_b8.svg`(2026-09-24 사용자 확정)와 `logo_b8_small.svg`(16~32 px 용 단순판), 거기서 뽑은
`app.ico`(16~256 px 다중 크기)와 `app_256.png`. 그림: 위아래의 들쭉날쭉한 분홍 띠가 가운데 청록 막대로
모이고(닿는 곳은 더 붉다), 왼쪽 안쪽은 걸쭉한 흰 액체가 고인 모양이다. 오른쪽은 둥근 장식 방울을 정리하고
막대를 타고 흐르는 줄기, 매달린 방울, 떨어지는 방울이 한 낙하 축으로 이어진다.
사용자 손그림 2장(`보고서/로고 손그림 1·2 2026-09-20.png`)을 GPT-6 Astra 워커(R3·R4)가 SVG 로 옮겼다.
앞선 안(a·b·b2~b5·c)과 뒤의 7차안(b7, 액체 양이 적어 채택 안 함)도 같은 폴더에 있다. 다시 뽑기:
`tools\\render_logo.py --ico …\\logo_b8.svg --small …\\logo_b8_small.svg`.
런타임에는 SVG 를 읽지 않는다(QtSvg 를 exe 에 넣지 않으려고) — ico·png 만 쓴다.

부르는 곳: `window.WizardWindow`(창 아이콘·사이드바). exe 파일 자체의 아이콘은 `SECS.spec` 의 `icon=`.
파일이 없어도(개발 중 실수) 화면은 떠야 하므로 빈 아이콘·None 을 돌려준다.
"""
from __future__ import annotations

from pathlib import Path

from PyQt6 import QtCore, QtGui

ASSETS = Path(__file__).resolve().parent / 'assets'


def app_icon() -> QtGui.QIcon:
    """창·작업 표시줄 아이콘(다중 크기 ico)."""
    path = ASSETS / 'app.ico'
    return QtGui.QIcon(str(path)) if path.is_file() else QtGui.QIcon()


def mark_pixmap(size: int, device_pixel_ratio: float = 1.0) -> QtGui.QPixmap | None:
    """사이드바에 넣을 정사각 마크. 고해상도 화면에서도 또렷하게 배율만큼 크게 줄여 담는다."""
    path = ASSETS / 'app_256.png'
    if not path.is_file():
        return None
    ratio = max(1.0, float(device_pixel_ratio))
    pixels = int(round(size * ratio))
    pixmap = QtGui.QPixmap(str(path)).scaled(
        pixels, pixels, QtCore.Qt.AspectRatioMode.KeepAspectRatio,
        QtCore.Qt.TransformationMode.SmoothTransformation)
    pixmap.setDevicePixelRatio(ratio)
    return pixmap


__all__ = ['ASSETS', 'app_icon', 'mark_pixmap']
