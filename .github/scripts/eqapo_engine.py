#!/usr/bin/env python3
"""Fetch a pinned Equalizer APO Benchmark.exe for playback tests."""
from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import time
import tomllib
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Callable

DEFAULT_MANIFEST = Path(__file__).resolve().parent.parent / "eqapo-engines.toml"
FORMATS = {"nupkg", "nsis"}
# The real GUI-management check installs this exact SSE2 release, not the
# auto-detect bootstrapper (which could select another architecture/version).
# Verified against v2.60.4's release asset digest on 2026-10-29.
XT_SETUP_VERSION = "2.60.4"
XT_SETUP_URL = (
    "https://github.com/115dkk/EqualizerAPO-XT/releases/download/v2.60.4/"
    "EqualizerAPO-XT-x64-sse2-x64-sse2-Setup.exe"
)
XT_SETUP_SHA256 = "54c6f27b4e6b5c9ba9d41ad8395d2ccfefacf827c0344aef8b8ad8db7a09c6b3"


class EngineError(RuntimeError):
    """A manifest, download, extraction, or registration failure."""


def load_manifest(
    path: Path, engine: str, formats: set[str] = FORMATS
) -> dict[str, str]:
    with path.open("rb") as stream:
        manifest = tomllib.load(stream)
    if engine not in manifest or not isinstance(manifest[engine], dict):
        raise EngineError(f"engine {engine!r} is missing from {path}")
    entry = manifest[engine]
    required = {"version", "url", "sha256", "format"}
    missing = sorted(required - entry.keys())
    if missing:
        raise EngineError(f"engine {engine!r} is missing keys: {', '.join(missing)}")
    if any(not isinstance(entry[key], str) or not entry[key] for key in required):
        raise EngineError(f"engine {engine!r} keys must be nonempty strings")
    digest = entry["sha256"]
    if len(digest) != 64 or any(ch not in "0123456789abcdef" for ch in digest):
        raise EngineError(f"engine {engine!r} sha256 must be 64 lowercase hex digits")
    if entry["format"] not in formats:
        choices = " or ".join(sorted(formats))
        raise EngineError(f"engine {engine!r} format must be {choices}")
    return {key: entry[key] for key in required}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fetch_file(
    url: str,
    expected: str,
    target: Path,
    opener: Callable[..., object] = urllib.request.urlopen,
    sleeper: Callable[[float], None] = time.sleep,
) -> bool:
    """Fetch target atomically; return False when an existing cache was reused."""
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_file() and sha256(target) == expected:
        return False
    if target.exists():
        target.unlink()
    temporary = target.with_name(f".{target.name}.part")
    temporary.unlink(missing_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "secs2r-ci"})
    last_error: Exception | None = None
    for attempt in range(4):
        try:
            with opener(request) as response, temporary.open("wb") as output:
                shutil.copyfileobj(response, output)
            actual = sha256(temporary)
            if actual != expected:
                temporary.unlink(missing_ok=True)
                raise EngineError(
                    f"checksum mismatch: expected {expected} got {actual}"
                )
            os.replace(temporary, target)
            return True
        except EngineError:
            raise
        except Exception as error:  # urllib exposes several unrelated failure types.
            last_error = error
            temporary.unlink(missing_ok=True)
            if attempt == 3:
                break
            sleeper(2 ** (attempt + 1))
    raise EngineError(f"download failed after 4 attempts: {last_error}")


def extract_nupkg(package: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(package) as archive:
        for info in archive.infolist():
            normalized = info.filename.replace("\\", "/")
            path = PurePosixPath(normalized)
            windows_path = PureWindowsPath(info.filename)
            if (
                path.is_absolute()
                or windows_path.is_absolute()
                or windows_path.drive
                or ".." in path.parts
            ):
                raise EngineError(f"unsafe nupkg path: {info.filename}")
            if len(path.parts) < 3 or tuple(part.lower() for part in path.parts[:2]) != (
                "lib",
                "app",
            ):
                continue
            relative = Path(*path.parts[2:])
            if not relative.parts:
                continue
            output = destination / relative
            if info.is_dir():
                output.mkdir(parents=True, exist_ok=True)
            else:
                output.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(info) as source, output.open("wb") as target:
                    shutil.copyfileobj(source, target)


def seven_zip() -> str:
    found = shutil.which("7z")
    if found:
        return found
    fallback = Path(r"C:\Program Files\7-Zip\7z.exe")
    if fallback.is_file():
        return str(fallback)
    raise EngineError("7z was not found in PATH or C:\\Program Files\\7-Zip")


def extract_package(package: Path, package_format: str, destination: Path) -> None:
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    if package_format == "nupkg":
        extract_nupkg(package, destination)
    else:
        result = subprocess.run(
            [seven_zip(), "x", "-y", f"-o{destination}", str(package)],
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode:
            raise EngineError(
                f"7z extraction failed ({result.returncode}):\n{result.stdout}{result.stderr}"
            )


def find_benchmark(root: Path) -> Path:
    candidates = sorted(
        path.resolve()
        for path in root.rglob("*")
        if path.is_file() and path.name.lower() == "benchmark.exe"
    )
    if len(candidates) != 1:
        listing = "\n".join(f"  {path}" for path in candidates) or "  (none)"
        raise EngineError(
            f"expected exactly one Benchmark.exe under {root}, found {len(candidates)}:\n{listing}"
        )
    return candidates[0]


def register_config(config: Path) -> None:
    if sys.platform != "win32":
        raise EngineError("--register requires Windows")
    import winreg  # pylint: disable=import-outside-toplevel

    config.mkdir(parents=True, exist_ok=True)
    access = winreg.KEY_SET_VALUE | winreg.KEY_WOW64_64KEY
    with winreg.CreateKeyEx(
        winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\EqualizerAPO", 0, access
    ) as key:
        winreg.SetValueEx(key, "ConfigPath", 0, winreg.REG_SZ, str(config.resolve()))


def xt_registered_paths() -> tuple[Path, Path]:
    """Read what XT's installer registered; never synthesize an installation."""
    import winreg

    with winreg.OpenKey(
        winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\EqualizerAPO", 0,
        winreg.KEY_READ | winreg.KEY_WOW64_64KEY,
    ) as key:
        install = Path(winreg.QueryValueEx(key, "InstallPath")[0])
        config = Path(winreg.QueryValueEx(key, "ConfigPath")[0])
    if not any(part.lower().startswith("equalizerapo-xt") for part in install.parts):
        raise EngineError(f"installer registered a non-XT InstallPath: {install}")
    if not install.is_absolute() or not config.is_absolute():
        raise EngineError("XT's registry paths are not absolute")
    if not (install / "EqualizerAPO.dll").is_file() or not (config / "config.txt").is_file():
        raise EngineError(f"XT installation is incomplete: {install}, {config}")
    return install, config


def install_xt(version: str, destination: Path) -> tuple[Path, Path]:
    """Run the vendor's silent installer only on a disposable Windows runner.

    Contract: EqualizerAPO-XT v2.60.4 .github/scripts/Install-ReleaseBuild.ps1
    uses Setup.exe --silent. Its services/install/ApoRegistration.cpp writes
    HKLM SOFTWARE\\EqualizerAPO InstallPath and ConfigPath. Missing registration
    is a failure, not a reason to replace the real install with a registry mock.
    """
    if (sys.platform != "win32" or os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"):
        raise EngineError("--install-xt requires a disposable GitHub-hosted Windows runner")
    if version != XT_SETUP_VERSION:
        raise EngineError("XT manifest changed: update the GUI installer's URL and SHA256 pin too")
    setup = destination / "setup-download" / "EqualizerAPO-XT-Setup.exe"
    fetch_file(XT_SETUP_URL, XT_SETUP_SHA256, setup)
    result = subprocess.run([str(setup), "--silent"], capture_output=True, text=True,
                            timeout=240, check=False)
    if result.returncode:
        raise EngineError(f"XT silent installer exited {result.returncode}: {result.stdout}{result.stderr}")
    deadline = time.monotonic() + 60
    while True:
        try:
            install, config = xt_registered_paths()
            return find_benchmark(install), config
        except (OSError, EngineError) as error:
            if time.monotonic() >= deadline:
                raise EngineError(f"XT silent installation did not register a usable configuration: {error}") from error
            time.sleep(0.5)


def write_github_env(
    path: Path, benchmark: Path, engine: str, config: Path | None
) -> None:
    with path.open("a", encoding="utf-8", newline="\n") as stream:
        stream.write(f"SECS_EQAPO_BENCHMARK={benchmark.resolve()}\n")
        stream.write(f"SECS_EQAPO_ENGINE={engine}\n")
        if config is not None:
            stream.write(f"SECS_EQAPO_CONFIG_DIR={config.resolve()}\n")


def fetch(args: argparse.Namespace) -> None:
    manifest = Path(args.manifest).resolve()
    destination = Path(args.dest).resolve()
    entry = load_manifest(manifest, args.engine)
    if args.install_xt:
        if args.engine != "xt" or args.register:
            raise EngineError("--install-xt requires --engine xt and cannot use --register")
        benchmark, config = install_xt(entry["version"], destination)
        if args.github_env:
            write_github_env(Path(args.github_env), benchmark, args.engine, config)
        print(f"XT {entry['version']} installed with Setup.exe --silent: {benchmark}; config: {config}")
        return
    extension = ".nupkg" if entry["format"] == "nupkg" else ".exe"
    package = destination / "download" / (
        f"{args.engine}-{entry['version']}-{entry['sha256']}{extension}"
    )
    fetch_file(entry["url"], entry["sha256"], package)
    engine_dir = destination / "engine"
    extract_package(package, entry["format"], engine_dir)
    benchmark = find_benchmark(engine_dir)
    config = None
    if args.register:
        if args.engine != "original":
            raise EngineError("--register is only meaningful for the original engine")
        config = destination / "config"
        register_config(config)
    if args.github_env:
        write_github_env(Path(args.github_env), benchmark, args.engine, config)
    print(f"engine: {args.engine}")
    print(f"version: {entry['version']}")
    print(f"Benchmark: {benchmark}")
    print(f"config: {config if config is not None else '(--config-path mode)'}")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(
        description="Fetch a pinned Equalizer APO engine for disposable CI runners."
    )
    subparsers = result.add_subparsers(dest="command", required=True)
    command = subparsers.add_parser("fetch")
    command.add_argument("--engine", required=True, choices=("xt", "original"))
    command.add_argument("--dest", required=True)
    command.add_argument("--manifest", default=str(DEFAULT_MANIFEST))
    command.add_argument(
        "--register",
        action="store_true",
        help=(
            "On Windows, set the 64-bit HKLM EqualizerAPO ConfigPath to DEST/config; "
            "intended only for disposable CI runners and the original engine."
        ),
    )
    command.add_argument(
        "--install-xt", action="store_true",
        help="Install the pinned XT Setup.exe silently on a disposable Windows CI runner; require its real registry values.",
    )
    command.add_argument("--github-env")
    command.set_defaults(function=fetch)
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        args.function(args)
    except (EngineError, OSError, subprocess.SubprocessError, tomllib.TOMLDecodeError, zipfile.BadZipFile) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
