# M5 도달표: 업스트림 기능이 새 화면에서 닿는 자리

[`M5-inventory.md`](M5-inventory.md)의 항목마다 새 화면에서 그것에 닿는 화면 요소의 id(`data-ui` 또는 같은 값의 `id`)를 적습니다. M5는 이 표에 빈칸이 없어야 끝납니다([`M5.md`](M5.md) 6절). 화면 테스트가 표의 id들이 실제로 그려지는지 확인합니다.

- 한 줄은 목록의 한 항목입니다. 항목 이름은 목록의 낱말을 짧게 옮기고, 자리 표시는 `M5-inventory.md`의 소절을 따릅니다.
- **id**는 화면 요소의 안정된 id입니다. 여럿이면 쉼표로 잇습니다. 역할이나 위치처럼 바뀌는 부분은 `<role>`, `<n>`으로 적습니다.
- **편차**는 업스트림과 다르게 닿는 자리입니다([`DEVIATIONS.md`](DEVIATIONS.md)의 번호). 없앤 기능(U4)은 id 칸에 `—`를 적고 편차 번호를 답니다.
- **작업서**는 그 줄을 채운 작업서입니다. 연결(P33) 뒤에 id가 바뀌면 그 작업서가 고칩니다.

## 1. 장치·마이크 (Step 1) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 2. 구성 (Step 2) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 3. 레벨 체크 (Step 3) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 4. 배치 (Step 4) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 5. 측정 (Step 5) — P31a

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 머리 "위치 k / N" 또는 모두 측정함, "이 위치 약 …" | `measure.where` | — | P31a |
| [그래프 숨기기] / [그래프 보기] (배치도가 커짐) | `measure.graph.toggle` | — | P31a |
| 고를 수 있는 배치도(클릭, 화살표 키), 측정 중에는 무시 | `measure.map`, `measure.map.position.<n>` | — | P31a |
| 그래프와 축 단추, 범례(채널 칩, 서브 굵게, "굵은 선: 위치 1"), 위치 칩(켜고 끄기, 커서 강조, 위치 1곳 이하면 없음) | `measure.plot`, `measure.plot.axes`, `measure.plot.legend.role.<role>`, `measure.plot.legend.position.<n>`, `measure.graph.empty` | — | P31a |
| 위치 목록 "NN 위치 설명", 완료·대기, 툴팁의 원본 파일, 누르면 고름 | `measure.positions`, `measure.positions.<n>` | — | P31a |
| 원본 메모 "불러옴: a.mdat (위치 1–9) · 측정함 (위치 10)" | `measure.sources` | — | P31a |
| 역할 진행 막대 | `measure.run`, `measure.run.progress` | U8: 트레이의 `jobs.job.measure`에도 보임 | P31a |
| [이 위치 측정] (잰 위치면 다시 측정, 모두 끝나면 끔): 카운트다운, 프로젝트 경로, 계획 확인, 측정 | `measure.position.start`, `measure.next`, `measure.path.note` | 모두 측정하면 끄는 대신 주 동작이 "보정으로"(`measure.next`). 경로가 없으면 `~/Documents/SECS/측정 …`을 몰래 만들지 않고 저장 대화상자를 먼저 엶(작업서 P31a, 편차 번호 없음) | P31a |
| [측정 취소]: 카운트다운은 바로, 스윕은 지금 스윕 뒤 멈춤 | `measure.cancel` | U8: 트레이의 `jobs.cancel.measure`도 같은 잡을 멈춤 | P31a |
| [마지막 위치 다시]: 마지막으로 끝낸 위치를 지움 | `measure.redo`, `measure.clear`, `measure.clear.confirm`, `measure.clear.undo` | U5: 확인 뒤 지우고 알림에서 되돌림 | P31a |
| 옵션: 시작 지연, 스윕 길이, 반복 1/2/4/8(음향 타이밍 기준이면 1) | `measure.options.delay`, `measure.options.sweep`, `measure.options.repeats`, `measure.options.repeats.acoustic` | — | P31a |
| [mdat 불러오기] | `measure.mdat.load` | — | P31a |
| [파일 관리] | `measure.files.open` | — | P31a |
| [데모 데이터] (튜토리얼만) | `measure.sample.load` | — | P31a |
| 바쁨 물방울과 경과 시간 | `measure.status`, `measure.run` | U8: 경과·예상 시간은 트레이의 `jobs.job.measure` | P31a |
| 로그 패널: 대기, 역할·위치 진행, 카운트다운, 낮은 S/N 경고, 오류, 새 프로젝트 경로, mdat 줄 | `measure.run`, `measure.warning` | U8: 줄마다의 기록은 트레이의 `jobs.history`, 화면에는 지금 진행과 이 위치의 마지막 경고만. U6: 경고 문턱은 알림이 실어 오는 20 dB(툴팁의 40 dB 없음) | P31a |
| MdatAssignDialog: 측정마다 제목, 내용(읽을 수 없음 / 저역만 · 서브로 보임 / 전대역), 채널(건너뜀 또는 "코드 · 이름"), 여러 위치면 위치; 스캔으로 미리 채움; 문제가 있으면 가져오기 끔 | `measure.mdat.dialog`, `measure.mdat.row.<n>.position`, `measure.mdat.row.<n>.role`, `measure.mdat.problem`, `measure.mdat.import` | — | P31a |
| MdatFilesDialog: 위치, 파일(또는 측정함 / 없음), 채널 있음/전체, [가져오기…], [지우기] | `measure.files`, `measure.files.<n>.load`, `measure.files.<n>.clear` | U5: 지우기는 `measure.clear` 확인과 되돌리기 | P31a |
| load_mdat: 파일 대화상자, 덮개 아래에서 해석, 파일의 위치·채널만 바꿈 | `measure.mdat.load`, `measure.mdat.dialog` | U8: 덮개 대신 트레이의 `jobs.job.mdat_import` | P31a |
| 진행 조건: 측정 중이거나 측정이 없으면 막힘 | `frame.header.blocked` | 측정 중 `measuring`, 다른 장치 잡 `device_busy`, 측정 방식 없음 `no_path`. 측정 없음은 보정 단계의 `no_measurements` | P31a |
| 건너뛰기 [측정 건너뛰기 (mdat 불러오기)] | `measure.mdat.load`, `measure.import.note` | mdat 경로(연결·점검 건너뜀)에서는 단계 머리의 주 동작이 mdat 가져오기 | P31a |

## 6. 보정 (screen 9) — P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 7. 보정 설정 (inventory §5) — P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 8. 그래프 (inventory §4) — P31a, P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 측정 그래프: 로그 x 5–22000 Hz와 고정 눈금, y −35…+20 dB, 이동·확대 없음, (역할, 위치)마다 곡선, 280점 1/12 옥타브, 200–2000 Hz 평균 정규화 | `measure.plot` | 평활과 정규화는 서비스(`measurement_curves`)가 하고 화면은 그리기만 | P31a |
| 측정 곡선의 선 규칙: 채널 색, 서브 굵게, 위치 1 굵게, 다른 위치 가늘고 옅게, 칩에 커서를 대면 강조 | `measure.plot.legend.role.<role>`, `measure.plot.legend.position.<n>` | — | P31a |
| 측정 그래프의 축 범위 팝오버: 주파수·레벨 하한과 상한, [자동], [적용], "하한은 상한보다 낮아야", 실행 동안 기억 | `measure.plot.axes` | 범위는 화면이 열려 있는 동안 기억(그래프를 숨겨도 유지). 단계를 옮겨도 남기려면 P33이 범위를 보관해야 함 | P31a |
| 배치도 ArrayLayoutView (5단계) | `measure.map` | — | P31a |

## 9. 내보내기 (screen 8) — P32

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 10. 화면 전체 (inventory §2) — P29, P32

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 11. 튜토리얼 (inventory §3) — P32, P35

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
