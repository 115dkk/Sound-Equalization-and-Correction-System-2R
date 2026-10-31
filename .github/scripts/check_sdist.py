#!/usr/bin/env python3
"""Check the secs2r source distribution before it can reach PyPI.

The sdist carries the AGPL-3.0-only licence of the package (docs/adr/0019,
0023): its PKG-INFO must say so, and bindings/python/LICENSE must be in it.
ci.yml runs this on the sdist the Linux wheel job builds, the same way
release.yml builds the one it publishes.

    python .github/scripts/check_sdist.py bindings/python/dist/secs2r-*.tar.gz
"""
from __future__ import annotations

import sys
import tarfile

LICENSE_EXPRESSION = "AGPL-3.0-only"


def problems(path: str) -> list[str]:
    """What is wrong with the sdist at ``path``; empty when it passes."""
    with tarfile.open(path) as sdist:
        names = sdist.getnames()
        if not names:
            return ["the sdist is empty"]
        root = names[0].split("/")[0]
        found = []
        if f"{root}/LICENSE" not in names:
            found.append(f"{root}/LICENSE is missing")
        try:
            info = sdist.extractfile(f"{root}/PKG-INFO").read().decode("utf-8")
        except KeyError:
            return found + [f"{root}/PKG-INFO is missing"]
    if f"License-Expression: {LICENSE_EXPRESSION}" not in info.splitlines():
        found.append(f"PKG-INFO does not declare License-Expression: {LICENSE_EXPRESSION}")
    if "License-File: LICENSE" not in info.splitlines():
        found.append("PKG-INFO does not list License-File: LICENSE")
    return found


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print("usage: check_sdist.py <secs2r-*.tar.gz>", file=sys.stderr)
        return 2
    found = problems(argv[0])
    for problem in found:
        print(f"::error::{problem}")
    if not found:
        print(f"{argv[0]}: {LICENSE_EXPRESSION} with LICENSE")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
