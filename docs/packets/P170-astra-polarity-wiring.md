# P170 ASTRA: 극성 경고의 화면 연결과 문구

ADR 0033(`docs/adr/0033-speaker-polarity.md`)의 화면 부분을 연결하십시오. 작업 트리는 `E:\wt\p1-polarity`(브랜치 `claude/p1-polarity`)이고, 그 안에서만 일하십시오. 같은 트리에서 Daybreak가 `crates/**`를 고치고 있으니 `crates/**`와 `features.toml`, `apps/secs2r/ui/src/ipc/bindings/**`는 읽기만 하십시오.

## 배경

- 엔진은 보정마다 메인 채널의 상대 극성을 재어 두 메모를 남깁니다. 서비스는 이것을 `Notice { code, params }`로 바꿉니다(작업서 P169).
  - `polarity_reversed`: params `role`(역할 코드, 예 `SR`), `confidence`(0~1의 소수, 문자열), `compared`(비교한 스피커 수, 문자열). 다른 스피커 다수와 반대 부호인 메인 하나.
  - `polarity_split`: params `a`, `b`(역할을 쉼표로 이은 문자열, 예 `L,R`과 `SL,SR`), `confidence`. 두 묶음이 서로 반대인데 어느 쪽이 맞는지 알 수 없는 경우(메인이 둘뿐이거나 둘씩 갈린 경우).
- 극성 설정은 시스템 화면 패치 시트의 극성 단추(`apps/secs2r/ui/src/screens/system/SystemScreen.tsx:457-475`, `data-ui="system.patch.<역할>.pol"`)이고, IPC는 `config_set_speaker { index, band_index: null, pol: "+" | "-" }`입니다(`useSystemScreen.ts:22-32`의 `setPolarity`). 스피커 `index`는 `config_get` 결과의 `speakers` 순서입니다.
- 극성을 바꾸면 결과는 낡은 결과가 되므로(`config_key`), 사용자는 '다시 보정하기'를 누릅니다.
- 보정 화면의 판정 카드 마크업은 Claude가 그렸습니다(`apps/secs2r/ui/src/screens/calibrate/Results.tsx`의 `Verdict`). 그 마크업은 아래 셋을 부릅니다.
  - `polarityWarnings(result.notes)`(`screens/calibrate/text.ts`): `polarity_reversed`와 `polarity_split` 메모만 고르는 필터. 이미 있습니다.
  - `actions.flipPolarity(role)`: 그 역할 스피커의 첫 대역 극성을 지금 값의 반대로 바꿉니다.
  - `actions.openPolarity(role)`: 시스템 화면의 그 역할 극성 단추로 이동합니다(`polarity_split`에서는 첫 묶음의 첫 역할).
- 설정으로 이동하는 도우미는 `apps/secs2r/ui/src/App.tsx:53`의 `navigateToSetting(step, selector, fallback)`이고, `openPatch`(`App.tsx:96`)가 본보기입니다. 보정 화면 훅은 `useCalibrateScreen(goToMeasurements)`(`screens/calibrate/useCalibrateScreen.ts:12`)이며 `App.tsx:97`에서 만듭니다.

## 할 일

1. `screens/calibrate/types.ts`의 `CalibrateActions`에 `flipPolarity: (role: string) => void`와 `openPolarity: (role: string) => void`를 추가하십시오(Claude가 이미 넣었으면 그대로 두십시오).
2. `useCalibrateScreen.ts`에 두 동작을 구현하십시오. `flipPolarity`는 `config_get`으로 스피커 목록을 읽어 역할이 같은 스피커의 `index`와 첫 대역의 `pol`을 찾고, `config_set_speaker { index, role: null, size: null, band_index: null, out: null, pol: 반대값 }`을 부릅니다. 역할을 찾지 못하면 아무것도 하지 않습니다. 호출이 끝나면 결과가 낡은 결과로 바뀌는 것은 서비스가 알리므로 화면은 더 할 일이 없습니다. `openPolarity`는 `App.tsx`에서 `navigateToSetting("system", '[data-ui="system.patch.<역할>.pol"]', '[data-ui^="system.patch."][data-ui*=".pol"]')`을 넘겨받아 부릅니다. `useCalibrateScreen`의 시그니처에 이 이동 함수를 추가하고 `App.tsx:97`을 고치십시오.
3. `screens/calibrate/text.ts`의 `noticeText`에 두 코드의 문장을 추가하십시오. 역할은 `roleCode`로, `confidence`는 소수 둘째 자리까지, `compared`는 정수로 보입니다. `polarity_split`의 `a`, `b`는 쉼표로 나눠 역할 코드로 바꾸고 '·'로 잇습니다(예 `L·R`). 다음 단계 문구의 키 `POLARITY_NEXT`와 필터 `polarityWarnings`, 타입 `PolarityCode`는 Claude가 `LOW_PROTECT_NEXT` 아래에 이미 넣었으니 그대로 쓰고, 키 이름만 카탈로그와 맞추십시오.
4. `i18n/ko.json`과 `i18n/en.json`에 키를 추가하십시오. 한국어는 합니다체, 영어는 같은 뜻으로 쓰되 직역하지 마십시오. 키 이름은 다음과 같습니다.
   - `notice.polarity_reversed`: `{code}`의 직접음이 다른 스피커 `{compared}`개와 반대 부호라는 뜻과 상관 `{confidence}`를 담습니다. 예: "극성 반전 의심: {code}의 직접음이 다른 스피커 {compared}개와 반대 부호입니다(상관 {confidence})."
   - `notice.polarity_reversed.next`: 스피커 선의 +/−를 확인하거나 극성 설정을 뒤집은 뒤 다시 보정하라는 뜻.
   - `notice.polarity_split`: `{a}`와 `{b}`의 직접음 부호가 서로 반대이고 어느 쪽이 맞는지는 알 수 없다는 뜻, 상관 `{confidence}`.
   - `notice.polarity_split.next`: 스피커 선의 +/−를 확인하라는 뜻.
   - `calibrate.polarity.flip`: 단추 문구, "{code} 극성 뒤집기".
   - `calibrate.polarity.open`: 단추 문구, 패치 시트의 극성 칸으로 간다는 뜻.
   두 카탈로그의 키 집합과 자리표시자 집합은 같아야 합니다(`i18n/i18n.test.ts`가 확인합니다).
5. 모의 서비스(`ipc/mock.ts`)의 보정 결과 가운데 하나에 `polarity_reversed` 메모(`SR`, confidence `0.94`, compared `3`)를 넣어 갤러리와 시험에서 경고가 보이게 하십시오. 기존 시험의 기대값이 바뀌면 그 시험을 고치되, 메모 수를 세는 시험이 있으면 수를 맞추십시오.
6. 시험을 추가하십시오.
   - `screens/calibrate/text.test.ts`: 두 코드의 문장(역할 코드, 상관 둘째 자리, 묶음 표기)과 '모든 코드의 문장이 있다' 시험(`text.test.ts:205`)의 목록 갱신.
   - `screens/calibrate/CalibrateScreen.test.tsx` 또는 `useCalibrateScreen.test.tsx`: `polarity_reversed` 메모가 있으면 경고가 보이고, 단추를 누르면 `config_set_speaker`가 `{ index, band_index: null, pol: "-" }`로 불리는 것(모의 IPC 호출 기록으로 확인), `polarity_split`이면 이동 함수가 그 역할 선택자로 불리는 것.
   - `ipc/mock.test.ts`가 메모 수를 고정하고 있으면 갱신.

## 지킬 것

- 마크업과 스타일은 고치지 마십시오. `Results.tsx`에서 고쳐도 되는 것은 호출 코드뿐이고, 새 컴포넌트나 클래스는 만들지 마십시오.
- 화면 문구는 합니다체, 상태와 라벨은 명사구, 효과를 말하는 라벨(`CLAUDE.md`의 화면 규칙). em-dash와 이모지는 쓰지 마십시오.
- `ipc/bindings/**`는 ts-rs가 만드는 파일이니 손대지 마십시오.

## 허용 파일

`apps/secs2r/ui/src/screens/calibrate/{types.ts,useCalibrateScreen.ts,useCalibrateScreen.test.tsx,text.ts,text.test.ts,CalibrateScreen.test.tsx}`, `apps/secs2r/ui/src/App.tsx`, `apps/secs2r/ui/src/i18n/{ko.json,en.json}`, `apps/secs2r/ui/src/ipc/{mock.ts,mock.test.ts,mockWiring.test.ts}`, `apps/secs2r/ui/src/state/**`의 시험. `Results.tsx`는 호출 코드만.

## 검증

`E:\wt\p1-polarity\apps\secs2r\ui`에서 포그라운드로 돌리고, 끝나기 전에 턴을 끝내지 마십시오. Node 24(npm 11)를 쓰십시오.

```sh
npm ci
npm run typecheck
npm test
npm run build
```

## 보고

바꾼 파일과 한 일 한 줄씩, 추가한 키 목록, 추가한 시험 이름, 위 명령의 통과 요약, 남은 문제.
