# -*- coding: utf-8 -*-
"""`secs.wizard` — 측정 위저드 패키지.

`run_wizard(argv)` 가 진입점이다. 명령줄 인자를 받아 REW 백엔드(실제/가짜/오프라인)를
고르고 `Project` 를 열어 `WizardWindow` 를 띄운 뒤, Qt 앱의 종료 코드를 돌려준다.
`__main__` 이 부른다. 창을 먼저 띄우고 REW 연결은 뜬 뒤 백그라운드에서 본다.

- `window`   : `WizardWindow` — 사이드바 · 7칸 진행바 · 본문 · 하단 내비
- `step1_devices` … `step5_measure` · `step_calibration`(보정 설계) · `step8_mdat`(내보내기) : 단계별 화면
- `plot`     : 그래프 공용(주파수 눈금 · 곡선 색)
- `layout_map`: 목업 `roomSVG`/`arrSVG` 배치도(QGraphicsView) — 2 · 4 · 5단계
- `style`    : 목업 CSS 변수를 옮긴 QSS · 색 토큰 · 공용 부품
- `qt`       : `QtWidgets` 대역 — 글자를 가진 위젯을 번역 위젯으로 바꿔 준다
- `strings` · `en_g1`·`en_g2`·`en_g4`·`en_g8`(+ 풍선 `en_g5`, 튜토리얼 `en_g6`·`en_g9`) : 화면 문구(한국어 원문)와 영어 번역 쌍

진입: `run_secs.py --wizard [프로젝트.secs] [--fake-rew]`.
무거운 Qt 임포트는 실제로 쓸 때까지 미룬다.
"""
from __future__ import annotations

from typing import Any

__all__ = ['WizardWindow', 'build_backend', 'run_wizard', 'OfflineBackend', 'new_project',
           'open_project', 'single_amp', 'BIAMP_UI',
           'DEFAULT_PRESET', 'SAMPLE_PRESET', 'SAMPLE_SUBS', 'SAMPLE_ARRAY']  # noqa: E305

#: 사용자가 화면에서 바이앰프(스피커 하나를 LF · HF 등 대역별 출력으로 따로 재기)를 고를 수 있는지.
#: 2026-10-11 사용자: "바이앰프 기능도 잠시 끄자"(공개 Beta 1.0.0 — 바이앰프 대역 측정은 아직 보정 설계가 한 IR 로
#: 합치지 못해 뺀다, `calibration.engine_inputs`). False 면 2단계에 '바이앰프로 시작' 칸 · 앰프 구성 · 대역 열이 없고,
#: 바이앰프 `.secs` 는 열 때 단일 앰프로 바꾼다(`open_project` → `single_amp`, 파일은 사용자가 저장할 때까지 그대로).
#: 영어 화면을 닫은 방식(`i18n.ENGLISH_UI`)과 같다 — 모델 · 엔진의 바이앰프 지원(`SpeakerConfig.from_preset(biamp=True)`,
#: `역할:대역` 측정)과 화면 코드는 그대로이고 고르는 길만 없다. 여기(위저드 패키지)에 둔 까닭: 화면만의 결정이라 모델 ·
#: 엔진은 몰라도 되고, Qt 없이 읽혀 도구도 볼 수 있다. 읽는 쪽은 실행 때 `secs.wizard.BIAMP_UI` 로 읽는다(시험이
#: monkeypatch 로 켠다). 다시 열려면 True — 그때 튜토리얼 2단계 '확인할 것' 의 바이앰프 안내 줄(10-11 에 뺌)과
#: 그림 s2_overview · s2_preset · s2_patch 도 되살린다.
BIAMP_UI = False

#: 인자 없이 켰을 때의 구성. 사용자가 프리셋을 고르는 출발점이라 가장 작은 것.
DEFAULT_PRESET = '2.0'

#: `--fake-rew`(샘플 데이터) 로 새로 열 때 바탕이 되는 프리셋 — 7.1.4(바이앰프 끔),
#: `sample_dolby_714.npz` 실측과 맞춘다(2026-09-24 사용자 지시).
SAMPLE_PRESET = '7.1.4'

#: 샘플 구성의 서브 수 — 데모 방의 서브 원측정 3개(SW1 = DBA L · SW2 = DBA R · SW3 = INFRA 인프라 서브). 2026-10-11 사용자
#: 결정: 서브 1개(DBA+INFRA 합)로는 보정 설계의 BA 탭(서브 2개 이상, R9) · 인프라 서브 퀘스트가 튜토리얼에 안 나왔다.
SAMPLE_SUBS = 3


#: 샘플 모드로 새로 열 때의 배치 = 청취 위치 1점 — 샘플 실측이 1점뿐이다(2026-09-24 사용자 결정).
SAMPLE_ARRAY = 1


def new_project(fake: bool = False):
    """프로젝트 없이 켰을 때의 새 프로젝트. 샘플 모드(`fake`)는 7.1.4 + 서브 3 · 배치 1점.

    샘플 구성은 2단계에서 [7.1.4] 를 누르고 서브 + 를 두 번 누른 뒤 서브를 SW1 옆에 모아 출력 번호를 표 순서대로 다시 매긴
    것이다 — L R C SW1 SW2 SW3 Ls Rs Lb Rb 탑 4 = 출력 1~14(10-11 사용자: SW2 · SW3 이 13 · 14 로 끝에 붙어 "순서가
    이상"). 구성 이름은 7.1.4 로 둔다 — `set_sub_count` 가 이름을 'custom' 으로 바꾸면 2단계에서 [빈 구성] 버튼이 켜지고
    사이드바가 '사용자 정의' 라 데모 방(7.1.4 · 서브 3)이 무엇인지 안 보였다. 사이드바는 '7.1.4 · 서브 3 · 출력 14'.
    """
    from secs.model import Project

    project = Project.from_preset(SAMPLE_PRESET if fake else DEFAULT_PRESET)
    from secs.model import DspProfile
    project.dsp = DspProfile.presets()['camilla']     # 새 프로젝트 기본 DSP = 카밀라(10-09 사용자). 엔진 기본값은 그대로
    if fake:
        project.config.set_sub_count(SAMPLE_SUBS)
        speakers = project.config.speakers
        subs = [s for s in speakers if s.role.startswith('SW')]
        rest = [s for s in speakers if not s.role.startswith('SW')]
        at = next((i for i, s in enumerate(rest) if s.role in ('Ls', 'Lb', 'Tfl')), len(rest))   # 메인 3개 뒤
        project.config.speakers = rest[:at] + subs + rest[at:]
        project.config.renumber()
        project.config.preset = SAMPLE_PRESET
        project.extra['array'] = SAMPLE_ARRAY
    apply_auto_defaults(project)
    return project


def apply_auto_defaults(project) -> None:
    """새 프로젝트 기본값(개정 9, 09-25 사용자: 보정 목표를 높게): 보정 범위 20 Hz–20 kHz(전 대역), DSP 에
    FIR 이 있으면 혼합위상(IIR + FIR — 최소위상과 초과위상을 함께 고쳐 GD를 편다). 새 프로젝트에만 — 저장된 프로젝트의
    값은 건드리지 않는다."""
    project.curtain = (20.0, 20000.0)
    if int(getattr(project.dsp, 'fir_taps_max', 0) or 0) > 0:
        project.bc['phase'] = 'mixed'


def single_amp(project) -> bool:
    """구성을 단일 앰프로 바꾼다 — 바이앰프를 닫은 화면(`BIAMP_UI` False)이 바이앰프 `.secs` 를 받을 때. 바꿨으면 True.

    2단계에서 '바이앰프로 시작' 을 끈 것과 같은 구성(대역 하나 · `biamp_default` 끔)이 되지만 출력 번호는 다시 매기지
    않는다: 대역이 여럿인 스피커는 LF(없으면 첫) 대역의 출력 · 극성을 그대로 쓰는 한 줄이 되고, 다른 스피커의 출력은
    그대로다. 다시 매기면(`renumber`) 뒤 스피커가 앞 스피커의 HF 출력 자리로 밀려 전대역 스윕이 트위터 앰프로 갈 수 있다.
    측정 기록은 지우지 않는다 — `역할:대역` IR 은 남고 보정 설계가 이미 빼며 노트로 알린다(`calibration.engine_inputs`).
    위치의 '완료' 표시만 새 구성의 역할을 다 쟀는지로 다시 매긴다(그대로 두면 5단계가 안 잰 L · R 까지 완료로 보였다).
    4단계 음향 타이밍 기준이 대역(`R:HF`)이었으면 그 스피커(`R`)로 옮긴다(`timing.timing_settings` 는 없어진 역할을 R 로 돌린다).
    """
    from secs.model import Band, role_kind

    config = project.config
    changed = bool(config.biamp_default)
    config.biamp_default = False
    split = False
    for speaker in config.speakers:
        if len(speaker.bands) <= 1:
            continue
        keep = next((band for band in speaker.bands if band.band == 'LF'), speaker.bands[0])
        speaker.bands = [Band('SUB' if role_kind(speaker.role) == 'sub' else 'FULL', int(keep.out), str(keep.pol))]
        split = True
    if not split:
        return changed
    roles = [speaker.role for speaker in config.speakers]
    for record in project.measurements or []:
        if isinstance(record, dict):
            irs = record.get('ir') if isinstance(record.get('ir'), dict) else {}
            record['done'] = bool(roles) and all(irs.get(role) for role in roles)
    devices = project.extra.get('devices')
    if isinstance(devices, dict):
        reference = str(devices.get('timing_ref_role') or '')
        if ':' in reference and reference.split(':')[0] in roles:
            project.extra['devices'] = {**devices, 'timing_ref_role': reference.split(':')[0]}
    return True


def open_project(path):
    """`.secs` 를 열어 위저드가 받을 모양으로 — `run_wizard` · 창의 [불러오기] · 튜토리얼 끝내기가 부른다.

    바이앰프를 닫아 두었으면(`BIAMP_UI` False) `single_amp`. 메모리의 프로젝트만 바뀌고 파일은 사용자가 저장할 때까지
    그대로다(여는 것만으로는 '저장 안 한 변경' 으로 치지 않는다 — 다시 열면 같은 변환이 된다).
    """
    from secs.model import Project

    project = Project.load(path)
    if not BIAMP_UI:
        single_amp(project)
    return project


class OfflineBackend:
    """REW 를 붙일 수 없을 때 쓰는 빈 백엔드.

    1단계(장치)가 '미연결' 을 보여주고 재시도 버튼을 누를 수 있게만 한다.
    """

    def __init__(self, reason: str = '') -> None:
        self.reason = reason

    # RewStatus
    def is_running(self) -> bool:
        return False

    def version(self) -> str | None:
        return None

    def is_pro(self) -> bool:
        return False

    # DeviceLister
    def output_devices(self) -> list:
        return []

    def input_devices(self) -> list:
        return []

    # LevelMonitor
    def start(self, on_level) -> None:
        raise RuntimeError(self.reason or 'REW 에 연결되어 있지 않습니다')

    def stop(self) -> None:
        return None

    def play_pink(self, role: str, output: int, level_db: float) -> None:
        raise RuntimeError(self.reason or 'REW 에 연결되어 있지 않습니다')

    def stop_generator(self) -> None:
        return None

    # Measurer
    def run_position(self, plan, pos, on_progress=None, cancel=None) -> list:
        raise RuntimeError(self.reason or 'REW 에 연결되어 있지 않습니다')


def build_backend(fake: bool = False, delay_s: float = 0.05,
                  use_sample_dolby: bool = False) -> tuple:
    """(rew, devices, levels, measurer) 를 만든다.

    `fake=True` 면 `secs.rewapi.fake.FakeREW`(5개 프로토콜을 모두 구현),
    아니면 `RealREW`. 실제 REW 를 붙일 수 없으면 `OfflineBackend`.
    `use_sample_dolby=True`(샘플 모드 전용)면 FakeREW 가 7.1.4 + 서브 3 원측정 role 에 실측
    IR(`sample_dolby_714.npz`)을 돌려준다 — 나머지 role 은 그대로 합성.
    """
    if fake:
        from secs.rewapi.fake import FakeREW
        backend: Any = FakeREW(delay_s=delay_s, use_sample_dolby=use_sample_dolby)
    else:
        try:
            from secs.rewapi import RealREW
            backend = RealREW()
        except Exception as error:  # noqa: BLE001 - 연결 불가는 화면에서 안내한다
            backend = OfflineBackend(str(error))
    return backend, backend, backend, backend


def _set_windows_app_id() -> None:
    """Windows 에 "이것은 독립된 앱"이라고 알린다 — 작업 표시줄이 우리 아이콘으로 묶고,
    파이썬으로 띄워도 python.exe 아이콘 밑에 들어가지 않는다. 창을 만들기 전에 불러야 한다.
    다른 OS 나 실패는 조용히 넘어간다(모양만의 문제다)."""
    import sys

    if sys.platform != 'win32' or getattr(sys, 'frozen', False):
        # 빌드된 exe 는 이미 독립된 앱이다. 명시적 AppUserModelID 는 파이썬으로 띄울 때만 필요하고,
        # exe 에서까지 주면 Windows 11 작업 표시줄이 그 ID 로 등록된 바로 가기(없음)의 아이콘을
        # 찾다가 빈 아이콘을 보이는 경우가 있다(09-22 사용자 보고: 작업 표시줄에 로고가 안 뜸).
        return
    try:
        import ctypes

        from secs.version import APP_NAME, __version__
        app_id = f'{APP_NAME}.Wizard.{__version__}'.replace(' ', '')   # AppUserModelID 는 공백 금지
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(app_id)
    except Exception:  # noqa: BLE001
        pass


def _pin_taskbar_icon(window) -> None:
    """빌드된 exe 에서 창의 큰/작은 아이콘을 exe 자체의 아이콘 자원으로 한 번 더 지정한다.

    Qt 가 QIcon 으로 만든 HICON 을 이미 붙이지만, 작업 표시줄이 그것을 못 읽는 경우가 있어
    (09-22 사용자 보고) Win32 `WM_SETICON` 으로 exe 의 아이콘을 직접 준다. 실패는 조용히 넘어간다.
    """
    import sys

    if sys.platform != 'win32' or not getattr(sys, 'frozen', False):
        return
    try:
        import ctypes
        import ctypes.wintypes as wt

        big, small = wt.HICON(), wt.HICON()
        count = ctypes.windll.shell32.ExtractIconExW(sys.executable, 0, ctypes.byref(big), ctypes.byref(small), 1)
        if count <= 0:
            return
        hwnd = int(window.winId())
        WM_SETICON, ICON_SMALL, ICON_BIG = 0x0080, 0, 1
        if big.value:
            ctypes.windll.user32.SendMessageW(hwnd, WM_SETICON, ICON_BIG, big.value)
        if small.value:
            ctypes.windll.user32.SendMessageW(hwnd, WM_SETICON, ICON_SMALL, small.value)
    except Exception:  # noqa: BLE001
        pass


def run_wizard(argv: list[str] | None = None, splash=None) -> int:
    """위저드 진입점. 프로젝트 경로와 `--fake-rew` 를 해석해 창을 띄운다.

    `splash`(`secs.wizard.splash.StartupSplash`)가 있으면 진행 문구를 바꾸다가 창이 뜨면 닫는다.
    """
    import sys

    from PyQt6 import QtWidgets

    args = list(sys.argv[1:] if argv is None else argv)
    fake = '--fake-rew' in args
    positional = [a for a in args
                  if not a.startswith('-') and a.lower().endswith('.secs')]
    path = positional[0] if positional else None

    # 새로 열면 항상 최초 설정 — 09-22 사용자 지시(2.0 · 서브 0 · 바이앰프 끔). 다만
    # 샘플 데이터 모드(--fake-rew, 프로젝트 없이)는 실측(sample_dolby_714.npz)과
    # 맞춘 7.1.4 + 서브 3 · 바이앰프 끔으로 연다(09-24 사용자 지시, 서브 3 은 10-11). `.secs` 를 주면
    # 그 구성을 그대로 따른다(바이앰프는 10-11 부터 닫아 단일 앰프로 연다 — `open_project`).
    # 샘플 모드의 배치는 1점(`new_project`) — 샘플 실측이 청취 위치 하나뿐이다(09-24 사용자 결정).
    project = open_project(path) if path else new_project(fake)
    rew, devices, levels, measurer = build_backend(fake=fake, use_sample_dolby=fake)

    _set_windows_app_id()
    app = QtWidgets.QApplication.instance() or QtWidgets.QApplication(sys.argv[:1])
    from secs.wizard import wheel_guard
    wheel_guard.install(app)             # 드롭다운 위 휠로 선택이 바뀌지 않게(09-23)
    if splash is not None:
        splash.message('화면을 만드는 중…')
    from secs.wizard.branding import app_icon
    from secs.wizard.window import WizardWindow
    # 창을 먼저 띄운다 — 만드는 동안에는 REW 를 기다리지 않고, 뜬 뒤 백그라운드에서 연결을 본다.
    if hasattr(rew, 'hold_offline'):
        rew.hold_offline(60.0)
    app.setWindowIcon(app_icon())        # 대화상자·메시지 창도 같은 아이콘을 쓴다
    window = WizardWindow(project, rew=rew, devices=devices, levels=levels,
                          measurer=measurer, project_path=path)
    window.ask_before_close = not fake   # 실제 실행에서만 닫을 때 저장을 묻는다(09-22) — 체험은 저장 안 함
    if fake:
        window.set_trial(True)             # --fake-rew = 체험 모드(노란 띠, 10-09)
    window.show()
    _pin_taskbar_icon(window)
    if splash is not None:
        splash.finish(window)
    window.start_rew_watch()
    return int(app.exec())


def __getattr__(name: str) -> Any:
    if name == 'WizardWindow':
        from secs.wizard.window import WizardWindow
        return WizardWindow
    raise AttributeError(f'module {__name__!r} has no attribute {name!r}')
