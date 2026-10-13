#!/usr/bin/env python3
"""Every test an implemented feature names must be one Cargo compiles and runs.

secs-policy finds `#[test] fn name` in a crate's sources, but a file that no
module includes is never compiled. This check asks Cargo for the real test
list of each crate (`cargo test -p <crate> --all-targets -- --list`)
and fails when a referenced test is not in it. Run by ci.yml after the tests
are built. The pure functions are unit-tested in ``test_check_feature_tests.py``.
"""
from __future__ import annotations

import subprocess
import sys
import tomllib
from collections import defaultdict
from pathlib import Path


def referenced_tests(registry: dict) -> dict[str, set[str]]:
    """crate -> test function names, for features marked implemented."""
    wanted: dict[str, set[str]] = defaultdict(set)
    for feature in registry.get("feature", []):
        if feature.get("status") != "implemented":
            continue
        for reference in feature.get("tests", []):
            crate, _, name = reference.partition("::")
            if crate and name:
                wanted[crate].add(name)
    return dict(wanted)


def listed_tests(terse_list: str) -> set[str]:
    """Function names from `--list --format terse` lines (`path::name: test`)."""
    names = set()
    for line in terse_list.splitlines():
        if line.endswith(": test"):
            path = line[: -len(": test")]
            names.add(path.rsplit("::", 1)[-1])
    return names


def missing(wanted: dict[str, set[str]], listed: dict[str, set[str]]) -> list[str]:
    return sorted(f"{crate}::{name}" for crate, names in wanted.items()
                  for name in names if name not in listed.get(crate, set()))


def main() -> int:
    with open("features.toml", "rb") as fh:
        wanted = referenced_tests(tomllib.load(fh))
    listed = {}
    for crate in sorted(wanted):
        result = subprocess.run(
            ["cargo", "test", "--locked", "-q", "-p", crate, "--all-targets",
             "--", "--list", "--format", "terse"],
            capture_output=True, text=True, encoding="utf-8")
        if result.returncode != 0:
            print(result.stderr)
            print(f"::error::cargo test --list failed for {crate}")
            return 1
        listed[crate] = listed_tests(result.stdout)
    gone = missing(wanted, listed)
    if gone:
        print("::error::features.toml names tests that Cargo does not run:\n" + "\n".join(gone))
        return 1
    total = sum(len(names) for names in wanted.values())
    print(f"all {total} referenced tests are compiled and listed by Cargo")
    return 0


if __name__ == "__main__":
    sys.exit(main())
