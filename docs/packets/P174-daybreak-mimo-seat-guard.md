# P174 Daybreak: MIMO 위치별 가드

ADR 0036(`docs/adr/0036-mimo-seat-guard.md`)을 구현하십시오. 먼저 그 ADR과 `CONTEXT.md`, `CLAUDE.md`를 읽으십시오. 작업 트리는 `E:\wt\p1-seat-guard`(브랜치 `claude/p1-seat-guard`)이고 그 안에서만 일하십시오.

## 목표

MIMO 설정 `mimo_seat_guard`(기본 꺼짐)를 추가하고, 켜면 MIMO의 대역 가드와 T60 가드(FIR과 IIR 실현 모두)가 위치의 합 대신 가장 나빠지는 위치를 봅니다. 끄면 MIMO 계산은 골든과 비트 단위로 같아야 합니다.

## 지금 코드의 사실

- `crates/secs-engine/src/mimo/design.rs`: `BandGuard`(`energies`가 대역마다 모든 위치의 늦은 에너지를 `total`에 더함, `evaluate_band`가 1/3 옥타브 대역과 늦은 봉우리 ±1.5 Hz 대역의 최악 `10·log10(after/before)`를 냄), `T60Guard`(`evaluate_mode`가 모드마다 위치들의 기준 에너지와 시험 에너지를 각각 더한 뒤 비교), `GuardStep::passes`(대역 ≤ 1 dB, 꼬리 ≤ 0.15), `design_selected`의 다섯 단계 −3 dB 줄이기.
- `crates/secs-engine/src/mimo/iir.rs`: `BandGuard::power`가 위치를 더하고, `OutcomeGuard`가 `T60Guard`를 다시 쓰며, `postprocess`가 `mimo_worse_db`로 위반을 계산합니다.
- MIMO 설정은 `crates/secs-types/src/calibration.rs`의 `CalibrationSettings`(`mimo_on`, `fsiso`, `mimo_low`, `mimo_max_supports`, `mimo_fir`, `mimo_iir_analytic_derivatives`)이고, 낡음 키는 `mimo_key()`입니다(`engine_key`가 아님). 엔진 쪽은 `mimo/run.rs`의 `MimoKey::from_settings` → `MimoKey::request()` → `MimoRequest`(`design.rs`)입니다. `SettingEdit`의 MIMO 변형(`mimo_fir` 등)과 `.secs` 저장(`crates/secs-project/src/calibration.rs`, 켰을 때만 쓰는 선택 키), 서비스 편집(`crates/secs-service/src/settings_edit.rs`), CLI(`apps/secs2r-cli/src/main.rs`의 `--mimo-fir`)가 본보기입니다.
- 메모는 `notes.rs`의 `MimoNote::{GuardReduced, GuardDisabled}`이고 `cause: GuardCause`를 가집니다. 서비스 변환은 `crates/secs-service/src/calibration.rs`의 `notices`/`scalar_notice`입니다.

## 바꿀 것

1. 설정 `mimo_seat_guard: bool`을 `CalibrationSettings`(기본 `false`), `SettingEdit`(`mimo_seat_guard { on }`), `.secs`(켰을 때만 `mimo_seat_guard: true`), `mimo_key`(켰을 때만 `|seat_guard`), `MimoKey`·`MimoRequest`, 서비스 편집, CLI `project set --mimo-seat-guard on|off`에 `mimo_fir`와 같은 방식으로 추가하십시오. ts-rs 바인딩은 `cargo test -p secs-types`로 다시 만드십시오.
2. 켜져 있으면 네 가드가 위치마다의 값을 따로 두었다가 가장 큰 비율을 쓰게 하십시오. 대역 가드는 위치마다 `10·log10(after/before)`의 최댓값, T60 가드는 위치마다 꼬리 에너지 비율의 최댓값입니다. 한도(1 dB, 0.15), 모드 선택(가장 센 모드의 20 dB 안), 줄이는 절차(−3 dB 다섯 번 뒤 끔)는 그대로입니다. 꺼져 있으면 지금의 합산 코드를 그대로 타십시오(같은 식에 `β`를 끼우는 방식이 아니라 분기해서, 끈 결과가 비트 단위로 같게).
3. `GuardCause`에 `position: Option<u32>`를 추가해 켰을 때 판정이 나온 위치를 넣으십시오. 꺼져 있으면 `None`입니다. 서비스 알림에는 값이 있을 때만 `position` 매개변수를 넣으십시오. 화면 쪽은 다른 작업서가 맡습니다.
4. `features.toml`에 `calibration.mimo_seat_guard`(kind `workflow`, upstream `"new; ADR 0036"`)와 `cli.mimo_seat_guard`를 추가하고 실제 시험 이름을 적으십시오.

## 시험

- `secs-engine::mimo_seat_guard_rejects_a_support_that_worsens_one_position`: 합성 방에서 위치 하나만 나빠지는 서포트가 끄면 통과하고 켜면 줄어들며, 메모의 `position`이 그 위치.
- `secs-engine::mimo_seat_guard_off_keeps_goldens_bit_identical`: 끈 계산이 MIMO 골든(FIR·IIR·run)과 같음.
- `secs-engine::mimo_seat_guard_applies_to_iir_guards`.
- `secs-types::mimo_seat_guard_defaults_off_and_preserves_legacy_mimo_key`.
- `secs-project::mimo_seat_guard_round_trip_and_legacy_default`.
- `secs-service::mimo_seat_guard_edits_persist_and_mark_only_mimo_stale`.
- `secs2r-cli::project_set_persists_mimo_seat_guard_switch`.

## 지킬 것

결정성(ADR 0007), unsafe 금지(ADR 0002), 기존 시험의 기대값 변경 금지, 새 의존성 금지. 병렬은 map만.

## 허용 파일

`crates/secs-types/**`, `crates/secs-project/**`, `crates/secs-engine/**`, `crates/secs-engine-testkit/**`, `crates/secs-service/**`, `apps/secs2r-cli/**`, `features.toml`, `Cargo.lock`, 재생성되는 `apps/secs2r/ui/src/ipc/bindings/**`. `apps/secs2r/ui/**`의 나머지와 `docs/**`, `CHANGELOG.md`는 손대지 마십시오.

## 검증

`E:\wt\p1-seat-guard`에서 포그라운드로 돌리고 끝나기 전에 턴을 끝내지 마십시오.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-types --locked
cargo test --workspace --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-engine -p secs-types
python .github/scripts/check_feature_tests.py
python .github/scripts/upstream.py check-tree
```

## 보고

바꾼 파일과 한 일 한 줄씩, 추가한 시험의 최종 이름, 검증 명령의 통과 요약, 남은 문제.
