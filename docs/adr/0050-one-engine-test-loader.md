# 0050. 엔진 시험 로더 하나

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A9. 함께 해결하는 기술 부채: TD-251(testkit이 `test_support.rs`의 사본), TD-142의 남은 부분.

## 배경

엔진의 오라클 사례 로더가 두 벌입니다. 안쪽 시험이 쓰는 `crates/secs-engine/src/test_support.rs`(743줄, `cfg(test)`)와 바깥 크레이트가 쓰는 `crates/secs-engine-testkit/src/lib.rs`(606줄)입니다. `crate::`를 `secs_engine::`으로 바꾼 것과 `MimoAcousticMetrics` 본문을 빼면 같은 파일이고(diff 160줄), `load_fixture`는 diff가 없습니다. `EngineConfig`에 필드가 생길 때마다 두 로더에 같은 줄을 넣어야 했습니다(`inverted_mains: Vec::new()`, `dip_limit: false`가 둘 다에 있습니다). 감사 #155의 TD-142는 문서만 고쳤습니다(`docs/ENGINE.md:45`).

바깥 소비자는 둘입니다. `crates/secs-export/tests/golden.rs:10`과 `crates/secs-service/src/curves_tests.rs:13, 48`이 `secs_engine_testkit::replay_case`를 씁니다. testkit은 `secs-engine`의 `test-support` 기능을 켜서 `secs_engine::testkit::mimo_acoustic_metrics`(`crates/secs-engine/src/testkit.rs:16`)를 가져옵니다. 안쪽 시험은 `crates/secs-engine/src/tests/mod.rs:9`에서 `pub(crate) use crate::test_support as cases;`로 로더를 받습니다.

리뷰는 `test_support.rs`를 지우고 안쪽 시험이 testkit을 dev-dependency로 쓰자고 했습니다. 그 방향 그대로는 되지 않습니다. Cargo는 dev-dependency 순환을 허용하지만, `secs-engine`의 단위 시험 바이너리 안에서 `crate::Design`과 `secs_engine::Design`(testkit이 의존하는 라이브러리 빌드의 것)은 서로 다른 타입입니다. 로더가 돌려주는 `Case`의 `config`, `measurements`, `design`이 모두 엔진 타입이므로 안쪽 시험은 그 값을 `crate::` 함수에 넣을 수 없습니다. 안쪽 시험을 `tests/`로 옮길 수도 없습니다. 단계 안쪽 시험(`rc.rs`, `low_protect.rs`, `rc_upper.rs`)은 `pub(crate)` 항목에 닿아야 하고, 리뷰의 'Fine as is'가 그것을 그대로 두기로 했기 때문입니다.

## 결정

로더는 `secs-engine` 안에 한 벌만 두고, testkit 크레이트는 그것을 다시 내보내는 얇은 facade가 됩니다. 방향이 리뷰와 반대이지만 결과는 같습니다. 743줄이 한 벌이 됩니다.

### `secs_engine::testkit`이 로더를 소유합니다

- `crates/secs-engine/src/test_support.rs`의 내용을 `crates/secs-engine/src/testkit.rs`로 옮깁니다. module은 `#[cfg(any(test, feature = "test-support"))] pub mod testkit;`으로 컴파일됩니다. 안쪽 시험은 `crate::testkit`을, 바깥은 `secs_engine::testkit`을 쓰므로 타입이 한 벌입니다.
- 로더가 쓰는 `secs-oracle`과 `serde_json`은 `test-support` 기능이 켜는 선택적 의존(`optional = true`)이 되고 dev-dependency로도 남습니다. `secs-engine`은 게시 크레이트이므로 `secs-oracle`(게시하지 않음)은 기본 기능에 들어가지 않습니다. `cargo publish --dry-run`이 이것을 확인합니다(ADR 0016의 게시 게이트).
- 공개 interface는 지금 두 로더가 공통으로 가진 것입니다. `names()`, `load(name) -> Case`, `load_fixture(&Fixture) -> Case`, `production_case(name, threads) -> ReplayCase`, `replay_case(name) -> ReplayCase`, `Replay::for_case(name)`, `pairs`, `manual`, `peq`, `MimoAcousticMetrics`, `mimo_acoustic_metrics`. `read_recorded_fit`·`same_recorded_fit`·`replay`·`replay_with_context`처럼 안쪽 시험만 쓰는 것은 `pub(crate)`로 두고, 그 가운데 `replay`와 `replay_with_context`는 리뷰대로 `tests/mod.rs`에 남깁니다.

### testkit 크레이트는 facade입니다

`crates/secs-engine-testkit/src/lib.rs`는 `pub use secs_engine::testkit::*;` 한 줄과 crate 문서만 남습니다. 바깥 소비자 둘의 `use` 경로는 바뀌지 않습니다. 크레이트를 지우지 않는 까닭은 `secs-export`·`secs-service`가 `secs-engine`의 `test-support` 기능을 직접 켜면 그 크레이트들의 보통 빌드에 기능이 새어 들어가기 때문입니다(feature unification). dev-dependency인 facade가 그 경계입니다.

### 바뀌지 않는 것

오라클 사례 파일과 골든, `secs-engine`의 기본 기능 빌드(로더가 들어가지 않음), 바깥 소비자의 `use` 경로, 안쪽 시험의 `cases::` 별칭.

## 시험

깊어진 interface에서 먼저 씁니다. 로더 자체의 시험은 testkit에 둡니다.

- `names()`가 `oracle/goldens/engine_inputs`의 사례 이름과 같고, `load(name)`가 모든 사례를 오류 없이 읽으며 `replay_case(name).design`이 기록된 설계와 같은지(지금 `tests/replay.rs:8-20`이 하는 일을 로더의 시험으로 한 번 더 두지 않고, 그 시험을 로더의 시험으로 봅니다).
- `secs-export/tests/golden.rs`와 `secs-service/src/curves_tests.rs`가 수정 없이 통과하는지. 이것이 facade의 시험입니다.
- 얕은 시험 삭제: 없습니다. 사본이 사라질 뿐입니다.

## 이행

1. `test_support.rs`를 `testkit.rs`로 합치고 `cfg`와 선택적 의존을 둡니다. `tests/mod.rs`의 `cases` 별칭을 `crate::testkit`으로 바꿉니다. 한 커밋(Daybreak).
2. testkit 크레이트를 facade로 줄입니다. 한 커밋.
3. `docs/ENGINE.md:45`와 `lib.rs:82-86`의 문서를 실제와 맞춥니다. 한 커밋(docs).

각 단계 뒤 `cargo test -p secs-engine -p secs-engine-testkit -p secs-export -p secs-service --locked`와 `cargo publish --workspace --dry-run --locked --allow-dirty`(`api_dry_run.py`를 거친 버릴 사본에서)가 통과해야 합니다. 리뷰에서는 `secs-engine`의 기본 빌드가 `secs-oracle`을 끌어오지 않는지(`cargo tree -p secs-engine -e normal`), 두 로더의 diff 160줄 가운데 남는 것이 `MimoAcousticMetrics` 본문뿐인지 확인합니다.

## 결과

- 오라클 사례 형식이 바뀌면 한 파일만 고칩니다. `EngineConfig`의 새 필드도 한 곳입니다.
- `secs-engine-testkit`은 이름만 남는 facade가 됩니다. 다음 리뷰가 '이름만 바꾼 module'로 다시 제안하지 않도록, 이 ADR이 그 이유(feature unification의 경계)를 기록합니다.
- 위험: `test-support` 기능이 켜진 `secs-engine`은 `secs-oracle`을 의존합니다. 워크스페이스 안에서는 `secs-export`와 `secs-service`의 시험 빌드가 이미 그렇게 돌고 있으므로 새로운 것은 아닙니다.
- 편차 항목은 없습니다.
