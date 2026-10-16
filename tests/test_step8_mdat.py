"""내보내기 단계(화면 번호 8, 2026-09-26 새로 — `secs.wizard.step8_mdat`, `calibration.export_before_after_mdat`).

'동작 → 변화' 로 본다:
- 보정 결과가 없으면 [REW mdat 저장] 이 꺼져 있고 이유가 보이며, 눌러도 파일이 생기지 않는다.
- 보정 설계에서 [계산] → 내보내기 화면이 모드·채널을 보이고, [저장] → REW 가 읽는 mdat 하나에 채널마다
  '보정 전(원 측정)' = 실측 IR 그대로, '보정 후 <모드>' = 엔진 최종 응답(`stage_responses(...)['final']`), 서브는 원 측정.
- MIMO 를 계산했으면 MIMO 가 붙은 입력만 '보정 후 + MIMO(IIR)' 가 더 들어가고, 끄면 빠진다.
- 보정 설계에서 설정을 바꾸면(결과가 낡으면) 내보내기 화면이 알린다.
- 창: 8 = 새 화면(흐름 마지막). 옛 8단계(go(10))는 2026-10-10 슬림화로 지웠다.
보정 계산은 `test_calibration_step` 의 합성 방·빠른 PEQ 맞춤·가짜 MIMO 를 그대로 쓴다.
"""
from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401  (패키지 엔진 전용)
from test_calibration_step import _compute, _page, _project, fake_mimo, fast_engine, synth  # noqa: F401


def _export_page(calibration):
    from secs.wizard.step8_mdat import Step8Mdat

    page = Step8Mdat(calibration.project, calibration=calibration)
    page.set_context(calibration.project_path)
    return page


def test_without_a_result_nothing_is_written(qapp, synth, tmp_path):  # noqa: F811
    case, irs = synth
    calibration = _page(_project(case, 1), {1: irs[1]}, tmp_path)
    page = _export_page(calibration)
    try:
        assert not page.export_button.isEnabled()
        assert '보정 결과가 없습니다' in page.source_label.text()
        assert page.export(folder=tmp_path / 'out') == []
        assert '보정 결과가 없습니다' in page.export_note.text()
        assert not (tmp_path / 'out').exists()
    finally:
        page.deleteLater()
        calibration.deleteLater()


def test_export_writes_before_and_after_per_channel(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    from secs import rew_mdat
    from secs.correction_engine import N_FFT

    case, irs = synth
    project = _project(case, 1)
    calibration = _page(project, {1: irs[1]}, tmp_path)
    page = _export_page(calibration)
    try:
        calibration.mode_buttons['zero'].click()
        run = _compute(calibration, qapp)
        page.refresh()
        assert page.export_button.isEnabled()
        assert '최소 지연 모드' in page.source_label.text() and '채널 2개 보정 전·후' in page.source_label.text()
        assert page.roles_label.text().split(' · ') == ['L', 'R', 'SW1']              # 돌비 표기(role_code), 번역 없음
        assert page.rc_label.text().startswith('RC 상한')

        out = tmp_path / '한글 폴더'
        files = page.export(folder=out)
        assert len(files) == 1 and Path(files[0]).parent.parent == out
        assert Path(files[0]).name == 'SECS 보정 전후 최소 지연 모드.mdat'
        assert '1개 파일을 저장했습니다' in page.export_note.text()
        loaded = {m.name: m for m in rew_mdat.load(files[0]).measurements}
        assert list(loaded) == ['01_L_1 보정 전(원 측정)', '01_L_2 보정 후 최소 지연 모드',
                                '02_R_1 보정 전(원 측정)', '02_R_2 보정 후 최소 지연 모드',
                                '03_SW1_1 보정 전(원 측정)']
        # 계산값을 '측정' 이라 부르지 않는다(R18) — '측정' 은 원 측정에만.
        assert all(('측정' in name) == name.endswith('(원 측정)') for name in loaded)
        assert all('계산값' in m.notes for name, m in loaded.items() if '보정 후' in name)

        ir, _rate, t0 = irs[1]['L']
        before = loaded['01_L_1 보정 전(원 측정)'].impulse_response
        assert np.allclose(before.samples[:ir.size] / 0.01, np.asarray(ir, np.float32), atol=1e-5)
        assert before.start_time_s == pytest.approx(t0, abs=1e-6)

        engine = run.engine
        want = np.fft.irfft(engine.stage_responses('L', engine.long_spectra(), engine.main_position)['final'], N_FFT)
        after = loaded['01_L_2 보정 후 최소 지연 모드'].impulse_response.samples[:N_FFT]
        assert np.allclose(after / 0.01, want.astype(np.float32), atol=1e-5 * float(np.max(np.abs(want))))
        assert not np.allclose(after[:ir.size], before.samples[:ir.size])       # 전·후가 실제로 다르다

        calibration.mode_buttons['mixed'].click()             # 보정 설계 설정이 바뀌면 알린다(마지막 결과는 낼 수 있다)
        page.refresh()
        assert '설정·측정이 바뀌었습니다' in page.source_label.text() and page.export_button.isEnabled()
    finally:
        page.deleteLater()
        calibration.deleteLater()


def test_mimo_items_follow_the_mimo_switch(qapp, fast_engine, synth, fake_mimo, tmp_path):  # noqa: F811
    from secs import rew_mdat

    case, irs = synth
    calibration = _page(_project(case, 3), irs, tmp_path)
    page = _export_page(calibration)
    try:
        calibration.mode_buttons['zero'].click()
        _compute(calibration, qapp)
        calibration.mimo_check.click()
        calibration.mimo_button.click()
        assert calibration.wait(120000) and calibration.mimo_run is not None
        page.refresh()
        assert 'MIMO 적용 1개' in page.source_label.text()
        names = [m.name for m in rew_mdat.load(page.export(folder=tmp_path / 'a')[0]).measurements]
        assert '01_L_3 보정 후 + MIMO(IIR)' in names and not any(n.startswith('02_R_3') for n in names)

        calibration.mimo_check.click()                         # MIMO 를 끄면 내보내기에서도 빠진다
        page.refresh()
        names = [m.name for m in rew_mdat.load(page.export(folder=tmp_path / 'b')[0]).measurements]
        assert not any('MIMO' in n for n in names)
    finally:
        page.deleteLater()
        calibration.deleteLater()


def test_window_step8_is_the_new_export(qapp, fake, tmp_path):  # noqa: F811
    import re

    import secs.model as model
    from PyQt6 import QtWidgets
    from secs.i18n import language_manager
    from secs.wizard import tooltips
    from secs.wizard.step8_mdat import Step8Mdat
    from secs.wizard.strings import CAL_STEP, FLOW

    assert FLOW[-1] == 8                                   # 내보내기가 흐름 끝(옛 8단계 go(10) 은 10-10 에 지움)
    project = model.Project(name='내보내기', config=model.SpeakerConfig.from_preset('2.1'))
    win = make_window(project, fake, tmp_path / 'export.secs')
    try:
        win.go(CAL_STEP)
        assert win.advance() and win.current_step == 8
        page = win.step_widget()
        assert isinstance(page, Step8Mdat) and page is win.export_page is win.step8
        assert page.calibration is win.calibration_page
        assert page.export_base_dir().parent.parent == tmp_path and page.export_base_dir().parent.name == "SECS_export"
        assert not page.export_button.isEnabled() and '보정 결과가 없습니다' in page.source_label.text()
        assert win.next_page() is None
        buttons = [w for w in page.findChildren(QtWidgets.QAbstractButton)]
        assert buttons and all(tooltips.resolve(w)[1] for w in buttons)
        assert win.steps_bar._current == 7 and win.previous_page() == CAL_STEP

        win.go(8)
        language_manager().set_language('en', persist=False)
        qapp.processEvents()
        hangul = re.compile('[가-힣]')
        left = [w.text() for w in page.findChildren(QtWidgets.QWidget)
                if isinstance(w, (QtWidgets.QLabel, QtWidgets.QAbstractButton)) and w.isVisibleTo(page)
                and hangul.search(w.text())]
        assert not left, left
    finally:
        language_manager().set_language('ko', persist=False)
        win.close()
        qapp.processEvents()


def test_filter_card_exports_camilla_text_and_eqapo(qapp, fast_engine, synth, tmp_path):  # noqa: F811
    """REW mdat 아래 '필터 내보내기' 카드(10-15): 카밀라 · 텍스트 · EqAPO 버튼, 결과는 카드 자기 안내 줄에."""
    case, irs = synth
    calibration = _page(_project(case, 1), {1: irs[1]}, tmp_path)
    page = _export_page(calibration)
    try:
        assert list(page.filter_buttons) == ['camilla', 'text', 'eqapo']
        assert page.camilla_button is page.filter_buttons['camilla']
        assert not any(b.isEnabled() for b in page.filter_buttons.values())          # 결과 없으면 꺼짐
        assert page.export(folder=tmp_path / 'none', kind='eqapo') == []
        assert '보정 결과가 없습니다' in page.filter_note.text() and page.export_note.text() == ''

        calibration.mode_buttons['zero'].click()
        _compute(calibration, qapp)
        page.refresh()
        assert page.filter_buttons['camilla'].isEnabled() and page.filter_buttons['eqapo'].isEnabled()
        out = tmp_path / 'out'
        eq = page.export(folder=out, kind='eqapo')
        assert Path(eq[0]).name == 'config.txt' and Path(eq[0]).parent.name.startswith('SECS_eqapo_RC_BA_zero_')
        assert (Path(eq[0]).parent / 'secs_zero' / 'secs_zero.txt').exists()
        assert '파일을 저장했습니다' in page.filter_note.text() and page.export_note.text() == ''
        # 서브(BA)가 있는 결과 — 텍스트는 RC 만이라 꺼지고 이유가 카드에(10-15 사용자)
        assert not page.filter_buttons['text'].isEnabled() and '서브(BA)' in page.filter_scope.text()
        assert page.export(folder=out, kind='text') == [] and '서브(BA)' in page.filter_note.text()
        cam = page.export(folder=out, kind='camilla')
        assert Path(cam[0]).suffix == '.yml'
    finally:
        page.deleteLater()
        calibration.deleteLater()


def test_eqapo_is_off_with_mimo(qapp, fast_engine, synth, fake_mimo, tmp_path):  # noqa: F811
    """EqAPO 는 RC · BA 까지(10-15 사용자: MIMO 는 경로가 많아 EqAPO 에서 무겁다) — MIMO 를 켜면 꺼지고, 끄면 켜진다."""
    case, irs = synth
    calibration = _page(_project(case, 3), irs, tmp_path)
    page = _export_page(calibration)
    try:
        calibration.mode_buttons['zero'].click()
        _compute(calibration, qapp)
        calibration.mimo_check.click()
        calibration.mimo_button.click()
        assert calibration.wait(120000) and calibration.mimo_run is not None
        page.refresh()
        assert not page.filter_buttons['eqapo'].isEnabled() and page.filter_buttons['camilla'].isEnabled()
        assert 'EqAPO 는 RC · BA 까지' in page.filter_scope.text()
        assert page.export(folder=tmp_path / 'm', kind='eqapo') == [] and not (tmp_path / 'm').exists()
        calibration.mimo_check.click()
        page.refresh()
        assert page.filter_buttons['eqapo'].isEnabled() and 'EqAPO' not in page.filter_scope.text()
    finally:
        page.deleteLater()
        calibration.deleteLater()
