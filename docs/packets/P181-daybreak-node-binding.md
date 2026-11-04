# P181 Daybreak: Node 바인딩 격리 크레이트 `secs-node-sys`

ADR 0040(`docs/adr/0040-npm-package.md`)의 Rust 부분을 구현하십시오. 작업 트리는 `E:\wt\npm-binding`(브랜치 `claude/npm-binding`)이고 그 안에서만 일하십시오. 커밋하지 마십시오. 빌드 디렉터리는 이 트리의 `target`만 쓰십시오(`CARGO_TARGET_DIR`을 다른 곳으로 두지 마십시오).

## 배경

- 본보기는 Python 바인딩 `bindings/python/src/lib.rs`(PyO3, 같은 JSON 봉투)와 .NET 격리 크레이트 `bindings/dotnet/native`(lint 표, `unsafe-budget.toml`, `docs/UNSAFE_BOUNDARIES.md`의 절, `features.toml`의 `api.dotnet`)입니다. 공개 API는 `crates/secs-api`(`engine`·`client` 기능)이고 JSON 타입은 `crates/secs-types/src/ipc.rs`(`Envelope`, `IpcError { code, params, retryable, detail }`)와 `local_api.rs`(`ServerEvent`)에 있습니다.
- napi-rs의 `#[napi]` 매크로는 생성 코드에 `#[allow(unsafe_code)]`를 넣어 루트의 `#![forbid(unsafe_code)]`와 양립하지 않습니다(E0453, 2026-11-04 확인). 그래서 이 크레이트는 루트에 `#![deny(unsafe_code)]`를 두고, 손으로 쓰는 `unsafe` 토큰은 0개입니다.
- 판은 `napi = "3.16.1"`, `napi-derive = "3.7.0"`, `napi-build = "2.6.0"`(2026-11-04의 안정판, Rust 1.88 이상)입니다. napi의 기본 기능은 끄고 `napi8`, `tokio_rt`, `serde-json`을 켜십시오. 문서는 https://napi.rs/docs/concepts/class , https://napi.rs/docs/concepts/async-fn , https://napi.rs/docs/concepts/error-handling , https://docs.rs/napi/3.16.1 입니다. 확신 없는 API는 WebFetch로 확인하고 쓰십시오.

## 할 일

1. **정책 게이트.** `unsafe-budget.toml`에 새 표 `[macro-unsafe]`를 추가하고 `secs-node-sys = "napi-derive 3.7.0 emits #[allow(unsafe_code)] in generated code (ADR 0040)"`를 적으십시오. 파일 머리 주석에 이 표의 뜻(손으로 쓴 unsafe는 0이지만 의존 매크로가 만든 unsafe 때문에 `forbid` 대신 `deny`를 두는 크레이트)을 추가하십시오. `crates/secs-policy/tests/gates.rs`를 고치십시오.
   - `Budget` 구조체가 `macro_unsafe: BTreeMap<String, String>`도 읽게 하십시오(없으면 빈 표).
   - `every_crate_root_forbids_unsafe`: `macro-unsafe`에 있는 크레이트는 예산이 0이어도 라이브러리·바이너리·빌드 스크립트 루트에 `#![deny(unsafe_code)]`가 있어야 하고 `#![forbid(unsafe_code)]`가 있으면 안 됩니다. 시험·예제·벤치 루트는 지금처럼 `forbid`입니다.
   - `budgeted_crates_carry_the_unsafe_lint_table`: `macro-unsafe` 크레이트도 예산이 있는 크레이트와 같은 lint 표(`[lints.rust] unsafe_code = "deny"`, `unsafe_op_in_unsafe_fn = "deny"`, clippy 셋)를 가져야 하고 `[lints] workspace = true`를 쓰면 안 됩니다.
   - `no_unsafe_outside_budget`: 그대로 두십시오(`secs-node-sys`의 토큰 수는 0이어야 통과).
   - `budget_names_only_existing_crates`: `macro-unsafe`의 이름도 실제 크레이트여야 합니다. `workspace_members_are_all_covered_by_policy_gates`의 목록에 `("secs-node-sys", "bindings/node")`를 추가하십시오.
   - 게이트의 새 규칙마다 통과와 실패를 보는 단위 시험을 추가해 두십시오(지금 파일의 방식대로 임시 디렉터리나 문자열 입력으로).
2. **크레이트.** `Cargo.toml`(작업 공간)의 `members`에 `"bindings/node"`를 추가하고 `bindings/node/Cargo.toml`을 만드십시오. `name = "secs-node-sys"`, `description = "The Node-API isolation crate for the secs2r npm package."`, `version = "0.20.0"`, `edition.workspace`, `rust-version.workspace`, `license = "AGPL-3.0-only"`, `repository.workspace`, `publish = false`, `[lib] name = "secs2r_node"`, `crate-type = ["cdylib"]`, .NET 크레이트와 같은 `[lints.rust]`·`[lints.clippy]` 표, 의존 `napi`, `napi-derive`, `secs`(.NET과 같은 줄), `serde_json.workspace`, `tokio = { version = "1", default-features = false, features = ["rt-multi-thread"] }`(napi의 `tokio_rt`가 요구하는 것만), `[build-dependencies] napi-build = "2.6.0"`. `build.rs`에는 `#![forbid(unsafe_code)]` 아래 `napi_build::setup()` 호출만 두십시오. `LICENSE`는 루트 `LICENSE-AGPL`을 복사해 두십시오.
3. **모듈.** `src/lib.rs`(`#![deny(unsafe_code)]`, 문서 주석에 ADR 0040), `src/core.rs`(napi와 무관한 순수 Rust: 요청 검증, 봉투 풀기, 오류 변환, 이벤트 큐), `src/engine.rs`, `src/client.rs`, `src/events.rs`(napi 클래스)로 나누십시오. `core.rs`는 Rust 단위 시험으로 검증하고, napi 클래스는 얇게 두십시오.
   - `Engine`: `#[napi(factory)] pub async fn create(app_settings_path: Option<String>)`(생성이 설정 파일을 읽으므로 `spawn_blocking`), `#[napi] pub async fn call(&self, request: serde_json::Value) -> Result<serde_json::Value>`(Python의 `call`과 같이 성공 봉투를 풀어 `Response`를 돌려주고, 실패 봉투는 오류), `#[napi(js_name = "waitJob")] pub async fn wait_job(&self, job_id: f64, timeout_ms: f64) -> Result<serde_json::Value>`(`job_id`는 안전한 음이 아닌 정수, `timeout_ms`는 유한한 0 이상, 아니면 `invalid_argument` 오류), `#[napi] pub fn events(&self) -> Result<EngineEvents>`(생성 즉시 구독해 started 이벤트를 놓치지 않게).
   - `Client`: `#[napi(factory)] pub async fn create(port: Option<u16>, token: Option<String>, token_path: Option<String>)`(둘 다 주면 거부, 기본 포트 4712), `call`, `status`, `events`. 모두 `&self`로 두고 상태는 `Arc`에 두십시오. `Send`·`Sync`는 손으로 구현하지 말고 컴파일로 확인하십시오.
   - `EngineEvents`와 `ClientEvents`: 구독마다 전용 읽기 스레드가 공개 API의 이벤트를 받아 유한한 큐(기본 1024, 넘치면 가장 오래된 것을 버리고 센다)에 넣게 하고, `#[napi] pub async fn next(&self) -> Result<Option<serde_json::Value>>`가 큐에서 하나를 기다리게 하십시오(끝나면 `None`). `#[napi] pub fn close(&self)`는 종료를 표시하고 읽기 스레드를 깨웁니다(엔진 구독은 `recv_timeout`으로 종료 표시를 확인, 클라이언트 구독은 공개 API의 closer로). `#[napi(js_name = "droppedEvents")] pub fn dropped_events(&self) -> BigInt`(엔진만). 종료한 구독이 프로세스 종료를 막지 않게 하십시오(읽기 스레드는 `close` 뒤 끝나고, 열린 채 버려진 구독은 `Drop`에서 닫습니다).
   - 오류: napi `Error`를 던지되 JS 쪽이 `SecsError`를 만들 수 있도록 메시지를 JSON 한 줄 `{"code":..,"detail":..,"params":..,"retryable":..}`로 두고 `reason`을 `secs_error`로 두십시오(napi `Error::new(Status::GenericFailure, message)`). 어떤 모양이 JS에서 `code`를 가장 안전하게 복원하는지 문서로 확인해 보고에 적으십시오. 패닉은 공개 API처럼 `internal` 오류로 바꾸십시오.
   - JSON 값은 napi의 `serde-json` 기능으로 JS 객체와 직접 오가게 하십시오. u64가 JS 수를 넘는 경우는 공개 API가 JSON 수를 쓰므로 그대로 두십시오.
4. **문서와 등록.** `docs/UNSAFE_BOUNDARIES.md`에 'npm 패키지의 Node-API (secs-node-sys, ADR 0040)' 절을 추가하십시오(손으로 쓴 unsafe 없음, 매크로가 만든 unsafe의 범위, `deny`를 두는 이유). `features.toml`에 `api.node` 항목을 `partial`로 추가하고 Rust 단위 시험 이름을 적으십시오(JS 시험은 P182가 추가). `.github/scripts/release_gate.py`가 판을 올리는 매니페스트 목록에 `bindings/node/Cargo.toml`을 추가하고 `test_release_gate.py`를 맞추십시오(`package.json`은 P182에서).
5. **시험.** `core.rs`의 단위 시험: 요청 검증(음수·비정수 작업 번호, 무한 timeout 거부), 성공 봉투 풀기와 실패 봉투의 오류 변환(`code`·`detail`·`params`·`retryable` 보존), 큐 넘침 시 가장 오래된 것부터 버리고 센 수가 맞음, `close` 뒤 `next`가 `None`, 토큰과 토큰 경로를 둘 다 주면 거부. 이름은 `features.toml`에 적은 것과 같게 하십시오.

## 지킬 것

- 손으로 쓰는 `unsafe`는 0개입니다. `unsafe impl Send/Sync` 금지.
- 운영체제·GPU 의존을 직접 추가하지 마십시오. `secs-sys`, `secs-cuda-sys`에 의존하지 마십시오.
- `apps/**`, `crates/**`(`secs-policy`의 시험 파일 제외), `bindings/python/**`, `bindings/dotnet/**`는 고치지 마십시오.
- 주석은 이웃 크레이트의 영어 어투를 따르고, 한국어 문서는 합니다체, em-dash와 이모지 금지.

## 허용 파일

`bindings/node/**`(JS 파일은 만들지 않음), `Cargo.toml`(members만), `Cargo.lock`, `unsafe-budget.toml`, `crates/secs-policy/tests/gates.rs`, `docs/UNSAFE_BOUNDARIES.md`, `features.toml`, `.github/scripts/release_gate.py`, `.github/scripts/test_release_gate.py`.

## 검증

포그라운드로 돌리고 끝날 때까지 턴을 끝내지 마십시오.

```sh
cd E:\wt\npm-binding
cargo fmt --all -- --check
cargo clippy -p secs-node-sys -p secs-policy --all-targets --locked -- -D warnings
cargo test -p secs-node-sys -p secs-policy --locked
cargo deny --all-features check
python -m unittest discover -s .github/scripts -p "test_release_gate.py"
python .github/scripts/check_feature_tests.py
```

`check_feature_tests.py`가 로컬 Windows의 플랫폼 조건부 시험 둘 때문에 실패하면 그 둘만인지 확인해 보고에 적으십시오.

## 보고

바꾼 파일, 위 명령의 마지막 줄, 오류 모양에 대해 확인한 문서와 선택, 남은 문제(특히 Node 없이 확인하지 못한 것).
