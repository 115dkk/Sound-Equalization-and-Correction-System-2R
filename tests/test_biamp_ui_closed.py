"""바이앰프 잠시 끄기(2026-10-11 사용자: "바이앰프 기능도 잠시 끄자" · "그냥 ui에서만 끄면 되잖아").

`secs.wizard.BIAMP_UI = False` 한 줄로 2단계의 '바이앰프로 시작' 칸 · 앰프 구성 · 대역 열이 없다. 모델 · 엔진의 바이앰프
지원은 그대로다(`SpeakerConfig.from_preset(biamp=True)` 를 쓰는 시험들). 바이앰프 `.secs` 는 열 때 단일 앰프로 다룬다
(`secs.wizard.open_project` — 파일은 그대로). 다시 켜면(True) 이 파일을 지운다.
"""
from __future__ import annotations

import pytest

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

CHECKBOX_TEXT = '메인 스피커는 바이앰프(LF/HF)로 시작'


def _project(biamp: bool):
    import secs.model as model

    return model.Project(name='바이앰프', config=model.SpeakerConfig.from_preset('2.1', biamp=biamp))


def _table_combo_items(step) -> set[str]:
    from PyQt6 import QtWidgets

    return {combo.itemText(i) for combo in step.table.findChildren(QtWidgets.QComboBox) for i in range(combo.count())}


def test_flag_is_off(engine):
    import secs.wizard

    assert secs.wizard.BIAMP_UI is False


def test_step2_has_no_biamp_checkbox_or_columns(qapp, fake, tmp_path):  # noqa: F811
    from PyQt6 import QtWidgets

    project = _project(biamp=False)
    project.config.biamp_default = True                 # 옛 파일처럼 켜져 있어도 칸이 없으니 새 스피커는 단일 앰프
    window = make_window(project, fake, tmp_path / 'closed.secs')
    try:
        window.go(2)
        qapp.processEvents()
        step = window.config_page
        assert step._biamp is None
        assert not [box for box in window.findChildren(QtWidgets.QCheckBox) if box.text() == CHECKBOX_TEXT]
        assert step.table.isColumnHidden(2) and step.table.isColumnHidden(3)
        assert not step.table.isColumnHidden(4) and not step.table.isColumnHidden(5)
        assert '바이앰프' not in _table_combo_items(step)
        step._preset_buttons['5.1'].click()
        assert all(len(s.bands) == 1 for s in project.config.speakers)
        assert project.config.biamp_default is False
        step._add_speaker()
        assert len(project.config.speakers[-1].bands) == 1
    finally:
        window.close()


def test_flag_on_brings_the_checkbox_and_columns_back(qapp, fake, tmp_path, monkeypatch):  # noqa: F811
    from PyQt6 import QtWidgets
    import secs.wizard

    monkeypatch.setattr(secs.wizard, 'BIAMP_UI', True)
    project = _project(biamp=False)
    window = make_window(project, fake, tmp_path / 'open.secs')
    try:
        step = window.config_page
        assert isinstance(step._biamp, QtWidgets.QCheckBox) and step._biamp.text() == CHECKBOX_TEXT
        assert not step.table.isColumnHidden(2) and not step.table.isColumnHidden(3)
        assert '바이앰프' in _table_combo_items(step)
        step._biamp.setChecked(True)
        assert all(len(s.bands) == 2 for s in project.config.speakers if s.role in ('L', 'R'))
    finally:
        window.close()


def _saved_biamp_project(tmp_path):
    """바이앰프 2.1(L · R = LF/HF, SW1) + 위치 1 측정 기록(대역 IR) + 음향 타이밍 기준 R:HF 를 저장한 파일."""
    import numpy as np
    from secs.rewapi.session import IrSaver

    project = _project(biamp=True)
    path = tmp_path / 'biamp.secs'
    saver = IrSaver(path)
    entry = {'pos': 1, 'label': '위치 1', 'uuid': {}, 'mdat': None, 'ir': {}, 'spl_db': {}, 't0': {},
             'done': True, 'sample_rate': 48000}
    rng = np.random.default_rng(11)
    for role in ('L:LF', 'L:HF', 'R:LF', 'R:HF', 'SW1'):
        saved = saver.save(role, 1, rng.standard_normal(4800) * 0.1, 48000)
        entry['ir'][role] = str(saved.relative_to(path.parent)).replace('\\', '/')
    project.measurements.append(entry)
    project.extra['devices'] = {'timing_reference': 'acoustic', 'timing_ref_role': 'R:HF', 'timing_ref_output': 4}
    project.save(path)
    return path


def test_opening_a_biamp_project_drops_biamp_without_touching_the_file(engine, tmp_path):
    from secs.wizard import open_project
    from secs.wizard.timing import timing_settings

    path = _saved_biamp_project(tmp_path)
    before = path.read_bytes()
    project = open_project(path)
    config = project.config
    assert config.biamp_default is False
    # 출력 번호는 다시 매기지 않는다 — 바이앰프였던 스피커는 LF 출력, 서브는 원래 출력 그대로
    assert [(s.role, [(b.band, b.out) for b in s.bands]) for s in config.speakers] == \
        [('L', [('FULL', 1)]), ('R', [('FULL', 3)]), ('SW1', [('SUB', 5)])]
    assert config.validate() == []
    record = project.measurements[0]
    assert set(record['ir']) == {'L:LF', 'L:HF', 'R:LF', 'R:HF', 'SW1'}    # 측정 기록은 지우지 않는다
    assert record['done'] is False                                       # 단일 앰프 L · R 은 아직 안 쟀다
    assert project.extra['devices']['timing_ref_role'] == 'R'
    assert timing_settings(project) == ('acoustic', 3)
    assert path.read_bytes() == before                                   # 저장하기 전까지 파일은 그대로


def test_opening_with_the_flag_on_keeps_biamp(engine, tmp_path, monkeypatch):
    import secs.wizard

    monkeypatch.setattr(secs.wizard, 'BIAMP_UI', True)
    project = secs.wizard.open_project(_saved_biamp_project(tmp_path))
    assert project.config.biamp_default is True
    assert [len(s.bands) for s in project.config.speakers] == [2, 2, 1]


def test_opened_biamp_project_walks_the_steps_without_errors(qapp, fake, tmp_path):  # noqa: F811
    """대역 측정이 남은 채 열어도 1~5단계가 예외 없이 돌고, 5단계는 위치 1을 '완료' 로 보지 않는다. 보정 설계는 대역 IR 을
    빼고(노트) 메인이 없다는 오류로 다시 재라고 알린다."""
    from secs.calibration import CalibrationError, engine_inputs
    from secs.rewapi.session import load_project_irs
    from secs.wizard import open_project

    path = _saved_biamp_project(tmp_path)
    project = open_project(path)
    window = make_window(project, fake, path)
    try:
        for number in (1, 2, 3, 4, 5):
            window.go(number)
            qapp.processEvents()
        assert window.step5.done_positions() == []
        assert window.step5.plan().roles == ['L', 'R', 'SW1']
        irs = load_project_irs(project, path)
        with pytest.raises(CalibrationError):
            engine_inputs(project, irs)
    finally:
        window.close()
