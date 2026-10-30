#!/usr/bin/env python3
"""Create Tauri updater metadata and the Windows host update manifest.

The static updater platform keys follow tauri-plugin-updater 2.14.0: it looks
for ``{os}-{arch}-{installer}`` first, then ``{os}-{arch}``. Each signature in
``latest.json`` is the corresponding ``.sig`` file's text.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

PRODUCT = "SECS2R"
RFC3339 = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$"
)

# The suffix follows ``SECS2R-<version>``. One universal macOS updater archive
# serves both architectures, and AppImage is Linux's installer-neutral fallback.
UPDATER_ASSETS = (
    ("windows", "-windows-x64-setup.exe", (
        "windows-x86_64", "windows-x86_64-nsis"
    )),
    ("macos", "-macos-universal.app.tar.gz", (
        "darwin-aarch64", "darwin-x86_64", "darwin-aarch64-app", "darwin-x86_64-app"
    )),
    ("linux", "-linux-x86_64.AppImage", (
        "linux-x86_64", "linux-x86_64-appimage"
    )),
    ("linux", "-linux-amd64.deb", ("linux-x86_64-deb",)),
)


def validate_pub_date(value: str) -> None:
    """Require the RFC 3339 spelling parsed by tauri-plugin-updater."""
    if not RFC3339.fullmatch(value):
        raise SystemExit(f"pub-date is not RFC3339: {value}")
    try:
        datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise SystemExit(f"pub-date is not RFC3339: {value}") from error


def _required_asset(assets: Path, version: str, suffix: str) -> Path:
    expected = assets / f"{PRODUCT}-{version}{suffix}"
    if expected.is_file():
        return expected

    candidates = sorted(path for path in assets.glob(f"{PRODUCT}-*{suffix}") if path.is_file())
    if candidates:
        names = ", ".join(path.name for path in candidates)
        raise SystemExit(
            f"updater asset version does not match {version}: expected {expected.name}; found {names}"
        )
    raise SystemExit(f"required updater asset is missing: {expected}")


def latest_manifest(version: str, previous: str, notes: str, assets: Path,
                    base_url: str, pub_date: str, only: set[str] | None = None,
                    asset_version: str | None = None) -> dict[str, Any]:
    """Build one static Tauri updater manifest after checking every input."""
    validate_pub_date(pub_date)
    if not assets.is_dir():
        raise SystemExit(f"updater asset directory is missing: {assets}")

    selected = {"windows", "macos", "linux"} if only is None else only
    named_version = asset_version or version
    platforms: dict[str, dict[str, str]] = {}
    for group, suffix, keys in UPDATER_ASSETS:
        if group not in selected:
            continue
        asset = _required_asset(assets, named_version, suffix)
        signature = Path(f"{asset}.sig")
        if not signature.is_file():
            raise SystemExit(f"required updater signature is missing: {signature}")
        try:
            signature_text = signature.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as error:
            raise SystemExit(f"could not read updater signature {signature}: {error}") from error
        entry = {
            "signature": signature_text,
            "url": f"{base_url.rstrip('/')}/{asset.name}",
        }
        for key in keys:
            platforms[key] = entry.copy()

    manifest: dict[str, Any] = {
        "version": version,
        "notes": notes,
        "pub_date": pub_date,
        "platforms": platforms,
    }
    if previous:
        manifest["previous"] = previous
    return manifest


def sha256_file(path: Path) -> str:
    """Return the lowercase SHA-256 digest of a file."""
    digest = hashlib.sha256()
    try:
        with path.open("rb") as file:
            for chunk in iter(lambda: file.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError as error:
        raise SystemExit(f"could not read {path}: {error}") from error
    return digest.hexdigest()


def host_manifest(exe: Path, version: str) -> dict[str, str]:
    """Build the signed-host companion manifest."""
    return {
        "version": version,
        "sha256": sha256_file(exe),
        "file": "secs2r-host.exe",
    }


def write_json(path: Path, value: dict[str, Any]) -> None:
    """Write reproducible UTF-8 JSON with an LF trailing newline."""
    text = json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    path.write_bytes(text.encode("utf-8"))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    subparsers = parser.add_subparsers(dest="command", required=True)

    latest = subparsers.add_parser("latest", help="write latest.json for Tauri updater clients")
    latest.add_argument("--version", required=True)
    latest.add_argument("--asset-version", default=None,
                        help="version in asset file names (default: --version)")
    latest.add_argument("--only", action="append", choices=["windows", "macos", "linux"],
                        help="include one platform group (repeatable; default: all)")
    latest.add_argument("--previous", required=True)
    latest.add_argument("--notes-file", type=Path, required=True)
    latest.add_argument("--assets", type=Path, required=True)
    latest.add_argument("--base-url", required=True)
    latest.add_argument("--pub-date", required=True)
    latest.add_argument("--out", type=Path, required=True)

    host = subparsers.add_parser("host", help="write secs2r-host.json")
    host.add_argument("--exe", type=Path, required=True)
    host.add_argument("--version", required=True)
    host.add_argument("--out", type=Path, required=True)

    args = parser.parse_args(argv)
    if args.command == "latest":
        try:
            notes = args.notes_file.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as error:
            raise SystemExit(f"could not read notes file {args.notes_file}: {error}") from error
        write_json(
            args.out,
            latest_manifest(
                args.version,
                args.previous,
                notes,
                args.assets,
                args.base_url,
                args.pub_date,
                set(args.only) if args.only else None,
                args.asset_version,
            ),
        )
    else:
        write_json(args.out, host_manifest(args.exe, args.version))
    return 0


if __name__ == "__main__":
    sys.exit(main())
