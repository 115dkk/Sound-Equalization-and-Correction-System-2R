#!/usr/bin/env python3
"""Run upstream pure assignment functions and load_mdat without importing Qt."""
from __future__ import annotations
import ast
import json
import re
import tempfile
from pathlib import Path
from types import SimpleNamespace

import common
import numpy as np
from secs import model
from secs.mdat_loader import load_mdat_project, suggest_assignment
from secs.rewapi.session import MeasurementSession

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
    fam.save(f'file_{i}', {'path':str(path.relative_to(common.ROOT)), 'roles':roles,
                          'project':model.Project.from_preset('7.1.4').to_dict()},
             {'titles':titles,'flags':flags,'split':split,'assignment':assignment,
              'position_tagged':[base != title.strip() for (base,_),title in zip(split,titles)],
              'import_count':count,'secs':text})
fam.finish()
assert sum(p.stat().st_size for p in fam.dir.iterdir()) < 2*1024*1024
