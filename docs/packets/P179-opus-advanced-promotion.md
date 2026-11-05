# P179 Opus: '고급'에 둔 설정 셋을 카드 본문으로 올리기

ADR 0038(`docs/adr/0038-advanced-promotion.md`)의 화면 부분을 구현하십시오. 작업 트리는 `E:\wt\advanced-promote`(브랜치 `claude/advanced-promote`)이고 그 안에서만 일하십시오. 이 작업은 화면만 바꾸고 IPC와 엔진, 카탈로그 키는 바꾸지 않습니다.

## 배경

유지보수자가 '고급' 아래의 설정 가운데 셋을 카드 본문으로 올리기로 정했습니다. 안정 id는 모두 그대로이고, 옮긴 자리와 노출 조건은 ADR 0038의 '올리는 셋과 노출 조건'에 있습니다. 화면 규칙은 `docs/ARCHITECTURE.md`와 CLAUDE.md의 make-interfaces-feel-better 항목을 따르십시오.

## 할 일

1. `apps/secs2r/ui/src/screens/calibrate/Settings.tsx`
   - `RangeSection`: `calibrate.range.boost` NumberField를 `<Advanced id="calibrate.range.advanced">` 밖으로 꺼내 `calibrate.range.dip_limit` Toggle 바로 위에 두십시오. 지금처럼 `calibrate-fields` 한 줄로 감싸십시오. '고급'에는 해석적 미분만 남으므로 `count={1}`로 고치고, `defaultOpen={advancedOpen || analytic !== "off"}`는 그대로 두십시오. 주석도 고치십시오.
   - `BaSection`: `calibrate.ba.edit` Segmented, `SubEditor`, `anyFixed`일 때의 `calibrate.ba.main_delay`를 `<Advanced id="calibrate.ba.advanced">` 밖으로 꺼내 경고 목록(`calibrate.ba.settings_warnings`) 바로 아래, '고급' 바로 위에 두십시오. '고급'에는 인프라 서브 fieldset만 남습니다. `defaultOpen={editSub !== undefined}`는 더 이상 뜻이 없으므로 빼고 `count={1}`을 주십시오. `editSub`는 지금처럼 처음 고를 서브로만 쓰십시오. 편집기는 카드가 보이면 항상 보입니다(ADR 0038 3항).
2. `apps/secs2r/ui/src/screens/measure/MeasureScreen.tsx`
   - `MeasureOptions`: `measure.options.sweep` Select와 `repeatable`일 때의 `measure.options.repeats` Segmented를 `<Advanced id="measure.options.advanced">` 밖으로 꺼내 `measure.options.delay` 아래에 두십시오. 비게 된 Advanced와 `measure-options__advanced` 래퍼, `advancedOpen` prop을 지우십시오. 래퍼의 CSS 클래스가 스타일 파일에 있으면 그 규칙도 지우고, 본문에서 필요한 간격은 카드의 기존 규칙으로 맞추십시오.
3. 시험을 고치십시오.
   - `screens/calibrate/CalibrateScreen.test.tsx`: 311·314와 1283~1346 근처의 `calibrate.range.advanced` 단정은 부스트 상한을 펴지 않고 바꿀 수 있다는 단정으로 바꾸고(522의 `commitNumber("calibrate.range.boost", "6")`가 '고급'을 열지 않고 통과해야 함), '고급'의 개수 글자가 1개임을 확인하십시오. 544의 `calibrate.ba.main_delay`와 1230·1246의 reach id 목록은 '고급' 없이 닿아야 합니다. '고급'을 펴면 `calibrate.ba.infra`만 있고 `calibrate.ba.edit`은 밖에 있음을 단정하십시오.
   - `screens/calibrate/useCalibrateScreen.test.tsx` 519 근처: `calibrate.range.advanced` 안에 토글이 없다는 단정이 여전히 맞는지 보고, 필요하면 부스트 상한도 밖에 있다는 단정을 추가하십시오.
   - `screens/measure/MeasureScreen.test.tsx` 325~347: `measure.options.advanced` 클릭을 빼고, `byUi("measure.options.advanced")`가 null임을 단정하십시오. 60~61의 reach id 목록은 그대로입니다.
4. 투어와 갤러리를 고치십시오.
   - `gallery/tour/scenes.ts`의 `calibrate.sub` 장면: `click(ui("calibrate.ba.advanced"))`를 빼고, note를 ko `"서브 정렬에서 고정 방식으로 바꾼 SW2를 고른 상태입니다. 값과 고정 필터가 카드 본문에 보입니다."`, en `"SW2, switched to the fixed method, is chosen in the BA card; its values and fixed filters sit in the card body."`로 바꾸십시오. `calibrate.analytic` 장면은 그대로 두십시오('고급'에 남는 항목).
   - `screens/calibrate/specimens.tsx` 324~325: 제목에서 "고급 open"과 "BA 고급"을 빼고 본문에 있다고 적으십시오(예: `"BA settings: a fixed sub chosen in the card body"`, `"BA editor: values, fixed filters and MIMO support of a fixed sub"`).
   - `screens/measure/specimens.tsx` 192 근처: `advancedOpen` prop을 빼고 제목을 `"Measure · options (acoustic timing: one repetition)"`로 바꾸십시오.
5. `docs/M5-reach.md`의 `calibrate.range.boost`, `measure.options.sweep`, `measure.options.repeats`, `calibrate.ba.edit`, `calibrate.ba.main_delay` 행의 작업서 칸에 `, P179 (ADR 0038)`를 덧붙이십시오. 항목 글에 '고급'이 있으면 본문으로 고치십시오.

## 지킬 것

- 안정 id와 `data-ui`는 바꾸지 마십시오. `measure.options.advanced`만 사라집니다.
- 카탈로그(`i18n/ko.json`, `en.json`)는 바꾸지 마십시오. 새 문구가 필요하면 멈추고 보고하십시오.
- 마크업은 이웃 컴포넌트(`NumberField`, `Segmented`, `Toggle`, `Advanced`)의 쓰임을 그대로 따르고, 새 CSS는 꼭 필요할 때만 `apps/secs2r/ui/src/styles/` 아래의 해당 파일에 두십시오. UI 글자 14px 하한, 합니다체, 상태와 라벨은 명사구. em-dash와 이모지 금지.
- IPC 바인딩, `crates/**`, `features.toml`, `ipc/mock.ts`, `ipc/recorded/**`는 읽기만 하십시오.

## 허용 파일

`apps/secs2r/ui/src/screens/calibrate/{Settings.tsx,CalibrateScreen.test.tsx,useCalibrateScreen.test.tsx,specimens.tsx}`, `apps/secs2r/ui/src/screens/measure/{MeasureScreen.tsx,MeasureScreen.test.tsx,specimens.tsx}`, `apps/secs2r/ui/src/gallery/tour/scenes.ts`, `apps/secs2r/ui/src/styles/**`(필요할 때만), `docs/M5-reach.md`.

## 검증

포그라운드로 돌리고 끝날 때까지 턴을 끝내지 마십시오. 화면 의존성은 Node 24입니다.

```sh
cd E:\wt\advanced-promote\apps\secs2r\ui
npm ci
npm run typecheck
npm test
npm run build
```

## 보고

바꾼 파일 목록, 위 네 명령의 마지막 줄(통과한 시험 수 포함), 옮긴 항목마다 노출 조건이 ADR 0038과 같다는 확인, 남은 문제.
