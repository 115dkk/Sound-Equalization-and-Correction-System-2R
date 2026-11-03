"""Screen gallery review issues to close, for ui-gallery-close.yml.

ui-gallery.yml opens one review issue per pull request, titled
"화면 갤러리 리뷰: PR #<n>", and only ever comments on it, so the issues piled
up after every merge. When any pull request closes, the workflow closes every
open review issue whose pull request is no longer open; issues left over from
earlier pull requests go at the same time.

  python3 gallery_close.py --issues issues.json --open-prs prs.json

reads the JSON of `gh issue list --json number,title` and
`gh pr list --json number` and prints "<issue> <pull request>" for each issue
to close. Standard library only.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

# The title ui-gallery.yml gives a pull request's review; a manual run's
# review ("수동 실행 (<ref>)") has no pull request and stays open.
REVIEW_TITLE = re.compile(r"화면 갤러리 리뷰: PR #(\d+)")


def stale_reviews(issues: list[dict], open_prs: set[int]) -> list[tuple[int, int]]:
    """(issue, pull request) for each review whose pull request is not open."""
    stale = []
    for item in issues:
        match = REVIEW_TITLE.fullmatch(item["title"])
        if match and int(match.group(1)) not in open_prs:
            stale.append((item["number"], int(match.group(1))))
    return sorted(stale)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--issues", type=Path, required=True)
    parser.add_argument("--open-prs", type=Path, required=True)
    args = parser.parse_args()
    issues = json.loads(args.issues.read_text(encoding="utf-8"))
    open_prs = {item["number"] for item in json.loads(args.open_prs.read_text(encoding="utf-8"))}
    for number, pr in stale_reviews(issues, open_prs):
        print(number, pr)


if __name__ == "__main__":
    main()
