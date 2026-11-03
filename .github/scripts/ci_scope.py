"""Whether a pull request needs ci.yml's heavy jobs, for ci.yml's scope job.

A pull request whose own commits are all ci: or docs: commits (maintainer
request 2026-11-03) and whose changed files are all ones the heavy jobs never
read skips those jobs: cargo test, the installers, the playbacks, the screen
gallery, Miri and fuzzing. A file counts as light when it is under docs/, ends
in .md, or is under .github/, unless it is ci.yml itself or a file a heavy
job runs or reads. Those are found in ci.yml: every .github path named in a
job gated on the scope job, the local modules those scripts import and the
files under .github/ their string literals name. A prefix alone is not
enough: a ci: commit that touches a crate or integrity.py still runs
everything.

  python3 ci_scope.py --commits commits.jsonl --files files.jsonl \\
      --expected-commits N --expected-files M --workflow .github/workflows/ci.yml

reads one JSON object per line, {"parents": n, "subject": s} for each commit
of `gh api pulls/<n>/commits` and {"filename": f, "previous_filename": p} for
each file of `gh api pulls/<n>/files`, and prints "true" when the heavy jobs
must run and "false" when they may be skipped. Anything it cannot tell, such
as a list cut short by the API's page limits, counts as heavy. Standard
library only.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

LIGHT_SUBJECT = re.compile(r"^(ci|docs)(\([^)]*\))?!?:")
# The condition every gated job of ci.yml carries.
GATE = "needs.scope.outputs.heavy == 'true'"
WORKFLOW = ".github/workflows/ci.yml"
JOB_START = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
# `needs: x`, `needs: [x, y]` or a block list under `needs:`.
NEEDS = re.compile(r"^    needs:[ \t]*(.*)\n((?:      - .*\n)*)", re.MULTILINE)
GITHUB_PATH = re.compile(r"\.github/[A-Za-z0-9_./-]+")
IMPORT = re.compile(r"^\s*(?:from|import)\s+([A-Za-z_][A-Za-z0-9_]*)", re.MULTILINE)
STRING = re.compile(r"""["']([A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*)["']""")


def light_subject(subject: str) -> bool:
    return bool(LIGHT_SUBJECT.match(subject))


def gated_jobs(workflow: str) -> dict[str, str]:
    """The text of each job of ci.yml that runs only when the scope job says so.

    A job gated on the scope job is one, and so is one that needs such a job
    (manage-e2e after bundle): it is skipped along with it.
    """
    jobs: dict[str, list[str]] = {}
    current = None
    in_jobs = False
    for line in workflow.splitlines():
        if line.startswith("jobs:"):
            in_jobs = True
            continue
        match = JOB_START.match(line) if in_jobs else None
        if match:
            current = match.group(1)
            jobs[current] = []
        elif current:
            jobs[current].append(line)
    texts = {name: "\n".join(lines) + "\n" for name, lines in jobs.items()}
    needs = {name: set(re.findall(r"[A-Za-z0-9_][A-Za-z0-9_-]*", match.group(1) + " " + match.group(2)))
             for name, text in texts.items()
             if (match := NEEDS.search(text))}
    gated = {name for name, text in texts.items() if GATE in text}
    while grown := {name for name, wanted in needs.items() if wanted & gated} - gated:
        gated |= grown
    return {name: texts[name] for name in sorted(gated)}


def heavy_inputs(workflow: str, root: Path) -> set[str]:
    """ci.yml and every file under .github/ that a gated job runs or reads."""
    root = root.resolve()
    github = root / ".github"
    found = {WORKFLOW}
    pending = []

    # Files only: a directory a job names (the unittest discover root) or a
    # script's "." is not an input, and taking one in would make every file
    # under it heavy.
    def add(path: Path) -> None:
        path = path.resolve()
        if not path.is_file() or not path.is_relative_to(github):
            return
        relative = path.relative_to(root).as_posix()
        if relative not in found:
            found.add(relative)
            if path.suffix == ".py":
                pending.append(path)

    for text in gated_jobs(workflow).values():
        # Windows steps write the paths with backslashes.
        for token in GITHUB_PATH.findall(text.replace("\\", "/")):
            add(root / token.rstrip("/."))
    while pending:
        script = pending.pop()
        text = script.read_text(encoding="utf-8")
        for module in IMPORT.findall(text):
            add(script.parent / f"{module}.py")
        for literal in STRING.findall(text):
            for base in (script.parent, github, root):
                add(base / literal)
    return found


def light_path(path: str, heavy: set[str]) -> bool:
    if path in heavy:
        return False
    return path.startswith(("docs/", ".github/")) or path.endswith(".md")


def decide(commits, files, expected_commits, expected_files, heavy: set[str]) -> tuple[bool, str]:
    """(heavy jobs run, why) for a pull request's commits and changed files."""
    if expected_commits is None or len(commits) != expected_commits:
        return True, f"the commit list has {len(commits)} of {expected_commits} commits"
    if expected_files is None or len(files) != expected_files:
        return True, f"the file list has {len(files)} of {expected_files} files"
    # A merge of the base branch brings no change of the pull request's own.
    own = [item["subject"] for item in commits if item["parents"] < 2]
    if not own:
        return True, "the pull request has no commit of its own"
    for subject in own:
        if not light_subject(subject):
            return True, f"commit '{subject}' is not a ci: or docs: commit"
    for item in files:
        for path in (item["filename"], item.get("previous_filename")):
            if path and not light_path(path, heavy):
                return True, f"{path} is read by a heavy job"
    return False, "every commit is a ci: or docs: commit and no heavy job reads a changed file"


def read_lines(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--commits", type=Path, required=True)
    parser.add_argument("--files", type=Path, required=True)
    parser.add_argument("--expected-commits", type=int)
    parser.add_argument("--expected-files", type=int)
    parser.add_argument("--workflow", type=Path, default=Path(WORKFLOW))
    parser.add_argument("--summary", type=Path, help="markdown file to append the decision to")
    args = parser.parse_args()
    root = args.workflow.resolve().parents[2]
    heavy = heavy_inputs(args.workflow.read_text(encoding="utf-8"), root)
    run, why = decide(read_lines(args.commits), read_lines(args.files),
                      args.expected_commits, args.expected_files, heavy)
    print("true" if run else "false")
    print(f"heavy jobs {'run' if run else 'skipped'}: {why}", file=sys.stderr)
    if args.summary:
        with args.summary.open("a", encoding="utf-8") as summary:
            summary.write(f"Heavy jobs {'run' if run else 'skipped'}: {why}.\n")


if __name__ == "__main__":
    main()
