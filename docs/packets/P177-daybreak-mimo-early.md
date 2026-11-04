# P177 Daybreak: MIMO 초기 구간 설정

ADR 0037(`docs/adr/0037-mimo-early.md`)을 구현하십시오. 먼저 그 ADR과 `CONTEXT.md`, `CLAUDE.md`를 읽으십시오. 작업 트리는 `E:\wt\p1-mimo-early`(브랜치 `claude/p1-mimo-early`)이고 그 안에서만 일하십시오.

## 목표

MIMO 설정 `mimo_early_ms`(기본 60, 범위 5~60)를 추가하고, MIMO 설계가 보호하는 초기 구간 `EARLY`를 이 값에서 계산하게 합니다. 60이면 FIR·IIR·run 골든과 비트 단위로 같아야 합니다.

## 지금 코드의 사실

- `crates/secs-engine/src/mimo/design.rs`의 `FS_LOW = 600`, `EARLY = 36`(60 ms). `block_pair`가 `t ≥ onset + EARLY`인 행만 잔차에 넣고 그 앞은 목표 0에 `early_weight`를 둡니다. 순위 매기기(`rank_supports`)와 설계(`design_selected`)가 같은 상수를 씁니다. `iir.rs`의 `build_problem`은 `onset + 36`에서 구간을 나눕니다. `EARLY`를 쓰는 곳을 모두 찾으십시오(`T60Guard`, `late_energy` 등 늦은 구간의 정의에도 쓰일 수 있으니 하나씩 확인하고, 가드의 '늦은 에너지' 정의는 바꾸지 마십시오. 서포트가 작용하는 시작점만 설정을 따릅니다. 가드가 같은 상수를 쓰고 있다면 가드 쪽은 60 ms 상수로 남겨 두고 보고에 적으십시오).
- MIMO 설정의 배선은 `mimo_fir`(불리언)와 `fsiso`·`mimo_low`(숫자)가 본보기입니다. `CalibrationSettings`(`crates/secs-types/src/calibration.rs`), `CalibrationLimits`(범위), `SettingEdit`, `.secs` 저장(`crates/secs-project/src/calibration.rs`, 60이 아닐 때만 쓰는 선택 키), `mimo_key`(60이 아닐 때만 `|early=<ms>`), `MimoKey`·`MimoRequest`(`crates/secs-engine/src/mimo/run.rs`, `design.rs`), 서비스 편집(`crates/secs-service/src/settings_edit.rs`, 범위 검사), CLI(`apps/secs2r-cli/src/main.rs`의 `--fsiso`와 같은 꼴로 `--mimo-early <ms>`).
- 이 트리에는 극성 검사, 딥 보정 제한, 채널 사이 위상 검사, MIMO 위치별 가드(`mimo_seat_guard`, P174)가 이미 들어 있습니다. 설정 배선은 P174의 `mimo_seat_guard`가 가장 가까운 본보기입니다.

## 바꿀 것

1. `mimo_early_ms: f64`(기본 60.0, 한도 5~60)를 설정·편집·저장·키·요청·서비스·CLI에 추가하십시오. 엔진은 계산 입구에서 범위 밖이나 유한하지 않은 값을 입력 오류로 거부합니다. ts-rs 바인딩은 `cargo test -p secs-types`로, 기록 응답은 `SECS_RECORD=1 cargo test -p secs-service --test recorded --locked`로 다시 만드십시오.
2. `EARLY` 상수를 `MimoRequest`의 값에서 계산한 표본 수(`round(ms × FS_LOW / 1000)`, 하한 3)로 바꾸고, FIR 설계의 `block_pair`·순위·설계와 IIR의 `build_problem` 구간이 같은 값을 쓰게 하십시오. 60이면 지금의 36과 같아야 합니다.
3. `features.toml`에 `calibration.mimo_early`(kind `workflow`, upstream `"new; ADR 0037"`)와 `cli.mimo_early`를 추가하고 실제 시험 이름을 적으십시오.

## 시험

- `secs-engine::mimo_early_sixty_keeps_goldens_bit_identical`: 60(그리고 필드가 없는 옛 설정)의 FIR·IIR·run 결과가 골든과 같음.
- `secs-engine::mimo_early_shorter_window_lets_supports_act_sooner`: 합성 방(모드 둘, 위치 셋)에서 20 ms가 60 ms보다 온셋 뒤 20~60 ms 구간의 잔여 에너지를 줄이고 가드를 통과함.
- `secs-engine::mimo_early_clamps_to_three_samples`: 5 ms가 3 표본이고 그 아래는 입력 오류.
- `secs-types::mimo_early_defaults_sixty_and_preserves_legacy_mimo_key`.
- `secs-project::mimo_early_round_trip_and_legacy_default`.
- `secs-service::mimo_early_edits_validate_persist_and_mark_only_mimo_stale`.
- `secs2r-cli::project_set_persists_mimo_early`.

## 지킬 것

결정성(ADR 0007), unsafe 금지(ADR 0002), 기존 시험의 기대값 변경 금지, 새 의존성 금지. cargo는 이 트리의 기본 target 폴더로 빌드하고 `CARGO_TARGET_DIR`을 다른 트리로 주지 마십시오.

## 허용 파일

`crates/secs-types/**`, `crates/secs-project/**`, `crates/secs-engine/**`, `crates/secs-engine-testkit/**`, `crates/secs-service/**`, `apps/secs2r-cli/**`, `features.toml`, `Cargo.lock`, 재생성되는 `apps/secs2r/ui/src/ipc/bindings/**`와 `apps/secs2r/ui/src/ipc/recorded/**`. `apps/secs2r/ui/**`의 나머지와 `docs/**`, `CHANGELOG.md`는 손대지 마십시오.

## 검증

`E:\wt\p1-mimo-early`에서 포그라운드로 돌리고 끝나기 전에 턴을 끝내지 마십시오.

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

바꾼 파일과 한 일 한 줄씩, `EARLY`를 쓰던 자리마다 설정을 따르게 했는지 상수로 남겼는지, 추가한 시험의 최종 이름, 검증 명령의 통과 요약, 남은 문제.
