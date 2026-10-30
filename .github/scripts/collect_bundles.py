#!/usr/bin/env python3
"""Copy what `tauri build` produced into one folder under stable names.

Release asset names are an interface (download pages and, later, the
updater depend on them), so they are fixed here and not left to Tauri's
product-name-based file names:

    SECS2R-<version>-windows-x64-setup.exe
    SECS2R-<version>-windows-x64-setup.exe.sig
    SECS2R-<version>-windows-x64-portable.zip
    SECS2R-<version>-windows-x64-cli.zip
    SECS2R-<version>-macos-universal.dmg
    SECS2R-<version>-macos-universal.app.tar.gz
    SECS2R-<version>-macos-universal.app.tar.gz.sig
    SECS2R-<version>-macos-universal-cli.tar.gz
    SECS2R-<version>-linux-x86_64.AppImage
    SECS2R-<version>-linux-x86_64.AppImage.sig
    SECS2R-<version>-linux-amd64.deb
    SECS2R-<version>-linux-amd64.deb.sig
    SECS2R-<version>-linux-x86_64-cli.tar.gz

On Windows the release exe and host shell must be GUI-subsystem executables
(no console window behind either app), while the CLI and host service use the
console subsystem; the check reads the PE headers. The portable zip and the
installer carry all three companions next to the app (release.yml adds them to
the installer's resources). Every CLI archive is checked by running the binary
it holds: `--version` must name this version.

The pure functions are unit-tested in ``test_collect_bundles.py``.
"""
from __future__ import annotations

import argparse
import io
import shutil
import struct
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

PRODUCT = "SECS2R"
PE_SUBSYSTEM_WINDOWS_GUI = 2
PE_SUBSYSTEM_WINDOWS_CUI = 3

PORTABLE_README = """SECS 2R portable

압축을 푼 폴더에서 "SECS 2R.exe"를 실행합니다. 설치가 필요 없고, 이 판은
자동 업데이트를 하지 않지만 새 판이 나오면 알려 줍니다. Windows 10/11에 포함된
Microsoft Edge WebView2 런타임을 씁니다. 화면 없이 쓰는 CLI는 같은 폴더의
secs2r-cli.exe입니다(secs2r-cli.exe --help). 관리 기능을 설치하면
secs2r-host.exe와 secs2r-host-shell.exe를 사용합니다.

Unzip and run "SECS 2R.exe". Nothing is installed and this build does not
update itself, but it tells you when a new release is available. It uses the
Microsoft Edge WebView2 runtime that ships with Windows 10 and 11. The
command-line tool is secs2r-cli.exe in the same
folder (secs2r-cli.exe --help). Installing managed mode uses secs2r-host.exe
and secs2r-host-shell.exe.

License: GPL-3.0-only (LICENSE.txt). Source:
https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R
"""

CLI_README = """SECS 2R CLI

화면 없이 SECS 2R의 계산과 내보내기를 실행하는 명령줄 도구입니다. 같은 판의
앱과 같은 엔진을 씁니다. 쓰는 법은 secs2r-cli --help로 확인하십시오.

The command-line tool that runs SECS 2R's calculation and exports without a
window. It uses the same engine as the app of the same version. Run
secs2r-cli --help for its commands.

License: GPL-3.0-only (LICENSE.txt). Source:
https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R
"""


def asset_name(version: str, kind: str) -> str:
    names = {
        "nsis": f"{PRODUCT}-{version}-windows-x64-setup.exe",
        "nsis-sig": f"{PRODUCT}-{version}-windows-x64-setup.exe.sig",
        "portable": f"{PRODUCT}-{version}-windows-x64-portable.zip",
        "cli-windows": f"{PRODUCT}-{version}-windows-x64-cli.zip",
        "dmg": f"{PRODUCT}-{version}-macos-universal.dmg",
        "updater-macos": f"{PRODUCT}-{version}-macos-universal.app.tar.gz",
        "updater-macos-sig": f"{PRODUCT}-{version}-macos-universal.app.tar.gz.sig",
        "cli-macos": f"{PRODUCT}-{version}-macos-universal-cli.tar.gz",
        "appimage": f"{PRODUCT}-{version}-linux-x86_64.AppImage",
        "appimage-sig": f"{PRODUCT}-{version}-linux-x86_64.AppImage.sig",
        "deb": f"{PRODUCT}-{version}-linux-amd64.deb",
        "deb-sig": f"{PRODUCT}-{version}-linux-amd64.deb.sig",
        "cli-linux": f"{PRODUCT}-{version}-linux-x86_64-cli.tar.gz",
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


def check_pe_subsystem(path: Path, expected: int) -> None:
    """Require one PE executable with the subsystem assigned to its role."""
    if not path.is_file():
        raise SystemExit(f"required Windows executable is missing: {path}")
    try:
        subsystem = pe_subsystem(path.read_bytes())
    except (OSError, ValueError, struct.error) as error:
        raise SystemExit(f"could not inspect {path}: {error}") from error
    if subsystem != expected:
        raise SystemExit(f"{path} has PE subsystem {subsystem}, expected {expected}")


def expected_cli_version(version: str) -> str:
    """What `secs2r-cli --version` prints for this release."""
    return f"secs2r-cli {version}"


def write_portable_zip(path: Path, exe: Path, cli: Path, host: Path,
                       shell: Path, license_file: Path, host_manifest: Path,
                       host_manifest_signature: Path) -> None:
    """Write the portable app with every Windows companion and update file."""
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.write(exe, "SECS 2R/SECS 2R.exe")
        zf.write(cli, "SECS 2R/secs2r-cli.exe")
        zf.write(host, "SECS 2R/secs2r-host.exe")
        zf.write(host_manifest, "SECS 2R/secs2r-host.json")
        zf.write(host_manifest_signature, "SECS 2R/secs2r-host.json.sig")
        zf.write(shell, "SECS 2R/secs2r-host-shell.exe")
        zf.write(license_file, "SECS 2R/LICENSE.txt")
        zf.writestr("SECS 2R/portable", b"")
        zf.writestr("SECS 2R/README.txt", PORTABLE_README)


def write_cli_zip(path: Path, exe: Path, license_file: Path) -> None:
    """The Windows CLI archive: one folder with the exe, the licence and a readme."""
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.write(exe, "secs2r-cli/secs2r-cli.exe")
        zf.write(license_file, "secs2r-cli/LICENSE.txt")
        zf.writestr("secs2r-cli/README.txt", CLI_README)


def write_cli_tar(path: Path, binary: Path, license_file: Path) -> None:
    """The macOS and Linux CLI archive; the binary keeps its executable bit."""

    def add_bytes(tf: tarfile.TarFile, name: str, data: bytes, mode: int) -> None:
        info = tarfile.TarInfo(name)
        info.size = len(data)
        info.mode = mode
        tf.addfile(info, io.BytesIO(data))

    with tarfile.open(path, "w:gz") as tf:
        add_bytes(tf, "secs2r-cli/secs2r-cli", binary.read_bytes(), 0o755)
        add_bytes(tf, "secs2r-cli/LICENSE.txt", license_file.read_bytes(), 0o644)
        add_bytes(tf, "secs2r-cli/README.txt", CLI_README.encode("utf-8"), 0o644)


def check_cli(binary: Path, version: str) -> None:
    """Run the CLI that goes into the release; its version must be this release's."""
    reported = subprocess.run([str(binary), "--version"], capture_output=True, text=True,
                              check=True).stdout.strip()
    if reported != expected_cli_version(version):
        raise SystemExit(f"{binary} reports '{reported}', expected '{expected_cli_version(version)}'")


def _one(pattern: str, root: Path) -> Path:
    found = sorted(root.glob(pattern))
    if len(found) != 1:
        raise SystemExit(f"expected exactly one {pattern} under {root}, found {found}")
    return found[0]


def collect(name: str, target: str, version: str, out: Path, repo: Path,
            cli_version: str | None = None) -> list[Path]:
    """Collect the bundles of `name`. `cli_version` is what the CLI must report
    (default `version`; ci.yml names its assets with a CI version instead)."""
    cli_version = cli_version or version
    release = repo / "target" / target / "release" if target else repo / "target" / "release"
    bundle = release / "bundle"
    license_file = repo / "LICENSE"
    out.mkdir(parents=True, exist_ok=True)
    made: list[Path] = []

    if name == "windows":
        exe = release / "secs2r.exe"
        cli = release / "secs2r-cli.exe"
        host = release / "secs2r-host.exe"
        shell = release / "secs2r-host-shell.exe"
        check_pe_subsystem(exe, PE_SUBSYSTEM_WINDOWS_GUI)
        check_pe_subsystem(cli, PE_SUBSYSTEM_WINDOWS_CUI)
        check_pe_subsystem(host, PE_SUBSYSTEM_WINDOWS_CUI)
        check_pe_subsystem(shell, PE_SUBSYSTEM_WINDOWS_GUI)
        check_cli(cli, cli_version)
        setup = out / asset_name(version, "nsis")
        shutil.copy2(_one("nsis/*-setup.exe", bundle), setup)
        setup_signature = out / asset_name(version, "nsis-sig")
        shutil.copy2(_one("nsis/*-setup.exe.sig", bundle), setup_signature)
        made += [setup, setup_signature]
        host_manifest = release / "secs2r-host.json"
        host_manifest_signature = release / "secs2r-host.json.sig"
        for required in (host_manifest, host_manifest_signature):
            if not required.is_file():
                raise SystemExit(f"required Windows host update file is missing: {required}")
        portable = out / asset_name(version, "portable")
        write_portable_zip(portable, exe, cli, host, shell, license_file,
                           host_manifest, host_manifest_signature)
        made.append(portable)
        cli_zip = out / asset_name(version, "cli-windows")
        write_cli_zip(cli_zip, cli, license_file)
        made.append(cli_zip)
    elif name == "macos":
        dmg = out / asset_name(version, "dmg")
        shutil.copy2(_one("dmg/*.dmg", bundle), dmg)
        updater = out / asset_name(version, "updater-macos")
        shutil.copy2(_one("macos/*.app.tar.gz", bundle), updater)
        updater_signature = out / asset_name(version, "updater-macos-sig")
        shutil.copy2(_one("macos/*.app.tar.gz.sig", bundle), updater_signature)
        made += [dmg, updater, updater_signature]
        cli = release / "secs2r-cli"
        check_cli(cli, cli_version)
        cli_tar = out / asset_name(version, "cli-macos")
        write_cli_tar(cli_tar, cli, license_file)
        made.append(cli_tar)
    elif name == "linux":
        appimage = out / asset_name(version, "appimage")
        shutil.copy2(_one("appimage/*.AppImage", bundle), appimage)
        appimage.chmod(0o755)
        appimage_signature = out / asset_name(version, "appimage-sig")
        shutil.copy2(_one("appimage/*.AppImage.sig", bundle), appimage_signature)
        deb = out / asset_name(version, "deb")
        shutil.copy2(_one("deb/*.deb", bundle), deb)
        deb_signature = out / asset_name(version, "deb-sig")
        shutil.copy2(_one("deb/*.deb.sig", bundle), deb_signature)
        made += [appimage, appimage_signature, deb, deb_signature]
        cli = release / "secs2r-cli"
        check_cli(cli, cli_version)
        cli_tar = out / asset_name(version, "cli-linux")
        write_cli_tar(cli_tar, cli, license_file)
        made.append(cli_tar)
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
    parser.add_argument("--cli-version", default=None,
                        help="version the CLI must report (default: --version)")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    collect(args.name, args.target, args.version, args.out, Path.cwd(), args.cli_version)
    return 0


if __name__ == "__main__":
    sys.exit(main())
