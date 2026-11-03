# P169 Daybreak: 메인 극성 검사와 극성 설정 적용

ADR 0033(`docs/adr/0033-speaker-polarity.md`)을 구현하십시오. 먼저 그 ADR과 `CONTEXT.md`, `CLAUDE.md`를 읽으십시오. 이 작업서의 줄 번호는 모두 `origin/main` 4957a457 기준이며, 작업 트리는 `E:\wt\p1-polarity`(브랜치 `claude/p1-polarity`)입니다. 그 트리에서만 일하고, 다른 체크아웃은 건드리지 마십시오.

## 목표

1. 패치 시트의 극성 설정(`config.speakers[].bands[].pol`)이 메인 스피커의 계산과 모든 내보내기에 실제로 적용됩니다.
2. 보정 계산이 메인 채널 사이의 상대 극성을 재어 반대인 스피커를 메모로 알립니다.
3. 극성이 모두 `+`인 프로젝트에서는 계산 결과, 신호 사슬 JSON, 설계 ID, 내보낸 파일이 지금과 바이트 단위로 같습니다. 골든과 오라클 시험이 모두 그대로 통과해야 합니다.

## 지금 코드의 사실

- `pol`은 `crates/secs-project/src/config.rs:9-14`의 `Band { band, out, pol }`에 있고 `"+"`가 기본입니다. 편집은 `set_speaker`(`config.rs:397-452`), 검사는 `pol_invalid`(`config.rs:565-575`)입니다. 계산과 내보내기 어디서도 읽지 않습니다. 업스트림도 같습니다.
- 엔진 입력은 `crates/secs-engine/src/calibration.rs:111-245`의 `engine_inputs(view: &ProjectView, irs: &MeasurementSet, settings)`가 만듭니다. `ProjectView`(`calibration.rs:18-22`)는 `groups`, `project_xo`, `target`만 가집니다. 만드는 곳은 `crates/secs-service/src/calibration.rs:68, 166, 1525`, `crates/secs-service/src/export.rs:598`, `crates/secs-engine/src/calibration.rs:376`, `crates/secs-engine/src/xo.rs:477`, `crates/secs-engine/examples/engine_perf.rs:819`입니다.
- 측정 표본은 `Samples::{F32, F64}`(`crates/secs-engine/src/input.rs:19`)이고, `MeasurementSet.positions`는 `(위치, Vec<(역할, Measurement)>)`입니다. 도착은 `measurement::onset_index`(`measurement.rs:50-64`)가 절댓값으로 찾습니다.
- 위치 가중은 `levels::weights(positions, main_position)`(`levels.rs:13`, 기준 위치 5배)입니다.
- 설계는 `design_pass`(`design.rs:530`)가 패스마다 돕니다. `RcPass::Probe`는 RC 상한의 위치 일치도 탐색 패스이고, 메모는 `design.rs:554`부터 모읍니다.
- 메모는 `crates/secs-engine/src/notes.rs`의 `Note`이고, 서비스의 `scalar_notice`(`crates/secs-service/src/calibration.rs:887-1042`)가 `Notice { code, params }`로 바꿉니다. `match`가 모든 변형을 다루므로 새 변형은 반드시 추가해야 합니다. 같은 이유로 `crates/secs-engine/src/tests/design.rs:35, 636-660`과 `crates/secs-service/src/calibration.rs:1776`의 시험도 고쳐야 합니다.
- 신호 사슬의 `MainChannel`(`crates/secs-types/src/chain.rs:74-91`)에는 극성이 없고 `SubChannel.polarity: i8`만 있습니다. `input_hp_hz`(`chain.rs` `SignalChain`)가 '없으면 직렬화하지 않아 설계 ID가 그대로'인 본보기입니다. ts-rs 바인딩(`apps/secs2r/ui/src/ipc/bindings/MainChannel.ts`)은 `cargo test -p secs-types`로 다시 만듭니다.
- 내보내기 계획은 `crates/secs-graph/src/plan.rs`입니다. 메인 출력 처리 `main_ops`는 `317-362`(게인, top PEQ, HP, 지연)에서 만들고, 출력은 스피커마다 하나입니다(`292-297`). `Op::Invert { fixed }`는 이미 있고(`plan.rs:57`), 서브 피드와 MIMO IIR 서포트가 씁니다(`plan.rs:415, 468`). 작성기의 처리: Equalizer APO `crates/secs-filters/src/lib.rs:377`, 필터 목록 `lib.rs:1029`, 컨볼루션·재생 `crates/secs-filters/src/render.rs:199`, CamillaDSP는 계획이 아니라 사슬에서 필터를 만들고 메인 게인의 `inverted`가 `false`로 고정되어 있습니다(`crates/secs-camilla/src/lib.rs:930-937`).
- CamillaDSP 파이프라인 순서(`secs-camilla/src/lib.rs:876-1280`): 입력 보정 → 메인 출력 처리 필터 → 서브 버스 합과 PEQ → 서브 게인·극성(`bus_mapping`) → MIMO 서포트 합류 `mimo_merge` → 서브 고정 필터(물리 출력마다, 합류 뒤) → `route_out`. Equalizer APO 작성기도 서포트 복사 뒤에 서브 고정 필터를 둡니다(`secs-filters/src/lib.rs:533` 근처의 불변식 주석).
- 엔진 안의 응답 시뮬레이션은 `crates/secs-engine/src/chain.rs:107-174`의 `stage_responses`와 `input_response`(`chain.rs:176`)입니다. 사슬의 게인·지연·top PEQ·XO만 곱하고, 측정 스펙트럼 `spectra`는 `measurement::prepare`가 만든 것입니다.
- 결과의 낡음은 `config_key(project)`(`crates/secs-service/src/calibration.rs:1115`)가 `project.config` 전체를 넣으므로, 극성을 바꾸면 이미 낡은 결과가 됩니다. 확인만 하고 바꾸지 마십시오.

## 바꿀 것

### A. 극성 설정 적용

1. `crates/secs-project/src/config.rs`의 `SpeakerConfig`에 `pub fn inverted_mains(&self) -> Vec<String>`를 추가하십시오. 역할이 메인(`role_kind`가 서브가 아님)이고 첫 대역의 `pol`이 `"-"`인 스피커의 역할을 구성 순서대로 돌려줍니다. 대역이 없는 스피커는 뺍니다.
2. `ProjectView`에 `pub inverted: &'a [String]`을 추가하고, 위에 적은 일곱 곳에서 채우십시오. 서비스는 `project.config.inverted_mains()`를, 공개 엔진의 `calibration.rs:376`도 같은 함수를 씁니다. `xo.rs:477`과 `engine_perf.rs:819`는 빈 조각이면 됩니다.
3. `engine_inputs`가 `view.inverted`에 든 역할의 `Measurement.samples`에 모든 위치에서 −1을 곱하게 하십시오. `F32`는 `F32`로, `F64`는 `F64`로 유지합니다. `EngineConfig`에 `pub inverted_mains: Vec<String>`을 추가해 그 역할 목록을 넘기고, `test_support.rs:154`와 `crates/secs-engine-testkit/src/lib.rs:156`의 리터럴에도 빈 벡터를 넣으십시오.
4. `MainChannel`에 `polarity`를 추가하십시오. 값은 `1` 또는 `-1`이고, `1`이면 직렬화하지 않으며 없으면 `1`로 읽습니다. ts-rs가 `i8`에 `#[ts(optional)]`을 거부하면 `Option<i8>`(`None`이 `1`)로 두되, 어느 쪽이든 극성이 모두 `+`인 사슬의 JSON은 지금과 바이트 단위로 같아야 합니다. 설계는 `design.rs:857`에서 `config.inverted_mains`에 든 역할에 `-1`을 넣습니다. `MainChannel` 리터럴이 있는 곳(`secs-filters/src/playback.rs:562`, `secs-graph/src/design.rs:56`, `secs-graph/src/validate.rs:110`, `secs-service/src/cache.rs:518`, `secs-service/src/design_export.rs:629`와 시험들)을 모두 고치십시오.
5. 내보내기 계획: `plan.rs`의 `main_ops` 끝에 `polarity == -1`이면 `Op::Invert { fixed: true }`를 넣으십시오. 모든 작성기가 메인의 고정 `Invert`를 그 스피커의 **물리 출력에서 MIMO 서포트 합류 뒤**에 적용하게 하십시오. 메인 본 신호와 그 스피커로 들어오는 서포트 신호가 함께 뒤집혀야 합니다.
   - CamillaDSP: `mimo_merge` 뒤, 서브 고정 필터와 같은 자리에 그 출력의 `Gain` 필터(`gain` 0, `inverted` true)를 두십시오. 메인 게인 필터의 `inverted`는 `false`로 둡니다. 우회 설정(`_bypass.yml`)에도 같은 필터가 남아야 합니다.
   - Equalizer APO: 서포트 복사 뒤에 `Copy: 출력=-1.0*출력`을 두십시오.
   - 필터 목록 텍스트: 그 출력의 절에 '극성 반전' 줄을 넣으십시오.
   - 컨볼루션 IR과 재생(`render.rs`, `playback.rs`): 그 출력으로 끝나는 모든 입력·출력 짝(MIMO 서포트 짝 포함)의 부호를 뒤집으십시오. `Route::from_parts`에 주는 `parts`에 출력의 고정 연산을 넣는 방식이 자연스럽습니다.
   - 헤드룸 계산은 크기만 보므로 바뀌지 않습니다.
6. 이중 적용 금지: 엔진 안의 `stage_responses`, `input_response`, mdat 내보내기용 응답, MIMO 설계는 이미 뒤집힌 측정을 쓰므로 `polarity`를 곱하지 마십시오. `secs-graph`의 응답 평가(`plan.rs:551`)는 계획의 `Invert`를 −1로 평가하는데, 이 평가에 엔진의 뒤집힌 스펙트럼을 함께 곱는 호출이 있는지 찾아 확인하고, 있으면 보고에 적으십시오(두 번 뒤집히는 자리가 있어서는 안 됩니다).

### B. 극성 검사

새 모듈 `crates/secs-engine/src/polarity.rs`를 만드십시오. `#![forbid(unsafe_code)]`인 크레이트이므로 unsafe는 없습니다. 병렬은 쓰지 마십시오(위치 수 × 짝 수의 6 ms FFT라 가볍습니다).

```rust
/// ADR 0033. 상대 극성을 재는 대역(Hz)과 직접음 구간(봉우리 앞, 뒤 ms).
pub const BAND_HZ: (f64, f64) = (300.0, 3000.0);
pub const WINDOW_MS: (f64, f64) = (1.5, 4.5);
/// 도착 뒤 봉우리를 찾는 길이(ms)와 상관의 지연 탐색 폭(±ms).
pub const PEAK_SEARCH_MS: f64 = 5.0;
pub const LAG_MS: f64 = 1.0;
/// 판정에 쓰는 |r̄|의 하한.
pub const DECISION: f64 = 0.5;

/// 한 위치에서 두 임펄스 응답의 대역 제한 직접음 교차 상관.
/// 돌려주는 값은 ±LAG_MS 안에서 절댓값이 가장 큰 정규화 상관(−1~+1)이고,
/// 어느 쪽이든 구간을 자를 수 없으면(표본이 모자라면) None입니다.
pub fn pair_correlation(a: &Samples, b: &Samples, sample_rate: u32) -> Option<f64>;

/// 메인 채널들의 상대 극성 판정. 위치마다 `pair_correlation`을 구해
/// `levels::weights`로 평균하고(두 채널이 모두 측정된 위치만), ADR 0033의
/// 규칙으로 `Note::PolarityReversed`와 `Note::PolaritySplit`을 돌려줍니다.
/// 메모의 역할 순서는 `mains`의 순서입니다.
pub fn check(set: &MeasurementSet, mains: &[String], main_position: u32) -> Vec<Note>;
```

- 봉우리: `onset_index` 뒤 `PEAK_SEARCH_MS` 안의 최대 절댓값 표본. 구간: 봉우리 − 1.5 ms부터 + 4.5 ms까지, 양 끝 10%에 Hann 테이퍼. 구간이 응답의 앞이나 뒤를 넘으면 넘는 쪽은 0으로 두십시오.
- 상관: 두 구간 뒤를 0으로 채워 길이 4096(48 kHz)으로 `secs_dsp::fft::rfft`한 뒤, 대역 가중 `W(f)`(300 Hz~3 kHz 안은 1, 양 끝 반 옥타브는 코사인으로 0까지)를 곱한 교차 스펙트럼 `X_a · conj(X_b) · W`를 `irfft`해 교차 상관을 얻고, `sqrt(Σ|X_a|²W · Σ|X_b|²W)`로 나누십시오. ±1 ms 지연 안에서 절댓값이 가장 큰 값을 돌려줍니다. 48 kHz가 아닌 레이트는 표본 수를 그 레이트로 환산하십시오(엔진에 들어오는 측정은 정책상 48 kHz지만 함수는 레이트를 받습니다).
- 판정은 ADR 0033 '판정' 항목 그대로입니다. 짝 결과 `r̄`의 절댓값이 0.5 미만이면 그 짝은 판정하지 않습니다. 반대 후보가 메인 수의 절반 미만이면 후보마다 `PolarityReversed { role, confidence, compared }`(`compared`는 그 메인의 판정된 짝 수, `confidence`는 그 짝들의 |r̄| 중앙값)를 남기고, 절반 이상이면 `PolaritySplit { groups: (Vec<String>, Vec<String>), confidence }` 하나를 남깁니다. 묶음은 서로 같은 부호끼리이고, 첫 묶음은 `mains`에서 먼저 나오는 메인이 든 쪽이며, `confidence`는 두 묶음 사이 판정된 짝의 |r̄| 중앙값입니다. 후보가 없으면 빈 벡터입니다.
- `notes.rs`에 두 변형을 추가하고, `design_pass`에서 `RcPass::Probe`가 아닌 패스에서만 한 번 `polarity::check(req.measurements, &config.mains, config.main_position)`의 결과를 메모에 넣으십시오. `subs_only` 계산에서는 하지 않습니다. 검사는 `engine_inputs`가 뒤집은 측정으로 하므로 설정으로 이미 바로잡은 스피커는 알리지 않습니다.
- 서비스 `scalar_notice`: `PolarityReversed` → code `"polarity_reversed"`, params `role`, `confidence`(f64의 `to_string`), `compared`. `PolaritySplit` → code `"polarity_split"`, params `a`, `b`(역할을 쉼표로 이은 것), `confidence`. 화면 쪽은 다른 작업서가 맡으므로 코드와 매개변수 이름을 그대로 지키십시오.

### C. 등록

`features.toml`에 세 항목을 추가하십시오. `status = "implemented"`에는 실제로 있는 시험 이름만 적으십시오(`python .github/scripts/check_feature_tests.py`가 확인합니다).

- `engine.polarity_check`(kind `engine`, upstream `"new; ADR 0033"`): 아래 `secs-engine` 시험들.
- `engine.main_polarity`(kind `engine`): 극성 설정 적용의 엔진·타입·프로젝트 시험.
- `export.main_polarity`(kind `export`): 작성기 시험.

## 시험

이름은 바꿔도 되지만 보고에 최종 이름을 적으십시오. features.toml의 이름과 맞아야 합니다.

- `secs-engine::polarity_pair_correlation_finds_sign_and_confidence`: 합성 펄스(짧은 최소 위상 응답)와 그 부호를 뒤집고 0.5 ms 늦춘 사본에서 r ≤ −0.9, 같은 부호 사본에서 r ≥ +0.9. 2 kHz 위에서만 부호가 다른 사본(트위터만 반전)에서는 |r| ≥ 0.5가 유지되어 대역 상한이 3 kHz인 이유를 보입니다.
- `secs-engine::polarity_check_flags_a_minority_and_splits_a_pair`: 메인 넷 가운데 하나를 뒤집으면 그 역할의 `PolarityReversed`(compared 3) 하나, 메인 둘 가운데 하나를 뒤집으면 `PolaritySplit` 하나, 넷 가운데 둘을 뒤집으면 `PolaritySplit` 하나.
- `secs-engine::polarity_check_averages_positions_and_skips_undecided_pairs`: 기준 위치에서만 반대이고 다른 세 위치에서 같으면 가중 평균(5 대 3)으로 반대 판정, 상관이 0.5에 못 미치는 잡음 짝은 판정하지 않음.
- `secs-engine::polarity_check_runs_once_on_the_final_pass`: 위치 일치도 상한(`RcHiBasis::Agreement`)으로 설계해도 극성 메모는 하나.
- `secs-engine::golden_rooms_have_no_polarity_notes`: 골든 방(dolby, room, room_dba)의 설계에 극성 메모가 없고, 기존 골든 시험이 그대로 통과함을 함께 확인.
- `secs-engine::inverted_main_designs_on_the_flipped_response`: 극성 `-`인 메인이 있는 설계는, 극성 `+`로 두고 그 메인의 임펄스 응답을 뒤집어 넣은 설계와 `MainChannel.polarity`와 극성 메모를 빼면 같음.
- `secs-types::main_polarity_serializes_only_when_inverted`: `polarity` 1인 사슬의 JSON과 설계 ID가 지금과 같고, −1이면 직렬화되며 없으면 1로 읽힘.
- `secs-project::inverted_mains_read_the_first_band_of_mains_only`: 메인의 첫 대역 `-`만 세고 서브와 둘째 대역은 세지 않음.
- `secs-graph::inverted_main_ends_its_output_path_with_a_fixed_invert`.
- `secs-camilla::inverted_main_inverts_the_physical_output_after_the_mimo_merge`: MIMO 서포트가 그 출력에 합류하는 설계에서 반전 필터가 `mimo_merge` 뒤 파이프라인에 있고, 메인 게인은 `inverted: false`이며, `_bypass.yml`에도 남음.
- `secs-filters::inverted_main_copies_negative_after_the_support_merge_and_lists_the_inversion`: Equalizer APO의 `Copy: X=-1.0*X`가 서포트 복사 뒤에 있고, 필터 목록에 '극성 반전' 줄이 있음.
- `secs-filters::convolution_routes_into_an_inverted_output_are_negated`: 그 출력으로 가는 메인 짝과 서포트 짝의 IR이 극성 `+`일 때의 음수.
- `secs-service::polarity_notes_map_to_notices_with_role_confidence_and_groups`.
- `secs-service::polarity_setting_marks_results_stale_and_flips_the_engine_input`: `config_set_speaker`로 `pol`을 `-`로 바꾸면 결과가 낡고, 다시 보정하면 사슬의 그 메인 `polarity`가 −1.

## 지킬 것

- 결정성: 병렬 없음, 부동소수점 누적은 원래 순서(ADR 0007). unsafe 금지(ADR 0002).
- 극성이 모두 `+`인 설계의 출력은 바이트 단위로 같아야 합니다. 골든·오라클 시험을 그대로 통과시키고, 기존 시험의 기대값을 고치지 마십시오.
- `Cargo.lock`은 cargo가 고치는 범위에서만 바뀝니다. 새 의존성은 추가하지 마십시오.
- 주석과 문서 문자열은 이웃 코드의 밀도와 말투를 따르십시오. 한국어 문구는 하십시오체, em-dash와 이모지 없음.

## 허용 파일

- `crates/secs-engine/**`(새 모듈 `src/polarity.rs`, `lib.rs`의 `pub mod`, `notes.rs`, `calibration.rs`, `input.rs`, `design.rs`, `test_support.rs`, `src/tests/**`, `examples/engine_perf.rs`, `xo.rs`)
- `crates/secs-engine-testkit/src/lib.rs`
- `crates/secs-types/src/chain.rs`와 그 시험, 재생성되는 `apps/secs2r/ui/src/ipc/bindings/MainChannel.ts`
- `crates/secs-project/src/config.rs`, `crates/secs-project/tests/**`
- `crates/secs-graph/**`, `crates/secs-camilla/**`, `crates/secs-filters/**`, `crates/secs-export/**`
- `crates/secs-service/src/calibration.rs`, `crates/secs-service/src/export.rs`, `crates/secs-service/src/cache.rs`, `crates/secs-service/src/design_export.rs`, `crates/secs-service/tests/**`
- `features.toml`, `Cargo.lock`

`apps/secs2r/ui/**`(위의 바인딩 하나 제외), `docs/**`, `CHANGELOG.md`, `CONTEXT.md`는 Claude와 다른 작업서가 고칩니다. 업스트림 파이썬 트리(`src/`, `tests/`, `tools/`)는 손대지 마십시오.

## 검증

모두 `E:\wt\p1-polarity`에서 포그라운드로 돌리고, 끝나기 전에 턴을 끝내지 마십시오. 전체 테스트는 오래 걸립니다(수십 분).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-types --locked
cargo test --workspace --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine -p secs-types -p secs-graph -p secs-camilla -p secs-filters
python .github/scripts/check_feature_tests.py
python .github/scripts/upstream.py check-tree
```

`cargo test -p secs-types`가 바꾼 `MainChannel.ts`는 커밋 대상이니 `git status`로 확인해 보고에 적으십시오.

## 보고

- 바꾼 파일 목록과 각 파일에서 한 일 한 줄.
- A-6의 확인 결과(계획의 `Invert`를 엔진 스펙트럼에 곱하는 호출이 있는지).
- 추가한 시험의 최종 이름(features.toml과 같은 이름).
- 위 검증 명령의 출력(통과 요약 줄).
- 남은 문제나 ADR과 달리 한 결정과 그 이유.
