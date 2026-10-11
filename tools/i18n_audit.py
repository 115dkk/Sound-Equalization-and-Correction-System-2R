# -*- coding: utf-8 -*-
"""위저드 소스의 한글 문자열 중 영어 번역이 등록되지 않은 것을 찾는다.

    .venv-secs\\Scripts\\python.exe tools\\i18n_audit.py [--list] [--json out.json]

문자열 리터럴(독스트링 제외)과 f-string 을 모은다. f-string 은 `{0}` 자리표시자
꼴로 바꿔 카탈로그 패턴과 맞춰 본다. 로그·예외처럼 화면에 안 나오는 것도 섞여
나오므로 결과는 사람이 거른다.
"""
from __future__ import annotations

import ast
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))

HANGUL = re.compile(r'[가-힣]')


def _docstring_nodes(tree: ast.AST) -> set[int]:
    found = set()
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            body = getattr(node, 'body', [])
            if body and isinstance(body[0], ast.Expr) and isinstance(body[0].value, ast.Constant) \
                    and isinstance(body[0].value.value, str):
                found.add(id(body[0].value))
    return found


def _fstring_source(node: ast.JoinedStr) -> str:
    out, index = '', 0
    for part in node.values:
        if isinstance(part, ast.Constant):
            out += str(part.value)
        else:
            out += '{%d}' % index
            index += 1
    return out


def collect(path: Path) -> list[tuple[int, str, bool]]:
    tree = ast.parse(path.read_text(encoding='utf-8'))
    skip = _docstring_nodes(tree)
    inside_f = set()
    rows = []
    for node in ast.walk(tree):
        if isinstance(node, ast.JoinedStr):
            for part in ast.walk(node):
                if isinstance(part, ast.Constant):
                    inside_f.add(id(part))
            text = _fstring_source(node)
            if HANGUL.search(text):
                rows.append((node.lineno, text, True))
    for node in ast.walk(tree):
        if isinstance(node, ast.Constant) and isinstance(node.value, str) \
                and id(node) not in skip and id(node) not in inside_f and HANGUL.search(node.value):
            rows.append((node.lineno, node.value, False))
    return sorted(rows)


def main() -> int:
    import secs.wizard.strings  # noqa: F401  카탈로그 등록
    from secs.i18n import translate
    targets = sorted((ROOT / 'src' / 'secs' / 'wizard').glob('*.py'))
    targets += [ROOT / 'src' / 'secs' / name for name in
                ('sheet.py', 'export.py', 'verify.py', 'verify_flow.py', 'design.py', 'bass.py',
                 'peq_refine.py', 'model.py')]
    targets += sorted((ROOT / 'src' / 'secs' / 'tesira').glob('*.py'))
    missing: dict[str, list] = {}
    total = 0
    for path in targets:
        if not path.exists() or path.name == 'strings.py':
            continue
        for line, text, is_f in collect(path):
            total += 1
            probe = re.sub(r'\{\d+\}', 'X', text) if is_f else text
            if translate(probe, 'en') == probe:
                missing.setdefault(path.name, []).append({'line': line, 'text': text, 'f': is_f})
    count = sum(len(v) for v in missing.values())
    print(f'한글 문자열 {total}개 중 영어 미등록 {count}개')
    for name, rows in missing.items():
        print(f'  {name:24s} {len(rows):4d}')
        if '--list' in sys.argv:
            for row in rows:
                print(f'      {row["line"]:4d} {"f" if row["f"] else " "} {row["text"][:100]!r}')
    if '--json' in sys.argv:
        out = Path(sys.argv[sys.argv.index('--json') + 1])
        out.write_text(json.dumps(missing, ensure_ascii=False, indent=1), encoding='utf-8')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
