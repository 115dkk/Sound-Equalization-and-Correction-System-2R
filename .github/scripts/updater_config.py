#!/usr/bin/env python3
"""Print the `tauri build --config` overlay for the app and the host shell.

Kept out of tauri.conf.json on purpose (ADR 0020, ADR 0018):

* ``--windows-resources`` puts the CLI, the host service, the host shell and
  the service's signed manifest next to "SECS 2R.exe" in the installer, so a
  local ``tauri dev`` or ``tauri build`` needs none of them.
* ``--pubkey-env NAME`` replaces ``plugins.updater.pubkey`` with the public
  key in the environment variable NAME when it is set and not empty. ci.yml
  signs each pull request run with a key made for that run; release.yml never
  passes this, so a release always carries the key in tauri.conf.json.

The pure function is unit-tested in ``test_updater_config.py``.
"""
from __future__ import annotations

import argparse
import json
import os
import sys

WINDOWS_RESOURCES = {
    "../../target/release/secs2r-cli.exe": "secs2r-cli.exe",
    "../../target/release/secs2r-host.exe": "secs2r-host.exe",
    "../../target/release/secs2r-host-shell.exe": "secs2r-host-shell.exe",
    "../../target/release/secs2r-host.json": "secs2r-host.json",
    "../../target/release/secs2r-host.json.sig": "secs2r-host.json.sig",
}


def overlay(windows_resources: bool, pubkey: str | None) -> dict:
    config: dict = {}
    if windows_resources:
        config["bundle"] = {"resources": dict(WINDOWS_RESOURCES)}
    if pubkey:
        config["plugins"] = {"updater": {"pubkey": pubkey}}
    return config


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--windows-resources", action="store_true",
                        help="ship the CLI, the host service, the shell and the service manifest")
    parser.add_argument("--pubkey-env", metavar="NAME",
                        help="take the updater public key from this environment variable")
    args = parser.parse_args(argv)
    pubkey = os.environ.get(args.pubkey_env, "").strip() if args.pubkey_env else ""
    print(json.dumps(overlay(args.windows_resources, pubkey or None), separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
