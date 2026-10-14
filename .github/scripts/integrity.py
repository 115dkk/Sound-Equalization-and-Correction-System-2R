#!/usr/bin/env python3
"""The same input must give the same bytes whatever the thread count.

ADR 0007 keeps parallel work to maps and every floating-point accumulation in
its original order, so a design, a MIMO run and their exports do not depend on
how many threads Rayon uses. ci.yml's `integrity` job runs the CLI's mock flow
(`secs2r-cli run`) once per thread count into its own folder and then calls

    integrity.py compare <reference-folder> <folder>...

which hashes every file under each folder (SHA-256, by relative path) and fails
when a folder has a file the reference lacks, lacks one it has, or holds
different bytes. The pure functions are unit-tested in ``test_integrity.py``.
"""
from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path

CHUNK = 1 << 20


def file_hash(path: Path) -> str:
    """SHA-256 of one file, read in chunks so large exports stay out of memory."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(CHUNK):
            digest.update(chunk)
    return digest.hexdigest()


def tree_hashes(root: Path) -> dict[str, str]:
    """Relative POSIX path -> SHA-256 for every regular file under ``root``."""
    return {
        path.relative_to(root).as_posix(): file_hash(path)
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


def differences(reference: dict[str, str], other: dict[str, str]) -> list[str]:
    """One line per path that is missing, extra or different, sorted by path."""
    lines = []
    for path in sorted(reference.keys() | other.keys()):
        if path not in other:
            lines.append(f"missing  {path}")
        elif path not in reference:
            lines.append(f"extra    {path}")
        elif reference[path] != other[path]:
            lines.append(f"differs  {path}  {reference[path][:12]} != {other[path][:12]}")
    return lines


def compare(reference: Path, others: list[Path]) -> int:
    expected = tree_hashes(reference)
    if not expected:
        print(f"{reference}: no files; the run wrote nothing to compare", file=sys.stderr)
        return 1
    for path, digest in expected.items():
        print(f"{digest}  {path}")
    failed = False
    for other in others:
        lines = differences(expected, tree_hashes(other))
        if lines:
            failed = True
            print(f"\n{other} differs from {reference}:")
            for line in lines:
                print(f"  {line}")
        else:
            print(f"\n{other}: {len(expected)} files identical to {reference}")
    return 1 if failed else 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    compare_parser = commands.add_parser("compare", help="compare folders byte for byte")
    compare_parser.add_argument("reference", type=Path)
    compare_parser.add_argument("others", type=Path, nargs="+")
    args = parser.parse_args(argv)
    return compare(args.reference, args.others)


if __name__ == "__main__":
    sys.exit(main())
