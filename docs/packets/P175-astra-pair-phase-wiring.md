# P175 ASTRA: 채널 사이 위상 메모의 화면 문구

ADR 0035(`docs/adr/0035-pair-phase.md`)의 화면 부분을 연결하십시오. 작업 트리는 `E:\wt\p1-pair-phase`(브랜치 `claude/p1-pair-phase`)이고 그 안에서만 일하십시오. `crates/**`, `features.toml`, `apps/secs2r/ui/src/ipc/bindings/**`는 읽기만 하십시오.

## 배경

- 엔진 메모 `pair_phase`: params `a`, `b`(역할 코드), `bands`(`"lo:hi:phase:loss,..."`, 주파수 정수, 위상과 상쇄 소수 첫째 자리). 상쇄가 −6 dB 이하인 대역만 들어 있고, 짝은 가장 나쁜 순으로 6개까지입니다.
- 보정 화면 판정 카드의 계산 메모 목록은 `screens/calibrate/text.ts`의 `noteLines`가 만들고 `Results.tsx`가 `<li>`로 보입니다. 이 메모는 경고가 아니라 메모이므로 그 목록에 들어갑니다.
- 저음 도착 맞추기 설정은 `view.settings.lf_align`입니다.

## 할 일

1. `noticeText`에 `pair_phase` 문장을 추가하십시오. 대역이 여럿이면 이어진 대역(앞 대역의 `hi`가 다음 대역의 `lo`와 같음)을 한 구간으로 묶고, 구간마다 가장 나쁜 상쇄의 대역에서 위상차와 상쇄를 가져오십시오. 문장은 `notice.pair_phase`(`채널 사이 위상({a}·{b}): {spans}`)로 만들고, 구간 하나는 `notice.pair_phase.span`(`{lo}~{hi} Hz에서 위상차 {phase}°, 함께 울리면 {loss} dB`)으로 만들어 구간 사이는 쉼표로 이으십시오. 역할은 `roleCode`로 바꾸고, 위상과 상쇄는 정수로 표시하십시오.
2. `noteLines`가 `pair_phase` 메모를 하나라도 받았고 `lf_align`이 꺼져 있으면 목록 끝에 `notice.pair_phase.hint`(저음 도착 맞추기를 켜면 메인끼리의 저역 위상이 가까워진다는 뜻) 한 줄을 넣게 하십시오. `noteLines`에 `lfAlign: boolean` 인자를 추가하고 `Results.tsx`의 호출(`noteLines(t, number, ...)`)에서 `view.settings.lf_align`을 넘기십시오. 켜져 있으면 안내 줄을 넣지 마십시오.
3. `i18n/ko.json`과 `i18n/en.json`에 `notice.pair_phase`, `notice.pair_phase.span`, `notice.pair_phase.hint`를 추가하십시오. 합니다체, 직역 없이.
4. 모의 서비스(`ipc/mock.ts`)의 보정 결과 가운데 하나에 `pair_phase` 메모(`L`, `Ls`, `"89:112:170.0:-20.3,112:141:174.0:-24.1"`)를 넣어 갤러리와 시험에서 보이게 하십시오. 기존 시험이 메모 수를 세면 수를 맞추십시오.
5. 시험을 추가하십시오. `text.test.ts`(두 언어의 문장, 이어진 대역 묶기, 안내 줄 유무, 모든 코드 문장 존재), `CalibrateScreen.test.tsx` 또는 `useCalibrateScreen.test.tsx`(메모 목록에 문장과 안내 줄이 보이는 것).

## 지킬 것

마크업과 스타일은 고치지 마십시오(`Results.tsx`에서는 `noteLines` 호출 인자만). 화면 문구는 합니다체. em-dash와 이모지 금지.

## 허용 파일

`apps/secs2r/ui/src/screens/calibrate/{text.ts,text.test.ts,CalibrateScreen.test.tsx,useCalibrateScreen.test.tsx}`, `apps/secs2r/ui/src/i18n/{ko.json,en.json}`, `apps/secs2r/ui/src/ipc/{mock.ts,mock.test.ts,mockWiring.test.ts}`. `Results.tsx`는 `noteLines` 호출 인자만.

## 검증

`E:\wt\p1-pair-phase\apps\secs2r\ui`에서 Node 24로 포그라운드 실행하고 끝나기 전에 턴을 끝내지 마십시오.

```sh
npm ci
npm run typecheck
npm test
npm run build
```

## 보고

바꾼 파일과 한 일 한 줄씩, 추가한 키, 추가한 시험 이름, 위 명령의 통과 요약, 남은 문제.
