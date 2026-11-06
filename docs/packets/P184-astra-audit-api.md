# P184 (ASTRA): audit #186, Category A findings in the wire types, the public API and the bindings

Tree: `E:\wt\audit-api`, a git worktree on branch `audit/186-api` (created by
Claude from `fix/audit-186`). Work only there. Every cargo command runs with
`CARGO_TARGET_DIR=C:\cargo-target\audit-api`. Commit on that branch, one
commit per finding, in the order below. Do not push, do not rebase, do not
spawn sub-agents, do not touch `audit-state.md`.

Allowed files:

- `crates/secs-types/**` (the wire types; run `cargo test -p secs-types --locked`
  after every change so `apps/secs2r/ui/src/ipc/bindings/*.ts` is regenerated,
  and commit the regenerated bindings with the change);
- `crates/secs-api/**`, `crates/secs-local-api/**` (keep away from the
  `eprintln!` at `lib.rs:241` and the `ServerConfig` log field that another
  worker adds: your edits there must be local to the lines you need);
- `bindings/python/**`, `bindings/dotnet/**`;
- `apps/secs2r-cli/**`;
- `apps/secs2r/src/host.rs`, `apps/secs2r/src/local_api.rs` (A-37 only);
- `crates/secs-service/src/lib.rs` and `crates/secs-service/src/ipc.rs`
  only to delete the `recommend_xo` special case (A-7);
- `apps/secs2r/ui/src/ipc/recommendXo.ts` and the regenerated
  `apps/secs2r/ui/src/ipc/bindings/**`;
- `docs/adr/0032-nuget-package.md` (one sentence, A-8) and
  `docs/UNSAFE_BOUNDARIES.md` (A-35);
- `features.toml`: register every new test name in the `[[feature]]` entry
  that covers the behaviour (`ipc.*`, `api.*`, `bindings.*` entries) or add
  one in the same style; `python3 .github/scripts/check_feature_tests.py`
  must accept it.

Forbidden: everything else, in particular `crates/secs-service/**` beyond
the two deletions above, `crates/secs-engine/**`, the screens, `CHANGELOG.md`.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0003-ipc-and-screens.md`
(or the ADR that freezes method ids; search `docs/adr` for "메서드 id"),
`docs/adr/0016-public-rust-api-and-local-api.md`, `docs/adr/0032-nuget-package.md`,
`crates/secs-types/src/ipc.rs` (the `Request`/`Response` enums, `IpcError`,
`ErrorCode`, `examples()` around line 2922), `crates/secs-types/src/local_api.rs`,
`crates/secs-api/src/lib.rs`, `crates/secs-local-api/src/lib.rs`,
`bindings/python/src/lib.rs`, `bindings/python/secs2r/__init__.pyi`,
`bindings/python/tests/test_secs2r.py`, `bindings/dotnet/native/src/{core.rs,ffi.rs}`,
`bindings/dotnet/Secs2R/Native.cs`, `bindings/dotnet/Secs2R.Tests/PackageTests.cs`,
`apps/secs2r-cli/src/main.rs` (`deps()` at 587-627), and the finding texts
quoted below.

## Why

The monthly audit (issue #186) found behaviour defects on the wire and in
the bindings. Each is a Category A item of `/115dkk-skills:resolve-audit`:
observable, so made test-first at the agreed seam named below. Expected
values come from literals or the finding, never from the code under test.
The maintainer decided (2026-11-06) that changing the payload of an existing
variant is a 0.x wire change: the method id stays, the shape may change,
the bindings are regenerated.

## The protocol for every item

Identical to P183: announce; Red (the named test fails on the assertion,
not on compilation; stub first if needed); Green (minimal); Tidy (only this
item's lines); gate; commit with `git add <files>` and

```
fix(audit): A-n <한국어 한 문장> (#186 TD-xxx)

<Red 시험이 무엇을 증명하는지 한두 문장. 바뀐 동작 한 문장.>

Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>
```

(하십시오체, no em-dash, no program-as-subject verbs 묻다/적다/넘기다,
"추가하다" not "더하다"). Gate per item: `cargo fmt --all -- --check`,
`cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`,
`cargo test -p <touched crates> --locked`; every fifth commit and at the end
`cargo test --workspace --locked` in the foreground.

Local tooling: `dotnet --version` tells whether the .NET SDK is here; if it
is, `dotnet test bindings/dotnet/Secs2R.Tests` after A-8 and A-35. For
Python: create a venv once (`python -m venv C:\cargo-target\audit-api\venv`),
`pip install maturin pytest`, and `maturin develop --release` from
`bindings/python` (with `CARGO_TARGET_DIR` set as above); then
`python -m pytest bindings/python/tests -q`. If `maturin develop` cannot
build here, write the tests anyway, run `cargo check -p secs2r-python`
(or the crate's real name from `bindings/python/Cargo.toml`), and report
that CI's wheel job verifies them.

## Items

### A-7 · TD-157 / A22 · `RecommendXo` is a unit variant with a string special case

Finding: "`Request::RecommendXo`가 unit variant라서 `Service::call_value`가
JSON의 `"recommend_xo"`를 문자열로 먼저 잡아 `{background}`를 받고
`ObservedEvents` 감싸기(676-682)를 다시 씁니다. typed 경로(`Secs::call`, CLI
`recommend-xo`, Python·.NET의 `Request`)는 배경 추천을 요청할 수 없고, ts-rs의
`Request`에는 `params`가 없어 화면이 타입을 넓혀 캐스팅합니다."
Recommendation: `RecommendXo(Option<RecommendXoParams>)` (adjacent tagging
reads an absent `params` as `None`), delete the special case
(`crates/secs-service/src/lib.rs:711-738`, `ipc.rs:52`), regenerate the
bindings, keep `recommend_xo_accepts_absent_null_and_background_params` as
the wire test, method id unchanged.

Fixed interface: `Request::RecommendXo(#[serde(default)] RecommendXoParams)`
if the enum's tagging makes an absent `params` deserialize to the default;
otherwise `RecommendXo(Option<RecommendXoParams>)`. Check `examples()` and
`every_method_name_is_listed_and_parses` in `secs-types` and
`local_api.rs`'s access table; they must keep passing. The CLI's
`recommend-xo` gets `--background`.

Seam: `Service::call_value` and `Service::call`. Red tests in
`crates/secs-types/src/ipc.rs` tests: `recommend_xo_accepts_absent_null_and_background_params`
(three JSON literals: no `params`, `"params": null`, `{"background": true}`
all parse to `Request::RecommendXo(..)` with `background` false/false/true)
and in `crates/secs-service/tests/service.rs` (allowed only for this test;
add it at the end of the file): `typed_recommend_xo_can_ask_for_a_background_job_td157`
(`Service::call(Request::RecommendXo(RecommendXoParams { background: true }))`
starts a job whose `Started` event says background, compared with a literal).
Then delete the special case; `call_value` must still accept the three
literal shapes (reuse the first test through `call_value`).

Also update `apps/secs2r/ui/src/ipc/recommendXo.ts` to the regenerated
type (no cast); run `npm run typecheck` in `apps/secs2r/ui` (Node 24;
`npm ci` once if `node_modules` is missing).

Commit subject: `fix(audit): A-7 RecommendXo 요청이 typed 페이로드로 배경 추천을 받습니다 (#186 TD-157)`.

### A-8 · TD-160 / A13 · three error shapes across the local API, the C ABI and Python

Finding: "오류 모양이 셋입니다. 로컬 API는 `IpcError`(`code, params, retryable, detail`,
`message` 없음), C ABI 봉투는 `message`를 더하고 `params`·`retryable`을
유지하지만 C#이 `params`를 버리며, 파이썬 `SecsError`는 `code`·`message`·`detail`뿐입니다.
ADR 0032는 셋이 '같은 모양'이라고 적습니다. `message`는 `Error::Display`라 코드를
Debug 표기로 찍고 `code`는 snake_case입니다." Recommendation: `WireError { code,
message, detail, params, retryable }` with `From<Error>` in `secs-api`; the
two bindings only serialize it; `SecsError` gains `params` and `retryable`,
`SecsException` gains `Params`; `Display` prints the snake_case code; fix
the ADR sentence.

Fixed interface (`crates/secs-api/src/lib.rs`):

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireError {
    pub code: String,                       // snake_case ErrorCode
    pub message: String,                    // one line, starts with the code
    pub detail: Option<String>,
    pub params: BTreeMap<String, String>,
    pub retryable: bool,
}
impl From<&Error> for WireError { .. }
```

Seam: the serialized error. Red tests: `wire_error_has_one_shape_for_every_error_kind_td160`
in `crates/secs-api` (an `IpcError` with code `ProjectNotOpen`, params
`{"field":"x"}`, retryable true → the literal JSON
`{"code":"project_not_open","message":"project_not_open: …","detail":…,"params":{"field":"x"},"retryable":true}`),
`python_error_carries_params_and_retryable_td160` in
`bindings/python/tests/test_secs2r.py` (a call on a closed project raises
`SecsError` with `.code == "project_not_open"` and `.params == {}` and
`.retryable is False`), and in `bindings/dotnet/Secs2R.Tests/PackageTests.cs`
`SecsExceptionCarriesParams` if the SDK is present (else add the test and
report it as CI-verified). Delete the four duplicated `ErrorCode`-to-string
lines in both bindings.

Commit subject: `fix(audit): A-8 공개 API와 두 바인딩이 오류 모양 하나를 씁니다 (#186 TD-160)`.

### A-9 · TD-161 · the 1013 close looks like a normal end of stream

Finding: "느린 WebSocket 읽기는 코드 1013 "missed N events"로 닫히는데
`Events::next`가 모든 `Close`를 `None`으로 바꾸므로 Python·.NET 소비자는 보통의
스트림 끝으로 보고, 잡의 종료 이벤트가 들어 있을 수 있는 누락을 알지 못합니다."
Recommendation: send a `ServerEvent::Lagged { missed }` text frame before
closing, or make `Events::next` answer `Some(Err(Error::Protocol("missed N events")))`
on 1013; add a case to `crates/secs-local-api/tests/local_api.rs`.

Choose the event: `ServerEvent::Lagged { missed: u64 }` in
`crates/secs-types/src/local_api.rs` (regenerate bindings), sent before the
1013 close; `secs-api`'s `Events::next` yields it as a normal event; the
.NET and Python event streams pass it through as an event whose kind is
`"lagged"`.

Seam: the local API event stream. Red test
`slow_reader_receives_lagged_before_the_1013_close_td161` in
`crates/secs-local-api/tests/local_api.rs`: a subscriber that stops reading
while `EVENT_CAPACITY + 5` events are produced; when it reads again the next
text frame is `{"event":"lagged","missed":N}` with `N >= 5` (literal lower
bound), then the close frame with code 1013.

Commit subject: `fix(audit): A-9 느린 구독에 누락 수를 알리는 lagged 이벤트를 보냅니다 (#186 TD-161)`.

### A-1 · TD-150 · Python events cannot be stopped

Finding: "`EngineEvents.__next__`는 GIL을 놓은 채 시한 없는 `recv()`에,
`ClientEvents.__next__`는 소켓 읽기에 막힙니다. 유휴 엔진이나 앱의 `events()`를
도는 파이썬 프로그램은 멈출 방법이 없습니다. `close()`도 `timeout`도 없고 GIL을
다시 잡지 않아 `KeyboardInterrupt`도 전달되지 않습니다. 러스트의
`EventsCloser`(secs-api)는 노출되지 않았습니다." Recommendation:
`recv_timeout(100 ms)` loop with `py.check_signals()` between waits,
`close()` (engine: drop the receiver; client: `EventsCloser::close`), an
optional `timeout` argument, README and `.pyi`.

Fixed Python surface: `events(timeout: float | None = None)`; iteration
raises `StopIteration` when closed or when `timeout` seconds pass without an
event (document which; prefer `StopIteration` on timeout and a `.closed`
property); `close()` on both iterators; `KeyboardInterrupt` propagates
within 100 ms.

Seam: the Python iterator. Red tests in `bindings/python/tests/test_secs2r.py`:
`test_events_close_ends_iteration_td150` (a thread blocked in `next()`;
`close()` from the main thread; the thread ends within 1 s),
`test_events_timeout_returns_td150` (`events(timeout=0.2)` on an idle engine;
`next()` raises `StopIteration` within 1 s), and
`test_keyboard_interrupt_reaches_a_blocked_next_td150` (`_thread.interrupt_main()`
from a timer while `next()` blocks → `KeyboardInterrupt` within 1 s).
Update `__init__.pyi` and `bindings/python/README.md`.

Commit subject: `fix(audit): A-1 파이썬 이벤트 반복을 멈출 수 있습니다 (#186 TD-150)`.

### A-4 · TD-152 / A12 · the CLI's `Deps` lacks `clock_offset_seconds`

Finding: "CLI의 `deps()`가 `clock`에는 운영체제 UTC 오프셋을 넣으면서
`clock_offset_seconds`는 빠뜨려 기본값 0이 됩니다. `export.rs`는 이 둘로 완료
시각을 UTC로 되돌리므로 … CLI가 `secs-design.json`과 `last_export`에 적는 시각이
한국에서는 9시간 어긋납니다." Recommendation: one `Deps::native()` (or
`secs_sys::native_deps`) fills both and both apps use it; until then add the
field to the CLI and test an export's `created_utc` under a non-zero offset.

Fixed interface: `pub fn native_deps(app_version: Cow<'static, str>) -> Deps`
in `apps/secs2r-cli/src/main.rs` is not shareable with the Tauri app (it
lives in another binary); so add a small shared constructor in `secs-sys`:
`secs_sys::clock::local_clock() -> (Arc<dyn Fn() -> LocalTime + Send + Sync>, Arc<dyn Fn() -> i32 + Send + Sync>)`
that returns the two matching closures, and use it in the CLI's `deps()`.
`apps/secs2r/src/main.rs` is another worker's file: do not edit it; note in
the report that it should call the same constructor (Claude wires it).

Seam: the CLI's export manifest. Red test
`cli_export_records_utc_under_a_nonzero_clock_offset_td152` in
`apps/secs2r-cli` tests (the crate has CLI tests; follow their pattern of
running a deterministic export into a temp folder): with `deps()` built in
a mode whose clock is fixed at 2024-01-02 12:04:05 local and offset +32400,
`secs-design.json`'s `created_utc` equals the literal `2024-01-02T03:04:05Z`.
If the CLI only exposes the deterministic (UTC) clock, add a test-only
offset to the deterministic mode for this test.

Commit subject: `fix(audit): A-4 CLI가 시계 오프셋도 함께 넣어 UTC 시각이 맞습니다 (#186 TD-152)`.

### A-35 · TD-205 · FFI `RESULTS` unbounded; free latency undocumented

Finding: "`secs_events_next`가 `recv_timeout` 동안 구독 뮤텍스를 쥐고 있어
`secs_events_free`는 바로 돌아오지만 소켓은 그 폴링이 끝나야 닫히므로 호출자의
`timeout_ms`까지 늦어집니다. … 날 C 호출자가 `secs_result_take`/`free`를 부르지
않으면 `RESULTS`가 무한히 자랍니다." Recommendation: write both facts into
`ffi.rs`'s docs and `docs/UNSAFE_BOUNDARIES.md`; cap `RESULTS` (1024; past
it evict the oldest and answer `invalid_handle` for it).

Seam: the C ABI result table. Red test `results_table_is_capped_and_evicts_the_oldest_td205`
in `bindings/dotnet/native/src/core.rs` tests: create 1025 results without
taking them; taking the first handle answers the `invalid_handle` envelope
(literal code), the 1025th is still readable. Documentation lines in
`ffi.rs` and `docs/UNSAFE_BOUNDARIES.md` (the unsafe budget of
`secs-dotnet-sys` is 32; do not add `unsafe`).

Commit subject: `fix(audit): A-35 C ABI 결과 표에 상한을 두고 해제 지연을 문서화합니다 (#186 TD-205)`.

### A-36 · TD-206 · dropping `Engine` leaves jobs running

Finding: "잡 스레드는 분리돼 있고 `Service::shutdown`은 앱만 부릅니다. `Engine`
드롭, `secs_engine_free`, 파이썬 `Engine`은 부르지 않아 임베딩 프로그램이 엔진을
버린 뒤에도 계산 잡이 계속 돌며 캐시를 씁니다." Recommendation:
`Drop for Engine` calls `service.shutdown(Duration::from_secs(3))` and the
docs say termination events after the drop are lost.

Seam: `secs_api::Engine` drop. Red test
`dropping_the_engine_stops_its_jobs_within_three_seconds_td206` in
`crates/secs-api/tests/public_api.rs`: start a long mock calibration, drop
the engine, assert within 3.5 s that no job thread is still running (the
service's `jobs` ledger is private; observe through a shared `AtomicBool`
set by a progress sink, or through the cache folder not changing for 1 s
after the drop; state which in the report).

Commit subject: `fix(audit): A-36 엔진을 버리면 잡이 멈춥니다 (#186 TD-206)`.

### A-37 · TD-208 · inconsistent error constructors; token folder failures look internal

Finding: "이름이 `invalid`인 도우미 셋이 매개변수 순서가 다르고 …,
`host_error`·`local_error`는 대화상자와 토큰 파일 실패를 `ErrorCode::Internal`로
매핑해 거부된 토큰 폴더가 내부 오류로 보입니다. `ServiceError::Project(String)`은
`ProjectError`와 `io::Error`를 글자로 펴서 `project_open`이 없는 파일·권한·상한
초과를 구별하지 못하고 `path` 매개변수도 없습니다." Recommendation:
`IpcError::invalid(field, detail)` and `IpcError::io(code, path, error)` in
`secs-types`; token files answer `ProjectIo` / `PermissionDenied`;
`ServiceError::Project` keeps the cause and maps `PermissionDenied`.

Scope here: the two `secs-types` constructors and the token-folder mapping
in `apps/secs2r/src/local_api.rs` (and `host.rs` if it maps io errors).
`ServiceError::Project` is in `secs-service` (not yours): report it as
left for the service worker.

Seam: `local_api_token_regenerate` (or the local API settings request that
writes the token). Red test `denied_token_folder_is_a_permission_error_td208`
in `apps/secs2r` tests (the crate has unit tests for `local_api.rs`; if not,
test `IpcError::io` in `secs-types`: an `io::Error` of kind
`PermissionDenied` with path `C:\x\token` → code `PermissionDenied`,
`params.path == "C:\\x\\token"`).

Commit subject: `fix(audit): A-37 오류 생성자를 통일하고 토큰 폴더 거부를 권한 오류로 알립니다 (#186 TD-208)`.

### A-40 · TD-214 · stopping the local API blocks on long requests

Finding: "`ServerHandle::stop`이 런타임 스레드를 join하고 `Runtime` 드롭이 진행
중인 `spawn_blocking` 서비스 호출을 기다리므로, 설정 화면에서 API를 끄거나
토큰을 다시 만들면 긴 로컬 API 요청이 끝날 때까지 `Manager` 뮤텍스가 막힙니다."
Recommendation: `runtime.shutdown_timeout(2 s)` instead of the implicit drop,
and document the bound.

Seam: `ServerHandle::stop`. Red test `stop_returns_within_the_shutdown_bound_while_a_request_runs_td214`
in `crates/secs-local-api/tests/local_api.rs`: a request that blocks the
service for 10 s (a test-only slow method or a service double); call
`stop`; assert it returns in under 3 s (literal).

Commit subject: `fix(audit): A-40 로컬 API 정지가 긴 요청을 기다리지 않습니다 (#186 TD-214)`.

## Verification (foreground; do not end the turn before they finish)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p secs-types --locked            # bindings regenerated and committed?
git status --short apps/secs2r/ui/src/ipc/bindings  # only content changes, see below
(cd apps/secs2r/ui && npm run typecheck)
python3 .github/scripts/check_feature_tests.py
python -m pytest bindings/python/tests -q    # if maturin develop worked
dotnet test bindings/dotnet/Secs2R.Tests     # if the SDK is here
git log --oneline fix/audit-186..audit/186-api
```

The bindings folder shows every file as modified after regeneration because
the generator writes LF: commit only the files whose `git diff` has content
changes. `check_feature_tests.py` on Windows reports two known names it
cannot list here (`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`); anything else is yours.

## Report

Per item: the commit hash, the Red test name and the assertion it failed
on, the Green summary, the gate outputs (tail), what could not be run
locally (Python wheel, .NET) and what CI covers. Then: items blocked or
left for another worker (A-37's `ServiceError::Project`), any seam you
changed and why, and the final `git log --oneline` of the branch.
