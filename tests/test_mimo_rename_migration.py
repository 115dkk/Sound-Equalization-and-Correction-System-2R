# -*- coding: utf-8 -*-
"""10-10 기능 이름 정리(저역 감쇠 → MIMO, BA → BA) 뒤에도 옛 파일이 그대로 열린다.

1. `.secs` 판 1 · 2(판 번호 없음 포함)의 보정 설계 설정 `art_on` · `art_low` · `art_max_supports` 와 서브마다 `art_support` 는
   불러올 때 `mimo_*` 로 옮겨지고(`Project._migrate_legacy` 5번), 저장하면 판 3 — 다시 열어도 값이 같다.
2. 옛 계산 결과 캐시(판 1: 옛 모듈 · 클래스 이름, 키 `art_run`, 속성 `art` · `spl_art`, 설정의 `art_*`)도 `load_run_cache` 가
   옮겨 연다. 옛 모듈 이름은 저장소에 남기지 않으므로 같은 모양의 '지금은 없는 모듈' 이름으로 흉내 낸다 — 옛 캐시가 가는 길
   (`importlib.util.find_spec` 이 None → 지금 모듈에서 같은 이름 · 끝 이름으로 찾기)과 같다.
"""
from __future__ import annotations

import json
import pickle

import numpy as np
import pytest

from conftest import load_engine


@pytest.fixture(scope="module")
def engine():
    return load_engine("package")


OLD_CALIBRATION = {'mode': 'mixed', 'art_on': True, 'fsiso': 120.0, 'art_low': 30.0, 'art_max_supports': 4,
                   'subs_manual': {'SW1': {'mode': 'auto', 'art_support': True},
                                   'SW2': {'mode': 'fixed', 'gain_db': -3.0, 'art_support': False}}}


def _old_secs(version):
    import secs.model as m

    data = m.Project(config=m.SpeakerConfig.from_preset('7.1.4')).to_dict()
    data.pop('version')
    if version is not None:
        data['version'] = version
    data['extra'] = {'calibration': json.loads(json.dumps(OLD_CALIBRATION))}
    return data


def _check_settings(project):
    import secs.calibration as cal

    calibration = project.extra['calibration']
    assert not any(str(k).startswith('art_') for k in calibration), calibration
    assert {k: calibration[k] for k in ('mimo_on', 'mimo_low', 'mimo_max_supports')} == \
        {'mimo_on': True, 'mimo_low': 30.0, 'mimo_max_supports': 4}
    assert list(calibration)[:3] == ['mode', 'mimo_on', 'fsiso']                # 자리(순서) 그대로
    settings = cal.CalibrationSettings.from_project(project)
    assert (settings.mimo_on, settings.mimo_low, settings.mimo_max_supports, settings.fsiso) == (True, 30.0, 4, 120.0)
    assert settings.sub('SW2')['mimo_support'] is False and settings.sub('SW1')['mimo_support'] is True
    assert settings.sub('SW2')['mode'] == 'fixed' and settings.sub('SW2')['gain_db'] == -3.0
    assert settings.excluded_supports() == ('SW2',)


@pytest.mark.parametrize('version', [None, 1, 2])
def test_old_secs_mimo_keys_load_save_load(engine, tmp_path, version):
    import secs.model as m

    path = tmp_path / 'old.secs'
    path.write_text(json.dumps(_old_secs(version), ensure_ascii=False), encoding='utf-8')
    project = m.Project.load(path)
    assert project.version == m.PROJECT_VERSION == 3
    _check_settings(project)

    project.save(path)                                     # 저장하면 판 3, 옛 키는 남지 않는다
    saved = json.loads(path.read_text(encoding='utf-8'))
    assert saved['version'] == 3 and '"art_' not in path.read_text(encoding='utf-8')
    again = m.Project.load(path)
    assert again.version == 3
    _check_settings(again)


def test_new_secs_is_not_migrated_twice(engine, tmp_path):
    """판 3 파일의 `mimo_*` 는 그대로(옛 키 표는 판 2 이하만)."""
    import secs.model as m

    project = m.Project(extra={'calibration': {'mimo_on': True, 'mimo_low': 40.0}})
    path = tmp_path / 'new.secs'
    project.save(path)
    loaded = m.Project.load(path)
    assert loaded.extra['calibration'] == {'mimo_on': True, 'mimo_low': 40.0}


def test_rename_keys_keeps_existing_new_key(engine):
    from secs.model import rename_keys

    assert rename_keys({'art_on': False, 'mimo_on': True}, {'art_on': 'mimo_on'}) == {'mimo_on': True}
    assert rename_keys({'x': 1}, {'art_on': 'mimo_on'}) == {'x': 1}
    assert rename_keys(None, {'art_on': 'mimo_on'}) is None


# ──────────────────────────── 계산 결과 캐시 ────────────────────────────
#: 지금 이름 → 옛 캐시 모양(GLOBAL 줄). 옛 모듈 자리에는 지금 없는 `secs.legacy_*` 를 쓴다.
_LEGACY_GLOBALS = (
    (b'csecs.correction_engine\nCorrectionEngine\n', b'csecs.legacy_engine\nLegacyEngine\n'),
    (b'csecs.correction_engine\nCorrectionResult\n', b'csecs.legacy_engine\nLegacyResult\n'),
    (b'csecs.correction_engine\n', b'csecs.legacy_engine\n'),
    (b'csecs.mimo\nMimoResult\n', b'csecs.legacy_mimo\nArtResult\n'),
    (b'csecs.mimo_iir\n', b'csecs.legacy_mimo_iir\n'),
    (b'csecs.calibration\nMimoRun\n', b'csecs.calibration\nArtRun\n'),
)


def _runs():
    import secs.calibration as cal
    import secs.model as m
    from secs import mimo, mimo_iir
    from secs.correction_engine import CorrectionEngine, CorrectionResult, SubAlign

    settings = cal.CalibrationSettings(mimo_on=True, mimo_low=30.0, mimo_max_supports=4)
    settings.set_sub('SW1', mode='auto')
    settings.set_sub('SW2', mode='fixed', gain_db=-3.0, mimo_support=False)
    eng = CorrectionEngine({}, ['L', 'R'], ['SW1', 'SW2'], {'프론트': ['L', 'R'], 'sub': ['SW1', 'SW2']},
                           cal.target_fn(m.Project()), xo={'프론트': 80.0},
                           sub_manual={r: e for r, e in settings.subs_manual.items()})
    f = np.linspace(0.0, 24000.0, 9)
    res = CorrectionResult(channels={}, subs={'SW1': SubAlign('SW1'), 'SW2': SubAlign('SW2', mode='fixed')},
                           xo={'프론트': 80.0}, sub_bus_delay_ms=0.0, sub_bus_polarity=1, lift_ms=8.0, f=f, before={},
                           after={}, combined={}, sub_sum=None, target_db=np.zeros(f.size))
    run = cal.CalibrationRun(settings=settings, engine=eng, result=res, metrics={}, curves={}, target=np.zeros(3),
                             rc_hi=300.0, rc_auto=True, schroeder_hz=None, latency_ms=10.0)
    design = mimo.MimoResult(supports={'L': {'SW1': np.zeros(4)}}, ranges={'SW1': (20.0, 120.0)})
    mimo_run = cal.MimoRun(fsiso=120.0, f_low=30.0, design=design,
                           iir={'L': {'SW1': mimo_iir.IirSupport(-6.0, 3.0, 1, [], [])}},
                           final={'L': np.zeros(f.size)}, applied={'L': np.ones(f.size)},
                           curves={'L': {'spl_mimo': np.arange(3.0)}}, exclude=('SW2',), max_supports=4)
    return run, mimo_run


def _write_legacy_cache(path, run, mimo_run):
    """판 1 캐시를 만든다: 옛 속성 · 키 이름 + 옛 GLOBAL(프로토콜 2 — GLOBAL 이 글자 줄이라 바꿔 쓸 수 있다)."""
    for holder in (run.settings, run.engine):
        state = holder.__dict__
        for old, new in (('art_on', 'mimo_on'), ('art_low', 'mimo_low'), ('art_max_supports', 'mimo_max_supports')):
            if new in state:
                state[old] = state.pop(new)
        for key in ('subs_manual', 'sub_manual'):
            if isinstance(state.get(key), dict):
                state[key] = {r: {('art_support' if k == 'mimo_support' else k): v for k, v in e.items()}
                              for r, e in state[key].items()}
    mimo_run.__dict__['art'] = mimo_run.__dict__.pop('design')
    mimo_run.curves = {r: {'spl_art': c['spl_mimo']} for r, c in mimo_run.curves.items()}
    data = pickle.dumps({'version': 1, 'run': run, 'art_run': mimo_run}, protocol=2)
    for new, old in _LEGACY_GLOBALS:
        assert new in data, new                             # 옛 길을 정말 지나는지
        data = data.replace(new, old)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def test_old_run_cache_opens_with_new_names(engine, tmp_path):
    import secs.calibration as cal
    from secs import mimo, mimo_iir
    from secs.correction_engine import CorrectionEngine, CorrectionResult, SubAlign

    project_path = tmp_path / 'p.secs'
    _write_legacy_cache(cal.run_cache_path(project_path), *_runs())
    loaded = cal.load_run_cache(project_path)
    assert loaded is not None
    run, mimo_run = loaded
    assert isinstance(run.engine, CorrectionEngine) and isinstance(run.result, CorrectionResult)
    assert isinstance(run.result.subs['SW2'], SubAlign) and run.result.subs['SW2'].mode == 'fixed'
    assert isinstance(mimo_run, cal.MimoRun) and isinstance(mimo_run.design, mimo.MimoResult)
    assert isinstance(mimo_run.iir['L']['SW1'], mimo_iir.IirSupport)
    assert 'art' not in mimo_run.__dict__ and mimo_run.design.ranges == {'SW1': (20.0, 120.0)}
    assert list(mimo_run.curves['L']) == ['spl_mimo']
    s = run.settings
    assert not any(k.startswith('art_') for k in s.__dict__)
    assert (s.mimo_on, s.mimo_low, s.mimo_max_supports) == (True, 30.0, 4)
    assert s.excluded_supports() == ('SW2',) and s.mimo_key() == (s.fsiso, 30.0, ('SW2',), 4, False)
    assert mimo_run.key() == (120.0, 30.0, ('SW2',), 4, False)
    assert run.engine.sub_manual['SW2']['mimo_support'] is False and 'art_support' not in run.engine.sub_manual['SW2']

    # 다시 저장하면 새 판(키 mimo_run) — 그대로 다시 열린다
    cal.save_run_cache(project_path, run, mimo_run)
    with open(cal.run_cache_path(project_path), 'rb') as fh:
        raw = pickle.load(fh)
    assert raw['version'] == cal.RUN_CACHE_VERSION == 2 and 'mimo_run' in raw and 'art_run' not in raw
    again, again_mimo = cal.load_run_cache(project_path)
    assert again.settings.mimo_on and again_mimo.design.ranges == {'SW1': (20.0, 120.0)}


def test_new_run_cache_round_trip(engine, tmp_path):
    import secs.calibration as cal

    run, mimo_run = _runs()
    project_path = tmp_path / 'p.secs'
    cal.save_run_cache(project_path, run, mimo_run)
    loaded_run, loaded_mimo = cal.load_run_cache(project_path)
    assert loaded_run.settings.mimo_key() == run.settings.mimo_key()
    assert loaded_mimo.key() == mimo_run.key() and list(loaded_mimo.curves['L']) == ['spl_mimo']
