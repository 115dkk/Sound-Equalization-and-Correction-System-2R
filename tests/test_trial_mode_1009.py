"""체험 모드(2026-10-09 사용자: 별도 창 말고 지금 창을 체험으로, 체험인 걸 확실히 알게).

'동작 → 변화': [체험 모드] → 같은 자리의 창이 체험(키 색 분홍 · 제목 [체험 모드] · 저장 막힘)으로 바뀌고 원래 창은 닫힌다,
[체험 끝내기] → 원래 프로젝트로 돌아온다(원래 색)."""
from __future__ import annotations

from test_wizard_common import engine, fake, make_window, qapp  # noqa: F401
from test_wizard_common import pytestmark  # noqa: F401


def test_trial_swaps_in_place_and_back(qapp, fake, tmp_path, monkeypatch):  # noqa: F811
    import secs.wizard as wiz
    from secs import model

    monkeypatch.setattr(wiz, 'build_backend', lambda fake=False, use_sample_dolby=False: (_f := __import__(
        'secs.rewapi.fake', fromlist=['FakeREW']).FakeREW(delay_s=0.0), _f, _f, _f))
    path = tmp_path / 'real.secs'
    project = model.Project(name='실제', config=model.SpeakerConfig.from_preset('2.1'))
    project.save(path)
    win = make_window(project, fake, str(path))
    win.show()
    try:
        assert not win.trial and '#d6338a' not in win.styleSheet()
        trial = win.trial_button.click() or qapp.instance()._secs_window
        qapp.processEvents()
        assert trial is not win and trial.trial and '#d6338a' in trial.styleSheet()   # 키 색 분홍
        assert trial.windowTitle().startswith('[튜토리얼]') and not win.isVisible()
        assert trial.trial_button.text() == '튜토리얼 끝내기'
        assert not win.step5.sample_button.isVisibleTo(win.step5) and trial.step5.sample_button.isVisibleTo(trial.step5)
        assert trial.save_project(str(tmp_path / 'x.secs')) is None and not (tmp_path / 'x.secs').exists()
        assert trial.calibration_page.export('mdat', folder=tmp_path / 'out') == []
        trial.trial_button.click()
        back = qapp.instance()._secs_window
        qapp.processEvents()
        assert back is not trial and not back.trial and '#d6338a' not in back.styleSheet()
        assert back.current_project_path() == path and back.project.name == '실제'
        back.ask_before_close = False
        back.close()
    finally:
        win.close()


def test_save_button_saves_in_place_and_menu_saves_as(qapp, fake, tmp_path, monkeypatch):  # noqa: F811
    """10-09 사용자: [프로젝트 저장] 은 지금 파일에 바로, 옆 ▾ 의 [다른 이름으로 저장…] 만 위치를 묻는다."""
    from PyQt6 import QtWidgets
    from secs import model

    path = tmp_path / 'p.secs'
    project = model.Project(name='p', config=model.SpeakerConfig.from_preset('2.1'))
    project.save(path)
    win = make_window(project, fake, str(path))
    asked = []
    other = tmp_path / 'q.secs'
    monkeypatch.setattr(QtWidgets.QFileDialog, 'getSaveFileName', lambda *a, **k: (asked.append(1) or str(other), ''))
    try:
        before = path.stat().st_mtime_ns
        win.save_button.click()
        assert not asked and path.stat().st_mtime_ns >= before and win.current_project_path() == path
        win.save_as_action.trigger()
        assert asked and other.exists() and win.current_project_path() == other
    finally:
        win.close()
