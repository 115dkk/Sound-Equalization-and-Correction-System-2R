# -*- coding: utf-8 -*-
"""공개하는 소스에 상용 룸 보정 제품의 상호와 그 기능 약어가 다시 들어오지 않게(2026-10-10 사용자 결정 — 기능 이름은 MIMO ·
BA 로 바꿨다). 같은 검사를 공개 스냅샷 도구(`tools/make_public_snapshot.py` 의 `BRAND`)가 스냅샷 전체에 한다.

본다: `src/` 의 모든 텍스트 파일 · 파일 이름, 공개하는 `tools/` 도구(저장소에서는 스냅샷 허용 목록 `TOOLS_INCLUDE`, 공개본에서는
`tools/` 전부 — 공개본에는 허용된 도구만 있다), 이 폴더(`tests/`)의 시험. 이 파일의 패턴은 낱말이 그대로 적히지 않게
글자 묶음으로 쓴다(이 파일도 공개된다). 옛 이름이 남아도 되는 곳은 옛 파일 이주 표뿐인데, 그 키들은 소문자 조각이라 걸리지 않는다.
"""
from __future__ import annotations

import ast
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

#: (이름, 패턴) — 상호 둘(영문 · 한글, 대소문자 무시), BA 옛 약어(대소문자 무시), 저역 감쇠 옛 약어(대문자 낱말만 —
#: 'part' · 'START' · 'Artifact' 는 아님).
PATTERNS = (
    ('상호(영문)', re.compile(r'[Dd][Ii][Rr][Aa][Cc]')),
    ('상호(한글)', re.compile(chr(0xB514) + chr(0xB77D))),
    ('BA 옛 약어', re.compile(r'[Dd][Ll][Bb][Cc]')),
    ('저역 감쇠 옛 약어', re.compile(r'(?<![A-Za-z0-9])A[R]T(?![A-Za-z0-9])')),
)
BINARY_SUFFIXES = {'.png', '.ico', '.npz', '.mdat', '.wav', '.zip', '.pyc', '.dll', '.exe', '.gif', '.jpg'}


def _public_tools() -> list[Path]:
    snapshot = ROOT / 'tools' / 'make_public_snapshot.py'
    if not snapshot.exists():                       # 공개본: tools/ 에는 허용된 도구만 있다
        return sorted(p for p in (ROOT / 'tools').glob('*') if p.is_file())
    tree = ast.parse(snapshot.read_text(encoding='utf-8'))
    for node in ast.walk(tree):
        target = getattr(node, 'target', None) if isinstance(node, ast.AnnAssign) else None
        if isinstance(target, ast.Name) and target.id == 'TOOLS_INCLUDE':
            return [ROOT / 'tools' / key.value for key in node.value.keys]
    raise AssertionError('make_public_snapshot.TOOLS_INCLUDE 를 못 찾았다')


def _files() -> list[Path]:
    src = [p for p in (ROOT / 'src').rglob('*') if p.is_file() and '__pycache__' not in p.parts]
    tests = sorted((ROOT / 'tests').glob('*.py'))
    return sorted(src) + _public_tools() + tests


def _hits(path: Path) -> list[str]:
    out = []
    rel = path.relative_to(ROOT).as_posix()
    for label, regex in PATTERNS:
        if regex.search(rel):
            out.append(f'{rel}: [{label}] (파일 이름)')
    if path.suffix.lower() in BINARY_SUFFIXES:
        return out
    try:
        text = path.read_text(encoding='utf-8')
    except (UnicodeDecodeError, FileNotFoundError):
        return out
    for number, line in enumerate(text.splitlines(), 1):
        for label, regex in PATTERNS:
            if regex.search(line):
                out.append(f'{rel}:{number}: [{label}] {line.strip()[:120]}')
    return out


def test_public_source_has_no_brand_names():
    files = _files()
    assert len(files) > 50 and any(p.name == 'calibration.py' for p in files)        # 정말 소스를 봤는지
    hits = [hit for path in files for hit in _hits(path)]
    assert not hits, '\n'.join(hits)


def test_patterns_catch_the_old_names():
    """패턴이 옛 이름은 잡고 비슷한 낱말은 놓아주는지(글자 묶음으로 쓴 패턴이 틀리지 않았는지)."""
    words = {'Dir' + 'ac Live': 0, 'dir' + 'ac_engine': 0, chr(0xB514) + chr(0xB77D) + '식': 1, 'DL' + 'BC 수동': 2, 'dl' + 'bc': 2,
             'A' + 'RT 계산': 3, 'SECS_' + 'A' + 'RT_EFFORT': 3}
    for word, index in words.items():
        assert PATTERNS[index][1].search(word), word
    for word in ('part', 'START_DELAYS', 'Artifact', 'start_art', 'MIMO 계산', 'RC_BA_MIMO', 'ArtResult'):
        assert not any(regex.search(word) for _label, regex in PATTERNS), word
