# M5 갤러리 리뷰

M5 판정(`docs/M5.md` §6)의 화면 리뷰 기록입니다. 화면을 그린 쪽(Opus 5.5)과 다른 쪽이 봅니다. Claude의 리뷰(아래)와 ASTRA의 읽기 전용 감사를 둘 다 거쳐, HIGH가 없을 때 끝납니다.

## Claude 리뷰 (full)

- **범위:** 갤러리 견본 131개 × 밝게·어둡게 × 한국어·영어(+튜토리얼 강조색) = 655장, `main`의 0.15.0 머지 커밋 `e0de246`.
- **틀과 스타일:** React 19, CSS 변수 토큰(`src/ui/tokens.css`), uPlot.
- **경계:** 화면을 실제로 본 것은 아래 "본 견본"이고, 나머지는 CI 갤러리 잡의 자동 검사(페이지 오류 없음, 13 px 미만 글자 없음)에 기댑니다.

본 견본: `screen.system.sample`, `screen.connect.mock`, `screen.measure.done`, `screen.calibrate.spl`, `screen.export.ready`(밝게·한국어, 1280), `screen.calibrate.ba.min`(어둡게·영어, 1024×700), `screen.calibrate.mimo-running`(어둡게·한국어), `screen.measure.mdat-assign.min`(밝게·영어), `screen.export.failed.min`(밝게·한국어), `frame.aside`, `app.live`, `tutorial.live.system`, `tutorial.live.calibrate`(P35 검토 때).

| 범주 | 본 것 | 결과 |
| --- | --- | --- |
| Typography | 위 견본의 제목, 라벨, 표, 숫자 열 | LOW 2 |
| Surfaces | 카드, 표 줄, 대화상자, 알림 | Clear |
| Color | 주 동작 하나(채운 단추), 상태 점·알약, 어둡게의 켜짐/꺼짐 주 동작 | Clear |
| Writing | 단계 설명, 카드 설명, 오류 알림, 대화상자 | Clear(P34 뒤) |
| Interruptions | mdat 배정 대화상자, 저장 확인, 튜토리얼 덮개 | Clear: 닫기 단추와 Esc, 취소가 있음 |
| Animations | 갤러리는 정지 화면 | Not reviewed: 10% 속도 재생은 앱에서 따로 |
| Icons | 레일, 단추, 알림 아이콘 | Clear |

### 찾은 것

#### Typography
| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| LOW | `screen.measure.*` 위치 목록(영어) | "Front right · 20 cm up"이 130 px 칸에서 두 줄 | 위치 목록 칸을 넓히거나 높이를 아이콘으로(`docs/UI-TERMS.md`) | 같은 목록 안에서 줄 높이가 들쭉날쭉합니다 |
| LOW | `screen.calibrate.*` 판정 타일(영어, 1024×700) | "GD spread"의 대역 글자가 둘째 줄로 | 라벨과 대역을 두 줄로 고정(다른 타일과 같은 꼴) | 타일들의 숫자 기준선이 어긋납니다 |

### 고려했지만 넘긴 것

| Location | Candidate | Rejected because |
| --- | --- | --- |
| 시스템·연결·점검의 채널 이름 | 한국어 화면의 "Left", "Subwoofer 1"을 한국어로 | SECS 2가 두 언어 모두에서 돌비 채널 이름을 영어로 보입니다(`src/secs/model.py:50`). 업스트림과 같게 둡니다 |
| 측정 곡선의 범례 13색 | 팔레트 절제 | 채널을 가르는 데이터 색이라 상태색 규칙과 별개입니다 |
| 갤러리의 "사전 출시판 0.13.0" | 앱 버전과 다름 | 갤러리 고정값입니다. 앱은 `bootstrap`의 버전을 보입니다 |

### 검증과 판정

- 검증: `npm run build:gallery`, `node scripts/gallery-shots.mjs`(655장, 오류 없음), 위 견본을 눈으로 확인.
- **Not verified:** 움직임(10% 속도), 실제 Tauri 창에서의 확대 배율(125%, 150%).
- **판정(Claude):** HIGH 없음. LOW 2건은 다음 화면 손질 묶음에서 다룹니다.

### 처리 (P40, 0.15.1)

- 위치 목록: 좁은 틀(카드 폭 ≤ 880 px)에서 행마다 상태를 라벨 아래 둘째 줄에 둡니다. 모든 행이 같은 높이이고, 지도와 목록은 나란히 남습니다. 지도와 목록을 위아래로 쌓는 안은 1024×700에서 목록이 첫 화면을 벗어나 버렸습니다.
- 판정 타일: 라벨 줄, 대역 줄, 값 줄을 공유하는 서브그리드로 모든 값이 한 기준선에 섭니다.
- 리뷰 밖에서 함께 고친 것: 계산 메모를 업스트림처럼 늘 보임(`step_calibration.py:2300`), 새 프로젝트·프로젝트 열기 대화상자의 첫 포커스, 상태 패널의 `custom`, 번들 경고. 도달표(`M5-reach.md`)의 빈 편차 칸 60곳을 채웠습니다.

## ASTRA 감사 (읽기 전용)

- **범위:** `main`의 0.15.1 커밋 `7d38df9`, `apps/secs2r/ui/src/` 전체(테스트와 생성 바인딩 제외, 약 21,500줄)와 두 카탈로그, CSS. 작업서는 [`packets/P41-astra-m5-audit.md`](packets/P41-astra-m5-audit.md), 도구는 Read·Grep·Glob만(쓰기 없음). ASTRA(gpt-6-astra, high)가 돌렸고, 보고 원문의 판정은 HIGH 10, MEDIUM 10, LOW 2입니다.
- **대조:** 워커의 자기 보고는 게이트가 아니므로 Claude가 항목마다 화면 코드와 서비스(`crates/secs-service`, `crates/secs-types`), 업스트림(`src/secs/wizard/`)을 읽어 다시 판정했습니다. 전제가 틀린 항목은 기각하고, 성립하지만 사용자가 알아볼 수 있거나 경로가 좁은 항목은 격하했습니다. 아래 표의 "처리"가 0.15.2면 이 기록과 같은 PR에서 고친 것입니다.

### 찾은 것 (대조 뒤)

| 보고 | 판정 | Location | Path | 처리 |
| --- | --- | --- | --- | --- |
| HIGH | **HIGH 성립** | `screens/connect/state.ts:39`, `app/AppFrame.tsx:55`, `app/StepRail.tsx:26`, `screens/system/useSystemScreen.ts:42` | 'mdat 파일만 가져오기'를 고르면 `measure_configure`로 서비스에 저장되고 연결·점검이 `skipped`가 되는데, 레일(disabled)·Ctrl+2·`go()`가 건너뛴 단계를 열지 않습니다. 프로젝트를 다시 열어도 방식이 남아 있어 새 프로젝트 말고는 측정 방식을 바꿀 길이 없습니다. 시스템의 '연결·점검으로'는 이때 측정·가져오기로 보냈습니다. | 0.15.2: 건너뛴 단계도 레일과 Ctrl+N으로 엽니다(Alt+←/→만 건너뜁니다). 시스템의 단추는 라벨대로 연결·점검으로 갑니다. |
| HIGH | **HIGH 성립** | `state/session.ts:179` | 낮은 SNR 경고가 위치별로 누적만 되고 지워지지 않습니다. 같은 위치를 고쳐 다시 재도 이전 경고가 현재 측정의 경고처럼 남고, 새 경고는 겹쳐 쌓입니다. 업스트림(`step5_measure.py:1313`)은 실행마다 새로 셉니다. | 0.15.2: `measure_position`·`position_clear`·`mdat_import`가 그 위치의 경고를, `sample_load`가 전부를 지웁니다. |
| HIGH | MEDIUM(격하) | `screens/measure/MdatDialogs.tsx:82` | `mdat_scan`은 위치 표기가 없는 측정에 1을 추정하고(`mdat_import.rs:288`, current=1), 대화상자가 그 값으로 행을 채우며 배정마다 위치를 명시하므로 `fallbackPosition`이 효력이 없습니다. 안내문("지금 고른 위치로 가져옵니다")과 다릅니다. 위치 열이 보여 알아볼 수는 있습니다. | 0.15.2: `position_tagged`가 아닌 행은 화면의 현재 위치로 채웁니다. |
| HIGH | MEDIUM(격하) | `state/session.ts:91`, `screens/connect/ConnectScreen.tsx:544` | 권장 스윕 레벨 결과가 프로젝트 교체와 입력 장치·채널·방식 변경 뒤에도 남고 적용 단추도 살아 있습니다(고급 아래). | 0.15.2: 프로젝트 교체와 그런 `measure_configure`에서 지웁니다. |
| HIGH | MEDIUM(격하) | `app/jobs/jobStore.ts:150` | `job_cancel`이 거절되면 상태가 `cancel_requested`에 머물러 작업이 스스로 끝날 때까지 취소를 다시 누를 수 없습니다. | 0.15.2: 요청 실패 시 `running`으로 되돌립니다. |
| HIGH | MEDIUM(격하) | `app/shortcuts.ts:82`, `ui/Dialog.tsx:56` | 모달 대화상자 위에서 Ctrl+N·Alt+←/→·Ctrl+O가 그대로 동작해 단계가 바뀌고, 측정 화면 안에서 그리는 mdat 배정 대화상자의 편집이 사라집니다. 업스트림의 QDialog는 키를 가집니다. | 0.15.2: `aria-modal` 대화상자가 열려 있으면 앱 단축키를 무시합니다. |
| HIGH | MEDIUM(격하) | `state/session.ts:118`, `screens/calibrate/ResultGraph.tsx:267` | 채널을 바꾸는 동안 이전 채널의 곡선이 잠깐 남고, 요청이 실패하면 그대로 남습니다. 프로젝트 전환 경쟁은 서비스가 세션 잠금 아래 순서대로 답하므로 실제로는 좁습니다. | 후속(P42): 곡선에 요청한 채널을 함께 두고 불러오는 중 상태를 그립니다. |
| HIGH | 기각 | `screens/connect/ConnectScreen.tsx:454` | `levels_get`의 `levels`는 "재생 레벨"이 아니라 그 채널을 재생할 때 잰 입력 RMS입니다(`monitor.rs:223,370`, 업스트림 `step3_level.py:401`과 같음). 바인딩의 주석("Played level")이 틀려 감사를 오도했습니다. | 0.15.2: `secs-types`의 주석을 고치고 바인딩을 다시 생성했습니다. |
| HIGH | 기각 | `state/triggers.ts:24` | `recommend_xo`는 추천값만 돌려주고 설정을 쓰지 않습니다(`calibration.rs:114`). 사용자가 XO 칸에 넣을 때 `calibration_settings_edit`가 결과·프로젝트를 다시 읽습니다. | — |
| HIGH | 기각 | `App.tsx:141`, `state/session.ts:214` | `app_settings_get`은 서비스가 늘 `Ok`로 답합니다(`ipc.rs:135`). IPC 전송 자체가 죽은 경우만 남고, 그때는 어느 화면도 뜨지 않습니다. | — |
| MEDIUM | **MEDIUM 성립(눈에 띔)** | `screens/system/SystemScreen.tsx:462` | 서비스의 `ConfigGroup.key`는 업스트림 이름(프론트·센터·…)이고 `id`가 표시용(front·center·…)인데 화면이 `key`로 이름을 찾아, 실제 앱에서 모든 그룹이 "기타"로 보입니다. 모의 서비스와 견본이 `id == key`로 만들어 갤러리에서 가려졌습니다. | 0.15.2: `id`로 찾습니다. 모의 서비스·견본도 실제 계약대로 한국어 `key`를 냅니다. |
| MEDIUM | MEDIUM 성립 | `state/session.ts:189` | 취소된 보정·MIMO·XO 추천도 `error.code: cancelled`를 달고 오므로(`jobs.rs:323`) 실패로 표시됩니다. 내보내기도 같습니다. | 0.15.2: `status === "failed"`일 때만 실패입니다. |
| MEDIUM | MEDIUM 성립 | `App.tsx:117`, `screens/connect/connect.css:355` | Ctrl+M의 마이크 미터가 위치 지정 없이 앱 틀 뒤 문서 흐름에 붙어 첫 화면 밖으로 밀립니다. | 0.15.2: 도움말과 같은 aside 자리의 오른쪽 아래에 띄웁니다(주 동작과 트레이를 덮지 않음). Esc·포커스는 후속. |
| MEDIUM | MEDIUM 성립 | `ui/plot/AxisRange.tsx:26`, `screens/calibrate/ResultGraph.tsx:361` | GD 탭에도 dB용 상한(60)이 적용되어 90 ms를 넣을 수 없습니다. | 0.15.2: ms 축은 ±1000(업스트림 `axis_range.py:162`은 ±100000). |
| MEDIUM | MEDIUM 성립 | `screens/tutorial/Tutorial.tsx:199,627` | 코치·완료 카드에 포커스 이동이 없고 완료 상태(`engaged` 거짓)에서 Esc 리스너가 빠집니다. | 후속(P42, Opus). |
| MEDIUM | MEDIUM 성립 | `App.tsx:120`, `screens/tutorial/engine.ts:125` | `stepOpen`을 넘기지 않아 이어 하기가 막힌 단계로 되감지 않습니다. | 후속(P42, Opus). |
| MEDIUM | MEDIUM 성립 | `state/session.ts:166`, `screens/connect/types.ts:17` | 미터 `stalled` 상태로 가는 코드가 없어 정의만 있는 상태입니다. | 후속(P42): 마지막 읽기 시각으로 판단하고 재시작을 잇습니다. |
| MEDIUM | 기각 | `screens/measure/MeasureScreen.tsx:179` | 두 대화상자는 같은 레이어(`ui/layer.tsx`)에 마운트 순서대로 붙으므로 나중에 열리는 배정 창이 위에 옵니다. | — |
| MEDIUM | 기각 | `screens/calibrate/Results.tsx:105` | `CalibrationSummary.warnings`는 서비스가 늘 비워 보냅니다(`calibration.rs:348`). 죽은 필드입니다. | 후속: 서비스가 채우면 화면에 붙입니다. |
| MEDIUM | LOW(격하) | `styles/tokens.css:25`, `app/app.css:242`, `screens/export/export.css:44`, `ui/maps/maps.css:182` | 13 px 캡션 토큰이 막힌 이유·내보내기 항목명·지도 채널 코드에 쓰입니다. 규칙(typography.md)은 캡션과 메타데이터에 13 px를 허용합니다. | 후속 손질에서 라벨(`export-fact dt`)만 14 px로. |
| LOW | LOW 성립 | `i18n/ko.json:76`, `i18n/en.json:76` | 전문가 표시 설명이 계산 메모를 말합니다(P40 뒤에는 늘 보임). | 0.15.2: 초고지연 모드, 장치 공유 방식, 프로젝트 파일의 역할 이름. |
| LOW | LOW(모호성) | `i18n/ko.json:785` | "끄는"은 끌다의 활용으로 맞지만 끄다와 겹칩니다. | 0.15.2: "끌어 옮기는 동안". |

### 확신하지 못한 것

| Location | Suspicion | What would confirm it |
| --- | --- | --- |
| `apps/secs2r/ui/src/screens/calibrate/ResultGraph.tsx:67`, `apps/secs2r/ui/src/screens/calibrate/ResultGraph.tsx:329` | SPL에서 서비스가 보낸 `target` 곡선을 버리고 현재 타겟 점을 직접 보간합니다. 서비스의 레벨 이동이나 오래된 결과의 타겟 기준과 달라질 가능성이 있습니다. | 서비스의 `result_curves` 타겟 생성·레벨 이동 규칙과 현재 점의 좌표계를 대조하고, 타겟 변경 전후의 실제 응답을 비교해야 합니다. |
| `apps/secs2r/ui/src/screens/calibrate/Results.tsx:128`, `apps/secs2r/ui/src/screens/calibrate/Results.tsx:131` | 판정·성적표의 대역을 카탈로그의 20–200 Hz와 20–100 Hz로 고정합니다. 생성 타입의 설명만으로는 서비스 지표가 이 대역을 항상 사용하는지 확정할 수 없습니다. | 서비스가 `VerdictSummary`와 `ChannelResultLine`을 만드는 곳에서 사용하는 지표·대역을 확인해야 합니다. |
| `apps/secs2r/ui/src/screens/calibrate/Settings.tsx:538` | 전체 보정 없이 `서브만 계산`을 마친 경우 BA 결과가 있어도 `결과 값 가져오기`는 전체 `calibration.chain.subs`가 없다는 이유로 꺼집니다. | `sub_take_values`가 독립된 서브 전용 결과를 지원하는지 서비스 계약을 확인해야 합니다. |
| `apps/secs2r/ui/src/screens/connect/ConnectScreen.tsx:120`, `apps/secs2r/ui/src/screens/calibrate/Settings.tsx:274`, `apps/secs2r/ui/src/screens/export/useExportScreen.ts:32` | 스윕 레벨 점검 중 전 채널 재생, 측정 중 XO·서브·MIMO 계산, MIMO 중 내보내기처럼 화면 간 작업 잠금이 일관되지 않습니다. | 실제 서비스의 작업 그룹 상호 배제와 각 요청의 거부·허용 정책을 확인해야 합니다. 버튼이 켜져 있다는 사실만으로 두 작업이 실제 병행되거나 결과가 손상된다고 단정하지 않습니다. |
| `apps/secs2r/ui/src/screens/system/SystemScreen.tsx:696` | DSP 필드 하나를 고칠 때 전체 `dsp.fields`를 보냅니다. 앞선 수정 응답 전에 다른 필드를 고치면 오래된 나머지 필드가 함께 전송될 수 있습니다. | 지연된 IPC 응답에서 연속 편집 순서와 서비스의 전체 필드 교체 정책을 확인해야 합니다. |
| `apps/secs2r/ui/src/screens/calibrate/ResultGraph.tsx:205` | 알려지지 않은 BA 경고는 코드와 JSON이 본문에 그대로 나옵니다. | 실제 서비스가 현재 카탈로그에 없는 BA 경고를 보내는 경로가 확인되면 결함으로 확정할 수 있습니다. |
| `apps/secs2r/ui/src/screens/project/project.css:61`, `apps/secs2r/ui/src/ui/ui.css:1118` | 최근 프로젝트 목록과 공통 대화상자에 높이 제한·본문 스크롤이 없습니다. 최근 파일이 많거나 오류 자세히를 펼치면 버튼이 창 밖으로 나갈 가능성이 있습니다. | 실제 창 1024×700에서 긴 최근 목록과 오류 자세히를 함께 렌더링해 버튼 접근성을 확인해야 합니다. |

(ASTRA가 적은 대로 둡니다. 서비스 계약을 함께 봐야 하는 것이라 P42에서 다룹니다.)

#### P42 판정

아래 줄 번호는 P42a 작업 시점 기준입니다. 원래 의심 표는 바꾸지 않았습니다.

| 위치 | 판정 | 근거 | 처리 |
| --- | --- | --- | --- |
| `ResultGraph.tsx:67,329` SPL 타겟 | 레벨·낡음 의심은 기각, 보간 불일치는 결함 | `secs-service/src/curves.rs:231–259`는 보정 곡선에서 계산 당시 기준 레벨을 빼고 타겟은 **현재** `project.target`을 오프셋 없이 보냅니다. 화면이 현재 점을 쓰는 좌표계는 맞습니다. 그러나 `ui/plot/targetEdit.ts:69–84`는 로그 주파수 선형 보간이고 `secs-types/src/target.rs:108–119`는 `t²(3−2t)` smoothstep입니다. | 허용 범위 밖이므로 수정하지 않습니다. P42b: `targetAt`의 구간 가중치를 smoothstep으로 맞추고 1/4·3/4 지점 회귀 테스트를 추가해야 합니다. `editors.tsx:59–65`의 드래그 중 곡선도 같은 함수로 맞아야 합니다. |
| `Results.tsx:128,131` 지표 대역 | 결함 아님 | `secs-engine/src/scorecard.rs:90–115,151–154,172–183`의 `in1_20`은 20–200 Hz ±1 dB 비율, `gd_20`은 20–100 Hz GD의 90–10 백분위 폭입니다. `calibration.rs:336–340`은 이 값의 평균, `curves.rs:383–391`은 채널별 값을 그대로 냅니다. | 카탈로그의 고정 대역을 유지합니다. 100–400 Hz 정규화 대역이나 RC 설정 대역과 혼동하지 않습니다. |
| `Settings.tsx:538` 서브 값 가져오기 | 현행 계약·업스트림과 일치 | `calibration.rs:61–100`은 서브 전용 결과를 `Results.subs`에 따로 저장합니다. `curves.rs:515–529`의 가져오기는 전체 `calibration`만 읽습니다. 업스트림 `wizard/step_calibration.py:1312–1318,1376`도 `self.run`만 가져오고 전체 결과가 없으면 단추를 끕니다. | 변경하지 않습니다. 전체 결과와 새 서브 전용 결과가 함께 있으면 BA 그래프는 후자(`curves.rs:153–160`), 가져오기는 전자입니다. 독립 서브 결과 지원은 계약·업스트림 동작을 바꾸는 별도 작업입니다. |
| `ConnectScreen.tsx:120`, `Settings.tsx:274`, `useExportScreen.ts:32` 작업 잠금 | 손상 의심은 기각; 레벨 단추의 잠금 누락은 화면 개선 필요 | `jobs.rs:17–34,169–185`는 Device(측정·가져오기·레벨), Compute(보정·XO·MIMO), Export를 별도 무리로 둡니다. 측정과 계산, MIMO와 내보내기는 스냅샷으로 병행합니다(`calibration.rs:66–81,114–134,153–184`, `export.rs:19–45,69–131`). 스윕 점검 중 모니터 시작은 같은 Device 무리라 `job_busy`로 거절되어 이중 재생은 불가능합니다. | 서비스의 무리 경계를 바꾸지 않습니다. P42b: `ConnectScreen.tsx:120`의 전 채널 재생과 개별 재생 단추에 `view.sweepCheck.running`을 잠금 조건으로 더하면 거절될 호출을 미리 막습니다. |
| `SystemScreen.tsx:697` DSP 연속 편집 | 결함, 수정 | `secs-types/src/ipc.rs`의 `DspSetParams`는 완전한 수동 편집이고 `project_edit.rs:568–604`는 다섯 필드 전체를 교체합니다. 같은 렌더의 값으로 만든 두 호출은 서로의 편집을 되돌립니다. | `useSystemScreen.ts`에서 렌더 기준 변경 필드만 추리고 DSP 요청을 직렬화해 최신 서비스 값에 합칩니다. 지연된 첫 응답 중 서로 다른 두 필드를 편집하는 훅 테스트로 두 값 보존을 확인합니다. 마크업은 바꾸지 않습니다. |
| `ResultGraph.tsx:205` 알려지지 않은 BA 경고 | 현재 생산 경로에서는 결함 아님 | `curves.rs:433–438,566–580`과 `secs-export/src/lib.rs:261–291`이 내는 코드는 `sub_narrow_band`, `sub_high_gain`뿐입니다. `screens/calibrate/text.ts:165–185`와 두 카탈로그에 모두 있습니다. | 현재 fallback은 닿지 않으므로 변경하지 않습니다. 새 경고 변형을 추가할 때 번역도 추가해야 합니다. |
| `project.css:61`, `ui.css:1118` 대화상자 높이 | 정적 구조상 결함; 실제 창 판정은 P42b | 최근 목록은 서비스에서 10개로 제한하지만(`project_edit.rs:782–786,809–824`) 행만 최소 520 px입니다. `ui.css:1107–1128,1166–1175`의 중앙 대화상자와 본문에는 높이 제한·스크롤이 없고, 펼친 오류 원문도 제한이 없습니다(`1255–1263`). | P42b 담당이므로 수정하지 않습니다. 뷰포트 높이 제한과 본문 스크롤을 적용한 뒤 1024×700, 최근 10개+긴 오류 원문에서 푸터 접근성을 확인해야 합니다. 이 작업에서는 브라우저 검증하지 않았습니다. |

P42a 연결 후속: 결과 곡선의 요청 선택값·진행 상태·단일 세대 검사를 함께 저장하며, 실패 시 이전 곡선을 지웁니다. 미터는 업스트림 `mic_meter.py:22–24`의 1초 기준으로 재시작합니다. 서비스 `monitor.rs:295–358`에는 읽기 중단 감지·재시작 신호가 없어 화면 세션이 `stop` 완료 뒤 `start`를 직렬화합니다. 자체·모의 경로는 `monitor.rs:202–229`에서 재생할 때만 이벤트를 내므로 쉬는 동안의 무수신이 정상이고, 감시는 REW 경로(`rew_loop`가 REW의 0.25초 콜백을 계속 보냄)에서만 겁니다(Claude 검토에서 고침: 처음 판은 자체·모의 경로도 1초마다 다시 시작했습니다). `CalibrationSummary.warnings`는 업스트림 `CalibrationRun`과 요약 표시(`calibration.py:535–550`, `wizard/step_calibration.py:2298–2303`)에 대응 값이 없고 서비스도 항상 비워 보내던 죽은 필드라 삭제하고 바인딩을 다시 생성했습니다. P42b는 이 필드를 그리지 않습니다.

### 보고 문제없던 것

- 실제 화면의 IPC 호출은 생성된 `Request`·응답 타입을 사용하는 공통 호출기를 통합니다. 화면에서 `as any`로 타입 검사를 우회한 호출은 발견하지 못했습니다.
- 일반 리소스 읽기에는 세대 검사가 있습니다. 다만 별도로 구현된 결과 곡선 읽기는 위에서 보고한 예외입니다.
- 일반 이벤트 버스, 화면 키보드 리스너, 타이머, ResizeObserver, MutationObserver와 uPlot 인스턴스에는 해제 코드가 있습니다.
- 공통 대화상자에는 보이는 닫기·취소와 Esc 처리가 있습니다. 파괴적 구성 변경과 위치 측정 삭제는 결과를 설명한 확인 및 되돌리기로 연결되어 있습니다.
- 공통 숫자 입력은 범위 검증·단위·화살표 증감·Esc 복원을 제공합니다. 성적표와 BA 표의 주요 수치 열에는 tabular figures가 적용됩니다.
- 시스템의 빈 구성·검증 오류, 연결 방식 미선택·REW 오류, 측정 곡선 없음·진행, 보정 결과 없음·낡음·계산 실패, 내보내기 결과 없음·낡음·실패·튜토리얼에는 명시적인 표현이 있습니다. 초기 설정 로딩 실패와 미터 읽기 중단 등 누락 경로는 위에 따로 적었습니다.
- 일반 IPC 오류는 번역된 원인과 다음 행동으로 표시하고 원문은 자세히에 둡니다. 한국어·영어의 주요 작업 라벨, 단위 자리표시자, 오류 안내에서 광범위한 의미 불일치나 해요체 혼용은 발견하지 못했습니다.
- 기본 단계 화면의 주 동작은 채운 버튼 하나로 구분됩니다. 채널 곡선 색은 데이터 구분 용도이고, 일반 상태색은 상태 의미에 사용됩니다. 실제 대비·확대 배율·움직임은 실행하지 않아 판정하지 않았습니다.

### 검증과 판정

- 검증: 고친 자리마다 회귀 테스트(`AppFrame.test.tsx`, `jobStore.test.ts`, `session.test.ts`, `MeasureScreen.test.tsx`, `useSystemScreen.test.tsx`)와 `npm run typecheck`, `npm test`, `npm run build`, `cargo test -p secs-types`(바인딩 재생성).
- **판정(ASTRA 원문):** HIGH 10, MEDIUM 10, LOW 2.
- **판정(대조 뒤):** HIGH 2, MEDIUM 12, LOW 3, 기각 5. HIGH 2건과 MEDIUM 8건, LOW 2건은 0.15.2에서 고쳤고, MEDIUM 4건과 LOW 1건은 [`packets/P42-m5-followups.md`](packets/P42-m5-followups.md)로 넘깁니다.
- **M5 §6 화면 리뷰 조건:** 두 리뷰(Claude 갤러리, ASTRA 코드) 모두 남은 HIGH가 없습니다. 충족.
