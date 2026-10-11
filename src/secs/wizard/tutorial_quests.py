# -*- coding: utf-8 -*-
"""체험 모드 튜토리얼 퀘스트(2026-10-09 사용자: 게임처럼, 체험 모드 + 도움말을 합친 느낌).

행동 규칙은 `UI 목업/2026-10-09 게임식 튜토리얼/게임식 튜토리얼 목업.html` 이고, 화면은 실제 SECS 위젯이다.
단계마다 먼저 둘러보기(look — 화면 영역 소개, 문구는 F1 도움말 그림 설명 `tutorial_text.CAPTIONS` 와 같은 말)를 하고,
그다음 눌러 보기(act — 실제 위젯을 눌러야 넘어간다)를 한다. 표시 · 넘김은 `tutorial_overlay.Tutorial` 이 맡는다.

퀘스트 칸:
- screen: 창 화면 번호(`window.go()` 번호 — 1~5, 보정 설계 `CAL_STEP`, 내보내기 8).
- kind: 'look' | 'act'.
- target(win) → 강조할 QWidget(여러 개면 목록 — 합친 칸을 강조, 첫 위젯이 기준).
- text / why: 안내 · [왜?] 문구(한국어 원문, `t()` 로 옮긴다 — 영어는 `en_g9`). why 는 "왜 하는가" 에 답한다(10-10 AI 시험:
  XO 의 [왜?] 가 방법만 말해 질문과 답이 어긋났다).
- signal(win) → 이 신호가 나오면 끝(act). done(win) → True 면 끝(act, 주기적으로 본다). 둘 다 있으면 어느 쪽이든.
- base(win): 퀘스트를 보여 줄 때 찍어 두는 값 — 있으면 done 은 done(win, base) 로 불린다(값이 실제로 바뀌었는지 판정,
  10-10 AI 시험: 출력 번호 퀘스트가 크기만 바꿔도 끝났다).
- auto(win): [다음으로 →] 를 두 번 눌렀을 때 결과를 대신 채우는 동작(act). note: 그때 알림으로 보일 '무엇을 했나'
  (10-10 AI 시험 2차: 건너뛰기가 출력 번호를 말없이 바꿨다).
- busy(win) · busy_text: True 인 동안은 끝나지 않고 카드 문구를 busy_text 로 바꾼다(레벨 순차 재생 · 계산 — 2차 시험:
  누르자마자 '완료' 가 떠 [다음] 이 안 먹었다).
- keep(win) → 위젯 목록: 안내 카드가 덮지 말아야 할 결과 자리(레벨 미터 · 계산 상태 줄).
- ok: 직접 해냈을 때 알림(없으면 '확인 ✓' — 3차 시험 결정 1A 로 '퀘스트 완료' 에서 바꿈) — 2차 시험: [다시 확인] 뒤 무엇이 됐는지 몰랐다.
- auto 가 False 를 돌려주면 대신 해 줄 수 없게 막힌 것이다 — 넘어가지 않고 카드가 이유를 보인다(10-10 코드 리뷰).
- blocked(win) → 지금 이 퀘스트를 할 수 없는 이유(''면 없음) — 카드가 그 이유를 보인다(화면 [다음 →] 이 막혔을 때).
- 보정 설계 탭 하나를 둘러보는 퀘스트는 대상을 `_on_tab(win, 탭, 위젯들)` 로 준다 — 그 탭을 펴 두고 가리킨다(10-11).
- animate(win, k) · animate_end(win): 카드를 보는 동안 1.8 s 마다 k = 0, 1, 2 … 로 불러 화면을 움직여 보여 주고, 떠날 때 되돌린다
  (10-10 사용자: '오른쪽 카드는 탭에 맞춰 바뀝니다' 를 탭이 저절로 넘어가며 한 번에 이해되게). False 를 돌려주면 거기서 멈춘다
  (3차 시험: 끝없이 돌아 읽기 · 누르기 어려웠다 — 한 바퀴만). 도는 동안 안내 카드 자리는 고정이다(`Tutorial._place`).
- apart: 대상 위젯이 여럿일 때 합친 칸 대신 위젯마다 따로 테두리친다(떨어진 탭 줄과 오른쪽 카드 — 3차 시험).
- when(win): False 면 이 퀘스트를 건너뛴다(예: 서브가 하나면 서브 정렬 · 인프라 서브 퀘스트 없음).

`GLOSSARY` 는 약어 · 용어 풀이 — 그 말이 처음 나오는 카드에 한 줄씩 붙는다(10-10 AI 시험: 비전문가가 XO · FIR · REW ·
스윕 · 극성 … 을 몰랐다). 본문에 나온 말은 본문 아래, [왜?] 에만 나온 말은 [왜?] 를 펼쳤을 때 그 아래(2차 시험: [단일] 카드에
본문과 상관없는 MIMO 풀이가 보였다).
"""
from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Callable

from secs.wizard.strings import CAL_STEP

EXPORT_STEP = 8


@dataclass
class Quest:
    screen: int
    kind: str
    target: Callable
    text: str
    why: str = ''
    signal: Callable | None = None
    done: Callable | None = None
    auto: Callable | None = None
    when: Callable | None = None
    base: Callable | None = None
    note: str = ''
    busy: Callable | None = None
    busy_text: str = ''
    keep: Callable | None = None
    ok: str = ''
    blocked: Callable | None = None
    animate: Callable | None = None
    animate_end: Callable | None = None
    apart: bool = False


def card_of(widget):
    """위젯을 감싼 카드(objectName 'card') — 없으면 위젯 그대로."""
    w = widget
    while w is not None and w.objectName() != 'card':
        w = w.parentWidget()
    return w if w is not None else widget


def _next_to(screen: int) -> Quest:
    """[다음 →] 으로 다음 화면에 가는 퀘스트. 화면이 [다음 →] 을 막으면(출력 번호 중복 등) 카드가 이유를 보이고,
    두 번 눌러 건너뛰면 고칠 수 있는 것은 고치고 넘어간다(10-10 사용자: 튜토리얼은 넘어가라는데 안 넘어갔다)."""
    return Quest(screen, 'act', lambda w: w.next_button, '[다음 →] 으로 넘어가 볼까요?',
                 done=lambda w, s=screen: w.current_step != s,
                 auto=lambda w, s=screen: _go_on(w, s),
                 blocked=lambda w, s=screen: _blocked(w, s))


def _blocked(w, screen: int) -> str:
    """그 화면의 [다음 →] 이 막힌 이유(막히지 않았으면 '')."""
    if w.current_step != screen:
        return ''
    ok, message = w.can_advance()
    if ok:
        return ''
    if screen == 2 and '중복' in str(message):
        return f"{message} · [출력 번호 자동 정렬] 을 누르면 고쳐집니다"
    return str(message) or '이 화면에서 먼저 할 일이 있습니다'


def _go_on(w, screen: int):
    """화면 [다음 →] 대신 넘어간다. 구성 화면이 막혔으면 출력 번호 자동 정렬로 고쳐 보고, 그래도 막히면(역할 중복 등 —
    `go()` 는 진행 조건을 보지 않는다) 넘어가지 않고 False — 카드가 이유를 보인다(10-10 코드 리뷰)."""
    if screen == 2 and not w.can_advance()[0]:
        w.config_page._renumber()               # 출력 번호 중복 → 자동 정렬로 고치고 넘어간다
        if not w.can_advance()[0]:
            return False
    w.go(_after(screen))
    return None


def _after(screen: int) -> int:
    from secs.wizard.strings import FLOW
    return FLOW[FLOW.index(screen) + 1]


def _subs(w) -> int:
    return len(w.project.config.groups().get('sub', []))


def _sample(w) -> None:
    if not w.project.measurements:
        w.measure_page.load_sample()


def _compute(w) -> None:
    page = w.calibration_page
    if page.run is None and page._job is None:
        if page.needs_first_xo():                 # 튜토리얼은 XO 추천 없이 바로 보정 계산(빠르게)
            page.settings.xo_rec = {g: page.xo_of(g) for g in page.xo_group_names()}
            page.settings.store(page.project)
        page.compute_clicked()                    # 튜토리얼은 미리 계산해 둔 결과를 짧게 보여 준다


def _outs(w) -> dict:
    """(역할, 대역 번호) → 측정 출력 번호."""
    return {(s.role, i): b.out for s in w.project.config.speakers for i, b in enumerate(s.bands)}


def _out_changed(w, base: dict) -> bool:
    """남아 있는 스피커 중 하나라도 출력 번호가 바뀌었는가(크기 · 극성 · 스피커 삭제는 아님)."""
    now = _outs(w)
    return any(key in now and now[key] != out for key, out in base.items())


#: 튜토리얼 출력 퀘스트(10-11 사용자: "서브우퍼 1개만 채널을 엉뚱한걸 넣고 이걸 바꾸라고 지시하면 되겠네") — 시작할 때
#: `plant_wrong_out` 이 데모 SW3 의 출력(6)을 16 으로 틀리게 넣고, 퀘스트가 16 → 6 으로 고치게 한다. 데모 구성은 출력이
#: 표 순서대로 1~14(`secs.wizard.new_project`)라 SW3 = 6, 데모 장치는 16채널.
WRONG_ROLE, WRONG_OUT, RIGHT_OUT = 'SW3', 16, 6


def _out_of(w, role: str):
    return next((s.bands[0].out for s in w.project.config.speakers if s.role == role and s.bands), None)


def plant_wrong_out(w) -> bool:
    """튜토리얼 시작 때 부른다(`WizardWindow.set_trial` · `restart_tutorial`): SW3 이 6 이고 16 이 비어 있으면 16 으로."""
    config = w.project.config
    used = {b.out for s in config.speakers for b in s.bands}
    if _out_of(w, WRONG_ROLE) != RIGHT_OUT or WRONG_OUT in used:
        return False
    next(s for s in config.speakers if s.role == WRONG_ROLE).bands[0].out = WRONG_OUT
    w.config_page._touch()
    return True


def _fixed_out(w) -> bool:
    return _out_of(w, WRONG_ROLE) != WRONG_OUT


def _set_out(w) -> None:
    """대신 해 주기 — 틀리게 넣어 둔 SW3 출력을 6 으로(6 이 이미 쓰이면 비어 있는 가장 작은 번호로)."""
    config = w.project.config
    speaker = next((s for s in config.speakers if s.role == WRONG_ROLE and s.bands), None)
    if speaker is None:
        return
    used = {b.out for s in config.speakers for b in s.bands if s is not speaker}
    speaker.bands[0].out = RIGHT_OUT if RIGHT_OUT not in used else min(n for n in range(1, 65) if n not in used)
    w.config_page._touch()


def _out_cells(w) -> list:
    """측정 출력 칸 자리(창 좌표, 표 창에 보이는 만큼) — 안내 카드가 덮지 않게(10-10 3차 시험: 카드가 바꿔야 할 측정 출력 칸
    7~9행을 덮었다)."""
    from PyQt6 import QtCore

    table = w.config_page.table
    view = table.viewport()
    box = QtCore.QRect(view.mapTo(w, QtCore.QPoint(0, 0)), view.size())
    rects = []
    for row in range(table.rowCount()):
        cell = table.cellWidget(row, 4)
        if cell is not None and cell.isVisible():
            r = QtCore.QRect(cell.mapTo(w, QtCore.QPoint(0, 0)), cell.size()).intersected(box)
            if not r.isNull():
                rects.append(r)
    return rects


def _level_busy(w) -> bool:
    page = w.level_page
    return page._playing is not None or bool(getattr(page, '_queue', None))


def _cycle_tabs(w, k: int) -> bool:
    """보정 설계 탭을 SPL → GD → XO(→ 서브 정렬, 보일 때만) → SPL … 로 [계속] 을 누를 때까지 돌린다(10-11 사용자:
    "계속하기 안누르면 루프되게" — 10-10 3차 시험 때는 한 바퀴만 돌았다. 대신 넘기는 간격을 3.6 s 로 늘렸다)."""
    page = w.calibration_page
    tabs = ['spl', 'gd', 'xo'] + (['subs'] if page.tab_buttons['subs'].isVisible() else [])
    page.set_tab(tabs[k % len(tabs)])
    return True


def _tab_cards(w) -> list:
    """탭 줄 + 오른쪽 카드(보이는 것) — 3차 시험: 강조는 RC 범위 카드뿐인데 글은 탭 이야기였다."""
    page = w.calibration_page
    tabs = [b for b in page.tab_buttons.values() if b.isVisible()]
    return [tabs or [page.result_bar], page.rc_card, page.xo_card, page.sub_card]      # 탭 버튼들은 한 묶음


def _on_tab(w, key: str, targets):
    """보정 설계 탭 하나를 펴 두고 그 안을 가리키는 대상(10-11 사용자: 보정 설계 · 서브 정렬을 탭마다 자세히). 안내 카드가
    0.15 s 마다 대상을 다시 찾으니 탭이 다를 때만 바꾼다(그래프를 매번 다시 그리지 않게)."""
    page = w.calibration_page
    if page.tab != key:
        page.set_tab(key)
    return targets


def _row_name(widget):
    """설정 칸이 든 격자 줄의 이름 글자(0열) — 강조 테두리가 '서브' · 'HP' 같은 이름까지 감싸게. 못 찾으면 None."""
    from PyQt6 import QtWidgets

    def grids(layout):
        if layout is None:
            return
        if isinstance(layout, QtWidgets.QGridLayout):
            yield layout
        for i in range(layout.count()):
            child = layout.itemAt(i).layout()
            if child is not None:
                yield from grids(child)

    parent = widget.parentWidget()
    for grid in grids(parent.layout() if parent is not None else None):
        index = grid.indexOf(widget)
        if index >= 0:
            item = grid.itemAtPosition(grid.getItemPosition(index)[0], 0)
            return item.widget() if item is not None else None
    return None


def _sub_fields(w) -> list:
    """서브 정렬 설정 카드의 서브 · 방식 · 값 칸(게인 · 딜레이 · 극성 · 올패스)과 줄 이름."""
    page = w.calibration_page
    fields = [page.sub_pick, page.sub_mode_combo, page.sub_gain_spin, page.sub_delay_spin, page.sub_pol_combo,
              *[s for pair in page.sub_ap_spins for s in pair]]
    # 첫 위젯 = 맨 아래 칸 — 안내 카드는 첫 위젯이 보이게만 스크롤한다(10-11 캡처: 아래 줄이 창 밖으로 잘렸다)
    return [fields[-1], _row_name(page.sub_pick), *fields[:-1]]


def _sub_filters(w) -> list:
    """서브 정렬 설정 카드의 고정 필터 칸(HP · LP · PEQ)과 줄 이름."""
    page = w.calibration_page
    fields = [page.sub_hp_spin, page.sub_band_combos['hp'], page.sub_lp_spin, page.sub_band_combos['lp'],
              *[s for row in page.sub_peq_spins for s in row]]
    return [fields[-1], _row_name(page.sub_hp_spin), *fields[:-1]]          # 첫 위젯 = 맨 아래 칸(`_sub_fields`)


def _subs_running(w) -> bool:
    job = w.calibration_page._job
    return job is not None and job['kind'] == 'subs'


def _levels_full(w) -> bool:
    """모든 채널에 레벨이 들어왔는가 — 신호가 아니라 값으로 판정(2차 시험 t04: 재생 중 [이전으로] 로 갔다 오면 눌렀다는
    신호를 잃어 재생이 끝나도 퀘스트가 안 끝났다)."""
    levels = w.project.extra.get('levels') or {}
    roles = [s.role for s in w.project.config.speakers]
    return bool(roles) and all(role in levels for role in roles)


def _level_meters(w) -> list:
    rows = list(w.level_page._rows.values())
    return [card_of(rows[0][0])] if rows else []


QUESTS: tuple[Quest, ...] = (
    # ── 1 장치·마이크 ──
    Quest(1, 'look', lambda w: w.side_frame, '왼쪽 사이드바 — 구성 · 측정 요약과 [도움말] · [튜토리얼 끝내기] · [프로젝트 저장].'),
    Quest(1, 'look', lambda w: w.steps_bar,
          '위 진행바 — 지금 단계는 색이 채워져 보이고, 단계 이름을 눌러 언제든 오갈 수 있습니다. 사이드바 · 진행바가 자주색이고 '
          '버튼이 분홍이면 튜토리얼 중입니다 — 소리는 나지 않고 측정 파일은 임시 폴더에만 씁니다. 점선 틀은 보기만 하는 곳, '
          '분홍 실선 틀은 직접 눌러 볼 곳입니다.'),
    Quest(1, 'look', lambda w: card_of(w.devices_page.retry_button),
          'REW 연결 카드 — "API 연결됨" 이면 준비 끝. REW Pro 가 없으면 5 측정 화면의 [mdat 불러오기] 부터 시작합니다. 장치 연결 · '
          '레벨 순차 재생 · 자동 측정은 REW Pro 의 API 를 쓰는 기능입니다.'),
    Quest(1, 'look', lambda w: card_of(w.devices_page.input_combo), '마이크 · 오디오 카드 — 입력 장치 · 입력 채널 · 캘리브레이션 · 스윕 길이.'),
    Quest(1, 'act', lambda w: w.devices_page.retry_button, 'REW 연결을 [다시 확인] 해 볼까요?',
          why='측정 전에 SECS 가 REW 를 원격 제어할 수 있는지 확인합니다. 튜토리얼은 데모 REW 라 바로 연결되고, '
              '실제로는 REW Pro 를 API 모드로 먼저 띄웁니다.',
          signal=lambda w: w.devices_page.retry_button.clicked, auto=lambda w: None, note='연결 확인을 건너뛰었습니다.',
          ok='REW 연결을 다시 확인했습니다 — API 연결됨 ✓'),
    _next_to(1),
    # ── 2 구성 ──
    Quest(2, 'look', lambda w: list(w.config_page._preset_buttons.values()),
          '프리셋 줄 — 7.1.4 같은 구성을 한 번에 채웁니다. 오른쪽 서브 − / + 로 서브 수를 바꿉니다. 데모 방은 7.1.4 에 서브 3 입니다.'),
    Quest(2, 'look', lambda w: card_of(w.config_page.table), '출력 패치 시트 — 스피커마다 측정 출력 · 극성 · 크기.'),
    Quest(2, 'look', lambda w: card_of(w.config_page.dsp_profile), 'DSP 프로파일 — 필터를 넣을 장치와 FIR 탭 · IIR 밴드 수.'),
    Quest(2, 'act', lambda w: w.config_page.table,
          'SW3 의 측정 출력이 16 으로 잘못 들어가 있습니다. 표의 서브 그룹에서 SW3 의 "측정 출력" 을 6 으로 바꿔 볼까요?',
          why='측정 출력은 REW 가 그 스피커로 스윕을 내보내는 오디오 인터페이스 출력입니다. 실제 패치와 같아야 '
              '해당 스피커에서 스윕이 나옵니다.',
          done=_fixed_out, auto=_set_out, keep=_out_cells, when=lambda w: _out_of(w, WRONG_ROLE) == WRONG_OUT,
          note='SW3 의 측정 출력을 6 으로 고쳐 두었습니다.'),
    _next_to(2),
    # ── 3 레벨 ──
    Quest(3, 'look', lambda w: card_of(w.level_page.master), '마스터 출력 — REW 출력 레벨과 [전 채널 자동 순차 재생].'),
    Quest(3, 'act', lambda w: w.level_page.sequence_button, '[전 채널 자동 순차 재생] 을 눌러 볼까요?',
          why='입력 레벨이 낮으면 노이즈에 묻히고 너무 높으면 클립됩니다. 채널마다 핑크 노이즈를 틀어 마이크 입력이 파란 레퍼런스 구간 '
              '−30 ~ −15 dBFS 에 드는지 봅니다. 튜토리얼은 실제 마이크가 없어 마이크 미터는 비어 있고, 채널 값은 예시로 채워집니다.',
          done=_levels_full, auto=lambda w: w.level_page.apply_defaults(),
          note='채널마다 기본 레벨을 채워 두었습니다.',
          busy=_level_busy, busy_text='채널을 하나씩 재생하는 중입니다 — 다 돌면 저절로 다음으로 넘어갑니다.',
          keep=_level_meters),
    _next_to(3),
    # ── 4 배치 ──
    Quest(4, 'look', lambda w: card_of(w.layout_page.view), '배치 선택과 위치 그림 — 1번은 늘 청취 위치입니다.'),
    Quest(4, 'look', lambda w: card_of(w.layout_page.timing_combo), '측정 옵션 — 타이밍 레퍼런스 · 레퍼런스 출력.'),
    # 샘플은 처음부터 [단일] 이라 누르게 하면 아무 변화가 없다(10-10 AI 시험) — 둘러보기로 알려만 준다
    Quest(4, 'look', lambda w: w.layout_page._picks[1],
          '튜토리얼 샘플은 청취 위치 1점만 있어 [단일] 이 골라져 있습니다. 실제로는 싱글 · 커플 · 패밀리처럼 여러 위치를 재는 것을 권합니다.',
          why='위치가 많을수록 한 점에만 맞춘 과보정을 피합니다. MIMO 는 위치가 3곳 이상이어야 계산할 수 있습니다.'),
    _next_to(4),
    # ── 5 측정 ── 그래프 소개는 샘플을 채운 뒤(2차 시험: 빈 그래프에 '굵은 선' 설명)
    Quest(5, 'look', lambda w: w.measure_page.positions, '위치 목록 — 지금 잴 위치는 옅은 색 줄이고, 다 재면 "완료" 로 바뀝니다.'),
    Quest(5, 'act', lambda w: w.measure_page.sample_button, '튜토리얼에서는 [데모 데이터] 로 위치 1 을 채워 볼까요?',
          why='보정은 측정한 응답으로 계산합니다. 실제로는 [이 위치 측정] 으로 채널마다 스윕하고, 데모 데이터는 7.1.4 · 서브 3 '
              '데모 방의 청취 위치 1점을 실제로 잰 것입니다.',
          done=lambda w: bool(w.project.measurements), auto=_sample, note='데모 방 위치 1 의 측정을 불러왔습니다.'),
    Quest(5, 'look', lambda w: w.measure_page.plot,
          '그래프 — 채널별 주파수 응답. 위치를 여럿 재면 굵은 선이 청취 위치입니다.'),
    Quest(5, 'look', lambda w: w.measure_page.mdat_button,
          'REW Pro 가 없으면 여기서 시작합니다 — REW 에서 채널마다 측정해 이름을 L · R · SW1 … 로 붙여 저장한 .mdat 를 '
          '[mdat 불러오기] 로 가져옵니다. 위치 2부터는 L_P2 처럼 붙이고, 자세한 건 [도움말] 의 "mdat 불러오기" 탭에 있습니다.'),
    _next_to(5),
    # ── 보정 설계 ── (10-11 사용자: 보정 설계를 더 자세히, 서브 정렬 설명이 부실 — 탭마다 무엇을 보고 어떤 모양이 좋은지)
    Quest(CAL_STEP, 'look', lambda w: card_of(w.calibration_page.compute_button),
          '보정 모드와 [계산] — 측정으로 보정 필터를 만듭니다. 모드는 보정 필터가 더할 지연의 한도로, 최소 지연은 크기만 '
          '고치고 저지연 · 표준은 지연을 더 써서 저역 위상까지 고칩니다.'),
    Quest(CAL_STEP, 'look', lambda w: [w.calibration_page.result_bar, w.calibration_page.plot],
          '탭 SPL · GD · XO 와 그래프 — 계산 전에는 SPL 탭에 측정과 타깃만 보이고, 계산하면 보정 후 곡선과 GD · XO 곡선이 나옵니다.'),
    # 강조 = 탭 줄 + 오른쪽 카드(따로따로), 글은 실제로 나오는 카드 이름(10-10 3차 시험: 강조는 RC 범위 카드인데 글은 탭 이야기)
    Quest(CAL_STEP, 'look', _tab_cards,
          '오른쪽 카드는 탭에 맞춰 바뀝니다 — SPL · GD 탭에서는 RC 범위 카드, XO 탭에서는 XO 카드, 서브 정렬 탭에서는 '
          '서브 정렬 설정 카드가 나옵니다. 탭을 한 번씩 넘겨 보여 드립니다.',
          animate=_cycle_tabs, animate_end=lambda w: w.calibration_page.set_tab('spl'), apart=True),
    Quest(CAL_STEP, 'act', lambda w: w.calibration_page.compute_button, '[계산] 을 눌러 볼까요?',
          why='측정만으로는 소리가 바뀌지 않습니다 — 계산해야 DSP 에 넣을 필터가 나옵니다. 룸 보정 RC 와 서브 정렬을 계산하고, 진행은 '
              '[계산] 아래 줄과 오른쪽 아래 알림에 나옵니다.',
          done=lambda w: w.calibration_page.run is not None, auto=_compute,
          note='계산을 시작해 두었습니다 — 뒤에서 이어 돕니다.',
          busy=lambda w: w.calibration_page.running(),
          # 데모 계산은 3 s 라 아래 줄엔 '(2/12) · 곧 끝남' 만 — 남은 시간 숫자는 오른쪽 아래 알림에 있다(3차 시험)
          busy_text='계산 중입니다 — 진행은 [계산] 아래 줄과 오른쪽 아래 알림에 나오고, 끝나면 저절로 다음으로 넘어갑니다.',
          keep=lambda w: [w.calibration_page.status]),
    Quest(CAL_STEP, 'look', lambda w: _on_tab(w, 'spl', [w.calibration_page.plot, *(
        w.calibration_page.legend_items[k] for k in ('before', 'after', 'target'))]),
          'SPL 탭 — 보정 전 · 보정 후 곡선과 타깃 점선을 겹쳐 봅니다. 회색으로 가린 바깥은 RC 가 손대지 않는 대역이고, '
          '그 안에서 보정 후 곡선이 타깃을 고르게 따라가면 잘된 보정입니다. 좁고 깊은 딥은 일부러 다 채우지 않습니다.'),
    # 계산 뒤 결과 줄을 한 줄씩 짚는다(10-10 3차 시험: '±1 dB 안' · 'GD 폭' · '전체 지연' 이 무슨 숫자인지 몰랐다)
    Quest(CAL_STEP, 'look', lambda w: _on_tab(w, 'spl', [w.calibration_page.metrics_label, w.calibration_page.filters_label,
                                                         w.calibration_page.summary_label]),
          '결과 줄 — "±1 dB 안" 은 20–200 Hz 가 타깃 ±1 dB 안에 든 비율로, 보정 전 → 후로 커질수록 좋습니다. "GD 폭" 은 '
          '20–100 Hz 에서 GD 가 퍼진 폭이라 작을수록 저역이 고르게 도착합니다. 다음 줄은 이 채널의 PEQ · FIR 필터 수, '
          '"전체 지연" 은 보정 필터가 더하는 지연입니다.'),
    Quest(CAL_STEP, 'look', lambda w: _on_tab(w, 'gd', [w.calibration_page.plot, *(
        w.calibration_page.legend_items[k] for k in ('before', 'after'))]),
          'GD 탭 — 주파수마다 소리가 늦게 도착하는 정도를 ms 로 봅니다. 저역에서 솟은 봉우리는 룸 모드가 오래 우는 자리이고, '
          'RC 범위 안에서 보정 후 봉우리가 낮아지고 곡선이 평평해질수록 좋습니다.'),
    Quest(CAL_STEP, 'act', lambda w: w.calibration_page.tab_buttons['xo'], 'XO 탭에서 메인과 서브가 이어지는 모양을 볼까요?',
          why='XO 부근에서 메인과 서브가 위상까지 맞게 합쳐져야 상쇄로 인한 딥이나 험프가 생기지 않습니다. 이 탭에서 채널 그룹마다 XO 와 XO PEQ 를 고릅니다.',
          done=lambda w: w.calibration_page.tab == 'xo', auto=lambda w: w.calibration_page.set_tab('xo'),
          note='XO 탭을 열어 두었습니다. 곡선은 계산이 끝나야 보입니다.'),
    # 그래프(왼쪽)와 XO 카드(오른쪽)를 따로 테두리친다 — 사이 빈칸은 배경일 뿐
    Quest(CAL_STEP, 'look', lambda w: _on_tab(w, 'xo', [[w.calibration_page.plot, *(
        w.calibration_page.legend_items[k] for k in ('xo_main', 'xo_sub', 'xo_sum', 'target'))], w.calibration_page.xo_card]),
          'XO 탭 — 고른 채널의 메인 · 서브와 둘의 합을 타깃과 겹쳐 봅니다. 합이 XO 둘레에서 움푹 꺼지거나 솟지 않고 타깃을 '
          '따라가면 좋은 XO 입니다. 세로선을 끌거나 XO 카드에서 주파수를 바꾸고, [XO 추천 계산] 은 후보마다 끝까지 계산해 '
          '그룹별로 가장 좋은 값을 고릅니다.', apart=True),
    Quest(CAL_STEP, 'act', lambda w: w.calibration_page.tab_buttons['subs'], '서브 정렬 탭도 열어 볼까요?',
          why='서브가 여럿이면 자리마다 서로 보강되거나 상쇄됩니다. 서브 정렬은 서브마다 값을 찾아 서브 합이 고르고, '
              '서로 지우지 않고 크게 더해지게 맞춥니다. 그래서 이 탭은 서브가 둘 이상일 때만 보입니다.',
          done=lambda w: w.calibration_page.tab == 'subs', auto=lambda w: w.calibration_page.set_tab('subs'),
          when=lambda w: _subs(w) >= 2, note='서브 정렬 탭을 열어 두었습니다.'),
    # 서브 정렬 탭을 한 부분씩(10-11 사용자: "서브정렬 쪽 설명이 많이 부실") — 그래프 → 결과 줄 · 값 표 → 설정 카드
    Quest(CAL_STEP, 'look', lambda w: _on_tab(w, 'subs', [w.calibration_page.plot, *(
        w.calibration_page.legend_items[k] for k in ('sub_before', 'sub_after', 'sub_sum', 'target'))]),
          '서브 정렬 그래프 — 색 하나가 서브 하나이고, 점선은 정렬 전 · 실선은 정렬 후입니다. 검은 굵은 선은 모든 서브를 '
          '더한 서브 합, 검은 점선은 정렬 전의 합, 갈색 점선은 타깃입니다. 정렬 후 합이 움푹 꺼진 곳 없이 타깃을 따라갈수록 좋습니다.',
          when=lambda w: _subs(w) >= 2),
    Quest(CAL_STEP, 'look', lambda w: _on_tab(w, 'subs', [w.calibration_page.metrics_label, w.calibration_page.sub_values,
                                                          w.calibration_page.sub_filters_label]),
          '결과 줄과 값 표 — "서브 합 평탄도" 는 서브 합이 출렁이는 폭이라 정렬 전 → 후로 작아질수록 좋고, "좌석 편차" 는 '
          '측정 위치 사이의 차이입니다. 표는 서브마다 DSP 에 넣을 게인 · 딜레이 · 극성 · 올패스이고, 서브 이름 색이 그래프 '
          '곡선 색입니다.',
          why='서브 정렬이 노리는 것이 이 두 숫자입니다 — 청취 위치의 서브 합은 고르게, 자리 사이 차이는 작게. 튜토리얼 '
              '데모는 위치가 1곳이라 좌석 편차가 비어 있습니다.',
          when=lambda w: _subs(w) >= 2),
    Quest(CAL_STEP, 'look', lambda w: _on_tab(w, 'subs', _sub_fields(w)),
          '서브 정렬 설정 — 서브를 하나 골라 방식을 정합니다. 자동은 엔진이 게인 · 딜레이 · 극성을 찾고, 고정은 아래 칸에 '
          '넣은 값을 그대로 DSP 에 넣습니다. Group 은 묶은 서브끼리의 상대값을 지키고 묶음 전체의 값만 엔진이 맞춥니다.',
          why='DBA 처럼 배치로 이미 맞춘 서브는 고정이나 Group 으로 그 값을 지키고, 나머지는 자동에 맡깁니다. '
              '[결과 값 가져오기] 는 마지막 계산 값을 칸에 옮겨 고정의 시작점으로 씁니다.',
          when=lambda w: _subs(w) >= 2),
    # 대상 = 인프라 서브 칸 그 자체(시험이 `target(win) is page.infra_combo` 로 찾는다) — 탭만 펴 둔다
    Quest(CAL_STEP, 'act', lambda w: _on_tab(w, 'subs', w.calibration_page.infra_combo),
          '[인프라 서브] 를 SW3 으로 골라 볼까요? 데모 방의 SW3 은 초저역만 맡는 인프라 서브입니다.',
          why='인프라 서브는 초저역만 맡고 나머지 서브는 그 위 대역을 맡습니다. 인프라에는 LP, 나머지 서브에는 같은 '
              'HP 가 자동으로 걸리고 인프라 대역 레벨을 위 대역에 맞춥니다.',
          done=lambda w: bool(w.calibration_page.infra_state()[0]),
          keep=lambda w: [_row_name(w.calibration_page.infra_combo)],      # '인프라 서브' 이름 글자를 덮지 않게(10-11 캡처)
          auto=lambda w: w.calibration_page.set_infra(w.calibration_page.sub_roles()[-1], 25.0, 'LR24'),
          # 데모 방의 마지막 서브 SW3 = 실측 INFRA(인프라 서브, 10-11) — 대신 해 줄 때도 뜻이 맞다
          when=lambda w: _subs(w) >= 2, note='마지막 서브를 인프라로 정해 두었습니다 — 25 Hz LR24.'),
    Quest(CAL_STEP, 'look', lambda w: _on_tab(w, 'subs', _sub_filters(w)),
          '고정 필터 — HP · LP · PEQ 는 이 서브 출력에 늘 걸리는 필터입니다. 인프라를 고르면 인프라 서브에는 LP, 나머지 '
          '서브에는 HP 가 인프라 XO 주파수로 저절로 들어갑니다. 방식과 상관없이 엔진은 이 필터를 건 응답으로 정렬을 찾습니다.',
          when=lambda w: _subs(w) >= 2),
    # 설정을 바꾸면 결과가 바뀌는 것을 직접 본다 — 튜토리얼은 계산하지 않고, [서브만 계산] 도 인프라를 정한 뒤의 결과를
    # 저장해 둔 것(`calibration.load_demo_subs_run`)을 짧게 보여 준다(10-11 사용자: "연산하는 척만").
    Quest(CAL_STEP, 'act', lambda w: _on_tab(w, 'subs', w.calibration_page.sub_compute_button),
          '설정이 바뀌었습니다. [서브만 계산] 으로 이 탭을 다시 계산해 볼까요?',
          why='설정을 바꾸면 결과가 낡습니다. [서브만 계산] 은 서브 정렬만 실제로 다시 풀어 이 탭의 그래프와 표를 바꾸고, '
              '몇 초 걸립니다.',
          base=lambda w: w.calibration_page.sub_run,
          done=lambda w, before: w.calibration_page.sub_run is not None and w.calibration_page.sub_run is not before,
          auto=lambda w: None, note='[서브만 계산] 은 건너뛰었습니다.',
          busy=_subs_running,
          busy_text='서브 정렬을 다시 계산하는 중입니다 — 끝나면 그래프와 표가 바뀌고 저절로 다음으로 넘어갑니다.',
          ok='서브 정렬을 다시 계산했습니다 — 그래프와 표가 새 결과입니다 ✓',
          when=lambda w: _subs(w) >= 2),
    # 버튼이 오른쪽 칸 맨 아래라 그래프가 밀려 올라간다 — 끝난 뒤 그래프 · 표로 돌아와 바뀐 결과를 본다(10-11 캡처)
    Quest(CAL_STEP, 'look', lambda w: _on_tab(w, 'subs', [w.calibration_page.plot, *(
        w.calibration_page.legend_items[k] for k in ('sub_before', 'sub_after', 'sub_sum')),
        w.calibration_page.metrics_label, w.calibration_page.sub_values]),
          '[서브만 계산] 뒤의 결과 — 인프라 서브는 인프라 XO 아래만, 나머지 서브는 그 위만 맡아 서브 합이 둘을 이어 붙인 '
          '모양이 되고, 값 표도 새로 찾은 값으로 바뀝니다. 이 결과는 서브 정렬 탭에만 보이니, SPL · XO 탭과 내보내기에 '
          '쓰려면 [계산] 을 다시 누릅니다.',
          when=lambda w: _subs(w) >= 2),
    _next_to(CAL_STEP),
    # ── 내보내기 ──
    Quest(EXPORT_STEP, 'look', lambda w: w.export_page.export_button,
          'REW mdat — 채널마다 _1 보정 전 → _2 보정 후 → _3 + MIMO 로 이름 붙인 측정을 씁니다. _2 · _3 은 계산한 예측입니다.'),
    Quest(EXPORT_STEP, 'look', lambda w: w.export_page.camilla_button,
          'CamillaDSP — 구성 화면 DSP 프로파일의 기본값입니다. 보정 모드 이름을 붙인 .yml 설정 파일 하나와, 채널마다 48 kHz WAV '
          '파일 하나씩을 담은 FIR 폴더를 씁니다. 튜토리얼에서는 내보내기 파일을 쓰지 않습니다.'),
)

#: 클리어 수에 쓰는 단계(화면 번호) — 진행바 순서.
SCREENS: tuple[int, ...] = (1, 2, 3, 4, 5, CAL_STEP, EXPORT_STEP)

#: 약어 · 용어 풀이(말, 풀이) — 퀘스트 문구(text + why)에서 처음 나오는 카드에 붙는다. 순서 = 카드에 붙는 순서.
GLOSSARY: tuple[tuple[str, str], ...] = (
    ('REW', 'REW — Room EQ Wizard 측정 프로그램. SECS 가 REW 로 스윕을 냅니다.'),
    ('API', 'API 모드 — SECS 가 REW 를 원격 제어하는 연결. REW Pro 가 필요하고 REW 를 -api 로 실행합니다.'),
    ('캘리브레이션', '캘리브레이션 — 측정 마이크 보정 파일, .cal.'),
    ('스윕', '스윕 — 낮은 주파수에서 높은 주파수로 훑어 올라가는 측정 신호(로그 사인).'),
    ('극성', '극성 — 스피커가 미는지 당기는지(+/−). 두 스피커가 같은 대역을 낼 때 극성이 반대면 저역이 상쇄됩니다.'),
    ('DSP', 'DSP — 보정 필터를 거는 프로세서. CamillaDSP · Tesira · miniDSP 등.'),
    ('FIR', 'FIR — 크기와 위상을 함께 보정하는 필터. 탭이 많을수록 저역 분해능이 좋아집니다.'),
    ('IIR', 'IIR — 파라메트릭 EQ(PEQ) 밴드 같은 짧은 필터. 밴드 단위로 셉니다.'),
    ('dBFS', 'dBFS — 디지털 풀스케일 0 dBFS 에 대한 레벨. −20 dBFS 는 풀스케일보다 20 dB 낮습니다.'),
    ('타이밍 레퍼런스', '타이밍 레퍼런스 — 채널 간 도착 시간을 같은 시계로 재려고 스윕과 함께 내는 레퍼런스 신호.'),
    ('MIMO', 'MIMO — Multi-Input Multi-Output, 다입력 다출력 저역 감쇠. 여러 스피커로 저역 룸 모드의 링잉 · 디케이를 줄이는 보정입니다.'),
    ('mdat', 'mdat — REW 측정 묶음 파일.'),
    ('RC', 'RC — Room Correction, 룸 보정. 방 때문에 생긴 주파수 응답의 피크 · 딥을 잡습니다.'),
    # 10-10 사용자 결정(1A): 보정 설계 SPL 탭은 타깃 기준(타깃 레벨 = 0 dB) — 옛 'dBFS 라 음수' 풀이는 화면과 어긋났다(3차 시험)
    ('SPL', 'SPL — 주파수 응답 그래프. 보정 설계에서는 타깃 레벨을 0 dB 로 두고 그립니다.'),
    ('타깃', '타깃 — 보정 목표 커브.'),
    ('GD', 'GD — Group Delay, 군지연. 주파수별 도착 지연으로, 룸 모드 링잉이 길면 커집니다.'),
    ('XO', 'XO — 크로스오버. 메인과 서브가 대역을 나누는 주파수.'),
    ('PEQ', 'PEQ — 주파수 · 게인 · Q 로 정하는 파라메트릭 EQ 밴드.'),
    ('부스트', '부스트 상한 — 보정이 올릴 수 있는 최대 레벨. 헤드룸과 드라이버를 지킵니다.'),
    ('프리셋', '프리셋 — 7.1.4 같은 채널 구성 프리셋.'),
    ('LP', 'LP · HP — 로우패스는 고역을, 하이패스는 저역을 컷하는 필터.'),
    # 서브 정렬 퀘스트(10-11)에 처음 나오는 말('서브 정렬' 은 [서브 정렬 탭] 퀘스트의 [왜?] 가 풀이한다)
    ('게인', '게인 — 레벨을 올리거나 내리는 값, dB.'),
    ('딜레이', '딜레이 — 신호를 일부러 늦추는 값, ms. 서브마다 도착 시각을 맞춥니다.'),
    ('올패스', '올패스 — 크기는 그대로 두고 한 주파수 둘레의 위상만 돌리는 필터.'),
    ('DBA', 'DBA — Double Bass Array. 앞 벽 서브와 뒷벽 서브를 반대 극성으로 늦춰 틀어 저역 반사를 지우는 배치.'),
)


def _has_term(text: str, term: str) -> bool:
    """문구에 그 말이 있는가 — 영문 약어는 낱말 단위('XO' 가 'XO PEQ' 에는 있지만 'EXPORT' 에는 없다)."""
    if re.fullmatch(r'[A-Za-z]+', term):
        return re.search(rf'(?<![A-Za-z]){term}(?![A-Za-z])', text) is not None
    return term in text


def glossary_for(quests) -> dict[int, tuple[list[str], list[str]]]:
    """퀘스트 순서대로 보며 각 말이 처음 나오는 퀘스트 번호 → (본문 아래 풀이, [왜?] 아래 풀이). 본문(text)에 나온 말이
    먼저 — 같은 카드의 본문과 [왜?] 에 다 있으면 본문 쪽에 붙는다."""
    out: dict[int, tuple[list[str], list[str]]] = {}
    seen: set[str] = set()
    for index, q in enumerate(quests):
        for slot, text in ((0, q.text), (1, q.why)):
            for term, gloss in GLOSSARY:
                if term not in seen and _has_term(text, term):
                    seen.add(term)
                    out.setdefault(index, ([], []))[slot].append(gloss)
    return out


def all_strings() -> list[str]:
    """번역해야 할 퀘스트 문구(테스트가 영어 쌍을 확인한다)."""
    return ([s for q in QUESTS for s in (q.text, q.why, q.note, q.busy_text) if s]
            + [gloss for _term, gloss in GLOSSARY])


__all__ = ['Quest', 'QUESTS', 'SCREENS', 'GLOSSARY', 'EXPORT_STEP', 'card_of', 'all_strings', 'glossary_for']
