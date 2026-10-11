"""SECS 내장 영/한 번역표 — `secs.i18n.translate()` 가 조회하는 `CATALOG` 하나만 만든다.

exe 안에 그대로 들어가므로 별도 언어 파일이 필요 없다. `add(영어, 한국어)` 는 영어가 원문인
문구(Qt 표준 대화상자 글자, 언어 선택 등)를, `add_korean(한국어, 영어)` 는 한국어가 원문인 위저드 문구를
등록한다(영어 키를 덮지 않는다). 옛 1.41 화면 전용 영→한 쌍은 2026-10-10 슬림화로 그 화면과 함께 지웠고,
위저드 글자에 겹쳐 쓰이는 것만 남았다. 아래는 전부 데이터이며 문구
자체는 사용자에게 보이는 텍스트이므로 고치지 않는다 — 오탈자를 발견해도 여기서 바로 고치지
말고 화면 담당에게 넘긴다.
"""
CATALOG = {}


def add(english, korean, source=None):
    pair = (korean, english)
    CATALOG[english] = pair
    CATALOG[korean] = pair
    if source is not None:
        CATALOG[source] = pair


def add_korean(korean, english):
    """한국어가 원문인 문구(위저드). 영어 키는 건드리지 않아 기존 영어 원문의 번역을 덮지 않는다."""
    pair = (korean, english)
    CATALOG[korean] = pair
    CATALOG.setdefault(english, pair)  # `t()` 로 이미 영어가 된 글자를 한국어로 되돌릴 때 쓴다


for en, ko in [
    ('Language', '언어'),
    ('Change the interface language. Your selection is saved automatically.', '화면 언어를 변경합니다. 선택한 언어는 자동으로 저장됩니다.'),
    ('Sound Equalization & Correction System', '음향 이퀄라이징 및 보정 시스템'),
    ('SECS {0} — Sound Equalization & Correction System', 'SECS {0} — 음향 이퀄라이징 및 보정 시스템'),
    ('Correction Filter', '보정 필터'),
    ('Filter Generation', '필터 생성'), ('Export', '내보내기'),
    ('System', '시스템'), ('Output', '출력 방식'), ('Integrated Stereo', '통합 스테레오'),
    ('Discrete Channels', '개별 채널'),
    ('Boost', '최대 보정량'), ('Tilt', '기울기'),
    ('@ Hz', '주파수'),
    # ('Normal', '일반') 은 뺐다(10-10 3차 시험) — 옛 1.41 화면 말이고, 위저드 극성 '정상' 의 영어가 'Normal' 이다
    ('Resolution', '해상도'), ('Low Res', '낮음'),
    ('Low', '저지연'), ('Zero', '무지연'), ('Latency', '지연 모드'),
    ('Low-Latency', '저지연'), ('Zero-Latency', '무지연'), ('Enable', '사용'),
    ('Crossover', '크로스오버'),
    ('Auto', '자동'), ('Manual', '수동'),
    ('PEQ Band Count', 'PEQ 밴드 수'),
    ('Show:', '표시:'), ('Before', '보정 전'), ('After', '보정 후'),
    ('Subwoofers', '서브우퍼'),
    ('Frequency (Hz)', '주파수 (Hz)'),
    ('Target Curve', '목표 곡선'),
    ('Manual {0} ms', '수동: {0} ms'),
    ('integrated_stereo', '통합 스테레오'), ('discrete_channels', '개별 채널'),
    ('File name', '파일 이름'),
    ('Error', '오류'), ('Done', '완료'),
    ('DSP Export', 'DSP 내보내기'),
    ('OK', '확인'), ('&OK', '확인'), ('Cancel', '취소'), ('&Cancel', '취소'),
    ('Yes', '예'), ('&Yes', '예'), ('No', '아니요'), ('&No', '아니요'), ('Close', '닫기'),
    ('&Close', '닫기'), ('Open', '열기'), ('&Open', '열기'), ('Save', '저장'), ('&Save', '저장'),
    ('Apply', '적용'), ('Reset', '초기화'), ('Help', '도움말'), ('Retry', '다시 시도'),
    ('File name:', '파일 이름:'), ('Files of type:', '파일 형식:'), ('Look in:', '위치:'),
    ('Name', '이름'), ('Size', '크기'), ('Type', '유형'), ('Date Modified', '수정한 날짜'),
    ('All Files (*)', '모든 파일 (*)'),
]:
    add(en, ko)

for en, ko in [
    ('Ignore', '제외'), ('Main L', '메인 왼쪽'), ('Main R', '메인 오른쪽'), ('Main L+R', '메인 좌우'),
    ('Main L+Sub', '메인 왼쪽 + 서브'), ('Main R+Sub', '메인 오른쪽 + 서브'),
    ('Sub1', '서브 1'), ('Sub2', '서브 2'),
    ('Subwoofer', '서브우퍼'), ('Subwoofer 1', '서브우퍼 1'), ('Subwoofer 2', '서브우퍼 2'),
    ('Measurement name', '측정 이름'), ('Date', '날짜'), ('Role', '역할'), ('Auto detect', '자동 감지'),
    ('Duplicate: {0}', '중복: {0}'),
    ('Launch REW in API mode', 'REW API 모드로 실행'),
    ('Channel', '채널'),
    ('Microphone position', '마이크 위치'), ('Status', '상태'),
]:
    add(en, ko)
for number in range(1, 9):
    add(f'Sub {number}', f'서브 {number}')

for en, ko in [
    ('Audio devices', '오디오 장치'),
    ('Channel configuration', '채널 구성'),
    ('Up to {0}ch', '최대 {0}ch'),
]:
    add(en, ko)

