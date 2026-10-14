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

## 6. 보정 (screen 9) — P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 7. 보정 설정 (inventory §5) — P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 8. 그래프 (inventory §4) — P31a, P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 9. 내보내기 (screen 8) — P32

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 결과 줄: 모드 · 총 지연 · 채널 N개 전후 · 서브 N개 측정 그대로 · MIMO 적용 N | `export.result.summary`, `export.result.mimo` | — | P32 |
| 낡은 결과 알림 | `export.result.stale`, `frame.header.blocked` | — | P32 |
| 내보낼 것이 없는 이유(보정으로 가는 길) | `export.result.empty`, `export.go_calibrate`, `frame.header.blocked` | — | P32 |
| 역할 코드 | `export.result.roles` | — | P32 |
| RC 요약 | `export.result.rc` | — | P32 |
| 내보낼 폴더 표시 | `export.folder.path` | — | P32 |
| [변경…] → `extra.export.folder` | `export.folder.change` | — | P32 |
| [폴더 열기] | `export.folder.open` | — | P32 |
| [REW mdat 저장] → 보정 전후 mdat, `SECS_mdat_<RC_BA\|RC_BA_MIMO>_<모드>_<시각>/` | `export.run.before_after_mdat` | — | P32 |
| [CamillaDSP 내보내기] | `export.run.camilla_dsp`(단계 머리의 주 동작) | — | P32 |
| 보정 화면의 숨은 내보내기 카드(단계별 mdat) | `export.advanced`, `export.run.staged_mdat` | U3 | P32 |
| 결과가 없거나 계산·내보내기 중이면 단추 끔 | `export.run.camilla_dsp`, `export.run.before_after_mdat`, `export.run.staged_mdat` | — | P32 |
| 메모: N개 파일 저장 → 경로 / 실패 / 튜토리얼에서는 저장 안 함 | `export.outcome.<kind>`, `export.outcome.<kind>.open`, `export.tutorial` | — | P32 |
| 기본 폴더 `<프로젝트 폴더>/SECS_export/<날짜> <이름>/`, 없으면 `~/Documents/SECS_export/<날짜>` | `export.folder.path` | — | P32 |
| MIMO는 켜고, 계산했고, 위치 3곳 이상일 때만 포함 | `export.result.mimo` | — | P32 |
| 아무 일도 하지 않는 [완료] 단추 | — | U4 | P32 |

## 10. 화면 전체 (inventory §2) — P29, P32

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 창 제목 "<버전> · 측정 위저드", 튜토리얼 표시, 1280×800, 밝은 테마 고정 | `frame`(레일 머리의 판 번호와 튜토리얼 표시) | — | P29 |
| 단계 막대: 단계 누르기, 완료 / 지금 / 아직 | `frame.rail`, `frame.rail.<step>` | — | P29 |
| 사이드바: 구성 요약 | `frame.status.config` | — | P29 |
| 사이드바: 측정 k / N | `frame.status.positions` | — | P29 |
| 사이드바: 보정 상태 | `frame.status.calibration` | — | P29 |
| 사이드바: REW 연결 | `frame.status.path` | — | P29 |
| 사이드바: 프로젝트 파일·저장 안 됨·튜토리얼 | `frame.status.file`, `frame.project` | — | P29 |
| [도움말] | `frame.help` | — | P29 |
| [튜토리얼] / [튜토리얼 끝내기] | `help.intro.tutorial`, `project.tutorial.start`, `tutorial.panel.end`, `tutorial.end.finish` | — | P32 |
| [튜토리얼 다시] | `tutorial.panel.restart`, `tutorial.end.restart` | — | P32 |
| [프로젝트 저장 ▾ 다른 이름으로 저장…] | `frame.project.save`, `frame.project.save_as` | — | P29 |
| [프로젝트 열기] | `frame.project.open`, `project.open`, `project.open.recent.<n>`, `project.open.browse` | — | P29, P32 |
| 숨긴 언어 줄 | `frame.settings.language` | U7 | P29 |
| 내비 [← 이전] / [다음 →] | `frame.rail.<step>`(Alt+← / Alt+→) | — | P29 |
| 가운데 메시지(막힌 이유, 건너뛰기 메모) | `frame.header.blocked` | — | P29 |
| 1, 3, 5단계의 건너뛰기 단추 | `frame.rail.connect`(mdat 경로에서 건너뜀); 각 단추는 §1, §3, §5 | — | P29, P30, P31a |
| 마지막 단계의 [완료] | — | U4 | P32 |
| 단계 이동: 모니터 시작·정지, REW 새로 읽기, "화면 준비 중…" | `frame.rail.<step>`, `jobs.tray` | U8 | P29 |
| 새 프로젝트(업스트림은 단추 없음, 기본 2.0 · CamillaDSP · 20 Hz–20 kHz · 혼합 위상) | `frame.project.new`, `project.new`, `project.new.start`, `project.new.preset`, `project.new.subs`, `project.new.create` | — | P29, P32 |
| 저장(Ctrl+S): 경로 또는 대화상자, `.irs` 복사, SPL 오프셋, 옆 `.mdat` | `frame.project.save`, `project.unsaved.save` | — | P29, P32 |
| 다른 이름으로 저장(Ctrl+Shift+S) | `frame.project.save_as` | — | P29 |
| 열기(Ctrl+O) → `open_project` | `frame.project.open`, `project.open.browse` | — | P29, P32 |
| 닫기·튜토리얼 전 저장 질문: 저장 / 저장 안 함 / 취소 | `project.unsaved`, `project.unsaved.save`, `project.unsaved.discard`, `project.unsaved.cancel` | — | P32 |
| 닫을 때 모니터·측정·계산·MIMO 워커 멈춤 | `project.unsaved`(닫는 길; 잡 정지는 P33의 호스트) | U8 | P32 |
| 언어 전환과 문구(`t()`, 카탈로그), 살아 있는 툴팁 | `frame.settings.language` | U7 | P29 |
| 툴팁(약 150개, 800 ms, 420 px) | `frame.rail.<step>`(막힌 이유 툴팁; 모든 화면의 키트 툴팁) | — | P29 |
| 닫힌 콤보 위의 휠은 페이지를 스크롤 | `frame.content`(기본 select는 휠로 값이 바뀌지 않음) | — | P29 |
| 전문가 모드·정보 창 없음 | `frame.settings.expert` | U9 | P29 |
| REW 감시 3초마다(3, 5, 8단계에서 멈춤) | `frame.status.path` | — | P29 |
| 튜토리얼 모드: 데모 프로젝트, 가짜 REW, 분홍 테마, 저장·내보내기 막음 | `project.tutorial`, `frame`(튜토리얼 강조색), `export.tutorial` | — | P32 |
| 기다림 표시: 로딩 카드, 작업 패널, 시작 화면 | `jobs.tray`, `jobs.job.<kind>`, `jobs.cancel.<kind>` | U8 | P29 |
| 메시지가 여러 곳에 흩어짐, 알림 모음 없음 | `jobs.history` | U8 | P29 |
| 단축키 Alt+← / Alt+→, Ctrl+1…5 | `frame.rail.<step>` | — | P29 |
| 단축키 Ctrl+S, Ctrl+Shift+S, Ctrl+O | `frame.project.save`, `frame.project.save_as`, `frame.project.open` | — | P29 |
| F1 도움말 | `frame.help`, `help` | — | P29, P32 |
| Ctrl+M 떠 있는 마이크 미터 | 연결·점검의 마이크 미터 패널(§3, P30); 단축키는 P33 | — | P30 |
| 도움말 Ctrl+F, Enter 다음, Esc | `help.search`, `help.search.next`, `help.close` | — | P32 |
| 배치도 화살표, 축 메뉴 Enter | `<id>.position.<n>`(§4, §5), `<id>.axes`(§8) | — | P30, P31a, P31b |
| 도움말 창: 비모달, 제목 "도움말 · <단계>", "k / 7" 칩, 검색 | `help`, `help.step`, `help.search` | — | P32 |
| 도움말 탭: 처음이라면, 따라 하기, 확인할 것, 문제 해결, 실제 REW, 측정의 mdat, 내보내기의 CamillaDSP | `help.tab.<tab>` | — | P32 |
| 도움말 그림을 누르면 큰 보기 | —(그림 없이 새 화면에 맞게 다시 씀, M5 §2) | — | P32 |
| 도움말이 지금 단계를 따라감 | `help`, `help.step` | — | P32 |

## 11. 튜토리얼 (inventory §3) — P32, P35

엔진의 줄입니다. 퀘스트 목록의 줄(장치부터 내보내기까지의 퀘스트, `plant_wrong_out`)은 P35가 더합니다.

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| `set_trial(True)`로 시작(데모 프로젝트, 튜토리얼 강조색) | `project.tutorial.start`, `tutorial` | — | P32 |
| `when()`이 거짓인 퀘스트는 뺌 | `tutorial`(`Quest.when`) | — | P32 |
| 끝에서 처음부터 다시 또는 끝내기 | `tutorial.end`, `tutorial.end.restart`, `tutorial.end.finish` | — | P32 |
| 퀘스트 칸: 화면, 종류, 표적, 문구, 왜, 끝 판정(`done`, `base`), `auto`+메모, `busy`+문구, `keep`, `ok`, `blocked`, `animate`, `apart` | `tutorial.coach`(`screens/tutorial/types.ts`의 `Quest`) | — | P32 |
| 용어 25개를 처음 나온 카드 또는 [왜?] 아래에 | `tutorial.coach`, `tutorial.coach.why` | — | P32 |
| 덮개: 표적 말고 모두 어둡게 | `tutorial.overlay` | — | P32 |
| 보기 표적: 점선, 누를 수 없음(휠은 지나감) | `tutorial.overlay.look` | — | P32 |
| 해 보기 표적: 강조색 실선, 퍼지는 고리, 누를 수 있음 | `tutorial.overlay.act` | — | P32 |
| `apart`: 표적마다 따로 테두리 | `tutorial.overlay`(`Quest.apart`) | — | P32 |
| 튜토리얼 단추는 언제나 누를 수 있음 | `tutorial.coach`, `tutorial.panel` | — | P32 |
| 다른 곳을 누르면 카드가 흔들리고 알림 | `tutorial.toast` | — | P32 |
| 표적을 먼저 보이게 스크롤 | `tutorial.overlay` | — | P32 |
| 코치 카드(340 px, 끌기): "<단계> · k / n", 잠시 치우기, 문구, 용어, [왜?], 힌트, [← 이전], [계속 →]; 표적·레일·머리를 피해 놓임 | `tutorial.coach`, `tutorial.coach.pause`, `tutorial.coach.why`, `tutorial.coach.rule`, `tutorial.coach.prev`, `tutorial.coach.next` | — | P32 |
| 해 보기: 첫 [계속]은 힌트를 흔들고, 둘째는 `auto`; 막히면 이유, 진행 중이면 그 문구 | `tutorial.coach.rule`, `tutorial.coach.blocked`, `tutorial.coach.next` | — | P32 |
| 틱: 끝나면 넘어감, 표적이 0.45초 넘게 없으면 건너뜀, 단계 끝 알림 | `tutorial.toast` | — | P32 |
| 퀘스트 목록(오른쪽 위, 알약으로 접힘) | `tutorial.panel`, `tutorial.panel.fold`, `tutorial.panel.pill` | — | P32 |
| 잠시 치우기: 모두 숨기고 이어 하기 단추만; 이어 하면 막힌 단계로 되감기 | `tutorial.coach.pause`, `tutorial.resume` | — | P32 |
