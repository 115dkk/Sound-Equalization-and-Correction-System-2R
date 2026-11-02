# 0032. NuGet 패키지와 C ABI 격리 크레이트

상태: 제안, 2026-11-02

## 배경

유지보수자는 2026-10-31에 C#(.NET, Unity)에서도 엔진을 쓸 수 있도록 nuget.org에 패키지를 올리자고 했습니다. 내용은 PyPI 패키지(ADR 0019)와 같은 엔진과 로컬 API 클라이언트입니다. 서두르지 말고 진행 중인 일을 끝낸 뒤에 하라는 지시였습니다.

Python 바인딩은 PyO3가 unsafe를 매크로 안에 감추므로 `secs-py`가 `#![forbid(unsafe_code)]`를 지킵니다. C#에는 그런 도구가 없습니다. .NET은 C ABI 함수만 부를 수 있는데, Rust 2024 판에서 C ABI 함수를 내보내려면 `#[unsafe(no_mangle)]`을 붙여야 하고, C#이 넘긴 포인터에서 바이트를 읽으려면 `std::slice::from_raw_parts`를 불러야 합니다. 둘 다 unsafe입니다. 그래서 ADR 0002에 따라 격리 크레이트와 그 예산을 이 ADR에서 먼저 정합니다.

바인딩 생성 도구는 유지보수자가 2026-10-31에 코드를 읽어 비교했습니다(시제품은 만들지 않았습니다). csbindgen은 `build.rs`에서만 돌아 내보낸 `extern "C"` 함수의 C# 선언(`NativeMethods.g.cs`)을 만듭니다. Interoptopus는 `secs-types`의 요청 열거형 변형마다 FFI 표시를 달아야 하고, uniffi-bindgen-cs는 IDL 계층이 하나 더 생기며 UniFFI를 늦게 따라갑니다(지원하는 최신판이 0.29.4이고, 0.32 지원은 아직 열린 이슈입니다).

## 결정

### 격리 크레이트

- `bindings/dotnet/native`에 `secs-dotnet-sys`를 둡니다. `cdylib`이고 crates.io에는 올리지 않습니다. 공개 API(`sound-equalization-and-correction-system-rust-api`)의 `engine`과 `client` 기능만 쓰며, 운영체제 의존성과 GPU는 넣지 않습니다.
- ADR 0002의 격리 크레이트 규칙을 따릅니다. 루트에 `#![deny(unsafe_code)]`를 걸고 자기 lint 표를 가지며, unsafe는 C ABI 함수를 모은 모듈 파일 하나에서만 허용합니다. 시험은 `forbid`입니다.
- `unsafe-budget.toml`에 `secs-dotnet-sys = 32`를 적습니다. 내보내는 함수마다 `#[unsafe(no_mangle)]`이 하나씩 들고, 포인터를 받는 함수는 `unsafe extern "C" fn`으로 선언하며 그 안에 바이트를 빌리는 블록을 하나씩 둡니다. 아래 함수 목록으로 세면 25개이고, 나머지는 함수를 하나둘 추가할 여유입니다. 넘기려면 이 ADR을 고칩니다.

### 경계 모양

unsafe가 드는 자리를 포인터에서 바이트를 빌리는 일 하나로 줄이는 모양입니다.

- **핸들은 번호입니다.** 엔진, 클라이언트, 이벤트 구독, 호출 결과를 포인터가 아닌 `u64` 번호로 주고받고, 실제 값은 크레이트 안의 표가 가집니다. `Box::from_raw` 같은 소유권 이동이 없으므로, 모르는 번호나 이미 해제한 번호를 넘기면 오류 봉투가 돌아올 뿐 정의되지 않은 동작(UB)이 생기지 않습니다. 0은 '없음'입니다.
- **문자열은 C#이 가진 메모리만 씁니다.** 요청은 C#이 UTF-8 바이트의 포인터와 길이로 넘깁니다. 응답은 결과 번호로 돌려주고, C#은 길이를 물어 자기 버퍼를 만든 뒤 그 버퍼로 복사해 받습니다. 복사가 끝나면 크레이트가 결과를 지웁니다. Rust가 할당한 메모리를 C#이 해제하는 함수는 없습니다.
- **응답은 언제나 JSON 봉투입니다.** 성공이면 `{"ok":true,"data":...}`, 실패면 `{"ok":false,"error":{"code","message","detail"}}`이고, 로컬 API 및 Python 패키지와 같은 모양입니다. 함수 안의 패닉은 `catch_unwind`로 잡아 `internal` 오류 봉투로 바꿉니다.
- **빌리기 전에 확인하는 것.** 널 포인터는 길이가 0일 때만 받고, 길이가 `isize::MAX`를 넘으면 `from_raw_parts`의 전제를 어기므로 읽기 전에 거부합니다. 요청 크기의 상한은 따로 두지 않습니다. Python 패키지처럼 같은 프로세스 안의 호출이고, 로컬 API의 본문 상한(ADR 0016)은 네트워크로 들어오는 요청을 위한 것이기 때문입니다.
- **함수 목록.** `secs_engine_new`(앱 설정 경로), `secs_engine_free`, `secs_engine_call`, `secs_engine_wait_job`(작업 번호, 밀리초), `secs_engine_subscribe`, `secs_client_new`(포트, 토큰, 토큰 파일 경로), `secs_client_free`, `secs_client_call`, `secs_client_status`, `secs_client_events`, `secs_events_next`(구독, 밀리초), `secs_events_free`, `secs_result_len`, `secs_result_take`(결과, 버퍼, 길이), `secs_result_free`.
- **이벤트는 당겨 옵니다.** 콜백을 등록하지 않고 C#이 `secs_events_next`를 시한과 함께 부릅니다. Unity IL2CPP와 .NET NativeAOT는 네이티브에서 부르는 콜백에 각자 제약(`MonoPInvokeCallback`, `UnmanagedCallersOnly`)이 있어, 콜백을 쓰지 않으면 두 환경에서 같은 코드가 돕니다.
- **C# 선언은 생성해서 커밋합니다.** csbindgen을 `build.rs`에서 돌려 `NativeMethods.g.cs`를 만들고 저장소에 둡니다. IPC 바인딩처럼 CI가 다시 생성해 바뀐 곳이 없는지 확인합니다.
- 구현 PR에서 `docs/UNSAFE_BOUNDARIES.md`에 이 크레이트의 경계 지도(숨긴 의무, unsafe 전에 흡수하는 입력, 남는 신뢰)를 씁니다. 남는 신뢰는 C#이 넘긴 포인터와 길이가 실제로 그만큼의 읽을 수 있는(출력은 쓸 수 있는) 메모리를 가리킨다는 것 하나입니다. 손으로 쓴 C# 래퍼가 이 약속을 지키고, 사용자 코드는 포인터를 다루지 않습니다.

### C# API

- 패키지 ID와 네임스페이스는 `Secs2R`입니다. PyPI 이름 `secs2r`에 맞췄습니다.
- 첫 판은 Python처럼 JSON을 그대로 주고받습니다. `Engine`과 `Client`는 `IDisposable`이고, `Call(JsonElement) -> JsonElement`와 문자열판 `Call(string) -> string`, `WaitJob(ulong, TimeSpan)`, `Events(CancellationToken) -> IAsyncEnumerable<JsonElement>`를 가집니다. 오류는 `SecsException`(`Code`, `Message`, `Detail`)으로 던집니다.
- 타입이 있는 C# API는 다음 일입니다. 요청의 `method`와 `params` 모양이 System.Text.Json의 다형성 직렬화에 맞는지 확인하지 않았고, `secs-types`는 TypeScript 바인딩(ts-rs)만 만들고 JSON Schema는 만들지 않습니다.
- 대상 프레임워크는 `netstandard2.1`(Unity 2021 이상)과 `net8.0`입니다. `netstandard2.1` 쪽에는 System.Text.Json과 Microsoft.Bcl.AsyncInterfaces 패키지 의존이 붙습니다.

### 빌드, 시험, 게시

- 네이티브 라이브러리는 win-x64, osx-x64, osx-arm64, linux-x64로 빌드해 패키지의 `runtimes/{rid}/native/`에 넣습니다.
- PR에서 바인딩이나 그 의존이 바뀌면 세 운영체제에서 네이티브 라이브러리와 패키지를 빌드하고, 그 패키지를 설치한 시험 프로젝트가 `dotnet test`로 실제 엔진을 불러 프로젝트를 만들고 보정을 계산합니다. Unity 편집기는 라이선스가 필요해 CI에서 돌리지 않습니다.
- 판은 crates.io 공개 크레이트의 판(`api-v<판>` 태그)과 같습니다. 릴리스 게이트가 PyPI와 함께 올리고, `secs-policy`가 같은지 확인합니다.
- 게시는 `release.yml`의 `nuget` 잡이 crates 잡 뒤에 합니다. 환경 `nuget`에서 nuget.org의 신뢰할 수 있는 게시(GitHub OIDC, `NuGet/login@v1`이 한 시간짜리 키를 받음)를 쓰고, 이미 있는 판은 `--skip-duplicate`로 건너뜁니다. 저장소에도, 사람 손에도 nuget.org API 키를 두지 않습니다. 첫 게시 전에 유지보수자가 nuget.org의 Trusted Publishing 화면에서 정책(소유자 `115dkk`, 저장소 `Sound-Equalization-and-Correction-System-2R`, 워크플로 `release.yml`, 환경 `nuget`)을 등록합니다.

### 라이선스

- 패키지와 `secs-dotnet-sys`는 AGPL-3.0-only입니다. 패키지가 담는 엔진은 crates.io에 AGPL-3.0-only로 올린 크레이트들이고(ADR 0023), 그 크레이트들은 AGPL 조건으로 다시 배포할 수 있습니다.
- 패키지 설명의 첫 문장에 이 패키지를 쓰는 프로그램도 AGPL과 호환되는 조건으로 배포해야 한다고 적습니다. Unity로 게임이나 앱을 만들어 파는 사용자가 설치 전에 알아야 하기 때문입니다.
- 루트 `LICENSE-AGPL`의 사본을 패키지에 싣고, `secs-policy`가 사본이 원문과 같은지 확인합니다.

## 고르지 않은 것

- **엔진을 별도 프로세스로 띄우고 표준 입출력으로 JSON을 주고받는 방식.** unsafe가 전혀 필요 없고 격리 크레이트도 늘지 않습니다. 하지만 운영체제마다 실행 파일을 하나 더 싣고, 엔진을 만들 때마다 프로세스가 생기며, 측정 데이터 같은 큰 요청이 매번 프로세스 사이로 복사됩니다. Unity는 플레이어 빌드에서 외부 프로세스 실행을 막는 플랫폼이 있습니다.
- **포인터 핸들과 Rust가 할당한 문자열.** csbindgen 예제의 흔한 모양이지만, 해제 함수마다 `Box::from_raw`와 `CString::from_raw`가 들어 unsafe가 늘고, 두 번 해제하거나 해제한 뒤 쓰면 UB가 됩니다. 번호 핸들에서는 같은 실수가 오류 봉투로 끝납니다.
- **Interoptopus와 uniffi-bindgen-cs.** 배경에 적은 이유로 고르지 않았습니다.

## 결과

- 격리 크레이트가 하나 늘고, 손으로 쓴 unsafe가 예산만큼 저장소에 들어옵니다. 안전한 C# API로 닿을 수 있는 UB는 다른 격리 크레이트처럼 보안 취약점으로 신고받습니다(`SECURITY.md`).
- `release.yml`에 `nuget` 잡을, `ci.yml`에 .NET 패키지 잡을 추가합니다.
- C#, .NET, Unity에서 프로젝트를 열고, 측정을 가져오고, 보정과 MIMO를 계산하고, 내보낼 수 있습니다. 장치 측정과 GPU는 Rust API와 Python 패키지처럼 빠집니다.
