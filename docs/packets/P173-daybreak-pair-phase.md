# P173 Daybreak: 메인 채널 사이의 위상 검사 메모

ADR 0035(`docs/adr/0035-pair-phase.md`)를 구현하십시오. 먼저 그 ADR과 `CONTEXT.md`, `CLAUDE.md`를 읽으십시오. 작업 트리는 `E:\wt\p1-pair-phase`(브랜치 `claude/p1-pair-phase`)이고 그 안에서만 일하십시오.

## 목표

보정 계산의 최종 패스가 끝나면 메인 짝마다 기준 위치에서 보정 뒤 응답의 위상차와 상쇄를 재어, 상쇄가 −6 dB 이하인 짝을 `Note::PairPhase`로 알립니다. 설정은 없고, 계산 결과와 내보낸 파일은 바뀌지 않습니다. 골든은 그대로 통과해야 합니다(메모만 늘어날 수 있으며, 메모를 비교하는 시험은 새 변형을 알아야 합니다).

## 지금 코드의 사실

- 보정 뒤 응답은 `crates/secs-engine/src/chain.rs`의 `stage_responses(design, role, spectra, position)`가 줍니다. 메인 단계(입력 보정, 메인 게인, 정렬 지연, top PEQ, XO HP)의 응답을 쓰고 서브 단계는 쓰지 않습니다. `design.rs`의 `design_pass`가 설계를 끝낸 뒤 `MainDesign.after`(기준 위치의 `combined × response`)를 만드는 자리 근처가 검사를 넣을 곳입니다. 기준 위치는 `config.main_position`, 스펙트럼은 `prepared.h`입니다.
- 설계 격자와 빈은 `crates/secs-engine/src/grid.rs`의 `design_freqs()`와 `BINS`입니다.
- `RcPass::Probe`(위치 일치도 탐색)와 `subs_only`에서는 하지 않습니다(극성 검사 `polarity::check`가 같은 조건으로 들어가 있으니 그 옆에 두십시오).
- 메모는 `notes.rs`의 `Note`, 서비스 변환은 `crates/secs-service/src/calibration.rs`의 `scalar_notice`이며 `match`가 모든 변형을 다룹니다. `crates/secs-engine/src/tests/design.rs`의 `note_value`와 `kinds`, 서비스의 `every_engine_note_maps_to_a_notice_code_and_params` 시험도 고쳐야 합니다.

## 바꿀 것

1. 새 모듈 `crates/secs-engine/src/pair_phase.rs`를 만드십시오.

```rust
/// ADR 0035. 검사 대역(Hz)과 1/3 옥타브 중심 주파수.
pub const BAND_HZ: (f64, f64) = (40.0, 200.0);
pub const CENTERS_HZ: [f64; 6] = [50.0, 63.0, 80.0, 100.0, 125.0, 160.0];
/// 알리는 상쇄의 한도(dB)와 짝 수의 상한, 상쇄의 바닥.
pub const LOSS_DB: f64 = -6.0;
pub const MAX_PAIRS: usize = 6;
pub const FLOOR_DB: f64 = -30.0;

/// 한 대역의 결과: (lo_hz, hi_hz, phase_deg, loss_db).
pub type Band = (f64, f64, f64, f64);

/// 두 응답의 1/3 옥타브 대역마다 위상차(도, (−180, 180])와 상쇄(dB)를 돌려줍니다.
/// 위상차는 Σ H_a·conj(H_b)의 각, 상쇄는 10·log10(Σ|H_a+H_b|² / Σ(|H_a|+|H_b|)²)를 FLOOR_DB로 묶은 값입니다.
pub fn bands(f: &[f64], a: &[Complex64], b: &[Complex64]) -> Vec<Band>;

/// 메인 짝마다 `bands`를 구해 LOSS_DB 이하인 대역이 있는 짝을 가장 나쁜 상쇄 순으로 MAX_PAIRS까지 `Note::PairPhase`로 돌려줍니다.
/// `responses`는 (역할, 응답)이며 역할 순서는 설계 순서입니다.
pub fn check(f: &[f64], responses: &[(String, Vec<Complex64>)]) -> Vec<Note>;
```

   - 대역 경계는 중심의 2^(±1/6)배이고, 빈은 `lo ≤ f < hi`로 넣습니다. 빈이 없는 대역은 건너뜁니다.
   - `Note::PairPhase { a: String, b: String, bands: Vec<Band> }`에는 LOSS_DB 이하인 대역만 넣습니다.
2. `design_pass`에서 최종 패스(Probe가 아니고 `subs_only`가 아님)에 메인마다 기준 위치의 메인 단계 응답을 모아 `pair_phase::check`를 부르고 메모에 넣으십시오. 응답을 얻지 못한 메인(측정 없음)은 뺍니다.
3. `scalar_notice`: code `"pair_phase"`, params `a`, `b`, `bands`(`"lo:hi:phase:loss,..."`, 주파수 정수, 위상과 상쇄 소수 첫째 자리). 화면 쪽은 다른 작업서가 맡습니다.
4. `features.toml`에 `engine.pair_phase`(kind `engine`, upstream `"new; ADR 0035"`)를 추가하고 실제 시험 이름을 적으십시오.

## 시험

- `secs-engine::pair_phase_bands_measure_phase_difference_and_loss`: 같은 응답은 0°·0 dB, 90°는 −3 dB, 120°는 −6 dB, 반대는 FLOOR_DB.
- `secs-engine::pair_phase_check_reports_only_cancelling_pairs_worst_first`: 셋 가운데 한 짝만 120° 넘게 어긋나면 그 짝만, 여러 짝이면 가장 나쁜 순, MAX_PAIRS 상한.
- `secs-engine::pair_phase_runs_once_on_the_final_pass_and_keeps_goldens`: 골든 방 설계가 메모 외에는 비트 단위로 같고, 위치 일치도 상한으로 설계해도 짝 메모가 중복되지 않음.
- `secs-service::pair_phase_notes_map_to_notices_with_bands`.

## 지킬 것

결정성(ADR 0007), unsafe 금지(ADR 0002), 기존 시험의 기대값 변경 금지, 새 의존성 금지. 병렬은 쓰지 마십시오.

## 허용 파일

`crates/secs-engine/**`, `crates/secs-service/src/calibration.rs`, `crates/secs-service/tests/**`, `features.toml`, `Cargo.lock`. `apps/**`, `docs/**`, `CHANGELOG.md`, `CONTEXT.md`는 손대지 마십시오.

## 검증

`E:\wt\p1-pair-phase`에서 포그라운드로 돌리고 끝나기 전에 턴을 끝내지 마십시오.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine
python .github/scripts/check_feature_tests.py
python .github/scripts/upstream.py check-tree
```

## 보고

바꾼 파일과 한 일 한 줄씩, 추가한 시험의 최종 이름, 검증 명령의 통과 요약, 남은 문제.
