#!/usr/bin/env python3
"""Freeze the SECS 2 project model, role vocabulary and IR WAV format."""
from __future__ import annotations

import io
import json
import math
import tempfile
from pathlib import Path

import numpy as np

import common
from secs import model
from secs.rewapi.session import _read_mono_float_wav, _read_wav_float32, _write_wav_float32

fam = common.Family("project")


def role_row(role: str) -> dict:
    return {
        "role": role,
        "label": model.role_label(role),
        "kind": model.role_kind(role),
        "group": model.role_group(role),
        "code": model.role_code(role),
        "order": model.role_order(role),
        "position": model.ROLE_POSITIONS.get(role),
    }


titles = []
base_titles = list(model.ROLES) + list(model.ROLE_CODES.values()) + list(model.ROLE_ALIASES)
for text in base_titles:
    titles.extend([text, text.lower(), f" {text} ", f"{text} 1", f"{text} ２"])
titles.extend([
    "", "DBA L", "L EQ", "L LF SUM", "LRS EQ 예측", "서브", "서브우퍼", "서브우퍼２",
    "SW 2", "LFE 8", "Custom", "한글", "Ｌ", "L:LF", "L ١", "L १", "L ፩", "L ²", "L ⑴",
    "L ½", "L Ⅻ", "Lः", "L\x1f1", "SUB01", "서브우퍼02",
])
roles_filter = ["L:LF", "R", "SW2", "Tfl", "Hbl"]
fam.save("roles", {"titles": titles, "filter": roles_filter}, {
    "roles": [role_row(r) for r in list(model.ROLES) + ["custom", "L:LF"]],
    "titles": [[t, model.role_from_title(t), model.role_from_title(t, roles_filter)] for t in titles],
})

direct = model.Project.from_preset("2.0")
fam.save("direct_preset", {"preset": "2.0"}, {
    "value": direct.to_dict(),
    "dsp_dataclass_default": model.DspProfile().to_dict(),
})

for preset in model.PRESETS:
    for subs in (0, 1, 2, 4, 8):
        project = model.Project.from_preset(preset)
        project.name = f"{preset} 서라운드"
        project.config.set_sub_count(subs)
        project.config.biamp_default = subs % 2 == 0
        if subs == 4:
            project.config.speakers.append(model.Speaker("custom", "small", [model.Band("FULL", 30)]))
        project.target = model.TargetCurve.presets()["harman"]
        project.dsp = model.DspProfile.presets()["camilla" if subs == 8 else "tesira"]
        value = project.to_dict()
        text = json.dumps(value, ensure_ascii=False, indent=2) + "\n"
        fam.save(f"preset_{preset.replace('.', '_')}_{subs}", {"preset": preset, "subs": subs}, {
            "value": value, "save": fam.blob(f"preset_{preset.replace('.', '_')}_{subs}", text.encode()),
            "validate": project.validate(),
        })

legacy_cases = {
    "v1": {
        "version": 1, "name": "옛 프로젝트", "bc": {"phase": "fir"}, "xo": {"하이트": 90, "프론트": 80},
        "mso": {**model.DEFAULT_MSO, "focus": 63},
        "extra": {"calibration": {"xo": {"하이트": 100}, "xo_rec": {"하이트": 110},
            "art_on": True, "art_low": 35, "art_max_supports": 2,
            "subs_manual": {"SW1": {"art_support": "L", "gain": -2}}}},
        "future": {"순서": [1, 2]},
    },
    "v2_conflicts": {
        "version": 2, "xo": {"하이트": 70}, "extra": {"calibration": {
            "art_on": False, "mimo_on": True, "subs_manual": {"SW1": {"art_support": "R", "mimo_support": "L"}}}},
    },
    "missing": {"name": "minimal", "unknown": 7},
    "v4": {"version": 4, "future": {"flag": True}},
    "v5": {"version": 5, "future": {"flag": True}},
}
for name, data in legacy_cases.items():
    text = json.dumps(data, ensure_ascii=False, indent=2) + "\n"
    project = model.Project.from_dict(json.loads(text))
    saved = json.dumps(project.to_dict(), ensure_ascii=False, indent=2) + "\n"
    fam.save(f"migrate_{name}", {"text": fam.blob(f"migrate_{name}_input", text.encode())}, {
        "value": project.to_dict(), "save": fam.blob(f"migrate_{name}_save", saved.encode())
    })

future_profile = model.DspProfile.from_dict({"profile": "future", "fir_taps": 4096})
fam.save("dsp_future", {"profile": "future", "fir_taps": 4096}, {
    "profile": future_profile.to_dict(),
})

focus_values = ["75", True, None, "NaN", "Infinity", "-Infinity", float("nan"), float("inf"), float("-inf")]
fam.save("focus", {"values": common.plain(focus_values)}, {
    "snapped": [model.snap_focus(value) for value in focus_values],
})

collection_cases = [
    {"curtain": [20, 200, 999], "bass": {"paths": [{}, {}, {}]}},
    {"bass": {"paths": {"L": {}, "R": {}}}},
]
fam.save("collections", {"projects": collection_cases}, {
    "rows": [[list(project.curtain), project.bass_path_count()]
             for project in map(model.Project.from_dict, collection_cases)],
})

invalid_config = model.SpeakerConfig(speakers=[
    model.Speaker("L", "small", [model.Band("NOPE", 1, "x")]),
    model.Speaker("R", "small", [model.Band("SUB", 1, "+")]),
    model.Speaker("L", "small", []),
    model.Speaker("C", "small", [model.Band("FULL", 1, "+")]),
])
fam.save("config_validation", {}, {
    "messages": invalid_config.validate(),
    "groups": list(invalid_config.groups()),
})

for profile_name, profile in model.DspProfile.presets().items():
    rows = []
    for preset in ("2.0", "5.1", "9.1.6"):
        config = model.SpeakerConfig.from_preset(preset, biamp=True)
        for phase in ("minphase", "mixed", "other"):
            for paths in (0, 1, 7, 31):
                rows.append([preset, phase, paths, profile.allocate_taps(config, phase, paths),
                             profile.fir_used(config, phase, paths)])
    fam.save(f"taps_{profile_name}", {"profile": profile.to_dict()}, {"rows": rows})

freqs = np.geomspace(5, 30000, 257)
curves = {**model.TargetCurve.presets(), "edited": model.TargetCurve([(1000, 1), (20, 3), (80, -2), (80, 4)])}
for name, curve in curves.items():
    fam.save(f"target_{name}", {"points": curve.to_dict(), "freqs": fam.f64(f"target_{name}_freqs", freqs)}, {
        "values": fam.f64(f"target_{name}_values", curve.sample(freqs)),
        "edge": [curve.at(x) for x in (0, 5, 20, 80, 1000, 30000, math.inf)],
    })

sort_cases = [
    [(0.0, 1.0), (-0.0, 2.0)],
    [(float("nan"), 1.0), (2.0, 2.0), (1.0, 3.0)],
]
fam.save("target_sort", {"points": common.plain(sort_cases)}, {
    "points": [common.plain(model.TargetCurve(points).points) for points in sort_cases],
})

tricky = {
    "floats": [0.0, -0.0, 1e-7, 1e-6, 1e-5, 1e15, 1e16, 100000.0, 1.2345678901234567,
               float("nan"), float("inf"), float("-inf")],
    "ints": [0, -1, 2**53, 2**80],
    "strings": ["", "quote \\\"", "line\n\ttab\b", "한글", "😀", " "],
    "empty": [[], {}],
}
finite_tricky = {**tricky, "floats": tricky["floats"][:-3]}
for index, (value, finite) in enumerate([
    (tricky, finite_tricky),
    (tricky["floats"], tricky["floats"][:-3]),
    ({"z": 1, "a": 2}, {"z": 1, "a": 2}),
]):
    text = json.dumps(value, ensure_ascii=False, indent=2) + "\n"
    finite_text = json.dumps(finite, ensure_ascii=False, indent=2) + "\n"
    fam.save(f"json_{index}", {}, {
        "save": fam.blob(f"json_{index}", text.encode()),
        "finite_save": fam.blob(f"json_{index}_finite", finite_text.encode()),
    })

json_round_trips = [
    {"future": "\\/"},
    {"future": {"__secs_project_nonfinite_0__": "NaN"}},
    {"future": {"$serde_json::private::Number": "123"}},
]
for index, value in enumerate(json_round_trips):
    text = json.dumps(value, ensure_ascii=False, indent=2) + "\n"
    project = model.Project.from_dict(json.loads(text))
    saved = json.dumps(project.to_dict(), ensure_ascii=False, indent=2) + "\n"
    fam.save(f"json_round_trip_{index}", {
        "text": fam.blob(f"json_round_trip_{index}_input", text.encode()),
    }, {
        "value": project.to_dict(),
        "save": fam.blob(f"json_round_trip_{index}_save", saved.encode()),
    })

zero_routes = {
    "version": 3,
    "curtain": [-0, -0.0],
    "target": [[-0, -0.0]],
    "xo": {"탑": -0, "하이트": -0.0},
}
zero_project = model.Project.from_dict(json.loads(json.dumps(zero_routes)))
zero_saved = json.dumps(zero_project.to_dict(), ensure_ascii=False, indent=2) + "\n"
fam.save("typed_negative_zero", {"value": zero_routes}, {
    "value": zero_project.to_dict(),
    "save": fam.blob("typed_negative_zero_save", zero_saved.encode()),
})

with tempfile.TemporaryDirectory() as directory:
    for index, (rate, samples) in enumerate([
        (48000, np.array([], dtype=np.float64)),
        (44100, np.array([0, -0.0, 1, -1, 1 / 3], dtype=np.float64)),
        (96000, np.linspace(-2, 2, 65, dtype=np.float64)),
    ]):
        path = Path(directory) / f"{index}.wav"
        _write_wav_float32(path, samples, rate)
        a, ar = _read_wav_float32(path)
        b, br = _read_mono_float_wav(path)
        fam.save(f"wav_{index}", {"rate": rate, "samples": common.plain(samples)}, {
            "wav": fam.blob(f"wav_{index}", path.read_bytes(), "wav"),
            "minimal": {"rate": ar, "samples": common.plain(a)},
            "fast": {"rate": br, "samples": common.plain(b)},
        })

fam.finish()
