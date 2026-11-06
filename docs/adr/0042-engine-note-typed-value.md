# 0042. 엔진의 Note를 화면까지 한 값으로

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A1. 해결하는 기술 부채 발견: TD-162(알림 매개변수의 미니 형식), TD-257(메모 페이로드의 위치 튜플). 관련 ADR: 0003(오류와 문구는 코드로 오간다), 0014(모의 서비스는 서비스 규칙을 갖지 않는다), 0016(공개 API).

## 배경

진단 하나가 엔진에서 화면까지 세 가지 모양을 거칩니다.

1. 엔진은 `crates/secs-engine/src/notes.rs`의 typed `Note`(33 variant)와 `MimoNote`, `GuardCause`를 만듭니다. `design.rs`와 `rc.rs`가 variant를 push합니다.
2. 서비스는 `crates/secs-service/src/calibration.rs:677-1095`의 `notices`, `guard_cause_params`, `scalar_notice`가 variant마다 `code`와 문자열 `params`(`Notice { code: String, params: BTreeMap<String, String> }`, `crates/secs-types/src/ipc.rs:1033`)로 펴서 보냅니다. 약 420줄의 match입니다.
3. 화면은 `apps/secs2r/ui/src/screens/calibrate/text.ts:155-390`의 46개 `case`가 같은 code를 다시 분기하고, `"hz:depth:fill,…"`(딥 보정 제한), `"lo:hi:phase:loss,…"`(채널 사이 위상), `"lo-hi,…"`(보정 대역) 같은 문자열을 `split(",")`과 `split(":")`로 되풀어 씁니다. `apps/secs2r/ui/src/ipc/fixtures.ts`와 `mock.ts`는 같은 미니 형식을 손으로 다시 만듭니다.

그래서 기능 하나(극성, 딥 보정 제한, 채널 사이 위상, MIMO 위치별 가드, 초기 구간)를 추가할 때 producer module, `Note` variant, `design.rs`의 push, 서비스의 match arm, `text.ts`의 case, 카탈로그 키, 모의 견본의 일곱 자리를 고쳐야 했습니다. 최근 다섯 기능이 모두 '엔진 작업서'와 '배선 작업서' 두 장으로 나뉜 까닭이 이것입니다(`docs/packets/P170`부터 `P178`까지).

deletion test를 적용하면 서비스의 `scalar_notice`를 지웠을 때 같은 문자열화가 `Design.notes`를 읽는 모든 곳에 다시 나타나므로 seam 자체는 진짜입니다. 문제는 그 seam이 엔진이 아니라 서비스에 있고, interface가 typed 값이 아니라 문자열 규약이라는 데 있습니다. 미니 형식은 ts-rs가 타입을 줄 수 없고, 형식이 어긋나면 `dips`는 조용히 빈 문자열로 표시되며(`text.ts:250`), 양쪽을 묶는 것은 손으로 쓴 문자열 시험(`text.test.ts:83-105`와 `calibration.rs:1708`)뿐입니다.

## 결정

`Note`가 세 모양을 거치지 않고 한 값으로 화면에 닿게 합니다. wire를 바꾸지 않는 1단계와 wire를 바꾸는 2단계로 나눕니다.

### 1단계: 기록 규약을 엔진이 소유합니다

`crates/secs-engine/src/notes.rs`에 다음을 둡니다.

```rust
/// One wire record of a note: the stable code and scalar string parameters.
pub struct NoteRecord {
    pub code: &'static str,
    pub params: Vec<(&'static str, String)>,
}

impl Note {
    /// Expands the note into records in producer order: one record per role,
    /// filter, mode, dip, band or support. Every value is a scalar string.
    pub fn record(&self) -> Vec<NoteRecord>;
}
```

- 서비스의 `notices`, `guard_cause_params`, `scalar_notice`(`calibration.rs:677-1095`)를 지우고, `CalibrationSummary.notes`·`rc`·`MimoSummary.notes`·`JobEvent::Log`를 채우던 자리는 `note.record().into_iter().map(Notice::from)` 한 줄이 됩니다. `From<NoteRecord> for Notice`는 `secs-service`에 둡니다.
- 기존에 항목마다 한 record로 펴던 것(`sub_filters`와 `sub_filter_peq`, `mode_decay`, `gd_cuts`, `two_way`, `mimo_support_*`)은 그대로이고, 미니 형식 셋은 이 단계에서 항목마다 한 record로 펴서 TD-162를 해결합니다. `pair_phase`는 밴드마다 `index`, `lo_hz`, `hi_hz`, `phase_deg`, `loss_db`를, `dip_limit`은 딥마다 `index`, `hz`, `depth_db`, `fill_db`를, `rc_bands`는 대역마다 `index`, `lo_hz`, `hi_hz`를 가집니다. 코드는 `pair_phase`, `dip_limit`, `rc_bands` 그대로이고 카탈로그 키 `notice.*` 89개는 바뀌지 않습니다. 화면은 `noticeText`의 `run`(같은 메모의 연속 record)으로 이미 묶어 읽으므로 `split` 파싱 셋만 지웁니다.
- 이 단계는 wire 모양(`Notice`)을 바꾸지 않습니다. 바뀌는 것은 미니 형식 셋의 값이 여러 record로 나뉘는 것뿐이고, 이것은 TD-162의 권고와 같습니다.

### 2단계: Note가 wire 타입입니다

`Note`, `MimoNote`, `GuardCause`를 `crates/secs-types/src/notes.rs`로 옮깁니다. `secs-engine`은 `pub use secs_types::notes::{Note, MimoNote, GuardCause}`로 같은 이름을 유지합니다(`secs-engine`은 이미 `secs-types`를 의존합니다, `Cargo.toml:27`).

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "code", rename_all = "snake_case")]
#[ts(export)]
pub enum Note {
    PolarityReversed { role: String, confidence: f64, compared: usize, opposite: usize },
    PairPhase { a: String, b: String, bands: Vec<PairBand> },
    DipLimit { role: String, dips: Vec<Dip> },
    ModeDecay { role: String, target_t60_s: f64, modes: Vec<Mode> },
    RcBands { bands: Vec<Band> },
    // … 나머지 variant는 지금 notes.rs와 같고, 이름 없는 튜플은 모두 이름 있는 struct가 됩니다.
}
pub struct PairBand { pub lo_hz: f64, pub hi_hz: f64, pub phase_deg: f64, pub loss_db: f64 }
pub struct Dip { pub hz: f64, pub depth_db: f64, pub fill_db: f64 }
pub struct Mode { pub hz: f64, pub t60_s: f64, pub cut_db: f64 }
pub struct Band { pub lo_hz: f64, pub hi_hz: f64 }
```

- serde 태그 `code`의 값은 1단계의 record 코드와 같은 snake_case입니다(`polarity_reversed`, `dip_limit`, `mimo_guard_reduced` 등). 그래서 카탈로그 키는 그대로이고, 화면의 `text.ts`는 `switch (note.code)`에서 문자열을 쪼개는 대신 판별 공용체의 필드를 읽습니다.
- `Note`가 싣는 엔진 타입은 함께 옮깁니다. `pair_phase::Band`는 `PairBand`로, `setup::SubFilter`는 `SubFilterNote { hp_hz, hp_type, lp_hz, lp_type, peq: Vec<Peq> }`로, `input::Peq`는 `secs_types::notes::Peq { hz, q, gain_db }`로 갑니다. `Note::SubFilters(Vec<(String, SubFilter)>)`처럼 역할 키 튜플을 가진 variant는 `Vec<RoleNote<T>>`(`{ role, value }`)가 됩니다. 이것이 TD-257의 권고입니다.
- wire에서 엔진 메모를 싣는 자리(`CalibrationSummary.notes`·`rc`, `MimoSummary.notes`, `MimoApplied.notes`)는 `Vec<Notice>`에서 `Vec<Note>`로 바뀝니다. 서비스가 스스로 만드는 알림(`settings_notice`, `measurement_resampled`, `measurement_low_snr`, 마이크 보정, 앱 설정 손상)은 지금처럼 `Notice { code, params }`로 남고 `JobEvent::Log`도 그대로입니다. 화면의 `noticeText`는 두 입력을 받습니다. typed `Note`는 필드로, `Notice`는 지금처럼 `params`로 읽습니다.
- 로컬 API와 파이썬·.NET 소비자(ADR 0016)는 `"dips":"80:6.1:3"` 대신 `{"code":"dip_limit","role":"L","dips":[{"hz":80,"depth_db":6.1,"fill_db":3}]}`를 받습니다. 메서드 id는 바뀌지 않으므로 ADR 0003이 얼린 것은 그대로이고, 응답 안의 모양이 바뀌는 것은 0.x에서 허용되는 변경입니다(이 PR의 wire 변경은 유지보수자가 2026-11-06에 승인했습니다).
- 모의 서비스(`mock.ts`)와 견본은 손으로 만든 미니 형식을 버리고 서비스 기록(ADR 0014, `apps/secs2r/ui/src/ipc/recorded/`)에서 받은 typed JSON을 씁니다.

바뀌지 않는 것: 엔진의 수치 코드와 골든, 카탈로그 키 89개, 메서드 id, `Notice`를 쓰는 서비스 알림.

## 시험

1단계(엔진 interface에서 먼저 씁니다).

- `crates/secs-engine/src/notes.rs`의 시험 module에 variant 집합마다 `record()`의 기대 record를 리터럴로 둡니다. 기대값은 지금 서비스 시험(`calibration.rs:1341-1420`의 극성·채널 사이 위상·RC 상한·RC 대역, `:1678-1706`의 서브 필터·모드 감쇠·MIMO 가드)이 가진 JSON 리터럴을 그대로 옮긴 것이라 코드에서 다시 유도한 값이 아닙니다.
- 미니 형식 셋은 record가 항목마다 하나이고 `index`가 0부터 producer 순서인 것을 단언합니다.
- 서비스에는 wire 시험 하나만 남깁니다. `calibration_summary_notes_are_scalar_records`가 응답의 모든 `params` 값이 `,`와 `:`를 담지 않는지 봅니다.
- 화면 `text.test.ts`는 세 코드의 새 record 묶음을 문장으로 바꾸는 사례로 바꾸고, `split`에 기대던 사례는 지웁니다.

2단계(`secs-types`에서 먼저 씁니다).

- `secs-types::notes::tests`에 variant마다 JSON 리터럴 왕복 시험을 둡니다(`serde_json`으로 문자열에서 읽어 다시 쓴 바이트가 같고, `code` 태그가 기대한 snake_case).
- `cargo test -p secs-types`가 `apps/secs2r/ui/src/ipc/bindings/Note.ts`를 생성하고, `text.ts`의 `switch`가 그 타입으로 컴파일됩니다(`npm run typecheck`).
- `crates/secs-local-api/tests`에 `calibration_summary` 응답에서 `notes[0].code`와 typed 필드를 읽는 사례 하나, `bindings/python/tests`와 .NET `PackageTests`에 같은 사례 하나씩을 둡니다.
- 지우는 시험: 서비스의 `notices()`·`scalar_notice` 시험(1단계에서 엔진으로 옮긴 것), `text.test.ts`의 문자열 파싱 사례, `mockWiring.test.ts`에서 미니 형식을 만들던 부분.

## 이행

1. `Note::record()`와 `NoteRecord`를 엔진에 두고 시험을 먼저 씁니다(빨강). 서비스의 match를 옮겨 초록을 만듭니다. 커밋 하나, wire 불변.
2. 미니 형식 셋을 항목마다 한 record로 펴고 화면의 `split` 파싱을 지웁니다. 커밋 하나. 여기까지가 TD-162입니다.
3. `secs-types`에 `notes.rs`를 만들고 이름 있는 struct로 옮깁니다. 엔진은 재수출로 이름을 유지하고, 엔진 producer(`polarity.rs`, `pair_phase.rs`, `rc.rs`, `mimo/*`)는 튜플 대신 struct를 채웁니다. 커밋 하나. 골든은 수치가 아니라 메모를 비교하지 않으므로 바뀌지 않지만, `cargo test -p secs-engine`이 전부 통과해야 합니다.
4. wire 필드를 `Vec<Note>`로 바꾸고 바인딩을 다시 생성합니다(`cargo test -p secs-types`). 화면 `text.ts`와 견본, 기록 응답을 갱신합니다. 커밋 하나.
5. 로컬 API·파이썬·.NET의 README와 ADR 0016의 응답 예를 고칩니다. 커밋 하나.

작업 분배: 1~3단계는 Daybreak(엔진·서비스), 4단계의 화면은 Opus 5.5, 5단계와 바인딩 시험은 ASTRA. Claude가 커밋마다 디프를 검토하며, 검토가 보는 것은 record 코드가 카탈로그 키와 하나도 어긋나지 않는지(`ko.json`의 `notice.*` 키 목록과 `record()`의 코드 집합을 비교하는 시험을 둡니다), 엔진의 수치 파일(`rc.rs`, `design.rs`)에서 push 자리 말고는 바뀐 줄이 없는지입니다.

## 결과

- 진단 하나의 정의, 직렬화, 표시 규약이 `secs-types/src/notes.rs`와 카탈로그 두 곳에 모입니다. 새 메모는 variant 하나와 카탈로그 키 하나로 끝나고 작업서는 한 장이 됩니다.
- 서비스에서 약 420줄, 화면에서 문자열 파싱과 `valid` 가드가 사라집니다.
- 계산 결과와 내보낸 파일은 바뀌지 않으므로 편차 항목은 없습니다. `CHANGELOG.md`에는 로컬 API 응답의 메모 모양이 바뀐다고 적습니다.
- 위험: `Note`의 `f64`가 JSON에서 NaN이 될 수 없으므로 엔진이 유한하지 않은 값을 메모에 넣지 않는다는 불변식이 생깁니다. `record()` 시험과 `secs-types`의 직렬화 시험이 이것을 확인합니다.
- 다음 감사는 `Notice { code, params }`가 서비스 알림에 남아 있는 것을 다시 제안하지 않아야 합니다. 그것은 엔진 밖에서 생기는 알림의 자리이고 이 ADR이 일부러 남긴 것입니다.
