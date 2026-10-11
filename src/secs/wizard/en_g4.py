# -*- coding: utf-8 -*-
"""위저드 영어 번역표 G4(도움말 창 · 로딩 · 단계 화면 문구 일부). (한국어 원문, 영어) 쌍.

옛 기능 설명표 `help_text.HELP` 전용 쌍은 2026-10-10 슬림화에서 그 모듈과 함께 지웠다
(F1 도움말은 튜토리얼 `tutorial_text` + `en_g6`, 버튼·칸 뜻은 풍선 도움말 `tooltips` + `en_g5`).
"""
PAIRS: tuple[tuple[str, str], ...] = (
    # ──────────────── 창 ────────────────
    ('도움말', 'Help'),
    ('닫기', 'Close'),
    ('이 단계의 도움말이 아직 없습니다.', 'No help for this step yet.'),
    # 로딩 오버레이(wizard/loading.py, 09-23)
    ('mdat 를 불러오는 중…', 'Loading the mdat…'),
    ('화면을 준비하는 중…', 'Preparing the screen…'),
    # 5단계 위치 고르기(09-23)
    ('이 위치 다시 측정', 'Re-measure this position'),
    ('위치 {0} 다시 측정 — 기존 기록을 새 측정으로 바꿉니다.', 'Re-measuring position {0} — the new measurement replaces the old record.'),
    ('다시 측정', 'Measure again'),   # 문장형(10-10 3차 시험 — 제목 · 버튼 대소문자 통일)
    # 3단계 재생 중 · 4단계 예상 소요 · 5단계 시작 지연
    ('재생 중 · {0}', 'Playing · {0}'),
    ('레벨을 아직 못 잰 채널: {0} 외 {1}개', 'Level not measured yet: {0} and {1} more'),     # 10-10 3차 시험
    ('약 {0} 분 {1} 초', 'about {0} min {1} s'),
    ('약 {0} 초', 'about {0} s'),
    ('위치 {0}: 마이크를 {1} 번 위치로 옮기세요. {2} 초 뒤 시작합니다.', 'Position {0}: move the mic to position {1}. Starting in {2} s.'),
    ('{0} 초 뒤 시작…', 'Starting in {0} s…'),
    ('시작 전에 취소했습니다.', 'Cancelled before starting.'),
    # 6단계 그래프 우클릭 메뉴
    ('타깃 저장…', 'Save target…'),
    ('타깃 불러오기…', 'Load target…'),
    # 6단계 오른쪽 열 · 닫을 때 저장 확인
    # 8단계 시트 주석 · 6단계 경고(계약 개정 5·6·7)
    ('위상 올패스 {0}밴드', 'Phase all-pass {0} bands'),
    ('위상 올패스', 'Phase all-pass'),
    # 채널 FIR 에 든 PEQ·XO(리뷰 3, 09-23)
    # 설계안 개정 6(09-24): 채널별 서브 경로 지연(C1) · 서브 극성(C3) · 딥 메우기(C4)
    ('저장하지 않은 변경이 있습니다. 저장할까요?', 'There are unsaved changes. Save them?'),
    ('저장', 'Save'),
    ('저장 안 함', 'Don\'t save'),
    ('취소', 'Cancel'),
    # 작업 중 패널
    ('측정 중…', 'Measuring…'),
    # 09-24: 로딩 표시를 더 넣은 자리(샘플 채우기 오버레이, 8단계 파일 쓰기·Tesira 통신 구석 패널)
    ('샘플 데이터를 불러오는 중…', 'Loading the sample data…'),
    ('파일을 내보내는 중…', 'Writing the export files…'),
    ('경과', 'elapsed'),
    ('예상', 'estimate'),
    ('남음', 'left'),
    # 로딩 창
    ('시작하는 중…', 'Starting…'),
    ('계산 모듈을 읽는 중…', 'Loading the calculation modules…'),
    ('화면을 만드는 중…', 'Building the window…'),
    # ──────────────── 2단계(구성) ────────────────
    ('프리셋', 'Presets'),
    ('2.0 … 9.1.6', '2.0 … 9.1.6'),
    ('서브우퍼 − / +', 'Subwoofer − / +'),
    ('메인 스피커는 바이앰프(LF/HF)로 시작', 'Start main speakers bi-amped (LF/HF)'),
    ('배치도', 'Room map'),
    ('스피커 원', 'Speaker circle'),
    ('보라 사각형', 'Purple square'),
    ('DSP 프로파일 · 필터 자원', 'DSP profile · filter resources'),
    ('채널당 올패스 밴드', 'All-pass bands'),     # 1024 폭 칸 150 px(QA 09-23)
    ('베이스 매니지먼트 경로 지연', 'Bass-mgmt path delays'),     # 09-24 개정 6 C1
    ('베이스 매니지먼트\n경로 지연', 'BM path delays'),     # 2단계 DSP 카드 이름 칸(값 칸 폭 확보, 09-24)
    ('출력 패치 시트', 'Output patch sheet'),
    ('측정 오디오 장치', 'Measurement audio device'),
    ('측정 출력', 'Measurement output'),
    ('+ 스피커 추가', '+ Add speaker'),
    ('출력 번호 자동 정렬', 'Renumber outputs'),
    ('패치 시트 CSV', 'Patch sheet CSV'),
    # ──────────────── 1단계(장치) ────────────────
    ('REW 연결', 'REW connection'),
    ('REW API 모드로 실행', 'Launch REW in API mode'),
    ('다시 확인', 'Retry'),
    ('설정 읽기', 'Read settings'),
    ('Pro 라이선스', 'Pro license'),
    ('마이크 · 오디오', 'Microphone · Audio'),
    ('입력 장치 · 입력 채널', 'Input device · input channel'),
    ('출력 드라이버', 'Output driver'),
    ('스윕 길이', 'Sweep length'),
    ('타이밍 레퍼런스 — 음향', 'Timing reference — acoustic'),
    ('레퍼런스 출력', 'Reference output'),
    # 1단계(장치) REW 실측값(2026-09-23: 실행 파일·캘리브레이션·레벨을 목업 고정 문구 대신 실제로 읽는다)
    ('알 수 없음', 'Unknown'),
    ('로드된 파일 없음', 'No file loaded'),
    ('REW 측정 설정을 읽어왔습니다: {0} Hz, {1}, 레벨 {2}', 'REW settings read: {0} Hz, {1}, level {2}'),
    # ──────────────── 3단계 ────────────────
    ('채널별 레벨', 'Channel levels'),
    ('▶ / ■', '▶ / ■'),
    ('막대', 'Bar'),
    ('값', 'Value'),
    ('마스터 출력', 'Master output'),
    ('슬라이더', 'Slider'),
    ('전 채널 자동 순차 재생', 'Play all in sequence'),
    ('마이크 게인', 'Mic gain'),
    # ──────────────── 4단계 ────────────────
    ('배치 선택', 'Layout'),
    ('배치 버튼', 'Layout buttons'),
    # 7단계 — 1점 측정 상태 표시(2026-09-24)
    ('점선 원', 'Dotted circle'),
    ('측정 옵션', 'Measurement options'),
    ('측정 시작 지연', 'Start delay'),
    ('위치당 스윕', 'Sweeps per position'),
    ('예상 소요', 'Estimated time'),
    # 2026-09-23: 전에는 합계만 보여줘 "위치 하나에 14분 20초" 로 오해했다 — 위치당 값을 주 표시로.
    ('위치당 {0}', '{0} each'),
    # 영어가 한국어보다 길어 1024 폭에서 헤딩까지 밀렸다(qa_capture --lang en, 09-23) — 줄였다.
    # 09-24 빌드 전 QA: 1024 오른쪽 칸에서 세 줄로 접혀 한 번 더 줄였다.
    ('{0} 위치 합계 {1} · 마이크 이동 시간 제외', 'Total for all positions ({0}): {1} + mic moves'),
    ('타이밍 레퍼런스', 'Timing reference'),
    # ──────────────── 5단계 ────────────────
    ('측정', 'Measure'),
    ('이 위치 측정', 'Measure this position'),
    ('측정 취소', 'Cancel'),
    ('마지막 위치 다시', 'Redo last'),
    ('mdat 불러오기', 'Load mdat'),
    ('측정 이름', 'Measurement names'),
    ('위치마다 mdat 를 따로 저장해 차례로 불러와도 됩니다. 이름의 위치 번호대로 들어갑니다.', 'You can also save one mdat per position and load them one after another; each goes to the position in its names.'),
    ('배정 창', 'Assignment window'),
    ('데모 데이터', 'Demo data'),
    ('이 위치 {0}', 'This position: {0}'),
    ('그래프', 'Graph'),
    # ──────────────── 6단계 ────────────────
    ('보정 범위', 'Correction range'),
    ('제어점', 'Control points'),
    ('빈 곳 우클릭', 'Right-click empty space'),
    ('보기', 'View'),
    ('범례', 'Legend'),
    ('측정 위치', 'Positions'),
    ('축', 'Axes'),
    ('주파수 하한', 'Freq. min'), ('주파수 상한', 'Freq. max'),
    ('시간 하한', 'Time min'), ('시간 상한', 'Time max'),
    ('세로 하한', 'Level min'), ('세로 상한', 'Level max'),
    ('하한은 상한보다 작아야 합니다.', 'Min must be less than max.'),
    ('축 범위', 'Axis range'),
    ('굵은 선 = 위치 1(청취 위치)', 'Bold line = position 1 (listening position)'),
    ('위치 칩', 'Position chips'),
    ('베이스 매니지먼트', 'Bass management'),
    ('서브 최적화', 'Sub optimisation'),
    # 위상 보정 방식(2026-09-24): 'fir'('+ 채널 FIR') 선택지를 없앴다 — 버튼은 영문 이름
    # 그대로(번역 없이 통과, CATALOG 에 없어도 안전) 쓰고 지연 숫자만 화면에 남긴다.
    ('채널 FIR 탭', 'Channel FIR taps'),
    ('서포트 FIR 탭', 'Support FIR taps'),
    ('권장값으로', 'Use recommended'),
    ('직접', 'Manual'),
    # '{0}탭' → '{0} taps' 는 8단계 묶음에 이미 있다(중복 등록하지 않음).
    ('그 밖', 'Other'),
    ('타깃 프리셋', 'Target presets'),
    ('계산 취소', 'Cancel calculation'),
    # ──────────────── 7단계 ────────────────
    ('입력', 'Input'),
    ('계산', 'Calculate'),
    ('빠른 감쇠', 'Fast decay'),
    ('체크', 'Checkbox'),
    ('숫자', 'Number'),
    # ──────────────── 8단계 ────────────────
    ('내보낼 폴더', 'Export folder'),
    ('시트 CSV', 'Sheet CSV'),
    ('FIR (WAV · CSV)', 'FIR (WAV · CSV)'),
    ('서브 딜레이', 'Sub delay'),
    ('README.txt', 'README.txt'),
    ('표', 'Table'),
    ('전송', 'Send'),
    ('검증', 'Verification'),
    ('{0}탭 ≈ {1} ms · 분해능 ≈ {2} Hz', '{0} taps ≈ {1} ms · resolution ≈ {2} Hz'),
    ('예측 vs 실측', 'Predicted vs measured'),
    ('서포트 필터', 'Support filter'),
    ('마이크 입력', 'Mic input'),
    ('1단계에서 입력 채널을 고르세요.', 'Pick the input channel in step 1.'),
    ('주파수', 'Frequency'),     # 옛 GUI 카탈로그의 '@ Hz' ↔ '주파수' 가 7단계 축에 '@ Hz (Hz)' 로 나왔다
    ('하이브리드', 'Hybrid'),
    ('{0}탭', '{0} taps'),
    # 도움말 창 재구성(09-24) — 구역 탭·검색칸·바닥글
    ('도움말 검색', 'Search help'),
    # ──────────────── 7단계 최적화 목표(5단계, 2026-09-24) ────────────────
    ('최적화 목표', 'Optimisation target'),
    ('1 · 좌석 균일', '1 · Seat uniformity'), ('2 · 좌석 우선', '2 · Favor seats'),
    ('3 · 균형', '3 · Balanced'), ('4 · 청취 위치 우선', '4 · Favor listening position'),
    # ──────────────── 7단계 서포트 채널 매트릭스 · 경로 편집 ────────────────
    ('입력당 서포트 수', 'Supports per input'),
    ('자동 선택', 'Auto-select'),
    ('게인', 'Gain'), ('이 경로로 보내는 양(dB). 낮을수록(더 마이너스) 적게 보냅니다.',
                      'How much is sent down this path (dB). Lower (more negative) sends less.'),
    ('딜레이', 'Delay'), ('서포트 소리가 입력보다 늦게 도착하지 않게 도착 시간을 맞춥니다(ms).',
                      'Times the arrival so the support does not lag the input (ms).'),
    # 정확히 등록해 둔다 — 안 하면 '{0} 재생' → 'Play {0}' 패턴(en_g1)이 끝의 "재생" 을 보고
    # 잘못 걸려 "Play L 의 저역 → SW1 에서 함께" 처럼 깨졌다(2026-09-24, test_i18n_wizard 로 발견).
    # 설계안 개정 8(09-24): 6·7단계 대역 재편 — 시트·7단계 엔진 문구
    # 마이크 입력 미터 창(10-07)
    ('마이크 미터', 'Mic meter'),
    ('마이크 입력 미터', 'Mic input meter'),
    ('피크 지우기', 'Clear peak'),
    ('클립!', 'Clip!'),
    ('클립 없음', 'No clip'),
    ('피크 {0:.1f} dBFS · {1}', 'Peak {0:.1f} dBFS · {1}'),
    ('● REW 수신 중', '● Receiving from REW'),
    ('● REW 레벨 모니터 다시 시작 중…', '● Restarting REW level monitor…'),
    ('● REW 응답 없음 — REW 를 -api 로 켜 주세요', '● No reply from REW — start REW with -api'),
    ('BM LP Hz', 'BM LP Hz'),
    # 결정 14(09-24): 입력 × 서브 센드 표(서브가 둘 이상일 때)
)
