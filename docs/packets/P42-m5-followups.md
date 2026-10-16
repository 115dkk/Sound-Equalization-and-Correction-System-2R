# P42: M5 감사 후속 (ASTRA 연결 · Opus 화면)

작업서로 나눠 0.15.3에서 끝냈습니다: 연결 [`P42a`](P42a-astra-wiring.md), 화면 [`P42b`](P42b-opus-screens.md), 투어가 찾은 결함 [`P42c`](P42c-astra-tour-findings.md). 남은 것은 [`P44`](P44-tour-followups.md)입니다.

`docs/M5-review.md`의 ASTRA 감사에서 성립했지만 0.15.2에 넣지 않은 것입니다. M5 종료 조건(HIGH 없음)과는 무관하고, 다음 화면 묶음에서 다룹니다. 모두 `apps/secs2r/ui/src/` 안이며, 커밋은 Claude가 합니다.

## Opus 5.5 (화면)

1. **튜토리얼 포커스.** `screens/tutorial/Tutorial.tsx`: 코치 카드가 열릴 때 첫 포커스와 허용 표적 사이의 이동을 정하고, 완료 카드(`EndCard`)로 포커스를 옮기며, 완료 상태에서도 Esc가 종료와 이어지게 합니다(지금은 `engaged`가 거짓이 되어 리스너가 빠짐).
2. **이어 하기의 되감기.** `App.tsx`가 `TutorialLayer`에 `stepOpen`을 넘깁니다(`engine.ts:resumeIndex`). 뜻은 "그 단계를 떠날 수 있는가"이고, 틀의 단계 상태(`useFrameModel`)로 판단합니다. 도달표의 "막힌 단계로 되감기"가 이것입니다.
3. **곡선 불러오는 중.** `screens/calibrate/ResultGraph.tsx`: 채널을 바꾼 뒤 새 곡선이 올 때까지 이전 채널 곡선을 그대로 두지 않습니다. 이전 곡선을 흐리게 두고 "불러오는 중"을 표시하거나 비우되, 깜빡임이 없어야 합니다. 상태값은 아래 ASTRA 항목이 줍니다.
4. **캡션 크기.** `screens/export/export.css`의 `export-fact dt`·`export-line dt`(라벨)를 `--text-ui`로. 막힌 이유(`rail-step__reason`)와 지도 채널 코드는 캡션으로 둡니다.
5. **떠 있는 마이크 미터.** `App.tsx`의 `app.mic`: 열릴 때 포커스 이동, Esc로 닫기, 닫은 뒤 포커스 복귀.

## ASTRA (연결·상태)

1. **곡선의 선택값.** `state/session.ts:refreshCurves`가 요청한 `tab`·`channel`·`group`을 결과와 함께 두고(`ResultCurvesResult`에는 채널이 없음), `useCalibrateScreen`이 현재 선택과 맞는 곡선만 `view.curves`로 내며 불러오는 중 플래그를 냅니다.
2. **미터 `stalled`.** `state/session.ts`: 마지막 Meter 이벤트 시각을 기억해 일정 시간(업스트림 `LevelMonitor`의 stale 기준) 새 읽기가 없으면 `monitor: "stalled"`로 바꾸고, 그 문구가 말하는 재시작(`level_monitor_stop` 뒤 `start`)을 실제로 잇습니다.
3. **`CalibrationSummary.warnings`.** 서비스가 채우기 시작하면 판정 카드에 원인·다음 행동으로 붙입니다. 지금은 늘 비어 있습니다(`calibration.rs:348`).
4. **확신하지 못한 것 7건.** `docs/M5-review.md` "확신하지 못한 것" 표를 서비스 계약과 대조해 결함이면 고치고 아니면 그 표에 이유를 적습니다.

## 검증

`CLAUDE.md`의 검사 전부. 화면을 바꾸면 `CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/gallery-shots.mjs`로 갤러리를 다시 찍고 바뀐 견본을 봅니다.
