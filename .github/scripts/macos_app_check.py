#!/usr/bin/env python3
"""Check that the macOS dmg a user downloads opens and calibrates.

Runs on a macOS runner against the dmg collect_bundles.py made (ci.yml's
bundle job, release.yml's build job, before the release is published):

1. The app is copied out of the dmg, as a user drags it to Applications.
2. Signature: `codesign --verify --deep --strict` must accept the bundle and
   every architecture must carry a real signature, not only the one the
   linker adds. Apple Silicon calls a downloaded app with an unsigned bundle
   damaged and will not open it, whatever System Settings allows (0.27.1).
3. Info.plist: NSMicrophoneUsageDescription. macOS ends an app that opens
   the microphone without one, so measuring would close SECS 2R.
4. Entitlements: under the hardened runtime macOS gives the microphone only
   to an app signed with com.apple.security.device.audio-input.
5. The app is launched through Launch Services, as a double click does, with
   SECS2R_SELF_TEST set (apps/secs2r/src/self_test.rs): the screen must
   render, and a mock-room measurement, a calibration and two exports must
   succeed through the screen's own command. Again for the Intel slice under
   Rosetta when the runner has it.

Every check runs even after one has failed, so one run reports every problem.
The pure functions are unit-tested in ``test_macos_app_check.py``.
"""
from __future__ import annotations

import argparse
import json
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

AUDIO_INPUT = "com.apple.security.device.audio-input"
SELF_TEST = "SECS2R_SELF_TEST"
EXPORTS = {
    "camilla-dsp": ".yml",
    "equalizer-apo": "config.txt",
}


def code_directory_flags(details: str) -> set[str] | None:
    """The flag names of `codesign -dv --verbose=4` output
    (`flags=0x10002(adhoc,runtime)`), or None when there is no signature."""
    match = re.search(r"^CodeDirectory\b.*\bflags=0x[0-9a-f]+\(([^)]*)\)", details, re.MULTILINE)
    if match is None:
        return None
    return {name for name in match.group(1).split(",") if name and name != "none"}


def signature_problems(arch: str, details: str) -> list[str]:
    flags = code_directory_flags(details)
    if flags is None:
        return [f"the {arch} slice is not signed"]
    if "linker-signed" in flags:
        return [f"the {arch} slice carries only the linker's signature (bundle not signed)"]
    return []


def info_plist_problems(info: dict) -> list[str]:
    usage = info.get("NSMicrophoneUsageDescription")
    if not isinstance(usage, str) or not usage.strip():
        return ["Info.plist has no NSMicrophoneUsageDescription"]
    return []


def entitlement_problems(entitlements: dict, hardened: bool) -> list[str]:
    if hardened and entitlements.get(AUDIO_INPUT) is not True:
        return [f"the hardened runtime is on but {AUDIO_INPUT} is not granted"]
    return []


def report_problems(label: str, report: dict | None, work: Path) -> list[str]:
    """What is wrong with one self-test run: its report and its exports."""
    if report is None:
        return [f"{label}: the app wrote no self-test report"]
    if report.get("ok") is not True:
        return [f"{label}: the self-test failed: {report.get('error')}"]
    problems = []
    for folder, wanted in EXPORTS.items():
        files = [path for path in (work / "export" / folder).rglob("*") if path.is_file()]
        if not any(path.name.endswith(wanted) for path in files):
            problems.append(f"{label}: the {folder} export has no {wanted} file")
        problems += [f"{label}: {path} is empty" for path in files if path.stat().st_size == 0]
    return problems


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    print("$ " + " ".join(cmd), flush=True)
    result = subprocess.run(cmd, capture_output=True, text=True, **kwargs)
    for stream in (result.stdout, result.stderr):
        if stream.strip():
            print(stream.rstrip())
    return result


def copy_app(dmg: Path, work: Path) -> Path:
    mount = Path(tempfile.mkdtemp(prefix="secs2r-dmg-"))
    attach = run(["hdiutil", "attach", "-nobrowse", "-readonly", "-noautoopen",
                  "-mountpoint", str(mount), str(dmg)])
    if attach.returncode != 0:
        raise SystemExit(f"cannot mount {dmg}")
    try:
        apps = sorted(mount.glob("*.app"))
        if len(apps) != 1:
            raise SystemExit(f"expected one app in {dmg}, found {apps}")
        app = work / apps[0].name
        shutil.rmtree(app, ignore_errors=True)
        if run(["ditto", str(apps[0]), str(app)]).returncode != 0:
            raise SystemExit(f"cannot copy {apps[0]}")
        return app
    finally:
        run(["hdiutil", "detach", str(mount)])


def executable(app: Path) -> Path:
    with open(app / "Contents" / "Info.plist", "rb") as handle:
        info = plistlib.load(handle)
    return app / "Contents" / "MacOS" / info["CFBundleExecutable"]


def check_signature(app: Path) -> tuple[list[str], bool]:
    """Problems with the bundle's signature, and whether the main executable
    runs under the hardened runtime."""
    problems = []
    if run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", str(app)]).returncode != 0:
        problems.append("codesign --verify --deep --strict rejects the bundle")
    archs = run(["lipo", "-archs", str(executable(app))]).stdout.split()
    hardened = False
    for arch in archs:
        details = run(["codesign", "-dv", "--verbose=4", "--arch", arch, str(app)]).stderr
        problems += signature_problems(arch, details)
        hardened |= "runtime" in (code_directory_flags(details) or set())
    # Gatekeeper's own verdict, for the log: every build that is not
    # notarized is rejected, so it is no gate.
    run(["spctl", "--status"])
    run(["spctl", "--assess", "--type", "execute", "--verbose=4", str(app)])
    return problems, hardened


def check_info_plist(app: Path) -> list[str]:
    with open(app / "Contents" / "Info.plist", "rb") as handle:
        return info_plist_problems(plistlib.load(handle))


def check_entitlements(app: Path, hardened: bool) -> list[str]:
    shown = subprocess.run(["codesign", "-d", "--entitlements", "-", "--xml", str(app)],
                           capture_output=True)
    entitlements = plistlib.loads(shown.stdout) if shown.stdout.strip() else {}
    print(f"entitlements: {entitlements}, hardened runtime: {hardened}")
    return entitlement_problems(entitlements, hardened)


def rosetta() -> bool:
    return subprocess.run(["arch", "-x86_64", "/usr/bin/true"], capture_output=True).returncode == 0


def self_test(app: Path, arch: str, work: Path, timeout: int) -> list[str]:
    label = f"self-test ({arch})"
    report = work / f"self-test-{arch}.json"
    log = work / f"self-test-{arch}.log"
    test_work = report.with_suffix(".work")
    for stale in (report, log):
        stale.unlink(missing_ok=True)
    shutil.rmtree(test_work, ignore_errors=True)
    cmd = ["open", "-n", "-W", "--env", f"{SELF_TEST}={report}",
           "--stdout", str(log), "--stderr", str(log)]
    if arch == "x86_64":
        cmd += ["--arch", "x86_64"]
    print(f"::group::{label}", flush=True)
    try:
        run(cmd + [str(app)], timeout=timeout)
    except subprocess.TimeoutExpired:
        run(["pkill", "-f", str(executable(app))])
        print(f"{label}: no exit within {timeout} s")
    if log.exists():
        print(log.read_text(errors="replace"))
    data = json.loads(report.read_text()) if report.exists() else None
    print(json.dumps(data, indent=2, ensure_ascii=False))
    print("::endgroup::", flush=True)
    return report_problems(label, data, test_work)


def crash_reports(since: float, work: Path) -> None:
    folder = Path.home() / "Library" / "Logs" / "DiagnosticReports"
    for path in (sorted(folder.glob("secs2r*")) if folder.exists() else []):
        if path.stat().st_mtime >= since:
            shutil.copy2(path, work / path.name)
            print(f"::group::crash report {path.name}")
            print(path.read_text(errors="replace")[:20000])
            print("::endgroup::")


def check(dmg: Path, work: Path, timeout: int) -> list[str]:
    started = time.time()
    work.mkdir(parents=True, exist_ok=True)
    app = copy_app(dmg, work)
    problems, hardened = check_signature(app)
    problems += check_info_plist(app)
    problems += check_entitlements(app, hardened)
    problems += self_test(app, "arm64", work, timeout)
    if rosetta():
        problems += self_test(app, "x86_64", work, timeout)
    else:
        print("::warning::Rosetta is missing; the Intel slice was not launched")
    crash_reports(started, work)
    shutil.rmtree(app, ignore_errors=True)
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dmg", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True,
                        help="folder for the copied app, the reports and the logs")
    parser.add_argument("--timeout", type=int, default=600, help="seconds per launch")
    args = parser.parse_args(argv)
    if sys.platform != "darwin":
        raise SystemExit("macos_app_check.py runs on macOS")
    problems = check(args.dmg.resolve(), args.work.resolve(), args.timeout)
    for problem in problems:
        print(f"::error::{problem}")
    print("the app opens and calibrates" if not problems else f"{len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
