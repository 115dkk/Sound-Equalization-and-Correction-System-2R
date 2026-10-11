"""소스에 정의되지 않은 전역 이름이 없는지(2026-10-10 코드 리뷰).

3489df8 의 문구 일괄 치환('Fake → Demo')이 변수 이름까지 바꿔 `new_project(demo)` 가 되었고, .secs 없이 켜면 창이 뜨기
전에 NameError 로 죽었다. 테스트는 그 경로(run_wizard)를 돌지 않아 못 잡았다 — pyflakes 가 없어 symtable 로 본다:
함수 · 클래스 안에서 읽는 전역 이름이 모듈에서 정의 · 임포트되지도, builtins 에 있지도 않으면 실패."""
from __future__ import annotations

import builtins
import symtable
from pathlib import Path

SRC = Path(__file__).resolve().parents[1] / 'src' / 'secs'
BUILTINS = set(dir(builtins)) | {'__file__', '__name__', '__doc__', '__spec__', '__package__', '__builtins__',
                                 '__loader__', '__path__', '__annotations__'}
#: `globals()[이름] = …` 로 만드는 이름(정적으로는 안 보임) — i18n 의 번역 위젯 클래스.
DYNAMIC = {'i18n.py': {'QWidget', 'QLabel', 'QPushButton', 'QCheckBox', 'QDialog', 'QFrame', 'QGroupBox'}}


def _undefined(path: Path) -> list[tuple[str, int, str]]:
    src = path.read_text(encoding='utf-8')
    top = symtable.symtable(src, str(path), 'exec')
    names = {s.get_name() for s in top.get_symbols() if s.is_assigned() or s.is_imported() or s.is_namespace()}
    out = []

    def walk(table):
        for child in table.get_children():
            for sym in child.get_symbols():
                name = sym.get_name()
                if sym.is_global() and sym.is_referenced() and not sym.is_assigned() \
                        and name not in names and name not in BUILTINS:
                    out.append((child.get_name(), child.get_lineno(), name))
            walk(child)
    walk(top)
    return out


def test_no_undefined_global_names_in_src():
    bad = {}
    for p in sorted(SRC.rglob('*.py')):
        rel = p.relative_to(SRC).as_posix()
        hits = [h for h in _undefined(p) if h[2] not in DYNAMIC.get(rel, set())]
        if hits:
            bad[rel] = hits
    assert not bad, bad
