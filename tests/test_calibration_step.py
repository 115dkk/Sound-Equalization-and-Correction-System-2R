"""보정 설계 단계(옛 6·7 통합, 2026-09-26 — `secs.wizard.step_calibration` · `secs.calibration`).

'동작 → 변화' 로 본다(요구 대장 규칙 — 곡선이 있다류는 근거가 아니다):
- 모드를 바꾸고 [계산] → 채널 필터(FIR 탭)·전체 지연·화면 숫자가 바뀐다.
- RC 상한을 수동으로 올리고 [계산] → 풀린 RC 상한이 바뀌고, 옛 상한 위(800 Hz 봉우리)의 보정 후 곡선이 타겟으로 온다.
- 측정 위치 1곳이면 MIMO 칸이 꺼지고 이유가 보이며, 3곳이 되면 켜진다. [MIMO 계산] → MIMO 곡선·mdat 단계가 생긴다.
- 내보내기: REW 가 읽는 mdat(이름에 '측정' 없음) · 카밀라 `-c` 를 통과하는 ASCII 설정.
- 창 흐름: 5 → 보정 설계 → 내보내기, 진행바 7칸.
엔진의 입력 PEQ 맞춤(least_squares, 채널당 ~18 s)은 빠른 탐욕 맞춤으로 바꿔 끼운다 — 엔진 수학이 아니라 화면·흐름을 본다.
MIMO 맞춤(수 분)은 결정적인 가짜 서포트로 바꿔 끼운다.
"""
from __future__ import annotations

import subprocess
import time
from pathlib import Path

import numpy as np
import pytest

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)

ROOT = Path(__file__).resolve().parents[1]
CAMILLA = ROOT / '실험' / 'camilladsp' / 'bin' / 'camilladsp.exe'
BUMP_HZ = 800.0          # 자동 RC 상한(500 Hz, 페이드 끝 707 Hz) 위에 일부러 넣은 봉우리


def _fast_fit_peq(g, corr_db, n, lo=20.0, hi=500.0, boost=10.0, ring_ms=None, f_max=None):
    """`correction_engine.fit_peq` 대역 — 탐욕 배치만(최소제곱 다듬기 없음), 밴드 4개까지."""
    from secs import correction_engine as de

    region = (g >= lo) & (g <= hi)
    bands = []
    for _ in range(min(int(n), 4)):
        cur = 20 * np.log10(np.abs(de.peq_response(g, bands)) + 1e-12)
        r = np.where(region, corr_db - cur, 0.0)
        k = int(np.argmax(np.abs(r)))
        if abs(r[k]) < 0.5:
            break
        bands.append((float(g[k]), 4.0, float(np.clip(r[k], -25.0, boost))))
    return bands


@pytest.fixture(scope='module')
def fast_engine(engine):  # noqa: F811
    from secs import correction_engine

    with pytest.MonkeyPatch.context() as mp:
        mp.setattr(correction_engine, 'fit_peq', _fast_fit_peq)
        yield


def _bump(ir: np.ndarray, rate: int) -> np.ndarray:
    """RBJ 피킹(800 Hz, +6 dB, Q 2)을 IR 에 건다 — 합성 방은 500 Hz 위가 평탄해 RC 상한의 효과가 안 보인다."""
    from scipy.signal import lfilter

    a = 10 ** (6.0 / 40)
    w0 = 2 * np.pi * BUMP_HZ / rate
    al = np.sin(w0) / (2 * 2.0)
    b = [1 + al * a, -2 * np.cos(w0), 1 - al * a]
    d = [1 + al / a, -2 * np.cos(w0), 1 - al / a]
    return lfilter(b, d, ir)


@pytest.fixture(scope='module')
def synth(engine):  # noqa: F811
    """합성 방(`room_model_decay`) 2.2 에서 서브 하나만(BA DE 를 건너뛰게), 위치 3곳, 메인에 800 Hz 봉우리."""
    import room_model_decay as rmd

    case = rmd.build_case('2.2', positions=3)
    irs = {}
    for pos, roles in case.irs.items():
        irs[pos] = {}
        for role, (ir, rate, t0) in roles.items():
            if role == 'SW2':
                continue
            irs[pos][role] = ((_bump(ir, rate) if role in ('L', 'R') else ir), rate, t0)
    return case, irs


def _project(case, positions: int):
    import copy

    project = copy.deepcopy(case.project)
    project.config.set_sub_count(1)
    project.measurements = [{'pos': p, 'ir': {r: f'{r}_P{p}.wav' for r in ('L', 'R', 'SW1')}}
                            for p in range(1, positions + 1)]
    return project


def _page(project, irs, tmp_path):
    from secs.wizard.step_calibration import StepCalibration

    # 이 파일의 MIMO 시험은 IIR 서포트(가짜 `design_mimo_iir`)를 본다 — 10-15 부터 새 프로젝트 기본은 FIR
    # (`test_export_filters.test_mimo_default_is_fir_for_new_projects` · FIR 경로는 test_calibration_mimo_fir_1012).
    project.extra.setdefault('calibration', {}).setdefault('mimo_fir', False)
    page = StepCalibration(project)
    page.set_context(tmp_path / 'c.secs', lambda: irs)
    # 첫 [계산] 은 XO 추천부터 돈다(10-09) — 이 파일의 테스트는 보정 계산만 보므로 '추천을 이미 했다' 로 둔다
    # (`xo_rec` 은 결과 키에 없어 결과가 낡지 않는다). 첫 계산 XO 자동은 test_calibration_tabs_1009.
    page.settings.xo_rec = {g: page.xo_of(g) for g in page.xo_group_names()}
    page.settings.store(project)
    return page


def _compute(page, qapp):  # noqa: F811
    if page.needs_first_xo():                  # 보정 계산만 본다(첫 계산 XO 자동은 test_calibration_tabs_1009)
        page.settings.xo_rec = {g: page.xo_of(g) for g in page.xo_group_names()}
        page.settings.store(page.project)
    page.compute_button.click()
    assert page.running(), page.status.text()
    assert page.wait(180000), '보정 계산이 끝나지 않았다'
    qapp.processEvents()
    assert page.run is not None, page.status.text()
    return page.run


# ──────────────────────────── 모드 ────────────────────────────
def test_switching_mode_changes_filters_latency_and_numbers(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    case, irs = synth
    project = _project(case, 3)
    page = _page(project, irs, tmp_path)
    try:
        page.mode_buttons['zero'].click()
        assert project.extra['calibration']['mode'] == 'zero'
        assert 'RC IIR 전용' in page.mode_spec.text()
        zero = _compute(page, qapp)
        assert page.status.text() == '계산 완료'
        assert zero.filters('L')['fir_taps'] == 0 and zero.latency_ms == pytest.approx(0.0, abs=0.01)
        assert 'FIR 없음' in page.filters_label.text()
        assert '전체 지연 0.0 ms' in page.summary_label.text()
        # '보정 전' = 메인 스피커 단독 원 측정(서브 합 아님 — 2026-09-27 사용자 결정). 화면 곡선도 그것에서 나온다.
        from secs.calibration import SPL_GRID, spl_db
        eng, res = zero.engine, zero.result
        assert res.before['L'] is eng.H0[eng.main_position]['L']
        assert np.allclose(zero.curves['L']['spl_before'], spl_db(res.before['L'], res.f))
        low = (SPL_GRID >= 30) & (SPL_GRID <= 60)          # XO 아래 — 서브가 들어간 합과는 달라야 한다
        assert np.max(np.abs(zero.curves['L']['spl_before'][low] - spl_db(res.combined['L'], res.f)[low])) > 1.0

        page.mode_buttons['mixed'].click()
        assert 'FIR 2,048' in page.mode_spec.text()
        assert page.stale() and '설정이 바뀌었습니다' in page.status.text()
        mixed = _compute(page, qapp)
        assert mixed.filters('L')['fir_taps'] == 2048
        assert mixed.latency_ms >= 8.0 - 1e-6
        assert 'FIR 2,048탭' in page.filters_label.text()
        assert '전체 지연 0.0 ms' not in page.summary_label.text()
        # 곡선도 바뀐다(FIR 이 저역 크기·위상을 더 맡는다) — 같은 채널의 보정 후 SPL 이 20–500 Hz 에서 달라진다.
        from secs.calibration import SPL_GRID as g
        band = (g >= 20) & (g <= 500)
        assert np.max(np.abs(zero.curves['L']['spl_after'][band] - mixed.curves['L']['spl_after'][band])) > 0.5
        # 채널을 바꾸면 지표 줄이 그 채널 것이 된다. 합성 방은 좌우 대칭이라 L·R 지표가 반올림까지 같을 수 있어(10-05 R1
        # 3차 맞춤 뒤 둘 다 ±1 dB 71 %) 글자가 다른지가 아니라 고른 채널의 엔진 지표를 읽는지 본다.
        text_l = page.metrics_label.text()
        mixed.metrics['R'] = {k: dict(v) for k, v in mixed.metrics['R'].items()}
        mixed.metrics['R']['after']['gd_20'] = 999.0               # 10-10 결정 2A: 결과 줄은 GD 폭(1/24 GD 최대는 뺐다)
        page.set_channel('R')
        assert page.channel == 'R' and '→ 999.0 ms' in page.metrics_label.text() and '→ 999.0 ms' not in text_l
        # 측정을 다시 하면(위치가 늘면) 결과가 낡았다고 알린다
        project.measurements.append({'pos': 4, 'ir': {'L': 'L_P4.wav'}})
        page.refresh()
        assert page.stale() and '측정이 바뀌었습니다' in page.status.text()
    finally:
        page.deleteLater()


def test_fir_touchup_brings_result_closer_to_design(fast_engine, synth, monkeypatch):  # noqa: F811
    """10-05 R1 3차 맞춤: 2차 PEQ 뒤 남은 넓은 오차를 FIR 이 다시 맡으면 필터가 설계 곡선에 더 가깝고(어느 채널도 멀어지지
    않음) 보정 후 ±1 dB 비율이 줄지 않는다(돌비 최고사양 Lb 1/6 oct 81 → 93 %, `tools/diag_r1_within.py`)."""
    from secs import calibration as cal
    from secs import correction_engine as de

    case, irs = synth
    project = _project(case, 3)
    settings = cal.CalibrationSettings()
    settings.mode = 'mixed'

    def design_err(run):
        eng, f = run.engine, run.result.f
        rc = (f >= 20.0) & (f <= eng.rc_hi)
        out = {}
        for role in run.result.channels:
            e = 10 * np.log10(de.smooth_power(f, np.abs(eng.chain[role]['F']) ** 2, 24.0) + 1e-30)[rc] - eng.design_db[role][rc]
            out[role] = float(np.sqrt(np.mean((e - np.median(e)) ** 2)))
        return out

    monkeypatch.setattr(de, 'FIR_TOUCHUP_ROUNDS', 0)
    off = cal.compute(project, irs, settings)
    monkeypatch.setattr(de, 'FIR_TOUCHUP_ROUNDS', 2)
    on = cal.compute(project, irs, settings)
    e_off, e_on = design_err(off), design_err(on)
    assert all(e_on[r] <= e_off[r] + 1e-9 for r in e_on) and any(e_on[r] < e_off[r] - 0.02 for r in e_on), (e_off, e_on)
    within = {name: float(np.mean([run.metrics[r]['after']['in1_20'] for r in run.metrics])) for name, run in (('off', off), ('on', on))}
    assert within['on'] >= within['off'], within


def test_ultra_params_follow_latency_limit(engine):  # noqa: F811
    """초고지연 모드(09-27): 지연 한도 30–200 ms → FIR 은 엔진 안전장치(여유 지연 ≤ FIR × 0.7) 안의 최소 2의 거듭제곱."""
    from secs import calibration as cal

    got = {ms: (cal.ultra_params(ms)['taps'], cal.ultra_params(ms)['max_latency_ms']) for ms in (10, 30, 50, 100, 120, 200, 500)}
    assert got == {10: (2048, 30.0), 30: (2048, 30.0), 50: (4096, 50.0), 100: (8192, 100.0), 120: (8192, 120.0),
                   200: (16384, 200.0), 500: (16384, 200.0)}
    for ms in (30, 75, 150, 200):
        m = cal.ultra_params(ms)
        assert m['lift_ms'] <= 0.7 * m['taps'] / 48.0
    assert cal.mode_label('ultra', 120) == '초고지연 모드 120 ms' and cal.mode_label('ultra') == '초고지연 모드'


def test_hidden_ultra_mode_opens_from_high_spec_and_sets_fir(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    """숨은 버튼: 최고사양에서 사양 줄 5번 → 초고지연 모드가 보이고 골라진다. 지연 한도를 바꾸면 FIR·지연이 따라온다."""
    from secs import calibration as cal
    from secs.i18n import language_manager

    case, irs = synth
    project = _project(case, 3)
    page = _page(project, irs, tmp_path)
    try:
        ultra = page.mode_buttons['ultra']
        assert not ultra.isVisibleTo(page) and not page.ultra_row.isVisibleTo(page)
        page.set_mode('ultra')                                   # 숨은 동안은 고를 수 없다
        assert page.settings.mode == 'mixed'
        for _ in range(5):                                       # 최고사양이 아니면 사양 줄을 눌러도 안 열린다
            page.spec_clicked()
        assert not ultra.isVisibleTo(page)
        page.mode_buttons['linear'].click()
        for _ in range(4):
            page.spec_clicked()
        assert page.settings.mode == 'linear'
        page.spec_clicked()
        assert page.settings.mode == 'ultra' and ultra.isVisibleTo(page) and page.ultra_row.isVisibleTo(page)
        assert project.extra['calibration']['mode'] == 'ultra'
        assert 'FIR 16,384' in page.mode_spec.text() and '200 ms' in page.mode_spec.text()
        page.ultra_spin.setValue(120.0)
        assert project.extra['calibration']['ultra_ms'] == 120.0 and 'FIR 8,192' in page.mode_spec.text()
        run = _compute(page, qapp)
        assert run.filters('L')['fir_taps'] == 8192
        assert 30.0 < run.latency_ms <= 120.0 + 1e-6
        page.ultra_spin.setValue(60.0)
        assert page.stale()
        # 저장·불러오기: 초고지연으로 저장된 프로젝트는 처음부터 보인다
        settings = cal.CalibrationSettings.from_project(project)
        assert settings.mode == 'ultra' and settings.ultra_ms == 60.0
        again = _page(project, irs, tmp_path)
        try:
            assert again.mode_buttons['ultra'].isVisibleTo(again) and again.ultra_spin.value() == 60.0
        finally:
            again.deleteLater()
        # 다른 모드로 돌아가도 이번 창에서는 버튼이 남는다(지연 칸은 숨는다)
        page.mode_buttons['mixed'].click()
        assert ultra.isVisibleTo(page) and not page.ultra_row.isVisibleTo(page)
        page.mode_buttons['ultra'].click()
        language_manager().set_language('en', persist=False)
        qapp.processEvents()
        import re
        assert not re.search('[가-힣]', page.mode_spec.text() + ultra.text()), (page.mode_spec.text(), ultra.text())
    finally:
        language_manager().set_language('ko', persist=False)
        page.deleteLater()


# ──────────────────────────── RC 상한 ────────────────────────────
def test_manual_rc_upper_changes_resolved_limit_and_after_curve(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    from secs import calibration as cal

    case, irs = synth
    project = _project(case, 3)
    page = _page(project, irs, tmp_path)
    try:
        page.mode_buttons['mixed'].click()
        assert page.rc_hi_auto.isChecked() and not page.rc_hi_spin.isEnabled()
        auto = _compute(page, qapp)
        assert auto.rc_auto and auto.rc_hi == pytest.approx(500.0)
        assert '자동' in page.rc_note.text() and '500 Hz' in page.rc_note.text()

        page.rc_hi_auto.click()                               # 자동 끔 → 칸이 켜진다
        assert page.rc_hi_spin.isEnabled()
        page.rc_hi_spin.setValue(1200.0)
        assert project.extra['calibration']['rc_hi'] == pytest.approx(1200.0)
        assert page.stale()
        manual = _compute(page, qapp)
        assert not manual.rc_auto and manual.rc_hi == pytest.approx(1200.0)
        assert '(수동)' in page.rc_note.text() and '1200 Hz' in page.rc_note.text()

        g = cal.SPL_GRID
        ref = (g >= 200) & (g <= 400)
        at = int(np.argmin(np.abs(g - BUMP_HZ)))

        def bump(run):
            dev = run.curves['L']['spl_after'] - run.target
            return float(dev[at] - np.median(dev[ref]))

        # 자동(500 Hz)은 800 Hz 봉우리를 그대로 두고, 수동 1200 Hz 는 타겟 쪽으로 끌어내린다.
        assert bump(auto) > 4.0
        assert bump(manual) < bump(auto) - 3.0
    finally:
        page.deleteLater()


def test_room_dims_fill_volume_and_survive_save_and_load(qapp, synth, tmp_path):  # noqa: F811
    """가로·세로·높이 셋을 넣으면 부피 = 곱, 부피를 직접 고치면 치수가 비워진다. 둘 다 .secs 에 저장·불러오기 되고,
    키가 없는 옛 파일은 부피 '없음'(자동 상한 500 Hz)으로 열린다."""
    import secs.model as model
    from secs import calibration as cal

    case, irs = synth
    project = _project(case, 1)
    page = _page(project, {1: irs[1]}, tmp_path)
    try:
        assert page.volume_spin.value() == 0.0 and page.settings.room_volume_m3 is None
        page.dim_spins[0].setValue(6.0)                      # 한두 칸만 넣은 동안은 부피가 그대로, 칸도 지워지지 않는다
        page.dim_spins[1].setValue(5.0)
        assert page.settings.room_volume_m3 is None and page.dim_spins[0].value() == 6.0
        page.dim_spins[2].setValue(3.0)
        assert page.volume_spin.value() == pytest.approx(90.0)
        assert project.extra['calibration']['room_volume_m3'] == pytest.approx(90.0)
        assert project.extra['calibration']['room_dims_m'] == [6.0, 5.0, 3.0]

        path = tmp_path / '방.secs'
        project.save(path)
        loaded = model.Project.load(path)
        settings = cal.CalibrationSettings.from_project(loaded)
        assert settings.room_volume_m3 == pytest.approx(90.0) and settings.room_dims_m == [6.0, 5.0, 3.0]
        again = _page(loaded, {1: irs[1]}, tmp_path)
        try:
            assert again.volume_spin.value() == pytest.approx(90.0)
            assert [s.value() for s in again.dim_spins] == [6.0, 5.0, 3.0]
        finally:
            again.deleteLater()

        page.volume_spin.setValue(120.0)                     # 부피를 직접 → 맞지 않는 치수는 지운다
        assert project.extra['calibration']['room_volume_m3'] == pytest.approx(120.0)
        assert project.extra['calibration']['room_dims_m'] is None
        assert [s.value() for s in page.dim_spins] == [0.0, 0.0, 0.0]

        old = dict(project.extra['calibration'])              # 옛 파일: 부피·치수 키가 없다
        old.pop('room_volume_m3')
        old.pop('room_dims_m')
        project.extra['calibration'] = old
        project.save(path)
        settings = cal.CalibrationSettings.from_project(model.Project.load(path))
        assert settings.room_volume_m3 is None and settings.room_dims_m is None
    finally:
        page.deleteLater()


def test_room_volume_reaches_engine_and_result_shows_how_rc_upper_was_set(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    """부피를 넣고 [계산] → 엔진이 그 부피를 받고, 결과 화면에 엔진 노트('RC 상한 … Hz (…)')와 부피가 보인다."""
    case, irs = synth
    project = _project(case, 1)
    page = _page(project, {1: irs[1]}, tmp_path)
    try:
        page.mode_buttons['zero'].click()
        page.set_room(dims_m=(6.0, 5.0, 3.0))
        run = _compute(page, qapp)
        assert run.engine.room_volume_m3 == pytest.approx(90.0)
        assert run.rc_note.startswith('RC 상한') and run.rc_note in run.engine.notes
        assert '슈뢰더' in run.rc_note
        note = page.rc_note.text()
        assert note.splitlines()[0] == run.rc_note and '룸 볼륨 90 m³' in note
        assert run.rc_note not in page.summary_label.text()        # RC 상한은 RC 카드에만(10-10)
        fs = run.schroeder_hz
        assert run.rc_hi == pytest.approx(500.0 if fs is None or fs > 500.0 else max(fs, 100.0))
    finally:
        page.deleteLater()


# ──────────────────────────── MIMO ────────────────────────────
def test_mimo_needs_three_positions(qapp, synth, tmp_path):  # noqa: F811
    case, irs = synth
    project = _project(case, 1)
    page = _page(project, {1: irs[1]}, tmp_path)
    try:
        assert not page.mimo_check.isEnabled() and not page.mimo_check.isChecked()
        assert not page.fsiso_spin.isEnabled() and not page.mimo_button.isEnabled()
        assert not page.mimo_reason.isHidden() and '1곳' in page.mimo_reason.text()
        assert page.compute_mimo() is False

        project.measurements = _project(case, 3).measurements           # 위치 3곳이 되면
        page.refresh()
        assert page.mimo_check.isEnabled() and page.mimo_reason.isHidden()
        assert not page.fsiso_spin.isEnabled()                            # 켜기 전엔 세부 칸이 꺼져 있다
        page.mimo_check.click()
        assert project.extra['calibration']['mimo_on'] is True
        assert page.fsiso_spin.isEnabled() and page.mimo_low_auto.isEnabled()
        page.fsiso_spin.setValue(120.0)
        assert project.extra['calibration']['fsiso'] == pytest.approx(120.0)
    finally:
        page.deleteLater()


@pytest.fixture()
def fake_mimo(monkeypatch):
    """결정적인 가짜 MIMO — L 입력에 SW1 서포트 하나(IIR: −6 dB, 3 ms, PEQ 1)."""
    from secs import mimo, mimo_iir

    calls = {}

    def design_mimo(engine, fsiso=150.0, level_db=-18.0, positions=None, f_low=None, **_):
        calls['design_mimo'] = (fsiso, f_low, list(positions or []))
        out = mimo.MimoResult()
        out.ranges = {'SW1': (20.0, float(fsiso)), 'L': None, 'R': None}
        out.supports = {'L': {'SW1': np.zeros(8)}}
        return out

    def design_mimo_iir(engine, design, positions, fsiso=150.0, workers=None, **_):
        calls['design_mimo_iir'] = (fsiso, workers)
        return {'L': {'SW1': mimo_iir.IirSupport(-6.0, 3.0, 1, [(45.0, 4.0, -3.0)], [])}}

    monkeypatch.setattr(mimo, 'design_mimo', design_mimo)
    monkeypatch.setattr(mimo_iir, 'design_mimo_iir', design_mimo_iir)
    return calls


def test_mimo_button_adds_mimo_curve_and_mdat_stage(qapp, fast_engine, synth, fake_mimo, tmp_path):  # noqa: F811
    from secs import calibration as cal
    from secs import rew_mdat

    case, irs = synth
    project = _project(case, 3)
    page = _page(project, irs, tmp_path)
    try:
        page.mode_buttons['zero'].click()
        _compute(page, qapp)
        page.mimo_check.click()
        page.mimo_low_auto.click()                       # 하한 수동 → design_mimo 에 f_low 로 간다
        page.mimo_low_spin.setValue(30.0)
        assert page.mimo_button.isEnabled() and 'MIMO 계산 전' in page.mimo_status.text()
        page.mimo_button.click()
        assert page.mimo_running()
        assert page.wait(120000)
        assert page.mimo_run is not None, page.mimo_status.text()
        assert fake_mimo['design_mimo'][:2] == (150.0, 30.0) and fake_mimo['design_mimo_iir'][1] == cal.MIMO_WORKERS
        assert '서포트 1개' in page.mimo_status.text()
        assert not page.legend_items['mimo'].isHidden()
        assert len(page.plot.getPlotItem().listDataItems()) == 4          # 전·후·타겟·MIMO
        mimo = page.mimo_run
        f = page.run.engine.f
        base, applied = cal.spl_db(mimo.final['L'], f), cal.spl_db(mimo.applied['L'], f)
        low = (cal.SPL_GRID >= 25) & (cal.SPL_GRID <= 120)
        assert np.max(np.abs(applied[low] - base[low])) > 0.5              # 서포트가 저역 응답을 바꿨다
        assert np.allclose(mimo.applied['R'], mimo.final['R'])               # 서포트 없는 입력은 그대로

        files = page.export('mdat', folder=tmp_path / 'out')
        names = [m.name for m in rew_mdat.load(files[0]).measurements]
        assert '01_L_4 MIMO 적용(IIR)' in names and not any('02_R_4' in n for n in names)

        page.mimo_check.click()                                            # MIMO 를 끄면 곡선도 내보내기에서도 빠진다
        assert page.legend_items['mimo'].isHidden()
        files = page.export('mdat', folder=tmp_path / 'out2')
        assert not any('MIMO' in m.name for m in rew_mdat.load(files[0]).measurements)
    finally:
        page.deleteLater()


# ──────────────────────────── 내보내기 ────────────────────────────
def test_export_writes_rew_mdat_and_valid_camilla_config(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    from secs import rew_mdat

    case, irs = synth
    project = _project(case, 3)
    page = _page(project, irs, tmp_path)
    try:
        assert page.export('mdat', folder=tmp_path) == []                  # 계산 전에는 쓸 것이 없다
        assert '계산 결과가 없습니다' in page.export_note.text()
        assert not page.export_buttons['mdat'].isEnabled()
        page.mode_buttons['mixed'].click()
        _compute(page, qapp)
        assert page.export_buttons['mdat'].isEnabled()
        out = tmp_path / '한글 폴더'                                        # 사용자 폴더에 한글이 있어도
        files = page.export('mdat', folder=out)
        assert len(files) == 1 and Path(files[0]).is_file() and Path(files[0]).parent.parent == out
        loaded = rew_mdat.load(files[0])
        names = [m.name for m in loaded.measurements]
        for role in ('L', 'R'):
            seq = '01' if role == 'L' else '02'
            for stage in ('1 보정 전 메인', '1 보정 전 서브', '2 XO 타겟 메인', '2 XO 타겟 서브', '3 합 후 보정'):
                assert f'{seq}_{role}_{stage}' in names, (role, stage, names)
        assert not any('측정' in n for n in names)                           # 계산값을 '측정' 이라 부르지 않는다
        assert all('계산값' in m.notes for m in loaded.measurements)
        assert all(m.sample_rate_hz == 48000 for m in loaded.measurements)
        # 필터 지연이 든 '합 후 보정' 은 시작 시각을 필터 지연만큼 당긴다(09-27: REW 초과위상에 28 ms 가 보였다)
        from secs.calibration import stage_items
        run = page.run
        by = {it.title: it for it in stage_items(run)}
        before, after = by['01_L_1 보정 전 메인'], by['01_L_3 합 후 보정']
        assert run.latency_ms > 1.0
        assert after.start_time_s == pytest.approx(before.start_time_s - run.latency_ms / 1000.0, abs=1e-9)
        assert '1개 파일을 저장했습니다' in page.export_note.text()

        files = page.export('camilla', folder=out)
        yml = Path(files[0])
        assert yml.suffix == '.yml' and yml.name.isascii()
        raw = yml.read_bytes()
        raw.decode('ascii')                                                  # CP949 로 읽는 GUI 서버도 읽는다
        text = raw.decode('ascii')
        assert 'Conv' in text and 'in_L_fir' in text and 'LinkwitzRileyHighpass' in text
        fir = [Path(p) for p in files if p.endswith('.wav')]
        # 10-10: CamillaGUI 폴더 모양(configs/ · coeffs/), wav 이름 앞에 설정 이름 — GUI 의 coeffs 하나를 설정 여럿이 같이 쓴다
        assert yml.parent.name == 'configs' and {p.parent.name for p in fir} == {'coeffs'}
        assert {p.name for p in fir} == {f'{yml.stem}_L_fir.wav', f'{yml.stem}_R_fir.wav'}
        assert f'"filename": "../coeffs/{yml.stem}_L_fir.wav"' in text      # 설정 기준 상대 경로
        if CAMILLA.is_file():
            done = subprocess.run([str(CAMILLA), '-c', str(yml)], capture_output=True, timeout=60)
            output = (done.stdout + done.stderr).decode('utf-8', 'replace')
            assert done.returncode == 0 and 'Config is valid' in output, output
    finally:
        page.deleteLater()


@pytest.mark.skipif(not CAMILLA.is_file(), reason='camilladsp.exe 없음(실험/camilladsp/bin)')
def test_zero_mode_camilla_config_has_no_fir_and_is_valid(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    case, irs = synth
    page = _page(_project(case, 1), {1: irs[1]}, tmp_path)
    try:
        page.mode_buttons['zero'].click()
        _compute(page, qapp)
        files = page.export('camilla', folder=tmp_path)
        text = Path(files[0]).read_text(encoding='ascii')
        assert 'Conv' not in text and 'in_L_peq0' in text
        done = subprocess.run([str(CAMILLA), '-c', files[0]], capture_output=True, timeout=60)
        assert done.returncode == 0, (done.stdout + done.stderr).decode('utf-8', 'replace')
    finally:
        page.deleteLater()


# ──────────────────────────── 화면 스레드 ────────────────────────────
def test_gui_thread_keeps_running_while_calculating(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    """계산은 워커 스레드 — 도는 동안 화면 타이머가 계속 돈다(가장 긴 틈 < 0.5 s)."""
    from PyQt6 import QtCore

    case, irs = synth
    page = _page(_project(case, 3), irs, tmp_path)
    ticks = []
    timer = QtCore.QTimer()
    timer.setInterval(20)
    timer.timeout.connect(lambda: ticks.append(time.perf_counter()))
    try:
        timer.start()
        page.mode_buttons['zero'].click()
        page.compute_button.click()
        assert page.running()
        assert page.wait(180000)
        timer.stop()
        gaps = np.diff(ticks)
        assert len(ticks) > 20 and float(np.max(gaps)) < 0.5, float(np.max(gaps))
    finally:
        timer.stop()
        page.deleteLater()


# ──────────────────────────── 창 흐름 ────────────────────────────
def test_wizard_flow_goes_measure_calibration_export(qapp, fake, tmp_path):  # noqa: F811
    import secs.model as model
    from secs.wizard.step_calibration import StepCalibration
    from secs.wizard.strings import CAL_STEP, STEPS

    project = model.Project(name='흐름', config=model.SpeakerConfig.from_preset('2.1'))
    win = make_window(project, fake, tmp_path / 'flow.secs')
    try:
        assert len(win.steps_bar.buttons) == len(STEPS) == 7
        assert win.steps_bar.buttons[5].text() == '보정 설계' and win.steps_bar.buttons[6].text() == '내보내기'
        win.go(5)
        win.step5.load_sample()                       # 샘플로 채우고 다음으로 → 보정 설계(5단계 [건너뛰기]는 10-09 부터 mdat 불러오기)
        win.go(CAL_STEP)
        assert win.current_step == CAL_STEP
        assert isinstance(win.step_widget(), StepCalibration) and win.step_widget() is win.calibration_page
        assert win.steps_bar._current == 6
        assert win.calibration_page.irs_provider is not None
        assert win.calibration_page.positions() == [1]
        assert not win.calibration_page.mimo_check.isEnabled()          # 샘플은 1점 → MIMO 불가
        assert win.next_button.text() == '다음 →'
        assert win.advance() and win.current_step == 8
        assert win.next_button.text() == '완료' and win.next_page() is None
        win.prev_button.click()
        assert win.current_step == CAL_STEP
        win.prev_button.click()
        assert win.current_step == 5
        win.steps_bar.buttons[5].click()               # 진행바 6번째 = 보정 설계
        assert win.current_step == CAL_STEP
        win.steps_bar.buttons[6].click()
        assert win.current_step == 8
        # 옛 6·7 화면은 2026-10-10 슬림화로 지웠다 — 그 번호는 열리지 않는다
        for old in (6, 7, 10):
            with pytest.raises(ValueError):
                win.go(old)
        assert win.current_step == 8
    finally:
        win.close()
        qapp.processEvents()


def test_every_calibration_control_has_a_tip_and_english(qapp, fake, tmp_path):  # noqa: F811
    """새 화면의 입력 위젯마다 풍선 도움말 키가 있고, 영어 화면에 한글이 남지 않는다."""
    import re

    import secs.model as model
    from PyQt6 import QtWidgets
    from secs.i18n import language_manager
    from secs.wizard import tooltips
    from secs.wizard.strings import CAL_STEP

    project = model.Project(name='문구', config=model.SpeakerConfig.from_preset('2.1'))
    win = make_window(project, fake, tmp_path / 'tips.secs')
    try:
        win.go(CAL_STEP)
        page = win.calibration_page
        kinds = (QtWidgets.QAbstractButton, QtWidgets.QComboBox, QtWidgets.QAbstractSpinBox)
        widgets = [w for w in page.findChildren(QtWidgets.QWidget) if isinstance(w, kinds)
                   and not isinstance(w.parentWidget(), (QtWidgets.QAbstractSpinBox, QtWidgets.QComboBox))]
        assert len(widgets) >= 15
        missing = [type(w).__name__ + ':' + (w.text() if hasattr(w, 'text') else '') for w in widgets
                   if not tooltips.resolve(w)[1]]
        assert not missing, missing
        language_manager().set_language('en', persist=False)
        qapp.processEvents()
        hangul = re.compile('[가-힣]')
        left = []
        for w in page.findChildren(QtWidgets.QWidget):
            if isinstance(w, (QtWidgets.QLabel, QtWidgets.QAbstractButton)) and w.isVisibleTo(page):
                if hangul.search(w.text()):
                    left.append(w.text())
            if isinstance(w, QtWidgets.QAbstractSpinBox) and hangul.search(w.specialValueText() if hasattr(w, 'specialValueText') else ''):
                left.append(w.specialValueText())
        assert not left, left
        assert win.steps_bar.buttons[5].text() == 'Room correction'      # 10-10 3차 시험 결정 2A
    finally:
        language_manager().set_language('ko', persist=False)
        win.close()
        qapp.processEvents()
