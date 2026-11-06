# 0054. 공개 API 하나가 세 바인딩의 이벤트, 대기, 오류 모양을 가진다

상태: 제안, 2026-11-06

근거: 감사 이슈 #186, 아키텍처 리뷰 A13. 해결하는 기술 부채 발견은 TD-160(오류 모양 셋), TD-161(1013 닫힘이 보통의 끝으로 보임), TD-205(C ABI의 `RESULTS` 상한 없음과 해제 지연 미기록), TD-206(엔진을 버려도 잡이 계속 돎), TD-209(`wait_job` 시한 초과의 뜻과 README 공백)이고, TD-150(파이썬 이벤트를 멈출 수 없음)과 TD-168(`Engine::service()`의 호환 약속)도 여기서 정합니다.

## 배경

ADR 0016은 공개 크레이트 `sound-equalization-and-correction-system-rust-api`(`crates/secs-api`)가 `Engine`과 `Client` 두 adapter를 같은 interface로 준다고 정했습니다. 2026-11-06의 코드에서 다시 센 결과, 두 adapter가 같은 것은 `Secs::call` 하나뿐입니다(`lib.rs:150-153`).

- 이벤트: `Engine`은 `Observer`(`:184-246`)가 유한 큐(1024)와 버림 횟수, 종료 이벤트 32개 보존을 가집니다. `Client::events`(`:626`, `:713-791`)는 막히는 iterator만 주고 큐도 버림 횟수도 없습니다.
- 잡 대기: `wait_job`(`:421-448`)은 `Engine`에만 있고, 시한 초과를 `Error::Protocol`로 보고합니다.
- .NET 바인딩(`bindings/dotnet/native/src/core.rs:22-63, 254-336`)이 `Client`를 위해 pump 스레드와 `SubscriptionReceiver::{Engine, Client}`, `Dropped::{Engine, Client}`를 다시 구현했고, `:272-290`은 `Observer::send`의 `try_send`/`Full`/`Disconnected` 분기를 그대로 되풀이합니다.
- 파이썬 바인딩은 다시 구현하지 않았습니다. `EngineEvents`(`bindings/python/src/lib.rs:72-99`)는 버림 횟수가 있고 `ClientEvents`(`:154-172`)는 막히는 iterator라 같은 패키지 안에서 `Engine`과 `Client`의 이벤트 의미가 다릅니다. 둘 다 GIL을 놓은 채 시한 없이 막혀 `KeyboardInterrupt`가 닿지 않고 `close()`가 없습니다(TD-150).
- 오류 변환이 세 벌입니다. `crates/secs-api/src/lib.rs:156-161, 806-816`, `bindings/python/src/lib.rs:189-214`(필드 셋), `bindings/dotnet/native/src/core.rs:381-417`(필드 다섯). `ErrorCode`를 문자열로 바꾸는 네 줄이 두 바인딩에 똑같이 있고, ADR 0032는 셋이 같은 모양이라고 했지만 실제로는 다릅니다(TD-160).
- 로컬 API 서버는 느린 구독을 코드 1013 `missed N events`로 끝내는데(`crates/secs-local-api/src/lib.rs:481-487`) `Events::next`가 모든 종료를 `None`으로 바꾸므로 소비자는 누락을 모릅니다(TD-161).
- CLI(`apps/secs2r-cli/src/main.rs:417-455, 632`)는 `secs-api`를 쓰지 않고 `Service`를 직접 만들어 자기 `Condvar` 대기를 가집니다. `EngineBuilder`(`crates/secs-api/src/lib.rs:297-331`)에 `process`·`memory`·`apply`·`managed_host`·`eqapo_install`·`clock` setter가 없어 올라탈 수 없기 때문입니다.

deletion test로 보면 .NET의 pump와 enum 둘을 지웠을 때 다른 바인딩에서 다시 나타납니다(conda-forge의 파이썬은 같은 코드이니 네 번째 언어까지). `secs-api`가 소유하면 어디에도 다시 나타나지 않습니다.

## 결정

### `Secs` trait가 세 함수를 가진다

```rust
pub trait Secs {
    fn call(&self, request: Request) -> Result<Response>;
    fn wait_job(&self, job: JobId, timeout: Duration) -> Result<JobEvent>;
    fn subscribe(&self) -> Result<Subscription>;
}
```

- `Subscription { receiver: mpsc::Receiver<Event>, dropped: DroppedEventCount, closer: SubscriptionCloser }`. `Event`는 `Job(JobEvent)`, `Changed { method }`, `Lagged { missed }`, `Closed { reason }`의 enum입니다. 큐 상한은 두 adapter 모두 1024이고, 가득 차면 새 이벤트를 버리고 `dropped`를 올립니다. 종료 이벤트와 `Lagged`·`Closed`는 버리지 않고 자리가 날 때까지 기다립니다. `closer.close()`는 다른 스레드에서 구독을 끝냅니다.
- `Engine` adapter는 지금의 `Observer`입니다. `Client` adapter는 .NET이 가지고 있던 pump 스레드를 `secs-api`로 옮긴 것입니다. 1013 종료는 `Lagged { missed }` 뒤에 `Closed`로 끝납니다.
- `wait_job`: `Engine`은 지금처럼 보존된 종료 이벤트를 봅니다. `Client`는 구독을 시작해 종료 이벤트를 기다리고, `/v1/status`의 실행 중 목록에 없는 잡은 돌려줄 종료 이벤트가 없으므로 `Error::JobNotFound { job }`로 답합니다. 시한 초과는 새 variant `Error::Timeout { job }`입니다(TD-209). 상대가 프로토콜을 어긴 것이 아니므로 `Protocol`을 쓰지 않습니다.
- `Drop for Engine`은 `service.shutdown(Duration::from_secs(3))`을 부릅니다(TD-206). 드롭 뒤의 종료 이벤트는 사라지며 rustdoc에 그렇게 기록합니다.

### 서버는 누락을 이벤트로 알린다

`secs_types::local_api::ServerEvent`에 `Lagged { missed: u64 }`를 추가하고, `secs-local-api`는 1013으로 끝내기 전에 그 프레임을 보냅니다. 큐 크기(`EVENT_CAPACITY` 256)와 1013 규칙을 ADR 0016의 로컬 API 절에 기록합니다.

### 오류 모양은 `WireError` 하나

```rust
pub struct WireError { pub code: String, pub message: String, pub detail: String,
                       pub params: BTreeMap<String, String>, pub retryable: bool }
impl From<&Error> for WireError { … }
```

- `code`는 snake_case입니다(`IpcError`의 serde 표기, `transport_error`, `protocol_error`, `timeout`, `job_not_found`). `Error`의 `Display`도 같은 code를 출력합니다. 지금의 `SECS API error ProjectNotOpen: …`처럼 Debug 표기를 출력하지 않습니다.
- 두 바인딩은 `WireError`를 직렬화만 합니다. C ABI 봉투의 `error`는 이 다섯 필드이고(지금 모양과 같음), 파이썬 `SecsError`는 `params`와 `retryable` 속성을 추가하며, C# `SecsException`은 `Params`(`IReadOnlyDictionary<string, string>`)와 `Retryable`을 추가합니다. ADR 0032의 '같은 모양'이 참이 됩니다.

### `EngineBuilder`가 빠진 seam을 받는다

`process`, `memory`, `apply`, `managed_host`, `eqapo_install`, `clock`(`clock_offset_seconds`와 함께), `unix_ms`, `uuid`, `acceleration`, `log_dir` setter를 추가합니다. CLI는 `secs_api::Engine`을 쓰고 자기 `Events`와 `Condvar` 대기를 지웁니다. 이벤트 출력은 `observer`로 합니다. ADR 0053의 `native_deps`가 만든 `Deps`를 builder에 넣는 `EngineBuilder::from_deps(deps)`를 두어 두 조립이 겹치지 않게 합니다.

### 파이썬과 C ABI

- 파이썬은 `_Events` 하나가 `Subscription`을 사용합니다. `__next__`는 `recv_timeout(100 ms)`를 돌며 사이마다 `py.check_signals()`를 불러 `KeyboardInterrupt`가 닿게 하고, `close()`는 `closer.close()`이며, `events(timeout=None)`의 시한이 다하면 `None`이 아니라 `TimeoutError`를 냅니다(TD-150). `dropped_events()`는 두 adapter에 같이 있습니다. README와 `.pyi`에 큐 1024, 막힘, 닫기, `Lagged`를 기록합니다.
- C ABI의 `RESULTS` 표는 1024개를 상한으로 하고, 넘으면 가장 오래된 결과를 버려 그 핸들은 `invalid_handle`이 됩니다. `secs_events_free`가 구독 뮤텍스 때문에 호출자의 `timeout_ms`까지 늦을 수 있다는 것과 함께 `ffi.rs`의 문서와 `docs/UNSAFE_BOUNDARIES.md`에 기록합니다(TD-205). `secs_events_next`의 두 arm은 `Subscription` 하나를 읽는 arm 하나가 됩니다.

### 호환 약속의 문장

ADR 0016의 결정 첫 항목에 한 문장을 추가합니다. '판 호환을 약속하는 표면은 `Secs` trait의 세 함수, `Subscription`과 `Event`, `WireError`, `EngineBuilder`의 setter입니다. `Engine::service()`와 builder가 받는 내부 crate의 trait(`AudioBackend`, `Transport`)는 불안정한 탈출구이며 rustdoc에 그렇게 표시합니다'(TD-168). 0.x에서 minor는 호환이 깨질 수 있으므로 이 ADR의 변경(`Error` variant 추가, `Display` 변경)은 다음 minor에 들어갑니다.

## 시험

- `crates/secs-api/tests/public_api.rs`: 같은 사례 목록을 `Engine`과 로컬 API 서버에 연결한 `Client`에 돌립니다. `call` 오류가 `WireError`로 같은 다섯 필드가 되는지, `subscribe`가 잡 이벤트를 순서대로 주고 느린 소비자가 `dropped`를 올리는지(엔진)와 `Lagged`를 받는지(클라이언트), `wait_job`이 끝난 잡과 모르는 잡과 시한 초과를 각각 어떻게 답하는지, `closer.close()`가 막힌 `recv`를 끝내는지, `Engine`을 드롭하면 잡 스레드가 3초 안에 끝나는지.
- `WireError`의 직렬화를 문자열 골든으로 고정합니다(`code`가 snake_case, 필드 순서).
- `crates/secs-local-api/tests/local_api.rs`: 느린 구독이 `Lagged { missed }` 프레임 뒤에 1013으로 끝나는 사례(TD-161).
- 파이썬 `bindings/python/tests/test_secs2r.py`: `events(timeout=0.2)`가 유휴 엔진에서 돌아오는지, 다른 스레드의 `close()`가 막힌 `next()`를 끝내는지, 모의 방 흐름(`project_new sample`, `measure_configure mock`, `measure_position`, `calibrate`, `export`)이 휠에서 도는지(TD-159). TD-127의 검사(`dropped_events()`가 막힌 수신자를 잠그지 않음)도 여기로 옮깁니다(TD-158).
- .NET `PackageTests.cs`: `SecsException.Params`가 거절의 `field`를 담는지, `EventStream`이 `Lagged`를 그대로 전달하는지.
- CLI: `recommend-xo`·`calibrate`의 종료 대기가 `Engine::wait_job`으로 같은 결과를 내는지 기존 CLI 시험으로 확인합니다.

## 이행

1. `WireError`와 `Error::{Timeout, JobNotFound}`, `Display` 변경, 두 바인딩의 직렬화(TD-160, TD-209).
2. `ServerEvent::Lagged`와 서버의 프레임, `Client` 구독 pump의 이동, `Secs::subscribe`·`wait_job`, .NET의 enum 둘 삭제(TD-161).
3. 파이썬 `_Events` 통합과 시험 이동(TD-150, TD-158, TD-159).
4. `Drop for Engine`, `RESULTS` 상한과 문서(TD-205, TD-206).
5. `EngineBuilder` setter와 CLI 이동. ADR 0016의 문장(TD-168).

## 결과

- 바인딩 셋이 같은 세 함수와 같은 오류 모양을 받습니다. 큐 상한, 버림 집계, 닫기, 누락 알림이 `secs-api` 한 곳에 있습니다(#155 TD-127이 다룬 자리).
- `Error`에 variant가 둘 늘고 `Display`가 바뀌므로 라이브러리 판의 minor가 오릅니다. 파이썬과 C#의 오류 객체는 속성이 늘 뿐 기존 속성은 그대로입니다.
- .NET의 200 ms 폴링(ADR 0032)은 그대로이고 폴링이 읽는 큐의 소유자만 바뀝니다.
- CLI가 모든 명령에서 공개 API를 쓰므로 CI가 공개 API를 매일 시험합니다.
- 계산과 내보낸 파일은 바뀌지 않습니다. 편차 항목은 없습니다.
