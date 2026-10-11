# -*- coding: utf-8 -*-
"""튜토리얼(게임식, 2026-10-09 — `tutorial_quests` · `tutorial_overlay`) 문구의 한국어 원문 → 영어 쌍.

`strings.py` 가 다른 묶음과 똑같이 `add_korean` 으로 등록한다.
"""

PAIRS: tuple[tuple[str, str], ...] = (
    # 오버레이 · 카드 · 퀘스트 창
    ('왜?', 'Why?'),
    ('← 이전으로', '← Back'),
    ('다음으로 →', 'Next →'),
    ('튜토리얼 목록 접기', 'Fold the tutorial list'),
    ('퀘스트', 'Quests'),
    ('클리어', 'cleared'),
    ('둘러보기', 'Tour'),
    ('눌러 볼까요?', 'Try this'),
    ('튜토리얼 다시', 'Restart tutorial'),
    ('튜토리얼 완료', 'Tutorial complete'),
    ('샘플 데이터로 측정부터 내보내기까지 한 바퀴 돌았습니다. 실제로는 REW 를 API 모드로 띄우고 '
     '여러 위치(싱글 9곳 이상)를 잽니다.',
     'You went from measuring to export with sample data. For a real measurement, start REW in API mode and '
     'measure several positions (Solo layout, 9 or more positions).'),
    ('처음부터 다시', 'Start over'),
    ('실제 측정 시작', 'Start real measurement'),
    ('확인 ✓', 'Done ✓'),
    ('아직 넘어갈 수 없어요 — {0}', 'You cannot move on yet — {0}'),
    ('{0} · [출력 번호 자동 정렬] 을 누르면 고쳐집니다', '{0} · [Renumber outputs] fixes it'),
    ('이 화면에서 먼저 할 일이 있습니다', 'Something on this screen needs doing first'),
    ('해 봤어요 ✓', 'Already done ✓'),
    ('분홍 테두리 안을 눌러 보세요', 'Click inside the pink outline'),
    ('카드의 [계속 →] 으로 넘어가세요', 'Use [Continue →] on the card to go on'),
    # 튜토리얼 잠시 치우기(10-11 사용자: 튜토리얼을 잠시 치우고 다른 기능을 만질 수 있게 — 카드의 아이콘)
    ('튜토리얼 잠시 치우기', 'Put the tutorial aside'),
    ('튜토리얼 이어 하기', 'Resume tutorial'),
    ('{0} 단계에서 바뀐 것이 있어 거기서부터 이어 갑니다', 'Something changed in {0}, so the tutorial picks up there'),
    # 퀘스트
    ('왼쪽 사이드바 — 구성 · 측정 요약과 [도움말] · [튜토리얼 끝내기] · [프로젝트 저장].',
     'Left sidebar — setup and measurement summary, [Help], [End tutorial], [Save project].'),
    ('REW 연결 카드 — "API 연결됨" 이면 준비 끝.', 'REW connection card — "API connected" means ready.'),
    ('REW 연결을 [다시 확인] 해 볼까요?', 'Check the REW connection with [Retry].'),
    ('측정을 시작하기 전에 SECS 가 REW 를 조종할 수 있는지 확인합니다. 튜토리얼은 데모 REW 가 붙어 있어 '
     '바로 연결되고, 실제로는 REW 를 API 모드로 먼저 띄웁니다.',
     'Before measuring, check that SECS can drive REW. The tutorial uses a practice REW, so it connects at once; '
     'for real, start REW in API mode first.'),
    ('[다음 →] 으로 넘어가 볼까요?', 'Move on with [Next →].'),
    ('프리셋 줄 — 7.1.4 같은 구성을 한 번에 채웁니다. 오른쪽 서브 − / + 로 서브 수를 바꿉니다. 데모 방은 7.1.4 에 서브 3 입니다.',
     'Preset row — fills in a layout such as 7.1.4 in one click. Change the number of subs with − / + on the right. '
     'The demo room is 7.1.4 with 3 subs.'),
    ('DSP 프로파일 — 필터를 넣을 장치와 FIR 탭 · IIR 밴드 수.', 'DSP profile — the target device and its FIR taps and IIR bands.'),
    ('측정 출력은 REW 가 그 스피커로 측정 신호를 내보내는 오디오 장치 채널입니다. 실제 배선과 같아야 '
     '그 스피커에서 소리가 납니다.',
     'The measured output is the audio-interface channel REW plays the test signal on for that speaker. It must match '
     'the real wiring for the sound to come from that speaker.'),
    ('마이크 레벨이 너무 작으면 잡음에 묻히고 너무 크면 찌그러집니다. 채널마다 신호를 내 마이크 레벨이 '
     '파란 레퍼런스 구간(−30 ~ −15 dBFS)에 드는지 봅니다.',
     'Too low and the mic level drowns in noise; too high and it distorts. Each channel plays so you can see '
     'the level land in the blue reference band (−30 to −15 dBFS).'),
    ('배치 선택과 위치 그림 — 1번은 늘 청취 위치입니다.', 'Arrangement and position map — 1 is always the listening position.'),
    ('측정 옵션 — 타이밍 레퍼런스 · 레퍼런스 출력.', 'Measurement options — timing reference and reference output.'),
    ('튜토리얼 샘플은 청취 위치 1점만 있어 [단일] 이 골라져 있습니다. 실제로는 싱글 · 커플 · 패밀리처럼 여러 위치를 재는 것을 권합니다.',
     'The tutorial sample has the listening position only, so [Single] is already picked. For a real '
     'measurement, several positions such as Solo, Couple or Family are recommended.'),
    ('위치가 많을수록 한 자리에만 맞춘 과한 보정을 피합니다. MIMO 는 위치가 3곳 이상이어야 계산할 수 있습니다.',
     'More positions keep the correction from over-fitting one seat. MIMO needs at least 3 positions.'),
    ('위치 목록 — 지금 잴 위치는 옅은 색 줄이고, 다 재면 "완료" 로 바뀝니다.',
     'Position list — the position to measure is the lightly shaded row; it changes to "Done" when measured.'),
    ('그래프 — 방금 채운 채널 곡선(채널마다 한 줄). 위치를 여럿 재면 굵은 선이 청취 위치입니다.',
     'Graph — the channel curves you just filled in (one per channel). With several positions, the thick line is '
     'the listening position.'),
    ('튜토리얼에서는 [데모 데이터] 로 위치 1 을 채워 볼까요?',
     'In the tutorial, fill position 1 with [Demo data].'),
    ('REW 로 따로 잰 측정이 있으면 [mdat 불러오기] — 채널마다 이름을 L · R · SW1 …(위치 2부터 L_P2 처럼) 지어 '
     '저장한 .mdat 를 가져옵니다. 자세한 건 F1 「mdat 불러오기」.',
     'If you measured in REW yourself, use [Load mdat] — name each measurement L, R, SW1 … (from position 2 like L_P2) '
     'and load the saved .mdat. More in F1 "Load mdat".'),
    ('[계산] 을 눌러 볼까요?', 'Press [Calculate].'),
    ('XO 탭에서 메인과 서브가 이어지는 모양을 볼까요?', 'Open the XO tab to see how mains and subs join.'),
    ('XO 근처에서 메인과 서브 소리가 매끈하게 합쳐져야 저역이 비거나 부풀지 않습니다. 이 탭에서 채널 그룹마다 '
     'XO 와 XO PEQ 를 고릅니다.',
     'Mains and subs must add up smoothly around the XO, or the bass dips or bulges there. This tab sets the XO '
     'and XO PEQ per channel group.'),
    ('서브 정렬 탭도 열어 볼까요?', 'Open the sub alignment tab too.'),
    ('서브가 여럿이면 자리마다 저역이 서로 더해지거나 지워집니다. 엔진이 서브마다 레벨 · 딜레이 · 극성을 찾아 '
     '여러 위치에서 고르게 맞춥니다.',
     'With several subs the bass adds or cancels differently at each seat. The engine finds level, delay and polarity '
     'per sub to even it out across positions.'),
    ('[인프라 서브] 를 SW3 으로 골라 볼까요? 데모 방의 SW3 은 초저역만 맡는 인프라 서브입니다.',
     'Set [Infra sub] to SW3. In the demo room SW3 is an infra sub that plays only the deepest bass.'),
    ('아주 낮은 대역만 맡는 서브(인프라)를 따로 두면 그 서브는 초저역에, 나머지는 그 위에 힘을 씁니다. '
     '인프라에는 LP, 나머지 서브에는 같은 HP 가 자동으로 들어가고 인프라 대역 레벨을 위 대역에 맞춥니다.',
     'A sub that plays only the deepest band (infra) lets it focus there while the others work above it. '
     'The infra gets an LP and the other subs the same HP; the infra band level is matched to the band above.'),
    ('REW mdat — 채널마다 _1 보정 전 → _2 보정 후 → _3 + MIMO 로 이름 붙인 측정을 씁니다. _2 · _3 은 계산한 예측입니다.',
     'REW mdat — writes measurements named per channel _1 before → _2 after → _3 + MIMO. _2 and _3 are calculated '
     'predictions.'),
    ('REW 연결을 다시 확인했습니다 — API 연결됨 ✓', 'Checked the REW connection again — API connected ✓'),
    # 2차 시험 고침 — 카드 규칙 줄 · 건너뛰기 알림 · 진행 중 문구
    ('직접 해 보세요 — [계속 →] 을 두 번 누르면 예시로 채우고 넘어갑니다.',
     'Try it yourself — or press [Continue →] twice to fill in an example and move on.'),
    ('한 번 더 누르면 예시로 채우고 넘어갑니다.', 'Press once more to fill in an example and move on.'),
    ('진행 중…', 'Working…'),
    ('튜토리얼 · 저장 안 됨', 'Tutorial · not saved'),
    ('연결 확인을 건너뛰었습니다.', 'Skipped the connection check.'),
    ('SW3 의 측정 출력을 6 으로 고쳐 두었습니다.', "Set SW3's measurement output to 6."),
    ('채널마다 기본 레벨을 채워 두었습니다.', 'Filled in a default level for every channel.'),
    ('채널을 하나씩 재생하는 중입니다 — 다 돌면 저절로 다음으로 넘어갑니다.',
     'Playing the channels one by one — the tutorial moves on by itself when done.'),
    ('계산을 시작해 두었습니다 — 뒤에서 이어 돕니다.', 'Started the calculation — it keeps running in the background.'),
    ('XO 탭을 열어 두었습니다. 곡선은 계산이 끝나야 보입니다.',
     'Opened the XO tab. The curves appear when the calculation is done.'),
    ('서브 정렬 탭을 열어 두었습니다.', 'Opened the sub alignment tab.'),
    ('마지막 서브를 인프라로 정해 두었습니다 — 25 Hz LR24.',
     'Set the last sub as the infra — 25 Hz LR24.'),
    # 용어 풀이(tutorial_quests.GLOSSARY)
    ('REW — 측정 프로그램(Room EQ Wizard). SECS 가 REW 를 시켜 측정 신호를 냅니다.',
     'REW — the measuring program (Room EQ Wizard). SECS has REW play the test signals.'),
    ('API 모드 — SECS 가 REW 를 원격으로 조종하는 연결. REW 를 켤 때 API 모드로 켭니다.',
     'API mode — the link that lets SECS drive REW. Start REW in API mode.'),
    ('캘리브레이션 — 마이크마다 조금씩 다른 특성을 지워 주는 보정 파일.',
     'Calibration — a file that removes each microphone\'s own small differences.'),
    ('스윕 — 낮은 음에서 높은 음으로 훑어 올라가는 측정 신호.', 'Sweep — a test tone gliding from low to high.'),
    ('극성 — 스피커 콘이 미는지 당기는지(+/−). 서로 반대면 낮은 소리가 지워집니다.',
     'Polarity — whether the cone pushes or pulls (+/−). Opposite polarity cancels low sounds.'),
    ('DSP — 보정 필터를 실제로 거는 장치(카밀라 · Tesira · miniDSP 등).',
     'DSP — the device that runs the correction filters (CamillaDSP, Tesira, miniDSP …).'),
    ('FIR — 긴 보정 필터. 소리 크기와 도착 시간 어긋남을 함께 고칩니다. 길수록(탭이 많을수록) 낮은 소리까지 정밀합니다.',
     'FIR — a long correction filter that fixes loudness and arrival-time mismatch together. Longer (more taps) '
     'reaches lower notes.'),
    ('IIR — 짧은 EQ 필터. 보통 이퀄라이저처럼 밴드 단위로 셉니다.',
     'IIR — short EQ filters, counted in bands like an ordinary equaliser.'),
    ('dBFS — 디지털 최대(0)를 기준으로 한 레벨. −20 dBFS 는 최대보다 20 dB 작습니다.',
     'dBFS — level relative to digital full scale (0). −20 dBFS is 20 dB below the maximum.'),
    ('타이밍 레퍼런스 — 채널마다 소리가 도착하는 시간을 같은 시계로 재려고 함께 내는 기준 신호.',
     'Timing reference — a reference signal played along so every channel\'s arrival time is measured on the same clock.'),
    ('MIMO — 낮은 소리가 방에서 오래 울리는 것을 여러 스피커가 함께 빨리 멈추게 하는 보정.',
     'MIMO — several speakers working together so low sounds stop ringing in the room sooner.'),
    ('mdat — REW 측정 여러 개를 묶은 파일.', 'mdat — a REW file holding several measurements.'),
    ('RC — 룸 보정. 방 때문에 생긴 음 높이별 크기 굴곡(봉우리 · 골)을 고칩니다.',
     'RC — room correction: evens out the peaks and dips the room adds at different pitches.'),
    ('SPL — 소리 크기를 음의 높낮이(주파수)별로 본 그래프. 마이크로 잰 상대 크기라 값이 음수일 수 있습니다.',
     'SPL — loudness plotted against pitch (frequency). It is a relative mic level, so values can be negative.'),
    ('타깃 — 보정 뒤에 목표로 삼는 곡선.', 'Target — the curve the correction aims for.'),
    ('GD — 음 높이별로 소리가 늦게 도착하는 정도. 낮은 소리가 방에서 오래 울리면 커집니다.',
     'Group delay — how late each pitch arrives. It grows when low sounds keep ringing in the room.'),
    ('XO — 크로스오버. 메인과 서브가 소리를 나눠 맡는 주파수.', 'XO — crossover: the frequency where mains hand over to subs.'),
    ('PEQ — 특정 높이의 소리만 키우거나 줄이는 EQ 밴드(주파수 · 게인 · Q 로 정함).',
     'PEQ — an EQ band that raises or lowers one pitch range (set by frequency, gain and Q).'),
    ('부스트 상한 — 보정이 소리를 키울 수 있는 최대치(스피커 · 앰프 보호).',
     'Boost limit — the most the correction may raise the level (protects speakers and amps).'),
    ('프리셋 — 자주 쓰는 스피커 구성(7.1.4 등)을 한 번에 채우는 버튼.',
     'Preset — fills in a common speaker layout (7.1.4 and so on) in one click.'),
    ('LP · HP — 저역 통과(높은 음을 깎음) · 고역 통과(낮은 음을 깎음) 필터.',
     'LP · HP — low-pass (cuts highs) and high-pass (cuts lows) filters.'),
    # 영어 화면 다듬기(10-10 AI 시험 t06)
    ('{0} / 1 위치', '{0} / 1 position'),
    ('{0}\n1 위치', '{0}\n1 position'),
    ('위치 {0} / {1} · 모두 완료', 'Position {0} / {1} · all done'),
    (' 곳', ' slots'),
    # 현장 말로 다듬은 문구(10-10 저녁 사용자: AI 티 나는 말 · 기준 → 레퍼런스 · 군지연 → GD · REW Pro 안내)
    ('측정 전에 SECS 가 REW 를 원격 제어할 수 있는지 확인합니다. 튜토리얼은 데모 REW 라 바로 연결되고, 실제로는 REW Pro 를 API 모드로 먼저 띄웁니다.',
     'Checks that SECS can remote-control REW before measuring. The tutorial uses a demo REW, so it connects at once; for real, start REW Pro in API mode first.'),
    ('위치가 많을수록 한 점에만 맞춘 과보정을 피합니다. MIMO 는 위치가 3곳 이상이어야 계산할 수 있습니다.',
     'More positions avoid over-correcting for one point. MIMO needs at least 3 positions.'),
    ('XO 부근에서 메인과 서브가 위상까지 맞게 합쳐져야 상쇄로 인한 딥이나 험프가 생기지 않습니다. 이 탭에서 채널 그룹마다 XO 와 XO PEQ 를 고릅니다.',
     'Mains and subs must sum in phase around the XO, or cancellation leaves a dip or a hump. This tab '
     'sets the XO and XO PEQ per channel group.'),
    ('서브가 여럿이면 자리마다 서로 보강되거나 상쇄됩니다. 서브 정렬은 서브마다 값을 찾아 서브 합이 고르고, '
     '서로 지우지 않고 크게 더해지게 맞춥니다. 그래서 이 탭은 서브가 둘 이상일 때만 보입니다.',
     'Several subs reinforce or cancel each other differently at each seat. Sub alignment finds values for each sub '
     'so the sub sum is even and the subs add up instead of cancelling — which is why this tab appears only with two '
     'or more subs.'),
    ('인프라 서브는 초저역만 맡고 나머지 서브는 그 위 대역을 맡습니다. 인프라에는 LP, 나머지 서브에는 같은 HP 가 자동으로 걸리고 인프라 대역 레벨을 위 대역에 맞춥니다.',
     'The infra sub covers only the deepest band and the other subs the band above. The infra gets an LP and the other subs the same HP; the infra band level is matched to the band above.'),
    ('REW — Room EQ Wizard 측정 프로그램. SECS 가 REW 로 스윕을 냅니다.',
     'REW — Room EQ Wizard, the measurement program. SECS plays the sweeps through REW.'),
    ('API 모드 — SECS 가 REW 를 원격 제어하는 연결. REW Pro 가 필요하고 REW 를 -api 로 실행합니다.',
     'API mode — the link SECS uses to remote-control REW. Needs REW Pro; start REW with -api.'),
    ('캘리브레이션 — 측정 마이크 보정 파일, .cal.',
     "Mic calibration file — the measurement mic's correction file (.cal)."),
    ('FIR — 크기와 위상을 함께 보정하는 필터. 탭이 많을수록 저역 분해능이 좋아집니다.',
     'FIR — a filter that corrects magnitude and phase together. More taps give finer low-frequency resolution.'),
    ('dBFS — 디지털 풀스케일 0 dBFS 에 대한 레벨. −20 dBFS 는 풀스케일보다 20 dB 낮습니다.',
     'dBFS — level relative to digital full scale, 0 dBFS. −20 dBFS is 20 dB below full scale.'),
    ('타이밍 레퍼런스 — 채널 간 도착 시간을 같은 시계로 재려고 스윕과 함께 내는 레퍼런스 신호.',
     'Timing reference — a reference signal played with each sweep so arrival times are measured on the same clock.'),
    ('MIMO — Multi-Input Multi-Output, 다입력 다출력 저역 감쇠. 여러 스피커로 저역 룸 모드의 링잉 · 디케이를 줄이는 보정입니다.',
     'MIMO — multi-input multi-output bass decay control: uses several speakers to shorten low-frequency room-mode ringing and decay.'),
    ('mdat — REW 측정 묶음 파일.',
     'mdat — a REW file holding several measurements.'),
    ('RC — Room Correction, 룸 보정. 방 때문에 생긴 주파수 응답의 피크 · 딥을 잡습니다.',
     'RC — room correction: tames the peaks and dips the room adds to the frequency response.'),
    ('타깃 — 보정 목표 커브.',
     'Target — the target curve the correction aims for.'),
    ('GD — Group Delay, 군지연. 주파수별 도착 지연으로, 룸 모드 링잉이 길면 커집니다.',
     'GD — group delay: arrival delay per frequency; it grows with long room-mode ringing.'),
    ('XO — 크로스오버. 메인과 서브가 대역을 나누는 주파수.',
     'XO — crossover: the frequency where mains and subs split the band.'),
    ('PEQ — 주파수 · 게인 · Q 로 정하는 파라메트릭 EQ 밴드.',
     'PEQ — a parametric EQ band set by frequency, gain and Q.'),
    ('부스트 상한 — 보정이 올릴 수 있는 최대 레벨. 헤드룸과 드라이버를 지킵니다.',
     'Boost limit — the most the correction may boost; protects headroom and drivers.'),
    ('프리셋 — 7.1.4 같은 채널 구성 프리셋.',
     'Preset — a channel-layout preset such as 7.1.4.'),
    ('LP · HP — 로우패스는 고역을, 하이패스는 저역을 컷하는 필터.',
     'LP · HP — low-pass cuts the highs, high-pass cuts the lows.'),
    ('REW Pro 가 없으면(대부분 그렇습니다) 5 측정 화면의 [mdat 불러오기] 부터 시작하면 됩니다. 앞 단계의 장치 연결 · 레벨 순차 재생과 5단계 자동 측정은 REW Pro(API) 사용자용 기능입니다.',
     'Without REW Pro (most users), start from [Load mdat] on the 5 Measure screen. Device connection and level playback in the earlier steps, and automatic measurement in step 5, are REW Pro (API) features.'),
    ('● 튜토리얼 — 마이크 레벨은 실제 REW 에서만 보입니다', '● Tutorial — mic level shows with a real REW only'),
    # 10-10 3차 시험 고침 · 사용자 결정 1A(업무 말) · 3A(카드 버튼 '계속 →')
    ('계속 →', 'Continue →'),
    ('단계 {0}/{1} 끝 · {2}', 'Step {0}/{1} done · {2}'),
    ('위 진행바 — 지금 단계는 색이 채워져 보이고, 단계 이름을 눌러 언제든 오갈 수 있습니다. 사이드바 · 진행바가 자주색이고 '
     '버튼이 분홍이면 튜토리얼 중입니다 — 소리는 나지 않고 측정 파일은 임시 폴더에만 씁니다. 점선 틀은 보기만 하는 곳, '
     '분홍 실선 틀은 직접 눌러 볼 곳입니다.',
     'Step bar — the current step is filled in; click a step name to move any time. A plum sidebar and step bar with '
     'pink buttons means tutorial mode — nothing plays and measurement files go to a temp folder only. A dashed '
     'outline is for looking; a solid pink outline is for you to click.'),
    ('REW 연결 카드 — "API 연결됨" 이면 준비 끝. REW Pro 가 없으면 5 측정 화면의 [mdat 불러오기] 부터 시작합니다. 장치 연결 · '
     '레벨 순차 재생 · 자동 측정은 REW Pro 의 API 를 쓰는 기능입니다.',
     'REW connection card — "API connected" means ready. Without REW Pro, start from [Load mdat] on the 5 Measure '
     'screen. Device connection, level playback and automatic measurement use the REW Pro API.'),
    ('마이크 · 오디오 카드 — 입력 장치 · 입력 채널 · 캘리브레이션 · 스윕 길이.',
     'Microphone · Audio panel — input device, input channel, mic calibration file, sweep length.'),
    ('출력 패치 시트 — 스피커마다 측정 출력 · 극성 · 크기.',
     'Output patch sheet — measurement output, polarity and size per speaker.'),
    ('SW3 의 측정 출력이 16 으로 잘못 들어가 있습니다. 표의 서브 그룹에서 SW3 의 "측정 출력" 을 6 으로 바꿔 볼까요?',
     'SW3\'s measurement output is wrongly set to 16. In the sub group of the table, change SW3\'s "Measurement output" to 6.'),
    ('측정 출력은 REW 가 그 스피커로 스윕을 내보내는 오디오 인터페이스 출력입니다. 실제 패치와 같아야 해당 스피커에서 스윕이 나옵니다.',
     "The measurement output is the audio interface output REW sends that speaker's sweep to. It must match the "
     "real patch for the sweep to come out of that speaker."),
    ('마스터 출력 — REW 출력 레벨과 [전 채널 자동 순차 재생].',
     'Master output — REW output level and [Play all in sequence].'),
    ('[전 채널 자동 순차 재생] 을 눌러 볼까요?', 'Press [Play all in sequence].'),
    ('입력 레벨이 낮으면 노이즈에 묻히고 너무 높으면 클립됩니다. 채널마다 핑크 노이즈를 틀어 마이크 입력이 파란 레퍼런스 구간 '
     '−30 ~ −15 dBFS 에 드는지 봅니다. 튜토리얼은 실제 마이크가 없어 마이크 미터는 비어 있고, 채널 값은 예시로 채워집니다.',
     'Too low and the input is buried in noise; too high and it clips. Pink noise plays on each channel so you can '
     'check the mic input lands in the blue reference band, −30 to −15 dBFS. The tutorial has no real mic, so the '
     'mic meter stays empty and the channel values are filled in as examples.'),
    ('보정은 측정한 응답으로 계산합니다. 실제로는 [이 위치 측정] 으로 채널마다 스윕하고, 데모 데이터는 7.1.4 · 서브 3 '
     '데모 방의 청취 위치 1점을 실제로 잰 것입니다.',
     'The correction is calculated from measured responses. For real, [Measure this position] sweeps every '
     'channel; the demo data is a real measurement of a 7.1.4 · 3 subs demo room at the listening position.'),
    ('데모 방 위치 1 의 측정을 불러왔습니다.', 'Loaded the demo room measurement, position 1.'),
    ('그래프 — 채널별 주파수 응답. 위치를 여럿 재면 굵은 선이 청취 위치입니다.',
     'Graph — frequency response per channel. With several positions, the bold line is the listening position.'),
    ('REW Pro 가 없으면 여기서 시작합니다 — REW 에서 채널마다 측정해 이름을 L · R · SW1 … 로 붙여 저장한 .mdat 를 '
     '[mdat 불러오기] 로 가져옵니다. 위치 2부터는 L_P2 처럼 붙이고, 자세한 건 [도움말] 의 "mdat 불러오기" 탭에 있습니다.',
     'Without REW Pro, start here — measure each channel in REW, name the measurements L, R, SW1 …, save the .mdat '
     'and bring it in with [Load mdat]. From position 2 add a suffix like L_P2; more in the "Load mdat" tab of [Help].'),
    ('보정 모드와 [계산] — 측정으로 보정 필터를 만듭니다. 모드는 보정 필터가 더할 지연의 한도로, 최소 지연은 크기만 '
     '고치고 저지연 · 표준은 지연을 더 써서 저역 위상까지 고칩니다.',
     'Correction mode and [Calculate] — turns the measurements into correction filters. The mode caps the delay the '
     'filters may add: Minimum latency corrects magnitude only, while Low latency and Standard spend more delay to '
     'correct the bass phase too.'),
    ('탭 SPL · GD · XO 와 그래프 — 계산 전에는 SPL 탭에 측정과 타깃만 보이고, 계산하면 보정 후 곡선과 GD · XO 곡선이 나옵니다.',
     'Tabs SPL, GD, XO and the graph — before calculating the SPL tab shows only the measurement and target; '
     'after it you also get the corrected curve and the GD and XO curves.'),
    ('오른쪽 카드는 탭에 맞춰 바뀝니다 — SPL · GD 탭에서는 RC 범위 카드, XO 탭에서는 XO 카드, 서브 정렬 탭에서는 '
     '서브 정렬 설정 카드가 나옵니다. 탭을 한 번씩 넘겨 보여 드립니다.',
     'The card on the right follows the tab — the SPL and GD tabs show the RC range card, the XO tab the XO card, '
     'and the sub alignment tab the sub alignment setup card. Watch the tabs step through once.'),
    ('측정만으로는 소리가 바뀌지 않습니다 — 계산해야 DSP 에 넣을 필터가 나옵니다. 룸 보정 RC 와 서브 정렬을 계산하고, 진행은 '
     '[계산] 아래 줄과 오른쪽 아래 알림에 나옵니다.',
     'Measuring alone changes nothing — calculating makes the filters for the DSP. It works out RC room correction '
     'and sub alignment; progress shows on the line under [Calculate] and in the notice at the bottom right.'),
    ('계산 중입니다 — 진행은 [계산] 아래 줄과 오른쪽 아래 알림에 나오고, 끝나면 저절로 다음으로 넘어갑니다.',
     'Calculating — progress shows on the line under [Calculate] and in the notice at the bottom right; the '
     'tutorial moves on by itself when done.'),
    ('결과 줄 — "±1 dB 안" 은 20–200 Hz 가 타깃 ±1 dB 안에 든 비율로, 보정 전 → 후로 커질수록 좋습니다. "GD 폭" 은 '
     '20–100 Hz 에서 GD 가 퍼진 폭이라 작을수록 저역이 고르게 도착합니다. 다음 줄은 이 채널의 PEQ · FIR 필터 수, '
     '"전체 지연" 은 보정 필터가 더하는 지연입니다.',
     'Result lines — "Within ±1 dB" is the share of 20–200 Hz within ±1 dB of the target; the more it grows from '
     'before → after, the better. "GD spread" is how widely the group delay spreads over 20–100 Hz, so smaller means '
     'the bass arrives more evenly. The next line is the PEQ and FIR filters for this channel, and "Total latency" '
     'is the delay the correction filters add.'),
    ('CamillaDSP — 구성 화면 DSP 프로파일의 기본값입니다. 보정 모드 이름을 붙인 .yml 설정 파일 하나와, 채널마다 48 kHz WAV '
     '파일 하나씩을 담은 FIR 폴더를 씁니다. 튜토리얼에서는 내보내기 파일을 쓰지 않습니다.',
     'CamillaDSP — the default DSP profile on the Setup screen. Writes one .yml settings file named after the '
     'correction mode and an FIR folder with one 48 kHz WAV file per channel. The tutorial writes no export files.'),
    ('스윕 — 낮은 주파수에서 높은 주파수로 훑어 올라가는 측정 신호(로그 사인).',
     'Sweep — a test signal gliding from low to high frequency (log sine).'),
    ('극성 — 스피커가 미는지 당기는지(+/−). 두 스피커가 같은 대역을 낼 때 극성이 반대면 저역이 상쇄됩니다.',
     'Polarity — whether the speaker pushes or pulls (+/−). Where two speakers play the same band, opposite '
     'polarity cancels the low end.'),
    ('IIR — 파라메트릭 EQ(PEQ) 밴드 같은 짧은 필터. 밴드 단위로 셉니다.',
     'IIR — short filters such as parametric EQ (PEQ) bands, counted in bands.'),
    ('SPL — 주파수 응답 그래프. 보정 설계에서는 타깃 레벨을 0 dB 로 두고 그립니다.',
     'SPL — the frequency-response graph. In Room correction it is drawn with the target level at 0 dB.'),
    ('DSP — 보정 필터를 거는 프로세서. CamillaDSP · Tesira · miniDSP 등.',
     'DSP — the processor that runs the correction filters, such as CamillaDSP, Tesira or miniDSP.'),
    # 10-11 사용자: 보정 설계 · 서브 정렬을 탭마다 자세히
    ('SPL 탭 — 보정 전 · 보정 후 곡선과 타깃 점선을 겹쳐 봅니다. 회색으로 가린 바깥은 RC 가 손대지 않는 대역이고, '
     '그 안에서 보정 후 곡선이 타깃을 고르게 따라가면 잘된 보정입니다. 좁고 깊은 딥은 일부러 다 채우지 않습니다.',
     'SPL tab — the before and after curves over the dashed target. The grey-shaded outside is the band RC leaves '
     'alone; inside it, a good correction has the after curve following the target evenly. Narrow, deep dips are '
     'deliberately not filled all the way.'),
    ('GD 탭 — 주파수마다 소리가 늦게 도착하는 정도를 ms 로 봅니다. 저역에서 솟은 봉우리는 룸 모드가 오래 우는 자리이고, '
     'RC 범위 안에서 보정 후 봉우리가 낮아지고 곡선이 평평해질수록 좋습니다.',
     'GD tab — how late each frequency arrives, in ms. Peaks in the bass are room modes that keep ringing; within the '
     'RC range, the lower the peaks and the flatter the curve after correction, the better.'),
    ('XO 탭 — 고른 채널의 메인 · 서브와 둘의 합을 타깃과 겹쳐 봅니다. 합이 XO 둘레에서 움푹 꺼지거나 솟지 않고 타깃을 '
     '따라가면 좋은 XO 입니다. 세로선을 끌거나 XO 카드에서 주파수를 바꾸고, [XO 추천 계산] 은 후보마다 끝까지 계산해 '
     '그룹별로 가장 좋은 값을 고릅니다.',
     'XO tab — the chosen channel’s main, sub and their sum over the target. A good XO has the sum following the '
     'target with no dip or bump around the crossover. Drag the vertical line or change the frequency on the XO card; '
     '[Recommend XO] runs every candidate through and picks the best value per group.'),
    ('서브 정렬 그래프 — 색 하나가 서브 하나이고, 점선은 정렬 전 · 실선은 정렬 후입니다. 검은 굵은 선은 모든 서브를 '
     '더한 서브 합, 검은 점선은 정렬 전의 합, 갈색 점선은 타깃입니다. 정렬 후 합이 움푹 꺼진 곳 없이 타깃을 따라갈수록 좋습니다.',
     'Sub alignment graph — one color per sub; dashed is before alignment, solid is after. The thick black line is '
     'the sum of all subs, the black dashed line the sum before alignment, the brown dashed line the target. The '
     'closer the aligned sum follows the target, with no dips, the better.'),
    ('결과 줄과 값 표 — "서브 합 평탄도" 는 서브 합이 출렁이는 폭이라 정렬 전 → 후로 작아질수록 좋고, "좌석 편차" 는 '
     '측정 위치 사이의 차이입니다. 표는 서브마다 DSP 에 넣을 게인 · 딜레이 · 극성 · 올패스이고, 서브 이름 색이 그래프 '
     '곡선 색입니다.',
     'Result line and value table — "Sub sum flatness" is how much the sub sum ripples, so smaller from before → '
     'after is better; "Seat-to-seat spread" is the difference between measurement positions. The table holds the '
     'gain, delay, polarity and all-pass to enter in the DSP for each sub; each sub name has its curve color.'),
    ('서브 정렬이 노리는 것이 이 두 숫자입니다 — 청취 위치의 서브 합은 고르게, 자리 사이 차이는 작게. 튜토리얼 '
     '데모는 위치가 1곳이라 좌석 편차가 비어 있습니다.',
     'These two numbers are what sub alignment aims for — an even sub sum at the listening position and a small '
     'difference between seats. The tutorial demo has one position, so the seat spread is empty.'),
    ('서브 정렬 설정 — 서브를 하나 골라 방식을 정합니다. 자동은 엔진이 게인 · 딜레이 · 극성을 찾고, 고정은 아래 칸에 '
     '넣은 값을 그대로 DSP 에 넣습니다. Group 은 묶은 서브끼리의 상대값을 지키고 묶음 전체의 값만 엔진이 맞춥니다.',
     'Sub alignment setup — pick a sub and choose its mode. Auto lets the engine find gain, delay and polarity; '
     'Fixed puts the values in the fields below into the DSP as they are. Group keeps the relative values between '
     'grouped subs and lets the engine set only the group’s common values.'),
    ('DBA 처럼 배치로 이미 맞춘 서브는 고정이나 Group 으로 그 값을 지키고, 나머지는 자동에 맡깁니다. '
     '[결과 값 가져오기] 는 마지막 계산 값을 칸에 옮겨 고정의 시작점으로 씁니다.',
     'Subs already matched by their layout, such as a DBA, keep their values with Fixed or Group; leave the rest on '
     'Auto. [Take result values] copies the last calculated values into the fields as a starting point for Fixed.'),
    ('고정 필터 — HP · LP · PEQ 는 이 서브 출력에 늘 걸리는 필터입니다. 인프라를 고르면 인프라 서브에는 LP, 나머지 '
     '서브에는 HP 가 인프라 XO 주파수로 저절로 들어갑니다. 방식과 상관없이 엔진은 이 필터를 건 응답으로 정렬을 찾습니다.',
     'Fixed filters — HP, LP and PEQ always sit on this sub’s output. Picking an infra sub fills in an LP on the infra '
     'and an HP on the other subs at the infra XO frequency. Whatever the mode, the engine aligns the subs with these '
     'filters in place.'),
    ('설정이 바뀌었습니다. [서브만 계산] 으로 이 탭을 다시 계산해 볼까요?',
     'The settings changed. Recalculate this tab with [Subs only].'),
    ('설정을 바꾸면 결과가 낡습니다. [서브만 계산] 은 서브 정렬만 실제로 다시 풀어 이 탭의 그래프와 표를 바꾸고, '
     '몇 초 걸립니다.',
     'Changing a setting makes the result out of date. [Subs only] really re-solves just the sub alignment and '
     'redraws this tab’s graph and table; it takes a few seconds.'),
    ('[서브만 계산] 뒤의 결과 — 인프라 서브는 인프라 XO 아래만, 나머지 서브는 그 위만 맡아 서브 합이 둘을 이어 붙인 '
     '모양이 되고, 값 표도 새로 찾은 값으로 바뀝니다. 이 결과는 서브 정렬 탭에만 보이니, SPL · XO 탭과 내보내기에 '
     '쓰려면 [계산] 을 다시 누릅니다.',
     'After [Subs only] — the infra sub plays only below the infra XO and the other subs only above it, so the sub sum '
     'joins the two, and the value table shows the newly found values. This result shows only on the Sub alignment '
     'tab; to use it in the SPL and XO tabs and the export, press [Calculate] again.'),
    ('[서브만 계산] 은 건너뛰었습니다.', 'Skipped [Subs only].'),
    ('서브 정렬을 다시 계산하는 중입니다 — 끝나면 그래프와 표가 바뀌고 저절로 다음으로 넘어갑니다.',
     'Recalculating the sub alignment — the graph and table change when it is done, and the tutorial moves on by '
     'itself.'),
    ('서브 정렬을 다시 계산했습니다 — 그래프와 표가 새 결과입니다 ✓',
     'Sub alignment recalculated — the graph and table show the new result ✓'),
    ('게인 — 레벨을 올리거나 내리는 값, dB.', 'Gain — how much the level is raised or lowered, in dB.'),
    ('딜레이 — 신호를 일부러 늦추는 값, ms. 서브마다 도착 시각을 맞춥니다.',
     'Delay — holding a signal back on purpose, in ms, to line up each sub’s arrival time.'),
    ('올패스 — 크기는 그대로 두고 한 주파수 둘레의 위상만 돌리는 필터.',
     'All-pass — a filter that leaves the level alone and only turns the phase around one frequency.'),
    ('DBA — Double Bass Array. 앞 벽 서브와 뒷벽 서브를 반대 극성으로 늦춰 틀어 저역 반사를 지우는 배치.',
     'DBA — Double Bass Array: front-wall subs plus rear-wall subs played inverted and delayed to cancel the bass '
     'reflection.'),
)
