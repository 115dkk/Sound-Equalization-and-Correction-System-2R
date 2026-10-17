# -*- coding: utf-8 -*-
"""mdat 내보내기에 원 측정 환경(마이크 캘 · 장치)이 그대로 들어가고, 템플릿 제작자 환경은 새지 않는다(10-17 버그 리포트)."""
from __future__ import annotations

import numpy as np

from conftest import load_engine

load_engine()       # src 를 sys.path 에 넣는다
from secs.mdat_loader import load_mdat_project
from secs.rew_mdat import load
from secs.rew_mdat.builder import DEFAULT_TEMPLATE, MeasurementInput, build_mdat, build_mdat_bytes
from secs.rew_mdat.source_info import from_rew_measurement
from secs.rewapi.interfaces import MeasurementResult
from secs.rewapi.session import IrSaver, MeasurementSession, load_project_sources


INFO = {
    'strings': {'outputName': 'BlackHole 16ch, L', 'sourceFileName': 'Umik-1  Gain: 18dB', 'osName': 'mac os x'},
    'boxed': {'micSensitivity': {'type': 'java.lang.Double', 'value': -1.337},
              'usesEmbeddedCalData': {'type': 'java.lang.Boolean', 'value': True}},
    'cal': {'meterCal': {'inverseCSelected': False, 'sourceName': '7135539_90deg.txt',
                         'sourcePath': '/Users/someone/7135539_90deg.txt',
                         'freqArray': [10.0, 100.0, 1000.0], 'gainArray': [-1.5, 0.25, 0.0], 'phaseArray': [0.0, 0.0, 0.0],
                         'serialNum': {'type': 'java.lang.Integer', 'value': 7135539},
                         'sensitivitydB': {'type': 'java.lang.Double', 'value': -1.337}}},
    'notes': 'Acoustic reference played from L',
    'format': '256k Log Swept Sine, 1 sweep at -14.0 dBFS',
    'date': 1792195538802,          # 2026-10-17
}


def _ir() -> np.ndarray:
    ir = np.zeros(4096, np.float32)
    ir[100] = 1.0
    ir[140] = -0.3
    return ir


def test_template_file_has_no_author_environment():
    fields = load(DEFAULT_TEMPLATE).measurements[0].unknown_fields
    cal = fields['meterCal']
    assert str(cal.get('sourceName')) == '' and str(cal.get('sourcePath')) == '' and cal.get('gainArray') is None
    for name in ('outputName', 'sourceFileName', 'timingRefName', 'osName', 'jre'):
        assert str(fields[name]) == ''


def test_without_source_info_fields_are_empty():
    data = build_mdat_bytes([MeasurementInput('L', 'n', 48000, _ir())])
    fields = load_bytes(data)[0].unknown_fields
    cal = fields['meterCal']
    assert str(cal.get('sourceName')) == '' and cal.get('gainArray') is None
    assert fields['usesEmbeddedCalData'].get('value') is False
    assert str(fields['outputName']) == ''


def load_bytes(data: bytes):
    from secs.rew_mdat import loads
    return loads(data).measurements


def test_source_info_goes_in_as_is_and_computed_is_marked():
    data = build_mdat_bytes([MeasurementInput('before', 'SECS', 48000, _ir(), source_info=INFO),
                             MeasurementInput('after', 'SECS', 48000, _ir(), source_info=INFO, computed=True)])
    before, after = load_bytes(data)
    for m in (before, after):
        cal = m.unknown_fields['meterCal']
        assert str(cal.get('sourceName')) == '7135539_90deg.txt'
        np.testing.assert_allclose(cal.get('gainArray').data, [-1.5, 0.25, 0.0])
        assert cal.get('serialNum').get('value') == 7135539
        assert str(m.unknown_fields['outputName']) == 'BlackHole 16ch, L'
        assert m.unknown_fields['micSensitivity'].get('value') == -1.337
    assert str(before.unknown_fields['sourceFileFormat']).startswith('256k Log Swept Sine')
    assert 'Acoustic reference played from L' in before.notes
    assert before.unknown_fields['sourceFileDate'] == INFO['date']           # 원 측정 시각 그대로
    assert after.unknown_fields['sourceFileDate'] > INFO['date']             # 계산값 = 내보낸 시각(1970 아님)
    assert 'computed' in str(after.unknown_fields['sourceFileFormat'])
    assert 'Acoustic reference' not in after.notes


def test_mdat_import_keeps_source_info_round_trip(tmp_path):
    path = build_mdat([MeasurementInput('L', 'n', 48000, _ir(), source_info=INFO)], tmp_path / 'a.mdat')
    info = load_mdat_project(path).measurements[0].source_info
    assert info['cal']['meterCal']['sourceName'] == '7135539_90deg.txt'
    assert info['cal']['meterCal']['gainArray'] == [-1.5, 0.25, 0.0]
    assert info['strings']['outputName'] == 'BlackHole 16ch, L'
    # 다시 써도 같은 값(불러오기 → 내보내기 → 불러오기)
    again = build_mdat([MeasurementInput('L', 'n', 48000, _ir(), source_info=info)], tmp_path / 'b.mdat')
    assert load_mdat_project(again).measurements[0].source_info['cal'] == info['cal']


class _Project:
    measurements: list = []


def test_session_keeps_source_beside_wav_and_drops_stale(tmp_path):
    project = _Project()
    project.measurements = []
    secs = tmp_path / 'p.secs'
    session = MeasurementSession(project, measurer=None, project_path=secs, auto_save=False)
    session.record(MeasurementResult('L', 1, 'u', _ir(), 48000, 'L', source_info=INFO))
    assert IrSaver.source_path(secs.with_suffix('.irs') / 'L_P1.wav').is_file()
    assert load_project_sources(project, secs)[1]['L']['cal'] == INFO['cal']
    session.record(MeasurementResult('L', 1, 'u2', _ir(), 48000, 'L'))       # 다시 잰 측정엔 환경 없음
    assert load_project_sources(project, secs) == {}


def test_rew_measurement_reads_cal_file(tmp_path):
    cal = tmp_path / 'mic.txt'
    cal.write_text('"Sens Factor =-1.337dB, SERNO: 7135539"\n10.0 -1.5 0\n100\t0.25\n1000 0.0 3.0\n', encoding='utf-8')
    info = from_rew_measurement(str(cal), 'Out dev', 'In dev', 'out 1')
    item = info['cal']['meterCal']
    assert item['sourceName'] == 'mic.txt'
    assert item['freqArray'] == [10.0, 100.0, 1000.0] and item['phaseArray'][2] == 3.0
    assert item['sensitivitydB']['value'] == -1.337
    assert info['strings'] == {'outputName': 'Out dev, out 1', 'sourceFileName': 'In dev'}
    assert from_rew_measurement(None) is None
    assert load is not None
