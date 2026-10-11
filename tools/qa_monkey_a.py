# -*- coding: utf-8 -*-
"""실사용 QA A(1~4단계 + 공통) 몽키 테스트 — 모든 버튼·체크·드롭다운·스핀박스를 눌러 보고
순서 밖 조작까지 시험한다(2026-09-23 사용자 지시).

`qa_capture.py` 는 창 크기별 스크린샷 + 정적 점검(가로 스크롤·잘림)이 전부였다. 이 도구는
그 위에 **조작**을 더한다: 버튼 클릭, 체크박스 토글, 콤보 전 항목, 스핀박스 상하한, 프리셋
연타, 서브 0↔8, 바이앰프 반복, 스피커 전부 삭제 후 진행, 출력 중복, 언어 전환 후 재방문,
저장→불러오기 왕복. 각 조작 뒤 예외·Qt 경고·가로 스크롤/잘림(`qa_capture` 점검 재사용)·
형제 위젯 겹침·화면-모델 불일치를 검사한다.

FakeREW(REW 없이 도는 가짜)로 돈다. **오프스크린 플랫폼 금지**(글꼴이 없어 폭이 틀린다) —
실제 windows 플랫폼에서 `WA_DontShowOnScreen` 로 화면에 띄우지 않고 검사한다.

    .venv-secs\\Scripts\\python.exe tools\\qa_monkey_a.py --preset 2.1
    .venv-secs\\Scripts\\python.exe tools\\qa_monkey_a.py --all

결과: 표준 출력에 문제 목록(마크다운 표) + 스크린샷은 `--out` 아래 문제 있는 것만 남긴다.
"""
from __future__ import annotations

import argparse
import datetime as _dt
import os
import sys
import tempfile
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
sys.path.insert(0, str(ROOT / 'tools'))
os.environ.pop('QT_QPA_PLATFORM', None)      # 오프스크린 금지 — qa_capture.py 와 같은 이유

import qa_capture  # noqa: E402 - sys.path 조작 뒤 임포트

SIZES = qa_capture.SIZES  # 1024x640, 1280x800, 1920x1080, 2560x1440, 1024x420
#: 사용자 지시의 좁은 세로 크기(qa_capture 의 극단 폭과 별개로 이번 회차가 추가로 요구).
NARROW_TALL = (800, 1200)

#: 이번 회차 담당 범위 — 1~4단계 + 공통(사이드바·상단 진행바·도움말·저장/불러오기·언어·닫기).
STEPS_IN_SCOPE = (1, 2, 3, 4)


# ──────────────────────────── 문제 수집 ────────────────────────────
class Issue:
    def __init__(self, screen, size, lang, action, symptom, repro, severity, guess):
        self.screen = screen
        self.size = size
        self.lang = lang
        self.action = action
        self.symptom = symptom
        self.repro = repro
        self.severity = severity
        self.guess = guess
        self.screenshot = ''


ISSUES: list[Issue] = []
EXCEPTIONS: list[str] = []


def _excepthook(etype, value, tb):
    EXCEPTIONS.append(''.join(traceback.format_exception(etype, value, tb)))


_IGNORED_QT_SUBSTRINGS = (
    'QWindowsWindow::setGeometry',
    'QBackingStore::endPaint',
    'setStyleSheet: No stylesheet found',
)


def _qt_msg_handler(_mode, _ctx, message) -> None:
    if any(s in message for s in _IGNORED_QT_SUBSTRINGS):
        return
    EXCEPTIONS.append(f'[Qt] {message}')


def install_hooks() -> None:
    sys.excepthook = _excepthook
    from PyQt6 import QtCore
    QtCore.qInstallMessageHandler(_qt_msg_handler)


def patch_modals() -> None:
    """QFileDialog·QMessageBox 가 뜨면 테스트가 멈춘다(CLAUDE.md 알려진 함정) — 막는다."""
    from PyQt6 import QtCore
    from PyQt6 import QtWidgets as RealQt

    RealQt.QFileDialog.getSaveFileName = staticmethod(lambda *a, **k: ('', ''))
    RealQt.QFileDialog.getOpenFileName = staticmethod(lambda *a, **k: ('', ''))
    RealQt.QFileDialog.getExistingDirectory = staticmethod(lambda *a, **k: '')
    RealQt.QFileDialog.getOpenFileNames = staticmethod(lambda *a, **k: ([], ''))

    def _rank(box, button) -> int:
        order = {
            RealQt.QMessageBox.ButtonRole.DestructiveRole: 0,
            RealQt.QMessageBox.ButtonRole.RejectRole: 1,
            RealQt.QMessageBox.ButtonRole.AcceptRole: 2,
        }
        return order.get(box.buttonRole(button), 3)

    def _auto_exec(self):
        buttons = self.buttons()
        if buttons:
            target = sorted(buttons, key=lambda b: _rank(self, b))[0]
            QtCore.QTimer.singleShot(0, target.click)
        return RealQt.QDialog.exec(self)

    RealQt.QMessageBox.exec = _auto_exec
    from secs.i18n import QMessageBox as IQMessageBox
    IQMessageBox.exec = _auto_exec

    def _auto_menu_exec(self, *_a, **_k):
        # 1~4단계·공통에는 우클릭 메뉴가 없지만(구조 안내 확인), 혹시 몰라 막아 둔다.
        return None
    RealQt.QMenu.exec = _auto_menu_exec


# ──────────────────────────── 창 만들기 ────────────────────────────
def build_window(preset: str, biamp: bool, tmp: Path):
    import secs.model as model
    import secs.rewapi as rewapi
    from secs.wizard import WizardWindow

    project = model.Project(config=model.SpeakerConfig.from_preset(preset, biamp=biamp))
    project.extra['array'] = 9
    rew = rewapi.FakeREW(delay_s=0.0)
    win = WizardWindow(project, rew, rew, rew, rew, project_path=tmp / 'qa.secs')
    win.ask_before_close = False
    return win


def settle(app, rounds: int = 4) -> None:
    for _ in range(rounds):
        app.processEvents()


# ──────────────────────────── 정적 점검(재사용 + 겹침) ────────────────────────────
def sibling_overlaps(area) -> list[str]:
    """같은 부모를 둔 보이는 위젯끼리 상당히 겹치면 보고한다(레이아웃 붕괴 의심).

    busy 패널 같은 의도된 오버레이는 다른 조상(스택)에 얹히므로 대상이 아니다.
    `parent.children()` 는 직계 자식만 순서대로 준다(재귀 없음) — 형제만 비교한다.
    """
    from PyQt6 import QtWidgets

    found: list[str] = []
    parents = {w.parentWidget() for w in area.findChildren(QtWidgets.QWidget)}
    for parent in parents:
        if parent is None:
            continue
        kids = [w for w in parent.children()
                if isinstance(w, QtWidgets.QWidget) and w.isVisible()
                and w.width() > 4 and w.height() > 4]
        for i in range(len(kids)):
            for j in range(i + 1, len(kids)):
                a, b = kids[i].geometry(), kids[j].geometry()
                inter = a.intersected(b)
                if inter.width() > 4 and inter.height() > 4:
                    found.append(f'{type(kids[i]).__name__}×{type(kids[j]).__name__} '
                                 f'{inter.width()}x{inter.height()}px @ {parent.objectName() or type(parent).__name__}')
    return found


def layout_checks(win, area) -> dict:
    hbar = area.horizontalScrollBar() if hasattr(area, 'horizontalScrollBar') else None
    hscroll = hbar.maximum() if hbar is not None else 0
    clipped = qa_capture.clipped_widgets(area)
    if hscroll:
        clipped = clipped + qa_capture.overflow_widgets(area)
    overlaps = sibling_overlaps(area)
    return {'hscroll': hscroll, 'clipped': clipped, 'overlaps': overlaps}


def report_layout(ctx, action: str, checks: dict) -> None:
    if checks['hscroll']:
        add_issue(ctx, action, f'가로 스크롤 {checks["hscroll"]}px', '중',
                  'qa_capture 류 자동 점검(hscroll)')
    for line in checks['clipped'][:5]:
        add_issue(ctx, action, f'글자 잘림/넘침: {line}', '중', 'qa_capture 류 자동 점검(clip)')
    for line in checks['overlaps'][:5]:
        add_issue(ctx, action, f'위젯 겹침: {line}', '하', '겹침 자동 점검(형제 geometry)')


def check_exceptions(ctx, action: str) -> None:
    global EXCEPTIONS
    for exc in EXCEPTIONS:
        add_issue(ctx, action, '예외/Qt 경고: ' + exc.strip().splitlines()[-1], '상', exc.strip())
    EXCEPTIONS = []


def add_issue(ctx: dict, action: str, symptom: str, severity: str, guess: str) -> None:
    issue = Issue(ctx['screen'], ctx['size'], ctx['lang'], action, symptom,
                  ctx['repro'], severity, guess)
    ISSUES.append(issue)


def snapshot_if_bad(win, out: Path, tag: str, before_count: int) -> None:
    if len(ISSUES) > before_count:
        path = out / f'A_{tag}.png'
        win.grab().save(str(path))
        for issue in ISSUES[before_count:]:
            if not issue.screenshot:
                issue.screenshot = path.name


# ──────────────────────────── 위젯 조작 ────────────────────────────
def all_buttons(container):
    """QCheckBox 는 QAbstractButton 이지만 `toggle_each_checkbox` 가 따로 다룬다(중복 클릭 방지)."""
    from PyQt6 import QtWidgets
    return [b for b in container.findChildren(QtWidgets.QAbstractButton)
            if b.isVisible() and b.isEnabled() and not isinstance(b, QtWidgets.QCheckBox)]


def click_each(app, container, ctx, out: Path, tag_prefix: str, skip_object_names=()) -> None:
    """현재 존재하는 버튼을 한 번씩 누른다. 누르는 동안 표가 다시 만들어져도(1단계)
    매번 새로 찾아 안전하게 진행한다."""
    clicked_ids: set[int] = set()
    guard = 0
    while guard < 400:
        guard += 1
        candidates = [b for b in all_buttons(container) if b.objectName() not in skip_object_names]
        target = next((b for b in candidates if id(b) not in clicked_ids), None)
        if target is None:
            break
        clicked_ids.add(id(target))
        label = target.text().replace('\n', ' ')[:20] or target.objectName() or type(target).__name__
        before = len(ISSUES)
        try:
            target.click()
            settle(app)
        except RuntimeError:
            continue          # 클릭 사이 위젯이 재생성돼 지워진 경우(09-20 알려진 함정과 같은 부류)
        except Exception:
            EXCEPTIONS.append(traceback.format_exc())
        check_exceptions(ctx, f'{tag_prefix} 버튼 "{label}" 클릭')
        snapshot_if_bad(win_ref[0], out, f'{tag_prefix}_{label}_{guard}', before)


def _sample_indices(count: int, max_samples: int) -> list[int]:
    """항목이 많은 콤보(출력 채널 64개 등)는 전부 돌면 너무 느리다 — 처음·끝·균등 간격 표본만."""
    if count <= max_samples:
        return list(range(count))
    step = (count - 1) / (max_samples - 1)
    return sorted({round(i * step) for i in range(max_samples)})


def cycle_each_combo(app, container, ctx, out: Path, tag_prefix: str, max_samples: int = 5) -> None:
    """콤보를 전부 찾아 표본 항목을 눌러 본다.

    표 안 콤보(역할·크기·구동·출력)는 값이 바뀌면 표 전체가 다시 만들어져(1단계
    `_rebuild_table`) 같이 찾아 둔 나머지 콤보 참조가 죽은 C++ 객체가 된다 — 그 뒤
    콤보는 `sip.isdeleted()`/`RuntimeError` 로 조용히 건너뛴다(버그 아님, 커버리지만 준다).
    """
    from PyQt6 import QtWidgets, sip

    combos = [c for c in container.findChildren(QtWidgets.QComboBox) if c.isVisible() and c.isEnabled()]
    for combo in combos:
        if sip.isdeleted(combo):
            continue
        name = combo.objectName() or type(combo).__name__
        try:
            count = combo.count()
        except RuntimeError:
            continue
        if count <= 1:
            continue
        original = combo.currentIndex()
        for index in _sample_indices(count, max_samples):
            if sip.isdeleted(combo):
                break
            before = len(ISSUES)
            try:
                combo.setCurrentIndex(index)
                settle(app)
            except RuntimeError:
                break
            except Exception:
                EXCEPTIONS.append(traceback.format_exc())
                check_exceptions(ctx, f'{tag_prefix} 콤보 "{name}" 항목 {index}')
            else:
                check_exceptions(ctx, f'{tag_prefix} 콤보 "{name}" 항목 {index}')
            snapshot_if_bad(win_ref[0], out, f'{tag_prefix}_{name}_{index}', before)
        try:
            if not sip.isdeleted(combo):
                combo.setCurrentIndex(original)
                settle(app)
        except RuntimeError:
            pass


def toggle_each_checkbox(app, container, ctx, out: Path, tag_prefix: str) -> None:
    from PyQt6 import QtWidgets
    boxes = [c for c in container.findChildren(QtWidgets.QCheckBox) if c.isVisible() and c.isEnabled()]
    for box in boxes:
        name = box.objectName() or box.text()[:16] or 'checkbox'
        for _ in range(2):        # 켜고 끄고
            before = len(ISSUES)
            try:
                box.click()
                settle(app)
            except RuntimeError:
                break
            except Exception:
                EXCEPTIONS.append(traceback.format_exc())
                check_exceptions(ctx, f'{tag_prefix} 체크 "{name}" 토글')
            else:
                check_exceptions(ctx, f'{tag_prefix} 체크 "{name}" 토글')
            snapshot_if_bad(win_ref[0], out, f'{tag_prefix}_{name}', before)


def nudge_each_spinbox(app, container, ctx, out: Path, tag_prefix: str) -> None:
    from PyQt6 import QtWidgets
    boxes = [s for s in container.findChildren(QtWidgets.QAbstractSpinBox) if s.isVisible() and s.isEnabled()]
    for spin in boxes:
        name = spin.objectName() or 'spin'
        before = len(ISSUES)
        try:
            if hasattr(spin, 'setValue') and hasattr(spin, 'maximum'):
                original = spin.value()
                spin.setValue(spin.maximum())
                settle(app)
                spin.setValue(spin.minimum())
                settle(app)
                spin.setValue(original)
                settle(app)
        except RuntimeError:
            continue
        except Exception:
            EXCEPTIONS.append(traceback.format_exc())
            check_exceptions(ctx, f'{tag_prefix} 스핀 "{name}" 상하한')
        else:
            check_exceptions(ctx, f'{tag_prefix} 스핀 "{name}" 상하한')
        snapshot_if_bad(win_ref[0], out, f'{tag_prefix}_{name}', before)


def slide_sliders(app, container, ctx, out: Path, tag_prefix: str) -> None:
    from PyQt6 import QtWidgets
    sliders = [s for s in container.findChildren(QtWidgets.QSlider) if s.isVisible() and s.isEnabled()]
    for slider in sliders:
        before = len(ISSUES)
        try:
            original = slider.value()
            slider.setValue(slider.maximum())
            settle(app)
            slider.setValue(slider.minimum())
            settle(app)
            slider.setValue(original)
            settle(app)
        except Exception:
            EXCEPTIONS.append(traceback.format_exc())
        check_exceptions(ctx, f'{tag_prefix} 슬라이더 상하한')
        snapshot_if_bad(win_ref[0], out, f'{tag_prefix}_slider', before)


win_ref: list = [None]  # click_each 등에서 현재 창을 참조하기 위한 얕은 전역(스크린샷용)


# ──────────────────────────── 화면-모델 불일치 점검 ────────────────────────────
def check_model_consistency(win, ctx) -> None:
    step = win.current_step
    project = win.project
    before = len(ISSUES)
    try:
        if step == 2:                     # 구성(09-23 부터 2단계)
            s1 = win.config_page
            if int(s1._sub_count.text()) != project.config.sub_count():
                add_issue(ctx, '모델 일치', '서브 카운터 표시가 project.config.sub_count() 와 다름',
                          '상', 'step2_config.py _refresh_top/_sub_count')
            if s1._biamp is not None and s1._biamp.isChecked() != bool(project.config.biamp_default):
                add_issue(ctx, '모델 일치', '바이앰프 체크 표시가 config.biamp_default 와 다름',
                          '상', 'step2_config.py _refresh_top')
            tap_data = s1.dsp_taps.currentData()
            if tap_data is not None and int(tap_data) != int(project.dsp.fir_taps):
                add_issue(ctx, '모델 일치', 'FIR 탭 콤보 값이 project.dsp.fir_taps 와 다름',
                          '상', 'step2_config.py _refresh_dsp')
        elif step == 1:                   # 장치·마이크
            s2 = win.devices_page
            stored = dict(project.extra.get('devices', {}) or {})
            if stored.get('sweep_length') and s2.sweep_combo.currentData() not in (None, stored.get('sweep_length')):
                add_issue(ctx, '모델 일치', '스윕 길이 콤보가 project.extra 저장값과 다름',
                          '중', 'step1_devices.py _store/_refresh_devices')
        elif step == 4:
            s4 = win.step4
            if s4.count() != int(project.extra.get('array', 9) or 9):
                add_issue(ctx, '모델 일치', '배치 선택 count() 가 project.extra["array"] 와 다름',
                          '상', 'step4_layout.py count()/set_count()')
    except Exception:
        EXCEPTIONS.append(traceback.format_exc())
        check_exceptions(ctx, '모델 일치 점검 자체 예외')
    return before


# ──────────────────────────── 시나리오 ────────────────────────────
def visit_and_probe(win, app, out: Path, ctx: dict) -> None:
    """현재 단계에 들어간 상태에서 정적 점검 + 위젯 조작 + 모델 일치 점검을 한 번 돈다."""
    win.go(ctx['step'])
    settle(app)
    area = win.stack.currentWidget()
    widget = win.step_widget()

    before = len(ISSUES)
    checks = layout_checks(win, area)
    report_layout(ctx, '진입 직후 정적 점검', checks)
    check_exceptions(ctx, '진입')
    snapshot_if_bad(win, out, f"enter_step{ctx['step']}", before)

    win_ref[0] = win
    prefix = f"s{ctx['step']}"
    # 구성(2단계)의 프리셋·× 삭제 버튼은 매번 다 눌리면 구성이 매 방문마다 무너진다 —
    # 이 둘은 `test_sequence_violations` 가 의도적인 순서·조합으로 따로 시험한다.
    skip = ('preset', 'iconbtn') if ctx['step'] == 2 else ()
    click_each(app, widget, ctx, out, prefix, skip_object_names=skip)
    cycle_each_combo(app, widget, ctx, out, prefix)
    toggle_each_checkbox(app, widget, ctx, out, prefix)
    nudge_each_spinbox(app, widget, ctx, out, prefix)
    slide_sliders(app, widget, ctx, out, prefix)

    before = len(ISSUES)
    check_model_consistency(win, ctx)
    snapshot_if_bad(win, out, f"model_step{ctx['step']}", before)

    before = len(ISSUES)
    checks = layout_checks(win, area)
    report_layout(ctx, '조작 뒤 정적 점검', checks)
    snapshot_if_bad(win, out, f"after_step{ctx['step']}", before)


def test_common(win, app, out: Path, ctx: dict) -> None:
    """사이드바·상단 진행바·도움말·저장/불러오기·언어 전환·닫기."""
    action = '공통'
    # 상단 진행바 점 클릭으로 단계 이동
    for index, button in enumerate(win.steps_bar.buttons, start=1):
        before = len(ISSUES)
        try:
            button.click()
            settle(app)
        except Exception:
            EXCEPTIONS.append(traceback.format_exc())
        check_exceptions(ctx, f'{action} 상단 진행바 {index}단계 클릭')
        snapshot_if_bad(win, out, f'stepsbar_{index}', before)
    win.go(1)
    settle(app)

    # 도움말(F1 대신 버튼) — 비모달, 열고 닫기
    before = len(ISSUES)
    try:
        win.show_help()
        settle(app)
        dlg = getattr(win, '_help_dialog', None)
        if dlg is None or not dlg.isVisible():
            add_issue(ctx, action, '도움말 버튼을 눌러도 창이 안 뜸', '상', 'window.py show_help')
        else:
            for step in STEPS_IN_SCOPE:
                dlg.set_step(step)
                settle(app)
            dlg.close()
    except Exception:
        EXCEPTIONS.append(traceback.format_exc())
    check_exceptions(ctx, f'{action} 도움말 열기/단계별 내용')
    snapshot_if_bad(win, out, 'help', before)

    # 저장 → 불러오기 왕복(파일 대화상자는 막혀 있으므로 경로를 직접 준다)
    before = len(ISSUES)
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / 'roundtrip.secs'
        try:
            win.go(1)
            settle(app)
            original_subs = win.project.config.sub_count()
            original_biamp = win.project.config.biamp_default
            original_speakers = len(win.project.config.speakers)
            saved = win.save_project(str(path))
            if saved is None or not path.exists():
                add_issue(ctx, action, '저장 버튼(경로 지정)으로 .secs 파일이 안 만들어짐', '상',
                          'window.py save_project')
            else:
                loaded = win.load_project(str(path))
                settle(app)
                if (win.project.config.sub_count() != original_subs
                        or win.project.config.biamp_default != original_biamp
                        or len(win.project.config.speakers) != original_speakers):
                    add_issue(ctx, action, '저장→불러오기 왕복 뒤 값이 달라짐(서브 수/바이앰프/스피커 수)',
                              '상', 'window.py save_project/load_project')
        except Exception:
            EXCEPTIONS.append(traceback.format_exc())
    check_exceptions(ctx, f'{action} 저장→불러오기 왕복')
    snapshot_if_bad(win, out, 'save_load_roundtrip', before)

    # 사이드바 값 상시 갱신 확인(구성 요약이 실제 config 와 같은 스피커 수를 말하는지)
    before = len(ISSUES)
    try:
        text = win.side_layout.text()
        speakers_now = len(win.project.config.speakers)
        outs_now = sum(len(s.bands) for s in win.project.config.speakers)
        if f'출력 {outs_now}' not in text and f'Outputs {outs_now}' not in text:
            add_issue(ctx, action, f'사이드바 구성 요약이 출력 수({outs_now})와 안 맞음: "{text}"',
                      '중', 'window.py _refresh_sidebar')
    except Exception:
        EXCEPTIONS.append(traceback.format_exc())
    check_exceptions(ctx, f'{action} 사이드바 표시 확인')
    snapshot_if_bad(win, out, 'sidebar_summary', before)


def test_sequence_violations(win, app, out: Path, ctx: dict) -> None:
    action = '순서 밖 조작'

    # ① 프리셋 연타(2.0 → 9.1.6 → 2.1 → 2.0 빠르게)
    before = len(ISSUES)
    s1 = win.config_page
    win.go(2)
    settle(app)
    try:
        for key in ('9.1.6', '2.0', '9.1.6', '2.1', '2.0'):
            s1._preset_buttons[key].click()
            app.processEvents()
    except Exception:
        EXCEPTIONS.append(traceback.format_exc())
    check_exceptions(ctx, f'{action} 프리셋 연타')
    checks = layout_checks(win, win.stack.currentWidget())
    report_layout(ctx, f'{action} 프리셋 연타 뒤', checks)
    snapshot_if_bad(win, out, 'preset_mash', before)

    # ② 서브 0 ↔ 8 반복
    before = len(ISSUES)
    try:
        for _ in range(3):
            for _ in range(9):
                s1._sub_plus.click()
                app.processEvents()
            if win.project.config.sub_count() != 8:
                add_issue(ctx, action, f'서브를 9번 눌러도 8에서 안 멈춤(현재 {win.project.config.sub_count()})',
                          '상', 'model.py SpeakerConfig.set_sub_count 상한')
            for _ in range(9):
                s1._sub_minus.click()
                app.processEvents()
            if win.project.config.sub_count() != 0:
                add_issue(ctx, action, f'서브를 9번 내려도 0에서 안 멈춤(현재 {win.project.config.sub_count()})',
                          '상', 'model.py SpeakerConfig.set_sub_count 하한')
    except Exception:
        EXCEPTIONS.append(traceback.format_exc())
    check_exceptions(ctx, f'{action} 서브 0↔8 반복')
    snapshot_if_bad(win, out, 'sub_0_8_cycle', before)
    s1._apply_preset('2.1')
    settle(app)

    # ③ 바이앰프 켜고 끄기 반복(칸은 `secs.wizard.BIAMP_UI` 일 때만 — 10-11 부터 닫혀 None)
    before = len(ISSUES)
    try:
        for _ in range(6 if s1._biamp is not None else 0):
            s1._biamp.click()
            app.processEvents()
    except Exception:
        EXCEPTIONS.append(traceback.format_exc())
    check_exceptions(ctx, f'{action} 바이앰프 반복')
    snapshot_if_bad(win, out, 'biamp_cycle', before)

    # ④ 같은 출력 중복 지정
    before = len(ISSUES)
    try:
        if len(win.project.config.speakers) >= 2:
            first_out = win.project.config.speakers[0].bands[0].out
            win.project.config.speakers[1].bands[0].out = first_out
            s1.refresh()
            settle(app)
            messages = win.project.config.validate()
            if not messages:
                add_issue(ctx, action, '출력을 중복 지정해도 config.validate() 가 오류를 안 냄',
                          '상', 'model.py SpeakerConfig.validate')
            ok, _msg = win.can_advance()
            if ok:
                add_issue(ctx, action, '출력 중복 상태에서도 다음 단계로 진행 가능(can_advance True)',
                          '상', 'step2_config.py Step2Config.can_advance')
    except Exception:
        EXCEPTIONS.append(traceback.format_exc())
    check_exceptions(ctx, f'{action} 출력 중복 지정')
    checks = layout_checks(win, win.stack.currentWidget())
    report_layout(ctx, f'{action} 출력 중복 표시', checks)
    snapshot_if_bad(win, out, 'duplicate_output', before)
    s1._apply_preset('2.1')
    settle(app)

    # ⑤ 스피커 전부 삭제 후 진행 시도
    before = len(ISSUES)
    try:
        while win.project.config.speakers:
            win.project.config.speakers.pop(0)
        win.project.config.preset = 'custom'
        s1.refresh()
        settle(app)
        ok, message = win.can_advance()
        if ok:
            add_issue(ctx, action, '스피커를 전부 지워도 다음 단계로 진행 가능(can_advance True)',
                      '상', 'step2_config.py Step2Config.can_advance')
        try:
            win.advance()
            settle(app)
            if win.current_step != 1:
                add_issue(ctx, action, '스피커 0대인데 [다음] 이 실제로 단계를 넘김',
                          '상', 'window.py advance()/can_advance()')
        except Exception:
            EXCEPTIONS.append(traceback.format_exc())
            add_issue(ctx, action, '스피커 0대에서 [다음] 을 누르면 예외 발생', '상',
                      traceback.format_exc().strip().splitlines()[-1])
    except Exception:
        EXCEPTIONS.append(traceback.format_exc())
    check_exceptions(ctx, f'{action} 스피커 전부 삭제 후 진행')
    snapshot_if_bad(win, out, 'delete_all_speakers', before)
    s1._apply_preset(ctx.get('preset', '2.1'))
    if s1._biamp is not None:
        s1._biamp.setChecked(ctx.get('biamp', False))
    settle(app)

    # ⑥ 측정 전(1~4단계도 안 채운 채) 5단계로 건너뛰기 시도 — go() 는 직접 부르면 이동은 허용된다.
    before = len(ISSUES)
    try:
        win.go(5)
        settle(app)
        win.go(1)
        settle(app)
    except Exception:
        EXCEPTIONS.append(traceback.format_exc())
        add_issue(ctx, action, '구성 없이 5단계로 이동(go(5))하면 예외', '상',
                  traceback.format_exc().strip().splitlines()[-1])
    check_exceptions(ctx, f'{action} 미구성 상태로 5단계 이동')
    snapshot_if_bad(win, out, 'jump_step5_unconfigured', before)

    # ⑦ 같은 출력 두 번 추가(스피커 추가 버튼 연타 후 같은 출력으로 몰아 보기)
    before = len(ISSUES)
    try:
        s1._add_speaker()
        s1._add_speaker()
        settle(app)
    except Exception:
        EXCEPTIONS.append(traceback.format_exc())
    check_exceptions(ctx, f'{action} 스피커 추가 연타')
    snapshot_if_bad(win, out, 'add_speaker_mash', before)


def test_language_roundtrip_all_steps(win, app, out: Path, ctx: dict) -> None:
    """언어를 바꾼 뒤 1~4단계를 전부 재방문 — 위젯 값이 안 사라지는지, 잘림이 없는지."""
    from secs.i18n import language_manager

    action = '언어 전환 후 재방문'
    before_state = (win.project.config.sub_count(), win.project.config.biamp_default,
                    len(win.project.config.speakers))
    manager = language_manager()
    for lang in ('en', 'ko'):
        before = len(ISSUES)
        try:
            manager.set_language(lang, persist=False)
            settle(app)
        except Exception:
            EXCEPTIONS.append(traceback.format_exc())
        check_exceptions(ctx, f'{action} → {lang}')
        for step in STEPS_IN_SCOPE:
            win.go(step)
            settle(app)
            area = win.stack.currentWidget()
            checks = layout_checks(win, area)
            report_layout({**ctx, 'lang': lang, 'step': step}, f'{action} {lang} {step}단계', checks)
        after_state = (win.project.config.sub_count(), win.project.config.biamp_default,
                       len(win.project.config.speakers))
        if after_state != before_state:
            add_issue(ctx, action, f'언어 전환만으로 구성 값이 바뀜 {before_state} → {after_state}',
                      '상', 'i18n language_manager / step refresh')
        snapshot_if_bad(win, out, f'lang_revisit_{lang}', before)
    manager.set_language('ko', persist=False)
    settle(app)


# ──────────────────────────── 실행 ────────────────────────────
def run_combo(preset: str, biamp: bool, langs: list[str], interact_sizes: list[tuple[int, int]],
              layout_sizes: list[tuple[int, int]], out: Path) -> None:
    from PyQt6 import QtCore
    from PyQt6.QtWidgets import QApplication
    from secs.i18n import language_manager

    with tempfile.TemporaryDirectory() as tmp:
        win = build_window(preset, biamp, Path(tmp))
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.show()
        app = QApplication.instance()
        win_ref[0] = win

        for lang in langs:
            language_manager().set_language(lang, persist=False)
            settle(app)
            for size in interact_sizes:
                win.resize(*size)
                settle(app)
                for step in STEPS_IN_SCOPE:
                    ctx = {'screen': f'{step}단계', 'size': f'{size[0]}x{size[1]}',
                           'lang': lang, 'step': step, 'preset': preset, 'biamp': biamp,
                           'repro': (f'qa_monkey_a.py --preset {preset}'
                                    + (' --biamp' if biamp else '')
                                    + f' --lang {lang} --size {size[0]}x{size[1]} --step {step}')}
                    visit_and_probe(win, app, out, ctx)
            # 나머지 크기는 조작 없이 정적 점검만(가로 스크롤·잘림·겹침) — 전부 다 누르면 너무 느리다.
            for size in layout_sizes:
                win.resize(*size)
                settle(app)
                for step in STEPS_IN_SCOPE:
                    win.go(step)
                    settle(app)
                    ctx = {'screen': f'{step}단계', 'size': f'{size[0]}x{size[1]}',
                           'lang': lang, 'step': step, 'preset': preset, 'biamp': biamp,
                           'repro': (f'qa_monkey_a.py --preset {preset}'
                                    + (' --biamp' if biamp else '')
                                    + f' --lang {lang} --size {size[0]}x{size[1]} --step {step} (정적 전용)')}
                    before = len(ISSUES)
                    checks = layout_checks(win, win.stack.currentWidget())
                    report_layout(ctx, '정적 점검(비조작 크기)', checks)
                    check_exceptions(ctx, '정적 점검(비조작 크기)')
                    snapshot_if_bad(win, out, f'static_{step}_{size[0]}x{size[1]}_{lang}', before)

        # 창 크기 왕복(줄였다 늘렸다) — 마지막 크기 기준으로 1024 → 2560 → 1024
        all_sizes = list(interact_sizes) + list(layout_sizes)
        ctx = {'screen': '전체(크기 왕복)', 'size': '왕복', 'lang': langs[-1], 'step': 1,
               'preset': preset, 'biamp': biamp,
               'repro': f'qa_monkey_a.py --preset {preset} --resize-cycle'}
        win.go(1)
        before = len(ISSUES)
        for size in all_sizes + list(reversed(all_sizes)) + [NARROW_TALL, all_sizes[0]]:
            win.resize(*size)
            settle(app)
        checks = layout_checks(win, win.stack.currentWidget())
        report_layout(ctx, '크기 왕복 뒤 1단계', checks)
        check_exceptions(ctx, '크기 왕복')
        snapshot_if_bad(win, out, 'resize_cycle', before)

        # 공통 기능 + 순서 밖 조작 + 언어 왕복(대표 크기 하나로)
        win.resize(1280, 800)
        settle(app)
        ctx = {'screen': '공통', 'size': '1280x800', 'lang': 'ko', 'step': 1,
               'preset': preset, 'biamp': biamp,
               'repro': f'qa_monkey_a.py --preset {preset} --common'}
        test_common(win, app, out, ctx)
        ctx['screen'] = '순서 밖 조작'
        test_sequence_violations(win, app, out, ctx)
        ctx['screen'] = '언어 전환'
        test_language_roundtrip_all_steps(win, app, out, ctx)

        # 닫기(정상 케이스: ask_before_close False — 기본값)
        before = len(ISSUES)
        try:
            win.close()
            settle(app)
        except Exception:
            EXCEPTIONS.append(traceback.format_exc())
        check_exceptions(ctx, '창 닫기(기본, 확인 없음)')
        # 닫기 확인(더러운 상태 + ask_before_close True) — 별도 창으로 다시 만든다.
        try:
            win2 = build_window(preset, biamp, Path(tmp))
            win2.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
            win2.ask_before_close = True
            win2._dirty = True
            win2.show()
            win2.close()   # QMessageBox.exec 는 patch_modals() 가 즉시 응답하게 해 둠
            settle(app)
        except Exception:
            EXCEPTIONS.append(traceback.format_exc())
        check_exceptions(ctx, '창 닫기(저장 확인 대화상자)')
        snapshot_if_bad(win, out, 'close', before)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--preset', default='2.1')
    ap.add_argument('--biamp', action='store_true')
    ap.add_argument('--all', action='store_true', help='2.0 · 2.1 · 9.1.6(바이앰프) 전부')
    ap.add_argument('--lang', default='ko,en')
    ap.add_argument('--sizes', default='')
    ap.add_argument('--out', default='')
    args = ap.parse_args(argv)

    install_hooks()
    from PyQt6 import QtCore
    from PyQt6.QtWidgets import QApplication
    patch_modals()

    os.environ['SECS4_SETTINGS_PATH'] = str(Path(tempfile.mkdtemp(prefix='secs-qamonkey-')) / 'settings.ini')
    app = QApplication.instance() or QApplication([])

    all_sizes = ([tuple(int(v) for v in s.split('x')) for s in args.sizes.split(',') if s.strip()]
                 or SIZES + [NARROW_TALL])
    # 실제로 버튼·콤보·체크·스핀을 다 눌러 보는 크기(느림) vs 정적 점검만 하는 나머지 크기(빠름).
    # 최소 폭(1024x640)과 기준 폭(1280x800)이 잘림·겹침이 가장 잘 드러나는 크기다.
    interact_sizes = [s for s in all_sizes if s in ((1024, 640), (1280, 800))] or all_sizes[:2]
    layout_sizes = [s for s in all_sizes if s not in interact_sizes]
    langs = [s.strip() for s in args.lang.split(',') if s.strip()]
    out = Path(args.out) if args.out else ROOT / '보고서' / '05 화면 QA' / f'{_dt.date.today():%Y-%m-%d} 실사용 QA'
    out.mkdir(parents=True, exist_ok=True)

    combos = [(args.preset, args.biamp)]
    if args.all:
        combos = [('2.0', False), ('2.1', False), ('9.1.6', True)]

    for preset, biamp in combos:
        run_combo(preset, biamp, langs, interact_sizes, layout_sizes, out)

    print(f'| 화면 | 크기 | 언어 | 조작 | 증상 | 재현 | 심각도 | 원인 추정 | 스크린샷 |')
    print('|---|---|---|---|---|---|---|---|---|')
    for issue in ISSUES:
        print(f'| {issue.screen} | {issue.size} | {issue.lang} | {issue.action} | '
              f'{issue.symptom} | `{issue.repro}` | {issue.severity} | {issue.guess[:80]} | '
              f'{issue.screenshot} |')
    print(f'\n총 {len(ISSUES)}건 (스크린샷 {out})')
    return 0


if __name__ == '__main__':
    sys.exit(main())
