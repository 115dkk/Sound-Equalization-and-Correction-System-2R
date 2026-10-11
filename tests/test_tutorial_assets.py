"""도움말 튜토리얼 그림 — 계약의 키가 한·영 모두 있는지(없으면 MANIFEST 의 missing 에), 크기 예산을 지키는지.

그림은 `tools/make_tutorial_shots.py` 가 만든다. Qt 없이 파일만 본다(빠름). 키 목록의 기준은 계약 문서
`문서/워커 과제/도움말 튜토리얼 계약 2026-09-24.md` 1절 표 — 도구의 KEYS 도 같은 표에서 왔다.
"""
from __future__ import annotations

import ast
import json
import re
import struct
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
TUTORIAL = ROOT / 'src' / 'secs' / 'wizard' / 'assets' / 'tutorial'
CONTRACT = ROOT / '문서' / '워커 과제' / '도움말 튜토리얼 계약 2026-09-24.md'
TOOL = ROOT / 'tools' / 'make_tutorial_shots.py'
LANGS = ('ko', 'en')
MAX_FILE = 150 * 1024
MAX_WIDTH = 920
MAX_TOTAL = 6 * 1024 * 1024


def _tool_keys() -> tuple[str, ...]:
    """도구의 KEYS(임포트하지 않고 읽는다 — 도구는 임포트 때 Qt 환경 변수를 만진다)."""
    tree = ast.parse(TOOL.read_text(encoding='utf-8'))
    for node in tree.body:
        if isinstance(node, ast.AnnAssign) and getattr(node.target, 'id', '') == 'KEYS':
            return tuple(ast.literal_eval(node.value))
    raise AssertionError('make_tutorial_shots.KEYS 를 못 찾음')


def _contract_keys() -> tuple[str, ...]:
    text = CONTRACT.read_text(encoding='utf-8')
    return tuple(re.findall(r'^\| `(\w+)` \|', text, flags=re.M))


def _png_size(path: Path) -> tuple[int, int]:
    head = path.read_bytes()[:24]
    assert head[:8] == b'\x89PNG\r\n\x1a\n', f'{path.name}: PNG 아님'
    return struct.unpack('>II', head[16:24])


@pytest.fixture(scope='module')
def manifest() -> dict:
    return json.loads((TUTORIAL / 'MANIFEST.json').read_text(encoding='utf-8'))


def test_keys_match_contract():
    keys = _tool_keys()
    if CONTRACT.is_file():
        assert keys == _contract_keys(), '도구 KEYS 가 계약 표와 다르다'
    assert len(keys) == len(set(keys))


def test_manifest_shape(manifest):
    assert manifest['window'] == [1280, 800]
    assert re.fullmatch(r'\d{4}-\d{2}-\d{2}', manifest['generated'])
    for key, entry in manifest['shots'].items():
        assert len(entry['crop']) == 4, key
        assert all(len(mark) == 4 for mark in entry['marks']), key


def test_every_key_present_or_listed_missing(manifest):
    missing = manifest.get('missing', {})
    for key in _tool_keys():
        if key in missing:
            continue
        entry = manifest['shots'].get(key)
        assert entry is not None, f'{key}: MANIFEST 에 없음(missing 에도 없음)'
        for lang in LANGS:
            name = entry.get(lang)
            assert name == f'{key}_{lang}.png', f'{key}: {lang} 파일 이름 {name!r}'
            assert (TUTORIAL / name).is_file(), f'{name} 파일이 없음'


def test_size_budget():
    pngs = sorted(TUTORIAL.glob('*.png'))
    assert pngs, '튜토리얼 그림이 하나도 없음'
    total = 0
    for path in pngs:
        size = path.stat().st_size
        total += size
        assert size <= MAX_FILE, f'{path.name}: {size / 1024:.0f} KB > 150 KB'
        width, _height = _png_size(path)
        assert width <= MAX_WIDTH, f'{path.name}: 폭 {width} > {MAX_WIDTH}'
    assert total <= MAX_TOTAL, f'전체 {total / 1024 / 1024:.2f} MB > 6 MB'


def test_no_stray_files(manifest):
    """MANIFEST 에 없는 그림이 폴더에 남아 exe 를 키우지 않게."""
    listed = {entry[lang] for entry in manifest['shots'].values() for lang in LANGS if lang in entry}
    stray = {p.name for p in TUTORIAL.glob('*.png')} - listed
    assert not stray, f'MANIFEST 에 없는 그림: {sorted(stray)}'
