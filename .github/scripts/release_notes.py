#!/usr/bin/env python3
"""Release notes for release.yml: the CHANGELOG.md section of the version,
or the commit subjects since the previous tag when the section is missing.

The pure function is unit-tested in ``test_release_notes.py``.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path

SECTION_RE = re.compile(r"^## (\S+)")


def changelog_section(text: str, version: str) -> str | None:
    """Body of the ``## <version> ...`` section, without its heading."""
    lines = text.splitlines()
    body: list[str] | None = None
    for line in lines:
        match = SECTION_RE.match(line)
        if match:
            if body is not None:
                break
            if match.group(1) == version:
                body = []
            continue
        if body is not None:
            body.append(line)
    if body is None:
        return None
    return "\n".join(body).strip() or None


def header(version: str, sha: str, upstream_version: str) -> str:
    kind = "사전 출시판" if version.startswith("0.") else "정식 판"
    return (f"SECS 2R {version} ({kind}). SECS 2 {upstream_version} 기준, 커밋 `{sha[:12]}`에서 빌드했습니다.\n\n"
            "설치 파일은 아래 자산에 있고, `SHA256SUMS.txt`로 받은 파일을 확인할 수 있습니다.\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--version", required=True)
    parser.add_argument("--previous", default="")
    parser.add_argument("--sha", required=True)
    args = parser.parse_args(argv)

    with open("UPSTREAM.toml", "rb") as fh:
        upstream_version = tomllib.load(fh)["upstream"]["version"]
    out = [header(args.version, args.sha, upstream_version)]
    section = changelog_section(Path("CHANGELOG.md").read_text(encoding="utf-8"), args.version)
    if section:
        out.append(section)
    else:
        log_range = f"{args.previous}..{args.sha}" if args.previous else args.sha
        log = subprocess.run(["git", "log", "--no-merges", "--pretty=format:- %s", log_range],
                             check=True, capture_output=True, text=True, encoding="utf-8").stdout
        subjects = [line for line in log.splitlines() if not line.startswith("- chore(release): ")]
        out.append("## 바뀐 것\n\n" + ("\n".join(subjects) or "- (없음)"))
    print("\n".join(out).rstrip() + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
