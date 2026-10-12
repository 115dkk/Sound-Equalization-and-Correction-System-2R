"""MIMO 서포트 FIR 옵션(2026-10-12 사용자: 체크박스 'FIR 사용', 서포트당 8,000탭 상한).

'동작 → 변화' 로 본다:
- 설정 `mimo_fir`(옛 .secs = 끔) · 저장 · 불러오기 · 바꾸면 MIMO 결과가 낡는다(`mimo_key`).
- `compute_mimo(fir=True)`: IIR 맞춤 없이 저속 FIR 100 표본(= 48 kHz 8,000탭) 서포트, 보정 후 응답 · 단계 mdat · 전후 mdat ·
  카밀라 내보내기(Conv wav 8,000 프레임)가 FIR 로.
- `FirSupport.response` 는 내보내는 48 kHz FIR(`fir_to_48k`)의 응답과 같다.
- 화면: 체크 → 설정, [MIMO 계산]이 fir 를 넘기고 MIMO 줄에 'FIR 서포트', 영어에 한글 없음.
합성 방 5.1.2(위치 3)에서 입력 L 하나만 실제 `design_mimo` 로 푼다(R28 시험과 같은 방식).
"""
from __future__ import annotations

import copy

import numpy as np
import pytest

from conftest import load_engine
from test_calibration_r28_mimo_cap import five  # noqa: F401
from test_calibration_step import _compute, _page, fast_engine  # noqa: F401
from test_calibration_sub_combine import fast_ba, two_subs  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401

load_engine()                                       # src 를 sys.path 에(픽스처 없는 시험도)


def test_setting_round_trip_and_key(tmp_path, two_subs):  # noqa: F811
    from secs import calibration as cal
    from secs import model

    project, _irs = two_subs
    old = copy.deepcopy(project)
    old.extra['calibration'] = {'mode': 'mixed', 'mimo_on': True}                 # 10-12 전 .secs 모양
    path = tmp_path / 'old.secs'
    old.save(path)
    s = cal.CalibrationSettings.from_project(model.Project.load(path))
    assert s.mimo_fir is False
    iir_key = s.mimo_key()
    s.mimo_fir = True
    s.store(old)
    old.save(path)
    back = cal.CalibrationSettings.from_project(model.Project.load(path))
    assert back.mimo_fir is True and back.mimo_key() != iir_key


def test_fir_support_response_matches_exported_fir():
    from secs import mimo
    from secs.correction_engine import SR
    from secs.export_camilladsp import fir_to_48k

    rng = np.random.default_rng(12)
    # 실제 서포트 모양(줄어드는 꼬리 * Fsiso 저역 통과) — 흰 잡음 탭은 48 kHz 로 옮길 때 8,000 표본에서 잘린 보간 꼬리 때문에
    # 1 % 쯤 어긋난다(내보내기 쪽 성질, FirSupport 와 무관).
    taps = np.convolve(mimo.lowpass_fir(150.0), rng.standard_normal(mimo.FIR_TAPS_LOW) * np.exp(-np.arange(mimo.FIR_TAPS_LOW) / 15))
    sp = mimo.FirSupport(taps)
    h48 = fir_to_48k(taps, mimo.FS_LOW)
    assert sp.taps_48k == h48.size == mimo.FIR_TAPS_48K == 8000
    nfft = 1 << 15
    f = np.fft.rfftfreq(nfft, 1.0 / SR)
    want = np.fft.rfft(h48, nfft)
    got = sp.response(f)
    band = f < 250
    assert np.max(np.abs(got[band] - want[band])) < 1e-2 * np.max(np.abs(want[band]))
    assert np.all(got[f >= mimo.FS_LOW / 2] == 0)


def _l_only(monkeypatch, taps_seen):
    """`design_mimo` 를 입력 L 하나로 — IIR 맞춤은 불리면 실패(FIR 은 건너뛰어야 한다)."""
    from secs import mimo, mimo_iir

    original = mimo.design_mimo

    def design_mimo(engine, **kw):
        taps_seen.append(kw.get('taps_low'))
        return original(engine, inputs=['L'], **kw)

    def no_iir(*_a, **_k):
        raise AssertionError('FIR 서포트인데 IIR 맞춤을 불렀다')

    monkeypatch.setattr(mimo, 'design_mimo', design_mimo)
    monkeypatch.setattr(mimo_iir, 'design_mimo_iir', no_iir)


def test_compute_fir_skips_iir_and_exports_conv(fast_engine, fast_ba, five, monkeypatch, tmp_path):  # noqa: F811
    from secs import calibration as cal
    from secs import mimo

    project, irs = five
    run = cal.compute(project, irs, cal.CalibrationSettings(mode='zero'))
    taps_seen = []
    _l_only(monkeypatch, taps_seen)
    progress = []
    m = cal.compute_mimo(run, fir=True, progress=lambda k, n: progress.append((k, n)))
    assert taps_seen == [mimo.FIR_TAPS_LOW] and progress == [(1, 1)]
    assert m.fir and cal.mimo_kind(m) == 'FIR' and m.key()[-1] is True
    sups = m.iir['L']
    assert sups and all(isinstance(sp, mimo.FirSupport) and sp.taps_48k == 8000 for sp in sups.values())
    assert any(n.startswith('FIR 서포트') for n in m.notes)
    assert not np.allclose(m.applied['L'], m.final['L'])
    # 단계 · 전후 mdat 이름
    names = [it.title for it in cal.stage_items(run, m)]
    assert any(n.endswith('4 MIMO 적용(FIR)') for n in names)
    names = [it.title for it in cal.before_after_items(run, m)]
    assert any(n.endswith('_3 보정 후 + MIMO(FIR)') for n in names)
    # 카밀라: 서포트 경로마다 Conv wav(8,000 프레임), PEQ 없음
    files = cal.export_camilladsp(run, tmp_path, m)
    text = files[0].read_text(encoding='ascii')
    assert 'MIMO FIR support paths' in text
    wavs = [p for p in files if p.name.startswith('secs_') and '_mimo_L_' in p.name]
    assert len(wavs) == len(sups)
    from scipy.io import wavfile
    for p in wavs:
        _sr, data = wavfile.read(p)
        assert data.shape[0] == 8000
    assert 'mimo_L_' in text and 'mimo_L_' + next(iter(sups)) + '_peq0' not in text
    # 예산 글자
    assert cal.mimo_fir_budget_text(m, 10 ** 9).endswith('안')
    assert cal.mimo_fir_budget_text(m, 1).endswith('초과')


def test_mimo_card_fir_check(qapp, fast_engine, fast_ba, two_subs, tmp_path, monkeypatch):  # noqa: F811
    """체크 → 설정 저장 · MIMO 결과 낡음 → [MIMO 계산]이 fir 를 넘기고 MIMO 줄에 'FIR 서포트'."""
    from secs import mimo, mimo_iir

    calls = []

    def design_mimo(engine, fsiso=150.0, positions=None, f_low=None, exclude=(), max_supports=None, taps_low=None, **_):
        calls.append(taps_low)
        out = mimo.MimoResult()
        out.ranges = {'SW1': (20.0, 120.0), 'SW2': (20.0, 120.0)}
        out.supports = {'L': {'SW1': np.zeros(100), 'SW2': np.zeros(100)}}
        return out

    monkeypatch.setattr(mimo, 'design_mimo', design_mimo)
    monkeypatch.setattr(mimo_iir, 'design_mimo_iir',
                        lambda engine, design, positions, **kw: {'L': {'SW1': mimo_iir.IirSupport(-6.0, 3.0, 1, [], [])}})
    project, irs = two_subs
    page = _page(copy.deepcopy(project), irs, tmp_path)
    try:
        check = page.mimo_fir_check
        assert not check.isChecked() and not check.isEnabled()                     # MIMO 끔
        page.mode_buttons['zero'].click()
        _compute(page, qapp)
        page.mimo_check.click()
        assert check.isEnabled()
        page.mimo_button.click()
        assert page.wait(120000) and page.mimo_run is not None
        assert calls == [None] and 'IIR 서포트' in page.mimo_status.text()
        check.click()
        assert page.project.extra['calibration']['mimo_fir'] is True and page.mimo_stale()
        page.mimo_button.click()
        assert page.wait(120000)
        assert calls == [None, mimo.FIR_TAPS_LOW] and page.mimo_run.fir and not page.mimo_stale()
        assert 'FIR 서포트 2개' in page.mimo_status.text()
    finally:
        page.deleteLater()


def test_mimo_card_fir_is_english(qapp, two_subs, tmp_path):  # noqa: F811
    import re

    from PyQt6 import QtWidgets
    from secs.i18n import language_manager

    project, irs = two_subs
    page = _page(copy.deepcopy(project), irs, tmp_path)
    try:
        language_manager().set_language('en', persist=False)
        qapp.processEvents()
        page.refresh()
        assert page.mimo_fir_check.text() == 'Use FIR'
        labels = [w.text() for w in page.findChildren(QtWidgets.QLabel) if w.isVisibleTo(page)]
        assert 'Support filter' in labels
        assert not [x for x in labels if re.search('[가-힣]', x)]
    finally:
        language_manager().set_language('ko', persist=False)
        page.deleteLater()


@pytest.mark.parametrize('fir', [False, True])
def test_estimate_fir_is_shorter(fir, two_subs):  # noqa: F811
    from secs import calibration as cal

    project, _irs = two_subs
    iir_s = cal.estimate_mimo_seconds(project, 9)
    fir_s = cal.estimate_mimo_seconds(project, 9, fir=True)
    assert fir_s < iir_s
    assert cal.estimate_mimo_seconds(project, 9, fir=fir) == (fir_s if fir else iir_s)
