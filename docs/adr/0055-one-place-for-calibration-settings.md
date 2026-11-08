# 0055. 보정 설정은 선언 하나에서 필드, 편집, 한도, 검증이 나온다

상태: 제안, 2026-11-06

근거: 감사 이슈 #186, 아키텍처 리뷰 A14. 이 ADR이 해결하는 기술 부채 발견은 TD-248의 공개 API 클램프(`CalibrateParams.settings`가 편집 검증을 거치지 않음)이고, TD-174의 손 사본 가운데 설정 한도는 ADR 0052의 `SCALAR_EDITS` 표로 여기서 나옵니다.

## 배경

불리언 설정 하나(`dip_limit`, ADR 0034)를 추가할 때 손으로 고친 자리입니다(생성 파일과 시험 제외, 2026-11-06의 코드에서 다시 확인).

1. `crates/secs-types/src/calibration.rs:185, 257, 366-367`(필드, 기본값, `engine_key`)
2. `crates/secs-types/src/settings_edit.rs:120`(`SettingEdit::DipLimit`)
3. `crates/secs-service/src/settings_edit.rs:400`(`apply` arm. 숫자 설정은 `:18-114`의 `validate_edit`도)
4. `crates/secs-service/src/calibration.rs:961, 2029`
5. `crates/secs-project/src/calibration.rs:30, 79, 273-274`(업스트림 JSON 왕복)
6. `crates/secs-types/src/rules.rs:17-87`(숫자만, `bindings/rules.ts` 생성)
7. `features.toml:2172-2185, 2275-2280`
8. `apps/secs2r/ui/src/screens/calibrate/Settings.tsx:513-517`
9. `apps/secs2r/ui/src/i18n/ko.json:1089-1090`, `en.json`
10. `apps/secs2r/ui/src/screens/calibrate/text.ts:242-248`(메모를 남기면)
11. `apps/secs2r/ui/src/ipc/mock.ts:136, 661`(숫자는 `rcEditRefusal`도)
12. `apps/secs2r/ui/src/screens/calibrate/fixtures.ts:85`
13. `apps/secs2r-cli/src/main.rs:211, 682, 737-741`(`ProjectAction::Set`의 `if let Some(..) … edits.push(SettingEdit::X)` 블록 열한 개 가운데 하나)
14. `apps/secs2r/ui/src/ipc/recorded/calibration_settings_get.json`(다시 기록)

`mimo_early_ms`(ADR 0037)도 같은 열다섯 파일을 지났습니다. 화면에서는 `Settings.tsx`의 컨트롤 54개(`Toggle` 13, `NumberField` 27, `Segmented` 8, `Select` 6)와 `changeSetting` 호출 37곳이 `id`, 라벨 키, 도움말 키, 읽는 필드, 한도, 편집 kind 여섯 속성을 되풀이합니다(`:1100-1106`, `:1169-1179`, `:556-562`).

셋(서비스 `apply`, 모의 `editSettings`, CLI `Set`)은 같은 '편집 kind → 필드, 한도, 부작용' 표의 adapter입니다. 공개 API와 로컬 API의 `CalibrateParams.settings`는 그 표를 거치지 않아, `secs-service/src/calibration.rs:270-289`의 `snapshot`이 `rc_bands`만 고치고 나머지 값은 그대로 엔진에 들어갑니다(TD-248). 엔진 입구는 `taps`와 `input_hp_hz`만 봅니다(`crates/secs-engine/src/design.rs:199-220`). 최근 다섯 기능이 모두 '엔진 작업서 + 배선 작업서' 두 장이 된 둘째 이유가 이 흩어짐입니다(첫째는 ADR 0042의 `Note`).

## 결정

### 러스트: 선언 매크로 한 줄

`secs-types/src/calibration.rs`에 선언 매크로 `calibration_settings!`를 두고 스칼라·불리언 설정을 한 줄씩 선언합니다.

```rust
calibration_settings! {
    // 필드          타입           기본값   JSON 키          편집 variant·필드      한도 키        정책    engine_key
    dip_limit:      bool,          false,   "dip_limit",     DipLimit { on },       -,             -,      flag;
    input_hp_hz:    Option<f64>,   None,    "input_hp_hz",   InputHp { hz },        input_hp,      refuse, "input_hp=";
    boost_max_db:   f64,           6.0,     "boost_max_db",  BoostMax { db },       boost,         clamp,  number;
    …
}
```

매크로가 만드는 것은 다음입니다.

- `CalibrationSettings`의 필드와 `Default`, 그리고 `engine_key`·`manual_key`·`mimo_key`에 들어가는 항목(`flag`는 켜졌을 때만 `|이름`, `number`는 늘, `"접두="`는 값이 있을 때만. 지금의 키 문자열과 바이트 단위로 같아야 하며 기존 골든 문자열 시험이 지킵니다).
- `SettingEdit`의 스칼라 variant.
- `CalibrationSettings::apply_scalar(&mut self, edit: &SettingEdit) -> Result<Applied, SettingsRefusal>`. 한도 키와 정책으로 `clamp`(한도로 자름)나 `refuse`(`field`, `reason`, `min`, `max`를 담은 거절)를 적용합니다. `secs-service/src/settings_edit.rs`의 `validate_edit`와 `apply`의 스칼라 arm은 이것을 부르고, 설정 사이의 규칙(`rc_lo < rc_hi`, 부피와 치수의 일치, 묶음 공유)만 손 코드로 남깁니다.
- `CalibrationSettings::checked(self) -> Result<Self, SettingsRefusal>`. 모든 스칼라에 같은 정책을 적용합니다. `snapshot`이 `override_settings`에 이것을 부르므로 공개 API·로컬 API·CLI가 넣은 설정도 화면 편집과 같은 검증을 거칩니다(TD-248). 거절은 `ErrorCode::InvalidRequest`에 `field`를 담아 돌려줍니다. 결정 7의 기본값대로 클램프 정책의 필드는 자르고 거절 정책의 필드는 거절합니다.
- `rules::SCALAR_EDITS: [(kind, field, limit key, policy)]`와 `KNOWN_KEYS`(프로젝트 파일의 키 순서, F16). `cargo test -p secs-types`를 실행하면 `SCALAR_EDITS`가 `bindings/rules.ts`에 저장됩니다.
- `secs-project/src/calibration.rs`의 스칼라 읽기·쓰기는 `KNOWN_KEYS`와 매크로가 만든 접근자로 돌고, 업스트림 JSON 바이트는 바뀌지 않습니다(기존 왕복 골든이 지킵니다).

엔진 입구에는 `check_config`를 두어 유한하지 않거나 `CALIBRATION_LIMITS` 밖인 값을 `EngineError::Input`으로 거부하되 필드 이름을 메시지에 담습니다. 서비스가 `checked`로 먼저 걸러도 엔진은 공개 crate이므로 자기 입구에서 한 번 더 검사합니다(ADR 0031이 `input_hp_hz`에 한 것을 모든 필드로).

구조를 가진 설정(`subs_manual`, `bundle_layouts`, `xo`, `rc_bands`, `room_dims_m`)은 매크로에 넣지 않습니다. 규칙이 값 하나의 한도가 아니기 때문입니다.

### CLI: 표 한 행

`apps/secs2r-cli`의 `project set`은 clap 구조체를 그대로 두되(도움말 문장이 거기 있음), 열한 개의 `if let Some(..) { edits.push(..) }` 블록을 `SCALAR_FLAGS: [(flag, kind)]` 표와 `SettingEdit::scalar(kind, value)` 생성자로 바꿉니다. 새 스칼라 설정은 플래그 한 줄과 표 한 행입니다.

### 화면: 설정 행 표와 `SettingRow` 하나

`screens/calibrate/settingRows.ts`가 단순한 컨트롤 약 40개를 행으로 가집니다.

```ts
type SettingRow = { id: string; field: keyof CalibrationSettings; kind: SettingEdit["kind"];
  control: "toggle" | "number" | "segmented" | "select"; limits?: keyof CalibrationLimits; unit?: string; step?: number; digits?: number;
  shownWhen?: (s: CalibrationSettings, view: CalibrateView) => boolean; disabledWhen?: …; hint?: (s, view) => string | null };
```

`SettingRow` 컴포넌트 하나가 행을 표시합니다. `data-ui` id와 카탈로그 키 규약(`calibrate.<section>.<name>`, `.help`, `.hint`)은 그대로이고 표에 한 번 적힙니다. ADR 0038의 '뜻이 없는 상황에서는 표시하지 않는다'는 `shownWhen` 열이고, '고급'에 두는 것은 카드가 행을 어느 묶음에 넣는지로 정합니다. 범위 카드의 추가 구간 편집기(`:369-537`), 서브 편집기(`:736-896`), XO 카드(`:592-708`)는 손으로 둡니다. 모의 서비스는 ADR 0052대로 `SCALAR_EDITS`를 조회해 적용합니다.

## 시험

- `secs-types`: 매크로가 만든 표를 순회하는 시험 하나. 모든 `SettingEdit` 스칼라 variant에 필드가 있고, 기본값이 JSON으로 왕복하며, 한도 키가 `CalibrationLimits`에 있고, `engine_key`가 지금의 골든 문자열과 같습니다.
- `secs-service`: `settings_edit` 시험은 그대로 통과해야 합니다(동작이 같음). 새 시험 `calibrate_params_settings_are_checked_like_edits`: 공개 API로 넣은 `boost_max_db = 1e9`는 한도로 잘리고, `rc_lo = 5`는 `field = rc_lo`로 거절되며, `room_volume_m3 = -1`은 엔진까지 가지 않습니다.
- `secs-engine`: `design_rejects_out_of_range_settings_with_the_field_name`. `room_volume_m3 < 0`, `boost_max_db` 한도 밖, `lift_ms`·`peq_bands`의 범위 밖이 NaN을 퍼뜨리지 않고 입력 오류로 끝납니다(TD-248의 엔진 쪽).
- 화면: `settingRows`를 순회하는 시험 하나가 54개 컨트롤의 id, 한도, 편집 kind를 검사하고, `CalibrateScreen.test.tsx:1217-1328`의 `REACH_IDS` 130개 가운데 행 표의 id는 표에서 만듭니다. 기존 화면 시험은 그대로 통과해야 합니다.
- CLI: 표의 모든 플래그가 해당 편집을 만드는지 순회 시험 하나.

## 이행

1. 매크로와 `apply_scalar`·`checked`·`SCALAR_EDITS`. 서비스 `settings_edit.rs`와 `snapshot`이 그것을 씁니다(TD-248). 엔진 `check_config`.
2. `secs-project`의 스칼라 왕복을 `KNOWN_KEYS`로.
3. CLI 표.
4. 화면 `settingRows.ts`와 `SettingRow`(화면은 Opus 5.5가 맡습니다), 모의 서비스 조회(ADR 0052).

## 결과

- 불리언 설정 추가가 러스트 한 줄, 화면 표 한 행, 카탈로그 두 키, CLI 표 한 행, `features.toml` 항목으로 줄어듭니다. 서비스·모의 서비스·CLI의 설정 표 사본이 사라집니다.
- 공개 API와 로컬 API의 설정 덮어쓰기가 화면 편집과 같은 검증을 거칩니다. 지금까지 한도 밖 값을 넣어 돌리던 호출은 거절되거나 잘립니다. 이것은 동작 변경이므로 CHANGELOG에 기록합니다. 업스트림 SECS 2는 공개 API가 없으므로 편차 항목은 없습니다.
- 매크로가 만든 코드는 읽기 어려울 수 있으므로 `cargo doc`이 생성 항목을 보여 주도록 매크로 안에 doc 주석을 둡니다.
- 다음 감사는 구조를 가진 설정 다섯이 손 코드인 것을 다시 제안하지 않습니다.
