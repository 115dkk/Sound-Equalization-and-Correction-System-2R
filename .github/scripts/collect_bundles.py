#!/usr/bin/env python3
"""Copy what `tauri build` produced into one folder under stable names.

Release asset names are an interface (download pages and, later, the
updater depend on them), so they are fixed here and not left to Tauri's
product-name-based file names:

    SECS2R-<version>-windows-x64-setup.exe
    SECS2R-<version>-windows-x64-portable.zip
    SECS2R-<version>-macos-universal.dmg
    SECS2R-<version>-linux-x86_64.AppImage
    SECS2R-<version>-linux-amd64.deb

On Windows the release exe must be a GUI-subsystem executable (no console
window behind the app); the check reads the PE header.

The pure functions are unit-tested in ``test_collect_bundles.py``.
"""
from __future__ import annotations

import argparse
import shutil
import struct
import sys
import zipfile
from pathlib import Path

PRODUCT = "SECS2R"
PE_SUBSYSTEM_WINDOWS_GUI = 2

PORTABLE_README = """SECS 2R portable

압축을 푼 폴더에서 "SECS 2R.exe"를 실행합니다. 설치가 필요 없고, 이 판은
자동 업데이트를 하지 않습니다. Windows 10/11에 포함된 Microsoft Edge
WebView2 런타임을 씁니다.

Unzip and run "SECS 2R.exe". Nothing is installed and this build does not
update itself. It uses the Microsoft Edge WebView2 runtime that ships with
Windows 10 and 11.

License: GPL-3.0-only (LICENSE.txt). Source:
https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R
"""


def asset_name(version: str, kind: str) -> str:
    names = {
        "nsis": f"{PRODUCT}-{version}-windows-x64-setup.exe",
        "portable": f"{PRODUCT}-{version}-windows-x64-portable.zip",
        "dmg": f"{PRODUCT}-{version}-macos-universal.dmg",
        "appimage": f"{PRODUCT}-{version}-linux-x86_64.AppImage",
        "deb": f"{PRODUCT}-{version}-linux-amd64.deb",
    }
    return names[kind]


def pe_subsystem(data: bytes) -> int:
    """The optional header's Subsystem field of a PE image."""
    if data[:2] != b"MZ":
        raise ValueError("not an MZ executable")
    (pe_offset,) = struct.unpack_from("<I", data, 0x3C)
    if data[pe_offset:pe_offset + 4] != b"PE\0\0":
        raise ValueError("no PE signature")
    optional = pe_offset + 4 + 20
    (subsystem,) = struct.unpack_from("<H", data, optional + 68)
    return subsystem


def _one(pattern: str, root: Path) -> Path:
    found = sorted(root.glob(pattern))
    if len(found) != 1:
        raise SystemExit(f"expected exactly one {pattern} under {root}, found {found}")
    return found[0]


def collect(name: str, target: str, version: str, out: Path, repo: Path) -> list[Path]:
    release = repo / "target" / target / "release" if target else repo / "target" / "release"
    bundle = release / "bundle"
    out.mkdir(parents=True, exist_ok=True)
    made: list[Path] = []

    if name == "windows":
        exe = release / "secs2r.exe"
        subsystem = pe_subsystem(exe.read_bytes())
        if subsystem != PE_SUBSYSTEM_WINDOWS_GUI:
            raise SystemExit(f"{exe} has PE subsystem {subsystem}, expected {PE_SUBSYSTEM_WINDOWS_GUI} "
                             "(keep windows_subsystem = \"windows\" in apps/secs2r/src/main.rs)")
        setup = out / asset_name(version, "nsis")
        shutil.copy2(_one("nsis/*-setup.exe", bundle), setup)
        made.append(setup)
        portable = out / asset_name(version, "portable")
        with zipfile.ZipFile(portable, "w", compression=zipfile.ZIP_DEFLATED) as zf:
            zf.write(exe, "SECS 2R/SECS 2R.exe")
            zf.write(repo / "LICENSE", "SECS 2R/LICENSE.txt")
            zf.writestr("SECS 2R/README.txt", PORTABLE_README)
        made.append(portable)
    elif name == "macos":
        dmg = out / asset_name(version, "dmg")
        shutil.copy2(_one("dmg/*.dmg", bundle), dmg)
        made.append(dmg)
    elif name == "linux":
        appimage = out / asset_name(version, "appimage")
        shutil.copy2(_one("appimage/*.AppImage", bundle), appimage)
        appimage.chmod(0o755)
        deb = out / asset_name(version, "deb")
        shutil.copy2(_one("deb/*.deb", bundle), deb)
        made += [appimage, deb]
    else:
        raise SystemExit(f"unknown bundle set {name}")

    for path in made:
        print(f"{path.name}\t{path.stat().st_size} bytes")
    return made


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--name", required=True, choices=["windows", "macos", "linux"])
    parser.add_argument("--target", default="")
    parser.add_argument("--version", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    collect(args.name, args.target, args.version, args.out, Path.cwd())
    return 0


if __name__ == "__main__":
    sys.exit(main())
