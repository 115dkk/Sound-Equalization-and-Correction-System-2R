# P178 ASTRA: MIMO 초기 구간의 화면 연결과 문구

ADR 0037(`docs/adr/0037-mimo-early.md`)의 화면 부분을 연결하십시오. 작업 트리는 `E:\wt\p1-mimo-early`(브랜치 `claude/p1-mimo-early`)이고 그 안에서만 일하십시오. `crates/**`, `features.toml`, `apps/secs2r/ui/src/ipc/bindings/**`, `apps/secs2r/ui/src/ipc/recorded/**`는 읽기만 하십시오.

## 배경

- 보정 설정에 `mimo_early_ms: number`(기본 60, 한도 `CALIBRATION_LIMITS.mimo_early_ms` = [5, 60])가 생겼습니다. 바인딩 `ipc/bindings/CalibrationSettings.ts`, `CalibrationLimits.ts`, `rules.ts`, `SettingEdit.ts`(`{ kind: "mimo_early", ms }`)를 읽어 확인하십시오. 숫자 설정의 본보기는 `fsiso`와 `mimo_max`이고, 이 설정은 `mimo_key`에만 들어가므로 바꿔도 보정 결과는 낡지 않고 MIMO 결과만 낡습니다(모의 서비스도 그렇게 해야 합니다).
- Claude가 MIMO 카드의 '고급'에 숫자 입력 마크업을 그렸습니다(`screens/calibrate/Settings.tsx`, `data-ui="calibrate.mimo.early"`). 입력은 `actions.changeSetting({ kind: "mimo_early", ms })`를 부릅니다.

## 할 일

1. `i18n/ko.json`과 `i18n/en.json`에 키를 추가하십시오. 합니다체, 직역 없이.
   - `calibrate.mimo.early`: 라벨 '초기 구간'.
   - `calibrate.mimo.early.hint`: 서포트가 온셋 뒤 작용하기 시작하는 시점(ms)이고, 줄이면 초기 울림이 짧아질 수 있는 대신 서포트 구동이 늘고 가드에 더 자주 걸린다는 것, 60이 지금까지의 동작이라는 것.
2. 모의 서비스(`ipc/mock.ts`)의 보정 설정에 `mimo_early_ms: 60`을 넣고(기록 응답에 이미 있으면 중복 정의하지 않음), `mimo_early` 편집을 받아 5~60 밖이면 실제 서비스처럼 거부하고 안이면 저장하되 MIMO 결과만 낡게 하십시오. 보정 화면 fixtures에도 키를 넣으십시오.
3. 시험을 추가하십시오. `CalibrateScreen.test.tsx` 또는 `useCalibrateScreen.test.tsx`(입력을 바꾸면 `calibration_settings_edit`이 `{ kind: "mimo_early", ms: 20 }`으로 불리고 MIMO 결과만 낡음, 범위 밖 값은 거부), `mock.test.ts`·`mockWiring.test.ts`(설정 키와 편집 변형을 세면 갱신).

## 지킬 것

마크업과 스타일은 고치지 마십시오(`Settings.tsx`는 호출 코드만). 화면 문구는 합니다체, 상태와 라벨은 명사구. em-dash와 이모지 금지.

## 허용 파일

`apps/secs2r/ui/src/screens/calibrate/{types.ts,text.ts,text.test.ts,CalibrateScreen.test.tsx,useCalibrateScreen.test.tsx,fixtures.ts,settingsView.ts}`, `apps/secs2r/ui/src/i18n/{ko.json,en.json}`, `apps/secs2r/ui/src/ipc/{mock.ts,mock.test.ts,mockWiring.test.ts}`, `apps/secs2r/ui/src/state/**`. `Settings.tsx`는 호출 코드만.

## 검증

`E:\wt\p1-mimo-early\apps\secs2r\ui`에서 Node 24로 포그라운드 실행하고 끝나기 전에 턴을 끝내지 마십시오.

```sh
npm ci
npm run typecheck
npm test
npm run build
```

## 보고

바꾼 파일과 한 일 한 줄씩, 추가한 키, 추가한 시험 이름, 위 명령의 통과 요약, 남은 문제.
