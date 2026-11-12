"""Issue #208 probe: repeat the reporter's calibration on this platform.

Usage: issue208.py <secs2r-cli> <work dir> <runs>

Downloads the reporter's public attachments, builds the same project
(seven bed mains, four heights, four subs, low-latency mode, RC upper 160 Hz,
MIMO FIR), calibrates and exports `runs` times, and prints hashes plus a
comparison with the CamillaDSP config the reporter exported on macOS.
This branch is a one-off experiment and is never merged.
"""

from __future__ import annotations

import hashlib
import json
import platform
import re
import shutil
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

MDAT_URL = "https://drive.usercontent.google.com/download?id=1crqr19sfND5qItsha7AvQUm6eJmL5n3z&export=download&confirm=t"
EXPORT_URL = "https://github.com/user-attachments/files/34732734/SECS_camilla_RC_BA_MIMO_mixed_20261112-151555.zip"
ORDER = ["L", "R", "Ls", "Rs", "Lb", "Rb", "Tfl", "Tfr", "Tbl", "Tbr", "SW1", "SW2", "SW3", "SW4"]
REPORTER_XO = [100.0, 100.0, 100.0, 80.0]
FILTER_LINE = re.compile(r"^  ([A-Za-z0-9_]+): (\{.*\})$")


def cli(exe: str, *args: str) -> list[dict]:
    done = subprocess.run([exe, "--quiet", *args], capture_output=True, check=False)
    replies = []
    for line in done.stdout.decode("utf-8", "replace").splitlines():
        try:
            replies.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    failed = [r for r in replies if not r.get("reply", {}).get("ok", True)]
    if done.returncode != 0 or failed:
        raise SystemExit(f"{args[0]} failed ({done.returncode}): {json.dumps(failed)[:600]} {done.stderr[-600:]!r}")
    return replies


def download(url: str, path: Path) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "probe"})
    with urllib.request.urlopen(request, timeout=300) as response, open(path, "wb") as out:
        shutil.copyfileobj(response, out)


def filters(path: Path) -> dict[str, dict]:
    found = {}
    inside = False
    for line in path.read_text(encoding="utf-8").splitlines():
        if line == "filters:":
            inside = True
            continue
        if inside and not line.startswith("  "):
            inside = False
        match = FILTER_LINE.match(line) if inside else None
        if match:
            found[match.group(1)] = json.loads(match.group(2))
    return found


def normalized_hash(path: Path) -> str:
    kept = [
        line
        for line in path.read_text(encoding="utf-8").splitlines()
        if "RawFile" not in line and '"type": "File"' not in line and not line.startswith(("title", "description"))
    ]
    return hashlib.sha256("\n".join(kept).encode()).hexdigest()[:16]


def main() -> None:
    exe, work, runs = sys.argv[1], Path(sys.argv[2]).resolve(), int(sys.argv[3])
    work.mkdir(parents=True, exist_ok=True)
    print(f"platform {platform.system()} {platform.machine()} {platform.platform()}")
    print(json.dumps(cli(exe, "bootstrap")[-1])[:300])

    mdat = work / "input" / "1112.mdat"
    mdat.parent.mkdir(exist_ok=True)
    download(MDAT_URL, mdat)
    print(f"mdat bytes {mdat.stat().st_size} sha256 {hashlib.sha256(mdat.read_bytes()).hexdigest()}")
    reporter_zip = work / "reporter" / "export.zip"
    reporter_zip.parent.mkdir(exist_ok=True)
    download(EXPORT_URL, reporter_zip)
    with zipfile.ZipFile(reporter_zip) as archive:
        name = next(n for n in archive.namelist() if n.endswith("configs/secs_mixed.yml"))
        reporter_yml = work / "reporter" / "secs_mixed.yml"
        reporter_yml.write_bytes(archive.read(name))
    reporter = filters(reporter_yml)

    base = work / "base"
    base.mkdir(exist_ok=True)
    project = base / "p.secs2r"
    cli(exe, "project", "new", "--preset", "7.1.4", "--subs", "4", str(project))
    cli(exe, "import-mdat", str(project), str(mdat))
    data = json.loads(project.read_text(encoding="utf-8"))
    speakers = [s for s in data["config"]["speakers"] if s["role"] != "C"]
    speakers.sort(key=lambda s: ORDER.index(s["role"]))
    for index, speaker in enumerate(speakers):
        speaker["bands"][0]["out"] = index + 1
    data["config"]["speakers"] = speakers
    data["target"] = {"points": [[25.0, 0.0], [200.0, 0.0], [1000.0, 0.0], [16000.0, 0.0]]}
    project.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
    cli(exe, "project", "set", "--mimo", "on", "--mimo-supports", "fir", str(project))

    for attempt in range(runs):
        best = cli(exe, "recommend-xo", str(project))[-1]["reply"]["data"]["data"]["best"]
        print(f"recommend-xo run {attempt + 1}: {[value for _, value in best]}")
    data = json.loads(project.read_text(encoding="utf-8"))
    data["xo"] = {group: value for (group, _), value in zip(best, REPORTER_XO)}
    data["calibration"]["rc_hi"] = 160.0
    project.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")

    hashes = set()
    for attempt in range(runs):
        run = work / f"run{attempt + 1}"
        shutil.rmtree(run, ignore_errors=True)
        shutil.copytree(base, run, ignore=shutil.ignore_patterns("*.mdat"))
        cli(exe, "calibrate", "--mimo", str(run / "p.secs2r"))
        cli(exe, "export", "--kind", "camilla-dsp", "--out", str(run / "out"), str(run / "p.secs2r"))
        yml = run / "out" / "configs" / "secs_mixed.yml"
        coeffs = hashlib.sha256()
        for wav in sorted((run / "out" / "coeffs").glob("*.wav")):
            coeffs.update(wav.read_bytes())
        mine = filters(yml)
        same = sum(1 for name, value in mine.items() if reporter.get(name) == value)
        digest = (normalized_hash(yml), coeffs.hexdigest()[:16])
        hashes.add(digest)
        subs = [line for line in yml.read_text(encoding="utf-8").splitlines() if line.startswith("# sub ")]
        print(
            f"run {attempt + 1}: yml {digest[0]} coeffs {digest[1]} filters {len(mine)} "
            f"identical to reporter {same}/{len(reporter)} main_Tbr_gain {mine['main_Tbr_gain']['parameters']['gain']} "
            f"(reporter {reporter['main_Tbr_gain']['parameters']['gain']})"
        )
        if attempt == 0:
            print("\n".join(subs))
            (work / "result").mkdir(exist_ok=True)
            shutil.copy(yml, work / "result" / "secs_mixed.yml")
    print(f"distinct results over {runs} runs: {len(hashes)}")
    if len(hashes) != 1:
        raise SystemExit("results differ between runs on the same platform")


if __name__ == "__main__":
    main()
