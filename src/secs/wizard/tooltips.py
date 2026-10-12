# -*- coding: utf-8 -*-
"""풍선 도움말 — 버튼·칸·표 머리·그래프에 커서를 잠시 대면 뜨는 '기능 설명'의 단일 출처(2026-09-24).

누가 부름: 단계 화면(`step1_devices` … `step5_measure` · `step_calibration` · `step8_mdat`)·창(`window`)이 위젯을 만들 때
`attach(위젯, 키)` / `attach_header(표, 키들)` / `attach_cells(표, 키)` 로 키를 단다. 창이
`install(app)` 으로 앱 전체 이벤트 필터(`TipFilter`)를 한 번 걸고, 필터가 ToolTip 이벤트에서
`TIPS[키]` 를 **그 순간의 언어로** 옮겨(`secs.i18n.translate`) 보여 준다 — 언어를 바꿔도 따로
다시 달 필요가 없다.

입력: 위젯 동적 속성 `secsHelp`(키 문자열), 표 머리 속성 `secsHeaderHelp`(열 순서 키 목록)·
`secsHeaderHelpAll`(모든 열 공통 키), 표·목록 칸 속성 `secsCellHelp`. 출력: 폭 `MAX_WIDTH_PX`
안에서 낱말 단위로 줄을 바꾼 풍선. 위젯에 원래 툴팁(값의 전체 이름 등)이 있으면 그것을 첫 줄
(굵게)에 두고 설명을 아래에 붙인다.

키 규칙: `<구역>.<이름>` — 구역은 `win`(창 공통: 사이드바·단계 바·내비), `s1`~`s8`(단계), `cal`(보정 설계, 09-26),
`mdat`(5단계 mdat 배정 창), `load`(로딩 막). 한 위젯 = 한 키. 선택지가 여럿이면 줄 나눔 목록
(`- 이름: 뜻`)으로 쓰고 한 문단에 이어 붙이지 않는다. 문구는 한국어 원문이고 영어는
`en_g5.py` 에 (한국어, 영어) 쌍으로 등록한다(`tests/test_tooltips.py` 가 전수 검사).

버튼·칸 뜻의 정본은 여기다(F1 도움말은 순서를 안내하는 튜토리얼 — `tutorial_text`).
"""
from __future__ import annotations

import atexit
import html

from PyQt6 import QtCore, QtGui, QtWidgets

#: 풍선 최대 폭(px) — 이보다 긴 줄은 낱말 사이에서 바꾼다.
MAX_WIDTH_PX = 420
#: 커서를 대고 이만큼 머물러야 뜬다(ms). Qt 기본(스타일 힌트 SH_ToolTip_WakeUpDelay = 700 ms)보다
#: 조금 길게 — 표·버튼을 스치기만 할 때 설명이 튀어나오지 않게(2026-09-24 사용자 "길게 대면").
HOVER_DELAY_MS = 800

HELP_PROP = 'secsHelp'
HEADER_PROP = 'secsHeaderHelp'
HEADER_ALL_PROP = 'secsHeaderHelpAll'
CELL_PROP = 'secsCellHelp'

TIPS: dict[str, str] = {
    'win.help': '이 단계 튜토리얼 창 열기(F1). 열어 둔 채 작업 가능.',
    'win.help_search': '지금 탭 안 글자 찾기(Ctrl+F). Enter = 다음.',
    'win.help_tabs': '도움말 구분.\n- 따라 하기: 누를 순서\n- 확인할 것: 흔한 실수\n- 문제 해결: 증상별 점검\n- 실제 REW: 실측에서 달라지는 점',
    'win.help_close': '도움말 닫기. F1 · [도움말]로 다시 열기.',
    'win.help_viewer': '그림 확대 보기. 클릭 · Esc 로 닫기.',
    'win.save': '프로젝트(.secs) 저장. 측정 IR 은 옆 <이름>.irs 폴더에 저장.',
    'win.mic_meter': '마이크 입력 레벨 창(떠 있음). 단계를 옮겨도 유지.',
    'win.load': '저장한 프로젝트(.secs) 열기. 1단계부터 다시 확인.',
    'win.language': '화면 언어(한국어 / English). 다음 실행에도 유지.',
    'win.step_bar': '그 단계로 바로 이동.',
    'win.prev': '이전 단계. 입력값 유지.',
    'win.next': '다음 단계. 이 단계에 빠진 값이 있으면 꺼지고, 이유는 가운데에 표시.',
    'win.skip1': 'REW 연결 없이 진행. mdat 불러오기용.',
    'win.skip3': '레벨 확인 건너뛰기. 기본값 사용.',
    'win.skip5': '측정 대신 REW 측정(.mdat) 불러오기.',
    'win.trial_end': '튜토리얼 종료. 실제 REW · 이전 프로젝트로 복귀. 튜토리얼 데이터는 저장 안 됨.',
    'win.save_more': '저장 옵션. 다른 이름으로 저장(Ctrl+Shift+S) — 그 파일이 지금 프로젝트가 됨.',
    'win.tutorial_restart': '튜토리얼을 처음부터 다시.',
    'win.trial': '튜토리얼 전환. 7.1.4 · 서브 3 데모 방 샘플 · 데모 REW, 키 색 분홍. 저장 · 내보내기 안 됨.',
    'load.cancel': '계산 중지. 이전 결과 유지.',
    's1.rew_launch': 'REW API 연결.\n- REW API 모드로 실행: -api 로 띄움(10~20 s)\n- 다시 확인: 상태 · 장치 다시 읽기\nAPI 모드가 아니면 REW 를 닫고 다시.',
    's1.read_settings': 'REW 측정 레벨 · 마이크 캘 파일 다시 읽기.',
    's1.input_device': '측정 마이크 입력 장치(REW 목록).',
    's1.input_channel': '마이크가 꽂힌 입력 채널.',
    's1.output_driver': '스윕 출력 드라이버. Equalizer APO 사용 시 ASIO 제외.',
    's1.sweep': '스윕 길이(샘플 · 초). 길수록 SNR ↑ · 시간 ↑. 기본 256k(≈5.5 s).',
    's2.preset': '스피커 구성 프리셋(메인.서브.탑). 바꾸면 측정 삭제.',
    's2.preset_custom': '빈 구성에서 시작. [+ 스피커 추가]로 추가.',
    's2.sub_count': '서브우퍼 수(0~8). 서브마다 출력 1개. 바꾸면 측정 삭제.',
    's2.biamp': '메인을 LF · HF 두 출력(바이앰프)으로 시작. 기본 끔.',
    's2.room': '스피커 배치도. 원 클릭 = 대역 · 출력 · 극성 요약. 보라 사각형 = 서브.',
    's2.device': '스윕 출력 Windows 장치. 목록은 REW 에서 읽고, 1단계 API 연결이 필요.',
    's2.role': '채널(돌비 이름). 바꾸면 구성이 사용자 정의.',
    's2.size': '스피커 크기.\n- 소형: 저역을 XO 로 서브에(베이스 매니지먼트)\n- 대형: 저역까지 직접 재생',
    's2.drive': '앰프 구성(출력 수).\n- 싱글: 1\n- 바이: LF · HF\n- 트라이: LF · MF · HF',
    's2.band': '이 줄의 대역 — FULL · LF · MF · HF · SUB. 대역마다 필터 따로.',
    's2.out': '스윕 출력 번호 = DSP 입력 패치 번호. 겹치면 "중복" 표시.',
    's2.polarity': '극성 정상 ↔ 반전 전환.',
    's2.remove': '이 스피커 삭제.',
    's2.add': '스피커 추가. 빈 채널 중 첫 번째로.',
    's2.renumber': '출력 번호를 표 순서로 1부터 다시 매김.',
    's2.csv': '표를 CSV 로 저장(DSP 패치용).',
    's2.dsp_profile': '쓰는 DSP. 고르면 FIR 탭 · IIR · 올패스 같은 필터 자원이 그 장비 값으로 채워짐. 손으로 고치면 사용자 정의.',
    's2.fir_taps': '채널당 FIR 길이(탭). 괄호 안 지연은 선형위상 지연으로, 48 kHz 에서 탭 ÷ 2. 길수록 분해능 ↑.',
    's2.fir_budget': 'DSP 전체 FIR 탭 예산. 채널 FIR 과 서포트 경로 FIR 이 나눠 씀.',
    's2.iir_bands': '채널당 IIR(PEQ) 밴드 수.',
    's2.allpass_bands': '채널당 올패스 밴드 수(PEQ 와 별도). 0 = 안 씀.',
    's2.bm_delays': '메인 → 서브 버스 베이스 매니지먼트 센드에 채널마다 걸 수 있는 지연 수. 0 = 서브를 메인 합 하나에 맞춤.',
    's2.fir_note': 'FIR 탭이 정하는 값(48 kHz).\n- 창 길이 = 탭 ÷ 샘플레이트\n- 분해능 = 샘플레이트 ÷ 탭',
    's3.play': '핑크 노이즈 2.5 s 재생, 서브는 50 Hz 사인. ■ = 정지.',
    's3.meter': '마이크 입력 레벨(dB). 목표 = 파란 구간 -30 ~ -15 dB.',
    's3.master': 'REW 출력 레벨(dBFS), 바로 전송. 낮게 시작. -6 dBFS 이상은 REW 잠금 해제 필요.',
    'mic.reset_peak': '피크 유지값 지우기.',
    's3.sequence': '모든 채널 차례로 재생.',
    's4.arrangement': '측정 위치 배치. 5곳 이상 권장. 바꾸면 측정 삭제.',
    's4.arrangement_single': '청취 위치 1곳만(빠른 확인). 좌석 편차 · MIMO 불가. 바꾸면 측정 삭제.',
    's4.map': '마이크 위치. 반원 = 귀 높이 ±20 cm, 점선 원 = 지름 1 m.',
    's4.start_delay': '[이 위치 측정] 후 스윕 시작까지 대기(초). 예상 시간에 포함.',
    's4.timing': '채널 도착 시간 레퍼런스.\n- 음향: 레퍼런스 스피커가 스윕마다 신호음(반복 1 고정)\n- 루프백: 남는 출력 → 입력 케이블\n- 없음: 크기 보정만, 딜레이 · BA 불가',
    's4.timing_speaker': '음향 레퍼런스 신호음을 내는 스피커(서브 제외).',
    's4.timing_channel': '루프백 출력 채널.',
    's5.sweep': '스윕 길이 — 길수록 노이즈에 강하고 오래 걸림.',
    's5.repeat': '스윕 반복 — 같은 채널을 여러 번 재 평균. 음향 타이밍 레퍼런스는 1.',
    's5.measure': '현재 위치의 모든 채널 스윕. 잰 위치면 다시 재서 교체.',
    's5.cancel': '스윕 중지.',
    's5.redo': '마지막 위치 지우고 다시 측정.',
    's5.mdat': 'REW 측정(.mdat) 가져오기. 이름 짓는 법은 F1 "mdat 불러오기".',
    's5.sample': '7.1.4 · 서브 3 데모 방 샘플로 위치 1 채우기. 튜토리얼 전용.',
    's5.map': '측정 위치. 클릭 · ←/→ 로 선택. 측정 중 고정.',
    's5.positions': '위치 목록. 줄 클릭 = 현재 위치.',
    's5.plot': '채널별 응답. 굵은 선 = 위치 1. SNR < 40 dB 면 레벨 ↑ 또는 스윕 길게.',
    'mdat.role': '배정할 스피커. 안 쓰면 [건너뜀]. 한 위치에 같은 채널은 한 번만.',
    'mdat.pos': '이 측정을 넣을 청취 위치. 이름의 P2 · _P3 에서 읽어 둔 값.',
    's5.files': '위치마다 mdat 파일을 따로 넣고 빼는 창.',
    's5.plot_toggle': '그래프 · 범례를 숨기고 배치도를 크게. 다시 누르면 그래프.',
    'files.load': '이 위치에 mdat 를 불러옴. 파일 속 측정은 모두 이 위치로, 든 채널만 바뀜.',
    'files.clear': '이 위치의 측정만 지움. 다른 위치는 그대로.',
    'win.plot_axes': '가로 · 세로 축의 하한/상한. [자동] = 기본. 종료 전까지 유지.',
    'win.plot_positions': '위치별 곡선 켜기/끄기. 칩에 커서 = 그 위치 강조. 1 = 청취 위치.',
    'win.auto': '측정 후 보정 설계 자동 계산 상태. "완료" 면 내보내기 가능.',
    'win.tutorial_pause': '튜토리얼 잠시 치우기. 안내 카드 · 막 · 목록이 사라져 모든 기능을 자유롭게 사용. 오른쪽 위 [튜토리얼 이어 하기]로 복귀.',
    'win.tutorial_resume': '멈췄던 안내 카드로 복귀. 그 단계 화면으로 이동하고, 이미 해 둔 것은 넘어감.',
    's8.folder_change': '내보낼 폴더 변경(프로젝트에 저장).',
    's8.folder_open': '내보낼 폴더 열기.',
    's8.export_before_after': 'REW mdat 1개: 채널마다 보정 전 · 보정 후(· MIMO). 보정 후는 계산값.',
    'cal.mode_zero': '최소 지연: IIR(PEQ)만. 추가 지연 0, 위상 보정 없음.',
    'cal.mode_mixed': '저지연: PEQ + 짧은 FIR. 저역 초과위상(주로 XO) 보정.',
    'cal.mode_linear': '표준: 지연 한도를 더 써서 서브 롤오프 · 방 초과위상까지 보정.',
    'cal.mode_ultra': '초고지연: 지연 30~200 ms, FIR 최대 16,384탭. 영상 · 라이브 부적합.',
    'cal.ultra_ms': '초고지연 전체 지연 한도, 30~200 ms. FIR 길이는 자동 — 200 ms = 16,384탭.',
    'cal.compute': '레벨 · 딜레이 · BA · XO · 채널 필터를 계산. 첫 계산은 XO 추천부터.',
    'cal.cancel': '계산 중지. 이전 결과 유지.',
    'cal.rc_lo': '룸 보정(RC) 하한(Hz). 아래는 반 옥타브에 걸쳐 0 으로.',
    'cal.rc_hi_auto': 'RC 상한 자동 = 슈뢰더 주파수, 최대 500 Hz. 룸 볼륨이 없으면 500 Hz.',
    'cal.rc_hi': 'RC 상한(Hz, 수동). 위상 보정도 이 안에서만.',
    'cal.volume': '룸 볼륨(m³). 자동 상한 = 2000·√(T60/V). 없음 = 500 Hz.',
    'cal.dim_w': '방 가로(m). 세 값 입력 시 부피 자동.',
    'cal.dim_d': '방 세로(m). 세 값 입력 시 부피 자동.',
    'cal.dim_h': '방 높이(m). 세 값 입력 시 부피 자동.',
    'cal.mimo_max': '입력당 MIMO 서포트 상한. 0 = 제한 없음.',
    'cal.mimo_fir': '서포트를 FIR 그대로 씁니다(서포트당 8,000탭). 끄면 IIR(PEQ 9 + 올패스 3)로 근사합니다.',
    'cal.mimo_on': 'MIMO 사용 — 다입력 다출력 저역 감쇠. 측정 위치 3곳 이상.',
    'cal.fsiso': 'MIMO 가 다루는 가장 높은 주파수. 이 위로는 MIMO 를 걸지 않음. 50~150 Hz.',
    'cal.mimo_low_auto': '서포트 하한 자동 판정.',
    'cal.mimo_low': '서포트 하한(Hz, 수동). 자동 판정값 아래로는 안 내려감.',
    'cal.mimo_compute': 'MIMO 서포트 계산(수 분). [계산] 후 다시 필요.',
    'cal.xo_group': 'XO 를 볼 · 고칠 그룹.',
    'cal.xo_hz': '그룹 XO(40~200 Hz). 결과는 [계산]으로 갱신.',
    'cal.xo_recommend': '후보 XO 60 · 80 · 100 · 120 Hz 를 모두 계산해 그룹별 최적값 적용. 수 분 걸림.',
    'cal.tab_xo': '고른 채널의 메인 · 서브 · 합 · 타겟(1/12 oct). 세로선 끌기 = XO.',
    'cal.channel': '결과를 볼 채널.',
    'cal.tab_spl': '청취 위치 SPL(1/24 oct): 보정 전 · 후 · 타겟 · MIMO.',
    'cal.tab_gd': 'GD, 1/24 oct. 순수 지연은 뺌. 낮고 평탄할수록 좋음.',
    'cal.tab_subs': 'BA (Bass Alignment): 서브마다 정렬 전 = 점선, 정렬 후 = 실선. 검은 굵은 선 = 서브 합, 검은 점선 = 정렬 전 합.',
    'cal.sub_values': '서브마다 DSP 에 넣을 게인 · 딜레이 · 극성 · 올패스. 이름 색 = 곡선 색.',
    'cal.sub_pick': '고칠 서브.',
    'cal.sub_mode': '정렬 방식.\n- 자동: 엔진이 게인 · 딜레이 · 극성 결정\n- 고정: 입력값 그대로\n- Group: 안의 상대값 유지, 공통 값만 엔진',
    'cal.sub_gain': '게인(dB). 고정 = 출력값, Group = 안의 상대값.',
    'cal.sub_delay': '딜레이(ms). 고정 = 출력값, Group = 안의 상대값.',
    'cal.sub_polarity': '극성. 고정 = 그대로, Group = 안의 상대 극성.',
    'cal.sub_ap_f': '올패스 중심(Hz). 비우면 없음.',
    'cal.sub_ap_q': '올패스 Q.',
    'cal.sub_main_delay': '고정 서브가 있을 때 모든 메인에 더하는 딜레이(ms).',
    'cal.sub_hp_type': 'HP 종류 — 끔 · LR · BW, 숫자는 dB/oct. 기본 LR 24.',
    'cal.boost_max': '부스트 상한(dB). RC · XO PEQ · 서브 버스 PEQ 모두 적용. 기본 10 dB.',
    'cal.xo_peq_on': 'XO PEQ(6밴드) 사용 — XO 둘레를 서브와 잇기 전 타겟에 맞춤.',
    'cal.xo_peq_lo': 'XO PEQ 하한(Hz). 자동 = XO ½.',
    'cal.xo_peq_hi': 'XO PEQ 상한(Hz). 자동 = XO × 4.',
    'cal.sub_compute': 'BA 만 다시 계산. 메인 · MIMO · 내보내기 결과는 그대로.',
    'cal.infra_boost': '인프라 서브 게인 상한(dB). 기본 24 dB = 대역 레벨 끝까지 맞춤.',
    'cal.infra_sub': '인프라 서브 지정. 인프라 = XO 아래만, 나머지 = 위만. 없음 = 끔.',
    'cal.infra_xo': '인프라 XO 주파수(Hz).',
    'cal.infra_type': '인프라 XO 필터 종류. 인프라 LP 와 나머지 HP 가 같이 씀. 기본 LR 24.',
    'cal.sub_lp_type': 'LP 종류 — 끔 · LR · BW, 숫자는 dB/oct. 기본 LR 24.',
    'cal.sub_hp': '서브 고정 HP(Hz). 0 = 없음.',
    'cal.sub_lp': '서브 고정 LP(Hz). 0 = 없음.',
    'cal.sub_peq_f': '서브 고정 PEQ 중심(Hz). 비우면 없음.',
    'cal.sub_peq_q': '고정 PEQ Q.',
    'cal.sub_peq_gain': '고정 PEQ 게인(dB). 0 = 없음.',
    'cal.sub_mimo': '이 서브를 MIMO 서포트로 사용.',
    'cal.sub_take': '마지막 결과의 서브 값을 칸에 복사(계산 없음).',
    'cal.sub_layout': 'Group 레이아웃.\n- 상대값 고정: 입력값 유지\n- DBA · 멀티서브: 같은 필터, 레벨만 서브별',
    'cal.sub_dba_row': 'DBA 줄: 앞 = 정상, 뒤 = 반전 + 전파 지연.',
    'cal.sub_dba_trim': 'DBA 뒤 줄 지연 보정(ms). 자동값에 더함.',
    'cal.metrics': '결과 숫자, 보정 전 → 후.\n- ±1 dB 안: 20–200 Hz 중 타겟 ±1 dB 안에 든 비율, 클수록 좋음\n- GD 폭: 20–100 Hz GD 의 상위 10 % − 하위 10 %, 작을수록 고르게 도착\n- 서브 합 평탄도: BA 탭, 서브 합이 출렁이는 폭, 작을수록 좋음\n- 좌석 편차: 측정 위치 사이 서브 합 차이, 작을수록 좋음',
    'cal.plot': '보정 결과. 타겟 점 끌기 · 더블클릭 추가 · 우클릭 삭제/메뉴. 점선 = RC 하한 · 상한.',
    'cal.folder_change': '내보낼 폴더 변경(프로젝트에 저장).',
    'cal.folder_open': '내보낼 폴더 열기.',
    'cal.export_mdat': 'REW mdat 1개: 채널마다 보정 전 · 보정 후(· MIMO). 보정 후는 계산값.',
    'cal.export_camilla': 'CamillaDSP v4 설정(.yml) + 채널 FIR(wav). 장치는 파일 입출력 — 실사용 시 변경.',
}


# ──────────────────────────── 다는 쪽 ────────────────────────────
def _check(key: str) -> str:
    if key not in TIPS:
        raise KeyError(f'tooltips.TIPS 에 없는 키: {key}')
    return key


def attach(widget, key: str):
    """위젯에 풍선 도움말 키를 단다. 위젯을 그대로 돌려준다(한 줄로 쓰려고)."""
    widget.setProperty(HELP_PROP, _check(key))
    return widget


def attach_all(widgets, key: str) -> None:
    for widget in widgets:
        attach(widget, key)


def attach_header(table, keys) -> None:
    """표(QTableView)의 가로 머리에 키를 단다. keys = 열 순서 목록(빈 문자열 = 없음) 또는 모든 열 공통 문자열."""
    header = table.horizontalHeader()
    if isinstance(keys, str):
        header.setProperty(HEADER_ALL_PROP, _check(keys))
        return
    for key in keys:
        if key:
            _check(key)
    header.setProperty(HEADER_PROP, [str(k or '') for k in keys])


def attach_cells(view, key: str) -> None:
    """표·목록의 칸(아이템)에 커서를 댈 때 보일 키. 칸 자체의 툴팁이 있으면 그것이 첫 줄."""
    view.setProperty(CELL_PROP, _check(key))


# ──────────────────────────── 찾기 · 문구 ────────────────────────────
def _own_tip(widget) -> str:
    try:
        return str(widget.toolTip() or '')
    except RuntimeError:
        return ''


def resolve(widget) -> tuple[object, str]:
    """(키를 가진 위젯, 키). 위젯에서 부모로 올라가며 키를 찾는다 — 스핀박스 안쪽 편집칸처럼 안쪽
    부품에 커서가 있어도 바깥 위젯의 설명이 뜬다. 자기 툴팁만 가진 위젯을 만나면 거기서 멈춘다
    (그 툴팁이 원래대로 뜬다)."""
    node = widget
    while node is not None and isinstance(node, QtWidgets.QWidget):
        key = node.property(HELP_PROP)
        if key:
            return node, str(key)
        if _own_tip(node) or node.isWindow():
            return None, ''
        node = node.parentWidget()
    return None, ''


def _translate(text: str) -> str:
    from secs.i18n import translate
    from secs.wizard import strings  # noqa: F401 - 영어 쌍(en_g5) 등록
    return str(translate(text))


def help_for(key: str) -> str:
    """키의 설명을 지금 언어로."""
    return _translate(TIPS[key]) if key in TIPS else ''


def tip_text(widget) -> str:
    """위젯에 커서를 댈 때 보일 글(평문) — 원래 툴팁(첫 줄) + 설명. 키가 없으면 원래 툴팁."""
    owner, key = resolve(widget)
    if not key:
        return _own_tip(widget)
    own = _own_tip(owner)
    body = help_for(key)
    return f'{own}\n{body}' if own else body


def header_key(header, logical: int) -> str:
    keys = header.property(HEADER_PROP)
    if keys and 0 <= logical < len(keys) and keys[logical]:
        return str(keys[logical])
    return str(header.property(HEADER_ALL_PROP) or '')


def _wrap(text: str, metrics: QtGui.QFontMetrics, width: int) -> list[str]:
    """낱말 단위 줄바꿈 — 한글도 낱말(띄어쓰기) 사이에서만 바꾼다. '- ' 목록 줄은 이어지는 줄을 들여 쓴다."""
    lines: list[str] = []
    for paragraph in text.split('\n'):
        indent = '  ' if paragraph.startswith('- ') else ''
        words = paragraph.split(' ')
        current = ''
        for word in words:
            candidate = f'{current} {word}' if current else word
            if current and metrics.horizontalAdvance(candidate) > width:
                lines.append(current)
                current = indent + word
            else:
                current = candidate
        lines.append(current)
    return lines


def to_html(head: str, body: str, width: int = MAX_WIDTH_PX) -> str:
    """풍선에 넣을 서식 글: 첫 줄(원래 툴팁)은 굵게, 설명은 폭 안에서 낱말 단위로 줄바꿈."""
    metrics = QtGui.QFontMetrics(QtWidgets.QToolTip.font())
    bold = QtGui.QFont(QtWidgets.QToolTip.font())
    bold.setBold(True)
    parts = []
    if head:
        lines = _wrap(head, QtGui.QFontMetrics(bold), width)
        parts.append('<b>' + '<br>'.join(html.escape(line) for line in lines) + '</b>')
    if body:
        parts.append('<br>'.join(html.escape(line) for line in _wrap(body, metrics, width)))
    # white-space:pre — Qt 가 한글을 음절 사이에서 다시 접지 않게(줄은 위에서 이미 나눴다).
    return '<p style="white-space:pre">' + '<br>'.join(parts) + '</p>'


# ──────────────────────────── 보여 주기 ────────────────────────────
class TipFilter(QtCore.QObject):
    """앱 전체 ToolTip 이벤트를 가로채 키가 있는 위젯이면 설명 풍선을 띄운다."""

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self._timer = QtCore.QTimer(self)
        self._timer.setSingleShot(True)
        self._timer.timeout.connect(self._show_pending)
        self._pending: tuple | None = None

    def eventFilter(self, watched, event) -> bool:  # noqa: N802 - Qt 이름
        if event.type() != QtCore.QEvent.Type.ToolTip or not isinstance(watched, QtWidgets.QWidget):
            return False
        try:
            found = self._lookup(watched, event.pos())
        except RuntimeError:
            return False
        if found is None:
            return False
        anchor, head, key = found
        text = to_html(head, help_for(key))
        extra = HOVER_DELAY_MS - _wake_delay()
        if extra > 0 and not QtWidgets.QToolTip.isVisible():
            self._pending = (event.globalPos(), text, anchor)
            self._timer.start(extra)
        else:
            QtWidgets.QToolTip.showText(event.globalPos(), text, anchor)
        return True

    def _lookup(self, watched, pos):
        """(풍선 기준 위젯, 첫 줄, 키) 또는 None."""
        parent = watched.parentWidget()
        if isinstance(parent, QtWidgets.QHeaderView) and watched is parent.viewport():
            key = header_key(parent, parent.logicalIndexAt(pos))
            if not key:
                return None
            model = parent.model()
            logical = parent.logicalIndexAt(pos)
            head = ''
            if model is not None and logical >= 0:
                head = str(model.headerData(logical, parent.orientation(),
                                            QtCore.Qt.ItemDataRole.DisplayRole) or '')
            return watched, head, key
        if isinstance(parent, QtWidgets.QAbstractItemView) and watched is parent.viewport():
            key = str(parent.property(CELL_PROP) or '')
            if not key:
                return None
            index = parent.indexAt(pos)
            head = str(index.data(QtCore.Qt.ItemDataRole.ToolTipRole) or '') if index.isValid() else ''
            return watched, head, key
        owner, key = resolve(watched)
        if not key:
            return None
        return owner, _own_tip(owner), key

    def _show_pending(self) -> None:
        pending, self._pending = self._pending, None
        if pending is None:
            return
        pos, text, anchor = pending
        try:
            if not anchor.isVisible():
                return
            local = anchor.mapFromGlobal(QtGui.QCursor.pos())
        except RuntimeError:
            return
        if anchor.rect().contains(local):       # 그 사이 커서가 떠났으면 띄우지 않는다
            QtWidgets.QToolTip.showText(pos, text, anchor)


def _wake_delay() -> int:
    app = QtWidgets.QApplication.instance()
    if app is None:
        return 700
    return int(app.style().styleHint(QtWidgets.QStyle.StyleHint.SH_ToolTip_WakeUpDelay))


def install(app: QtWidgets.QApplication) -> TipFilter:
    """앱에 한 번만 건다. 두 번 불러도 같은 필터를 돌려준다.

    필터는 앱의 자식으로 두지 않고, 파이썬이 끝나기 전에(atexit) 떼어 낸다 — 앱이 지워지는 동안
    이미 정리 중인 파이썬 필터가 불려 종료 때 access violation 이 났다(09-24 테스트).
    """
    global _installed
    if _installed is not None:
        try:
            if _installed[0] is app:
                return _installed[1]
        except RuntimeError:
            pass
    tip_filter = TipFilter()
    app.installEventFilter(tip_filter)
    _installed = (app, tip_filter)
    app._secs_tip_filter = tip_filter
    return tip_filter


_installed: tuple | None = None


@atexit.register
def _uninstall() -> None:
    global _installed
    if _installed is None:
        return
    app, tip_filter = _installed
    _installed = None
    try:
        app.removeEventFilter(tip_filter)
    except RuntimeError:
        pass


__all__ = ['TIPS', 'MAX_WIDTH_PX', 'HOVER_DELAY_MS', 'attach', 'attach_all', 'attach_header',
           'attach_cells', 'resolve', 'tip_text', 'help_for', 'header_key', 'to_html', 'install',
           'TipFilter']
