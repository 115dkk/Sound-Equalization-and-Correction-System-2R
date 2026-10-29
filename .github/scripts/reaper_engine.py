#!/usr/bin/env python3
"""Fetch the pinned Linux REAPER for an isolated CI playback test; never run it."""
from __future__ import annotations

import argparse
import hashlib
import os
import re
import shutil
import sys
import tarfile
import time
import tomllib
import urllib.parse
import urllib.request
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Callable

DEFAULT_MANIFEST = Path(__file__).resolve().parent.parent / "reaper.toml"


class EngineError(RuntimeError):
    """A manifest, download or confined-extraction failure."""


def load_manifest(path: Path) -> dict[str, str]:
    with path.open("rb") as stream:
        entry = tomllib.load(stream)
    required = {"version", "url", "sha256", "format"}
    if not required <= entry.keys():
        raise EngineError("manifest requires version, url, sha256 and format")
    if any(not isinstance(entry[key], str) or not entry[key] for key in required):
        raise EngineError("manifest values must be nonempty strings")
    if not re.fullmatch(r"\d+\.\d+", entry["version"]):
        raise EngineError("version must be major.minor")
    url = urllib.parse.urlsplit(entry["url"])
    if (url.scheme != "https" or url.netloc not in {"reaper.fm", "www.reaper.fm"}
            or url.query or url.fragment
            or not re.fullmatch(r"/files/\d+\.x/reaper\d+_linux_x86_64\.tar\.xz", url.path)):
        raise EngineError("url must be an official reaper.fm Linux x86_64 HTTPS archive")
    if not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]):
        raise EngineError("sha256 must be 64 lowercase hex digits")
    if entry["format"] != "tar.xz":
        raise EngineError("format must be tar.xz")
    return {key: entry[key] for key in required}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fetch_file(
    url: str, expected: str, target: Path,
    opener: Callable[..., object] = urllib.request.urlopen,
    sleeper: Callable[[float], None] = time.sleep,
) -> bool:
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_file() and sha256(target) == expected:
        return False
    temporary = target.with_name(f".{target.name}.part")
    request = urllib.request.Request(url, headers={"User-Agent": "secs2r-ci"})
    last_error: Exception | None = None
    for attempt in range(4):
        try:
            with opener(request, timeout=60) as response, temporary.open("wb") as output:
                shutil.copyfileobj(response, output)
            actual = sha256(temporary)
            if actual != expected:
                raise EngineError(f"checksum mismatch: expected {expected} got {actual}")
            os.replace(temporary, target)
            return True
        except EngineError:
            temporary.unlink(missing_ok=True)
            raise
        except Exception as error:  # urllib exposes several unrelated failure types.
            last_error = error
            temporary.unlink(missing_ok=True)
            if attempt < 3:
                sleeper(2 ** (attempt + 1))
    raise EngineError(f"download failed after 4 attempts: {last_error}")


def extract_package(package: Path, destination: Path) -> None:
    """Extract regular files/directories only, into a new or empty real directory."""
    if destination.is_symlink() or (destination.exists() and any(destination.iterdir())):
        raise EngineError("extraction destination must be an empty, non-symlink directory")
    destination.mkdir(parents=True, exist_ok=True)
    root = destination.resolve()
    with tarfile.open(package, mode="r:xz") as archive:
        members = archive.getmembers()
        if len(members) > 50_000 or sum(member.size for member in members) > 1024 ** 3:
            raise EngineError("archive exceeds extraction limits")
        seen = set()
        # Validate every member before writing any of them. Do not use extractall:
        # no links, devices, platform-specific absolute paths or special modes.
        for member in members:
            path = PurePosixPath(member.name)
            windows = PureWindowsPath(member.name)
            if (path.is_absolute() or windows.drive or ".." in path.parts
                    or "\\" in member.name or ":" in member.name or not path.parts
                    or not (member.isfile() or member.isdir())):
                raise EngineError(f"unsafe archive member: {member.name}")
            output = (root / Path(*path.parts)).resolve()
            if not output.is_relative_to(root) or output == root or output in seen:
                raise EngineError(f"unsafe or duplicate archive path: {member.name}")
            seen.add(output)
        for member in members:
            output = root / Path(*PurePosixPath(member.name).parts)
            if member.isdir():
                output.mkdir(parents=True, exist_ok=True)
            else:
                output.parent.mkdir(parents=True, exist_ok=True)
                with archive.extractfile(member) as source, output.open("xb") as target:
                    shutil.copyfileobj(source, target)
                output.chmod(0o755 if member.mode & 0o111 else 0o644)


def find_reaper(root: Path) -> Path:
    candidates = sorted(path.resolve() for path in root.rglob("reaper") if path.is_file())
    if len(candidates) != 1:
        raise EngineError(f"expected exactly one reaper executable, found {len(candidates)}")
    return candidates[0]


def write_github_env(path: Path, executable: Path) -> None:
    value = str(executable.resolve())
    if any(ch in value for ch in "\r\n"):
        raise EngineError("executable path must not contain line breaks")
    with path.open("a", encoding="utf-8", newline="\n") as stream:
        stream.write(f"SECS_REAPER={value}\n")


def fetch(args: argparse.Namespace) -> None:
    entry = load_manifest(Path(args.manifest))
    destination = Path(args.dest).absolute()
    if destination.is_symlink():
        raise EngineError("destination must not be a symlink")
    destination = destination.resolve()
    package = destination / "download" / f"reaper-{entry['version']}-{entry['sha256']}.tar.xz"
    fetch_file(entry["url"], entry["sha256"], package)
    engine = destination / "engine"
    extract_package(package, engine)
    executable = find_reaper(engine)
    if args.github_env:
        write_github_env(Path(args.github_env), executable)
    print(f"version: {entry['version']}")
    print(f"REAPER: {executable}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    command = commands.add_parser("fetch")
    command.add_argument("--manifest", default=str(DEFAULT_MANIFEST))
    command.add_argument("--dest", required=True)
    command.add_argument("--github-env")
    args = parser.parse_args(argv)
    try:
        fetch(args)
    except (EngineError, OSError, ValueError, tomllib.TOMLDecodeError, tarfile.TarError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
