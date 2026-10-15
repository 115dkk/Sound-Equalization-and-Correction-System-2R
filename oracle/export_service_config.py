#!/usr/bin/env python3
"""Freeze upstream project-editor values and exact project bytes for service replay."""
from __future__ import annotations

import ast
import copy
import tempfile
from pathlib import Path

import common
from secs.calibration import CalibrationSettings
from secs.model import (ROLES, Band, DspProfile, Project, Speaker, SpeakerConfig,
                        TargetCurve, role_label)

fam = common.Family("service_config")


def load_position_seconds():
    """Compile upstream's pure estimate functions without importing its Qt screen."""
    source_path = Path(__file__).parents[1] / "src/secs/wizard/step4_layout.py"
    source = source_path.read_text(encoding="utf-8")
    module = ast.parse(source, filename=str(source_path))
    names = {"start_delay_seconds", "sweep_seconds", "position_seconds"}
    body = [
        node for node in module.body
        if isinstance(node, (ast.FunctionDef, ast.Assign, ast.AnnAssign))
        and (
            isinstance(node, ast.FunctionDef) and node.name in names
            or isinstance(node, ast.Assign)
            and any(isinstance(target, ast.Name) and target.id in {
                "SWEEP_OVERHEAD_S", "ACOUSTIC_OVERHEAD_S"
            } for target in node.targets)
            or isinstance(node, ast.AnnAssign)
            and isinstance(node.target, ast.Name)
            and node.target.id in {"SWEEP_OVERHEAD_S", "ACOUSTIC_OVERHEAD_S"}
        )
    ]
    namespace = {}
    exec(compile(ast.Module(body=body, type_ignores=[]), str(source_path), "exec"), namespace)
    return namespace["position_seconds"]


position_seconds = load_position_seconds()


def config_value(project: Project) -> dict:
    return {
        "preset": project.config.preset,
        "sub_count": project.config.sub_count(),
        "roles": [speaker.role for speaker in project.config.speakers],
        "outputs": [band.out for speaker in project.config.speakers for band in speaker.bands],
    }


def dsp_value(project: Project) -> dict:
    dsp = project.dsp
    return {
        "profile": dsp.profile,
        "name": dsp.name,
        "fir_taps": dsp.fir_taps,
        "fir_budget": dsp.fir_budget,
        "iir_bands": dsp.iir_bands,
        "allpass_bands": dsp.allpass_bands,
        "bm_path_delays": dsp.bm_path_delays,
    }


def target_value(project: Project) -> dict:
    return {
        "points": project.target.to_dict()["points"],
        "preset": project.bc.get("target_preset") or None,
    }


def save_bytes(project: Project, path: Path) -> bytes:
    project.save(path)
    return path.read_bytes()


def record(name: str, before: Project, after: Project, inputs: dict, values: dict) -> None:
    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw)
        before_path = root / "project.secs"
        after_path = root / "expected.secs"
        before.name = before_path.stem
        after.name = before_path.stem
        outputs = dict(values)
        outputs["before"] = fam.blob(f"{name}_before", save_bytes(before, before_path), "secs")
        outputs["project"] = fam.blob(f"{name}_project", save_bytes(after, after_path), "secs")
        outputs["project_json"] = after.to_dict()
        fam.save(name, inputs, outputs)


def preset_matrix() -> None:
    for preset in ("2.0", "2.1", "2.2", "5.1", "7.1.4", "9.1.6"):
        for subs in (0, 1, 2, 4, 8):
            before = Project.from_preset(preset)
            after = copy.deepcopy(before)
            after.config.set_sub_count(subs)
            record(f"preset_{preset.replace('.', '_')}_subs_{subs}", before, after,
                   {"op": "set_sub_count", "count": subs},
                   {"config": config_value(after)})


def config_edits() -> None:
    before = Project.from_preset("5.1")
    after = copy.deepcopy(before)
    after.config.set_sub_count(3)
    after.config.set_sub_count(1)
    after.config.speakers[0].role = "Lw"
    after.config.speakers[0].bands[0].out = 9
    used = {speaker.role for speaker in after.config.speakers}
    role = next((candidate for candidate in ROLES if candidate not in used), "custom")
    after.config.speakers.append(
        Speaker(role, "sub" if role.startswith("SW") else "small",
                [Band("SUB" if role.startswith("SW") else "FULL", after.config.next_free_out())]))
    del after.config.speakers[1]
    after.config.renumber()
    after.config.preset = "custom"
    record("config_edits", before, after, {"op": "config_edits"},
           {"config": config_value(after)})

    invalid = Project.from_preset("2.0")
    invalid.config = SpeakerConfig([
        Speaker("L", "small", [Band("FULL", 1), Band("BAD", 1, "x")]),
        Speaker("L", "sub", []),
    ], preset="custom")
    record("validation", copy.deepcopy(invalid), invalid, {"op": "validate"},
           {"messages": invalid.config.validate()})


def dsp_targets() -> None:
    for key, dsp in DspProfile.presets().items():
        before = Project.from_preset("2.0")
        after = copy.deepcopy(before)
        after.dsp = dsp
        record(f"dsp_{key}", before, after, {"op": "dsp_preset", "preset": key},
               {"dsp": dsp_value(after)})

    before = Project.from_preset("2.0")
    after = copy.deepcopy(before)
    after.dsp.fir_taps = 96000
    after.dsp.fir_budget = 48000
    after.dsp.iir_bands = 8
    after.dsp.allpass_bands = 2
    after.dsp.bm_path_delays = 3
    after.dsp.profile = "custom"
    record("dsp_manual", before, after, {"op": "dsp_manual"},
           {"dsp": dsp_value(after)})

    for key, target in TargetCurve.presets().items():
        before = Project.from_preset("2.0")
        after = copy.deepcopy(before)
        after.target = target
        after.bc["target_preset"] = key
        record(f"target_{key}", before, after, {"op": "target_preset", "preset": key},
               {"target": target_value(after)})

    before = Project.from_preset("2.0")
    after = copy.deepcopy(before)
    after.target = TargetCurve([(10.0, -30.0), (1000.0, 2.0), (24000.0, 30.0)])
    after.bc["target_preset"] = ""
    record("target_manual", before, after, {"op": "target_manual"},
           {"target": target_value(after)})


def patch_layout_calibration() -> None:
    project = Project.from_preset("2.1")
    lines = ["역할,이름,크기,대역,측정 출력,극성"]
    for speaker in project.config.speakers:
        for band in speaker.bands:
            lines.append(f"{speaker.role},{role_label(speaker.role)},{speaker.size},"
                         f"{band.band},{band.out},{band.pol}")
    patch = ("﻿" + "\n".join(lines) + "\n").encode("utf-8")
    record("patch", copy.deepcopy(project), project, {"op": "patch"},
           {"patch": fam.blob("patch_sheet", patch, "csv")})

    for count in (1, 9, 13, 17):
        for timing in ("acoustic", "loopback", "none"):
            before = Project.from_preset("2.1")
            after = copy.deepcopy(before)
            after.extra["array"] = count
            after.extra["start_delay"] = "5 초"
            after.extra["devices"] = {
                "timing_reference": timing,
                "timing_ref_role": "L",
                "timing_ref_output": 1,
            }
            seconds = position_seconds(after)
            record(f"estimate_{count}_{timing}", before, after,
                   {"op": "layout", "count": count, "timing": timing},
                   {"per_position_seconds": seconds, "total_seconds": count * seconds})

    before = Project.from_preset("2.0")
    after = copy.deepcopy(before)
    settings = CalibrationSettings.from_project(after)
    settings.mode = "ultra"
    settings.ultra_ms = 180.0
    settings.rc_lo = 25.0
    settings.mimo_on = True
    settings.store(after)
    record("calibration", before, after, {"op": "calibration"},
           {"calibration": settings.__dict__})


preset_matrix()
config_edits()
dsp_targets()
patch_layout_calibration()
fam.finish()
