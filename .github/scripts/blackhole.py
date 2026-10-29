#!/usr/bin/env python3
"""Install pinned BlackHole and grant app microphone consent on disposable macOS CI runners."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
import tomllib
import urllib.request
from pathlib import Path

DEFAULT_MANIFEST = Path(__file__).resolve().parent.parent / "blackhole.toml"
DEVICE = "BlackHole 2ch"
TCC_PATH = Path("Library/Application Support/com.apple.TCC/TCC.db")


class BlackHoleError(RuntimeError):
    """A pin, installation, device discovery or microphone-consent failure."""


def load_manifest(path: Path) -> dict[str, str]:
    with path.open("rb") as stream:
        entry = tomllib.load(stream).get("blackhole", {})
    required = {"version", "url", "sha256", "format"}
    if not isinstance(entry, dict) or any(
        not isinstance(entry.get(key), str) or not entry[key] for key in required
    ):
        raise BlackHoleError("BlackHole pin needs version, url, sha256 and format strings")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", entry["version"]):
        raise BlackHoleError("BlackHole version must be a pinned numeric release")
    if not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]):
        raise BlackHoleError("BlackHole sha256 must be 64 lowercase hex digits")
    if entry["format"] != "pkg":
        raise BlackHoleError("BlackHole format must be pkg")
    expected = f"https://existential.audio/downloads/BlackHole2ch-{entry['version']}.pkg"
    if entry["url"] != expected:
        raise BlackHoleError("BlackHole URL must name the pinned official 2ch installer")
    return {key: entry[key] for key in required}


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def fetch_file(url: str, expected: str, target: Path, opener=urllib.request.urlopen,
               sleeper=time.sleep) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_file() and sha256(target) == expected:
        return
    temporary = target.with_suffix(".pkg.part")
    request = urllib.request.Request(url, headers={"User-Agent": "secs2r-ci"})
    for attempt in range(4):
        try:
            with opener(request, timeout=60) as response, temporary.open("wb") as stream:
                shutil.copyfileobj(response, stream)
            actual = sha256(temporary)
            if actual != expected:
                raise BlackHoleError(f"BlackHole checksum mismatch: expected {expected}, got {actual}")
            os.replace(temporary, target)
            return
        except BlackHoleError:
            raise
        except OSError as error:
            if attempt == 3:
                raise BlackHoleError(f"BlackHole download failed after 4 attempts: {error}") from error
            sleeper(2 ** (attempt + 1))
        finally:
            temporary.unlink(missing_ok=True)


def has_device(value: object) -> bool:
    """system_profiler nests endpoints under SPAudioDataType / _items."""
    if isinstance(value, dict):
        return value.get("_name") == DEVICE or any(has_device(item) for item in value.values())
    if isinstance(value, list):
        return any(has_device(item) for item in value)
    return False


def run(command: list[str]) -> str:
    print("$ " + " ".join(command), flush=True)
    result = subprocess.run(command, text=True, capture_output=True, timeout=120)
    if result.stderr.strip():
        print(result.stderr.rstrip(), flush=True)
    if result.returncode:
        raise BlackHoleError(f"{command[0]} failed ({result.returncode}): {result.stdout}{result.stderr}")
    return result.stdout


def install(destination: Path, manifest: Path) -> None:
    entry = load_manifest(manifest)
    package = destination / f"BlackHole2ch-{entry['version']}.pkg"
    fetch_file(entry["url"], entry["sha256"], package)
    print(f"BlackHole pin: {json.dumps(entry, sort_keys=True)}", flush=True)
    print(run(["sudo", "installer", "-pkg", str(package), "-target", "/"]))
    # BlackHole's README documents this restart after driver installation.
    run(["sudo", "killall", "-9", "coreaudiod"])
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        devices = json.loads(run(["system_profiler", "SPAudioDataType", "-json"]))
        if has_device(devices):
            print(f"{DEVICE} is available", flush=True)
            return
        time.sleep(2)
    raise BlackHoleError(f"{DEVICE} did not appear within 60 seconds after restarting CoreAudio")


def sql_value(value: object) -> str:
    if value is None:
        return "NULL"
    if isinstance(value, int):
        return str(value)
    return "'" + str(value).replace("'", "''") + "'"


def consent_sql(columns: list[dict], bundle_id: str, now: int) -> str:
    """Use named, inspected columns, never an image-version-specific positional row.

    Matches runner-images' configure-tccdb-macos.sh microphone grants, but for
    the app bundle (client_type=0), not its runner ancestor (client_type=1).
    No camera, Accessibility, screen recording, or Full Disk Access is granted.
    """
    values = {
        "service": "kTCCServiceMicrophone", "client": bundle_id, "client_type": 0,
        "auth_value": 2, "auth_reason": 4, "auth_version": 1, "csreq": None,
        "policy_id": None, "indirect_object_identifier_type": 0,
        "indirect_object_identifier": "UNUSED", "indirect_object_code_identity": None,
        "flags": 0, "last_modified": now, "pid": None, "pid_version": None,
        "boot_uuid": "UNUSED", "last_reminded": now,
    }
    names = {column["name"] for column in columns}
    required = {"service", "client", "client_type", "auth_value", "auth_reason", "auth_version",
                "indirect_object_identifier", "last_modified"}
    missing = required - names
    if missing:
        raise BlackHoleError(f"Unsupported TCC schema; missing columns: {sorted(missing)}")
    for column in columns:
        if column["name"] not in values and column["notnull"] and column["dflt_value"] is None:
            raise BlackHoleError(f"Unsupported required TCC column: {column['name']}")
    selected = {key: value for key, value in values.items() if key in names}
    fields = ", ".join(f'"{name}"' for name in selected)
    literals = ", ".join(sql_value(value) for value in selected.values())
    return f"INSERT OR REPLACE INTO access ({fields}) VALUES ({literals});"


def grant_microphone(bundle_id: str) -> None:
    # Hosted images pre-authorize the runner, not arbitrary bundles launched by
    # Launch Services (`open`). See runner-images' build/configure-tccdb-macos.sh
    # and helpers/utils.sh. Direct DB access is for these disposable VMs only.
    if not re.fullmatch(r"[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)+", bundle_id):
        raise BlackHoleError("Microphone consent needs an app bundle identifier")
    now = int(time.time())
    for database in (Path("/") / TCC_PATH, Path.home() / TCC_PATH):
        if not database.is_file():
            raise BlackHoleError(f"TCC database is missing: {database}")
        # As runner-images' helpers: sudo for the system store, the logged-in
        # user for its own store (do not leave root-owned user WAL files).
        prefix = (["sudo"] if database == Path("/") / TCC_PATH else []) + [
            "sqlite3", "-bail", "-cmd", ".timeout 5000", "-json", str(database)
        ]
        try:
            columns = json.loads(run(prefix + ["PRAGMA table_info(access);"]))
            statement = consent_sql(columns, bundle_id, now)
            print(f"TCC columns in {database}: {[column['name'] for column in columns]}")
            run(prefix + [statement])
            rows = json.loads(run(prefix + [
                "SELECT service, client, client_type, auth_value, auth_reason, auth_version "
                "FROM access WHERE service='kTCCServiceMicrophone' "
                f"AND client={sql_value(bundle_id)} AND client_type=0;"
            ]) or "[]")
            if len(rows) != 1 or rows[0]["auth_value"] != 2:
                raise BlackHoleError(f"Microphone consent readback failed in {database}: {rows}")
            print(f"Microphone consent in {database}: {json.dumps(rows)}", flush=True)
        except (BlackHoleError, json.JSONDecodeError) as error:
            raise BlackHoleError(
                f"Cannot grant microphone access to {bundle_id} in {database}: {error}. "
                "The audio check cannot run without consent; it will not be skipped."
            ) from error
    # Discard cached decisions before Launch Services opens the app.
    run(["sudo", "killall", "tccd"])


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    command = commands.add_parser("install", help="install only on a disposable GitHub macOS runner")
    command.add_argument("--dest", type=Path, required=True, help="private download folder, not an artifact")
    command.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    command = commands.add_parser("grant-microphone", help="authorize only the shipped app's microphone")
    command.add_argument("--bundle-id", required=True)
    args = parser.parse_args(argv)
    if sys.platform != "darwin" or os.environ.get("GITHUB_ACTIONS") != "true":
        parser.error("this helper changes only disposable GitHub Actions macOS runners")
    try:
        if args.command == "install":
            install(args.dest.resolve(), args.manifest)
        else:
            grant_microphone(args.bundle_id)
    except (BlackHoleError, OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
