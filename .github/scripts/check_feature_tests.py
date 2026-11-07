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
import argparse
import json
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


def cargo_listings(wanted: dict[str, set[str]]) -> tuple[dict[str, set[str]], dict[str, set[str]]] | None:
    """Ask Cargo on this machine which referenced tests it lists, and which of those are ignored."""
    listed: dict[str, set[str]] = {}
    ignored: dict[str, set[str]] = {}
    for crate in sorted(wanted):
        for ignored_only, destination in ((False, listed), (True, ignored)):
            command = list_command(crate, ignored_only)
            result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8")
            if result.returncode != 0:
                print(result.stderr)
                suffix = " --ignored" if ignored_only else ""
                print(f"::error::cargo test --list{suffix} failed for {crate}")
                return None
            destination[crate] = listed_tests(result.stdout)
    return listed, ignored


def union_listings(legs: list[dict]) -> tuple[dict[str, set[str]], dict[str, set[str]]]:
    """Merge the listings of several OS legs (TD-239).

    A test counts as listed when any leg lists it, and as ignored only when
    every leg that lists it ignores it, so a `cfg(windows)` test needs only
    the Windows leg and a test ignored on one platform alone is not held
    against the registry.
    """
    listed: dict[str, set[str]] = defaultdict(set)
    ignored: dict[str, set[str]] = {}
    for leg in legs:
        for crate, names in leg.get("listed", {}).items():
            listed[crate].update(names)
    for crate, names in listed.items():
        ignored[crate] = {
            name for name in names
            if all(name in set(leg.get("ignored", {}).get(crate, []))
                   for leg in legs if name in set(leg.get("listed", {}).get(crate, [])))
        }
    return dict(listed), ignored


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emit-listing", metavar="PATH",
                        help="write this machine's Cargo listing as JSON and exit without judging")
    parser.add_argument("--union", nargs="+", metavar="PATH",
                        help="judge the registry against the merged listings of these JSON files")
    args = parser.parse_args(argv)
    with open("features.toml", "rb") as fh:
        wanted = referenced_tests(tomllib.load(fh))
    if args.union:
        legs = []
        for path in args.union:
            with open(path, encoding="utf-8") as fh:
                legs.append(json.load(fh))
        listed, ignored = union_listings(legs)
    else:
        listings = cargo_listings(wanted)
        if listings is None:
            return 1
        listed, ignored = listings
        if args.emit_listing:
            with open(args.emit_listing, "w", encoding="utf-8") as fh:
                json.dump({"listed": {crate: sorted(names) for crate, names in listed.items()},
                           "ignored": {crate: sorted(names) for crate, names in ignored.items()}},
                          fh, indent=1, sort_keys=True)
            print(f"wrote the listing of {sum(len(n) for n in listed.values())} tests to {args.emit_listing}")
            return 0
    gone = missing(wanted, listed, ignored)
    if gone:
        print("::error::features.toml names tests that Cargo does not run:\n" + "\n".join(gone))
        return 1
    total = sum(len(names) for names in wanted.values())
    print(f"all {total} referenced tests are compiled and listed by Cargo")
    return 0


if __name__ == "__main__":
    sys.exit(main())
