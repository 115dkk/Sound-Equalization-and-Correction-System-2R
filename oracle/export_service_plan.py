#!/usr/bin/env python3
"""Freeze measurement planning, signatures and time estimates."""
from __future__ import annotations

import ast
import json
import os
import tempfile
from pathlib import Path

import common
import engine_cases
import numpy as np
from secs import calibration as cal
from secs import model
from secs.rewapi.interfaces import (
    MeasurementPlan,
    MeasurementResult,
    SWEEP_LENGTHS,
    plan_from_project,
    split_role,
    sweep_length_samples,
)
from secs.rewapi.session import MeasurementSession


def load_functions(path: Path, names: tuple[str, ...], globals_):
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
    code = compile(ast.Module(body=body, type_ignores=[]), str(path), "exec")
    namespace = dict(globals_)
    exec(code, namespace)
    return [namespace[name] for name in names]


start_delay_seconds, sweep_seconds, position_seconds = load_functions(
    common.SRC / "secs/wizard/step4_layout.py",
    ("start_delay_seconds", "sweep_seconds", "position_seconds"),
    {"SWEEP_OVERHEAD_S": 2.5, "ACOUSTIC_OVERHEAD_S": 0.7},
)
measurement_roles, default_ref_role, timing_settings = load_functions(
    common.SRC / "secs/wizard/timing.py",
    ("measurement_roles", "default_ref_role", "timing_settings"),
    {"TIMING_CHOICES": (("acoustic", ""), ("loopback", ""), ("none", ""))},
)

fam = common.Family("service_plan")

# estimate_xo_seconds splits the candidates over half the cores
# (mimo_iir._workers reads os.cpu_count()). Pin the count so the goldens do not
# depend on the machine that exports them; the Rust test reads it back.
ORACLE_CPU_COUNT = 4
os.cpu_count = lambda: ORACLE_CPU_COUNT


def normalize(value):
    if isinstance(value, tuple):
        return [normalize(item) for item in value]
    if isinstance(value, list):
        return [normalize(item) for item in value]
    if isinstance(value, dict):
        return {str(key): normalize(item) for key, item in value.items()}
    return value


def record(name, project, positions):
    plan = plan_from_project(project, positions)
    outputs = {
        "plan": {
            "roles": plan.roles,
            "outputs": plan.outputs,
            "positions": plan.positions,
            "output_device": plan.output_device,
            "input_device": plan.input_device,
            "input_channel": plan.input_channel,
            "sweep_level_db": plan.sweep_level_db,
            "sweep_length": plan.sweep_length,
            "repetitions": plan.repetitions,
            "timing_reference": plan.timing_reference,
            "timing_ref_output": plan.timing_ref_output,
            "naming": plan.naming,
            "start_frequency": plan.start_frequency,
            "end_frequency": plan.end_frequency,
            "sample_rate": plan.sample_rate,
            "validate": plan.validate(),
            "names": [[role, pos, plan.name_for(role, pos)] for pos in positions for role in plan.roles],
            "outputs_for": [[role, plan.output_for(role)] for role in plan.roles + ["missing"]],
            "total_steps": plan.total_steps(),
        },
        "timing_settings": timing_settings(project),
        "measurement_roles": measurement_roles(project),
        "default_ref_role": default_ref_role(project),
        "sweep_seconds": sweep_seconds(project),
        "position_seconds": position_seconds(project),
        "measured_positions": cal.measured_positions(project),
        "measurement_signature": cal.measurement_signature(project),
        "target_key": cal.target_key(project),
        "estimate_seconds": {mode: cal.estimate_seconds(project, mode) for mode in ("zero", "mixed", "linear")},
        "estimate_subs_seconds": cal.estimate_subs_seconds(project),
        "estimate_xo_seconds": {mode: cal.estimate_xo_seconds(project, mode) for mode in ("zero", "mixed")},
        "cpu_count": os.cpu_count(),
    }
    fam.save(name, {"project": project.to_dict(), "positions": list(positions)}, normalize(outputs))


for case in engine_cases.cases():
    project = case.project
    project.measurements = [
        {
            "pos": pos,
            "ir": {role: f"room.irs/{role}_P{pos}.wav" for role in roles},
            "t0": {role: value[2] if len(value) > 2 else 0.0 for role, value in roles.items()},
        }
        for pos, roles in case.irs.items()
    ]
    record(f"engine_{case.name}", project, sorted(case.irs))

project_fixtures = sorted((common.GOLDENS / "project").glob("preset_*.json"))
for path in project_fixtures[:4] + project_fixtures[-2:]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    project = model.Project.from_dict(payload["outputs"]["value"])
    positions = list(range(1, int(project.extra.get("array", 1) or 1) + 1))
    record(f"project_{path.stem}", project, positions)

multi = model.Project.from_preset("2.1")
multi.config.speakers[0].bands = [model.Band("LF", 1), model.Band("HF", 2)]
multi.config.speakers[1].bands = [model.Band("LF", 3), model.Band("HF", 4)]
multi.config.speakers[2].bands = [model.Band("SUB", 5)]
multi.extra["devices"] = {"timing_reference": "acoustic", "timing_ref_role": "R:HF"}
record("multi_amp", multi, [1, 2, 3])

single = model.Project.from_preset("2.0")
single.extra["array"] = 1
single.extra["devices"] = {"timing_reference": "loopback", "timing_ref_output": 8,
                           "sweep_length": "512k", "sample_rate": 96000, "repetitions": 2}
record("single_position", single, [1])


class SyntheticIrSaver:
    def __init__(self, project_path):
        self.project_path = Path(project_path)

    def save(self, role, pos, ir, sample_rate):
        del ir, sample_rate
        target = self.project_path.with_suffix(".irs") / f"{role}_P{pos}.wav"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(b"")
        return target


def session_result(role, pos, uuid, *, offset=None):
    return MeasurementResult(role, pos, uuid, np.array([0.25, -0.5], dtype=np.float32),
                             48000, f"{role} P{pos}", spl_db=72.5 + pos,
                             t0_s=-0.001 * pos, spl_offset_db=offset, snr_db=31.0)


def session_case(name, payload):
    project = model.Project.from_dict(payload)
    plan = MeasurementPlan(["L", "R"], {"L": 1, "R": 2}, [1, 2])
    with tempfile.TemporaryDirectory() as raw_dir:
        path = Path(raw_dir) / "session.secs"
        project.save(path)
        session = MeasurementSession(project, None, SyntheticIrSaver(path),
                                     project_path=path, auto_save=True)
        steps = []

        def capture(step):
            steps.append({"name": step, "text": path.read_text(encoding="utf-8")})

        session.record(session_result("L", 2, "l-2-repair", offset=114.5), plan)
        capture("repair_2")
        session.start_position(1, plan)
        session.save()
        capture("start_1")
        session.record(session_result("L", 1, "l-1"), plan)
        capture("record_1_l")
        session.record(session_result("R", 1, "r-1", offset=116.25), plan)
        capture("record_1_r_offset")
        session.start_position(3, plan)
        session.save()
        capture("start_3")
        session.start_position(2, plan)
        session.save()
        capture("start_2")
        session.record(session_result("L", 2, "l-2"), plan)
        capture("record_2_l")
        session.record(session_result("R", 2, "r-2"), plan)
        capture("record_2_r")
        session.record(session_result("L", 2, "l-2-new", offset=115.75), plan)
        capture("record_2_l_again")
        session.start_position(1, plan)
        session.save()
        capture("restart_1")
        session.record(session_result("R", 1, "r-1-new"), plan)
        capture("remeasure_1_r")
        def clear_pos(entry):
            try:
                return int(entry.get("pos", -1)) != 2
            except (AttributeError, TypeError, ValueError):
                return True

        project.measurements[:] = [m for m in (project.measurements or []) if clear_pos(m)]
        session.save()
        capture("clear_2")

    fam.save(f"session_{name}", {"project": payload}, {"steps": steps})


fixture = json.loads(project_fixtures[0].read_text(encoding="utf-8"))
session_case("project", fixture["outputs"]["value"])
odd = model.Project.from_preset("2.0").to_dict()
odd["measurements"] = [
    {"future_before": [1, 2], "pos": "1", "uuid": ["odd"], "ir": "bad",
     "spl_db": {"old": 1}, "t0": None, "done": "yes", "future_after": {"x": True}},
    {"first": "keep-order", "pos": "2", "label": "custom", "uuid": ["odd"],
     "ir": "bad", "spl_db": {"legacy": None}, "t0": None, "done": "truthy",
     "sample_rate": "old", "last": [4, 5]},
    {"head": True, "pos": "3", "uuid": {"old": "u"}, "ir": {"old": "old.wav"},
     "spl_db": {"old": 12.0}, "t0": {"old": 0.5}, "future": "survives"},
    {"pos": "not-a-position", "keep": "untouched"},
    {"no_pos": True, "payload": [None, 3]},
]
session_case("odd", odd)
empty = model.Project.from_preset("2.0").to_dict()
empty["measurements"] = []
session_case("empty", empty)

helper_inputs = {
    "roles": ["L", " L:LF ", "R:", "SW1", ":HF"],
    "lengths": list(SWEEP_LENGTHS) + ["1.5k", "1024", "bad", ""],
    "delays": ["끄기", "5 초", "10 초", "15 초", "20s", "abc", "", None],
}
length_rows = []
for length in helper_inputs["lengths"]:
    try:
        length_rows.append([length, sweep_length_samples(length), None])
    except Exception as error:
        length_rows.append([length, None, type(error).__name__])
fam.save("helpers", helper_inputs, {
    "split_role": [[role, split_role(role)] for role in helper_inputs["roles"]],
    "sweep_length_samples": length_rows,
    "start_delay_seconds": [[text, start_delay_seconds(text)] for text in helper_inputs["delays"]],
})

invalid = [
    MeasurementPlan([], {}, []),
    MeasurementPlan(["L", "L"], {"L": 1}, [1, 1]),
    MeasurementPlan(["L", "R"], {"L": 1}, [1], repetitions=2, timing_reference="acoustic", sweep_length="bad"),
]
fam.save("validation", {}, {
    "rows": [{"validate": plan.validate(), "total_steps": plan.total_steps(),
              "names": [plan.name_for("L:LF", 3), plan.output_for("missing")]}
             for plan in invalid]
})

fam.finish()
if sum(path.stat().st_size for path in fam.dir.iterdir()) >= 2 * 1024 * 1024:
    raise SystemExit("service_plan exceeds 2 MiB")
