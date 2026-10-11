# -*- coding: utf-8 -*-
"""내보내기 단계(화면 번호 8, 진행바 7번째) — 보정 설계(9)의 새 엔진 결과를 REW 로 열 .mdat 하나로 저장한다(2026-09-26).

2026-09-26 사용자 결정: 옛 8단계(DSP 시트 · Tesira · 검증 A/B)를 걷어내고 새로 만든다 — **우선 mdat 내보내기만**.
옛 화면(`step8_export`)은 흐름 밖에 남아 있다가 2026-10-10 슬림화로 지웠다(되살릴 기준: git 태그 slim-before-2026-10-10).

누가 부르나: `WizardWindow` 가 만들어 스택에 넣고(`export_page` = `step8`), 들어올 때 `set_context(프로젝트 경로)`·`refresh()`.
입력: 보정 설계 화면(`StepCalibration`)의 `run`(`calibration.CalibrationRun`)과, MIMO 를 켜고 계산했으면 `mimo_run`.
출력: `<내보낼 폴더>/SECS_export_<모드>_<날짜시간>/SECS 보정 전후 <모드>.mdat`
(`calibration.export_before_after_mdat` — 채널마다 보정 전 = 원 측정, 보정 후 = 선택한 모드의 최종 응답, MIMO 적용).
내보낼 폴더는 보정 설계와 같은 값(`project.extra['export']['folder']`, 없으면 프로젝트 파일 옆).
파일 쓰기는 워커 스레드(`run_off_thread`) — 화면 스레드는 기다리는 동안에도 돈다.
"""
from __future__ import annotations

import os
import time
from pathlib import Path

from PyQt6 import QtCore, QtGui
from secs.wizard import qt as QtWidgets

from secs import calibration as cal
from secs.model import role_code
from secs.wizard.loading import run_off_thread
from secs.wizard.strings import t
from secs.wizard.style import add_heading, card, label, path_label, plain_label, set_variant, two_columns
from secs.wizard.tooltips import attach


def _join(parts) -> str:
    """' · ' 로 이은 한 줄 — 조각마다 옮긴다(보정 설계 화면과 같은 규칙)."""
    return ' · '.join(str(t(part)) for part in parts if part)


class Step8Mdat(QtWidgets.QWidget):
    """7 · 내보내기 — 보정 전·후 REW mdat."""

    changed = QtCore.pyqtSignal()

    def __init__(self, project, calibration=None, parent=None) -> None:
        super().__init__(parent)
        self.project = project
        self.calibration = calibration            # StepCalibration(결과를 든 화면) — 없으면 내보낼 것이 없다
        self.project_path: Path | None = None
        self.last_files: list[str] = []
        self._exporting = False
        self._build()
        self.refresh()

    # ──────────────────────────── 뼈대 ────────────────────────────
    def _build(self) -> None:
        outer = QtWidgets.QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(14)
        add_heading(outer, t('7 · 내보내기'))
        _body, main, _side = two_columns(outer, side=False)
        main.addWidget(self._source_card())
        main.addWidget(self._mdat_card())
        outer.addStretch(1)

    def _source_card(self) -> QtWidgets.QFrame:
        frame, box = card('보정 결과')
        self.source_label = label('', 'spec', wrap=True)
        box.addWidget(self.source_label)
        # 채널 이름은 번역하지 않는다(돌비 표기 — CLAUDE.md '채널 이름은 plain_label').
        self.roles_label = plain_label('', 'spec')
        self.roles_label.setWordWrap(True)
        box.addWidget(self.roles_label)
        self.rc_label = label('', 'spec', wrap=True)
        box.addWidget(self.rc_label)
        return frame

    def _mdat_card(self) -> QtWidgets.QFrame:
        frame, box = card('REW mdat')
        path_row = QtWidgets.QHBoxLayout()
        path_row.setSpacing(8)
        path_row.addWidget(label('내보낼 폴더', 'note'))
        path_row.addStretch(1)
        self.folder_change = QtWidgets.QPushButton(t('변경…'))
        set_variant(self.folder_change, 'ghost', 'sm')
        self.folder_change.clicked.connect(lambda: self.choose_export_folder())
        attach(self.folder_change, 's8.folder_change')
        self.folder_open = QtWidgets.QPushButton(t('폴더 열기'))
        set_variant(self.folder_open, 'ghost', 'sm')
        self.folder_open.clicked.connect(self.open_export_folder)
        attach(self.folder_open, 's8.folder_open')
        path_row.addWidget(self.folder_change)
        path_row.addWidget(self.folder_open)
        box.addLayout(path_row)
        self.export_path_label = path_label('')
        # 경로 글자 폭이 최소 폭이 되지 않게(보정 설계 화면 09-26 QA 와 같은 이유) — 넘치면 가운데를 … 로.
        self.export_path_label.setSizePolicy(QtWidgets.QSizePolicy.Policy.Ignored, QtWidgets.QSizePolicy.Policy.Preferred)
        box.addWidget(self.export_path_label)
        row = QtWidgets.QHBoxLayout()
        row.setSpacing(8)
        self.export_button = QtWidgets.QPushButton(t('REW mdat 저장'))
        set_variant(self.export_button, 'accent', 'sm')
        self.export_button.clicked.connect(lambda: self.export(open_after=True))
        attach(self.export_button, 's8.export_before_after')
        row.addWidget(self.export_button)
        # 카밀라 내보내기도 여기에(10-09 사용자: 내보내기 탭에 카밀라 버튼이 없다) — 보정 설계 화면의 것과 같은 일.
        self.camilla_button = QtWidgets.QPushButton(t('CamillaDSP 내보내기'))
        set_variant(self.camilla_button, 'ghost', 'sm')
        self.camilla_button.clicked.connect(lambda: self.export(kind='camilla', open_after=True))
        attach(self.camilla_button, 'cal.export_camilla')
        row.addWidget(self.camilla_button)
        row.addStretch(1)
        box.addLayout(row)
        self.export_note = label('', 'note', wrap=True)
        box.addWidget(self.export_note)
        return frame

    # ──────────────────────────── 문맥 ────────────────────────────
    def set_context(self, project_path=None) -> None:
        """창이 넘긴다: 프로젝트 파일 위치(내보낼 폴더 기본값)."""
        self.project_path = Path(project_path) if project_path else None
        self.refresh()

    def run(self) -> cal.CalibrationRun | None:
        return getattr(self.calibration, 'run', None)

    def mimo_run(self) -> cal.MimoRun | None:
        """MIMO 는 보정 설계에서 켜고 지금 보정 위에서 계산했을 때만(그래프에 보이는 것과 같다)."""
        page = self.calibration
        return page.mimo_run if page is not None and page.show_mimo() else None

    def ready(self) -> tuple[bool, str]:
        """(내보낼 수 있나, 아니면 이유)."""
        page = self.calibration
        if page is not None and page.working():
            return False, '보정 설계 계산 중…'
        if self.run() is None:
            return False, '보정 결과가 없습니다 — 보정 설계에서 [계산]을 누르세요'
        return True, ''

    # ──────────────────────────── 그리기 ────────────────────────────
    def refresh(self) -> None:
        """보정 설계 결과로 다시 채운다(단계 진입·언어 전환·불러오기)."""
        self.export_path_label.set_full_text(str(self.export_base_dir()))
        ok, reason = self.ready()
        run = self.run()
        self.export_button.setEnabled(ok and not self._exporting)
        self.camilla_button.setEnabled(ok and not self._exporting)
        if run is None:
            self.source_label.setText(t(reason))
            self.roles_label.setText('')
            self.roles_label.setVisible(False)
            self.rc_label.setText('')
            self.rc_label.setVisible(False)
            return
        engine = run.engine
        parts = [cal.mode_label(run.settings.mode), f'전체 지연 {run.latency_ms:.1f} ms']
        if not ok:
            parts.append(reason)
        elif self.calibration is not None and self.calibration.stale():
            parts.append('설정·측정이 바뀌었습니다 — 마지막 계산 결과를 내보냅니다')
        mimo = self.mimo_run()
        channels = list(engine.chain)
        subs = [r for r in engine.subs if r in engine.irs.get(engine.main_position, {})]
        counts = [f'채널 {len(channels)}개 보정 전·후', f'서브 {len(subs)}개는 측정 그대로' if subs else '',
                  f'MIMO 적용 {sum(1 for r in channels if mimo.iir.get(r))}개' if mimo is not None else '']
        self.source_label.setText(_join(parts + counts))
        self.roles_label.setText(' · '.join(role_code(r) for r in channels + subs))
        self.roles_label.setVisible(True)
        self.rc_label.setText(_join(cal.rc_pieces(run)))
        self.rc_label.setVisible(True)

    # ──────────────────────────── 내보내기 ────────────────────────────
    def export_base_dir(self) -> Path:
        """보정 설계·옛 8단계와 같은 내보낼 폴더(프로젝트에 고른 값 → 프로젝트 파일 옆 → 문서 폴더)."""
        stored = (self.project.extra.get('export') or {}).get('folder')
        if stored:
            return Path(stored)
        from secs.wizard.step_calibration import default_export_dir
        return default_export_dir(self.project_path)

    def choose_export_folder(self, folder=None) -> Path | None:
        """[변경…]. 테스트(pytest)는 대화상자를 띄우지 않는다(모달에 멈춘다) — `folder` 를 준다."""
        if folder is None:
            if os.environ.get('PYTEST_CURRENT_TEST'):
                return None
            folder = QtWidgets.QFileDialog.getExistingDirectory(self, t('내보낼 폴더 선택'), str(self.export_base_dir()))
            if not folder:
                return None
        self.project.extra['export'] = {**(self.project.extra.get('export') or {}), 'folder': str(folder)}
        self.export_path_label.set_full_text(str(folder))
        self.changed.emit()
        return Path(folder)

    def open_export_folder(self) -> None:
        folder = self.export_base_dir()
        folder.mkdir(parents=True, exist_ok=True)
        if not os.environ.get('PYTEST_CURRENT_TEST'):
            QtGui.QDesktopServices.openUrl(QtCore.QUrl.fromLocalFile(str(folder)))

    def exporting(self) -> bool:
        return self._exporting

    def export(self, folder=None, open_after: bool = False, kind: str = 'mdat') -> list[str]:
        """`<folder 또는 내보낼 폴더>/SECS_export_<모드>_<날짜시간>/` 에 보정 전·후 mdat 을 쓰고 쓴 파일을 돌려준다.
        버튼으로 내보내면 그 폴더를 연다(pytest 에서는 안 연다)."""
        if getattr(self.calibration, 'trial', False):    # 튜토리얼는 파일을 쓰지 않는다(10-09 — 보정 설계와 같게)
            self.export_note.setText(t('튜토리얼에서는 내보내지 않습니다'))
            return []
        ok, reason = self.ready()
        if not ok:
            self.export_note.setText(t(reason))
            return []
        run, mimo = self.run(), self.mimo_run()
        base = Path(folder) if folder else self.export_base_dir()
        # 폴더 이름에 무엇이 들었는지(10-09 사용자) — ASCII 만.
        what = 'RC_BA_MIMO' if mimo is not None else 'RC_BA'
        target = base / f'SECS_{kind}_{what}_{run.settings.mode}_{time.strftime("%Y%m%d-%H%M%S")}'
        self._exporting = True
        self.export_button.setEnabled(False)
        self.camilla_button.setEnabled(False)
        offset = cal.project_spl_offset(self.project)

        def work() -> list[str]:
            if kind == 'camilla':
                return [str(p) for p in cal.export_camilladsp(run, target / 'camilladsp', mimo)]
            return [str(cal.export_before_after_mdat(run, target, mimo, offset))]
        try:
            files = run_off_thread(work, name='secs-export-mdat')
        except Exception as error:  # noqa: BLE001 - 파일 오류는 문구로
            self.export_note.setText(t(f'내보내기 실패: {error}'))
            return []
        finally:
            self._exporting = False
            self.export_button.setEnabled(self.ready()[0])
            self.camilla_button.setEnabled(self.ready()[0])
        self.last_files = files
        self.export_note.setText(t(f'{len(files)}개 파일을 저장했습니다 → {target}'))
        if open_after and files and not os.environ.get('PYTEST_CURRENT_TEST'):
            QtGui.QDesktopServices.openUrl(QtCore.QUrl.fromLocalFile(str(target)))
        return files

    # ──────────────────────────── 창 공통 ────────────────────────────
    def working(self) -> bool:
        return self._exporting

    def cancel(self) -> None:
        """끊을 일이 없다(파일 쓰기는 짧고 `export()` 가 끝날 때까지 기다린다)."""

    def wait(self, timeout_ms: int = 20000) -> bool:  # noqa: ARG002 - 창 공통 모양
        return not self._exporting

    def can_advance(self) -> tuple[bool, str]:
        return True, ''          # 마지막 단계


__all__ = ['Step8Mdat']
