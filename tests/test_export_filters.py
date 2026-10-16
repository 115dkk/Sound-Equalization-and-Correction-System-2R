"""필터 내보내기(2026-10-15 — `secs.export_filters`, `calibration.export_filter_text` · `export_eqapo`).

'동작 → 변화' 로 본다:
- 필터 계획의 전기 응답(입력 → 출력마다)에 원 측정을 곱해 더하면 엔진이 계산한 최종 응답(`stage_responses(...)['final']`)과
  같다 — 텍스트 · EqAPO 가 엔진과 같은 소리를 낸다(서브 2개 · FIR 있는 모드 · 고정 서브 필터 · MIMO 갈래 포함).
- EqAPO: 내보낸 폴더 = config.txt(입력 Copy · Include · 출력 Copy, ASCII) + 하위 폴더(필터 본체 · FIR wav) + 한국어 안내.
  Include · Convolution 이 가리키는 파일이 실제로 있고, 가상 채널은 쓰기 전에 만들어지며, 머리 여유 Preamp 가 부스트만큼.
- 텍스트: 채널마다 PEQ · FIR wav · 게인 · 딜레이가 사람이 읽는 줄로 — RC 만 적용한 결과(서브 · MIMO 없음)에서만.
- 범위(10-15 사용자): 텍스트 = RC, EqAPO = RC · BA(MIMO 면 막음), 카밀라 = 전부.
합성 방 2.2(서브 2개, 위치 3곳)를 BA 고정 · 빠른 PEQ 맞춤으로 계산한다(`test_calibration_sub_combine` 과 같은 방식).
"""
from __future__ import annotations

import re

import numpy as np
import pytest

from test_calibration_step import fast_engine  # noqa: F401
from test_calibration_sub_combine import fast_ba, two_subs  # noqa: F401
from test_wizard_common import engine  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


@pytest.fixture(scope='module')
def mixed_run(engine, two_subs):  # noqa: F811
    from secs import calibration as cal
    from secs import correction_engine
    from test_calibration_step import _fast_fit_peq
    from test_calibration_sub_combine import _fixed_ba

    project, irs = two_subs
    with pytest.MonkeyPatch.context() as mp:
        mp.setattr(correction_engine, 'fit_peq', _fast_fit_peq)
        mp.setattr(correction_engine.CorrectionEngine, '_align_subs', _fixed_ba)
        return cal.compute(project, irs, cal.CalibrationSettings(mode='mixed'))


@pytest.fixture(scope='module')
def rc_run(engine, two_subs):  # noqa: F811
    """서브 없는 2.0 — RC 만 적용한 결과(텍스트 내보내기의 범위)."""
    import copy

    from secs import calibration as cal
    from secs import correction_engine
    from test_calibration_step import _fast_fit_peq

    project, irs = two_subs
    project = copy.deepcopy(project)
    project.config.set_sub_count(0)
    project.measurements = [{'pos': m['pos'], 'ir': {r: v for r, v in m['ir'].items() if r in ('L', 'R')}}
                            for m in project.measurements]
    irs = {pos: {r: v for r, v in by_role.items() if r in ('L', 'R')} for pos, by_role in irs.items()}
    with pytest.MonkeyPatch.context() as mp:
        mp.setattr(correction_engine, 'fit_peq', _fast_fit_peq)
        return cal.compute(project, irs, cal.CalibrationSettings(mode='mixed'))


def _acoustic(plan, eng, role, pos):
    """계획의 전기 응답 × 원 측정(서브는 고정 필터까지 곱해 둔 H0 — 계획 쪽 고정 필터는 뺀다)의 합."""
    from secs.export_filters import plan_responses

    total = np.zeros(eng.f.size, complex)
    for out, h in plan_responses(plan, eng.f, role, skip_fixed=True).items():
        total = total + h * eng.H0[pos][out]
    return total


def _same(got, want, f, lo=20.0, hi=16000.0, tol_db=0.05):
    band = (f >= lo) & (f <= hi) & (np.abs(want) > 1e-3 * np.max(np.abs(want)))
    err = 20 * np.log10(np.abs(got[band]) / np.abs(want[band]))
    phase = np.angle(got[band] / want[band])
    return float(np.max(np.abs(err))), float(np.max(np.abs(phase)))


def test_plan_reproduces_engine_final_response(mixed_run):
    from secs.export_filters import make_plan

    run = mixed_run
    eng, res = run.engine, run.result
    plan = make_plan(eng, res)
    assert plan.outputs == ['L', 'R', 'SW1', 'SW2'] and plan.inputs == ['L', 'R', 'LFE']
    assert any(op[0] == 'fir' for op in plan.input_ops['L'])                  # mixed = FIR 있음
    assert any(a.polarity < 0 for a in res.subs.values()) and getattr(eng, 'main_extra_ms', 0.0) > 0   # 극성 · 추가 지연까지 본다
    for role in ('L', 'R'):
        # 엔진은 서브를 음수 딜레이로 두고 풀고, 내보내기는 모든 출력에 메인 추가 지연을 더해 양수로 만든다(카밀라와 같다)
        # — 전체가 그만큼 늦을 뿐 응답은 같다.
        extra = float(getattr(eng, 'main_extra_ms', 0.0) or 0.0)
        want = eng.stage_responses(role, eng.H0, eng.main_position)['final'] * np.exp(-2j * np.pi * eng.f * extra / 1000)
        got = _acoustic(plan, eng, role, eng.main_position)
        db, ph = _same(got, want, eng.f)
        assert db < 0.05 and ph < 0.01, (role, db, ph)


def test_eqapo_folder_is_ready_to_copy(mixed_run, tmp_path):
    from secs import calibration as cal

    files = cal.export_eqapo(mixed_run, tmp_path)
    config = tmp_path / 'config.txt'
    assert files[0] == config and all(p.exists() for p in files)
    text = config.read_text(encoding='ascii')
    include = re.search(r'^Include: (.+)$', text, re.M).group(1)
    body_path = tmp_path / include.replace('\\', '/')
    assert body_path.exists() and body_path.parent.name == 'secs_mixed'
    body = body_path.read_text(encoding='ascii')
    # Convolution 은 필터 본체 기준 경로 — 같은 폴더의 wav
    convs = re.findall(r'^Convolution: (.+)$', body, re.M)
    assert convs and all((body_path.parent / c).exists() for c in convs)
    # 사람이 고치는 것은 맨 위 Eval 두 줄(입력 · 출력 번호)뿐 — 기본값은 윈도우 순서
    lines = text.splitlines()
    evals = [line for line in lines if line.startswith('Eval: ')]
    assert evals == ['Eval: inL = 1; inR = 2; inLFE = 3', 'Eval: outL = 1; outR = 2; outSW1 = 3; outSW2 = 4']
    inc = lines.index(f'Include: {include}')
    assert lines.index(evals[1]) < lines.index('Copy: VML=0.0 VMR=0.0 VLFE=0.0') < inc
    # 장치 채널 번호를 쓰는 Copy 는 모두 If(번호 ≥ 1 이고 장치에 있음) 안에 — 없는 번호가 상수 1(직류)이 되던 10-15 버그
    depth, guard = 0, ''
    for line in lines:
        if line.startswith('If: '):
            depth, guard = depth + 1, line
        elif line == 'EndIf:':
            depth -= 1
        elif line.strip().startswith('Copy: ') and '`' in line:
            var = re.search(r'`(\w+)`', line).group(1)
            count = 'inputChannelCount' if line.strip().startswith('Copy: V') else 'outputChannelCount'
            assert depth == 1 and guard == f'If: {var} >= 1 && {var} <= {count}', (line, guard)
    assert depth == 0
    assert '  Copy: `outSW1`=VBSW1' in lines and '  Copy: VML=`inL`' in lines and '  Copy: `inLFE`=0.0' in lines
    assert not re.search(r'(^|[ =+*])\d+(?=[ +]|$)', '\n'.join(l for l in lines if l.lstrip().startswith('Copy: ')).replace('0.0', ''))
    # 가상 채널: 이름은 모두 대문자(EqAPO 의 Channel: 이 대문자로 찾는다), 만들어진(Copy 대상이 된) 뒤에만 쓴다
    made = {'VML', 'VMR', 'VLFE'}
    for line in body.splitlines():
        if line.startswith('Copy: '):
            for target, srcs in re.findall(r'(\w+)=(\S+)', line[6:]):
                assert set(re.findall(r'V[A-Z0-9]+', srcs)) <= made, line
                made.add(target)
        elif line.startswith('Channel: ') and line != 'Channel: all':
            assert line[9:] in made and line[9:] == line[9:].upper(), line
    assert not re.search(r'\bv[a-z]', '\n'.join(l for l in body.splitlines() if not l.startswith('#')))
    # 서브 버스: LR4 = 2차 Q 0.7071 두 개, 서브 게인 · 극성이 Copy 계수로
    eng = mixed_run.engine
    xo = mixed_run.result.channels['L'].xo_hz
    assert body.count(f'Filter: ON LPQ Fc {xo:.2f} Hz Q 0.7071') == 2 * 2          # 센드 L · R 마다 두 개
    for role, a in mixed_run.result.subs.items():
        k = a.polarity * 10 ** (a.gain_db / 20)
        assert f'Copy: VB{role}={k:.6f}*VSL+{k:.6f}*VSR+{k:.6f}*VLFE' in body
        assert a.delay_ms < 0.0005 or f'Delay: {a.delay_ms:.3f} ms' in body       # 0 ms 딜레이는 줄을 안 쓴다
    assert len(eng.sub_bus_peq) == 0 or 'Filter: ON PK' in body
    # 머리 여유: 계획의 가장 큰 부스트만큼(0.5 dB 올림)
    from secs.export_filters import headroom_db, make_plan
    peak = headroom_db(make_plan(eng, mixed_run.result))
    preamp = float(re.search(r'^Preamp: (-?[\d.]+) dB$', text, re.M).group(1))
    assert -preamp >= peak - 1e-9 and -preamp < peak + 0.5 + 1e-9
    guide = (tmp_path / 'EqAPO 적용 방법.txt').read_text(encoding='utf-8-sig')
    assert 'C:\\Program Files\\EqualizerAPO\\config' in guide and 'secs_mixed' in guide and '48000 Hz' in guide


def test_text_lists_every_value(rc_run, tmp_path):
    from secs import calibration as cal

    assert not rc_run.result.subs
    files = cal.export_filter_text(rc_run, tmp_path)
    text = files[0].read_text(encoding='utf-8-sig')
    assert files[0].name == f'SECS 필터 {rc_run.settings.label()}.txt'
    assert '입력 보정' not in text and '메인 경로' not in text                       # RC 만 — 채널마다 한 목록
    for role, c in rc_run.result.channels.items():
        assert f'== {role} ==' in text
        assert f'secs_mixed_{role}_fir.wav' in text and (tmp_path / f'secs_mixed_{role}_fir.wav').exists()
        assert f'딜레이   {c.delay_ms:.3f} ms' in text
        for f0, q, g in c.peq:
            assert f'{f0:9.2f} Hz   Q {q:7.3f}   {round(g, 2) + 0.0:+7.2f} dB' in text
    assert '-0.00' not in text and '채널마다: 입력 → PEQ → FIR → 게인 → 딜레이 → 출력' in text
    assert not any(w in text for w in ('서브', 'LFE', 'MIMO', '올패스'))         # RC 만 — 다른 단계 설명은 없다


def test_export_scope_text_rc_eqapo_rc_ba_camilla_all(rc_run, mixed_run, tmp_path):
    """텍스트 = RC 만, EqAPO = RC · BA(MIMO 면 막음), 카밀라 = 전부(10-15 사용자)."""
    from secs import calibration as cal

    mimo = object()                                     # MIMO 를 적용한 상태(판정은 None 인지만 본다)
    assert cal.filter_export_block(rc_run, None, 'text') == ''
    assert '서브(BA)' in cal.filter_export_block(mixed_run, None, 'text')
    assert 'MIMO' in cal.filter_export_block(rc_run, mimo, 'text')
    assert cal.filter_export_block(mixed_run, None, 'eqapo') == ''
    assert 'MIMO' in cal.filter_export_block(mixed_run, mimo, 'eqapo')
    assert cal.filter_export_block(mixed_run, mimo, 'camilla') == ''
    with pytest.raises(cal.CalibrationError):
        cal.export_filter_text(mixed_run, tmp_path / 't')
    assert not (tmp_path / 't').exists()


def test_mimo_default_is_fir_for_new_projects(two_subs):
    """10-15 사용자: MIMO 서포트는 FIR 기본. 보정 설정이 있던 옛 파일(키 없음)은 그때 뜻대로 IIR."""
    import copy

    from secs import calibration as cal

    project, _irs = two_subs
    fresh = copy.deepcopy(project)
    fresh.extra.pop('calibration', None)
    assert cal.CalibrationSettings().mimo_fir is True
    assert cal.CalibrationSettings.from_project(fresh).mimo_fir is True
    fresh.extra['calibration'] = {'mode': 'mixed'}
    assert cal.CalibrationSettings.from_project(fresh).mimo_fir is False


def test_mimo_paths_follow_the_engine_chain(mixed_run, tmp_path):
    """IIR MIMO 갈래: 원신호 → 게인 · 극성 · 지연 · PEQ · 올패스 · HP · LP → 서포트 출력에 더함(카밀라와 같은 체인)."""
    from secs import mimo, mimo_iir
    from secs.export_filters import eqapo_filters, make_plan, plan_responses

    eng, res = mixed_run.engine, mixed_run.result
    design = mimo.MimoResult()
    design.ranges = {'SW1': (25.0, 150.0)}
    sp = mimo_iir.IirSupport(-6.0, 3.0, -1, [(45.0, 4.0, -3.0)], [(60.0, 0.7)])
    plan = make_plan(eng, res, mimo=design, iir={'L': {'SW1': sp}}, fsiso=150.0)
    assert [(m.input, m.support) for m in plan.mimo] == [('L', 'SW1')]
    f = eng.f
    extra = float(getattr(eng, 'main_extra_ms', 0.0) or 0.0)
    want = (sp.response(f) * np.exp(-2j * np.pi * f * (eng.align_delay_ms.get('SW1', 0.0) + extra) / 1000)
            * mimo_iir.lr4(f, 25.0, 'HP') * mimo_iir.lr4(f, 150.0, 'LP'))
    without = make_plan(eng, res)
    got = plan_responses(plan, f, 'L', skip_fixed=True)['SW1'] - plan_responses(without, f, 'L', skip_fixed=True)['SW1']
    band = (f > 10) & (f < 2000)
    assert np.allclose(got[band], want[band], atol=1e-9 * np.max(np.abs(want)))
    body = eqapo_filters(plan, {s: f'{s}.wav' for s in plan.firs()})
    assert 'Copy: VP1=VML' in body.splitlines()[body.splitlines().index('# MIMO support paths: copy of the raw input') + 1]
    assert body.index('Copy: VP1=VML') < body.index('# 1) input correction')         # 입력 보정 전 원신호
    assert 'Copy: VP1=-1.0*VP1' in body and 'Filter: ON AP Fc 60.00 Hz Q 0.7000' in body
