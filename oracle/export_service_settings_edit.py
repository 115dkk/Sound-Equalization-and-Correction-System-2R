#!/usr/bin/env python3
"""Replay upstream's actual settings/card handlers without importing Qt.

Source: src/secs/calibration.py and wizard/step_calibration.py (GPL-3.0-only).
The tiny controls below model spin-box bounds; handler bodies are compiled
unchanged from the pinned upstream tree, not reimplemented by this exporter.
"""
from __future__ import annotations

import ast
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace

import common
from secs import calibration as cal
from secs.model import Project

fam = common.Family("service_settings_edit")
source = Path(__file__).parents[1] / "src/secs/wizard/step_calibration.py"
module = ast.parse(source.read_text(encoding="utf-8"), filename=str(source))
cls = next(n for n in module.body if isinstance(n, ast.ClassDef) and n.name == "StepCalibration")
names = {
    "_on_volume", "_on_dims", "set_sub_mode", "_on_sub_value", "_on_sub_band_type",
    "infra_state", "set_infra", "set_xo", "_on_ultra_ms", "_on_rc_lo",
    "_on_rc_hi_auto", "_on_rc_hi", "_on_boost", "_on_xo_peq", "_on_infra_boost",
    "_on_sub_main_delay", "_on_mimo_on", "_on_fsiso", "_on_mimo_low_auto",
    "_on_mimo_low", "_on_mimo_max", "_on_mimo_fir",
}
namespace = {"cal": cal}
exec(compile(ast.Module(body=[n for n in cls.body if isinstance(n, ast.FunctionDef) and n.name in names],
                        type_ignores=[]), str(source), "exec"), namespace)


class Control:
    def __init__(self, value):
        self.data = value

    def value(self):
        return self.data

    def currentData(self):
        return self.data

    def isChecked(self):
        return self.data


class Screen:
    _syncing = False
    sub_role = "SW1"

    def __init__(self, settings):
        self.settings = settings
        self.roles = ["SW1", "SW2", "SW3", "SW4"]

    def _changed_setting(self):
        pass

    def sub_roles(self):
        return self.roles

    def xo_group_names(self):
        return ["프론트", "센터"]


for name in names:
    setattr(Screen, name, namespace[name])


def clamp(value, bounds):
    return min(max(value, bounds[0]), bounds[1])


def apply(screen, edit):
    s = screen.settings
    kind = edit["kind"]
    simple = {
        "ultra_ms": ("_on_ultra_ms", "ms", cal.ULTRA_MS_RANGE),
        "rc_lo": ("_on_rc_lo", "hz", (10, 200)),
        "room_volume": ("_on_volume", "m3", (0, 100000)),
        "boost_max": ("_on_boost", "db", cal.BOOST_RANGE),
        "infra_boost": ("_on_infra_boost", "db", cal.INFRA_BOOST_RANGE),
        "main_delay": ("_on_sub_main_delay", "ms", cal.SUB_MAIN_DELAY_RANGE),
        "fsiso": ("_on_fsiso", "hz", cal.FSISO_RANGE),
        "mimo_max": ("_on_mimo_max", "count", cal.MIMO_MAX_SUPPORTS_RANGE),
    }
    if kind in simple:
        method, field, bounds = simple[kind]
        value = clamp(edit[field], bounds)
        getattr(screen, method)(round(value) if kind == "mimo_max" else value)
    elif kind == "mode":
        s.mode = edit["mode"]
    elif kind in ("mimo_on", "mimo_fir"):
        getattr(screen, "_on_" + kind)(edit["on"])
    elif kind in ("rc_hi", "mimo_low"):
        bounds = cal.RC_HI_RANGE if kind == "rc_hi" else (15, 150)
        setattr(screen, kind + "_spin", Control(clamp(edit["hz"] or bounds[0], bounds)))
        getattr(screen, "_on_" + kind + "_auto")(edit["hz"] is None)
    elif kind == "room_dims":
        screen.dim_spins = [Control(clamp(v, (0, 100))) for v in edit["dims"]]
        screen._on_dims(0)
    elif kind == "xo":
        screen.set_xo(edit["group"], edit["hz"])
    elif kind == "xo_peq":
        screen.xo_peq_check = Control(edit.get("on", s.xo_peq_on))
        screen.xo_peq_lo_spin = Control(clamp(edit.get("lo", s.xo_peq_lo), cal.XO_PEQ_RANGE))
        screen.xo_peq_hi_spin = Control(clamp(edit.get("hi", s.xo_peq_hi), cal.XO_PEQ_RANGE))
        screen._on_xo_peq()
    elif kind == "infra":
        screen.roles = edit["subs"]
        screen.set_infra(edit["role"] or "", edit["hz"], edit["type"])
    elif kind == "sub_mode":
        screen.set_sub_mode(edit["role"], edit["mode"])
    elif kind == "sub_layout":
        s.set_layout(edit["bundle"], kind=edit["layout"])
    elif kind == "rear_trim":
        s.set_layout(edit["bundle"], rear_trim_ms=edit["ms"])
    elif kind == "sub":
        screen.sub_role = edit["role"]
        screen._on_sub_value(**edit["patch"])
    elif kind == "sub_band_type":
        screen.sub_role = edit["role"]
        screen.sub_band_combos = {edit["band"]: Control(edit["type"])}
        screen._on_sub_band_type(edit["band"])
    else:
        raise ValueError(kind)


def record(name, edits):
    project = Project.from_preset("2.1")
    project.config.set_sub_count(4)
    screen = Screen(cal.CalibrationSettings())
    results = []
    for edit in edits:
        apply(screen, edit)
        screen.settings.store(project)
        # The service returns the same normalized settings as a subsequent get.
        screen.settings = cal.CalibrationSettings.from_project(project)
        results.append({"settings": asdict(screen.settings),
                        "filter_warnings": cal.sub_filter_warnings(screen.settings, screen.roles)})
    fam.save(name, {"edits": edits}, {"steps": results})


scalar = []
for kind, field in [("ultra_ms", "ms"), ("rc_lo", "hz"), ("rc_hi", "hz"),
                    ("room_volume", "m3"), ("boost_max", "db"), ("infra_boost", "db"),
                    ("main_delay", "ms"), ("fsiso", "hz"), ("mimo_low", "hz"), ("mimo_max", "count")]:
    for value in (-100, 1, 1000000):
        scalar.append({"kind": kind, field: value})
scalar += [{"kind": k, "hz": None} for k in ("rc_hi", "mimo_low")]
scalar += [{"kind": "mode", "mode": mode} for mode in ("zero", "mixed", "linear", "ultra")]
scalar += [{"kind": k, "on": on} for k in ("mimo_on", "mimo_fir") for on in (True, False)]
scalar += [{"kind": "xo", "group": "프론트", "hz": hz} for hz in (0, 40, 80.5, 81.5, 1000)]
scalar += [{"kind": "xo", "group": "unknown", "hz": 90}]
scalar += [{"kind": "xo_peq", "on": False, "lo": -1, "hi": 1001},
           {"kind": "xo_peq", "on": True}, {"kind": "xo_peq", "lo": 120},
           {"kind": "mimo_max", "count": 2.5}, {"kind": "mimo_max", "count": 3.5}]
record("limits", scalar)
record("room", [
    {"kind": "room_dims", "dims": [4, 5, 2.5]},
    {"kind": "room_volume", "m3": 50.5},
    {"kind": "room_volume", "m3": 50.6},
    {"kind": "room_dims", "dims": [4, 5, 2.5]},
    {"kind": "room_dims", "dims": [0, 5, 2.5]},
    {"kind": "room_dims", "dims": [0.49, 5, 2.5]},
    {"kind": "room_dims", "dims": [0.5, 0.5, 0.5]},
    {"kind": "room_dims", "dims": [100, 100, 100]},
    {"kind": "room_dims", "dims": [101, 100, 100]},
    {"kind": "room_dims", "dims": [1, 1, 2.25]},
    {"kind": "room_volume", "m3": 0},
])
record("groups", [
    {"kind": "sub", "role": "SW2", "patch": {"hp_hz": 30, "lp_hz": 80, "peq": [[60, 2, -3]]}},
    {"kind": "sub_mode", "role": "SW2", "mode": "bundle:A"},
    {"kind": "sub_mode", "role": "SW1", "mode": "bundle:A"},
    {"kind": "sub_layout", "bundle": "A", "layout": "dba"},
    {"kind": "sub_mode", "role": "SW3", "mode": "bundle:A"},
    {"kind": "sub", "role": "SW1", "patch": {"hp_type": "BW12", "lp_type": "LR48", "peq": [[40, 1, 2]]}},
    {"kind": "sub", "role": "SW3", "patch": {"dba_row": "rear", "gain_db": -3, "delay_ms": 7, "polarity": -1, "allpass": [[50, 0.7]], "mimo_support": False}},
    {"kind": "rear_trim", "bundle": "A", "ms": -100},
    {"kind": "rear_trim", "bundle": "A", "ms": 100},
    {"kind": "sub_layout", "bundle": "A", "layout": "multi"},
    {"kind": "sub_mode", "role": "SW3", "mode": "bundle:B"},
    {"kind": "sub_mode", "role": "SW1", "mode": "fixed"},
    {"kind": "sub", "role": "SW1", "patch": {"hp_hz": 100, "lp_hz": 150}},
    {"kind": "sub_mode", "role": "SW2", "mode": "auto"},
    {"kind": "sub_layout", "bundle": "B", "layout": "values"},
])
record("sub_limits", [
    {"kind": "sub", "role": "SW1", "patch": {"gain_db": -100, "delay_ms": -1, "polarity": 0, "hp_hz": 1, "lp_hz": -1,
      "allpass": [[0, 1], [1, 0], [501, 21], [40, 1]], "peq": [[0, 1, 1], [1, 0, -31], [501, 21, 13], [50, 1, 0], [60, 1, 1]],
      "hp_type": "bad", "lp_type": "bad", "dba_row": "bad"}},
    {"kind": "sub", "role": "SW1", "patch": {"gain_db": 21, "delay_ms": 501, "polarity": -2, "hp_hz": 301, "lp_hz": 301}},
    *[{"kind": "sub_band_type", "role": "SW2", "band": band, "type": typ}
      for band in ("hp", "lp") for typ in ("off", *cal.SUB_BAND_TYPES, "off")],
    *[{"kind": "sub", "role": "SW1", "patch": {"hp_hz": 30, "lp_hz": hz}}
      for hz in (59, 60, 61, 0)],
])
roles = ["SW1", "SW2", "SW3", "SW4"]
record("infra", [
    {"kind": "infra", "role": None, "hz": 25, "type": "LR24", "subs": roles},
    {"kind": "sub_mode", "role": "SW1", "mode": "bundle:A"},
    {"kind": "sub_mode", "role": "SW2", "mode": "bundle:A"},
    {"kind": "sub_layout", "bundle": "A", "layout": "dba"},
    {"kind": "sub", "role": "SW4", "patch": {"lp_hz": 150}},
    *[{"kind": "infra", "role": role, "hz": hz, "type": "BW24", "subs": roles}
      for role, hz in (("SW1", 25), ("SW3", 30), (None, 30), ("SW2", 0), ("SW2", 301), (None, 25))],
])
# Gain warnings require a completed run, not manual gain settings. Pin that
# upstream-only prerequisite rather than inventing a pre-run gain warning.
gains = [-60, 12, cal.SUB_GAIN_WARN_DB, cal.SUB_GAIN_WARN_DB + 0.01, 51]
fam.save("gain_warnings", {"gains": gains}, {"warnings": cal.sub_gain_warnings(SimpleNamespace(
    result=SimpleNamespace(subs={f"SW{i+1}": SimpleNamespace(gain_db=g) for i, g in enumerate(gains)})))})
fam.finish()
