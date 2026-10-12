# -*- coding: utf-8 -*-
"""위저드 영어 번역표 G1(1~4단계·창·배치도·그래프). (한국어 원문, 영어) 쌍."""
PAIRS: tuple[tuple[str, str], ...] = (
    # ──────────────── step2_config.py ────────────────
    # 그룹 이름(GROUP_ORDER · role_group 표시 이름)
    ('프론트', 'Front'),
    ('센터', 'Center'),
    ('서브', 'Sub'),
    ('서라운드', 'Surround'),
    ('백', 'Back'),
    ('와이드', 'Wide'),
    ('탑', 'Top'),               # 10-10: 천장 탑(T*)과 벽 위쪽 하이트(H*)를 나눔
    ('하이트', 'Height'),
    ('기타', 'Other'),
    ('빈 구성', 'Empty'),
    ('직접 추가', 'Manual'),
    ('{0} 스피커', '{0} spk'),
    ('서브 하나 줄이기', 'Remove one sub'),
    ('서브 하나 늘리기', 'Add one sub'),
    ('메인 스피커는 바이앰프(LF/HF)로 시작', 'Mains start as bi-amp'),
    ('앰프 구성', 'Amp'),
    ('대역', 'Band'),
    ('측정 출력', 'Meas. output'),
    ('극성', 'Polarity'),
    ('측정 오디오 장치', 'Measurement audio device'),
    ('장치', 'Device'),
    ('채널당 FIR 탭', 'FIR taps / ch'),     # 값 칸에 단위·지연을 넣어 짧게(09-24)
    ('총 FIR 탭 예산', 'FIR tap budget'),
    ('채널당 IIR 밴드', 'IIR bands / ch'),
    ('장치 목록을 읽을 수 없음', 'Cannot read device list'),
    ('채널 수 미확인', 'Channel count unknown'),
    ('채널 {0}개 초과', 'Exceeds {0} channels'),
    ('채널 {0}개 · REW /audio 로 전송', '{0} ch · sent to REW'),
    ('  {0} 그룹 · {1}개', '  {0} group · {1}'),
    ('사용자 정의', 'Custom'),
    ('1채널', '1 ch'),
    ('소형 · XO 적용', 'Small · XO'),
    ('대형 · 풀레인지', 'Large · full'),
    ('싱글앰프', 'Single'),
    ('바이앰프', 'Bi-amp'),
    ('트라이앰프', 'Tri-amp'),
    ('스피커 제거', 'Remove speaker'),
    ('{0} (범위 밖)', '{0} (out of range)'),
    ('중복', 'Duplicate'),
    ('정상', 'Normal'),       # 극성 Normal / Inverted(10-10 3차 시험 — 옛 1.41 'Normal'(일반) 은 슬림화로 없어졌다)
    ('반전', 'Inverted'),
    ('극성 반전', 'Invert polarity'),
    ('{0} 측정 출력 · 최대 {1}', '{0} measurement outputs · highest {1}'),
    ('출력 중복 {0}건', '{0} duplicate outputs'),
    ('출력 중복 없음', 'No duplicate outputs'),
    ('서브 별도 출력', 'Subs on separate outputs'),
    ('서브 없음', 'No sub'),
    ('구성 오류 ', 'Configuration error '),
    (' 외 {0}건', ' and {0} more'),
    ('배치도나 표에서 스피커를 고르면 여기에 연결 요약이 나옵니다.',
     'Pick a speaker in the layout or the table to see its wiring summary here.'),
    ('메인', 'Main'),
    ('{0} → {1} → DSP 입력 {2} → 앰프', '{0} → {1} → DSP input {2} → amp'),
    # 551: 위 문구 + '  [반전]' 이 한 문자열로 setText 된다 — 합친 꼴도 등록.
    ('  [반전]', '  [inverted]'),
    ('대역마다 필터를 따로 계산합니다.', 'A separate filter is computed per band.'),
    ('서브는 LFE와 소형 스피커의 저역을 합산해 받습니다.',
     'The sub receives LFE plus the summed bass of the small speakers.'),
    ('풀레인지 한 출력으로 구동합니다.', 'Driven full-range from a single output.'),
    ('패치 시트 CSV 저장', 'Save patch sheet CSV'),
    ('스피커를 한 대 이상 추가하세요.', 'Add at least one speaker.'),

    # ──────────────── step1_devices.py ────────────────
    ('음향 (레퍼런스 스피커)', 'Acoustic (ref. speaker)'),
    ('루프백 (케이블)', 'Loopback (cable)'),
    ('없음', 'None'),
    ('연결 후 확인', 'Checked after connecting'),
    ('실행 파일', 'Executable'),
    ('버전', 'Version'),
    ('Pro 라이선스', 'Pro license'),
    ('blocking 모드', 'Blocking mode'),
    ('레퍼런스 출력 없음', 'No reference output'),
    ('입력 장치', 'Input device'),
    ('입력 채널', 'Input channel'),
    ('캘리브레이션', 'Mic calibration file'),      # 10-10 결정 2A — 6단계 'Room correction' 과 가른다
    ('출력 드라이버', 'Output driver'),
    ('샘플레이트', 'Sample rate'),
    ('스윕 길이', 'Sweep length'),
    ('타이밍 레퍼런스', 'Timing ref.'),
    ('레퍼런스 출력', 'Reference output'),
    ('API 연결됨 · 127.0.0.1:4735', 'API connected · 127.0.0.1:4735'),
    ('확인됨 · 자동 측정 가능', 'Confirmed · auto measurement'),
    ('Pro 아님 · 수동 측정', 'Not Pro · manual measurement'),
    ('REW 실행 중…', 'Starting REW…'),
    ('REW 가 이미 켜져 있지만 API 모드가 아닙니다. REW 를 닫은 뒤 이 버튼을 다시 누르면 -api 옵션으로 띄웁니다.',
     'REW is already running but not in API mode. Close REW, then press this button again to start it with -api.'),
    ('이 실행 모드에서는 REW 를 띄울 수 없습니다. REW 를 -api 옵션으로 직접 실행한 뒤 [다시 확인] 을 누르세요.',
     'REW cannot be started from this mode. Start REW with the -api option yourself, then press [Retry].'),
    ('REW 실행 파일을 찾지 못했습니다(C:\\Program Files\\REW). REW 를 -api 옵션으로 직접 실행한 뒤 [다시 확인] 을 누르세요.',
     'REW executable not found (C:\\Program Files\\REW). Start REW with the -api option yourself, then press [Retry].'),
    ('REW 를 API 모드로 띄우는 중입니다. 창이 뜨고 응답할 때까지 10~20 초 걸립니다.',
     'Starting REW in API mode. It takes 10–20 s until the window is up and responding.'),
    ('REW 가 시간 안에 응답하지 않았습니다. REW 창이 떴으면 [다시 확인] 을 누르세요.',
     'REW did not respond in time. If the REW window is up, press [Retry].'),
    ('REW 가 켜져 있지만 API 모드가 아닙니다. REW 를 닫고 [REW API 모드로 실행] 을 누르세요.',
     'REW is running but not in API mode. Close REW and press [Launch REW in API mode].'),
    ('REW 가 꺼져 있습니다. [REW API 모드로 실행] 을 누르면 REW 를 -api 옵션으로 띄웁니다.',
     'REW is not running. [Launch REW in API mode] starts REW with the -api option.'),
    ('REW 연결 후 표시됨', 'Shown after REW connects'),
    ('REW 연결 후 표시됨 (1단계)', 'Shown after REW connects (step 1)'),
    ('{0} · 출력 {1}', '{0} · output {1}'),
    ('REW Pro 라이선스가 없어 자동 측정을 시작할 수 없습니다.',
     'No REW Pro license, so automatic measurement cannot start.'),

    # ──────────────── step3_level.py ────────────────
    ('레벨 (dBFS, 마이크 입력)', 'Level (dBFS, at mic input)'),
    ('레벨 (dB)', 'Level (dB)'),
    ('값', 'Value'),
    ('REW 출력 레벨 -20.0 dBFS', 'REW output level -20.0 dBFS'),
    ('REW 출력 레벨 {0} dBFS', 'REW output level {0} dBFS'),
    ('UMIK-1 · 100 % (고정)', 'UMIK-1 · 100 % (fixed)'),
    ('{0} 재생', 'Play {0}'),
    ('레벨을 아직 못 잰 채널: {0}', 'Level not measured yet: {0}'),

    # ──────────────── step4_layout.py ────────────────
    ('끄기', 'Off'),
    ('5 초', '5 s'),
    ('10 초', '10 s'),
    ('15 초', '15 s'),
    ('{0}\n{1} 위치', '{0}\n{1} positions'),
    ('측정 시작 지연', 'Start delay'),
    ('위치당 스윕', 'Sweeps / pos.'),
    ('예상 소요', 'Est. time'),
    ('루프백', 'Loopback'),
    ('{0} 채널 순차', '{0} ch in sequence'),
    ('{0} 분', '{0} min'),
    ('2단계에서 스피커를 먼저 구성하세요.', 'Configure the speakers in step 2 first.'),

    # ──────────────── layout_map.py ────────────────
    ('단일 · 청취 위치 1점', 'Single · listening position'),
    ('단일', 'Single'),
    ('청취 위치 1점', '1 position'),      # 4단계 배치 단추 아랫줄(1024 폭 75 px) — 'listening position' 은 안 든다
    ('싱글 · 1인 의자', 'Solo · single chair'),
    ('커플 · 2~3인 소파', 'Couple · 2–3 seat sofa'),
    ('패밀리 · 소파 두 줄', 'Family · sofa, two rows'),
    ('소파 두 줄', 'Sofa / 2 rows'),
    # step4:49 가 이름을 ' · ' 에서 줄바꿈해 버튼에 쓴다 — 줄바꿈 꼴과 줄 단위 조각.
    ('싱글', 'Solo'),
    ('패밀리', 'Family'),
    ('1인 의자', 'Single chair'),
    ('커플', 'Couple'),
    ('2~3인 소파', '2–3 seat sofa'),
    ('넓은 소파/두 줄', 'Sofa / 2 rows'),
    # 4단계 배치 카드의 'N 위치' — 숫자는 따로 강조해 그리고 단위만 옮긴다(2026-09-24).
    ('위치', 'positions'),
    ('우 35', 'Right 35'),
    ('좌 35', 'Left 35'),
    ('앞 35', 'Front 35'),
    ('뒤 30', 'Back 30'),
    ('앞우 · 위 20', 'Front-right · up 20'),
    ('앞좌 · 위 20', 'Front-left · up 20'),
    ('뒤우 · 아래 20', 'Back-right · down 20'),
    ('뒤좌 · 아래 20', 'Back-left · down 20'),
    ('우 70 (소파 끝)', 'Right 70 (sofa end)'),
    ('좌 70 (소파 끝)', 'Left 70 (sofa end)'),
    ('앞 70 · 위 20', 'Front 70 · up 20'),
    ('뒤 55 (소파 밖)', 'Back 55 (behind sofa)'),
    ('앞우 55', 'Front-right 55'),
    ('앞좌 55', 'Front-left 55'),
    ('뒤우 55 · 아래', 'Back-right 55 · down'),
    ('뒤좌 55 · 아래', 'Back-left 55 · down'),
    ('청취 위치 · 귀 높이 (레퍼런스)', 'Listening position (reference)'),
    ('스크린', 'Screen'),
    ('청취 위치', 'Listening position'),
    ('역할 미지정', 'No role assigned'),
    ('스크린 · 프론트 스피커 방향', 'Screen · toward front speakers'),
    ('지름 1 m', 'Ø 1 m'),
    (' · 완료', ' · done'),
    (' · 다음 측정', ' · next'),
    ('귀 높이', 'Ear height'),
    ('+20 cm 위', '+20 cm up'),
    ('−20 cm 아래', '−20 cm down'),

    # ──────────────── __init__.py ────────────────
    ('REW 에 연결되어 있지 않습니다', 'Not connected to REW'),

    # ──────────────── style.py (BAND_KO) ────────────────
    ('풀레인지', 'Full'),

    # ──────────────── window.py ────────────────
    ('장치 확인 건너뛰기', 'Skip device check'),
    ('레벨 건너뛰기 (기본값 적용)', 'Skip levels (use defaults)'),
    ('측정 건너뛰기 (mdat 불러오기)', 'Skip measurement (load mdat)'),
    ('프로젝트', 'Project'),
    ('장치 확인을 건너뛰었습니다. REW 연결은 실제 측정에서 확인됩니다.',
     'Device check skipped. The REW connection will be verified at the actual measurement.'),
    ('모든 채널을 레퍼런스 구간(-30~-15 dBFS)으로 간주합니다.',
     'All channels are assumed to be in the reference zone (-30 to -15 dBFS).'),
    ('{0} · 서브 {1} · 출력 {2}', '{0} · {1} sub · {2} out'),
    ('{0} / {1} 위치', '{0} / {1} positions'),
    ('API 연결 (4735)', 'API connected (4735)'),
    ('SECS 프로젝트 (*.secs)', 'SECS project (*.secs)'),

    # ──────────────── plot.py ────────────────
    ('보정 범위 {0} ~ {1} Hz', 'Correction range {0} ~ {1} Hz'),
    ('XO {0} Hz · {1} 그룹', 'XO {0} Hz · {1} group'),
    ('타겟 제어점 {0} Hz · {1}{2} dB', 'Target point {0} Hz · {1}{2} dB'),
)

# SKIPPED:
#   step2_config.py  '역할,이름,크기,대역,측정 출력,극성' — 패치 시트 CSV 헤더(파일 포맷, 화면에 안 나옴).
#   style.py         QSS 본문 — 한글은 CSS 주석뿐.
#   window.py        '단계 번호는 1~{0} 여야 합니다: {1}' — 개발자용 ValueError.
#   window.py        '측정.secs' — 저장 대화상자의 기본 파일 이름(파일명은 그대로 둔다).
#
# 09-23 정리: 아래는 09-22 i18n.py 개선(패턴 캡처값에 한글이 섞이면 ', '/'\n'/' · ' 기준으로
# 재귀 translate — i18n.py:79~102) 뒤 이미 저절로 맞게 된 항목들이라 지웠다. 확인한 것:
# 그룹 이름·XO 그룹·서브 요약·배치 이름(줄바꿈 꼴 포함)의 캡처값 재번역, [반전] 합친 꼴,
# step1_devices.py 의 힌트 조립(지금은 각 문구가 완결형 리터럴로 등록돼 있다).
#
# NOTES: (리드가 t() 를 직접 걸거나 런타임을 손봐야 하는 곳 — 아직 안 됨)
#   [조립 문구 — 접두사가 동적 목록 앞에 그대로 붙어 통짜라 분해 번역이 못 물린다]
#   step2_config.py  '구성 오류 ' + ' · '.join(messages[:4]) + f' 외 {n}건' — messages 는 model.py
#                     validate() 문구(다른 조)라 조각 자체는 등록돼 있지만, 접두사 '구성 오류 '가
#                     첫 메시지에 붙어 나가 앞부분이 한국어로 남는다. 접두사를 별도 위젯으로 떼거나
#                     조립 전에 조각마다 t() 를 걸 것.
#   [한글 리터럴이 없어 JSON 에 안 잡힌 조립 — 자리표시자뿐이라 패턴 미등록]
#   step2_config.py, step3_level.py   role_label(role) / f'{role} · {role_label(role)}' — role_label 은
#                     model.py(다른 조). 표시 전에 t(role_label(..)).
#   [위젯 setText 를 안 타는 표시 경로 — t() 직접]
#   layout_map.py    ARRANGEMENTS 이름·MicPoint.text 는 데이터 상수 — 5단계 위치 목록(QListWidgetItem, 다른 조)도 이 값을 쓴다.
#   step2_config.py  QTableWidget.setHorizontalHeaderLabels — 일반 QtWidgets 면 번역을 안 탄다(i18n.QTableWidget 필요).
#   step2_config.py  QFileDialog.getSaveFileName 의 제목 '패치 시트 CSV 저장'이 리터럴 그대로라 t() 를
#                     안 거친다(영어 모드에서도 한국어로 뜬다) — window.py 의 저장·불러오기 대화상자는
#                     제목은 t() 로 고쳐졌지만 필터 문자열('SECS 프로젝트 (*.secs)')은 아직 그대로다.
#   step2_config.py, step1_devices.py, step4_layout.py   QComboBox 항목 — i18n.QComboBox 면 자동. 일반 콤보에 t() 를 걸 경우
#                     step4_layout.py 가 currentText()(표시 문구)를 project.extra['start_delay'] 에 저장하므로
#                     영어 문구가 .secs 에 들어간다 → 원문(또는 itemData)을 저장하도록 바꿀 것.
#   __init__.py      RuntimeError 문구 — 5단계 로그/오류 표시로 나올 수 있어 번역을 넣었다(str(exc) 를 t() 로).
#   [과매칭 주의 — 리터럴이 짧은 패턴]
#   '{0} 분', '{0} 재생', '{0} 스피커', '{0} 채널 순차', '{0} (범위 밖)', '{0} · 출력 {1}', '{0}\n{1} 위치', '{0} / {1} 위치':
#                         fullmatch 라 끝 낱말이 같은 다른 문구(예: '… 기준 스피커')가 정확 일치로 등록돼 있지 않으면 물릴 수 있다.
#   [영어 키 충돌]
#   등록은 translations.add_korean(영어 키를 덮지 않음)으로 할 것 — add(en, ko) 를 쓰면 레거시 영어 원문이 덮인다:
#   '정상'→'Normal'(레거시 '일반'), '장치'→'Device', '없음'→'None', '버전'→'Version' 등 흔한 낱말이 많다.
