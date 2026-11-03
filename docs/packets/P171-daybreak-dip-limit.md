# P171 Daybreak: 딥 보정 제한

ADR 0034(`docs/adr/0034-dip-limit.md`)를 구현하십시오. 먼저 그 ADR과 `CONTEXT.md`, `CLAUDE.md`를 읽으십시오. 작업 트리는 `E:\wt\p1-dip-limit`(브랜치 `claude/p1-dip-limit`)이고, 그 안에서만 일하십시오. 줄 번호는 그 트리의 `origin/main` 기준입니다.

## 목표

보정 설정 `dip_limit`(기본 꺼짐)을 추가하고, 켜면 메인 RC의 목표 보정에 ADR 0034의 딥 상한을 둡니다. 끄면 계산 결과와 골든이 비트 단위로 같아야 합니다.

## 지금 코드의 사실

- RC의 목표 보정은 `crates/secs-engine/src/rc.rs`의 `prepare_correction`에서 만듭니다. 위치 가중 평균 전력 `power`를 `smoothed_db`(1/24 옥타브와 1/3 옥타브 바닥)로 `sm`을 만들고, `corr = (target + level − sm).clamp(−25, 상한)`으로 묶은 뒤(`rc.rs:864-876`), 부스트 금지 마스크, 롤오프 경사, 긴 모드 상한, 구간 마스크를 `min`으로 겹칩니다(`rc.rs:890-909`). `design_db = corr.clone()`(`rc.rs:910`)이 설계 목표입니다.
- 평활 도우미는 `smooth(x, fraction)`(`rc.rs:98-100`, `smooth::smooth_power`를 `Fraction::Scalar`로 부름)이며 `fraction` 3.0이 1/3 옥타브입니다. 1옥타브는 1.0입니다.
- `mode_decay`가 켜져 있으면 `sm`을 바이쿼드를 넣은 응답으로 다시 만듭니다(`rc.rs:847-862`). 딥 상한은 그 뒤의 `sm`과 같은 `power`로 만든 기준 곡선을 써야 합니다.
- 옵트인 설정의 배선은 `hf_phase`와 같습니다. `secs-types/src/calibration.rs:178`(필드), `:243`(기본값), `:347-348`(`engine_key` 접미사), `:836-853`(시험), `secs-types/src/settings_edit.rs:117`(`SettingEdit`), `secs-project/src/calibration.rs:29`(`CALIBRATION_SHAPE`의 선택 키), `:75`(읽기), `:259-260`(켰을 때만 쓰기), `secs-project/tests/calibration.rs`, `secs-engine/src/input.rs:193`(`EngineConfig`), `secs-engine/src/calibration.rs:231`, `secs-engine/src/setup.rs:39, 134`, `secs-engine/src/test_support.rs:154`, `secs-engine-testkit/src/lib.rs:156`, `secs-service/src/settings_edit.rs:385`, `secs-service/src/calibration.rs:924-935`(알림), `apps/secs2r-cli/src/main.rs:208, 670, 712-713`(`--hf-phase`), `apps/secs2r-cli/tests/cli.rs:144-179`.
- RC 상한의 위치 일치도 탐색 패스(`design.rs:546-551`)는 `lf_align`, `mode_decay`, `hf_phase`를 끄지만 `low_protect`는 켠 채로 둡니다. `dip_limit`도 켠 채로 둡니다.
- 메모는 `secs-engine/src/notes.rs`의 `Note`이고, 서비스 `scalar_notice`(`secs-service/src/calibration.rs:887`)의 `match`가 모든 변형을 다룹니다. `secs-engine/src/tests/design.rs:35, 636-660`과 `secs-service/src/calibration.rs:1776`의 시험도 새 변형을 알아야 합니다.

## 바꿀 것

1. 설정 `dip_limit: bool`을 `CalibrationSettings`, `SettingEdit::DipLimit { on }`, `.secs`(켰을 때만 `dip_limit: true`), `engine_key`(켰을 때만 `|dip_limit`), `EngineConfig`, `Setup`, 서비스 편집, CLI `project set --dip-limit on|off`에 `hf_phase`와 같은 방식으로 추가하십시오. ts-rs 바인딩은 `cargo test -p secs-types`로 다시 만드십시오.
2. `rc.rs`의 `prepare_correction`에서 `s.dip_limit`이 켜져 있으면, `corr`를 상한으로 묶은 직후(부스트 금지 마스크 전)에 다음을 하십시오.
   - `env = smooth(&power, 1.0)`를 dB로 바꾼 기준 곡선(`mode_decay`가 `power`를 다시 만들었으면 그 뒤의 `power`).
   - 빈마다 `d = max(env − sm, 0)`, `a = if d <= 2 { d } else { 2 + (d − 2) / 2 }.min(6)`.
   - `corr[i] = corr[i].min(target[i] + level − env[i] + a)`.
   - 줄어든 양 `before − corr[i]`를 기억해 두었다가, `d ≥ 4`이고 줄어든 양이 0.5 dB 이상인 빈을 이어진 묶음으로 나눠, 묶음마다 가장 깊은 빈의 (주파수, `d`, `a`)를 메모 후보로 모으십시오. 깊은 순으로 6개까지입니다.
3. `Note::DipLimit { role: String, dips: Vec<(f64, f64, f64)> }`를 추가하고(주파수 Hz, 깊이 dB, 메운 양 dB), 켜져 있으면 메인마다 하나씩 남기십시오. 줄인 딥이 없으면 빈 벡터입니다. 서비스 알림: code `"dip_limit"`, params `role`, `count`, 그리고 `dips`를 `"hz:depth:fill,hz:depth:fill"`(소수 둘째 자리) 꼴 한 문자열로 넣으십시오. 화면 쪽은 다른 작업서가 맡습니다.
4. `features.toml`에 `calibration.dip_limit`(kind `workflow`, upstream `"new; ADR 0034"`)과 `cli.dip_limit`을 추가하고 실제 시험 이름을 적으십시오.

## 시험

- `secs-engine::dip_limit_fills_narrow_dips_partly_and_broad_loss_fully`: 합성 방에서 1/4 옥타브 8 dB 딥의 보정이 `2 + 3 = 5 dB`로 묶이고, 2옥타브 6 dB 넓은 레벨 차이는 그대로 고쳐짐.
- `secs-engine::dip_limit_off_and_golden_rooms_are_bit_identical`: 끈 설계와 골든 방이 비트 단위로 같음(`tests/hf_phase.rs:290`의 꼴).
- `secs-engine::dip_limit_notes_list_the_deepest_limited_dips`: 메모가 깊은 순 6개까지이고 값이 맞음.
- `secs-engine::dip_limit_stays_on_in_the_agreement_probe`.
- `secs-types::dip_limit_defaults_off_and_preserves_legacy_engine_key`.
- `secs-project::dip_limit_round_trip_and_legacy_default`.
- `secs-service::dip_limit_edits_persist_and_mark_results_stale`, `secs-service::dip_limit_notices_list_dips`.
- `secs2r-cli::project_set_persists_dip_limit_switch`.

## 지킬 것

결정성(ADR 0007), unsafe 금지(ADR 0002), 기존 시험의 기대값 변경 금지, 새 의존성 금지. 한국어 문구는 하십시오체, em-dash와 이모지 없음.

## 허용 파일

`crates/secs-types/**`, `crates/secs-project/**`, `crates/secs-engine/**`, `crates/secs-engine-testkit/**`, `crates/secs-service/**`, `apps/secs2r-cli/**`, `features.toml`, `Cargo.lock`, 재생성되는 `apps/secs2r/ui/src/ipc/bindings/**`. `apps/secs2r/ui/**`의 나머지와 `docs/**`, `CHANGELOG.md`는 손대지 마십시오.

## 검증

`E:\wt\p1-dip-limit`에서 포그라운드로 돌리고 끝나기 전에 턴을 끝내지 마십시오.

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
