#!/usr/bin/env python3
"""Publish the workspace's crates.io crates in dependency order (ADR 0016).

The published crates share one version. They are published one at a time,
each after the crates it depends on, and a crate whose version crates.io
already has is skipped, so a release whose publishing stopped half way can
be run again. ``cargo publish`` reads its token from
``CARGO_REGISTRY_TOKEN``: release.yml gets a short-lived one through trusted
publishing; the first release (0.1.0) used a maintainer's temporary token.

    python .github/scripts/publish_crates.py --version 0.1.0 [--dry-run]

``--dry-run`` prints the plan and asks crates.io which versions exist, and
publishes nothing. The pure functions are unit-tested in
``test_publish_crates.py``.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import urllib.error
import urllib.request

USER_AGENT = "secs2r-release (https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R)"


def publishable(metadata: dict) -> dict[str, dict]:
    """Workspace members cargo may publish to crates.io, by name.

    ``publish`` is null when any registry is allowed and a list of registry
    names otherwise (``publish = false`` is the empty list).
    """
    members = set(metadata["workspace_members"])
    out = {}
    for package in metadata["packages"]:
        if package["id"] not in members:
            continue
        registries = package.get("publish")
        if registries is None or "crates-io" in registries:
            out[package["name"]] = package
    return out


def publish_order(packages: dict[str, dict]) -> list[str]:
    """Names in an order where every crate follows the published crates it
    depends on (normal and build dependencies; dev-dependencies are stripped
    when publishing). Ties go by name, so the order is stable."""
    needs: dict[str, set[str]] = {}
    for name, package in packages.items():
        needs[name] = {
            dep["name"] for dep in package["dependencies"]
            if dep["name"] in packages and dep.get("kind") != "dev"
        }
    order: list[str] = []
    ready = sorted(name for name, deps in needs.items() if not deps)
    while ready:
        name = ready.pop(0)
        order.append(name)
        for other, deps in needs.items():
            if name in deps:
                deps.discard(name)
                if not deps and other not in order and other not in ready:
                    ready.append(other)
                    ready.sort()
    if len(order) != len(packages):
        cycle = sorted(set(packages) - set(order))
        raise SystemExit(f"dependency cycle among published crates: {cycle}")
    return order


def check_versions(packages: dict[str, dict], version: str) -> None:
    """Every published crate must carry the release's version."""
    wrong = {name: p["version"] for name, p in packages.items() if p["version"] != version}
    if wrong:
        raise SystemExit(f"published crates must all be at {version}: {wrong}")


def crate_version_exists(name: str, version: str) -> bool:
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}/{version}",
        headers={"User-Agent": USER_AGENT},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status == 200
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        raise


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--version", required=True, help="the version every published crate carries")
    parser.add_argument("--dry-run", action="store_true", help="print the plan, publish nothing")
    args = parser.parse_args(argv)

    metadata = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        check=True, capture_output=True, text=True, encoding="utf-8").stdout)
    packages = publishable(metadata)
    check_versions(packages, args.version)
    order = publish_order(packages)
    print(f"{len(order)} crates at {args.version}: {', '.join(order)}")
    for name in order:
        if crate_version_exists(name, args.version):
            print(f"{name} {args.version}: already on crates.io, skipped")
            continue
        if args.dry_run:
            print(f"{name} {args.version}: would publish")
            continue
        print(f"{name} {args.version}: publishing", flush=True)
        subprocess.run(["cargo", "publish", "-p", name, "--locked"], check=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
