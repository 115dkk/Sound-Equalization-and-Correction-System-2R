# -*- coding: utf-8 -*-
"""카밀라 내보내기를 CamillaGUI 에서 다루기 쉽게(2026-10-10, 사용자: "다른 사람이 윈도우나 맥에서 카밀라 GUI 로 설정할 텐데
아무것도 없는 상태에서 뭘 어떻게 넣어야 라우팅이 되고 세팅이 되는지").

- 폴더 모양 = 카밀라 폴더: `configs/<이름>.yml` + `coeffs/<이름>_<역할>_fir.wav`, FIR 경로는 설정 기준 `../coeffs/…`
  (이 PC 의 CamillaDSP 4.1.3 으로 확인: Conv 상대 경로는 설정 파일 폴더 기준 — 작업 폴더가 달라도 열린다).
- 파이프라인 맨 앞 `route_in` · 맨 끝 `route_out`(기본 1:1) — GUI Mixers 탭에서 번호만 바꾸면 라우팅 끝. 장치 채널 수와 맞는다.
- 머리 주석 = 사람이 읽는 안내(영어 ASCII): 입력 · 출력 순서표, 장치를 바꿔야 한다는 말, 지연.
엔진 계산 없이 가짜 결과로 본다(빠름). 카밀라 실행 파일이 있으면 configs · coeffs 를 다른 곳에 옮겨 `-c` 도 돌린다.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path
from types import SimpleNamespace as NS

import numpy as np
import pytest

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT / 'src') not in sys.path:
    sys.path.insert(0, str(ROOT / 'src'))


def _camilla() -> Path | None:
    """카밀라 실행 파일(git 제외) — 워크트리에서 돌면 본 저장소 쪽도 본다."""
    cands = [ROOT / '실험' / 'camilladsp' / 'bin' / 'camilladsp.exe']
    if ROOT.parent.name == 'worktrees':
        cands.append(ROOT.parents[2] / '실험' / 'camilladsp' / 'bin' / 'camilladsp.exe')
    return next((p for p in cands if p.is_file()), None)


def _fake(fir_len: int = 256):
    mains, h = ['L', 'R', 'C', 'Ls', 'Rs'], np.zeros(fir_len)
    h[10] = 1.0
    channels = {r: NS(peq=[(60.0, 4.0, -3.0)], fir=h.copy(), gain_db=0.0, xo_hz=80.0, delay_ms=0.0) for r in mains}
    subs = {'SW1': NS(gain_db=-2.0, polarity=1, allpass=[], delay_ms=1.0, hp_hz=0.0, lp_hz=0.0, peq=[], mode='auto')}
    eng = NS(top_peq={}, sub_bus_peq=[], align_delay_ms={r: 0.0 for r in mains + ['SW1']}, main_extra_ms=0.0)
    return eng, NS(channels=channels, subs=subs)


def _sections(text: str) -> dict:
    cfg, sect = {}, None
    for line in text.splitlines():
        if not line or line.startswith('#'):
            continue
        if not line.startswith(' '):
            key, _sep, rest = line.partition(':')
            sect = key
            cfg[sect] = json.loads(rest) if rest.strip() else ([] if key == 'pipeline' else {})
        elif line.strip().startswith('- '):
            cfg[sect].append(json.loads(line.strip()[2:]))
        else:
            k, _sep, v = line.strip().partition(': ')
            cfg[sect][k] = json.loads(v)
    return cfg


def test_export_layout_is_camillagui_folders_with_relative_fir(tmp_path):
    from secs.export_camilladsp import write_config

    eng, res = _fake()
    files = write_config(eng, res, tmp_path / 'camilladsp', name='secs_mixed', latency_ms=9.6, mode='mixed')
    yml = files[0]
    assert yml == tmp_path / 'camilladsp' / 'configs' / 'secs_mixed.yml'
    assert [p.name for p in files[1:]] == sorted(f'secs_mixed_{r}_fir.wav' for r in res.channels)
    assert all(p.parent == tmp_path / 'camilladsp' / 'coeffs' for p in files[1:])
    cfg = _sections(yml.read_text(encoding='ascii'))
    names = {spec['parameters']['filename'] for spec in cfg['filters'].values() if spec['type'] == 'Conv'}
    assert names == {f'../coeffs/secs_mixed_{r}_fir.wav' for r in res.channels}


def test_route_mixers_are_one_to_one_first_and_last(tmp_path):
    from secs.export_camilladsp import write_config

    eng, res = _fake()
    cfg = _sections(write_config(eng, res, tmp_path, name='x')[0].read_text(encoding='ascii'))
    n_in, n_out = len(res.channels) + 1, len(res.channels) + len(res.subs)          # 입력 맨 뒤 LFE
    assert cfg['pipeline'][0] == {'type': 'Mixer', 'name': 'route_in'}
    assert cfg['pipeline'][-1] == {'type': 'Mixer', 'name': 'route_out'}
    for name, n in (('route_in', n_in), ('route_out', n_out)):
        m = cfg['mixers'][name]
        assert m['channels'] == {'in': n, 'out': n}, name
        assert [(x['dest'], [s['channel'] for s in x['sources']]) for x in m['mapping']] == [(k, [k]) for k in range(n)]
        assert m['description'].isascii()
    assert cfg['devices']['capture']['channels'] == n_in and cfg['devices']['playback']['channels'] == n_out


def test_header_tells_order_devices_and_latency(tmp_path):
    from secs.export_camilladsp import HOWTO_MARK, write_config

    eng, res = _fake()
    text = write_config(eng, res, tmp_path, name='secs_ultra200', latency_ms=200.0, mode='ultra200')[0].read_text(encoding='ascii')
    head = [line for line in text.splitlines() if line.startswith('#')]
    assert head[1] == '# inputs 6: L, R, C, Ls, Rs, LFE / outputs 6: L, R, C, Ls, Rs, SW1'   # 맥 도구 · 테스트가 읽는 줄 그대로
    start = next(i for i, h in enumerate(head) if h.startswith(HOWTO_MARK))
    howto = '\n'.join(head[start:])
    assert 'configs' in howto and 'coeffs' in howto and 'placeholders' in howto
    assert 'VB-Audio Virtual Cable' in howto and 'BlackHole' in howto and 'samplerate 48000' in howto
    # 조사 문서(10-10): 장치 형식은 F32(F32_LE 는 오류), 클럭이 다르면 rate adjust(윈도우는 AsyncSinc), GUI 엔 wav 먼저
    assert 'F32 (F32_LE is for files only)' in howto and 'enable_rate_adjust' in howto and 'AsyncSinc' in howto
    assert 'FIRST' in howto and '4.1' in head[start]
    rows = [h for h in head[start:] if h.startswith('#   ') and h[4:7].strip().isdigit()]
    assert [r.split()[1:] for r in rows] == [['0', 'L', 'L'], ['1', 'R', 'R'], ['2', 'C', 'C'], ['3', 'Ls', 'Ls'],
                                             ['4', 'Rs', 'Rs'], ['5', 'LFE', 'SW1']]
    assert 'Latency: filters 200.0 ms' in howto and '-200 ms' in howto
    cfg = _sections(text)
    assert cfg['title'] == 'SECS 2 ultra200 (200.0 ms)' and '5 LFE' in cfg['description'] and '5 SW1' in cfg['description']


@pytest.mark.skipif(_camilla() is None, reason='camilladsp.exe 없음(실험/camilladsp/bin)')
def test_configs_and_coeffs_copied_elsewhere_still_open(tmp_path):
    """GUI 폴더(configs · coeffs)에 나눠 넣고, 원래 FIR 은 지운 뒤 다른 작업 폴더에서 `-c` — 상대 경로가 설정 기준이라 열린다."""
    from secs.export_camilladsp import write_config

    eng, res = _fake()
    files = write_config(eng, res, tmp_path / 'export', name='secs_mixed')
    home = tmp_path / 'camilladsp_home'
    shutil.copytree(tmp_path / 'export' / 'configs', home / 'configs')
    shutil.copytree(tmp_path / 'export' / 'coeffs', home / 'coeffs')
    shutil.rmtree(tmp_path / 'export' / 'coeffs')                 # 원래 자리에서 읽지 않는다는 증거
    elsewhere = tmp_path / 'cwd'
    elsewhere.mkdir()
    done = subprocess.run([str(_camilla()), '-c', str(home / 'configs' / files[0].name)], cwd=str(elsewhere),
                          capture_output=True, timeout=60)
    output = (done.stdout + done.stderr).decode('utf-8', 'replace')
    assert done.returncode == 0 and 'Config is valid' in output, output
