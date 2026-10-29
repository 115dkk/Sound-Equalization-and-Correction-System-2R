#!/usr/bin/env python3
"""Check that the macOS dmg a user downloads opens and calibrates.

Runs on a macOS runner against the dmg collect_bundles.py made (ci.yml's
bundle job, release.yml's build job, before the release is published):

1. The app is copied out of the dmg, as a user drags it to Applications.
2. Signature: `codesign --verify --deep --strict` must accept the bundle and
   every architecture must carry a real signature, not only the one the
   linker adds. Apple Silicon calls a downloaded app with an unsigned bundle
   damaged and will not open it, whatever System Settings allows (0.27.1).
3. Info.plist: NSMicrophoneUsageDescription, in English and Korean. macOS
   ends an app that opens the microphone without one, so measuring would
   close SECS 2R.
4. Entitlements: under the hardened runtime macOS gives the microphone only
   to an app signed with com.apple.security.device.audio-input.
5. The app is launched through Launch Services, as a double click does, with
   SECS2R_SELF_TEST set (apps/secs2r/src/self_test.rs): the screen must
   render, and a mock-room measurement, a calibration and two exports must
   succeed through the screen's own command. With --audio-device, the native
   launch must also play and record L/R through that loopback endpoint, read
   back all three channel configurations, and save non-silent loopback IRs.
   Again for the Intel slice under Rosetta when the runner has it (mock-only).

Every check runs even after one has failed, so one run reports every problem.
The pure functions are unit-tested in ``test_macos_app_check.py``.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from xml.parsers.expat import ExpatError

AUDIO_INPUT = "com.apple.security.device.audio-input"
SELF_TEST = "SECS2R_SELF_TEST"
SELF_TEST_AUDIO = "SECS2R_SELF_TEST_AUDIO"
LOCALIZATIONS = ("en", "ko")
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


def localized_problems(strings: dict[str, bytes | None]) -> list[str]:
    """`strings` maps a localization to its InfoPlist.strings, None when the
    bundle has none."""
    problems = []
    for language in LOCALIZATIONS:
        name = f"{language}.lproj/InfoPlist.strings"
        data = strings.get(language)
        if data is None:
            problems.append(f"{name} is missing")
            continue
        try:
            table = plistlib.loads(data)
        except (ValueError, ExpatError):
            problems.append(f"{name} is not a property list")
            continue
        usage = table.get("NSMicrophoneUsageDescription") if isinstance(table, dict) else None
        if not isinstance(usage, str) or not usage.strip():
            problems.append(f"{name} has no NSMicrophoneUsageDescription")
    return problems


def entitlement_problems(entitlements: dict, hardened: bool) -> list[str]:
    if hardened and entitlements.get(AUDIO_INPUT) is not True:
        return [f"the hardened runtime is on but {AUDIO_INPUT} is not granted"]
    return []


def finite(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def impulse_facts(data: bytes, t0: float) -> dict:
    """Read only the mono float32 layout our IR writer emits, not arbitrary WAVs.

    Curves IPC normalizes away amplitude and has no time axis. The records IPC
    proves that L/R were measured; their saved IRs prove amplitude and timing.
    Native measurement crops at onset - 10 ms, preserving 2 s after onset.
    """
    if (len(data) < 44 or data[:4] != b"RIFF" or data[8:16] != b"WAVEfmt " or
            data[36:40] != b"data"):
        raise ValueError("native IR is not the expected RIFF/float32 WAV")
    size, tag, channels, rate, byte_rate, align, bits = struct.unpack_from("<IHHIIHH", data, 16)
    payload = struct.unpack_from("<I", data, 40)[0]
    if (size != 16 or tag != 3 or channels != 1 or rate != 48000 or
            byte_rate != rate * 4 or align != 4 or bits != 32 or
            payload != len(data) - 44 or payload == 0 or payload % 4 or
            struct.unpack_from("<I", data, 4)[0] != len(data) - 8):
        raise ValueError("native IR must be a complete mono 48 kHz float32 WAV")
    samples = [sample[0] for sample in struct.iter_unpack("<f", data[44:])]
    if not finite(t0) or not all(finite(sample) for sample in samples):
        raise ValueError("native IR or its time origin is non-finite")
    peak = max(range(len(samples)), key=lambda index: abs(samples[index]))
    energy = math.fsum(sample * sample for sample in samples)
    radius = round(rate * 0.001)
    direct = math.fsum(sample * sample for sample in samples[max(0, peak - radius):peak + radius + 1])
    return {
        "sample_rate": rate, "frames": len(samples), "peak_amplitude": abs(samples[peak]),
        "peak_ms": peak * 1000 / rate, "arrival_ms": (t0 + peak / rate) * 1000,
        "direct_energy_fraction": direct / energy if energy else 0,
    }


def read_audio_impulses(work: Path) -> dict:
    project = json.loads((work / "audio.secs").read_text(encoding="utf-8"))
    records = [record for record in project["measurements"] if record["pos"] == 1]
    if len(records) != 1 or records[0].get("done") is not True:
        raise ValueError("project_save: native position 1 is missing or incomplete")
    record = records[0]
    facts = {}
    for role in ("L", "R"):
        path = work / "audio.irs" / f"{role}_P1.wav"
        if (work / record["ir"][role]).resolve() != path.resolve():
            raise ValueError(f"project_save: {role} does not reference its native IR")
        if path.stat().st_size > 4 * 48000 * 3 + 44:
            raise ValueError(f"project_save: {role} native IR exceeds the 3 s check budget")
        facts[role] = impulse_facts(path.read_bytes(), record["t0"][role])
    return facts


def audio_problems(audio: object, device: str) -> list[str]:
    if not isinstance(audio, dict) or audio.get("ok") is not True:
        return [f"native audio step did not pass: {audio}"]
    problems = []
    if audio.get("device") != device:
        problems.append("native audio used the wrong device")
    configurations = audio.get("configurations", [])
    expected = [("level_L", 1, [1, 2]), ("level_R", 2, [1, 2]), ("sweep", 1, [1, 1])]
    if not isinstance(configurations, list) or len(configurations) != 3:
        problems.append("measure_config_get: all three native configurations must be read back")
    else:
        for entry, (stage, channel, outputs) in zip(configurations, expected):
            config = entry.get("config", {}) if isinstance(entry, dict) else {}
            wanted = {
                "method": "native", "input_device": audio.get("input_device"),
                "output_device": audio.get("output_device"), "input_channel": channel,
                "output_channels": outputs, "roles": ["L", "R"], "sample_rate": 48000,
                "sweep_length": 65536, "repetitions": 1, "sweep_level_dbfs": -20,
                "share_mode": "shared", "timing": "none",
            }
            if (not isinstance(entry, dict) or entry.get("stage") != stage or
                    not isinstance(config, dict) or
                    any(config.get(key) != value for key, value in wanted.items()) or
                    not config.get("input_device") or not config.get("output_device")):
                problems.append(f"measure_config_get: wrong {stage} configuration")
    if audio.get("played_dbfs") != -20:
        problems.append("level_play: the native test must play at -20 dBFS")
    levels = audio.get("levels", {})
    for role in ("L", "R"):
        value = levels.get(role) if isinstance(levels, dict) else None
        if not finite(value) or abs(value - (-20)) > 6:
            problems.append(f"levels_get: {role} must be within 6 dB of -20 dBFS, got {value}")
    records = audio.get("records", {})
    records = records.get("records", []) if isinstance(records, dict) else []
    record = next((entry for entry in records if isinstance(entry, dict) and entry.get("position") == 1), {})
    if (record.get("done") is not True or
            any(role not in record.get("roles", []) or record.get("sources", {}).get(role) != "measured"
                for role in ("L", "R"))):
        problems.append("measurement_records: native position 1 needs measured L and R")
    impulses = audio.get("impulses", {})
    arrivals = []
    for role in ("L", "R"):
        ir = impulses.get(role, {}) if isinstance(impulses, dict) else {}
        if not isinstance(ir, dict):
            ir = {}
        limits = {
            "peak_amplitude": (10 ** (-6 / 20), 10 ** (6 / 20)),
            "peak_ms": (9, 11), "arrival_ms": (-1, 500), "direct_energy_fraction": (0.9, 1),
        }
        if (ir.get("sample_rate") != 48000 or ir.get("frames") != 96480 or
                any(not finite(ir.get(key)) or not low <= ir[key] <= high
                    for key, (low, high) in limits.items())):
            problems.append(f"measure_position/project_save: {role} has no unity loopback impulse at the 10 ms pre-roll: {ir}")
        if finite(ir.get("arrival_ms")):
            arrivals.append(ir["arrival_ms"])
    if len(arrivals) == 2 and abs(arrivals[0] - arrivals[1]) > 1:
        problems.append("measure_position: L and R loopback arrivals differ by more than 1 ms")
    if audio.get("impulse_error"):
        problems.append(f"measure_position/project_save: {audio['impulse_error']}")
    return problems


def audio_retries(audio: object) -> list[str]:
    """One line per native audio attempt that the self-test retried."""
    retries = audio.get("retries", []) if isinstance(audio, dict) else []
    if not isinstance(retries, list):
        return []
    return [f"retried {entry.get('stage')} after attempt {entry.get('attempt')}: {entry.get('error')}"
            for entry in retries if isinstance(entry, dict)]


def report_problems(label: str, report: dict | None, work: Path,
                    audio_device: str | None = None) -> list[str]:
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
    if audio_device is not None:
        problems += [f"{label}: {problem}" for problem in audio_problems(
            report.get("facts", {}).get("audio"), audio_device
        )]
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
    # The dmg shows the GPL as a license agreement (bundle.licenseFile), and
    # hdiutil asks for the answer on stdin, as Finder asks a user.
    cmd = ["hdiutil", "attach", "-nobrowse", "-readonly", "-noautoopen",
           "-mountpoint", str(mount), str(dmg)]
    print("$ " + " ".join(cmd), flush=True)
    attach = subprocess.run(cmd, input="Y\n", capture_output=True, text=True,
                            env={**os.environ, "PAGER": "cat"})
    if attach.returncode != 0:
        print(attach.stderr.rstrip())
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
        problems = info_plist_problems(plistlib.load(handle))
    resources = app / "Contents" / "Resources"
    strings = {}
    for language in LOCALIZATIONS:
        path = resources / f"{language}.lproj" / "InfoPlist.strings"
        strings[language] = path.read_bytes() if path.is_file() else None
    return problems + localized_problems(strings)


def check_entitlements(app: Path, hardened: bool) -> list[str]:
    shown = subprocess.run(["codesign", "-d", "--entitlements", "-", "--xml", str(app)],
                           capture_output=True)
    entitlements = plistlib.loads(shown.stdout) if shown.stdout.strip() else {}
    print(f"entitlements: {entitlements}, hardened runtime: {hardened}")
    return entitlement_problems(entitlements, hardened)


def rosetta() -> bool:
    return subprocess.run(["arch", "-x86_64", "/usr/bin/true"], capture_output=True).returncode == 0


def self_test(app: Path, arch: str, work: Path, timeout: int,
              audio_device: str | None = None) -> list[str]:
    label = f"self-test ({arch})"
    report = work / f"self-test-{arch}.json"
    log = work / f"self-test-{arch}.log"
    test_work = report.with_suffix(".work")
    for stale in (report, log):
        stale.unlink(missing_ok=True)
    shutil.rmtree(test_work, ignore_errors=True)
    # Explicitly clear the variable for Rosetta (and mock-only invocations),
    # even when the parent runner's environment happens to contain it.
    cmd = ["open", "-n", "-W", "--env", f"{SELF_TEST}={report}",
           "--env", f"{SELF_TEST_AUDIO}={audio_device or ''}",
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
    if audio_device and data is not None:
        audio = data.get("facts", {}).get("audio")
        if isinstance(audio, dict) and audio.get("ok") is True:
            try:
                audio["impulses"] = read_audio_impulses(test_work)
            except (OSError, ValueError, KeyError, TypeError, struct.error) as error:
                audio["impulse_error"] = str(error)
    problems = report_problems(label, data, test_work, audio_device)
    if data is not None and audio_device:
        data["check_problems"] = problems
        if problems:
            data["ok"] = False
            data["error"] = "; ".join(problems)
            audio = data.get("facts", {}).get("audio")
            if isinstance(audio, dict):
                audio["ok"] = False
        report.write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(data, indent=2, ensure_ascii=False))
    print("::endgroup::", flush=True)
    if data is not None:
        for note in audio_retries(data.get("facts", {}).get("audio")):
            print(f"::warning::{label}: {note}", flush=True)
    return problems


def crash_reports(since: float, work: Path) -> None:
    folder = Path.home() / "Library" / "Logs" / "DiagnosticReports"
    for path in (sorted(folder.glob("secs2r*")) if folder.exists() else []):
        if path.stat().st_mtime >= since:
            shutil.copy2(path, work / path.name)
            print(f"::group::crash report {path.name}")
            print(path.read_text(errors="replace")[:20000])
            print("::endgroup::")


def check(dmg: Path, work: Path, timeout: int, audio_device: str | None = None) -> list[str]:
    started = time.time()
    work.mkdir(parents=True, exist_ok=True)
    app = copy_app(dmg, work)
    problems, hardened = check_signature(app)
    problems += check_info_plist(app)
    problems += check_entitlements(app, hardened)
    problems += self_test(app, "arm64", work, timeout, audio_device)
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
    parser.add_argument("--audio-device", help="also play and record through this native loopback device")
    args = parser.parse_args(argv)
    if sys.platform != "darwin":
        raise SystemExit("macos_app_check.py runs on macOS")
    problems = check(args.dmg.resolve(), args.work.resolve(), args.timeout, args.audio_device)
    for problem in problems:
        print(f"::error::{problem}")
    print("the app opens and calibrates" if not problems else f"{len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
