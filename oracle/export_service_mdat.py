#!/usr/bin/env python3
"""Run upstream pure assignment functions and load_mdat without importing Qt."""
from __future__ import annotations
import ast
from io import StringIO
import json
import re
import tempfile
import uuid
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import common
import numpy as np
from secs import model
from secs.mdat_loader import load_mdat_project, suggest_assignment
from secs.rewapi.interfaces import MeasurementResult
from secs.rewapi.session import IrSaver, MeasurementSession
from secs.rew_mdat.builder import MeasurementInput, build_mdat_bytes

source = common.SRC / 'secs/wizard/step5_measure.py'
tree = ast.parse(source.read_text(encoding='utf-8'))
names = {'_tokens', '_starts_with_token', 'match_role', 'split_position', 'looks_like_sub', 'auto_assign'}
body = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in names]
ns = {'np': np, 're': re, '_POS_TAG': re.compile(r'^(.*?)[\s_\-]*P(\d+)$', re.I),
      '_TRAILING_NUMBER': re.compile(r'^(.*\S)\s+(\d+)$'), 'role_code': model.role_code,
      'Path': Path, 'magnitude_db': lambda *_: None}
for cls in tree.body:
    if isinstance(cls, ast.ClassDef):
        for node in cls.body:
            if isinstance(node, ast.FunctionDef) and node.name in ('problems', 'load_mdat'):
                node.decorator_list = []
                body.append(node)
exec(compile(ast.Module(body=body, type_ignores=[]), str(source), 'exec'), ns)

window_source = common.SRC / 'secs/wizard/window.py'
window_tree = ast.parse(window_source.read_text(encoding='utf-8'))
window_body = []
for cls in window_tree.body:
    if isinstance(cls, ast.ClassDef):
        for node in cls.body:
            if isinstance(node, ast.FunctionDef) and node.name in ('_write_side_mdat', '_spl_offsets'):
                node.decorator_list = []
                window_body.append(node)
window_ns = {'Path': Path, 'np': np}
exec(compile(ast.Module(body=window_body, type_ignores=[]), str(window_source), 'exec'), window_ns)
fam = common.Family('service_mdat')
rolesets = [['L','R','C','SW1','Ls','Rs','Lb','Rb','Tfl','Tfr','Tbl','Tbr'],
            ['L:LF','L:HF','R:LF','R:HF','SW1'], [], ['SW1','SW2','SW3']]
titles = ['', 'L', 'L 2', 'L_P2', 'L-P0', 'L p9', 'SW 2', 'LFE 9', 'SW2',
          'L Fin', 'L Fin 2', 'Ls Fin', 'R HF sweep', 'R Fin LF', 'SW1 80Hz',
          'Sweep 3', 'DBA L', 'DBA R', 'INFRA', 'LF', 'LRS', 'LTR', 'L EQ', 'L LF SUM',
          'L P1', 'R P2', 'unknown 2', '  L_P3  ', 'P2', 'Lé', 'R HF']
for i, roles in enumerate(rolesets):
    for j, flags in enumerate(([None]*len(titles), [False]*len(titles), [k%3==0 for k in range(len(titles))])):
        fam.save(f'titles_{i}_{j}', {'titles':titles, 'roles':roles, 'flags':flags}, {
            'split': [ns['split_position'](t) for t in titles],
            'position_tagged': [ns['split_position'](t)[0] != t.strip() for t in titles],
            'match': [ns['match_role'](t,roles) for t in titles],
            'match_used': [ns['match_role'](t,roles,set(roles[:1])) for t in titles],
            'suggest': suggest_assignment(titles,roles), 'auto': ns['auto_assign'](titles,roles,flags)})
for i, (assignment, positions) in enumerate([
    ([None,None],[1,1]), (['L','L'],[1,1]), (['L','L'],[1,2]),
    (['Tbl','Tbl','R'],[2,2,1]), (['L:LF','L:LF'],[1,1]),
    (['R','R','L','L'],[2,2,1,1])]):
    shim = SimpleNamespace(assignment=lambda:assignment, positions=lambda:positions)
    fam.save(f'problems_{i}', {'assignment':assignment,'positions':positions}, ns['problems'](shim))

paths = [common.SRC/'secs/rew_mdat/template_1.mdat'] + sorted((common.GOLDENS/'mdat').glob('build_*_bytes.bin'))
for i, path in enumerate(paths):
    loaded = load_mdat_project(path)
    titles = [m.title for m in loaded.measurements]
    flags = [ns['looks_like_sub'](*loaded.ir_provider.get_ir(m.uuid)) for m in loaded.measurements]
    roles = rolesets[0]
    split = [ns['split_position'](t) for t in titles]
    assignment = [None]*len(titles)
    for pos in sorted({p for _,p in split}):
        indices = [j for j,(_,p) in enumerate(split) if p==pos]
        for j,r in zip(indices,ns['auto_assign']([split[j][0] for j in indices],roles,[flags[j] for j in indices])):
            assignment[j]=r
    # Exercise actual upstream load_mdat body with a headless page shim. All
    # parsing, filtering, record writes and source updates remain upstream code.
    project = model.Project.from_preset('7.1.4')
    project.extra['array'] = 9
    plan_roles = [s.role for s in model.ordered_speakers(project.config.speakers,subs_last=True)]
    with tempfile.TemporaryDirectory() as temp:
        target = Path(temp)/'import.secs'
        session = MeasurementSession(project, None, project_path=target)
        page = SimpleNamespace(ensure_session=lambda:session, _off_thread=lambda fn,**kw:fn(),
            _append=lambda *_:None, plan=lambda:SimpleNamespace(roles=plan_roles),
            position_count=lambda:9,current_position=lambda:1,_magnitudes={},
            refresh=lambda:None,changed=SimpleNamespace(emit=lambda:None))
        count = ns['load_mdat'](page,str(path),ask=False)
        text = target.read_text(encoding='utf-8') if target.exists() else None
        sidecars = {}
        irs = target.with_suffix('.irs')
        if irs.is_dir():
            for sidecar in sorted(irs.glob('*.src.json')):
                sidecars[sidecar.relative_to(Path(temp)).as_posix()] = sidecar.read_bytes().decode('utf-8')
        page.project = project
        page.step5 = SimpleNamespace(plan=lambda: SimpleNamespace(roles=plan_roles))
        page.measurer = None
        page._spl_offsets = lambda: window_ns['_spl_offsets'](page)
        with patch('uuid.uuid4', side_effect=(uuid.UUID(int=n) for n in range(1, count + 1))), \
                patch('time.time', return_value=1704164645.0):
            side_mdat_path = window_ns['_write_side_mdat'](page, target)
        side_mdat = None if side_mdat_path is None else side_mdat_path.read_bytes()
    fam.save(f'file_{i}', {'path':str(path.relative_to(common.ROOT)), 'roles':roles,
                          'project':model.Project.from_preset('7.1.4').to_dict()},
             {'titles':titles,'flags':flags,'split':split,'assignment':assignment,
              'position_tagged':[base != title.strip() for (base,_),title in zip(split,titles)],
              'import_count':count,'secs':text,'source_sidecars':sidecars,
              'source_info':[m.source_info for m in loaded.measurements],
              'side_mdat': None if side_mdat is None else fam.blob(f'file_{i}_side_mdat', side_mdat)})

# A hand-built project covers multiple positions and a role absent from the plan.
with tempfile.TemporaryDirectory() as temp:
    target = Path(temp) / 'multi.secs'
    project = model.Project.from_preset('2.0')
    project.measurements = []
    session = MeasurementSession(project, None, project_path=target)
    plan = SimpleNamespace(roles=['Tfl', 'SW1'])
    for pos, role, samples, rate, t0, offset in [
            (2, 'Z', [0.25, -0.5, 1.0], 44100, 0.002, None),
            (1, 'SW1', [1.0, 0.0, -0.25], 48000, -0.001, 116.25),
            (1, 'Tfl', [0.5, 0.25, 0.0], 48000, 0.0, 117.5)]:
        session.record(MeasurementResult(role, pos, f'{role}-{pos}', np.asarray(samples, np.float32),
                                        rate, role, t0_s=t0, spl_offset_db=offset), plan)
    page = SimpleNamespace(project=project, step5=SimpleNamespace(plan=lambda: plan), measurer=None)
    page._spl_offsets = lambda: window_ns['_spl_offsets'](page)
    with patch('uuid.uuid4', side_effect=(uuid.UUID(int=n) for n in range(1, 4))), \
            patch('time.time', return_value=1704164645.0):
        side_path = window_ns['_write_side_mdat'](page, target)
    side_bytes = side_path.read_bytes() if side_path is not None else None
fam.save('side_multi_unplanned', {'roles': plan.roles}, {
    'side_mdat': None if side_bytes is None else fam.blob('side_multi_unplanned_mdat', side_bytes),
    'order': ['Tfl_P1', 'SW1_P1', 'Z_P2'],
})

from secs.rew_mdat.source_info import from_rew_measurement

cal_text = 'Sens Factor = -1.337 dB\n20 -1.25 0.5\n1000 0.25\n'
def open_cal(path, *args, **kwargs):
    if str(path) == '마이크.cal':
        return StringIO(cal_text)
    raise OSError('missing calibration fixture')

with patch('time.time', return_value=1704164645.0), patch(
        'builtins.open', side_effect=open_cal):
    generated_sources = {
        'rew': from_rew_measurement('마이크.cal', '출력 장치', '입력 장치', 'out 1'),
        'missing_cal': from_rew_measurement('없음.cal', '출력', '입력', 'out 2'),
    }
    fam.save('source_generated', {}, {
        'json': {name: json.dumps(info, ensure_ascii=False) for name, info in generated_sources.items()},
        'values': generated_sources,
    })

source_texts = {
    'normal': json.dumps({
        'strings': {'outputName': '출력 장치', 'sourceFileName': '입력.wav'},
        'boxed': {'inputVolume': {'type': 'java.lang.Double', 'value': -12.5},
                  'usesEmbeddedCalData': {'type': 'java.lang.Boolean', 'value': True}},
        'cal': {'meterCal': {'inverseCSelected': False, 'sourceName': '마이크.cal',
                             'sourcePath': '/캘/마이크.cal', 'freqArray': [20.0, 1000.0],
                             'gainArray': [-1.25, 0.5], 'phaseArray': None,
                             'serialNum': {'type': 'java.lang.Integer', 'value': 7}}},
        'date': 1704164644000, 'notes': '원 메모', 'format': 'REW'}, ensure_ascii=False),
    'missing_keys': '{}',
    'string_coercions': '{"strings":{"outputName":7,"sourceFileName":true,"osName":null}}',
    'boxed_coercions': json.dumps({'boxed': {
        'inputVolume': {'type': 'java.lang.Double', 'value': '1.5'},
        'splCalOffset': {'type': 'java.lang.Integer', 'value': '42'},
        'usesEmbeddedCalData': {'type': 'java.lang.Boolean', 'value': 0},
        'calDataLimitApplied': {'type': 'java.lang.Boolean', 'value': 'False'},
        'outputVolume': {'type': 'unknown.Box', 'value': 3},
    }}),
    'cal_coercions': json.dumps({'cal': {'meterCal': {
        'inverseCSelected': 2, 'sourceName': False, 'sourcePath': 42,
        'freqArray': [20, True, '30'], 'gainArray': [0, -2, '1.5'],
        'phaseArray': [False, 1, '2.25'],
    }}}),
    'date_float': '{"date":1704164645000.75}',
    'date_bool': '{"date":true}',
    'date_negative': '{"date":-2.5}',
    'date_zero': '{"date":0}',
    'notes_format': '{"notes":42,"format":7}',
    'empty_notes_format': '{"notes":"","format":""}',
    'top_list': '[]',
    'broken_json': '{broken',
    'bad_array_string': '{"cal":{"meterCal":{"freqArray":[20,"bad"]}}}',
    'bad_integer_fraction': '{"boxed":{"splCalOffset":{"type":"java.lang.Integer","value":"4.2"}}}',
    'bad_integer_range': '{"boxed":{"splCalOffset":{"type":"java.lang.Integer","value":2147483648}}}',
    'bad_nan_date': '{"date":NaN}',
    'bad_strings_list': '{"strings":["not","a","mapping"]}',
}
for name, text in source_texts.items():
    with tempfile.TemporaryDirectory() as temp:
        wav = Path(temp) / 'L_P1.wav'
        IrSaver.source_path(wav).write_text(text, encoding='utf-8')
        info = IrSaver.load_source(wav)
        try:
            with patch('uuid.uuid4', return_value=uuid.UUID(int=1)), patch(
                    'time.time', return_value=1704164645.0):
                built = build_mdat_bytes([MeasurementInput(
                    'L_P1', 'SECS · 위치 1 · 원 측정', 48000,
                    np.asarray([1.0, -0.5, 0.25], np.float32),
                    -0.001, 116.25, info)])
        except Exception as error:  # the fixture pins upstream's failure class; F10 keeps Rust building
            result = {'loaded': info is not None, 'exception': type(error).__name__}
        else:
            result = {'loaded': info is not None, 'bytes': fam.blob(f'source_{name}_mdat', built)}
    fam.save(f'source_{name}', {'text': text}, result)

fam.finish()
assert sum(p.stat().st_size for p in fam.dir.iterdir()) < 8*1024*1024
