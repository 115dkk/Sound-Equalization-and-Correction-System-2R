#!/usr/bin/env python3
"""The upstream SECS 2 tree and how SECS 2R follows it (docs/adr/0001).

Two subcommands:

* ``check-tree``: the upstream files in this repository must be byte-identical
  to the commit named in ``UPSTREAM.toml``. The Python tree is the oracle the
  Rust port is checked against, and keeping it untouched lets upstream
  releases merge without conflicts. Run by ci.yml.
* ``watch``: compare upstream's branch head with ``UPSTREAM.toml``. When
  upstream moved, write an issue body (commits, changed files and the crates
  each file reaches) and set ``has_update=true`` in ``$GITHUB_OUTPUT``. Run by
  upstream-watch.yml, which opens the issue.

The pure functions are unit-tested in ``test_upstream.py``.
"""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

UPSTREAM_TOML = Path("UPSTREAM.toml")

# Upstream files kept byte-identical. README.md and CHANGELOG.md are SECS 2R's
# own (upstream's copies live in docs/upstream/), .gitignore gains Rust and
# Node entries, so those three are not listed.
UPSTREAM_PATHS = (
    "src",
    "tests",
    "tools",
    "docs/STRUCTURE.md",
    "배포",
    "run_secs.py",
    "SECS.spec",
    "build_exe.ps1",
    "requirements.txt",
    "requirements-dev.txt",
    "LICENSE",
    "THIRD_PARTY_NOTICES.md",
)

# Where a change in an upstream file lands in SECS 2R. The first matching
# prefix wins, so specific files come before their directories.
REACH = (
    ("src/secs/correction_engine.py", "secs-engine (레벨·딜레이, BA, XO, RC)"),
    ("src/secs/calibration.py", "secs-engine (계산부), secs-service (설정·캐시·내보내기)"),
    ("src/secs/mimo_iir.py", "secs-engine (MIMO IIR 맞춤), secs-gpu"),
    ("src/secs/mimo.py", "secs-engine (MIMO)"),
    ("src/secs/scorecard.py", "secs-engine (성적표)"),
    ("src/secs/model.py", "secs-project (.secs, 역할 어휘)"),
    ("src/secs/mdat_loader.py", "secs-mdat, secs-project (제목 해석)"),
    ("src/secs/rew_mdat/serialization/", "secs-jos"),
    ("src/secs/rew_mdat/", "secs-mdat"),
    ("src/secs/rewapi/session.py", "secs-project (.irs 저장소), secs-rew"),
    ("src/secs/rewapi/", "secs-rew"),
    ("src/secs/export_camilladsp.py", "secs-camilla"),
    ("src/secs/export_filters.py", "secs-filters (필터 값 텍스트, Equalizer APO)"),
    ("src/secs/proc.py", "secs-rew (REW 실행)"),
    ("src/secs/version.py", "UPSTREAM.toml"),
    ("src/secs/i18n.py", "화면 카탈로그 (apps/secs2r/ui)"),
    ("src/secs/translations.py", "화면 카탈로그 (apps/secs2r/ui)"),
    ("src/secs/wizard/en_g", "화면 카탈로그 (apps/secs2r/ui)"),
    ("src/secs/wizard/strings.py", "화면 카탈로그 (apps/secs2r/ui)"),
    ("src/secs/wizard/tooltips.py", "화면 카탈로그 (apps/secs2r/ui)"),
    ("src/secs/wizard/tutorial_", "화면: 튜토리얼 (apps/secs2r/ui)"),
    ("src/secs/wizard/help_dialog.py", "화면: 도움말 (apps/secs2r/ui)"),
    ("src/secs/wizard/assets/", "화면 자산 (apps/secs2r/ui)"),
    ("src/secs/wizard/", "화면 (apps/secs2r/ui)"),
    ("src/", "검토 필요"),
    ("tests/", "오라클 골든 (tests/oracle)"),
    ("tools/", "참고 (개발 도구)"),
    ("README.md", "docs/upstream/README.md"),
    ("CHANGELOG.md", "docs/upstream/CHANGELOG.md"),
)

VERSION_RE = re.compile(r"^(STAGE|BUILD)\s*=\s*'([^']*)'", re.MULTILINE)


# ---------------------------------------------------------------------------
# Pure logic (unit-tested)
# ---------------------------------------------------------------------------
def reach(path: str) -> str:
    for prefix, target in REACH:
        if path == prefix or path.startswith(prefix):
            return target
    return "검토 필요"


def upstream_version(version_py: str) -> str:
    """'Beta 1.2.0' from upstream's ``src/secs/version.py``."""
    found = dict(VERSION_RE.findall(version_py))
    stage, build = found.get("STAGE", ""), found.get("BUILD", "")
    return f"{stage} {build}".strip() or "알 수 없음"


def parse_name_status(text: str) -> list[tuple[str, str]]:
    """``git diff --name-status`` lines as (status, path); a rename keeps its new path."""
    rows = []
    for line in text.splitlines():
        parts = line.split("\t")
        if len(parts) >= 2:
            rows.append((parts[0][:1], parts[-1]))
    return rows


STATUS_WORDS = {"A": "추가", "M": "수정", "D": "삭제", "R": "이름 바꿈", "C": "복사", "T": "형식 바뀜"}


def issue_title(new_head: str, version: str) -> str:
    return f"업스트림 SECS 2 {version} ({new_head[:7]}) 이식"


def issue_body(repo: str, branch: str, old: str, old_version: str, new: str, new_version: str,
               commits: list[str], changes: list[tuple[str, str]], rewritten: bool) -> str:
    lines = [
        f"업스트림 [{repo}]({repo}) `{branch}`에 새 커밋이 있습니다.",
        "",
        f"- 이식한 커밋: `{old[:12]}` (SECS 2 {old_version})",
        f"- 업스트림 머리: `{new[:12]}` (SECS 2 {new_version})",
        "",
    ]
    if rewritten:
        lines += [
            "> 이식한 커밋이 업스트림 머리의 조상이 아닙니다. 업스트림이 기록을 다시 썼으므로 병합 전에 두 기록을 비교해야 합니다.",
            "",
        ]
    lines += ["### 커밋", ""]
    lines += [f"- {c}" for c in commits] or ["- (없음)"]
    lines += ["", "### 바뀐 파일과 닿는 곳", "", "| 파일 | 변경 | 닿는 곳 |", "| --- | --- | --- |"]
    for status, path in changes:
        lines.append(f"| `{path}` | {STATUS_WORDS.get(status, status)} | {reach(path)} |")
    if not changes:
        lines.append("| (없음) | | |")
    lines += [
        "",
        "### 할 일",
        "",
        "- [ ] 업스트림을 병합합니다. `README.md`와 `CHANGELOG.md`가 충돌하면 우리 쪽을 남기고 업스트림 판은 `docs/upstream/`에 옮깁니다.",
        "- [ ] 바뀐 파이썬을 위 표의 크레이트별 이식 작업서로 나눕니다.",
        "- [ ] 오라클 골든을 새 커밋에서 다시 뽑습니다.",
        "- [ ] `UPSTREAM.toml`의 `commit`, `version`, `date`를 올립니다.",
        "",
        "이 이슈는 `upstream-watch.yml`이 열었습니다.",
    ]
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------
# I/O wrapper (runs in CI)
# ---------------------------------------------------------------------------
def _run(*args: str, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(list(args), check=check, capture_output=True, text=True,
                          encoding="utf-8", errors="replace")


def _git(*args: str) -> str:
    return _run("git", "-c", "core.quotepath=off", *args).stdout.strip()


def load_upstream() -> dict:
    with open(UPSTREAM_TOML, "rb") as fh:
        return tomllib.load(fh)["upstream"]


def check_tree() -> int:
    commit = load_upstream()["commit"]
    if _run("git", "cat-file", "-e", f"{commit}^{{commit}}", check=False).returncode != 0:
        print(f"::error::UPSTREAM.toml names {commit}, which is not in this clone "
              "(check out with fetch-depth: 0)")
        return 1
    diff = _run("git", "-c", "core.quotepath=off", "diff", "--stat", commit, "--", *UPSTREAM_PATHS,
                check=False)
    untracked = _git("ls-files", "--others", "--exclude-standard", "--", *UPSTREAM_PATHS)
    if diff.stdout.strip() or untracked:
        print("::error::the upstream Python tree differs from UPSTREAM.toml's commit "
              f"{commit[:12]}; SECS 2R never edits it in place (docs/adr/0001)")
        print(diff.stdout)
        if untracked:
            print("untracked:\n" + untracked)
        return 1
    print(f"upstream tree matches {commit[:12]} ({len(UPSTREAM_PATHS)} paths)")
    return 0


def watch(output: Path | None) -> int:
    upstream = load_upstream()
    repo, branch, old = upstream["repository"], upstream["branch"], upstream["commit"]
    remote = _run("git", "ls-remote", repo, f"refs/heads/{branch}").stdout.split()
    if not remote:
        print(f"::error::{repo} has no branch {branch}")
        return 1
    new = remote[0]
    payload = {"has_update": "false", "head": new}
    if new != old:
        _run("git", "fetch", "--no-tags", "--quiet", repo, branch)
        rewritten = _run("git", "merge-base", "--is-ancestor", old, "FETCH_HEAD",
                         check=False).returncode != 0
        log_range = f"{old}..FETCH_HEAD" if not rewritten else "FETCH_HEAD"
        commits = [c for c in _git("log", "--no-merges", "--format=%h %s", "-n", "50",
                                    log_range).splitlines() if c]
        changes = parse_name_status(_git("diff", "--name-status", old, "FETCH_HEAD"))
        new_version = upstream_version(_git("show", "FETCH_HEAD:src/secs/version.py"))
        title = issue_title(new, new_version)
        body = issue_body(repo, branch, old, upstream["version"], new, new_version,
                          commits, changes, rewritten)
        Path("upstream-issue.md").write_text(body, encoding="utf-8")
        payload.update(has_update="true", title=title)
        print(body)
    else:
        print(f"upstream {branch} is at {new[:12]}, already ported")
    text = "".join(f"{key}={value}\n" for key, value in payload.items())
    if output:
        with open(output, "a", encoding="utf-8") as fh:
            fh.write(text)
    print(text, end="")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=["check-tree", "watch"])
    args = parser.parse_args(argv)
    if args.command == "check-tree":
        return check_tree()
    out = os.environ.get("GITHUB_OUTPUT")
    return watch(Path(out) if out else None)


if __name__ == "__main__":
    sys.exit(main())
