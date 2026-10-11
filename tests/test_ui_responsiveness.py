"""반응 속도 최적화(2026-09-23)가 화면 결과를 바꾸지 않는지.

1단계 표는 모양이 같으면 다시 만들지 않고, 출력·극성만 바뀌면 셀 위젯의 값만 바꾼다.
그렇게 맞춘 표가 처음부터 다시 만든 표와 같아야 한다.
"""
from __future__ import annotations

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)


def _table_state(step) -> list:
    """행마다 (각 칸 위젯 종류, 출력 드롭다운 값·항목 수, 중복 표시, 극성 글자·상태)."""
    from PyQt6 import QtWidgets

    table = step.table
    rows = []
    for row in range(table.rowCount()):
        cells = []
        for column in range(table.columnCount()):
            widget = table.cellWidget(row, column)
            if widget is None:
                cells.append(None)
                continue
            combo = widget if isinstance(widget, QtWidgets.QComboBox) else widget.findChild(QtWidgets.QComboBox)
            pills = [w.text() for w in widget.findChildren(QtWidgets.QLabel) if w.objectName() == 'pill']
            text = widget.text() if hasattr(widget, 'text') else ''
            cells.append((type(widget).__name__, text, widget.property('state'),
                          None if combo is None else (combo.currentData(), combo.count(),
                                                      combo.itemText(combo.count() - 1)),
                          tuple(pills)))
        rows.append(tuple(cells))
    return rows


def _rebuilt_state(step, qapp) -> list:
    step._table_key = None                    # 캐시를 버리고 처음부터 다시 만든 표
    step.refresh()
    qapp.processEvents()
    return _table_state(step)


def test_saved_irs_read_back_bit_identical_from_memory_file_and_cache(engine, tmp_path):
    """방금 저장한 IR(메모리)·파일 직접 읽기·soundfile·창 캐시가 모두 같은 값이고,
    다시 저장하면 캐시가 새 값을 읽는다."""
    import numpy as np
    import soundfile as sf

    from secs.rewapi.session import load_project_irs
    from secs.model import Project, SpeakerConfig
    from secs.rewapi import session
    from secs.rewapi.session import IrSaver

    path = tmp_path / 'ir.secs'
    saver = IrSaver(path)
    rng = np.random.default_rng(3)
    project = Project(config=SpeakerConfig.from_preset('2.0'))
    entry = {'pos': 1, 'label': '위치 1', 'uuid': {}, 'mdat': None, 'ir': {}, 'spl_db': {},
             'done': True, 'sample_rate': 48000}
    for role in ('L', 'R'):
        saved = saver.save(role, 1, rng.standard_normal(4800) * 0.1, 48000)
        entry['ir'][role] = str(saved.relative_to(path.parent)).replace('\\', '/')
    project.measurements.append(entry)

    target = saver.file_for('L', 1)
    from_memory, rate = IrSaver.load_path(target)
    session._RECENT.clear()
    from_file, _ = IrSaver.load_path(target)                  # 헤더만 보고 바로 읽는 경로
    from_sf, sf_rate = sf.read(str(target), dtype='float32', always_2d=False)
    assert rate == sf_rate == 48000
    assert from_memory.tobytes() == from_file.tobytes() == from_sf.tobytes()
    from_memory[0] = 99.0                                     # 돌려준 배열은 고쳐도 된다(사본)

    cache: dict = {}
    first = load_project_irs(project, path, cache=cache)
    again = load_project_irs(project, path, cache=cache)
    plain = load_project_irs(project, path)
    assert again[1]['L'][0] is first[1]['L'][0]              # 두 번째는 파일을 다시 안 연다
    assert not first[1]['L'][0].flags.writeable               # 나눠 쓰니 읽기 전용
    assert first[1]['L'][0].tobytes() == plain[1]['L'][0].tobytes() == from_sf.tobytes()

    saver.save('L', 1, np.ones(4800) * 0.5, 48000)          # 다시 측정 → 새 값
    fresh = load_project_irs(project, path, cache=cache)
    assert fresh[1]['L'][0][0] == np.float32(0.5)
    assert len(cache) == 2                                    # 옛 L 항목은 버렸다


def test_step1_table_skips_and_in_place_updates_match_a_full_rebuild(qapp, fake, tmp_path):
    import secs.model as model

    project = model.Project(config=model.SpeakerConfig.from_preset('9.1.6', biamp=True))
    win = make_window(project, fake, tmp_path / 'w.secs')
    try:
        step = win.config_page
        first = step.table.cellWidget(1, 0)
        win.go(2)
        win.go(1)                                  # 아무것도 안 바꾸고 돌아옴 → 다시 만들지 않는다
        assert step.table.cellWidget(1, 0) is first

        step._toggle_polarity(0, 0)                # 극성만 → 값만 바꾼다
        speaker = project.config.speakers[1]
        speaker.bands[0].out = project.config.speakers[0].bands[0].out     # 중복을 만든다
        step._touch()
        assert step.table.cellWidget(1, 0) is first
        qapp.processEvents()
        in_place = _table_state(step)
        assert in_place == _rebuilt_state(step, qapp)

        speaker.bands[0].out = 40                  # 범위 밖(가짜 장치는 16채널) → 목록이 달라 다시 만든다
        step._touch()
        qapp.processEvents()
        assert _table_state(step) == _rebuilt_state(step, qapp)

        step._toggle_polarity(0, 0)                # 되돌리기·중복 해소도 값만
        speaker.bands[0].out = 2
        step._touch()
        qapp.processEvents()
        assert _table_state(step) == _rebuilt_state(step, qapp)
    finally:
        win.close()
        qapp.processEvents()
