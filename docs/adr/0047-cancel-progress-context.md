# 0047. 취소와 진행을 Context 하나로

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A6. 함께 해결하는 기술 부채: TD-207의 진행 보고 부분(서비스의 `report_progress`·`role_progress`·`JobProgressSink` 세 모양 가운데 엔진 쪽 경계).

## 배경

`secs-compute`의 `Context`는 취소 토큰(`CancelToken`), 진행 sink(`ProgressSink`), 스레드 풀, 메모리 예산을 함께 소유하는 깊은 module입니다(`crates/secs-compute/src/lib.rs:59-146, 337-470`). `CancelToken::check()`는 취소되면 `Err(Cancelled)`를 돌려주고, `Context::stage()`는 `StageProgress`를 만들어 보고 간격까지 맡습니다.

그런데 엔진의 단계 함수들은 `Context`가 생기기 전의 모양을 그대로 받습니다. `cancelled: &dyn Fn() -> bool` 매개변수가 36곳입니다(`crates/secs-engine/src/measurement.rs:198-377`, `levels.rs:119-303`, `rc.rs:872-1517`, `ba.rs:1402, 1422`, `xo.rs:167`, `fit.rs:332-495`, `export_view.rs:35`, `replay.rs:95`; `crates/secs-filters/src/lib.rs:1585, 1607, 1795`, `render.rs:115-460`). 호출자는 `design.rs:359-361, 757, 847`에서 `&|| ctx.cancel().is_cancelled()`로 다리를 놓고, 받은 쪽은 `check` 도우미 넷(`measurement.rs:190`, `rc.rs:815`, `levels.rs:249`, `design.rs:192`)에서 `bool`을 다시 `Result`로 되돌립니다. 같은 뜻의 함수가 넷이고, 그중 `design.rs:192`만 `Context`를 받습니다.

진행 보고도 여섯 모양입니다. `ctx.stage()`, `observed: &(dyn Fn(&str, Duration) + Sync)`(`design.rs:343, 388, 458, 535`, 성능 예제용 단계 시각), `progress: &dyn Fn(u64, u64)`(`secs-filters/src/lib.rs:1796`), `after_stage_write`(`secs-graph/src/files.rs:75`, 시험용 관찰 hook), `boundary: &dyn Fn(&str)`(`secs-camilla/src/lib.rs:720`, 시험용), 그리고 측정 방식의 두 번째 `Progress`·`ProgressSink`(`crates/secs-service/src/measure/mod.rs:83-90`, 역할·위치·단계·퍼센트).

이 때문에 ADR 0007의 '취소하면 1초 안에 멈춘다'는 약속을 확인하려면 매개변수 36개를 따라가야 하고, 새 단계는 어느 모양을 받을지부터 정해야 합니다. 2026-11-06 현재 코드도 리뷰가 적은 그대로입니다.

## 결정

### 취소는 `Context` 하나로 전달합니다

- 엔진의 단계 함수는 `cancelled: &dyn Fn() -> bool` 대신 `ctx: &secs_compute::Context`를 받습니다. 대상은 위에 나열한 36개 매개변수 전부이고, `secs-filters`의 렌더러와 컨볼루션 준비(`render.rs`, `lib.rs:1585-1795`)도 같습니다. `secs-filters`는 아직 `secs-compute`에 의존하지 않으므로 의존을 추가합니다. `secs-compute`는 게시 크레이트이고 운영체제 의존이 없어 ADR 0016의 규칙에 맞습니다.
- 취소 확인은 `ctx.cancel().check()?` 한 줄입니다. 이를 위해 `secs-engine`에 `impl From<secs_compute::Cancelled> for EngineError`(→ `EngineError::Cancelled`)를, `secs-filters`에 `impl From<Cancelled> for FiltersError`를 둡니다. `check` 도우미 넷은 지우고, `design.rs:192`의 `check(ctx)`도 `?`로 바뀌어 사라집니다.
- 확인하는 자리와 횟수는 지금과 같습니다. 셀마다 한 번, 단계 경계마다 한 번. 산술과 누적 순서는 바뀌지 않으므로 골든은 그대로입니다(ADR 0004).
- 폴링 횟수를 세는 시험을 위해 `secs-compute`에 `CancelToken::probe(after: usize) -> (CancelToken, Arc<AtomicUsize>)`를 둡니다. `after`번째 `is_cancelled()`부터 참을 돌려주고 호출 횟수를 셉니다. `test-support` 기능 뒤에 두어 생산 코드에는 들어가지 않습니다. 엔진의 `tests/cancel.rs::assert_cancels`는 `impl Fn(&Context) -> Result<T, EngineError>`를 받아 이 토큰으로 지금과 같은 '모든 flip 지점에서 `latency`번 안에 멈춘다' 검사를 합니다.

### 진행은 `ProgressSink` 하나로 보고합니다

- `secs-filters`의 `progress: &dyn Fn(u64, u64)`는 `ctx.stage("filters.convolution", total)`로 바뀝니다. 단계 이름은 `JobProgress.stage`로 화면까지 가는 안정 id이므로 `features.toml`의 `ipc.*` 항목과 같은 방식으로 문서에 기록합니다.
- `design.rs`의 `observed`는 성능 예제(`examples/engine_perf.rs`, TD-250)가 단계 벽시계 시간을 읽는 hook이고 진행이 아닙니다. `design_observed`와 그 아래 네 함수에만 남기되, 공개 API에서는 `design()`만 노출되는 지금 그대로입니다. A5(엔진 interface 좁히기)가 이것을 다룹니다.
- `secs-graph`의 `after_stage_write`와 `secs-camilla`의 `boundary`는 시험이 실패 지점을 끼워 넣는 hook이라 진행 보고가 아닙니다. 그대로 둡니다(A4가 작성기 도우미를 옮길 때 함께 봅니다).
- 측정 방식의 `measure::Progress`(역할, 위치, 단계, 퍼센트)는 엔진의 `Progress`(stage, done, total)와 뜻이 다르므로 타입은 남깁니다. 다만 sink는 하나입니다. `measure::ProgressSink`를 지우고, 측정 방식이 `JobContext::role_progress`를 그대로 받게 합니다(TD-171의 배선과 같은 작업이며, A-16이 그 시험을 둡니다).

### 바뀌지 않는 것

- `secs_compute::Context`, `CancelToken`, `ProgressSink`, `StageProgress`의 공개 interface. `Context::child_serial()`과 `serial()`도 그대로입니다.
- 공개 API(`secs-api`)와 wire. `design()`과 `recommend_xo()`는 이미 `&Context`를 받습니다.
- 오라클 골든과 업스트림 패리티.

## 시험

깊어진 interface에서 먼저 씁니다.

- `secs-compute`: `CancelToken::probe`가 `after`번째 확인부터 취소를 돌려주고 호출 횟수를 세는지. `Context::map`이 probe 토큰과 함께 쓰일 때 취소 지점이 기록되는지.
- `secs-engine::tests::cancel`: 기존 `assert_cancels` 사례(측정 준비, 레벨, RC 채널, BA, XO, 내보내기 뷰)가 `&Context`를 받는 새 서명으로 그대로 통과. 사례 수와 `latency` 값은 바뀌지 않습니다.
- `secs-filters`: 컨볼루션 준비가 `ctx.stage`로 `done == total`까지 보고하고, 중간 취소가 `FiltersError::Cancelled`로 끝나며 파일을 쓰지 않는지(기존 `render` 취소 시험을 `&Context`로 옮깁니다).
- 지워진 `check` 도우미를 쓰던 시험은 없습니다. 얕은 시험 삭제는 없고, 서명만 바뀝니다.

## 이행

커밋 단위입니다. 각 단계 뒤 `cargo test -p secs-compute -p secs-engine -p secs-filters --locked`와 엔진 골든이 통과해야 합니다.

1. `secs-compute`: `From<Cancelled>` 변환이 가능하도록 `Cancelled`를 그대로 두고, `test-support` 기능과 `CancelToken::probe`를 추가합니다(Daybreak).
2. `secs-engine`: `measurement.rs`, `levels.rs`, `rc.rs`, `ba.rs`, `xo.rs`, `fit.rs`, `export_view.rs`, `replay.rs`의 서명을 바꾸고 `check` 도우미 넷을 지웁니다. `design.rs`의 다리 클로저 셋을 `ctx`로 바꿉니다. `tests/cancel.rs`를 새 서명으로 옮깁니다(Daybreak, 한 커밋).
3. `secs-filters`: `render.rs`와 `lib.rs`의 서명, `progress` 클로저를 `ctx.stage`로. 호출자인 `secs-service/src/export.rs`를 맞춥니다(Daybreak).
4. `secs-service`: `measure::ProgressSink` 삭제와 `role_progress` 배선(A-16과 같은 커밋).

리뷰에서는 취소 확인 자리가 늘거나 줄지 않았는지(diff에서 `check(` 삭제 수와 `.check()?` 추가 수가 같은지), 골든이 비트 동일한지, 공개 crate의 서명 변경이 `secs-api`에 닿지 않는지 확인합니다.

## 결과

- 취소 지연의 상한은 `Context` 한 곳의 계약이 되고, 새 단계는 `ctx.cancel().check()?` 한 줄로 취소에 참여합니다.
- 진행 보고 모양은 엔진·작성기에서 `ProgressSink` 하나, 측정 방식에서 `role_progress` 하나로 줄어듭니다. 시험용 관찰 hook 둘은 남는데, 다음 리뷰가 다시 제안하지 않도록 이 ADR이 그 이유(실패 지점 주입)를 기록합니다.
- `secs-filters`가 `secs-compute`에 의존합니다. 둘 다 게시 크레이트라 crates.io 의존 그래프에 한 변이 늘고, 다음 minor에서 `secs-filters`의 공개 서명이 바뀝니다. ADR 0016이 호환을 약속하는 것은 긴 이름 크레이트 하나뿐이므로 허용됩니다.
- 편차 항목은 없습니다. 수치와 출력이 바뀌지 않기 때문입니다.
- 위험: `&dyn Fn() -> bool`을 받던 자리에 `&Context`를 전달하려면 호출자가 `Context`를 가져야 합니다. 시험 안의 호출자는 `Context::serial()`을, 재생(`replay.rs`)은 호출자의 `ctx`를 씁니다. `Context::serial()`은 예산이 무제한이므로, 예산 검사가 있는 단계(`memory::reserve`)를 시험이 우회하지 않도록 리뷰에서 확인합니다.
