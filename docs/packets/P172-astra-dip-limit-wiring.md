# P172 ASTRA: 딥 보정 제한의 화면 연결과 문구

ADR 0034(`docs/adr/0034-dip-limit.md`)의 화면 부분을 연결하십시오. 작업 트리는 `E:\wt\p1-dip-limit`(브랜치 `claude/p1-dip-limit`)이고 그 안에서만 일하십시오. `crates/**`, `features.toml`, `apps/secs2r/ui/src/ipc/bindings/**`는 읽기만 하십시오(Daybreak가 P171에서 고쳤고 바인딩은 ts-rs가 만듭니다).

## 배경

- 보정 설정에 `dip_limit: boolean`(기본 `false`)이 생겼습니다. 바인딩 `ipc/bindings/CalibrationSettings.ts`와 `SettingEdit.ts`를 읽어 확인하십시오. 편집의 `kind` 철자는 바인딩이 정합니다(`low_protect`처럼 snake_case일 것입니다). `hf_phase`가 같은 꼴의 본보기입니다.
- 엔진 메모 `dip_limit`: params `role`(역할 코드), `count`(줄인 딥 수), `dips`(`"hz:depth:fill,hz:depth:fill"`, 소수 둘째 자리). 켜져 있으면 메인마다 하나이고, 줄인 딥이 없으면 `count`가 `0`이고 `dips`가 빈 문자열입니다.
- Claude가 보정 범위 카드의 '극저역 보호' 아래에 토글 마크업을 그렸습니다(`screens/calibrate/Settings.tsx`, `data-ui="calibrate.range.dip_limit"`). 토글은 `change({ kind: "dip_limit", on })`을 부릅니다.

## 할 일

1. `screens/calibrate/types.ts`의 `SettingChange`에 `{ kind: "dip_limit"; on: boolean }`을 추가하고, 설정 변경을 `SettingEdit`으로 바꾸는 자리가 있으면(`hf_phase`를 그대로 따라) 거기에도 연결하십시오. 보정 범위 카드는 `SettingEdit`을 바로 넘기므로 토글은 바인딩의 철자로 부릅니다.
2. `screens/calibrate/text.ts`의 `noticeText`에 `dip_limit` 문장을 추가하십시오. `count`가 0이면 `notice.dip_limit_none`을 쓰고, 아니면 `notice.dip_limit`에 딥 목록을 넣으십시오. 딥 하나는 `notice.dip_limit.dip`(`{hz} Hz {depth} dB 딥을 {fill} dB까지`)로 만들고 쉼표로 이으십시오. 주파수는 정수, dB는 소수 첫째 자리입니다.
3. `i18n/ko.json`과 `i18n/en.json`에 키를 추가하십시오. 합니다체, 직역 없이.
   - `calibrate.range.dip_limit`: 라벨 "딥 보정 제한".
   - `calibrate.range.dip_limit.help`: 1옥타브 평균보다 깊이 꺼진 딥은 2 dB까지 그대로 메우고 그보다 깊은 부분은 절반만, 모두 6 dB까지만 메운다는 것, 한 스피커가 혼자 메우기 어려운 상쇄 딥에서 구동과 왜곡을 줄인다는 것.
   - `notice.dip_limit`: "딥 보정 제한({code}): {dips}".
   - `notice.dip_limit.dip`: "{hz} Hz {depth} dB 딥을 {fill} dB까지".
   - `notice.dip_limit_none`: "딥 보정 제한({code}): 줄인 딥 없음".
4. 모의 서비스(`ipc/mock.ts`)의 보정 설정에 `dip_limit: false`를 넣고, `DipLimit` 편집을 받아 저장하며 결과를 낡은 결과로 표시하게 하십시오(`hf_phase`와 같은 방식). 기록된 설정 자료(`ipc/recorded/calibration_settings_get.json`)와 보정 화면 fixtures에도 키를 넣으십시오.
5. 시험을 추가하십시오. `text.test.ts`(두 코드의 문장, 모든 코드 문장 존재), `CalibrateScreen.test.tsx` 또는 `useCalibrateScreen.test.tsx`(토글을 누르면 `calibration_settings_edit`이 바인딩 철자의 `dip_limit` 편집(`on: true`)으로 불림), `mock.test.ts`·`mockWiring.test.ts`가 설정 키를 세면 갱신.

## 지킬 것

마크업과 스타일은 고치지 마십시오(`Settings.tsx`에서는 호출 코드만). 화면 문구는 합니다체, 상태와 라벨은 명사구. em-dash와 이모지 금지. `ipc/bindings/**`는 손대지 마십시오.

## 허용 파일

`apps/secs2r/ui/src/screens/calibrate/{types.ts,text.ts,text.test.ts,CalibrateScreen.test.tsx,useCalibrateScreen.test.tsx,fixtures.ts,settingsView.ts}`, `apps/secs2r/ui/src/i18n/{ko.json,en.json}`, `apps/secs2r/ui/src/ipc/{mock.ts,mock.test.ts,mockWiring.test.ts}`, `apps/secs2r/ui/src/ipc/recorded/calibration_settings_get.json`, `apps/secs2r/ui/src/state/**`. `Settings.tsx`는 호출 코드만.

## 검증

`E:\wt\p1-dip-limit\apps\secs2r\ui`에서 Node 24로 포그라운드 실행하고 끝나기 전에 턴을 끝내지 마십시오.

```sh
npm ci
npm run typecheck
npm test
npm run build
```

## 보고

바꾼 파일과 한 일 한 줄씩, 추가한 키, 추가한 시험 이름, 위 명령의 통과 요약, 남은 문제.
