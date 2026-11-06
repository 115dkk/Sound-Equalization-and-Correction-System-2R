# 0046. 엔진 interface 좁히기

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A5. 직접 해결하는 기술 부채 발견은 없고, TD-219(호출자 없는 공개 항목)와 TD-254(module 단위 `#[allow(dead_code)]`)의 엔진 부분이 이 결정 위에서 정리됩니다. 관련 ADR: 0002(unsafe와 크레이트 경계), 0004(골든), 0016(공개 크레이트), 0040(npm 바인딩은 별도 브랜치).

## 배경

`docs/ARCHITECTURE.md` 5절은 엔진의 interface를 '한 번의 호출'로 약속합니다. 실제 표면은 그보다 넓습니다.

- `crates/secs-engine/src/lib.rs:36-115`는 `pub mod` 9개(`grid`, `input`, `display`, `export_view`, `mimo`, `pair_phase`, `polarity`, `preview`, `xo`)와 재수출 62개를 가지고, `#[allow(dead_code)]`가 module 14개에 걸려 있습니다. `mimo/mod.rs:4-27`은 이름 30개의 두 번째 표면입니다.
- 재수출 62개 가운데 36개는 크레이트 밖 어디에서도, 시험에서도 쓰이지 않습니다. `LayoutInfo`, `ManualPlan`, `EngineInputs`, `calibrate_with`, `engine_inputs`, `BaRow`, `input_response`, `DesignSpectra`, `MainDesign`, `RoleScores`, `SubDesign`, `production_fitters_with`, `ExportView`, `export_view`, `FitRequest`, `Fitter`, `PeqDerivatives`, `fit_peq_with`, `Spectra`, `long_spectra`, `IirDerivatives`, `IirSolveOptions`, `IirSolver`, `IirSupport`, `MimoDesign`, `MimoIir`, `MimoRequest`, `MimoRoleLateEnergy`, `MimoRoleScore`, `design_mimo`, `design_mimo_iir`, `design_mimo_iir_with_options`, `normalize_measurements` 등입니다.
- `Design`(`design.rs:67-93`)은 `pub` 필드 19개, `MainDesign` 12개, `SubDesign` 9개를 드러냅니다. `crates/secs-export/src/lib.rs`는 `run.design.replay.config().main_position`(417, 612), `run.design.chain.mains[i].fir`(428-434, 617), `run.design.rc_hi`(249, 481, 641), `run.design.replay.measurements()`(619, 702), `run.design.replay.export_view(..)`(416, 611), `run.design.replay.export_ir(..)`(398)처럼 안쪽 필드 여덟 자리를 직접 읽습니다.
- `FitterFactory`(`design.rs:38`)와 `Fitter`(`fit.rs:39`)는 단계 분류 `FitStage::{Ba, Top, Rc}`를 호출자에게 드러냅니다.

바깥에서 실제로 쓰는 것은 `secs-service`(`calibrate`, `late_energy`, `mimo::compute_with_acceleration`, `mimo::availability`, `xo::recommend_xo`, `preview::sub_combine`, `Note`·`MimoNote`·`GuardCause`, `EngineError`, `MimoKey`, `ProjectView`·`CalibrationRequest`, `StageResponses`), `secs-export`(위의 여덟 자리와 `stage_responses`, `ExportAxis`), `secs-gpu`(`lib.rs:7`), `secs-engine-testkit`(`FitterFactory`, `DesignRequest`, `design`)뿐입니다.

## 결정

엔진의 공개 표면을 바깥 셋이 실제로 쓰는 것으로 줄이고, `Design`의 안쪽 배치를 메서드 뒤에 둡니다. implementation은 바뀌지 않습니다.

### 남는 공개 표면

```rust
// lib.rs
pub use calibration::{Calibration, CalibrationRequest, ProjectView, calibrate};
pub use design::{Design, RoleLateEnergy, late_energy};
pub use chain::StageResponses;
pub use error::EngineError;
pub use input::{EngineConfig, Measurement, MeasurementSet, Samples, Tuning};
pub use mimo::{MimoKey, MimoRun, availability, compute_with_acceleration, AccelerationSelection};
pub use notes::{GuardCause, MimoNote, Note};   // ADR 0042 2단계 뒤에는 secs_types::notes의 재수출
pub use replay::ExportAxis;
pub use scorecard::Metrics;
pub use setup::SubFilter;
pub mod preview;   // sub_combine
pub mod xo;        // recommend_xo, XO_CANDIDATES
pub mod input;
```

- 재수출 36개와 `grid`, `export_view`, `pair_phase`, `polarity`, `display`의 `pub`을 지웁니다. 안쪽에서 쓰는 것은 `pub(crate)`가 되고, 오라클 골든 시험만 쓰는 것은 지금처럼 `cfg(test)`와 `test-support` 기능(`testkit.rs`) 뒤에 둡니다. `FitterFactory`, `DesignRequest`, `design`, `FitStage`는 `test-support` 기능 뒤로 가서 `secs-engine-testkit`만 봅니다.
- module 단위 `#[allow(dead_code)]` 14개는 표면을 줄인 뒤 항목 단위로 바꿉니다. 그래야 다음 감사가 정말 죽은 항목을 볼 수 있습니다(TD-254).

### Design의 메서드

```rust
impl Design {
    pub fn config(&self) -> &EngineConfig;
    pub fn main_position(&self) -> u32;
    pub fn latency_ms(&self) -> f64;
    pub fn rc_hi(&self) -> f64;
    pub fn measured(&self, position: u32, role: &str) -> Option<&Measurement>;
    pub fn main_roles(&self) -> impl Iterator<Item = &str>;
    pub fn sub_roles(&self) -> impl Iterator<Item = &str>;
    pub fn fir(&self, role: &str) -> Option<&[f64]>;
    /// The stage responses of one main at one position (today's free `stage_responses`).
    pub fn stage(&self, role: &str, position: u32) -> Result<StageResponses, EngineError>;
    /// The exported IR of a spectrum on the export axis (today's `replay.export_ir`).
    pub fn export_ir(&self, spectrum: &[C], axis: ExportAxis) -> (Vec<f32>, f64);
}
```

- `replay`, `spectra`, `mains`, `subs`, `chain`과 `MainDesign::{before, combined, after}`는 `pub(crate)`로 내립니다. 자유 함수 `stage_responses`와 `export_view`는 메서드 뒤로 들어가 `secs-export`의 여덟 자리가 메서드 호출이 됩니다.
- `secs-service`가 캐시에 보관하는 `DesignReplay`는 서비스가 `Design` 전체를 다시 만들 때만 쓰므로 `Design::replay_for_cache()`와 `Design::from_replay(..)` 한 쌍으로 드러내고 타입 자체는 `pub(crate)`로 둡니다. 캐시 판(ADR 0012)은 바뀌지 않습니다.

바뀌지 않는 것: 엔진의 수치 코드, 골든(ADR 0004), `calibrate`와 `compute_with_acceleration`의 시그니처, 단계 안쪽 골든 시험(`cfg(test)` 안에서 그대로).

## 시험

좁아진 interface에서 먼저 씁니다.

- `crates/secs-engine/tests/surface.rs`: `calibrate` 한 번과 `Design::stage`, `Design::export_ir`, `Design::fir`로 `secs-export`가 읽던 값을 얻어 지금 `secs-export`의 골든(`convolution_bundle_*`, 필터 텍스트 골든)이 단언하는 리터럴과 같은지 봅니다.
- `cargo doc -p secs-engine --no-deps`로 뽑은 공개 항목 목록을 시험 안의 기대 목록과 비교합니다(`rustdoc --output-format json`은 nightly이므로 `cargo public-api`를 쓰지 않고, `lib.rs`의 `pub use`·`pub mod` 줄을 읽는 작은 검사로 둡니다). 항목이 늘면 시험이 실패해 ADR을 고치게 합니다.
- `secs-service`, `secs-export`, `secs-gpu`, `secs-engine-testkit`의 기존 시험은 바뀌지 않고 통과해야 합니다.
- 지우는 시험: 없습니다. 단계 안쪽 골든 시험은 `cfg(test)`에서 그대로입니다.

## 이행

1. `Design`의 메서드를 추가하고 `surface.rs` 시험을 먼저 씁니다. 필드는 아직 `pub`입니다. 커밋 하나.
2. `secs-export`의 여덟 자리를 메서드로 바꿉니다. 골든 비트 동일. 커밋 하나.
3. `secs-service`가 쓰는 `DesignReplay`를 `replay_for_cache`·`from_replay`로 바꿉니다. 캐시 왕복 시험이 그대로 통과해야 합니다. 커밋 하나.
4. 재수출 36개와 `pub mod` 다섯을 지우고 `Design`의 필드를 `pub(crate)`로 내립니다. 컴파일러가 남은 바깥 사용을 모두 지목합니다. `cargo publish --dry-run`(ADR 0016)이 통과해야 합니다. 커밋 하나.
5. `#[allow(dead_code)]`를 항목 단위로 바꾸고 정말 죽은 항목(TD-254)을 지웁니다. 커밋 하나.
6. `docs/ARCHITECTURE.md` 5절에 남는 공개 표면을 적습니다. 커밋 하나.

작업 분배: Daybreak(엔진·작성기). Claude가 커밋마다 디프를 검토하며, 검토가 보는 것은 수치 파일에서 가시성 키워드 말고 바뀐 줄이 없는지, 골든 시험 파일에 손대지 않았는지, 공개 표면 시험의 기대 목록이 이 ADR의 목록과 같은지입니다.

## 결과

- interface가 줄고 implementation은 그대로입니다. 36개를 지워도 바깥은 바뀌지 않으므로 deletion test를 통과합니다.
- `Design`의 안쪽 배치를 바꿔도 `secs-export`가 깨지지 않습니다. 공개 크레이트(ADR 0016)가 내보내는 엔진 표면이 그만큼 줄어 판 호환을 약속할 수 있는 범위가 분명해집니다.
- 계산과 파일은 바뀌지 않으므로 편차 항목은 없습니다.
- 위험: `secs-engine`은 게시 크레이트이므로 공개 항목을 지우는 것은 라이브러리 판의 변경입니다. 0.x이고 ADR 0016이 긴 이름 크레이트 하나에만 호환을 약속하므로 허용되지만, CHANGELOG에 지운 항목을 적습니다. npm 바인딩 브랜치(ADR 0040)가 엔진 항목을 쓴다면 그 브랜치가 main을 병합할 때 같은 메서드로 옮겨야 합니다.
- 다음 감사는 `crates/secs-engine/src/tests/*`의 단계 안쪽 시험과 `lib.rs`의 남은 `#[allow(dead_code)]`를 다시 제안하지 않아야 합니다. 골든이 업스트림 함수 단위라(ADR 0004) 단계 interface로 들어가야 하고, 이 ADR은 바깥 interface만 좁힙니다.
