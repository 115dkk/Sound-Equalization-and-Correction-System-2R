#!/usr/bin/env python3
"""Fetch a pinned CamillaDSP binary for playback tests."""
from __future__ import annotations

import argparse
import os
import shutil
import stat
import tarfile
import tomllib
import zipfile
from pathlib import Path, PurePosixPath, PureWindowsPath

from eqapo_engine import EngineError, fetch_file, load_manifest

DEFAULT_MANIFEST = Path(__file__).resolve().parent.parent / "camilladsp-engines.toml"
FORMATS = {"tar.gz", "zip"}


def safe_member(name: str) -> PurePosixPath:
    normalized = name.replace("\\", "/")
    path = PurePosixPath(normalized)
    windows_path = PureWindowsPath(name)
    if (
        path.is_absolute()
        or windows_path.is_absolute()
        or windows_path.drive
        or ".." in path.parts
    ):
        raise EngineError(f"unsafe archive path: {name}")
    return path


def select_name(path: PurePosixPath) -> bool:
    return path.name.lower() in {"camilladsp", "camilladsp.exe"}


def extract_tar(package: Path, destination: Path) -> Path:
    candidates: list[tarfile.TarInfo] = []
    with tarfile.open(package, "r:gz") as archive:
        for member in archive.getmembers():
            path = safe_member(member.name)
            if member.issym() or member.islnk():
                raise EngineError(f"archive link is not allowed: {member.name}")
            if member.isfile() and select_name(path):
                candidates.append(member)
        if len(candidates) != 1:
            raise EngineError(
                f"expected exactly one camilladsp binary, found {len(candidates)}"
            )
        destination.mkdir(parents=True, exist_ok=True)
        output = destination / "camilladsp"
        source = archive.extractfile(candidates[0])
        if source is None:
            raise EngineError("camilladsp archive member has no file data")
        with source, output.open("wb") as target:
            shutil.copyfileobj(source, target)
    output.chmod(output.stat().st_mode | 0o111)
    return output.resolve()


def extract_zip(package: Path, destination: Path) -> Path:
    candidates: list[zipfile.ZipInfo] = []
    with zipfile.ZipFile(package) as archive:
        for member in archive.infolist():
            path = safe_member(member.filename)
            mode = member.external_attr >> 16
            if stat.S_ISLNK(mode):
                raise EngineError(f"archive link is not allowed: {member.filename}")
            if not member.is_dir() and select_name(path):
                candidates.append(member)
        if len(candidates) != 1:
            raise EngineError(
                f"expected exactly one camilladsp binary, found {len(candidates)}"
            )
        destination.mkdir(parents=True, exist_ok=True)
        output = destination / "camilladsp.exe"
        with archive.open(candidates[0]) as source, output.open("wb") as target:
            shutil.copyfileobj(source, target)
    output.chmod(output.stat().st_mode | 0o111)
    return output.resolve()


def extract_package(package: Path, package_format: str, destination: Path) -> Path:
    if destination.exists() and any(destination.iterdir()):
        raise EngineError(f"extraction destination is not empty: {destination}")
    if package_format == "tar.gz":
        return extract_tar(package, destination)
    return extract_zip(package, destination)


def write_github_env(path: Path, binary: Path, version: str) -> None:
    with path.open("a", encoding="utf-8", newline="\n") as stream:
        stream.write(f"SECS_CAMILLADSP={binary.resolve()}\n")
        stream.write(f"SECS_CAMILLADSP_VERSION={version}\n")


def fetch(args: argparse.Namespace) -> None:
    manifest = Path(args.manifest).resolve()
    destination = Path(args.dest).resolve()
    entry = load_manifest(manifest, args.engine, FORMATS)
    extension = ".tar.gz" if entry["format"] == "tar.gz" else ".zip"
    package = destination / "download" / (
        f"{args.engine}-{entry['version']}-{entry['sha256']}{extension}"
    )
    fetch_file(entry["url"], entry["sha256"], package)
    binary = extract_package(package, entry["format"], destination / "engine")
    if args.github_env:
        write_github_env(Path(args.github_env), binary, entry["version"])
    print(binary)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(
        description="Fetch a pinned CamillaDSP engine for disposable CI runners."
    )
    subparsers = result.add_subparsers(dest="command", required=True)
    command = subparsers.add_parser("fetch")
    command.add_argument("--engine", required=True, choices=("linux", "windows"))
    command.add_argument("--dest", required=True)
    command.add_argument("--manifest", default=str(DEFAULT_MANIFEST))
    command.add_argument("--github-env")
    command.set_defaults(function=fetch)
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        args.function(args)
    except (
        EngineError,
        OSError,
        tarfile.TarError,
        tomllib.TOMLDecodeError,
        zipfile.BadZipFile,
    ) as error:
        print(f"error: {error}", file=os.sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
