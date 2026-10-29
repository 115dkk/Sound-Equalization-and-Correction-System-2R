#!/usr/bin/env python3
"""Give the crates.io crates a pre-release version before ``cargo publish --dry-run``.

The dry run checks that every published crate builds from its packaged files
against the packages of the others (docs/adr/0016). Once a version is on
crates.io, a crate whose files changed since then still carries that version
until the release gate bumps it, and Cargo refuses to verify a local package
whose name and version crates.io already has with another checksum. This
script rewrites, in the working tree only, every published crate's version to
``<version>-dryrun`` (which no registry has), with the workspace's exact
``=`` requirements, other members' exact requirements and the lock entries to
match. CI runs it on a disposable checkout right before the dry run; run it
locally only on a tree you will reset afterwards.

    python3 .github/scripts/api_dry_run.py
    cargo publish --workspace --dry-run --locked --allow-dirty
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import release_gate as gate  # noqa: E402

SUFFIX = "dryrun"
MEMBER_GLOBS = ("crates/*/Cargo.toml", "apps/*/Cargo.toml", "bindings/*/Cargo.toml")


def dry_run_version(version: str) -> str:
    """``0.1.0`` becomes ``0.1.0-dryrun``; a version that already has a
    pre-release part is rejected, so the result never equals a real one."""
    if "-" in version or "+" in version:
        raise SystemExit(f"API version {version} already has a pre-release or build part")
    return f"{version}-{SUFFIX}"


def set_exact_dependencies(manifest: str, old: str, new: str) -> str:
    """Rewrite ``"=old"`` requirements in every dependency table of a member manifest."""
    lines = manifest.splitlines(keepends=True)
    section = None
    for i, line in enumerate(lines):
        header = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
        if header:
            section = header.group(1).strip()
            continue
        if section is not None and section.split(".")[-1].endswith("dependencies"):
            lines[i] = line.replace(f'"={old}"', f'"={new}"')
    return "".join(lines)


def main() -> int:
    root = Path.cwd()
    old = gate.package_field(gate._read(gate.API_MANIFEST), "version")
    new = dry_run_version(old)
    published = {
        gate.package_field(gate._read(m), "name"): m
        for m in sorted(Path("crates").glob("*/Cargo.toml"))
        if gate.manifest_publishes(gate._read(m))
    }
    for manifest in published.values():
        gate._write(manifest, gate.set_package_version(gate._read(manifest), new))
    for pattern in MEMBER_GLOBS:
        for manifest in sorted(root.glob(pattern)):
            text = gate._read(manifest)
            updated = set_exact_dependencies(text, old, new)
            if updated != text:
                gate._write(manifest, updated)
    cargo_toml = gate._read(gate.CARGO_TOML)
    gate._write(gate.CARGO_TOML, gate.set_exact_requirements(cargo_toml, set(published), old, new))
    gate._write(gate.CARGO_LOCK, gate.set_member_versions(gate._read(gate.CARGO_LOCK), set(published), new))
    print(f"{len(published)} published crates set to {new} for the dry run")
    return 0


if __name__ == "__main__":
    sys.exit(main())
