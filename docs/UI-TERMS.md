# 화면 용어

화면에 나오는 도메인 낱말의 표입니다. 한 개념에는 한 낱말만 쓰고, `apps/secs2r/ui/src/i18n/ko.json`과 `en.json`의 모든 키가 이 표를 따릅니다. 문장의 결은 [`DESIGN.md`](DESIGN.md) 10절과 make-interfaces-feel-better의 쓰기 규칙을 따르고, 코드와 문서의 낱말은 [`CONTEXT.md`](../CONTEXT.md)에 있습니다.

| 한국어 | English | 쓰는 곳 |
|---|---|---|
| 시스템 · 연결·점검 · 측정·가져오기 · 보정 · 내보내기 | System · Connect & check · Measure & import · Correction · Export | 단계 이름, 레일, 도움말 |
| 단계 머리 | step header | 도움말과 튜토리얼에서 주 동작의 자리를 가리킬 때 |
| 구성 | setup | 시스템의 스피커 구성, 프로젝트 상태 |
| 프리셋 | preset | 구성 프리셋(7.1.4 등) |
| 서브우퍼 / 서브 | subwoofer / sub | 구성의 이름과 대수(서브우퍼 1, 서브우퍼 2대)는 서브우퍼, 합성어(서브 합, 서브만 계산, 인프라 서브)는 서브 |
| 출력 패치 | output patch | 시스템의 스피커·출력 표, CSV 저장 |
| 측정 출력, 측정 출력 장치 | measurement output, measurement output device | 출력 패치, 연결·점검 |
| 내보낼 DSP | export DSP | 시스템의 DSP 장치와 필터 예산 |
| 측정 위치(위치) | measurement position (position) | 위치 목록, 배치도, 성적표 |
| 청취 위치 | listening position | 위치 1(기준 위치) |
| 측정 배치 | measurement arrangement | 위치 수를 고르는 카드. 배치 이름은 청취 위치만 / 의자 하나 / 2–3인 소파 / 소파 두 줄(Main seat / One chair / Sofa for 2–3 / Two rows) |
| 방 배치도, 측정 배치도 | room map, arrangement map | 스피커 그림, 측정 위치 그림 |
| 측정 방식 | measurement method | 자체 측정, REW, 모의 측정, mdat 파일만 가져오기. 프로젝트 상태의 같은 칸도 측정 방식 |
| 자체 측정 | built-in measurement (선택지 라벨은 Built-in) | 측정 방식 |
| 모의 측정 | simulated measurement (선택지 라벨은 Simulated) | 측정 방식, 튜토리얼 |
| 샘플레이트가 다른 측정 | Measurements with other sample rates | 보정 설정에서 펼치는 공통 측정 처리 정책 |
| 계산 중단 (기본값) · 48 kHz로 변환 | Stop calculation (default) · Convert to 48 kHz | 샘플레이트 처리 선택지 |
| 데모 방, 데모 데이터 | demo room, demo data | 7.1.4 · 서브 3의 표본 방과 그 측정 |
| 스윕, 스윕 길이, 스윕 반복, 스윕 레벨 | sweep, sweep length, sweep repeats, sweep level | 측정 옵션, 연결·점검 |
| 측정 시작 지연 | start delay | 측정을 누른 뒤 첫 스윕까지의 시간 |
| 타이밍 기준, 기준 스피커, 기준 출력 | timing reference, reference speaker, reference output | 측정 배치 |
| 레벨 점검 | level check | 연결·점검의 채널별 레벨(단추는 모든 채널 레벨 점검 / Check all channel levels) |
| 적정 구간 | recommended range | 마이크 레벨 −30…−15 dBFS |
| 핑크 노이즈 | pink noise | 레벨 점검 소리 |
| 마이크 보정 파일 | mic calibration file | 연결·점검, 용어 풀이 |
| 장치 공유 방식: 자동 · 독점 · 공유 | Device sharing: Automatic · Exclusive · Shared | 자체 측정의 장치 카드에서 펼치는 항목 |
| 보정 | correction | 단계와 결과 |
| 보정 계산 | Calculate correction(동사 calculate) | 주 동작, 잡 이름. 다시 계산은 recalculate |
| 다시 계산 필요 | Recalculation needed | 결과가 지금 입력과 맞지 않는 낡음 상태 |
| 보정 모드: 최소 지연 · 저지연 · 표준 · 초고지연 | correction mode: Minimum latency · Low latency · Standard · Ultra latency | 보정 설정, 내보내기 |
| 지연, 전체 지연, 지연 한도 | latency, total latency, latency limit | 보정 필터가 더하는 시간 |
| 딜레이 | delay | 서브·메인에 거는 시간 값(BA 값, 메인 추가 딜레이, 뒷줄 딜레이 보정) |
| 룸 보정(RC), RC 범위, RC 상한·하한 | room correction (RC), RC range, RC upper/lower limit | 보정 설정, 그래프 띠, 내보내기 |
| 방 부피, 방 크기 | room volume, room size | 자동 상한(슈뢰더 주파수) |
| 부스트 상한 | boost limit | RC, XO PEQ, 서브 버스 PEQ |
| 크로스오버(XO), XO 추천, XO PEQ | crossover (XO), XO recommendation, XO PEQ | 보정 설정, XO 탭 |
| 서브 정렬(BA) | bass alignment (BA) | 보정 설정, BA 탭 |
| 방식(BA): 자동 · 고정 · 묶음 | method: Automatic · Fixed · Bundle | 서브마다 정렬 방식 |
| 묶음 | bundle | 상대값을 지키는 서브 묶음(DBA, 멀티서브). XO 그룹(group)과 구분 |
| 인프라 서브, 인프라 XO | infra sub, infra XO | 가장 낮은 대역만 맡는 서브 |
| 서브 합, 서브 합 평탄도, 좌석 편차 | sub sum, sub sum flatness, seat spread | BA 결과, 판정 |
| GD, GD 폭 | group delay (GD), GD spread | GD 탭, 판정, 성적표. 화면에 '군지연'을 쓰지 않습니다(업스트림 10-10 '군지연 → GD') |
| 타겟, 타겟 곡선 | target, target curve | 프리셋은 플랫 · 하만 · 시네마(Flat · Harman · Cinema) |
| 판정, 채널별 성적표 | verdict, channel scorecard | 보정 결과 |
| 보정 전 · 보정 후 · 보정 전후 | before · after · before and after | 그래프, 내보내기 |
| MIMO, 서포트 | MIMO, support | MIMO 절, 내보내기 |
| 내보내기, 내보낼 폴더 | export, export folder | 내보내기 |
| CamillaDSP 설정 | CamillaDSP configuration | 내보내는 설정 파일. 화면의 설정(settings)과 영어에서 구분 |
| 단계별 mdat | staged mdat | 내보내기의 고급 |
| 작업, 작업 트레이 | job, job tray | 긴 작업의 진행·취소·기록 |
| 계산 메모 | calculation notes | 판정 카드에서 펼치는 목록 |
| 튜토리얼 | tutorial | 튜토리얼 목록, 코치 카드 |

## 쓰는 법

- 문장은 합니다체, 지시는 -세요, 상태와 라벨은 명사구입니다. 영어는 문장형 대문자(sentence case)와 명령형, 미국 철자(color, license, center, canceled)를 씁니다.
- 문장 안에서 단추나 라벨을 가리킬 때는 화면의 글자 그대로 작은따옴표로 감쌉니다('보정 계산', 'Calculate correction').
- 영어에서 개수를 붙이는 자리는 복수형이 틀리지 않게 `Label: {count}` 꼴로 씁니다. 코드가 단수와 복수를 따로 넘기지 않습니다.
- 한국어 조사는 영문 낱말에 붙여 씁니다('REW가', 'MIMO는'). `i18n.test.ts`가 확인합니다.
- 입력란과 설정에는 실제 동사를 씁니다. 이름은 입력하고, 파일은 불러오며, 기본값은 적용합니다. 이 동작들을 '채우다' 하나로 표현하지 않습니다.
- 화면이나 안내의 전환은 목적지를 적어 '이동'으로 설명합니다. '채우고 넘어갑니다'처럼 실행 내용과 목적지가 모호한 표현을 피합니다.
- 튜토리얼의 행동 안내에는 연습 목적을 연결하고, '이유'에는 그 행동이 필요한 근거를 적습니다. 모의 측정은 스피커 소리 재생 없이 측정 절차를 연습하기 위한 선택입니다.
- 사전에서나 볼 법한 한자어를 쓰지 않습니다. 홈시어터 사용자가 스스로 쓰는 말인지가 기준입니다. 분해능 대신 '더 낮은 주파수까지 세밀하게', 평활 대신 '스무딩', 분홍 잡음 대신 '핑크 노이즈', 군지연 대신 'GD', 통과 대역 대신 '두 필터 사이', 비용(cost) 대신 '평가값', 절 대신 '영역'을 씁니다. 위상, 극성, 크로스오버, 룸 모드처럼 이 분야 사용자가 찾는 말은 그대로 둡니다.
- 누구나 아는 사실을 다시 말하는 문장을 쓰지 않습니다. 튜토리얼에 '파일을 저장하지 않습니다', 코치 카드에 '계속을 두 번 누르면…' 같은 설명을 넣지 않습니다.
- 튜토리얼의 설명 단계는 기능과 효과를 설명합니다. 현재 조작할 수 없는 항목에 '고르세요', '바꾸세요'라고 지시하지 않습니다. 실행 지시는 실습 단계에서 허용된 조작에 사용합니다.

## 예외

- `system.role.*`(시스템 화면의 채널 이름)은 한국어 카탈로그에도 돌비 영문 이름(Left Surround 등)을 둡니다. 업스트림이 두 언어 모두 영문으로 보이고, `SystemScreen.test.tsx`가 그 글자를 확인합니다. 측정·가져오기의 `measure.role.*`은 한국어 이름입니다.
- 지도의 역할 없는 스피커는 '역할 미지정'입니다(`ArrangementMap.test.tsx`가 확인). 출력 패치의 열 이름은 '채널'입니다.
