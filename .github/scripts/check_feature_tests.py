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


# These ignored tests run in the named CI job with ``--ignored``.
IGNORED_CI_TESTS = {
    "secs-filters::exported_configs_play_as_designed": "eqapo-playback",
    "secs-filters::reaper_plays_convolution_bundles_as_written": "reaper-playback",
    "secs-camilla::camilla_exports_play_as_designed": "camilla-playback",
    "secs-filters::generation_entry_plays_from_unrelated_working_directory_across_two_applies": "eqapo-playback",
    "secs-camilla::one_running_pipeline_switches_generation_firs_by_unique_paths": "camilla-playback",
}


# Features the workspace build turns on for a crate through another member
# (resolver 3 unifies them), which a lone ``cargo test -p`` would leave off:
# the public API crate's ``client`` feature comes from the Python binding,
# so its client tests run in CI's workspace run and must be listed with it.
CRATE_FEATURES = {
    "sound-equalization-and-correction-system-rust-api": ["--all-features"],
}


def list_command(crate: str, ignored_only: bool) -> list[str]:
    """The ``cargo test -- --list`` invocation for one crate."""
    command = ["cargo", "test", "--locked", "-q", "-p", crate, "--all-targets",
               *CRATE_FEATURES.get(crate, []),
               "--", "--list", "--format", "terse"]
    if ignored_only:
        command.append("--ignored")
    return command


def missing(wanted: dict[str, set[str]], listed: dict[str, set[str]],
            ignored: dict[str, set[str]]) -> list[str]:
    return sorted(
        f"{crate}::{name}"
        for crate, names in wanted.items()
        for name in names
        if name not in listed.get(crate, set())
        or (name in ignored.get(crate, set())
            and f"{crate}::{name}" not in IGNORED_CI_TESTS)
    )


def main() -> int:
    with open("features.toml", "rb") as fh:
        wanted = referenced_tests(tomllib.load(fh))
    listed = {}
    ignored = {}
    for crate in sorted(wanted):
        for ignored_only, destination in ((False, listed), (True, ignored)):
            command = list_command(crate, ignored_only)
            result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8")
            if result.returncode != 0:
                print(result.stderr)
                suffix = " --ignored" if ignored_only else ""
                print(f"::error::cargo test --list{suffix} failed for {crate}")
                return 1
            destination[crate] = listed_tests(result.stdout)
    gone = missing(wanted, listed, ignored)
    if gone:
        print("::error::features.toml names tests that Cargo does not run:\n" + "\n".join(gone))
        return 1
    total = sum(len(names) for names in wanted.values())
    print(f"all {total} referenced tests are compiled and listed by Cargo")
    return 0


if __name__ == "__main__":
    sys.exit(main())
