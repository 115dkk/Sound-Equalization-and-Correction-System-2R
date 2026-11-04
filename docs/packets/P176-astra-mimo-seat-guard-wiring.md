# P176 ASTRA: MIMO 위치별 가드의 화면 연결과 문구

ADR 0036(`docs/adr/0036-mimo-seat-guard.md`)의 화면 부분을 연결하십시오. 작업 트리는 `E:\wt\p1-seat-guard`(브랜치 `claude/p1-seat-guard`)이고 그 안에서만 일하십시오. `crates/**`, `features.toml`, `apps/secs2r/ui/src/ipc/bindings/**`, `apps/secs2r/ui/src/ipc/recorded/**`는 읽기만 하십시오(Daybreak가 P174에서 고쳤고 바인딩과 기록 응답은 생성기가 만듭니다).

## 배경

- 보정 설정에 `mimo_seat_guard: boolean`(기본 `false`)이 생겼습니다. 바인딩 `ipc/bindings/CalibrationSettings.ts`와 `SettingEdit.ts`(`{ kind: "mimo_seat_guard", on }`)를 읽어 확인하십시오. `mimo_fir`가 같은 꼴의 본보기이고, 이 설정은 `mimo_key`에만 들어가므로 바꿔도 보정 결과는 낡지 않고 MIMO 결과만 낡습니다(모의 서비스도 그렇게 해야 합니다).
- 서포트를 줄이거나 끈 MIMO 알림(`mimo_guard_reduced`, `mimo_guard_disabled`)에 선택 매개변수 `position`(측정 위치 번호, 문자열)이 생겼습니다. 설정이 켜져 있을 때만 들어 있습니다. 이 알림이 화면 어디서 어떤 키로 문장이 되는지 먼저 찾으십시오(`screens/calibrate/text.ts`, `i18n/ko.json`의 `notice.*`, 서비스가 만드는 `result_lines`의 MIMO 줄 등). 찾은 자리에서 `position`이 있으면 '위치 3에서'처럼 그 자리를 문장 앞에 붙이고, 없으면 지금 문장 그대로 두십시오.
- Claude가 MIMO 카드의 '고급' 위에 토글 마크업을 그렸습니다(`screens/calibrate/Settings.tsx`, `data-ui="calibrate.mimo.seat_guard"`). 토글은 `actions.changeSetting({ kind: "mimo_seat_guard", on })`을 부릅니다.

## 할 일

1. `i18n/ko.json`과 `i18n/en.json`에 키를 추가하십시오. 합니다체, 직역 없이.
   - `calibrate.mimo.seat_guard`: 라벨 "위치별 가드".
   - `calibrate.mimo.seat_guard.hint`: 서포트를 줄일지 정할 때 모든 위치의 합 대신 가장 나빠지는 위치를 본다는 것, 한 자리가 나빠지는 설계를 막는 대신 서포트가 더 자주 줄어든다는 것.
   - 가드 알림의 위치 표기에 필요한 키(예: `notice.mimo_guard.position`, "위치 {position}에서 ")를 위의 조사 결과에 맞춰 추가하십시오.
2. 모의 서비스(`ipc/mock.ts`)의 보정 설정에 `mimo_seat_guard: false`를 넣고, 편집을 받아 저장하되 보정 결과가 아니라 MIMO 결과만 낡게 하십시오(`mimo_fir`와 같은 방식). 보정 화면 fixtures에도 키를 넣으십시오.
3. 시험을 추가하십시오. `CalibrateScreen.test.tsx` 또는 `useCalibrateScreen.test.tsx`(토글을 누르면 `calibration_settings_edit`이 `{ kind: "mimo_seat_guard", on: true }`로 불리고 MIMO 결과만 낡음), `text.test.ts`(위치가 있는 가드 알림과 없는 알림의 문장), `mock.test.ts`·`mockWiring.test.ts`(설정 키를 세면 갱신).

## 지킬 것

마크업과 스타일은 고치지 마십시오(`Settings.tsx`는 호출 코드만). 화면 문구는 합니다체, 상태와 라벨은 명사구. em-dash와 이모지 금지.

## 허용 파일

`apps/secs2r/ui/src/screens/calibrate/{types.ts,text.ts,text.test.ts,CalibrateScreen.test.tsx,useCalibrateScreen.test.tsx,fixtures.ts,settingsView.ts}`, `apps/secs2r/ui/src/i18n/{ko.json,en.json}`, `apps/secs2r/ui/src/ipc/{mock.ts,mock.test.ts,mockWiring.test.ts}`, `apps/secs2r/ui/src/state/**`. `Settings.tsx`는 호출 코드만.

## 검증

`E:\wt\p1-seat-guard\apps\secs2r\ui`에서 Node 24로 포그라운드 실행하고 끝나기 전에 턴을 끝내지 마십시오.

```sh
npm ci
npm run typecheck
npm test
npm run build
```

## 보고

바꾼 파일과 한 일 한 줄씩, 가드 알림을 찾은 자리, 추가한 키, 추가한 시험 이름, 위 명령의 통과 요약, 남은 문제.
