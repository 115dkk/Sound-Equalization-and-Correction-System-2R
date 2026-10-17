"""Freeze MIMO CamillaDSP and REW export outputs for two engine cases."""
from __future__ import annotations

import hashlib
import json
import shutil
import tempfile
from pathlib import Path

import common
import numpy as np
from engine_cases import cases
from scipy.io import wavfile
from secs import calibration as cal
from secs import correction_engine as ce
from secs import export_camilladsp as cam

NAMES = {"room_dba", "room_filters"}
N_FFT = 1 << 17
NOW_MS = 1792195538802
FOLDER = "{OUT}"


def replay_run(case):
    """Replay engine_design's fit decisions in the upstream call order."""
    outputs = []
    directory = common.GOLDENS / "engine_design"
    for path in sorted(directory.glob(case.name + "_fits_*.json")):
        fixture = json.loads(path.read_text())
        for request, output in zip(fixture["inputs"]["calls"], fixture["outputs"]["calls"]):
            if "reference" not in request:
                outputs.append(output["result"])
                continue
            ref = request["reference"]
            source = json.loads((common.GOLDENS / ref["family"] / (ref["fixture"] + ".json")).read_text())
            if "call" in ref:
                outputs.append(source["outputs"]["calls"][ref["call"]]["result"])
            else:
                outputs.append(source["outputs"]["candidates"][ref["candidate"]]["fits"][ref["fit"]]["result"])
    index = [0]
    original = ce.fit_peq

    def fit(*_args, **_kwargs):
        result = outputs[index[0]]
        index[0] += 1
        return [tuple(row) for row in result]

    ce.fit_peq = fit
    try:
        run = cal.compute(case.project, case.irs, case.settings)
    finally:
        ce.fit_peq = original
    assert index[0] == len(outputs), (case.name, index[0], len(outputs))
    return run


def band(value):
    return [float(x) for x in value]


def chain_value(run):
    eng, res = run.engine, run.result
    return {
        "sample_rate": 48000,
        "mains": [{
            "role": role,
            "input_peq": [band(x) for x in channel.peq],
            "fir": [float(x) for x in channel.fir],
            "gain_db": float(channel.gain_db),
            "delay_ms": float(channel.delay_ms),
            "xo_hz": None if channel.xo_hz is None else float(channel.xo_hz),
            "top_peq": [band(x) for x in eng.top_peq.get(role, [])],
            "align_delay_ms": float(eng.align_delay_ms[role]),
        } for role, channel in res.channels.items()],
        "subs": [{
            "role": role,
            "gain_db": float(sub.gain_db),
            "delay_ms": float(sub.delay_ms),
            "polarity": int(sub.polarity),
            "allpass": [band(x) for x in sub.allpass],
            "hp_hz": float(sub.hp_hz),
            "hp_type": str(sub.hp_type),
            "lp_hz": float(sub.lp_hz),
            "lp_type": str(sub.lp_type),
            "peq": [band(x) for x in sub.peq],
            "mode": str(sub.mode),
        } for role, sub in res.subs.items()],
        "sub_bus_peq": [band(x) for x in eng.sub_bus_peq],
        "main_extra_ms": float(eng.main_extra_ms),
        "lift_ms": float(res.lift_ms),
        "latency_ms": float(run.latency_ms),
    }


def sample_indexes(length):
    return sorted(set([0, 1, 2, length // 2, length - 3, length - 2, length - 1]
                      + list(range(0, length, 4096))))


def pack_runs(data):
    """Pack byte planes as literal and repeated-byte packets (length 1..128)."""
    out = bytearray()
    i = 0
    while i < len(data):
        end = i + 1
        while end < len(data) and data[end] == data[i] and end - i < 128:
            end += 1
        if end - i >= 3:
            out.extend((0x80 | (end - i - 1), data[i]))
            i = end
            continue
        start = i
        i += 1
        while i < len(data) and i - start < 128:
            end = i + 1
            while end < len(data) and data[end] == data[i] and end - i < 3:
                end += 1
            if end - i >= 3:
                break
            i += 1
        out.append(i - start - 1)
        out.extend(data[start:i])
    return bytes(out)


def packed_f32_blob(family, key, values):
    """Losslessly store float32 bits after byte-plane XOR and small RLE."""
    values = np.ascontiguousarray(values, dtype="<f4")
    planes = values.view(np.uint8).reshape(-1, 4).T.copy()
    delta = planes.copy()
    delta[:, 1:] = np.bitwise_xor(planes[:, 1:], planes[:, :-1])
    descriptor = family.blob(key, pack_runs(delta.tobytes()), "f32xrle")
    descriptor["samples_len"] = len(values)
    descriptor["samples"] = [[i, float(values[i])] for i in sample_indexes(len(values))]
    return descriptor


def applied_value(family, key, role, response):
    response = np.ascontiguousarray(response, dtype="<c16")
    indices = sample_indexes(len(response))
    ir = np.fft.irfft(response, N_FFT).astype("<f4")
    return {
        "role": role,
        "length": len(response),
        "sha256_c128": hashlib.sha256(response.tobytes()).hexdigest(),
        "samples": [[i, float(response[i].real), float(response[i].imag)] for i in indices],
        "ir_f32": packed_f32_blob(family, key + "_ir", ir),
    }


def paths_value(family, key, run, mimo_run):
    is_fir = cal.mimo_kind(mimo_run) == "FIR"
    raw = cam.mimo_paths(run.engine, mimo_run.design, None if is_fir else mimo_run.iir)
    paths = []
    for index, (input_role, support, support_filter, low_hz, delay_ms) in enumerate(raw):
        if is_fir:
            taps = cam.fir_to_48k(support_filter, mimo_run.design.fs_low)
            filter_value = {"Fir": {
                "taps_48k": family.f64(f"{key}_path_{index}", taps),
            }}
        else:
            filter_value = {"Iir": {
                "gain_db": float(support_filter.gain_db),
                "delay_ms": float(support_filter.delay_ms),
                "polarity": int(support_filter.polarity),
                "peq": [band(x) for x in support_filter.peq],
                "allpass": [band(x) for x in support_filter.allpass],
            }}
        paths.append({
            "input": input_role,
            "support": support,
            "filter": filter_value,
            "low_hz": float(low_hz),
            "delay_ms": float(delay_ms),
        })
    return {"fsiso": float(mimo_run.fsiso), "paths": paths}


def item_value(item):
    samples = np.ascontiguousarray(item.samples, dtype="<f4")
    return {
        "title": item.title,
        "notes": item.notes,
        "sample_rate_hz": item.sample_rate_hz,
        "start_time_s": item.start_time_s,
        "length": len(samples),
        "samples_sha256": hashlib.sha256(samples.tobytes()).hexdigest(),
        "samples": [[i, float(samples[i])] for i in sample_indexes(len(samples))],
    }


def normalize_folder(text, folder):
    return text.replace(str(folder.resolve()).replace("\\", "/"), FOLDER)


def export_value(family, key, run, mimo_run):
    folder = Path(tempfile.mkdtemp(prefix="secs_export_mimo_"))
    try:
        written = cal.export_camilladsp(run, folder, mimo_run)
        yaml = normalize_folder(Path(written[0]).read_text(encoding="ascii"), folder)
        wavs = []
        for path in written[1:]:
            path = Path(path)
            if "_mimo_" not in path.name:
                continue
            sample_rate, samples = wavfile.read(path)
            samples = np.ascontiguousarray(samples, dtype="<f4")
            wavs.append({
                "path": path.relative_to(folder).as_posix(),
                "sample_rate_hz": int(sample_rate),
                "frames": len(samples),
                "samples_sha256": hashlib.sha256(samples.tobytes()).hexdigest(),
                "samples": [[i, float(samples[i])] for i in sample_indexes(len(samples))],
            })
        paths = paths_value(family, key, run, mimo_run)
        roles = {path["input"] for path in paths["paths"]}
        applied = [applied_value(family, key + "_" + role, role, mimo_run.applied[role])
                   for role in run.engine.chain if role in roles]
        staged = [item_value(item) for item in cal.stage_items(run, mimo_run)
                  if "MIMO 적용" in item.title]
        before_after = [item_value(item) for item in cal.before_after_items(run, mimo_run)
                        if "+ MIMO" in item.title]
        return {
            "kind": cal.mimo_kind(mimo_run),
            "fsiso": float(mimo_run.fsiso),
            "cap_text": cal.mimo_cap_text(mimo_run),
            "paths": paths,
            "applied": applied,
            "yaml": yaml,
            "wavs": wavs,
            "staged": staged,
            "before_after": before_after,
        }
    finally:
        shutil.rmtree(folder)


def main():
    import time
    original_time = time.time
    time.time = lambda: NOW_MS / 1000.0
    common.check_libm()
    family = common.Family("export_mimo")
    for case in cases():
        if case.name not in NAMES:
            continue
        run = replay_run(case)
        chain = chain_value(run)
        for main in chain["mains"]:
            main["fir"] = family.f64(f"{case.name}_{main['role']}_fir", main["fir"])
        labels = {
            "mode": run.settings.mode,
            "ultra_ms": float(run.settings.ultra_ms),
            "latency_ms": float(run.latency_ms),
            "rc_hi_hz": float(run.rc_hi),
        }
        align_delay_ms = {
            role: float(value) for role, value in run.engine.align_delay_ms.items()
        }
        for fir in (False, True):
            kind = "fir" if fir else "iir"
            print(f"compute {case.name} {kind}", flush=True)
            mimo_run = cal.compute_mimo(run, fir=fir, workers=1)
            key = f"{case.name}_{kind}"
            family.save(
                key,
                inputs={
                    "chain": chain,
                    "labels": labels,
                    "align_delay_ms": align_delay_ms,
                },
                outputs=export_value(family, key, run, mimo_run),
            )
    family.finish()
    time.time = original_time
    assert sum(family.written.values()) <= 8 * 1024 * 1024


if __name__ == "__main__":
    main()
