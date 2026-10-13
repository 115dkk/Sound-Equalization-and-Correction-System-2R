"""Freeze CamillaDSP v4 text, coefficient files, and Python formatting edges."""
from __future__ import annotations

import math
import shutil
import tempfile
from collections import OrderedDict
from pathlib import Path
from types import SimpleNamespace

import common
from engine_cases import cases
from secs import calibration as cal
from secs import export_camilladsp as cam

FOLDER = "{OUT}"
COMPUTED = {"dolby", "dolby_zero", "dolby_ultra", "dolby_infra", "room_manual", "room_dba", "room_filters"}


def band(value):
    return [float(x) for x in value]


def chain_from(run):
    eng, res = run.engine, run.result
    return {
        "sample_rate": 48000,
        "mains": [{
            "role": role,
            "input_peq": [band(x) for x in c.peq],
            "fir": None,
            "gain_db": float(c.gain_db),
            "delay_ms": float(c.delay_ms),
            "xo_hz": None if c.xo_hz is None else float(c.xo_hz),
            "top_peq": [band(x) for x in eng.top_peq.get(role, [])],
            "align_delay_ms": float(eng.align_delay_ms[role]),
        } for role, c in res.channels.items()],
        "subs": [{
            "role": role,
            "gain_db": float(a.gain_db),
            "delay_ms": float(a.delay_ms),
            "polarity": int(a.polarity),
            "allpass": [band(x) for x in a.allpass],
            "hp_hz": float(a.hp_hz),
            "hp_type": str(a.hp_type),
            "lp_hz": float(a.lp_hz),
            "lp_type": str(a.lp_type),
            "peq": [band(x) for x in a.peq],
            "mode": str(a.mode),
        } for role, a in res.subs.items()],
        "sub_bus_peq": [band(x) for x in eng.sub_bus_peq],
        "main_extra_ms": float(eng.main_extra_ms),
        "lift_ms": float(res.lift_ms),
        "latency_ms": float(run.latency_ms),
    }


def fill_firs(fam, key, chain, channels):
    for item in chain["mains"]:
        item["fir"] = fam.f64(f"{key}_{item['role']}_fir", channels[item["role"]].fir)


def output_folder():
    # A fresh ASCII-only path keeps generated text deterministic after replacement.
    return Path(tempfile.mkdtemp(prefix="secs_camilla_"))


def replace_folder(text, folder):
    return text.replace(str(folder.resolve()).replace("\\", "/"), FOLDER)


def freeze_written(fam, key, chain, labels, write_call):
    folder = output_folder()
    try:
        paths = write_call(folder)
        yaml = Path(paths[0]).read_text(encoding="ascii")
        files = []
        for path in paths[1:]:
            path = Path(path)
            files.append({"path": path.relative_to(folder).as_posix(),
                          "bytes": fam.blob(f"{key}_{path.stem}", path.read_bytes(), "wav")})
        inputs = {"chain": chain, "labels": labels}
        outputs = {"yaml": replace_folder(yaml, folder),
                   "files": [Path(p).relative_to(folder).as_posix() for p in paths],
                   "coeffs": files,
                   "in_raw_bytes": (folder / "in.raw").stat().st_size if (folder / "in.raw").exists() else None}
        fam.save(key, inputs=inputs, outputs=outputs)
    finally:
        shutil.rmtree(folder)


def computed(fam):
    for case in cases():
        if case.name not in COMPUTED:
            continue
        run = cal.compute(case.project, case.irs, case.settings)
        chain = chain_from(run)
        fill_firs(fam, case.name, chain, run.result.channels)
        labels = {"mode": run.settings.mode, "ultra_ms": float(run.settings.ultra_ms),
                  "latency_ms": float(run.latency_ms), "rc_hi_hz": float(run.rc_hi)}
        freeze_written(fam, case.name, chain, labels,
                       lambda folder, run=run: cal.export_camilladsp(run, folder))


def main(role, peq=(), fir=(1.0,), gain=0.0, delay=0.0, xo=None, top=(), align=0.0):
    return SimpleNamespace(role=role, peq=list(peq), fir=list(fir), gain_db=gain,
                           delay_ms=delay, xo_hz=xo), list(top), align


def sub(role, gain=0.0, delay=0.0, polarity=1, allpass=(), hp=0.0, hp_type="LR24",
        lp=0.0, lp_type="LR24", peq=(), mode="auto"):
    return SimpleNamespace(role=role, gain_db=gain, delay_ms=delay, polarity=polarity,
                           allpass=list(allpass), hp_hz=hp, hp_type=hp_type,
                           lp_hz=lp, lp_type=lp_type, peq=list(peq), mode=mode)


def duck(mains, subs=(), bus=()):
    channels = OrderedDict((m[0].role, m[0]) for m in mains)
    submap = OrderedDict((s.role, s) for s in subs)
    eng = SimpleNamespace(top_peq={m[0].role: m[1] for m in mains},
                          align_delay_ms={m[0].role: m[2] for m in mains},
                          sub_bus_peq=list(bus), main_extra_ms=0.0)
    res = SimpleNamespace(channels=channels, subs=submap, lift_ms=0.0)
    chain = {
        "sample_rate": 48000,
        "mains": [{"role": c.role, "input_peq": [band(x) for x in c.peq], "fir": [float(x) for x in c.fir],
                   "gain_db": float(c.gain_db), "delay_ms": float(c.delay_ms),
                   "xo_hz": None if c.xo_hz is None else float(c.xo_hz),
                   "top_peq": [band(x) for x in top], "align_delay_ms": float(align)}
                  for c, top, align in mains],
        "subs": [{"role": s.role, "gain_db": float(s.gain_db), "delay_ms": float(s.delay_ms),
                  "polarity": int(s.polarity), "allpass": [band(x) for x in s.allpass],
                  "hp_hz": float(s.hp_hz), "hp_type": str(s.hp_type), "lp_hz": float(s.lp_hz),
                  "lp_type": str(s.lp_type), "peq": [band(x) for x in s.peq], "mode": str(s.mode)} for s in subs],
        "sub_bus_peq": [band(x) for x in bus], "main_extra_ms": 0.0, "lift_ms": 0.0, "latency_ms": 0.0,
    }
    return eng, res, chain


def make_case(fam, key, mains, subs=(), bus=(), options=None):
    eng, res, chain = duck(mains, subs, bus)
    options = dict(options or {})
    saved_options = dict(options)
    folder = output_folder()
    try:
        config_dir = folder / "where" / "configs"
        text = cam.make_config(eng, res, folder, options.pop("capture", {"type": "RawFile", "channels": len(mains)}),
                               options.pop("playback", {"type": "File", "channels": len(mains) + len(subs)}),
                               config_dir=config_dir, fir_prefix="edge_", **options)
        coeffs = []
        for path in sorted((folder / "coeffs").glob("edge_*.wav")):
            coeffs.append({"path": path.relative_to(folder).as_posix(),
                           "bytes": fam.blob(f"{key}_{path.stem}", path.read_bytes(), "wav")})
        fam.save(key, inputs={"chain": chain, "out": FOLDER, "config_dir": FOLDER + "/where/configs",
                              "fir_prefix": "edge_", "options": saved_options},
                 outputs={"yaml": replace_folder(text, folder), "coeffs": coeffs})
    finally:
        shutil.rmtree(folder)


def direct_cases(fam):
    make_case(fam, "direct_no_subs", [main("L", peq=[(80.0, 1.0, -2.0)], fir=[1.0, -0.0])])
    make_case(fam, "direct_empty_bus", [main("L")], [sub("SW1")], options={"lfe_gain_db": None})
    make_case(fam, "direct_lfe_none", [main("L", xo=80.0)], [sub("SW1")], options={"lfe_gain_db": None})
    make_case(fam, "direct_lfe_zero", [main("L", xo=80.0)], [sub("SW1")], options={"lfe_gain_db": 0.0})
    capture = OrderedDict([("type", "Alsa"), ("device", "hw:Loopback,1"), ("channels", 3),
                           ("format", "FLOAT32LE"), ("stop_on_rate_change", True)])
    playback = OrderedDict([("type", "CoreAudio"), ("device", "DAC"), ("channels", 2),
                            ("exclusive", False), ("extra", 7.25)])
    make_case(fam, "direct_devices_note", [main("L"), main("R")], options={
        "capture": capture, "playback": playback, "chunksize": 257,
        "note": "ASCII before / 한글 / after", "latency_ms": 12.25, "mode": "edge"})
    make_case(fam, "direct_rounding", [main("L", peq=[(1.23445, 0.123455, -0.00005)],
               gain=-0.0, delay=-1 / 96000, xo=80.0, top=[(1e16, 2.675, -2.675)])],
              [sub("SW1", gain=-0.0, delay=1 / 96000, polarity=-1,
                   allpass=[(2.675, 1.23455)], hp=1e-5, hp_type="BW12",
                   lp=1e16, lp_type="LR48", peq=[(9.99995, 1.00005, -1.23445)], mode="fixed")],
              bus=[(80.0, 0.70715, -0.00015)], options={"lfe_gain_db": 10.0, "lfe_lp_hz": 1e-5})


def write_name_case(fam):
    eng, res, chain = duck([main("L", fir=[1.0, -0.25, 0.0])])
    folder = output_folder()
    try:
        paths = cam.write_config(eng, res, folder, name="한글 / odd:name", note="named")
        fam.save("write_name_nonascii", inputs={"chain": chain, "name": "한글 / odd:name",
                                                "options": {"note": "named"}},
                 outputs={"yaml": replace_folder(Path(paths[0]).read_text(encoding="ascii"), folder),
                          "files": [Path(path).relative_to(folder).as_posix() for path in paths],
                          "coeffs": [{"path": Path(path).relative_to(folder).as_posix(),
                                      "bytes": fam.blob(f"write_name_{Path(path).stem}", Path(path).read_bytes(), "wav")}
                                     for path in paths[1:]],
                          "in_raw_bytes": (folder / "in.raw").stat().st_size})
    finally:
        shutil.rmtree(folder)


def pyfmt(fam):
    values = [0.0, -0.0, 1e-5, 9.999999999999999e-5, 0.0001, 1e15, 1e16,
              2.675, math.nextafter(2.675, -math.inf), math.nextafter(2.675, math.inf),
              1.23445, math.nextafter(1.23445, -math.inf), math.nextafter(1.23445, math.inf),
              5e-324, -5e-324, 1.7976931348623157e308]
    outputs = []
    for x in values:
        outputs.append({"repr": repr(x), "round3": repr(round(x, 3)), "round4": repr(round(x, 4)),
                        "round5": repr(round(x, 5)), "plus_g": f"{x:+g}", "g": f"{x:g}",
                        "dot1": f"{x:.1f}", "dot3": f"{x:.3f}", "plus2": f"{x:+.2f}",
                        "plus3": f"{x:+.3f}"})
    fam.save("pyfmt", inputs={"values": values}, outputs=outputs)


def main_export():
    fam = common.Family("camilla")
    computed(fam)
    direct_cases(fam)
    write_name_case(fam)
    pyfmt(fam)
    fam.finish()
    assert sum(fam.written.values()) <= 6_000_000


if __name__ == "__main__":
    main_export()
