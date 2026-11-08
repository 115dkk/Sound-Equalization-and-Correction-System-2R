"""Shared subprocess wrappers for trusted CI scripts."""
from __future__ import annotations

import subprocess


def run(*args: str, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(
        list(args),
        check=check,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )


def git(*args: str, quotepath: bool = True, runner=run) -> str:
    command = ["git"]
    if quotepath:
        command.extend(["-c", "core.quotepath=off"])
    return runner(*command, *args).stdout.strip()
