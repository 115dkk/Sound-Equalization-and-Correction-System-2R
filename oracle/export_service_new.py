#!/usr/bin/env python3
"""Freeze the exact project bytes written by upstream new-project behavior."""
from __future__ import annotations

import tempfile
from pathlib import Path

import common
from secs.model import DspProfile, Project
from secs.wizard import apply_auto_defaults, new_project

fam = common.Family("service_new")


def configured_project(preset: str, subs: int) -> Project:
    project = Project.from_preset(preset)
    project.dsp = DspProfile.presets()["camilla"]
    if project.config.sub_count() != subs:
        project.config.set_sub_count(subs)
    apply_auto_defaults(project)
    return project


def record(name: str, project: Project, inputs: dict) -> None:
    with tempfile.TemporaryDirectory() as raw:
        path = Path(raw) / "project.secs"
        project.name = path.stem
        project.save(path)
        outputs = {"project": fam.blob(f"{name}_project", path.read_bytes(), "secs")}
        fam.save(name, inputs, outputs)


record("default", new_project(False), {"sample": False, "preset": "2.0", "sub_count": 0})
record("sample", new_project(True), {"sample": True, "preset": "ignored", "sub_count": 0})
for preset, subs in (("2.0", 1), ("2.1", 2), ("5.1", 0), ("7.1.4", 3)):
    record(f"preset_{preset.replace('.', '_')}_subs_{subs}", configured_project(preset, subs),
           {"sample": False, "preset": preset, "sub_count": subs})

fam.finish()
