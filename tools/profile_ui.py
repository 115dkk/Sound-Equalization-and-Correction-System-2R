# -*- coding: utf-8 -*-
"""위저드 반응 속도 측정 도구 — 2026-09-23 사용자 보고("프로그램이 전체적으로 반응속도가 느리다")
의 병목을 **측정만** 한다(고치지 않는다). FakeREW 로 돈다(소리 안 남).

    .venv-secs\\Scripts\\python.exe tools\\profile_ui.py [--positions 3]

측정 항목(요청 순서대로):
  * 앱 시작: 임포트 시간(`-X importtime` 상위) · WizardWindow() 생성 · 첫 표시.
  * 단계 이동: 흐름 순서(`strings.FLOW`)대로 `go(n)` + (있으면) 그 단계의 `wait()`(디바운스+백그라운드 계산 대기),
    되돌아가기.
  * 2단계(구성): 프리셋 클릭(표 재구성) · 서브 +/− · 바이앰프 체크 · 역할/출력 드롭다운 변경.
  * 창 크기 변경(1024↔1920) 레이아웃 시간(보정 설계 화면).
  (옛 6·7·8′ 단계 항목은 2026-10-10 슬림화로 그 화면과 함께 뺐다.)
  * UI 스레드 정지 감지: QTimer 10 ms 하트비트로 50 ms 이상 뜬 간격을 기록하고, 그 구간과
    겹치는 측정 항목을 붙인다(이 도구 자체가 대부분 동기 호출이라 "막힘 = 그 항목 자체"인
    경우가 많다 — 진짜로 알아낼 수 있는 것은 recompute()+wait() 처럼 백그라운드 스레드를
    쓰는 경로에서도 GIL 때문에 화면이 막히는지다).
  * 100 ms 를 넘는 항목은 cProfile 로 다시 재서 상위 함수(누적시간) 10개를 같이 남긴다
    (재실행 가능한 항목만 — 상태를 누적시키는 항목은 방향을 바꿔 한 번씩만 잰다. 아래 표기 참고).

결과: 표준 출력에 마크다운 표 여러 개. `--out FILE` 을 주면 같은 내용을 그대로 파일에도 남긴다.
"""
from __future__ import annotations

import argparse
import cProfile
import io
import os
import pstats
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
sys.path.insert(0, str(ROOT / 'tools'))
os.environ.pop('QT_QPA_PLATFORM', None)      # qa_capture 와 같은 이유(글꼴) — 오프스크린 금지, windows 로
os.environ['SECS4_SETTINGS_PATH'] = str(Path(tempfile.mkdtemp(prefix='secs-profui-')) / 'settings.ini')

SLOW_MS = 100.0      # 보고서에서 굵게 표시할 기준(사용자 요청)
PROFILE_MS = 60.0    # 이 이상이면 cProfile 로 다시 재서 상위 함수를 남긴다
BLOCK_MS = 50.0      # 하트비트 막힘 기준(사용자 요청)


# ──────────────────────────── 기록 ────────────────────────────
class Recorder:
    def __init__(self) -> None:
        self.ops: list[dict] = []           # {label, start, end, ms, note, profile, error}
        self.heartbeats: list[float] = []

    def add(self, label: str, start: float, end: float, note: str = '',
            profile: str | None = None, error: str | None = None) -> None:
        self.ops.append({'label': label, 'start': start, 'end': end,
                          'ms': (end - start) * 1000.0, 'note': note,
                          'profile': profile, 'error': error})

    def blocked_ranges(self, threshold_ms: float = BLOCK_MS) -> list[tuple[float, float, list[str]]]:
        """연속 하트비트 사이 간격이 threshold_ms 를 넘는 구간과, 그 구간과 겹치는 측정 항목."""
        out = []
        for prev, cur in zip(self.heartbeats, self.heartbeats[1:]):
            gap_ms = (cur - prev) * 1000.0
            if gap_ms < threshold_ms:
                continue
            labels = [op['label'] for op in self.ops
                      if not (op['end'] < prev - 0.0008 or op['start'] > cur + 0.0008)]
            out.append((prev, gap_ms, labels or ['(측정 항목 밖 — 하트비트 콜백 자체의 지연이거나 이 스크립트 밖의 다른 지연)']))
        return out


REC = Recorder()


def run(label: str, func, note: str = '', profile: bool = True):
    """한 번 실행해 시간을 잰다. `SLOW_MS` 를 넘고 `profile=True` 면 cProfile 로 다시 한번
    재서(같은 호출을 반복해도 안전한 항목만 이 옵션을 켠다) 상위 10개 함수를 같이 남긴다.
    실패해도 스크립트를 멈추지 않는다."""
    t0 = time.perf_counter()
    try:
        result = func()
    except Exception as exc:  # noqa: BLE001 - 진단 도구, 하나 실패해도 나머지는 잰다
        t1 = time.perf_counter()
        REC.add(label, t0, t1, note, error=f'{type(exc).__name__}: {exc}')
        return None
    t1 = time.perf_counter()
    profile_text = None
    if profile and (t1 - t0) * 1000.0 >= PROFILE_MS:
        pr = cProfile.Profile()
        try:
            pr.enable()
            func()
            pr.disable()
            buf = io.StringIO()
            pstats.Stats(pr, stream=buf).sort_stats('cumulative').print_stats(10)
            profile_text = buf.getvalue()
        except Exception as exc:  # noqa: BLE001
            profile_text = f'(재실행 프로파일 실패: {exc})'
    REC.add(label, t0, t1, note, profile=profile_text)
    return result


def run_profiled_once(label: str, func, note: str = ''):
    """반복 실행하면 상태가 누적되는 항목(예: 서브 개수 +/−) — 딱 한 번만 cProfile 로 잰다.
    시간에 cProfile 오버헤드가 섞인다(표에 표시)."""
    pr = cProfile.Profile()
    t0 = time.perf_counter()
    try:
        pr.enable()
        result = func()
        pr.disable()
    except Exception as exc:  # noqa: BLE001
        t1 = time.perf_counter()
        REC.add(label, t0, t1, note + ' [1회, 실패]', error=f'{type(exc).__name__}: {exc}')
        return None
    t1 = time.perf_counter()
    buf = io.StringIO()
    pstats.Stats(pr, stream=buf).sort_stats('cumulative').print_stats(10)
    REC.add(label, t0, t1, note + ' [1회뿐 · cProfile 오버헤드 포함]', profile=buf.getvalue())
    return result


def pump(app, rounds: int = 3) -> None:
    for _ in range(rounds):
        app.processEvents()


# ──────────────────────────── 앱 시작 ────────────────────────────
def bench_importtime() -> str:
    """`python -X importtime -c "import secs.wizard.window"` 상위 15개(자기 시간 기준).

    `secs.wizard` 패키지 자체는 일부러 가볍다(`__init__.py`: "무거운 Qt 임포트는 실제로
    쓸 때까지 미룬다" — `secs.wizard.WizardWindow` 는 `__getattr__` 지연 속성). 그래서
    `import secs.wizard` 만 재면 PyQt6·numpy·scipy·pyqtgraph 를 하나도 안 읽어 9 ms 로
    나온다(실제로 이렇게 재 봤다가 잘못된 값인 걸 알았다 — 09-23). 실제 시작 비용을 보려면
    `WizardWindow` 가 든 `secs.wizard.window` 를 직접 읽어야 한다."""
    python = sys.executable
    code = f"import sys; sys.path.insert(0, r'{ROOT / 'src'}'); import secs.wizard.window"
    proc = subprocess.run([python, '-X', 'importtime', '-c', code],
                          capture_output=True, text=True, timeout=60)
    lines = [l for l in proc.stderr.splitlines() if l.startswith('import time:')]
    rows = []
    total_line = ''
    pat = re.compile(r'import time:\s*(\d+)\s*\|\s*(\d+)\s*\|(?:\s*(\S.*))?$')
    for line in lines:
        m = pat.match(line)
        if not m:
            continue
        self_us, cum_us, name = int(m.group(1)), int(m.group(2)), (m.group(3) or '').strip()
        rows.append((self_us, cum_us, name))
        if name in ('secs.wizard.window', 'secs.wizard', 'PyQt6', 'numpy', 'scipy', 'pyqtgraph'):
            total_line += f'{name}: 누적 {cum_us / 1000:.1f} ms · '
    rows.sort(key=lambda r: r[0], reverse=True)
    out = io.StringIO()
    out.write('| 모듈 | 자기 시간 | 누적 시간 |\n|---|---|---|\n')
    for self_us, cum_us, name in rows[:15]:
        out.write(f'| {name} | {self_us / 1000:.2f} ms | {cum_us / 1000:.2f} ms |\n')
    if total_line:
        out.write(f'\n(참고: {total_line})\n')
    if proc.returncode != 0:
        out.write(f'\n(주의: 종료 코드 {proc.returncode}, stderr 마지막 500자: {proc.stderr[-500:]})\n')
    return out.getvalue()


def bench_startup(app) -> None:
    import secs.model as model
    import secs.rewapi as rewapi
    from secs.wizard import WizardWindow
    from PyQt6 import QtCore

    project = model.Project(config=model.SpeakerConfig.from_preset('2.1', biamp=False))
    rew = rewapi.FakeREW(delay_s=0.0)
    tmp = Path(tempfile.mkdtemp(prefix='secs-profui-startup-'))

    def make():
        win = WizardWindow(project, rew, rew, rew, rew, project_path=tmp / 'qa.secs')
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        return win

    win = run('앱 시작 · WizardWindow() 생성(2.1, 8단계 화면 전부 미리 만듦)', make)
    if win is None:
        return
    run('앱 시작 · 첫 표시(show()+이벤트 처리)', lambda: (win.show(), pump(app, 5)), profile=False)
    win.close()
    pump(app)


# ──────────────────────────── 단계 이동 ────────────────────────────
def bench_navigation(win, app, preset: str) -> None:
    from secs.wizard.strings import FLOW

    order_fwd = list(FLOW)
    for step in order_fwd:
        run(f'{preset} {step}단계 go({step}) (정방향)', lambda s=step: win.go(s))
        widget = win.step_widget()
        if widget is not None and hasattr(widget, 'wait'):
            run(f'{preset} {step}단계 진입 후 대기(wait, 디바운스+백그라운드 계산)',
                lambda w=widget: w.wait(120000), profile=False)
        else:
            pump(app)   # step1 처럼 wait() 가 없는 화면도 singleShot(0, …) 을 비워 둔다
    for step in reversed(order_fwd[:-1]):
        run(f'{preset} {step}단계 go({step}) (되돌아가기)', lambda s=step: win.go(s))
        pump(app)


# ──────────────────────────── 2단계(구성, 09-23 전에는 1단계) ────────────────────────────
def bench_step1(preset: str, biamp: bool, app) -> None:
    import qa_capture
    from PyQt6 import QtCore, QtWidgets

    def fake_combo(data):
        combo = QtWidgets.QComboBox()
        combo.addItem('x', data)
        combo.setCurrentIndex(0)
        return combo

    with tempfile.TemporaryDirectory() as tmp:
        win = qa_capture.build_window(preset, biamp, 0, Path(tmp))
        win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
        win.show()
        win.go(2)
        step1 = win.config_page

        # 각 상호작용 뒤 pump(app) 으로 QTimer.singleShot(0, _fit_columns)·deleteLater 된
        # 옛 셀 위젯을 그때그때 비운다 — 안 비우면 뒤 항목(특히 창 크기 변경)의 첫
        # processEvents() 에서 밀린 콜백이 한꺼번에 몰려 엉뚱한 항목이 느려 보인다(이 도구
        # 만드는 중 실제로 겪음 — 09-23).
        run(f'{preset} 2단계(구성) 프리셋 재클릭({preset}, 표 재구성)',
            lambda: step1._apply_preset(preset))
        pump(app)
        run(f'{preset} 2단계(구성) 서브 + (한 개 늘리기)',
            lambda: step1._set_sub_count(+1), profile=False)
        pump(app)
        run_profiled_once(f'{preset} 2단계(구성) 서브 − (한 개 줄이기, 원상 복구 겸)',
                          lambda: step1._set_sub_count(-1))
        pump(app)
        run(f'{preset} 2단계(구성) 바이앰프 체크(메인 스피커 전체 대역 재구성)',
            lambda: step1._on_biamp(True))
        pump(app)
        run(f'{preset} 2단계(구성) 역할 드롭다운 변경(1행)',
            lambda: step1._on_role(1, fake_combo('C')))
        pump(app)
        run(f'{preset} 2단계(구성) 출력 드롭다운 변경(1행 0번째 대역)',
            lambda: step1._on_out(1, 0, fake_combo(5)))
        pump(app)

        run(f'{preset} 2단계(구성)에서 창 크기 1024→1920',
            lambda: (win.resize(1920, 1080), pump(app)))
        run(f'{preset} 2단계(구성)에서 창 크기 1920→1024',
            lambda: (win.resize(1024, 640), pump(app)))

        win.close()
        pump(app)


# ──────────────────────────── 5단계(측정) ────────────────────────────
def bench_measure(win, app, preset: str, positions: int) -> None:
    win.go(5)
    for pos in range(1, positions + 1):
        run(f'{preset} 5단계 위치 {pos} 측정(Fake, 스윕 0 s)+wait',
            lambda p=pos: (win.step5.measure_position(p), win.step5.wait(60000)),
            profile=False)
    run(f'{preset} 5단계 곡선 다시 그리기(_draw)',
        lambda: (win.step5._draw(), pump(app)))


# ──────────────────────────── 하트비트 ────────────────────────────
def install_heartbeat(app):
    from PyQt6 import QtCore

    timer = QtCore.QTimer()
    timer.setTimerType(QtCore.Qt.TimerType.PreciseTimer)
    timer.timeout.connect(lambda: REC.heartbeats.append(time.perf_counter()))
    timer.start(10)
    return timer  # 참조를 들고 있어야 한다(안 그러면 GC 됨)


# ──────────────────────────── 출력 ────────────────────────────
def format_report(preset_labels: list[str]) -> str:
    out = io.StringIO()
    out.write('| 항목 | 시간 | 비고 |\n|---|---|---|\n')
    for op in REC.ops:
        if op['error']:
            out.write(f"| {op['label']} | 오류 | {op['error']} |\n")
            continue
        ms = op['ms']
        shown = f'{ms:.1f} ms'
        if ms >= SLOW_MS:
            shown = f'**{shown}**'
        note = op['note']
        out.write(f"| {op['label']} | {shown} | {note} |\n")

    out.write('\n## cProfile 상위 10개(누적시간) — 100 ms 이상 항목\n\n')
    any_profile = False
    for op in REC.ops:
        if op['profile']:
            any_profile = True
            out.write(f"### {op['label']} ({op['ms']:.1f} ms)\n\n```\n{op['profile']}```\n\n")
    if not any_profile:
        out.write('(100 ms 이상인 항목이 없었습니다.)\n\n')

    out.write('## UI 스레드 막힘(하트비트 10 ms, 간격 ≥ 50 ms)\n\n')
    out.write('| 시작 이후 경과(참고용) | 막힌 시간 | 겹치는 측정 항목 |\n|---|---|---|\n')
    blocked = REC.blocked_ranges()
    if not blocked:
        out.write('| - | - | 50 ms 이상 뜬 하트비트 간격 없음 |\n')
    else:
        t0 = REC.heartbeats[0] if REC.heartbeats else 0.0
        for prev, gap_ms, labels in blocked:
            shown_gap = f'{gap_ms:.0f} ms'
            if gap_ms >= SLOW_MS:
                shown_gap = f'**{shown_gap}**'
            out.write(f"| +{prev - t0:.1f}s | {shown_gap} | {'; '.join(labels)} |\n")
    return out.getvalue()


def _force_utf8_stdout() -> None:
    """cp949 콘솔에서 한국어·− ·—· 같은 문자가 UnicodeEncodeError 를 내지 않게 한다(`secs.__main__` 과 같은 이유)."""
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, 'reconfigure', None)
        if reconfigure is not None:
            try:
                reconfigure(encoding='utf-8', errors='replace')
            except (OSError, ValueError):
                pass


def main() -> int:
    _force_utf8_stdout()
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--positions', type=int, default=3)
    ap.add_argument('--out', default='')
    ap.add_argument('--skip-importtime', action='store_true', help='디버깅용 — 임포트 시간 재기를 건너뛴다')
    args = ap.parse_args()

    sections = []
    print('임포트 시간(자식 프로세스, `-X importtime`) 재는 중…', file=sys.stderr)
    importtime_report = '' if args.skip_importtime else bench_importtime()

    from PyQt6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication([])
    heartbeat = install_heartbeat(app)

    print('앱 시작(WizardWindow 생성 · 첫 표시) 재는 중…', file=sys.stderr)
    bench_startup(app)

    for preset, biamp in (('2.1', False), ('9.1.6', True)):
        print(f'{preset} (biamp={biamp}) 2단계(구성) 인터랙션 재는 중…', file=sys.stderr)
        bench_step1(preset, biamp, app)

        print(f'{preset} (biamp={biamp}) 단계 이동·측정 재는 중…', file=sys.stderr)
        import qa_capture
        from PyQt6 import QtCore
        with tempfile.TemporaryDirectory() as tmp:
            win = qa_capture.build_window(preset, biamp, args.positions, Path(tmp))
            win.setAttribute(QtCore.Qt.WidgetAttribute.WA_DontShowOnScreen, True)
            win.show()
            bench_measure(win, app, preset, args.positions)
            bench_navigation(win, app, preset)
            run(f'{preset} 보정 설계에서 창 크기 1024→1920', lambda: (win.go(9), win.resize(1920, 1080), pump(app)),
                profile=False)
            run(f'{preset} 보정 설계에서 창 크기 1920→1024', lambda: (win.resize(1024, 640), pump(app)),
                profile=False)
            win.close()
            pump(app)

    heartbeat.stop()
    report = '# 반응 속도 측정 원자료\n\n'
    report += '## 앱 시작 임포트(`-X importtime`, 자식 프로세스)\n\n' + (importtime_report or '(건너뜀)\n')
    report += '\n' + format_report([])

    print(report)
    if args.out:
        Path(args.out).write_text(report, encoding='utf-8')
        print(f'\n(파일로도 저장: {args.out})', file=sys.stderr)
    return 0


if __name__ == '__main__':
    sys.exit(main())
