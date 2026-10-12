# -*- coding: utf-8 -*-
"""위저드 영어 번역표 G2(5~7단계·설계/저역 엔진·모델). (한국어 원문, 영어) 쌍."""
PAIRS: tuple[tuple[str, str], ...] = (
    # ───────────── step5_measure.py ─────────────
    ('건너뜀', 'Skipped'),
    ('측정 역할 배정', 'Assign measurement roles'),
    ('측정마다 배정할 스피커를 고르세요.', 'Choose a speaker to assign to each measurement.'),
    ('같은 역할이 두 번 배정됐습니다: {0}', 'The same role is assigned twice: {0}'),
    ('파일 관리', 'Files'),
    ('그래프 숨기기', 'Hide graph'),
    ('그래프 보기', 'Show graph'),
    ('불러올 측정이 없습니다. 하나 이상 역할을 고르세요.',
     'Nothing to load. Assign at least one role.'),
    ('위치 — / —', 'Position — / —'),
    ('프로젝트를 만들지 못했습니다: {0}', 'Could not create the project: {0}'),
    ('프로젝트를 새로 만들었습니다: {0}', 'Created a new project: {0}'),
    ('위치 {0} / {1}', 'Position {0} / {1}'),
    ('대기', 'Next'),
    ('불러온 위치 {0}개 · 채널 {1}', 'Loaded {0} positions · {1} channels'),
    ('불러온 위치 1개 · 채널 {0}', 'Loaded 1 position · {0} channels'),     # 단수(10-10 3차 시험 '1 positions')
    ('대기 중. REW blocking 모드로 채널별 SPL 스윕을 보냅니다.',
     'Idle. Per-channel SPL sweeps are sent to REW in blocking mode.'),
    ('경과 {0}:{1}', 'Elapsed {0}:{1}'),
    ('측정할 수 없습니다: REW 측정기 또는 저장 경로가 없습니다.',
     'Cannot measure: no REW measurer or no save path.'),
    ('측정 계획 오류: {0}', 'Measurement plan error: {0}'),
    ('위치 {0}: 마이크를 {1} 번 위치로 옮기고 시작합니다.',
     'Position {0}: move the mic to position {1}; starting.'),
    ('취소 요청을 보냈습니다. 진행 중인 스윕이 끝나면 멈춥니다.',
     'Cancel requested. Stopping after the current sweep.'),
    ('{0} 완료 (위치 {1})', '{0} done (position {1})'),
    ('⚠ S/N 이 낮습니다({0}, 권장 {1} dB 이상). 스윕 레벨이나 마이크 게인을 올리거나 '
     '방을 조용히 한 뒤 이 위치를 다시 재세요.',
     '⚠ Low signal-to-noise ratio ({0}; {1} dB or more recommended). Raise the sweep level or '
     'mic gain, or quiet the room, then redo this position.'),
    ('위치 {0} 측정을 취소했습니다 (수신 {1}개).',
     'Position {0} measurement cancelled ({1} received).'),
    ('위치 {0} 완료 · IR {1}개 저장 ({2}.irs/)', 'Position {0} done · {1} IRs saved ({2}.irs/)'),
    ('위치 {0} 완료 · IR {1}개', 'Position {0} done · {1} IRs'),
    ('측정 실패: {0}', 'Measurement failed: {0}'),
    ('위치 {0} 기록을 지웠습니다. 다시 측정하세요.', 'Position {0} record deleted. Measure it again.'),
    ('샘플 측정(데모 방 7.1.4 · 서브 3 실측) {0}개 위치 × {1}채널을 불러왔습니다.',
     'Loaded the sample measurement (7.1.4 · 3 subs demo room, real): {0} positions × {1} channels.'),
    ('샘플 측정(데모 방 7.1.4 · 서브 3 실측) 1개 위치 × {0}채널을 불러왔습니다.',      # 단수(10-10 3차 시험 '1 positions')
     'Loaded the sample measurement (7.1.4 · 3 subs demo room, real): 1 position × {0} channels.'),
    ('튜토리얼 임시 프로젝트 — 저장되지 않습니다', 'Tutorial temporary project — not saved'),
    # 샘플은 실측 1점뿐(2026-09-24 사용자 결정) — 버튼은 위치 1 만, FakeREW 는 위치 2 이상을 막는다.
    ('샘플은 실측 1점뿐 — 위치 2~{0} 은 비워 둡니다.',
     'The sample has one real position only — positions 2–{0} stay empty.'),
    ('샘플은 실측 1점뿐 — 위치 2 이상은 잴 수 없습니다',
     'The sample has one real position only — positions 2+ cannot be measured'),
    ('합성 측정(테스트용) {0}개 위치 × {1}채널을 불러왔습니다.',
     'Loaded synthetic test measurements: {0} positions × {1} channels.'),
    ('REW 프로젝트 (*.mdat)', 'REW project (*.mdat)'),
    ('REW 프로젝트 불러오기', 'Open REW project'),
    ('mdat 를 읽지 못했습니다: {0}', 'Could not read the mdat: {0}'),
    ('읽을 수 없음', 'Unreadable'),
    ('저역만(서브로 보임)', 'Lows only (looks like a sub)'),
    ('전대역', 'Full range'),
    ('mdat 불러오기를 취소했습니다.', 'mdat loading cancelled.'),
    ('건너뜀: "{0}"', 'Skipped: "{0}"'),
    ('측정이 진행 중입니다.', 'Measurement in progress.'),
    ('측정이 없습니다. 측정하거나 건너뛰기를 누르세요.', 'No measurements. Measure or press Skip.'),

    # ───────────── step6_design.py ─────────────
    ('끄기', 'Off'),
    ('시네마', 'Cinema'),
    ('플랫', 'Flat'),
    ('하만', 'Harman'),
    ('타겟 커브', 'Target curve'),
    ('서브', 'Sub'),
    ('게인', 'Gain'),
    ('극성', 'Polarity'),
    ('딜레이', 'Delay'),
    ('올패스', 'All-pass'),
    ('거리', 'Distance'),
    ('레벨 트림', 'Trim'),
    ('계산 대기 중', 'Waiting to calculate'),
    # score 는 그룹끼리 비교할 수 없는 상대값이라 요약 줄에서 뺐다(2026-09-24, XoStrip 막대에만 남김).
    ('추천 XO {0} Hz', 'Suggested XO {0} Hz'),
    ('반전', 'Inverted'),
    ('정상', 'Normal'),

    # ───────────── step7_bass.py ─────────────
    ('입력', 'Input'),
    ('둘 다', 'Both'),
    ('전', 'Pre'),
    ('후', 'Post'),
    ('응답', 'Response'),
    ('계산', 'Calculate'),
    ('게인 상한', 'Gain limit'),
    ('입력당 서포트', 'Supports per input'),
    ('딜레이', 'Delay'),
    ('극성 반전', 'Invert Polarity'),
    ('위치 {0} · {1}', 'Position {0} · {1}'),
    ('경로 {0} → {1}', 'Path {0} → {1}'),

    # ───────────── design.py (6단계 상태줄·안내에 뜨는 문구) ─────────────

    # ───────────── bass.py (7단계 상태줄·감쇠 토글 사유) ─────────────

    # ───────────── model.py (역할 이름·그룹 이름·구성 검증 문구) ─────────────
    ('프론트', 'Front'),
    ('센터', 'Center'),
    ('서라운드', 'Surround'),
    ('백', 'Back'),
    ('와이드', 'Wide'),
    ('탑', 'Top'),
    ('하이트', 'Height'),
    ('기타', 'Other'),
    ('사용자 정의', 'Custom'),
    ('역할 중복: {0}', 'Duplicate role: {0}'),
    ('{0}: 출력 채널 번호가 잘못되었습니다({1}, 1 이상 정수)',
     '{0}: invalid output channel number ({1}; must be an integer ≥ 1)'),
    ('출력 채널 중복: {0} ({1} ↔ {2})', 'Duplicate output channel: {0} ({1} ↔ {2})'),
    ('{0}: 알 수 없는 대역 {1}', '{0}: unknown band {1}'),
    ('{0}: 극성은 + 또는 - 여야 합니다({1})', '{0}: polarity must be + or - ({1})'),
    ('{0}: 대역이 없습니다', '{0}: no bands'),
    ('{0}: 서브우퍼는 SUB 대역 하나만 가질 수 있습니다({1})',
     '{0}: a subwoofer can only have a single SUB band ({1})'),
    ('{0}: 메인/탑/하이트 스피커에는 SUB 대역을 쓸 수 없습니다',
     '{0}: main/top/height speakers cannot use the SUB band'),
    ('{0}: 대역 조합 오류 {1} (가능: {2})', '{0}: invalid band combination {1} (allowed: {2})'),
    # ── 옛 en_g3(8′ 내보내기·검증)·en_g7(7단계 저역 설계)에서 옮긴 쌍 — 지금 화면도 쓰는 문구(2026-10-10 슬림화).
    #    순서를 en_g3 → en_g7 그대로 두어 겹치는 키의 영어가 예전과 같다.
    ('출력', 'Output'),
    ('게인', 'Gain'),
    ('딜레이', 'Delay'),
    ('극성', 'Polarity'),
    ('올패스', 'All-pass'),
    ('입력', 'Input'),
    ('서포트', 'Support'),
    ('필터', 'Filter'),
    ('밴드', 'Band'),
    ('편차', 'Deviation'),
    ('판정', 'Verdict'),
    ('내보낼 폴더', 'Export Folder'),
    ('변경…', 'Change…'),
    ('폴더 열기', 'Open folder'),
    ('내보낼 폴더 선택', 'Choose export folder'),
    ('시트 CSV', 'Sheet CSV'),
    ('프리셋 {0}', 'Preset {0}'),
    ('방식', 'Method'),
    ('전송', 'Send'),
    ('다시 측정', 'Measure again'),
    ('예측 vs 실측', 'Predicted vs Measured'),
    ('반전', 'Inverted'),
    ('정상', 'Normal'),
    ('내보내기 실패: {0}', 'Export failed: {0}'),
    ('{0}개 파일을 저장했습니다 → {1}', 'Saved {0} files → {1}'),
    ('실패: ', 'Failed: '),
    ('실패: {0}', 'Failed: {0}'),                      # 조립형('실패: ' + message)의 완성 꼴,
    ('{0} · 검증 {1}', '{0} · Verification {1}'),
    ('실측', 'Measured'),
    ('서브', 'Sub'),
    ('밴드 {0}({1} dB)', 'band {0} ({1} dB)'),
    ('1채널', '1 channel'),
    ('{0} 주파수', '{0} frequency'),
    ('{0} 게인', '{0} gain'),
    ('{0} 켬', '{0} on'),
    ('{0} PEQ {1} 끔', '{0} PEQ {1} off'),
    ('{0} XO 주파수', '{0} XO frequency'),
    ('{0} 딜레이', '{0} delay'),
    ('{0} 극성', '{0} polarity'),
    ('저역 설계', 'Low-frequency design'),
    ('채널 보정', 'Channel correction'),
    ('최적화 목표', 'Optimization goal'),
    ('빠른 감쇠', 'Fast decay'),
    ('균형', 'Balanced'),
    ('청취 위치', 'Listening position'),
    ('끔', 'Off'),
    ('응답 우선', 'Response first'),
    ('감쇠 우선', 'Decay first'),
    ('주파수 응답', 'Frequency response'),
    ('감쇠', 'Decay'),
    ('보정 전', 'Before'),
    ('보정 후', 'After'),
    ('보정 전후', 'Before / after'),
    ('감쇠 시간', 'Decay time'), ('간격', 'Step'),
    ('없음', 'none'),
    ('메인', 'Main'),
    ('합', 'Sum'),
    ('같음', 'same'),
    ('느려짐', 'slower'),
    ('계산 중…', 'Calculating…'),
    ('자동 보정', 'Auto correction'), ('필터 설계 계산 중…', 'Calculating channel filters…'),
    ('스윕 반복', 'Sweep repeats'),
    ('음향 타이밍 레퍼런스는 반복 1', 'Acoustic timing reference uses 1 repeat'),
)

# SKIPPED:
#   step5_measure.py '샘플' — 자동 생성 파일 이름 앞머리(SAMPLE_PREFIX). 파일 이름이라 번역하지 않음.
#   step5_measure.py '측정 {0}' — NOTES 참고(과매칭 위험이라 패턴으로 넣지 않음).
#   step6_design.py  '타겟.json' — 저장 창의 기본 파일 이름.
#   step7_bass.py    ' 개' — 스핀박스 접미사. `t(suffix)` 로 걸려 있고 카탈로그에도 등록돼 있다(해결됨).
#   step7_bass.py    '경로 ' — 조립 문구. '경로 {0} → {1}' 패턴으로 대신 넣음(NOTES).
#   design.py        'invalidate 인자는 …' — 개발자용 ValueError.
#   bass.py          'supports 는 [위치, 서포트, bin] …' 류 — 개발자용 예외(API 오용) 여러 건.
#   model.py         '알 수 없는 프리셋: …' — 개발자용 KeyError(화면은 목록에서만 고른다).
#
# 09-23 정리: 09-22 i18n.py 개선(캡처값에 한글이 섞이면 재귀 translate — i18n.py:79~102)과 이후
# 커밋으로 이미 해결된 항목을 지웠다. 확인한 것: '같은 역할이 두 번 배정됐습니다: {0}' 등 self.warning
# (i18n QLabel)로 뜨는 조립 메시지, '필터 설계 실패/저역 통합 실패: {0}'(self.status 도 i18n QLabel),
# '크로스오버 … 다점 편차 …' 합친 패턴, plot.py 의 TextItem·라벨(`_translate()`로 감쌈),
# layout_map.py 의 addSimpleText·툴팁(`_translate()`로 감쌈), step7_bass.py 의 setLabel('주파수'/'응답')과
# seg() 버튼(스타일 seg 가 이미 번역 QPushButton 을 만든다).
#
# NOTES: (리드가 `t()` 를 직접 걸거나 표시 경로를 손봐야 하는 곳 — 아직 안 됨)
#  [조립 문구 — 경고를 ' / ' 로 이은 뒤 붙이는 부분]
#   step6_design.py  '설계 완료 · N ms' + ' · ' + 경고(' / ' 로 최대 2개 이음) — 패턴 '설계 완료 · {0} ms · {1}'
#                     자체는 등록돼 있지만, i18n 의 캡처값 재번역은 ', '/'\n'/' · ' 로만 쪼개서 ' / ' 로 이은
#                     경고 묶음은 안 갈라져 한국어로 남는다. 잇기 전에 경고마다 t() 를 걸 것.
#   step7_bass.py    '저역 통합 완료 · N초' + ' · ' + 경고 — 위와 같은 ' / ' 조인 문제.
#   step5_measure.py '측정 계획 오류: ' + ' · '.join(problems) — problems 는 rewapi
#                     `MeasurementPlan.validate()` 문구(다른 조)라 그 자체가 등록돼 있지 않으면 한국어로 남는다.
#  [과매칭 위험으로 패턴에서 뺀 것]
#   step5_measure.py f'측정 {row + 1}'(mdat 제목이 빈 측정의 대체 이름) — '측정 {0}' 은 사이드바의
#                     '측정 20260920-1200.secs' 같은 파일 이름까지 먹는다. 호출부에서 t('측정') + 번호로.
#   step5_measure.py f'{number:02d}  {points[index].text}    {state}' — 한글 리터럴 없음. state
#                     ('완료'/'대기')와 위치 이름(points[].text, 4단계·layout_map 소관)을 각각 t() 로.
#  [위젯 setText 를 타지 않는 표시 경로]
#   step5_measure.py QListWidgetItem(위치 목록) — 위 항목의 문구.
#   step5_measure.py QPlainTextEdit 로그(setPlainText / appendPlainText) — `_append()` 가 t() 없이 그대로
#                     appendPlainText 하므로 영어 모드에서도 5단계 로그 전부가 한국어로 남는다.
#                     `_append()` 안에서 한 번만 t() 를 걸면 로그 전부가 해결된다.
#   step5_measure.py QFileDialog.getOpenFileName 제목·필터 — t() 없이 넘김(i18n.QFileDialog 로 바꾸면 해결).
#   step5_measure.py QDialog.setWindowTitle — 일반 QtWidgets.QDialog 라 t() 필요.
#   step5_measure.py combo.addItem(self.SKIP, None) — '건너뜀'. 일반 QComboBox 라 t() 필요.
#   step5_measure.py QDialogButtonBox 버튼 setText('불러오기'/'취소') — 카탈로그에는 등록돼 있으나
#                     버튼 자체가 t() 를 안 거쳐 여전히 한국어로 뜬다.
#   step6_design.py  QTableWidgetItem 셀 — '반전'/'정상' 은 t() 필요. 나머지는 숫자·단위.
#   step6_design.py  LEGEND 문구 — plot.py/범례 위젯이 어떻게 그리는지 확인 필요(라벨이면 자동, 직접 그리면 t()).
#   step6_design.py  XO_STRIP_TITLE / xo_strip_title() → xo_title.setText 로 가므로 라벨이면 자동.
#   step7_bass.py    item.setToolTip — QTableWidgetItem 툴팁, t() 필요.
#  [파일 포맷과 겹치는 값 — 표시할 때만 번역할 것]
#   model.py         그룹 이름('프론트'·'센터'·'서라운드'·'백'·'와이드'·'탑'·'하이트'·'기타')은
#                     `SpeakerConfig.groups()`·`project.xo` 의 **키**이고 `.secs` 에 저장된다. 모델 값은
#                     그대로 두고 label(group_label(name)) 같은 표시 지점에서만 번역해야 한다.
#   model.py         DspProfile 이름 '사용자 정의' — `.secs` 의 dsp.name 에도 저장된다(표시만 번역).
#  [영어 키 충돌 — `add_korean` 은 CATALOG[en] 을 setdefault 로 두므로 기존 번역은 안 깨지지만,
#   영어 → 한국어로 되돌릴 때는 먼저 등록된 한국어가 나온다]
#   '전'/'후' 는 레거시 'Before'/'After'('보정 전'/'보정 후')를 피해 'Pre'/'Post' 로,
#   극성 '정상' 은 레거시 'Normal'('일반')을 피해 'Non-inverted' 로 했다가(09-24) 10-10 3차 시험에서 현장 말
#   'Normal' / 'Inverted' 로 되돌렸다 — 옛 1.41 화면('일반')은 슬림화로 없어졌고 레거시 쌍도 translations.py 에서 뺐다.
#   en_g1 · en_g2 의 '정상' 쌍은 모두 같은 값이어야 한다(`CATALOG['정상']` 은 마지막 등록이 이긴다).
#   '끄기'/'끔'(Off), '딜레이'/'지연'(같은 값을 가리킬 때) 은 09-24 에 '끄기'·'딜레이' 로 통일했다
#   ('지연'은 '측정 시작 지연'처럼 카운트다운, '공통 지연'처럼 처리 지연에만 남긴다).
