# -*- coding: utf-8 -*-
"""도움말(F1) 튜토리얼 문안 — 처음 쓰는 사람이 화면 그림을 보며 1→7단계를 따라 하게 한다(2026-09-24).

사용자 요청: "기능 설명은 커서를 대는 풍선 도움말로 대체하고, 도움말(F1)은 튜토리얼에 가깝게, 사진을
최대한 많이." 그래서 여기에는 **버튼·칸 하나하나의 뜻을 쓰지 않는다** — 뜻은 `tooltips.TIPS`(풍선)가
정본이고, 각 단계 머리에 `HINT` 한 줄로 그쪽을 가리킨다. 계약: `문서/워커 과제/도움말 튜토리얼 계약
2026-09-24.md` 2절.

누가 부름: `help_dialog.HelpDialog`(탭 = 따라 하기 / 확인할 것 / 문제 해결 / 실제 REW, 1단계에만 앞에
'처음이라면'). 입력 없음, 출력은 아래 표들. 그림 키(`img`)는 `assets/tutorial/<key>_<ko|en>.png` 와
`MANIFEST.json` 의 키이고, 그림 속 빨간 번호의 설명은 `CAPTIONS[key]`(번호 순)다.

문구는 한국어 원문, 영어는 `en_g6.py` 에 (한국어, 영어) 쌍으로 둔다(`tests/test_tutorial_text.py` 가
누락 0 을 본다). 사실(버튼 이름·기본값·파일 이름)은 단계 화면 코드와 A 워커의 실제 캡처에서 확인했다 —
화면이 바뀌면 여기와 그림(`tools/make_tutorial_shots.py`)을 같이 고친다.
"""
from __future__ import annotations

from typing import TypedDict


class Action(TypedDict):
    do: str          # 무엇을 누르는지(한 문장)
    see: str         # 무엇이 보이면 다음으로 가는지
    img: str | None  # 그림 키(없으면 None)


class Step(TypedDict):
    goal: str
    steps: list[Action]
    checks: list[str]
    trouble: list[tuple[str, str]]
    real: list[str]


#: 각 단계 '따라 하기' 머리에 한 번 — 기능의 뜻은 풍선 도움말로 보낸다(계약 2절).
HINT = '칸·버튼의 뜻은 커서를 대면 풍선으로 뜹니다.'

#: 탭 이름(순서대로). '처음이라면' 은 1단계에서만 맨 앞에 붙는다.
TAB_INTRO = '처음이라면'
TABS: tuple[tuple[str, str], ...] = (
    ('steps', '따라 하기'),
    ('checks', '확인할 것'),
    ('trouble', '문제 해결'),
    ('real', '실제 REW'),
)

#: 그림 속 번호(①②…)의 설명 — (그림 제목, [번호 순 설명]). 번호는 계약 1절 표의 "강조" 순서이고
#: 실제 캡처에서 번호가 가리키는 것을 보고 적었다(예: s1_rew 의 ① 은 연결된 상태라 [다시 확인]).
CAPTIONS: dict[str, tuple[str, list[str]]] = {
    'side_nav': ('창 왼쪽 사이드바 · 튜토리얼', [
        '단계 목록 — 눌러서 그 단계로 바로 갑니다',
        '[도움말] — 이 창(F1)',
        '[튜토리얼] — 튜토리얼 중에는 [튜토리얼 끝내기]',
        '[프로젝트 저장] · ▾ 다른 이름으로 저장',
    ]),
    's1_overview': ('1단계 화면 전체', ['REW 연결 카드', '마이크 · 오디오 카드', '[다음 →]']),
    's1_rew': ('REW 연결 카드', [
        '연결 버튼 — REW 가 꺼져 있으면 [REW API 모드로 실행], 연결되면 [다시 확인]',
        '연결 상태 — "API 연결됨" 이면 준비 끝',
        '[설정 읽기]',
    ]),
    's1_mic': ('마이크 · 오디오 카드', ['입력 장치', '입력 채널', '캘리브레이션', '스윕 길이']),
    's2_overview': ('2단계 화면 전체 — 데모 방 7.1.4 · 서브 3', ['프리셋 줄', '출력 패치 시트', '배치도 · DSP 프로파일']),
    's2_preset': ('프리셋 줄', ['[7.1.4] 프리셋', '서브우퍼 − / +', '[+ 스피커 추가]']),
    's2_patch': ('출력 패치 시트', ['측정 출력 열', '극성 열', '크기 열']),
    's2_dsp': ('DSP 프로파일 · 필터 자원', ['DSP 장치', 'FIR 탭 · IIR 밴드 수']),
    's3_overview': ('3단계 화면 전체', ['채널별 레벨', '마스터 출력']),
    's3_channel': ('채널 한 줄', ['▶ 재생 / ■ 멈춤', '레벨 막대의 파란 레퍼런스 구간', '지금 레벨(dB)']),
    's3_master': ('마스터 출력 카드', ['마스터 슬라이더(REW 출력 레벨)', '[전 채널 자동 순차 재생]']),
    's4_overview': ('4단계 화면 전체', ['배치 선택', '측정 옵션']),
    's4_pick': ('배치 선택과 위치 그림', ['1번 = 청취 위치', '배치 종류 — 단일 · 싱글 · 커플 · 패밀리']),
    's4_options': ('측정 옵션', ['타이밍 레퍼런스', '레퍼런스 출력']),
    's5_before': ('5단계 — 측정 전', ['지금 위치(위치 1 / 1)', '[이 위치 측정]', '위치 목록']),
    's5_running': ('5단계 — 측정 중', ['진행 표시 — 경과 · 예상 · 남은 시간']),
    's5_after1': ('5단계 — 위치 1 을 잰 뒤', [
        '방금 잰 채널 곡선',
        '위치 목록의 "완료"',
        '[모든 위치 완료] — 1점 샘플은 여기서 끝',
    ]),
    's5_done': ('5단계 — 모든 위치 완료', ['모든 위치 "완료"', '켜진 [다음 →]']),
    's5_mdat': ('측정 대신 mdat 불러오기', [
        '[mdat 불러오기] — REW Pro 없이 REW 에서 잰 측정을 가져옵니다',
        '측정 지연 — 실제로 잴 때 스윕 전에 기다리는 시간',
    ]),
    's9_overview': ('보정 설계 화면 전체', [
        '보정 모드 · [계산]',
        '탭 — SPL · GD · XO · BA',
        '그래프 — 보정 전 · 후 · 타겟',
        '탭에 맞춰 바뀌는 설정 카드',
    ]),
    's9_after': ('계산이 끝난 뒤', ['"계산 완료" 와 전체 지연', 'SPL 그래프 — 보정 후 곡선과 타겟(점선)']),
    's9_spl': ('RC 범위 카드 — SPL · GD 탭', [
        'RC 하한 · 상한(자동)',
        '룸 볼륨 · 치수 — 자동 상한을 정합니다',
        '부스트 상한 — 헤드룸',
    ]),
    's9_xo': ('XO 탭', [
        '채널 — 고른 채널의 메인 · 서브 · 합',
        'XO 주파수',
        'XO PEQ 켜기 · 하한 · 상한(빈칸 = 자동)',
        '[XO 추천 계산]',
    ]),
    's9_subs': ('BA (Bass Alignment) 탭', [
        '인프라 서브 · 인프라 XO · 인프라 최대 부스트',
        '서브마다 방식 — 자동 · 고정 · Group',
        '[서브만 계산]',
        '서브마다 정렬 전은 점선, 정렬 후는 실선 · 검은 선은 서브 합',
    ]),
    's8_overview': ('내보내기 화면 전체', ['내보낼 보정 결과 요약', '내보낼 폴더', '내보내기 버튼']),
    's8_export': ('내보낸 뒤', [
        '[REW mdat 저장]',
        '[CamillaDSP 내보내기]',
        '저장한 파일 수와 폴더 경로',
        '[폴더 열기]',
    ]),
    's8_files': ('내보낸 폴더 안', ['REW mdat — 보정 전 · 후 · MIMO', 'CamillaDSP 설정 — configs 폴더의 yml', 'FIR — coeffs 폴더의 wav']),
}


# ──────────────────────────── 0 · 처음이라면 ────────────────────────────
INTRO = {
    'title': '처음이라면 — 튜토리얼로 한 바퀴',
    'body': [
        '마이크도 REW 도 없이 처음부터 내보내기까지 한 번 가 봅니다. 흐름을 익힌 뒤 실제 측정을 하면 훨씬 쉽습니다.',
        '사이드바의 [튜토리얼] 을 누르면 이 창이 튜토리얼로 바뀝니다. 배포 폴더의 "SECS 위저드 (샘플 데이터).cmd" 와 '
        '같고, 지금 프로젝트는 그대로입니다.',
        '튜토리얼 중에는 사이드바 · 진행바 · 강조 버튼의 키 색이 분홍으로 바뀌고 창 제목 앞에 [튜토리얼] 이 붙습니다. 소리는 '
        '나지 않고 저장 · 내보내기도 되지 않습니다. 측정 곡선은 7.1.4 · 서브 3 데모 방의 청취 위치 1점을 실제로 잰 것입니다.',
        '사이드바에서 단계 목록 · [도움말] · [튜토리얼] · [프로젝트 저장] 자리를 먼저 봐 둡니다.',
        'REW Pro 가 없으면 5 측정 화면의 [mdat 불러오기] 부터 시작하면 됩니다. 앞 단계의 장치 연결 · '
        '레벨 순차 재생과 5단계 자동 측정은 REW Pro(API) 사용자용 기능입니다.',
    ],
    'img': 'side_nav',
    'route': [
        '1 장치·마이크 — 이미 연결돼 있습니다. [다음 →] 을 누릅니다.',
        '2 구성 — 7.1.4 · 서브 3 데모 방이 이미 채워져 있습니다. 그대로 [다음 →].',
        '3 레벨 체크 — [전 채널 자동 순차 재생] 을 누른 뒤 [다음 →].',
        '4 배치 — 단일(청취 위치 1점) 그대로 [다음 →].',
        '5 측정 — [이 위치 측정] 을 누르면 데모 REW 가 바로 채웁니다. [다음 →].',
        '6 보정 설계 — [계산] 을 누르고 SPL · XO · BA 탭을 넘겨 봅니다. [다음 →].',
        '7 내보내기 — 튜토리얼에서는 버튼이 막혀 있습니다. 실제 프로젝트에서 [REW mdat 저장] · [CamillaDSP 내보내기] 를 씁니다.',
        '끝 — 사이드바의 [튜토리얼 끝내기] 로 원래 프로젝트로 돌아옵니다.',
    ],
    'outro': [
        '단계마다 F1 을 누르면 그 단계의 따라 하기가 그림과 함께 열립니다. 창은 열어 둔 채로 화면을 만질 수 있습니다.',
        '실제로 잴 때는 "SECS 위저드 (REW 연동).cmd" 를 쓰고, REW 를 먼저 roomeqwizard.exe -api 로 띄웁니다.',
    ],
}


# ──────────────────────────── 1~8단계 ────────────────────────────
TUTORIAL: dict[int, Step] = {
    1: {
        'goal': 'SECS 가 REW 와 연결됐는지, 측정 마이크가 어디로 들어오는지 확인합니다.',
        'steps': [
            {'do': '화면의 두 카드를 봅니다.',
             'see': '왼쪽이 REW 연결, 오른쪽이 마이크 · 오디오입니다. [다음 →] 은 오른쪽 아래에 있습니다.',
             'img': 's1_overview'},
            {'do': 'REW 연결 카드의 첫 버튼을 누릅니다.',
             'see': 'REW 가 꺼져 있으면 [REW API 모드로 실행] 으로 보이고 REW 를 띄웁니다(10~20 초). '
                    '상태가 초록색 "API 연결됨 · 127.0.0.1:4735" 로 바뀌면 다음으로 갑니다.',
             'img': 's1_rew'},
            {'do': 'Pro 라이선스 줄을 봅니다.',
             'see': '"확인됨 · 자동 측정 가능" 이면 SECS 가 스윕을 직접 돌립니다.',
             'img': 's1_rew'},
            {'do': '입력 장치와 입력 채널을 측정 마이크로 고릅니다.',
             'see': '목록은 REW 에서 읽어 옵니다. 마이크 이름이 보이면 다음으로 갑니다.',
             'img': 's1_mic'},
            {'do': '캘리브레이션과 스윕 길이를 봅니다.',
             'see': '캘리브레이션 파일 이름이 보이면 됩니다. 스윕 길이는 기본값 256k · 5.5 s 그대로 둡니다.',
             'img': 's1_mic'},
            {'do': '[다음 →] 을 누릅니다.',
             'see': '2단계 구성으로 갑니다. 왼쪽 사이드바의 단계 목록으로 언제든 돌아올 수 있습니다.',
             'img': 'side_nav'},
        ],
        'checks': [
            'REW 는 반드시 -api 옵션으로 떠 있어야 합니다. 그냥 켠 REW 에는 연결되지 않습니다.',
            '입력 채널이 마이크를 꽂은 채널과 같은지 봅니다. 다르면 3단계 막대가 움직이지 않습니다.',
            '캘리브레이션 파일은 REW 에서 넣습니다. SECS 는 읽어서 보여 주기만 합니다.',
        ],
        'trouble': [
            ('상태가 "미연결" 에서 바뀌지 않습니다.',
             'REW 가 -api 없이 켜져 있으면 REW 를 닫고 [REW API 모드로 실행] 을 다시 누릅니다. '
             'REW 창이 뜬 뒤에는 [다시 확인] 을 누릅니다.'),
            ('입력 장치 목록이 비어 있습니다.',
             'REW 가 연결된 뒤 [다시 확인] 을 누르면 목록을 다시 읽습니다.'),
            ('[다음 →] 이 눌리지 않고 "Pro 라이선스가 없어" 가 보입니다.',
             'SECS 가 측정을 시작할 수 없습니다. [장치 확인 건너뛰기] 로 넘어간 뒤, REW 에서 직접 잰 mdat 를 '
             '5단계 [측정 건너뛰기 (mdat 불러오기)] 로 가져옵니다.'),
            ('[설정 읽기] 결과에 "알 수 없음" 이 있습니다.',
             'REW 가 그 값을 알려 주지 않은 것입니다. REW 화면에서 직접 확인합니다.'),
        ],
        'real': [
            '샘플 데이터에서는 데모 REW 가 늘 연결돼 있습니다. 실제로는 REW 를 roomeqwizard.exe -api 로 먼저 띄웁니다.',
            'REW Pro 라이선스가 있어야 SECS 가 스윕을 직접 돌립니다. 없으면 REW 에서 재고 SECS 는 mdat 만 읽습니다.',
            '입력 장치 · 채널 · 캘리브레이션은 REW 에 설정된 값을 그대로 읽습니다. REW 가 입력마다 다른 '
            '캘리브레이션을 쓰면 여기 값만으로는 알 수 없습니다.',
        ],
    },
    2: {
        'goal': '어떤 스피커가 DSP 의 몇 번 출력에 물려 있는지 적습니다. 이 표대로 측정 순서와 내보내기 채널이 정해집니다.',
        'steps': [
            {'do': '화면을 위에서부터 봅니다.',
             'see': '프리셋 줄, 출력 패치 시트, 배치도와 DSP 프로파일 순서입니다. 아래쪽은 스크롤해야 보입니다.',
             'img': 's2_overview'},
            {'do': '프리셋 줄에서 내 방 구성을 누릅니다. 샘플 데이터는 [7.1.4] 에 서브 3 입니다.',
             'see': '출력 패치 시트에 스피커 줄이 채워집니다. 7.1.4 는 12줄이고, 샘플은 서브가 둘 더 있어 14줄입니다.',
             'img': 's2_preset'},
            {'do': '서브우퍼 − / + 로 서브 수를 맞춥니다.',
             'see': '서브마다 표에 한 줄씩 늘고 줄어듭니다. 샘플 데이터는 3 입니다.',
             'img': 's2_preset'},
            {'do': '빠진 스피커가 있으면 [+ 스피커 추가] 를 누릅니다.',
             'see': '표 아래에 새 줄이 생깁니다. 필요 없는 줄은 줄 끝의 × 로 지웁니다.',
             'img': 's2_preset'},
            {'do': '줄마다 측정 출력 번호를 실제 배선과 맞춥니다.',
             'see': '표 아래 배지가 초록 "출력 중복 없음" 이면 됩니다.',
             'img': 's2_patch'},
            {'do': '극성과 크기 칸을 봅니다.',
             'see': '배선이 뒤집힌 스피커만 극성을 바꿉니다. 크기는 프리셋 값 그대로 둬도 됩니다.',
             'img': 's2_patch'},
            {'do': '아래로 내려 DSP 프로파일에서 쓰는 DSP 장치를 고릅니다.',
             'see': 'FIR 탭 · IIR 밴드 수가 그 장치에 맞게 바뀝니다. 보정 설계 · 내보내기의 필터가 이 크기에 맞춰집니다.',
             'img': 's2_dsp'},
            {'do': '[다음 →] 을 누릅니다.',
             'see': '3단계 레벨 체크로 갑니다.',
             'img': None},
        ],
        'checks': [
            '측정 출력 번호는 REW 스윕이 나가는 오디오 장치의 출력 번호입니다. DSP 입력 패치도 이 번호대로 맞춥니다.',
            '"출력 중복 없음" 배지가 초록인지 봅니다.',
            '바이앰프 스피커라면 "메인 스피커는 바이앰프(LF/HF)로 시작" 을 켭니다. 샘플 데이터는 끈 채로 둡니다.',
            '샘플 데이터의 실측 곡선은 7.1.4 · 서브 3 · 바이앰프 끔 구성에만 들어 있습니다. 다른 구성은 합성 곡선으로 채워집니다.',
        ],
        'trouble': [
            ('측정 오디오 장치 목록이 비어 있습니다.',
             '1단계에서 REW 가 API 모드로 연결돼야 채워집니다. 1단계로 돌아가 [다시 확인] 을 누릅니다.'),
            ('[다음 →] 이 눌리지 않습니다.',
             '아래 가운데 글이 이유입니다. 대개 출력 번호가 겹치거나 빈 줄이 있습니다.'),
            ('보정 설계의 필터 길이가 장비와 맞지 않습니다.',
             'DSP 프로파일이 실제 장비와 같은지 봅니다. 필터 길이는 여기 FIR 탭 수에서 옵니다.'),
        ],
        'real': [
            '실제로는 표의 측정 출력 번호대로 오디오 인터페이스와 DSP 가 배선돼 있어야 합니다. 번호가 틀리면 다른 스피커가 울립니다.',
            '[패치 시트 CSV] 로 표를 저장해 DSP 입력 패치 작업에 씁니다.',
            'DSP 프로파일은 실제 장비의 FIR 탭 · IIR 밴드 한도와 같아야 내보내기 값을 그대로 넣을 수 있습니다.',
        ],
    },
    3: {
        'goal': '마이크를 청취 위치에 두고 채널마다 소리를 내어 레벨을 맞춥니다.',
        'steps': [
            {'do': '마이크를 청취 위치의 귀 높이에 둡니다.',
             'see': '화면 왼쪽은 채널 목록, 오른쪽은 마스터 출력입니다.',
             'img': 's3_overview'},
            {'do': '마스터 슬라이더를 낮게 둔 채 시작합니다.',
             'see': '소리는 작게 시작해서 조금씩 올립니다.',
             'img': 's3_master'},
            {'do': '첫 채널의 ▶ 를 누릅니다.',
             'see': '버튼이 ■ 로 바뀌고 2.5 초 동안 소리가 납니다. 막대가 파란 레퍼런스 구간(-30 ~ -15 dBFS)에 들어오면 됩니다.',
             'img': 's3_channel'},
            {'do': '막대가 모자라거나 넘치면 마스터 슬라이더를 옮기고 다시 ▶ 를 누릅니다.',
             'see': '줄 끝 값 칸에 지금 레벨(dB)이 나옵니다.',
             'img': 's3_channel'},
            {'do': '[전 채널 자동 순차 재생] 을 누릅니다.',
             'see': '채널이 차례로 재생되고 값 칸이 채워집니다. 아래 "레벨을 아직 못 잰 채널" 글이 사라지면 됩니다.',
             'img': 's3_master'},
            {'do': '[다음 →] 을 누릅니다.',
             'see': '4단계 배치로 갑니다.',
             'img': None},
        ],
        'checks': [
            '모든 채널이 파란 구간 안에 있는지 봅니다. 한 채널만 크게 낮으면 배선과 앰프를 먼저 봅니다.',
            'REW 출력 -6 dBFS 이상은 REW 에서 잠금을 풀어야 합니다. 스피커를 위해 낮게 시작합니다.',
            '마이크 게인은 SECS 가 아니라 REW 나 오디오 장치에서 바꿉니다.',
            '[레벨 건너뛰기 (기본값 적용)] 는 모든 채널을 레퍼런스 구간 안의 값으로 채웁니다. 흐름을 볼 때만 씁니다.',
        ],
        'trouble': [
            ('막대가 전혀 움직이지 않습니다.',
             '1단계의 입력 장치 · 입력 채널이 마이크와 맞는지 봅니다.'),
            ('한 채널만 소리가 안 납니다.',
             '2단계의 측정 출력 번호와 실제 배선을 비교합니다.'),
            ('막대가 파란 구간을 넘습니다.',
             '마스터 슬라이더를 내립니다. 너무 크면 스윕이 클립될 수 있습니다.'),
        ],
        'real': [
            '샘플 데이터에서는 소리가 나지 않고 데모 레벨만 보입니다.',
            '실제로는 핑크 노이즈가, 서브에서는 50 Hz 사인이 나고 REW 가 마이크 입력을 읽습니다. 방이 조용해야 값이 흔들리지 않습니다.',
            '마스터 슬라이더는 REW 출력 레벨(dBFS)을 바꿉니다. 5단계 스윕도 이 레벨로 나갑니다.',
        ],
    },
    4: {
        'goal': '측정 위치를 몇 군데, 어떤 모양으로 잴지 고릅니다. 1번은 늘 청취 위치입니다.',
        'steps': [
            {'do': '왼쪽 배치 선택과 오른쪽 측정 옵션을 봅니다.',
             'see': '배치를 고르면 아래 그림에 위치 번호가 나옵니다.',
             'img': 's4_overview'},
            {'do': '듣는 자리에 맞는 배치를 누릅니다.',
             'see': '샘플은 청취 위치 1점인 단일 그대로 둡니다. 실제로 잴 때는 1인 의자면 싱글 9 위치, '
                    '소파면 커플 13 위치나 패밀리 17 위치를 고릅니다.',
             'img': 's4_pick'},
            {'do': '그림에서 1번 위치를 확인합니다.',
             'see': '1번은 청취 위치(스윗스팟)이고 레벨 · 딜레이 정렬의 레퍼런스입니다. '
                    '반원 표시는 마이크를 20 cm 올리거나 내리는 자리입니다.',
             'img': 's4_pick'},
            {'do': '타이밍 레퍼런스와 레퍼런스 출력을 봅니다.',
             'see': '기본값 "음향 (레퍼런스 스피커)" 를 그대로 둡니다. 레퍼런스 출력은 서브가 아닌 스피커입니다.',
             'img': 's4_options'},
            {'do': '예상 소요를 봅니다.',
             'see': '위치당 시간과 전체 합계가 나옵니다. 마이크를 옮기는 시간은 빠져 있습니다.',
             'img': 's4_options'},
            {'do': '[다음 →] 을 누릅니다.',
             'see': '5단계 측정으로 갑니다.',
             'img': None},
        ],
        'checks': [
            '점 사이는 30 cm 이상, 전체 지름은 1 m 이상이 되게 배치합니다.',
            '높이도 귀 높이 ± 20 cm 로 나눠 잡습니다.',
            '실제 측정은 5 위치 이상을 권장합니다. 단일(1점)은 빠른 확인용이고 샘플이 이 배치입니다.',
            '타이밍 레퍼런스를 "없음" 으로 두면 거리 · 딜레이 · BA 를 계산할 수 없습니다.',
        ],
        'trouble': [
            ('예상 소요가 너무 깁니다.',
             '위치 수가 적은 배치를 고르거나 1단계 스윕 길이를 줄입니다. 스윕이 짧으면 노이즈에 약해집니다.'),
            ('레퍼런스 출력 목록에 원하는 스피커가 없습니다.',
             '목록은 2단계 구성을 따릅니다. 서브는 레퍼런스가 될 수 없습니다.'),
            ('루프백을 골랐는데 시간 값이 이상합니다.',
             '남는 출력을 남는 입력에 케이블로 이어야 합니다. REW 의 레퍼런스 입력도 REW 에서 지정합니다.'),
        ],
        'real': [
            '실제로는 위치마다 마이크를 옮깁니다. 측정 시작 지연을 켜면 [이 위치 측정] 을 누른 뒤 자리로 갈 시간이 생깁니다.',
            '음향 레퍼런스는 스윕마다 레퍼런스 스피커가 짧은 레퍼런스 신호를 먼저 냅니다. 소리가 한 번 더 나는 것이 정상입니다.',
            '소파 밖 20~30 cm 지점을 몇 개 섞으면 과보정을 막습니다.',
        ],
    },
    5: {
        'goal': '표시된 위치에 마이크를 두고, 위치마다 모든 채널을 스윕합니다.',
        'steps': [
            {'do': '위쪽 "위치 1 / 1" 과 오른쪽 위치 목록을 봅니다.',
             'see': '지금 잴 자리가 목록에서 색으로 표시됩니다. 샘플은 위치 1(청취 위치) 하나뿐입니다.',
             'img': 's5_before'},
            {'do': '마이크를 그 자리에 두고 [이 위치 측정] 을 누릅니다.',
             'see': '채널마다 차례로 스윕이 나갑니다.',
             'img': 's5_before'},
            {'do': '측정이 끝날 때까지 기다립니다.',
             'see': '오른쪽 아래 진행 표시에 경과 · 예상 · 남은 시간이 나옵니다. 멈추려면 [측정 취소] 를 누릅니다.',
             'img': 's5_running'},
            {'do': '그래프와 위치 목록을 봅니다.',
             'see': '그래프에 채널 곡선이 그려지고 목록의 01 이 "완료" 로 바뀝니다.',
             'img': 's5_after1'},
            {'do': '위치가 더 있으면 마이크를 옮기고 다시 [이 위치 측정] 을 누릅니다.',
             'see': '샘플은 1점이라 여기서 끝나고 버튼이 [모든 위치 완료] 로 바뀝니다.',
             'img': 's5_after1'},
            {'do': '위치 목록이 모두 "완료" 인지 봅니다.',
             'see': '버튼이 [모든 위치 완료] 로 바뀌고 [다음 →] 이 켜집니다.',
             'img': 's5_done'},
            {'do': 'REW Pro 가 없으면 측정 대신 [mdat 불러오기] 를 누릅니다.',
             'see': 'REW 에서 채널마다 직접 재고, 측정 이름을 L · R · C · SW1 … 로 지어 .mdat 로 저장한 파일을 고릅니다. '
                    '이름대로 채널이 채워진 창이 뜨면 맞는지 보고 [불러오기] 를 누릅니다. 자세한 건 「mdat 불러오기」 탭.',
             'img': 's5_mdat'},
            {'do': '[다음 →] 을 누릅니다.',
             'see': '보정 설계로 갑니다.',
             'img': None},
        ],
        'checks': [
            '측정을 시작하면 프로젝트가 문서\\SECS 폴더에 저절로 저장됩니다. 파일 이름은 사이드바 프로젝트 칸에 보입니다.',
            '이상한 위치는 목록에서 그 줄을 눌러 고른 뒤 다시 재면 그 기록만 바뀝니다.',
            'S/N 이 40 dB 미만이라는 경고가 뜨면 레벨을 올리거나 스윕을 길게 합니다.',
            '측정 중에는 위치를 바꿀 수 없습니다.',
            '위치마다 mdat 를 따로 저장해 불러와도 됩니다. 위치 2 파일은 측정 이름 끝에 _P2 를 붙입니다. 예: L_P2.',
        ],
        'trouble': [
            ('곡선이 노이즈처럼 거칩니다.',
             'S/N 경고를 봅니다. 3단계 레벨을 올리거나 1단계 스윕 길이를 늘립니다.'),
            ('한 채널 곡선만 아주 낮습니다.',
             '2단계 출력 번호와 배선을 봅니다. 고친 뒤 그 위치를 다시 잽니다.'),
            ('마지막 위치를 잘못 쟀습니다.',
             '[마지막 위치 다시] 를 누르면 그 위치를 지우고 다시 잽니다.'),
            ('측정이 끝나지 않습니다.',
             '[측정 취소] 로 멈추고 1단계에서 [다시 확인] 으로 REW 연결을 봅니다.'),
        ],
        'real': [
            '실제 스윕은 채널 수 × 스윕 길이만큼 걸립니다. 7.1.4 · 256k 면 위치당 1분 30초 남짓입니다.',
            '스윕 도중에는 방을 조용히 두고 마이크를 건드리지 않습니다.',
            'REW Pro 가 없으면 REW 에서 직접 재고 [mdat 불러오기] 로 가져옵니다 — 계산 결과는 SECS 로 잰 것과 같습니다.',
            '샘플은 데모 방에서 실제로 잰 1점(청취 위치)뿐입니다. 실제로는 5 위치 이상을 재고, 한 위치를 끝낼 때마다 다음 위치가 저절로 골라집니다.',
        ],
    },
    8: {
        'goal': '보정 결과를 보정 전 · 후를 견주는 REW mdat 와 CamillaDSP 설정으로 내보냅니다.',
        'steps': [
            {'do': '위쪽 요약에서 무엇을 내보내는지 봅니다.',
             'see': '보정 모드 · 채널 · RC 범위와 MIMO 포함 여부가 보입니다.',
             'img': 's8_overview'},
            {'do': '[REW mdat 저장] 을 누릅니다.',
             'see': 'mdat 파일 "SECS 보정 전후 <모드>.mdat" 하나가 저장되고, REW 의 File > Open 으로 엽니다. 채널마다 '
                    '<번호>_<채널>_1 보정 전 · _2 보정 후 · _3 + MIMO 순서입니다. _1 은 청취 위치 측정 그대로, _2 는 채널 보정과 '
                    '서브 합을 계산한 값이고, _3 은 MIMO 를 계산했을 때만 붙습니다. 서브는 측정 그대로 맨 뒤에 DSP 값과 함께 붙습니다.',
             'img': 's8_export'},
            {'do': '[CamillaDSP 내보내기] 를 누릅니다.',
             'see': 'camilladsp 폴더 안에 설정 secs_<모드>.yml 이 든 configs 와 48 kHz FIR wav 가 든 coeffs 가 저장됩니다. '
                    '카밀라 폴더와 같은 모양이라 그대로 복사하면 됩니다. yml 에는 입력 PEQ · FIR, 베이스 매니지먼트 XO, 서브 버스 '
                    '게인 · 딜레이 · 극성 · PEQ, 모든 서브로 가는 LFE +10 dB · 120 Hz 가 들고, MIMO 를 계산했으면 서포트 경로도 '
                    '들어갑니다.',
             'img': 's8_export'},
            {'do': '[폴더 열기] 로 저장한 곳을 엽니다.',
             'see': '프로젝트 옆 SECS_export 아래, 종류 · RC_BA(_MIMO) · 모드 · 시각 이름의 폴더로 나뉩니다.',
             'img': 's8_files'},
            {'do': 'CamillaDSP 에 올리는 법은 「CamillaDSP」 탭을 봅니다.',
             'see': '장치는 시험용 파일(in.raw · out.raw)로 들어 있어 GUI 에서 루프백 · 인터페이스로 바꿉니다. 라우팅은 Mixers 탭의 '
                    'route_in · route_out 번호만 바꿉니다.',
             'img': None},
        ],
        'checks': [
            '계산 결과가 없으면 버튼이 꺼져 있습니다 — 보정 설계에서 [계산] 을 먼저 누릅니다.',
            'MIMO 를 계산했으면 폴더 이름에 _MIMO 가 붙고 mdat 에 "+ MIMO" 측정이 더해집니다.',
            'mdat 의 레벨은 측정 때 REW 의 SPL 오프셋을 그대로 씁니다.',
        ],
        'trouble': [
            ('버튼이 눌리지 않습니다.',
             '보정 설계에서 [계산] 을 끝냈는지 봅니다. 튜토리얼에서는 내보내지 않습니다.'),
            ('REW 에서 mdat 레벨이 이상합니다.',
             'SPL 오프셋이 남게 측정을 REW 로 했는지 봅니다. 불러온 mdat 은 원래 오프셋을 씁니다.'),
        ],
        'real': [
            '내보낸 설정은 CamillaDSP 에 그대로 올립니다. 바꿀 때는 앰프를 먼저 끕니다.',
            '적용한 뒤 청취 위치에서 다시 재서 mdat 의 보정 후 곡선과 비교합니다.',
        ],
    },
    9: {
        'goal': '보정 모드를 고르고 [계산] 을 누른 뒤, SPL · GD · XO · BA 탭을 넘기며 결과를 보고 필요한 것만 '
                '고칩니다. 설정을 바꾸면 "설정이 바뀌었습니다" 만 뜨고, [계산] 을 눌러야 다시 계산합니다.',
        'steps': [
            {'do': '보정 모드를 고르고 [계산] 을 누릅니다.',
             'see': '모드는 보정 필터가 더할 지연의 한도입니다. 최소 지연은 크기만 고치고, 저지연 · 표준은 지연을 더 써서 저역 '
                    '위상까지 고칩니다. 처음 [계산] 은 XO 추천부터 돌리고, 상태 줄에 진행 k/n 과 남은 시간이 나옵니다.',
             'img': 's9_overview'},
            {'do': 'SPL 탭에서 보정 전 · 후 곡선과 타겟 점선을 봅니다.',
             'see': '회색으로 가린 바깥은 RC 가 손대지 않는 대역입니다. 그 안에서 보정 후 곡선이 타겟을 고르게 따라가면 잘된 '
                    '보정이고, 좁고 깊은 딥은 일부러 다 채우지 않습니다. 타겟은 점을 끌어 고치고, 빈 곳을 우클릭하면 플랫 · '
                    '하만 · 시네마 프리셋을 고릅니다.',
             'img': 's9_after'},
            {'do': 'GD 탭과 그래프 아래 결과 줄을 봅니다.',
             'see': 'GD 는 주파수마다 소리가 늦게 도착하는 정도입니다. 저역 봉우리가 낮아지고 곡선이 평평해질수록 좋습니다. '
                    '"±1 dB 안" 은 20–200 Hz 가 타겟 ±1 dB 안에 든 비율, "GD 폭" 은 20–100 Hz 에서 GD 가 퍼진 폭입니다.',
             'img': None},
            {'do': '오른쪽 RC 범위 카드에서 보정할 대역과 부스트 상한을 정합니다.',
             'see': '상한 자동은 룸 볼륨이나 치수로 정합니다. 부스트 상한은 RC · XO PEQ · 서브 버스 PEQ 의 올림을 모두 묶습니다.',
             'img': 's9_spl'},
            {'do': 'XO 탭에서 채널을 고르고 XO 와 XO PEQ 를 봅니다.',
             'see': '그 채널의 메인 · 서브 · 합이 그려지고 카드도 그 채널 그룹으로 바뀝니다. 합이 XO 둘레에서 꺼지거나 솟지 '
                    '않고 타겟을 따라가면 좋은 XO 입니다. 세로선을 끌어 XO 를 바꾸고, XO PEQ 상한 자동 = XO × 4.',
             'img': 's9_xo'},
            {'do': '서브가 둘 이상이면 BA 탭에서 그래프와 값 표를 봅니다.',
             'see': '색 하나가 서브 하나이고 점선은 정렬 전, 실선은 정렬 후, 검은 굵은 선은 서브 합입니다. 서브 합이 꺼진 곳 '
                    '없이 평평할수록, "서브 합 평탄도" 와 "좌석 편차" 가 작을수록 좋습니다. 값 표는 서브마다 DSP 에 넣을 '
                    '게인 · 딜레이 · 극성 · 올패스입니다.',
             'img': 's9_subs'},
            {'do': 'BA 설정 카드에서 인프라 서브와 서브마다 방식을 정합니다.',
             'see': '샘플은 SW3 이 초저역만 맡는 인프라 서브라, 고르면 SW3 에 LP, 나머지 서브에 같은 HP 가 들어갑니다. 방식은 '
                    '자동이면 엔진이 값을 찾고, 고정이면 칸의 값을 그대로 쓰고, Group 이면 묶은 서브의 상대값을 지킵니다. '
                    '바꾼 뒤 [서브만 계산] 을 누르면 이 탭만 다시 계산합니다(몇십 초, 창 구석에 예상 시간). 설정이 이상하면 '
                    '카드 아래에 빨간 경고가 나옵니다.',
             'img': 's9_subs'},
            {'do': '위치가 3곳 이상이면 MIMO 사용을 켜고 [MIMO 계산] 을 눌러도 됩니다.',
             'see': '서포트 필터는 기본 IIR(PEQ 9 + 올패스 3)이고 수 분 걸립니다. [FIR 사용] 을 켜면 서포트마다 8,000탭 '
                    'FIR 을 그대로 써서 1분 안팎이고 저역 감쇠도 더 크지만 FIR 탭을 많이 씁니다 — CamillaDSP 같은 PC DSP 용. '
                    '끝나면 MIMO 줄에 FIR 총 탭과 DSP FIR 예산 비교가 나오고, SPL 탭에 MIMO 적용 곡선이 그려집니다. '
                    '다 봤으면 [다음 →] 으로 내보내기로 갑니다.',
             'img': None},
        ],
        'checks': [
            '모드 · 범위 · 타겟 · XO · 서브 값 같은 설정을 바꾸면 "설정이 바뀌었습니다 — [계산]을 누르세요" 가 나옵니다. '
            '저절로 다시 계산하지 않습니다.',
            '[결과 값 가져오기] 는 계산하지 않고 마지막 결과의 서브 게인 · 딜레이 · 극성 · 올패스를 칸에 옮길 뿐입니다. '
            '[서브만 계산] 결과는 BA 탭에만 보이고, 내보내기에 쓰려면 [계산] 을 다시 누릅니다.',
            'Group 서브는 칸에 넣은 게인 · 딜레이 · 극성 · 올패스를 서로의 상대값으로 그대로 두고, Group 전체의 공통 '
            '게인 · 딜레이 · 극성만 엔진이 맞춥니다.',
            '[XO 추천 계산] 은 후보 XO 마다 보정을 끝까지 계산해 그룹별로 가장 좋은 값을 넣고 다시 계산합니다. 수 분 걸립니다.',
            '[MIMO 계산] 은 저역 감쇠를 줄이는 별도의 무거운 계산입니다. [계산] 을 다시 하면 그 위의 MIMO 결과는 지워지니 '
            '필요하면 다시 누릅니다.',
        ],
        'trouble': [
            ('"측정이 없습니다 — 5단계에서 측정하세요" 가 보입니다.',
             '5단계에서 측정하거나 mdat 를 불러옵니다.'),
            ('[MIMO 계산] 이 눌리지 않습니다.',
             '측정 위치가 3곳 이상이어야 하고, MIMO 사용이 켜져 있고, 먼저 [계산] 으로 보정 결과가 있어야 합니다.'),
            ('계산이 너무 오래 걸립니다.',
             '[계산 취소] 로 멈춥니다. 표준 모드, XO 추천, MIMO(IIR) 는 오래 걸립니다. MIMO 는 [FIR 사용] 을 켜면 훨씬 빠릅니다.'),
            ('BA 에 빨간 경고가 나옵니다.',
             '"통과 대역이 거의 없습니다" 는 그 서브의 LP 가 HP 의 2 배보다 낮아 소리를 거의 못 낸다는 뜻입니다 — 인프라 서브가 '
             '아닌 서브에 LP 가 들어가 있지 않은지 고정 필터를 확인합니다. "게인이 너무 큽니다" 는 서브 게인이 +12 dB 를 넘었다는 '
             '뜻으로, 막힌 대역을 게인으로 채운 것일 수 있습니다. 필터를 고치고 [서브만 계산] 으로 다시 봅니다.'),
        ],
        'real': [
            '측정 위치와 채널이 많을수록 계산이 오래 걸립니다. 계산 중에는 예상 시간이 상태 줄에 나옵니다.',
            'BA (Bass Alignment) 는 서브마다 극성과 딜레이를 도착 시각 ±12 ms 안에서 찾고, 평탄도와 함께 서로 잘 더해지는 정도인 '
            '합산 효율도 봅니다 — 상쇄가 적을수록 같은 앰프로 저역 최대음압이 큽니다.',
            '그래프와 숫자는 계산값입니다. DSP 에 넣은 뒤 다시 재서 확인합니다.',
        ],
    },
}


# ──────────────────────────── 단계별 덧붙임 탭(10-09) ────────────────────────────
#: 단계 → 기본 4탭 뒤에 붙는 탭 (키, 이름). 5단계 = mdat 불러오기(10-09 사용자: 이름 규칙을 자세히 따로 →
#: 10-10 사용자: 규칙이 아니라 "mdat 불러오기" 가 탭 이름이 맞다).
EXTRA_TABS: dict[int, tuple[tuple[str, str], ...]] = {
    5: (('names', 'mdat 불러오기'),),
    8: (('camilla', 'CamillaDSP'),),     # 10-10 사용자: 카밀라 GUI 로 아무것도 없는 상태에서 소리 날 때까지
}

#: mdat 불러오기 탭(10-10 사용자: "대충대충 읽으니 글로 어렵게 쓰지 말고 사진 위주로") — 그림이 본문이고 글은 한 줄씩.
#: 사실은 코드 그대로(`model.role_from_title` · `ROLE_ALIASES` · `ROLE_CODES`, `step5_measure.split_position` ·
#: `auto_assign`, `mdat_loader.suggest_assignment`). 구역 = 번호 머리말 + (앞 줄) + 그림 + (이름 줄) + (뒤 줄).
#: 그림은 `tools/make_mdat_help_shots.py` 가 만든다. 채널 이름 표의 정본은 그 도구의 `CHANNEL_ROWS`.
NAMES: dict = {
    'start': [
        'REW Pro 가 없으면 여기서 시작합니다.',
        '장비 연결 · 레벨 재생 · 자동 측정은 REW Pro 기능입니다. 없으면 REW 에서 직접 재서 .mdat 로 저장한 뒤, '
        '측정 화면의 [mdat 불러오기] 를 누릅니다.',
    ],
    'sections': [
        {'head': 'REW 에서 측정 이름을 이렇게 짓습니다.', 'before': [], 'img': 'mdat_names', 'rows': [], 'after': []},
        {'head': '파일을 고르면 이런 창이 뜹니다.', 'before': [], 'img': 'mdat_dialog', 'rows': [], 'after': []},
        {'head': '위치마다 파일을 따로 저장해도 됩니다.',
         'before': ['[mdat 불러오기] 를 두 번 누르면 위치 1, 2 에 차례로 들어갑니다.',
                    '[파일 관리] 에서는 위치를 골라 파일을 넣거나, 그 위치만 비웁니다.'],
         'img': 'mdat_files', 'rows': [], 'after': []},
        {'head': '채널 이름은 이 표대로 씁니다.', 'before': [], 'img': 'mdat_channels', 'rows': [],
         'after': ['SUB · SW · LFE 처럼 번호 없이 쓰면 SW1 입니다.',
                   '서브가 여럿이면 SW2 처럼 번호를 붙여 씁니다. SW 2 처럼 띄우면 위치 2 입니다.',
                   '대소문자 · 기호는 안 봅니다.']},
        {'head': '이런 이름은 자동으로 못 알아봅니다.', 'before': [], 'img': None,
         'rows': [('L EQ', 'L 뒤에 EQ 가 더 붙었습니다.'),
                  ('DBA L', 'L 앞에 DBA 가 붙었습니다.'),
                  ('INFRA', '채널 이름이 아닙니다.')],
         'after': ['창에서 직접 고르면 됩니다.']},
        {'head': '기억할 것', 'before': [], 'img': None, 'rows': [],
         'after': ['REW 에서 잴 때 타이밍 레퍼런스를 Acoustic 으로 두세요. 그래야 채널 사이 시간 차가 남습니다.',
                   '위치 수는 4단계에서 정합니다. 그보다 큰 위치 번호는 건너뜁니다.']},
    ],
}

#: mdat 불러오기 탭 그림의 제목과 번호 설명 — `CAPTIONS` 와 같은 모양. 계약 그림 키(27개)와 섞이지 않게 따로 둔다.
#: 번호 수 = MANIFEST 의 강조 수(`tests/test_help_dialog.py` 가 본다).
NAMES_CAPTIONS: dict[str, tuple[str, list[str]]] = {
    'mdat_names': ('측정 이름 = 채널 이름 + 위치 번호', [
        '위치 1 은 채널 이름만 씁니다.',
        '위치 2 는 이름 뒤에 _P2 를 붙입니다. 위치 3 은 _P3.',
    ]),
    'mdat_dialog': ('파일을 고르면 뜨는 창', [
        'REW 에서 지은 측정 이름',
        '이름을 보고 SECS 가 채널을 채워 둡니다',
        '못 알아본 이름은 [건너뜀]. 쓰려면 직접 고릅니다',
        '맞으면 [불러오기]',
    ]),
    'mdat_files': ('파일이 둘이면', [
        '첫 번째 [mdat 불러오기] — 위치1.mdat 가 위치 1 에 들어갑니다',
        '두 번째 [mdat 불러오기] — 위치2.mdat 가 위치 2 에 들어갑니다',
    ]),
    'mdat_channels': ('', []),
}


#: 내보내기 「CamillaDSP」 탭(10-10 사용자: "다른 사람이 윈도우나 맥에서 카밀라 GUI 로 설정할 텐데, 아무것도 없는 상태에서 뭘
#: 어떻게 넣어야 라우팅이 되고 세팅이 되는지 … 그림 많이"). mdat 불러오기 탭과 같은 모양(머리말 + 그림 + 짧은 줄) — 그림은
#: `tools/make_camilla_help_shots.py`. 사실 근거는 `문서/조사/CamillaDSP GUI 적용 조사 2026-10-10.md`(CamillaDSP 4.1.3 +
#: GUI 4.1.0 소스 · 이 PC 실측)와 내보내기 코드(`export_camilladsp` — configs/ · coeffs/ · route_in · route_out).
#: GUI 화면 이름은 4.1 기준이라 버전에 따라 조금 다를 수 있다고 맨 위에 적는다. 채널 예시는 바뀌지 않을 이름만.
CAMILLA: dict = {
    'start': [
        'SECS 보정을 CamillaDSP 로 틉니다 — 아무것도 없는 PC 에서 소리 날 때까지.',
        '윈도우 · 맥 모두 같은 순서입니다. CamillaDSP 4.1 기준이라 버전에 따라 화면 이름이 조금 다를 수 있습니다.',
    ],
    'sections': [
        {'head': '준비물 네 가지', 'before': [], 'img': 'cam_kit', 'rows': [], 'after': []},
        {'head': '소리 길', 'before': [], 'img': 'cam_path', 'rows': [],
         'after': ['플레이어 소리를 루프백으로 보냅니다. 윈도우는 CABLE Input 을 기본 재생 장치로, 맥은 소리 출력을 BlackHole 로.',
                   '윈도우 VB-CABLE 은 스테레오가 확실한 길입니다. 7.1.4 같은 12채널은 맥 BlackHole 16ch 가 쉽습니다.']},
        {'head': '카밀라 폴더부터 만듭니다', 'before': [], 'img': 'cam_folder', 'rows': [],
         'after': ['configs · coeffs 가 없으면 GUI 가 켜지자마자 꺼집니다.',
                   '사용자 이름에 한글이 있으면 C:\\camilladsp 처럼 영문 자리에 만들고 GUI 설정 파일의 경로를 고칩니다. '
                   '파일 자리: gui\\camillagui_backend\\_internal\\config\\camillagui.yml']},
        {'head': 'SECS 가 내보낸 파일 넣기',
         'before': ['내보낸 camilladsp 폴더는 카밀라 폴더와 같은 모양입니다. 두 폴더 안의 파일을 그대로 복사합니다.'],
         'img': 'cam_export', 'rows': [], 'after': []},
        {'head': '켜는 순서 — 창 두 개 + 브라우저', 'before': [], 'img': 'cam_start', 'rows': [],
         'after': ['명령은 camilladsp 폴더에서 칩니다. 윈도우는 그 폴더의 주소창에 cmd 를 치면 됩니다.',
                   '-g-20 은 볼륨 −20 dB 로 시작한다는 뜻입니다. 처음엔 앰프도 낮춰 둡니다.',
                   '맥은 "터미널이 마이크에 접근" 창이 뜨면 허용합니다. 거부하면 오류 없이 무음입니다.']},
        {'head': 'GUI 로 올려도 됩니다 — Files 탭', 'before': ['파일을 복사했으면 ★ 만 누르면 됩니다.'],
         'img': 'cam_files', 'rows': [],
         'after': ['wav 가 없어도 Valid 로 보이고 적용할 때 실패합니다. 그래서 wav 를 먼저 올립니다.',
                   '같은 이름은 묻지 않고 덮어씁니다.']},
        {'head': '장치 — Devices 탭', 'before': [], 'img': 'cam_devices', 'rows': [],
         'after': ['장치 이름은 돋보기로 고릅니다. 카밀라 창이 켜져 있어야 목록이 나옵니다.',
                   '맥은 type CoreAudio, 캡처는 BlackHole 16ch, 리샘플러는 비워 둡니다.',
                   'format 은 비워 둡니다. F32_LE 로 두면 오류가 납니다.',
                   '윈도우는 소리 제어판 › 장치 속성 › 고급 › 기본 형식을 두 장치 모두 48000 Hz 로 맞춥니다.',
                   'chunksize 는 1024 로 시작합니다. 끊기면 2048, 지연을 줄이려면 512.']},
        {'head': '순서표 — 설정 파일 맨 위', 'before': [], 'img': 'cam_order', 'rows': [],
         'after': ['번호는 0 부터입니다. 인터페이스 출력 1 = 0.',
                   'GUI 에서 저장하면 이 주석은 지워집니다. 같은 순서가 Title 탭 설명에도 있습니다.']},
        {'head': '플레이어 순서가 다르면 — Mixers 탭 route_in',
         'before': ['대부분의 플레이어는 LFE 를 4번째로 보냅니다. SECS 입력은 LFE 가 맨 뒤입니다.'],
         'img': 'cam_route_in', 'rows': [],
         'after': ['칸을 누르면 작은 창이 뜹니다. 연결을 지우고 새 칸의 + 를 누릅니다.',
                   '윈도우 7.1 순서는 FL FR FC LFE BL BR SL SR — 뒤가 옆보다 먼저입니다.']},
        {'head': '배선이 다르면 — route_out',
         'before': ['예: 인터페이스 출력 4 에 서브, 5 · 6 에 서라운드가 꽂힌 경우.'],
         'img': 'cam_route_out', 'rows': [],
         'after': ['out 을 바꾸면 Devices 의 재생 channels 도 같은 수로 바꿉니다.', '비워 둔 출력은 무음입니다.']},
        {'head': '적용하고 확인', 'before': [], 'img': 'cam_check', 'rows': [],
         'after': ['플레이어를 틀고 한 채널씩 소리를 내 맞는 스피커가 우는지 봅니다. 채널 테스트 파일이나 REW 신호 발생기를 씁니다.',
                   'Clipped samples 가 늘면 플레이어 볼륨이나 GUI 의 Volume 을 내립니다.',
                   '잘 되면 Save to file. 다음부터는 카밀라 창만 켜면 마지막 설정으로 바로 소리가 납니다.']},
        {'head': '지연 — 영상 싱크', 'before': [], 'img': 'cam_latency', 'rows': [],
         'after': ['보정 모드마다 소리가 늦게 나옵니다. 지연 값은 설정 파일 맨 위와 Title 탭에 있습니다.',
                   '영상은 플레이어의 오디오 싱크를 그만큼 당깁니다. 초고지연 200 ms 면 −200 ms.']},
        {'head': '자주 나는 문제', 'before': [], 'img': None, 'rows': [],
         'after': ['GUI 가 바로 꺼짐 → camilladsp 안에 configs · coeffs 폴더를 만듭니다.',
                   'State 가 Offline → 카밀라 창이 꺼졌거나 -p1234 없이 켰습니다.',
                   'State 가 STALLED → 플레이어가 루프백이 아닌 장치로 내보내고 있습니다.',
                   'RUNNING 인데 무음 → 맥은 마이크 권한, 윈도우는 기본 재생 장치와 48000 Hz 를 봅니다.',
                   'wrong number of channels → 장치 channels 를 route_in 의 in, route_out 의 out 과 같게 합니다.',
                   'unknown variant F32_LE → 장치의 format 을 비웁니다.',
                   '몇 분마다 끊김 → Rate adjust 를 켭니다. 윈도우는 리샘플러 AsyncSinc 도. 그래도 끊기면 chunksize 를 키웁니다.',
                   'Unable to find coefficient file → coeffs 폴더에 wav 가 있는지 봅니다.']},
    ],
}

#: CamillaDSP 탭 그림의 제목과 번호 설명 — 번호 수 = MANIFEST 의 강조 수(`tests/test_help_dialog.py`).
CAMILLA_CAPTIONS: dict[str, tuple[str, list[str]]] = {
    'cam_kit': ('준비물', [
        'CamillaDSP — github.com/HEnquist/camilladsp 의 4.1.3 릴리스. 압축만 풉니다',
        'CamillaGUI — github.com/HEnquist/camillagui-backend 의 4.1.0 번들',
        '루프백 — 윈도우 VB-CABLE · vb-audio.com, 맥 BlackHole',
        '오디오 인터페이스 — 지금 쓰는 그대로',
    ]),
    'cam_path': ('소리 길', [
        '루프백 — 카밀라가 여기서 소리를 받습니다. 캡처 장치',
        'CamillaDSP — route_in → SECS 보정 → route_out',
        '인터페이스 — 카밀라가 여기로 내보냅니다. 재생 장치',
    ]),
    'cam_folder': ('카밀라 폴더', [
        'bin · gui — 받은 압축을 풉니다',
        'configs · coeffs — 빈 폴더를 꼭 미리 만듭니다',
    ]),
    'cam_export': ('내보낸 폴더 → 카밀라 폴더', [
        'configs 안의 yml → 카밀라 configs',
        'coeffs 안의 wav → 카밀라 coeffs. 설정 안 경로가 ../coeffs/ 라 그대로 열립니다',
        'in.raw — SECS 검사용. 옮기지 않습니다',
    ]),
    'cam_start': ('켜는 순서', [
        '카밀라 엔진 — 이 창은 열어 둡니다. 닫으면 소리가 멈춥니다',
        'GUI 서버 — 창이 하나 더 뜹니다',
        '브라우저로 이 주소를 엽니다',
    ]),
    'cam_files': ('Files 탭', [
        'Filters 의 Upload files — coeffs 의 wav 를 모두 먼저',
        'Configs 의 Upload files — 그다음 yml',
        'Valid 에 초록 체크 — 아니면 마우스를 올려 이유를 봅니다',
        '★ — 활성으로 지정하고 GUI 로 불러옵니다',
    ]),
    'cam_devices': ('Devices 탭 — 윈도우 예', [
        'samplerate 48000 · chunksize 1024 로 시작',
        'Capture device = 루프백. channels = route_in 의 in',
        'Playback device = 인터페이스. channels = route_out 의 out',
        '시계가 다른 두 장치라 Rate adjust 를 켭니다. 리샘플러 AsyncSinc 는 윈도우만',
    ]),
    'cam_order': ('설정 파일 맨 위 순서표 — 5.1 예', [
        '입력 — 카밀라가 받는 순서',
        '출력 — 카밀라가 내보내는 순서',
    ]),
    'cam_route_in': ('route_in — 플레이어가 LFE 를 4번째로 보낼 때', [
        '가로 = 플레이어가 보내는 채널. 번호는 0 부터',
        'Ls · Rs · LFE 줄만 칸을 옮깁니다',
    ]),
    'cam_route_out': ('route_out — 서브는 출력 4, 서라운드는 5 · 6', [
        'out = 인터페이스 출력 수. Devices 의 재생 channels 도 같게',
        '출력 4 · 5 · 6 줄만 칸을 옮깁니다. 카밀라 번호는 하나 작습니다',
    ]),
    'cam_check': ('GUI 왼쪽 패널', [
        'Apply to DSP — GUI 의 설정을 카밀라로 보냅니다',
        'State 가 RUNNING 이면 돌고 있습니다',
        '입력 · 출력 미터 — 플레이어를 틀면 움직입니다',
        'Clipped samples — 늘면 볼륨을 내립니다',
        'Save to file — 잘 되면 저장합니다',
    ]),
    'cam_latency': ('보정 모드별 지연', [
        '영상 플레이어의 오디오 싱크 — 지연만큼 당깁니다',
    ]),
}

#: 그림 탭(덧붙임 탭 중 그림이 본문인 것) — 탭 키 → (문안, 그림 캡션). `help_dialog` 가 같은 모양으로 그린다.
PICTURE_TABS: dict[str, tuple[dict, dict]] = {
    'names': (NAMES, NAMES_CAPTIONS),
    'camilla': (CAMILLA, CAMILLA_CAPTIONS),
}


def names_image_keys() -> list[str]:
    """mdat 불러오기 탭이 쓰는 그림 키(순서대로)."""
    return [s['img'] for s in NAMES['sections'] if s['img']]


def camilla_image_keys() -> list[str]:
    """CamillaDSP 탭이 쓰는 그림 키(순서대로)."""
    return [s['img'] for s in CAMILLA['sections'] if s['img']]


def caption_of(key: str) -> tuple[str, list[str]]:
    """그림 키 → (제목, 번호 설명). 계약 그림이든 mdat 불러오기 · CamillaDSP 탭 그림이든."""
    return CAPTIONS.get(key) or NAMES_CAPTIONS.get(key) or CAMILLA_CAPTIONS.get(key) or ('', [])


def picture_strings(data: dict, captions: dict, keys) -> list[str]:
    """그림 탭 하나의 문구를 정해진 순서로 편다(`keys` = 한국어 캡션 키 순서). rows 앞의 이름은 번역하지 않는다."""
    out = list(data['start'])
    for section in data['sections']:
        out.append(section['head'])
        out += section['before']
        out += [text for _name, text in section['rows']]
        out += section['after']
    for key in keys:
        title, legend = captions[key]
        if title:                                              # 제목이 빈 그림은 쌍을 만들지 않는다
            out.append(title)
        out += legend
    return out


def picture_shape(data: dict, captions: dict, keys) -> tuple:
    sections = tuple((s['img'], len(s['before']), len(s['rows']), len(s['after'])) for s in data['sections'])
    return (len(data['start']), sections, tuple((key, bool(captions[key][0]), len(captions[key][1])) for key in keys))


def camilla_strings(camilla=None, captions=None) -> list[str]:
    """CamillaDSP 탭 문구(`en_g6` 가 같은 모양의 영어로 짝을 맞춘다)."""
    return picture_strings(CAMILLA if camilla is None else camilla,
                           CAMILLA_CAPTIONS if captions is None else captions, CAMILLA_CAPTIONS)


def camilla_shape(camilla=None, captions=None) -> tuple:
    return picture_shape(CAMILLA if camilla is None else camilla, CAMILLA_CAPTIONS if captions is None else captions,
                         CAMILLA_CAPTIONS)


def names_strings(names=None, extra_tabs=None, captions=None) -> list[str]:
    """덧붙임 탭 이름 + mdat 불러오기 문구를 정해진 순서로 편다(`en_g6` 가 같은 모양의 영어로 짝을 맞춘다)."""
    names = NAMES if names is None else names
    extra_tabs = EXTRA_TABS if extra_tabs is None else extra_tabs
    captions = NAMES_CAPTIONS if captions is None else captions
    out = [label for step in sorted(EXTRA_TABS) for _key, label in extra_tabs[step]]
    out += list(names['start'])
    for section in names['sections']:
        out.append(section['head'])
        out += section['before']
        out += [text for _name, text in section['rows']]      # 앞의 이름(L EQ …)은 번역하지 않는다
        out += section['after']
    for key in NAMES_CAPTIONS:                                 # 키 순서는 한국어 표를 따른다
        title, legend = captions[key]
        if title:                                              # 제목이 빈 그림은 쌍을 만들지 않는다
            out.append(title)
        out += legend
    return out


def names_shape(names=None, captions=None) -> tuple:
    names = NAMES if names is None else names
    captions = NAMES_CAPTIONS if captions is None else captions
    sections = tuple((bool(s['img']), len(s['before']), len(s['rows']), len(s['after'])) for s in names['sections'])
    return (len(names['start']), sections,
            tuple((key, bool(captions[key][0]), len(captions[key][1])) for key in NAMES_CAPTIONS))


def all_strings(intro=None, tutorial=None, captions=None, hint=None, tab_intro=None, tabs=None) -> list[str]:
    """화면 문구 전부를 정해진 순서로 편다. 인자를 안 주면 이 모듈의 한국어 원문.

    `en_g6.py` 가 같은 모양의 영어 표를 넘겨 같은 순서로 펴고, 둘을 짝지어 (한국어, 영어) 쌍을 만든다 —
    그래서 한국어를 두 번 옮겨 적지 않는다(모양이 다르면 `en_g6` 가 임포트 때 멈춘다).
    """
    intro = INTRO if intro is None else intro
    tutorial = TUTORIAL if tutorial is None else tutorial
    captions = CAPTIONS if captions is None else captions
    out: list[str] = [HINT if hint is None else hint, TAB_INTRO if tab_intro is None else tab_intro]
    out += [label for _key, label in (TABS if tabs is None else tabs)]
    out += [intro['title'], *intro['body'], *intro['route'], *intro['outro']]
    for key in CAPTIONS:                       # 키 순서는 한국어 표를 따른다
        title, legend = captions[key]
        out.append(title)
        out.extend(legend)
    for number in sorted(TUTORIAL):
        step = tutorial[number]
        out.append(step['goal'])
        for action in step['steps']:
            out += [action['do'], action['see']]
        out += step['checks']
        for symptom, fix in step['trouble']:
            out += [symptom, fix]
        out += step['real']
    return out


def image_keys() -> list[str]:
    """문안이 참조하는 그림 키(순서대로, 중복 포함)."""
    keys = [INTRO['img']]
    for step in TUTORIAL.values():
        keys += [a['img'] for a in step['steps'] if a['img']]
    return keys


__all__ = ['TUTORIAL', 'INTRO', 'CAPTIONS', 'HINT', 'TABS', 'TAB_INTRO', 'EXTRA_TABS', 'NAMES', 'NAMES_CAPTIONS',
           'CAMILLA', 'CAMILLA_CAPTIONS', 'PICTURE_TABS', 'all_strings', 'names_strings', 'names_shape',
           'names_image_keys', 'camilla_image_keys', 'camilla_strings', 'camilla_shape', 'picture_strings',
           'picture_shape', 'caption_of', 'image_keys']
