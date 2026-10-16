"""Freeze filter-value text and Equalizer APO exports from SECS 2."""
from __future__ import annotations

import copy
import hashlib
import shutil
import tempfile
from collections import OrderedDict
from pathlib import Path
from types import SimpleNamespace

import numpy as np

import common
import export_camilla
from engine_cases import cases
from secs import calibration as cal
from secs import export_filters as filters

STAMP = "2026-10-16 12:34"
N = 1 << 16
F = np.fft.rfftfreq(N, 1.0 / filters.SAMPLE_RATE)
SAMPLE = np.unique(np.r_[np.flatnonzero(F <= 2000.0), np.arange(0, F.size, 127)])


def band(value):
    return [float(x) for x in value]


def main(role, peq=(), fir=(1.0,), gain=0.0, delay=0.0, xo=None, top=(), align=0.0):
    return SimpleNamespace(role=role, peq=list(peq), fir=np.asarray(fir, float),
                           gain_db=gain, delay_ms=delay, xo_hz=xo), list(top), align


def sub(role, gain=0.0, delay=0.0, polarity=1, allpass=(), hp=0.0, hp_type="LR24",
        lp=0.0, lp_type="LR24", peq=(), mode="auto"):
    return SimpleNamespace(role=role, gain_db=gain, delay_ms=delay, polarity=polarity,
                           allpass=list(allpass), hp_hz=hp, hp_type=hp_type,
                           lp_hz=lp, lp_type=lp_type, peq=list(peq), mode=mode)


def duck(mains, subs=(), bus=(), main_extra=0.0, latency=0.0):
    channels = OrderedDict((m[0].role, m[0]) for m in mains)
    submap = OrderedDict((s.role, s) for s in subs)
    eng = SimpleNamespace(top_peq={m[0].role: list(m[1]) for m in mains},
                          align_delay_ms={m[0].role: m[2] for m in mains},
                          sub_bus_peq=list(bus), main_extra_ms=main_extra)
    res = SimpleNamespace(channels=channels, subs=submap, lift_ms=0.0)
    chain = {
        "sample_rate": 48000,
        "mains": [{"role": c.role, "input_peq": [band(x) for x in c.peq],
                   "fir": [float(x) for x in c.fir], "gain_db": float(c.gain_db),
                   "delay_ms": float(c.delay_ms), "xo_hz": None if c.xo_hz is None else float(c.xo_hz),
                   "top_peq": [band(x) for x in top], "align_delay_ms": float(align)}
                  for c, top, align in mains],
        "subs": [{"role": s.role, "gain_db": float(s.gain_db), "delay_ms": float(s.delay_ms),
                  "polarity": int(s.polarity), "allpass": [band(x) for x in s.allpass],
                  "hp_hz": float(s.hp_hz), "hp_type": str(s.hp_type), "lp_hz": float(s.lp_hz),
                  "lp_type": str(s.lp_type), "peq": [band(x) for x in s.peq], "mode": str(s.mode)}
                 for s in subs],
        "sub_bus_peq": [band(x) for x in bus], "main_extra_ms": float(main_extra),
        "lift_ms": 0.0, "latency_ms": float(latency),
    }
    return eng, res, chain


def op_value(fam, key, op):
    kind = op[0]
    fixed = filters.is_fixed(op)
    if kind == "peq":
        return {"kind": kind, "frequency": float(op[1]), "q": float(op[2]),
                "gain_db": float(op[3]), "fixed": fixed}
    if kind == "fir":
        return {"kind": kind, "name": op[1], "taps": fam.f64(f"{key}_{op[1]}", op[2]), "fixed": fixed}
    if kind == "gain":
        return {"kind": kind, "db": float(op[1]), "fixed": fixed}
    if kind == "invert":
        return {"kind": kind, "fixed": fixed}
    if kind in ("hp", "lp"):
        return {"kind": kind, "frequency": float(op[1]), "type": str(op[2]), "fixed": fixed}
    if kind == "ap":
        return {"kind": kind, "frequency": float(op[1]), "q": float(op[2]), "fixed": fixed}
    if kind == "delay":
        return {"kind": kind, "ms": float(op[1]), "fixed": fixed}
    raise AssertionError(kind)


def plan_value(fam, key, plan):
    ops = lambda suffix, value: [op_value(fam, f"{key}_{suffix}_{i}", op) for i, op in enumerate(value)]
    return {
        "inputs": list(plan.inputs), "outputs": list(plan.outputs),
        "input_ops": [[role, ops(f"input_{role}", value)] for role, value in plan.input_ops.items()],
        "main_ops": [[role, ops(f"main_{role}", value)] for role, value in plan.main_ops.items()],
        "send_ops": [[role, ops(f"send_{role}", value)] for role, value in plan.send_ops.items()],
        "lfe_ops": ops("lfe", plan.lfe_ops),
        "subs": [[role, {"sources": list(feed.sources), "gain_db": float(feed.gain_db),
                          "polarity": int(feed.polarity), "ops": ops(f"sub_{role}", feed.ops)}]
                 for role, feed in plan.subs.items()],
        "mimo": [{"input": feed.input, "support": feed.support,
                  "ops": ops(f"mimo_{i}", feed.ops)} for i, feed in enumerate(plan.mimo)],
    }


def response_value(fam, key, plan):
    outputs = []
    for source in plan.inputs:
        responses = filters.plan_responses(plan, F, source)
        outputs.append([source, [[role, fam.c128(f"{key}_{source}_{role}", value[SAMPLE])]
                                 for role, value in responses.items()]])
    return {"indices": fam.f64(f"{key}_indices", SAMPLE), "responses": outputs}


def mimo_value(fam, key, eng, mimo, iir, fsiso):
    if mimo is None:
        return None
    paths = []
    for index, (source, support, value, low, delay) in enumerate(filters.mimo_paths(eng, mimo, iir)):
        if isinstance(value, np.ndarray):
            filt = {"Fir": {"taps_48k": fam.f64(
                f"{key}_mimo_{index}", filters.fir_to_48k(value, getattr(mimo, "fs_low", 600)))}}
        else:
            filt = {"Iir": {"gain_db": float(value.gain_db), "delay_ms": float(value.delay_ms),
                            "polarity": int(value.polarity), "peq": [band(x) for x in value.peq],
                            "allpass": [band(x) for x in value.allpass]}}
        paths.append({"input": source, "support": support, "filter": filt,
                      "low_hz": float(low), "delay_ms": float(delay)})
    return {"fsiso": float(fsiso), "paths": paths}


def freeze_files(fam, key, folder, paths):
    return {
        "paths": [Path(path).relative_to(folder).as_posix() for path in paths],
        "files": [{"path": path.relative_to(folder).as_posix(),
                   "bytes": fam.blob(f"{key}_{i}_{path.name}", path.read_bytes(), "bin")}
                  for i, path in enumerate(map(Path, paths))],
    }


def freeze_wrapper_files(fam, key, folder, paths):
    files = []
    for index, raw_path in enumerate(paths):
        path = Path(raw_path)
        data = path.read_bytes()
        item = {"path": path.relative_to(folder).as_posix(), "length": len(data),
                "sha256": hashlib.sha256(data).hexdigest()}
        if path.suffix.lower() == ".txt":
            item["bytes"] = fam.blob(f"{key}_{index}_{path.name}", data, "bin")
        files.append(item)
    return {"paths": [item["path"] for item in files], "files": files}


def freeze_case(fam, key, mains, subs=(), bus=(), *, mode="mixed", ultra_ms=200.0,
                label="혼합 위상 모드", latency=12.25, rc_hi=321.0,
                rc_pieces=None, lfe_gain=10.0, lfe_lp=120.0, note="ASCII before / 한글 / after",
                mimo=None, iir=None, fsiso=150.0, low_fir_fs=600.0):
    eng, res, chain = duck(mains, subs, bus, latency=latency)
    plan = filters.make_plan(eng, res, mimo=mimo, iir=iir, fsiso=fsiso,
                             lfe_gain_db=lfe_gain, lfe_lp_hz=lfe_lp)
    plan_json = plan_value(fam, key, plan)
    firs = {stem: f"edge_{stem}.wav" for stem in plan.firs()}
    in_num, out_num = filters.windows_channels(plan)
    preamp = float(np.ceil(filters.headroom_db(plan) * 2) / 2)
    direct = {
        "eqapo_filters": filters.eqapo_filters(plan, firs, note),
        "eqapo_config": filters.eqapo_config(plan, "edge\\edge.txt", preamp, in_num, out_num, latency),
        "eqapo_guide": filters.eqapo_guide(plan, in_num, out_num, "edge", label, latency, preamp),
        "filter_text": filters.filter_text(plan, firs, f"SECS 2 보정 필터 — {label}",
                                           [f"만든 때 {STAMP} · 전체 지연 {latency:.1f} ms · RC 상한 {rc_hi:.0f} Hz",
                                            "샘플레이트 48000 Hz(FIR wav · 딜레이 샘플 수 기준)"]),
    }
    run = {"mode": mode, "ultra_ms": float(ultra_ms), "label": label, "latency_ms": float(latency),
           "rc_hi_hz": float(rc_hi), "rc_pieces": list(rc_pieces or [f"RC 상한 {rc_hi:.0f} Hz"]),
           "stamp": STAMP}
    mimo_json = mimo_value(fam, key, eng, mimo, iir, fsiso)
    written = {}
    root = Path(tempfile.mkdtemp(prefix="secs_filters_"))
    old_stamp = filters.stamp
    filters.stamp = lambda: STAMP
    try:
        if not subs and mimo is None:
            text_dir = root / "text"
            paths = filters.write_text(plan, text_dir, f"SECS 필터 {label}.txt",
                                       f"secs_{mode + (f'{ultra_ms:g}' if mode == 'ultra' else '')}",
                                       f"SECS 2 보정 필터 — {label}",
                                       [f"만든 때 {STAMP} · 전체 지연 {latency:.1f} ms · " + " · ".join(run["rc_pieces"]),
                                        "샘플레이트 48000 Hz(FIR wav · 딜레이 샘플 수 기준)"])
            written["text"] = freeze_files(fam, key + "_text", text_dir, paths)
        if mimo is None:
            eq_dir = root / "eqapo"
            name_mode = mode + (f"{ultra_ms:g}" if mode == "ultra" else "")
            paths = filters.write_eqapo(plan, eq_dir, f"secs_{name_mode}", mode_label=label,
                                        note=f"mode {name_mode}, total latency {latency:.1f} ms, RC upper {rc_hi:.0f} Hz; no MIMO",
                                        latency_ms=latency)
            written["eqapo"] = freeze_files(fam, key + "_eqapo", eq_dir, paths)
    finally:
        filters.stamp = old_stamp
        shutil.rmtree(root)
    fam.save(key, inputs={"chain": chain, "mimo": mimo_json, "run": run,
                          "lfe_gain_db": lfe_gain, "lfe_lp_hz": float(lfe_lp),
                          "fsiso": float(fsiso)},
             outputs={"plan": plan_json, "headroom_db": float(filters.headroom_db(plan)),
                      "windows_channels": {"inputs": list(in_num.items()), "outputs": list(out_num.items())},
                      "direct": direct, "response": response_value(fam, key, plan), "written": written})


def run_value(run):
    return {"mode": run.settings.mode, "ultra_ms": float(run.settings.ultra_ms),
            "label": run.settings.label(), "latency_ms": float(run.latency_ms),
            "rc_hi_hz": float(run.rc_hi), "rc_pieces": cal.rc_pieces(run), "stamp": STAMP}


def rc_case(source, mode):
    project = copy.deepcopy(source.project)
    project.config.set_sub_count(0)
    project.config.renumber()
    project.config.preset = "custom"
    roles = {speaker.role for speaker in project.config.speakers}
    irs = {pos: {role: value for role, value in by_role.items() if role in roles}
           for pos, by_role in source.irs.items()}
    settings = cal.CalibrationSettings(mode=mode,
                                       room_volume_m3=source.settings.room_volume_m3)
    return cal.compute(project, irs, settings)


def computed_case(fam, key, run, *, scope_error=False):
    chain = export_camilla.chain_from(run)
    export_camilla.fill_firs(fam, key, chain, run.result.channels)
    root = Path(tempfile.mkdtemp(prefix="secs_filters_computed_"))
    old_stamp = filters.stamp
    filters.stamp = lambda: STAMP
    written = {}
    blocked = None
    try:
        eqapo = root / "eqapo"
        written["eqapo"] = freeze_wrapper_files(
            fam, f"{key}_eqapo", eqapo, cal.export_eqapo(run, eqapo))
        if not run.result.subs:
            text = root / "text"
            written["text"] = freeze_wrapper_files(
                fam, f"{key}_text", text, cal.export_filter_text(run, text))
        if scope_error:
            folder = root / "blocked_text"
            try:
                cal.export_filter_text(run, folder)
            except cal.CalibrationError as error:
                blocked = {"type": type(error).__name__, "message": str(error),
                           "folder_exists": folder.exists()}
            else:
                raise AssertionError("subwoofer filter-text export was not blocked")
    finally:
        filters.stamp = old_stamp
        shutil.rmtree(root)
    fam.save(key, inputs={"chain": chain, "mimo": None, "run": run_value(run)},
             outputs={"written": written, "filter_text_error": blocked})


def computed_cases(fam):
    shared = {case.name: case for case in cases()}
    for name in ("dolby", "dolby_infra", "dolby_ultra"):
        case = shared[name]
        computed_case(fam, f"computed_{name}",
                      cal.compute(case.project, case.irs, case.settings),
                      scope_error=name == "dolby")
    room = shared["room"]
    computed_case(fam, "computed_room_rc_mixed", rc_case(room, "mixed"))
    computed_case(fam, "computed_room_rc_zero", rc_case(room, "zero"))


def direct_cases(fam):
    rounding_main = main("L", peq=[(1.23445, 0.123455, -0.00005)], fir=[1.0, -0.0],
                         gain=-0.0, delay=-1 / 96000, xo=80.0,
                         top=[(1e16, 2.675, -2.675)])
    freeze_case(fam, "rc_mixed", [main("L", peq=[(80.0, 1.0, -2.0)],
                                            fir=[0.5, -0.25, 0.125], gain=1.25, delay=2.675),
                                        main("R", peq=[(120.0, 0.7, 1.5)],
                                             fir=[1.0, 0.0], gain=-1.0, delay=0.0)])
    freeze_case(fam, "rc_zero", [main("L", peq=[(63.0, 1.2, -4.0)], gain=0.0, delay=0.0)],
                mode="zero", label="최소 지연 모드", latency=0.0)
    freeze_case(fam, "ba_edges", [rounding_main, main("Tml", xo=None)],
                [sub("SW1", gain=-0.0, delay=0.0, polarity=-1, allpass=[(2.675, 1.23455)],
                     hp=1e-5, hp_type="BW12", lp=1e16, lp_type="LR48",
                     peq=[(9.99995, 1.00005, -1.23445)], mode="fixed"),
                 sub("SW2", hp=22.0, hp_type="LR12", lp=180.0, lp_type="BW24"),
                 sub("SW3")], bus=[(80.0, 0.70715, -0.00015)])
    freeze_case(fam, "no_lfe_empty_source", [main("L")], [sub("SW1")], lfe_gain=None)
    freeze_case(fam, "ultra_name", [main("L", fir=[0.75, 0.25])], mode="ultra", ultra_ms=120.0,
                label="초고지연 모드 120 ms")

    mimo = SimpleNamespace(ranges={"SW1": (25.0, 150.0)}, fs_low=600.0,
                           supports=OrderedDict([("L", OrderedDict([("SW1", np.array([1.0, -0.25, 0.125]))]))]))
    freeze_case(fam, "mimo_fir", [main("L", align=0.25)], [sub("SW1", hp=18.0, hp_type="BW12")],
                mimo=mimo, iir=None, fsiso=150.0)
    support = SimpleNamespace(gain_db=-6.0, delay_ms=3.0, polarity=-1,
                              peq=[(45.0, 4.0, -3.0)], allpass=[(60.0, 0.7)])
    iir = OrderedDict([("L", OrderedDict([("SW1", support)]))])
    freeze_case(fam, "mimo_iir", [main("L", align=0.25)], [sub("SW1", lp=160.0, lp_type="LR12")],
                mimo=SimpleNamespace(ranges={"SW1": (25.0, 150.0)}), iir=iir, fsiso=150.0)


def main_export():
    fam = common.Family("filters")
    direct_cases(fam)
    computed_cases(fam)
    fam.finish()
    assert sum(fam.written.values()) <= 6_000_000


if __name__ == "__main__":
    main_export()
