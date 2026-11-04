# 0040. npm 패키지 secs2r와 Node 바인딩 격리 크레이트

상태: 제안, 2026-11-04

## 배경

유지보수자는 2026-11-04에 conda-forge(ADR 0039) 다음으로 npm에도 엔진과 로컬 API 클라이언트를 올리기로 했습니다. Node.js용 바인딩은 아직 없습니다. Python(ADR 0019)은 PyO3, .NET(ADR 0032)은 C ABI 격리 크레이트로 같은 JSON 봉투를 주고받습니다.

바인딩 방식은 napi-rs(Node-API)로 정했습니다. 2026-11-04에 확인한 안정판은 `napi` 3.16.1, `napi-derive` 3.7.0, `napi-build` 2.6.0, `@napi-rs/cli` 3.10.12이고 Rust 1.88 이상을 요구합니다. wasm은 엔진이 순수 Rust라 가능하지만 네이티브보다 느리고 로컬 API 클라이언트(HTTP)를 따로 묶어야 하므로 뒤로 미룹니다.

`#[napi]` 매크로가 만드는 코드는 `#[allow(unsafe_code)]`를 품고 있어, 크레이트 루트의 `#![forbid(unsafe_code)]`와 양립하지 않습니다. 2026-11-04에 scratchpad의 시제품 크레이트(함수 하나, 구조체 하나, 생성자와 동기·비동기 메서드)를 `cargo check --target x86_64-pc-windows-msvc`로 돌려 E0453(`allow(unsafe_code) incompatible with previous forbid`) 아홉 건을 확인했습니다. PyO3와 달리 napi-rs는 비격리 크레이트가 될 수 없습니다.

## 결정

### 격리 크레이트

- `bindings/node`에 `secs-node-sys`를 둡니다. `cdylib`이고 crates.io에는 올리지 않습니다(`publish = false`). 공개 API(`sound-equalization-and-correction-system-rust-api`)의 `engine`과 `client` 기능에만 의존하고, 운영체제·GPU 의존은 직접 갖지 않습니다.
- ADR 0002의 격리 크레이트 규칙을 따릅니다. 루트에 `#![deny(unsafe_code)]`를 걸고 자기 lint 표를 가집니다. 손으로 쓰는 unsafe의 예산은 0이며 `unsafe-budget.toml`에 `secs-node-sys = 0`으로 적습니다. 매크로가 만드는 unsafe만 허용되고, `Send`·`Sync`를 손으로 구현하지 않습니다. 바인딩이 `&mut self` 비동기 메서드를 쓰면 napi-rs가 unsafe를 요구하므로, 상태는 `Arc`에 두고 `&self` 메서드와 내부 동기화로 씁니다.
- `build.rs`는 `napi_build::setup()`만 부릅니다.

### JS API

Python 패키지와 같은 JSON 중심 API이고 호출은 모두 비동기입니다. 엔진의 동기 호출은 `tokio::task::spawn_blocking`으로 이벤트 루프와 Tokio 실행 스레드 밖에서 돌립니다.

- `Engine.create({ appSettingsPath? })`, `engine.call(request)`, `engine.waitJob(jobId, timeoutMs)`, `engine.events({ signal? })`.
- `Client.create({ port = 4712, token?, tokenPath? })`(둘 다 주면 거부), `client.call(request)`, `client.status()`, `client.events({ signal? })`.
- 이벤트는 `AsyncIterableIterator`이고 `close()`가 있습니다. 구독마다 전용 읽기 스레드와 유한한 큐를 두고, `for await`의 `break`(iterator의 `return()`)도 구독을 끝냅니다. 종료한 구독이 Node 프로세스 종료를 막지 않아야 합니다. 엔진 구독의 `droppedEvents()`는 `bigint`입니다.
- 오류는 JS `Error`를 상속한 `SecsError { code, detail, params?, retryable? }`입니다. 서비스 봉투의 오류 필드는 `code`, `params`, `retryable`, `detail`이고 `message`는 없으므로, 바인딩이 메시지를 만듭니다.
- 성공 봉투는 Python처럼 풀어 `Response { method, data }`를 돌려줍니다. 엔진 구독은 `JobEvent`, 클라이언트 구독은 `ServerEvent`(`{ kind: "job", event }` 또는 `{ kind: "changed", method }`)입니다. 둘은 다른 타입입니다.
- 작업 번호는 JSON과 같이 `number`이고, 입력은 안전한 음이 아닌 정수로 제한합니다. `timeoutMs`는 유한한 0 이상 값으로 제한합니다.
- 타입 선언은 손으로 쓴 느슨한 JSON 타입입니다. `secs-types`에서 만든 전체 타입은 뒤의 ADR로 미룹니다. 앱의 UI 디렉터리는 패키지 빌드의 입력이 아닙니다.

### 패키지 배치와 판

- 메인 패키지 `secs2r`와 플랫폼 패키지 넷(`@secs2r/secs2r-win32-x64-msvc`, `-darwin-x64`, `-darwin-arm64`, `-linux-x64-gnu`)입니다. 메인의 `optionalDependencies`가 플랫폼 패키지의 정확한 판을 가리킵니다. 스코프 `@secs2r`는 npm 조직으로 만들어야 하고, 2026-11-04에는 가용성을 확인하지 못했습니다(조직 페이지가 403). 스코프를 못 쓰면 `secs2r-win32-x64-msvc`처럼 스코프 없는 이름으로 바꿉니다.
- 판은 공개 크레이트와 PyPI, NuGet과 같습니다(`api-v<판>` 태그). 릴리스 게이트가 `package.json`의 판도 함께 올립니다.
- 라이선스는 AGPL-3.0-only(ADR 0023)이고 메인과 플랫폼 패키지 모두에 `LICENSE` 사본을 넣습니다. `@napi-rs/cli`는 라이선스 문자열만 복사하므로 사본은 직접 넣고 `npm pack --dry-run --ignore-scripts`로 확인합니다.

### 빌드와 게시

- `release.yml`에 PyPI와 같은 모양의 잡을 둡니다. 빌드 잡은 네 대상을 각 러너에서 `napi build --platform --release --target <삼중자> -- --locked`로 만들고, 실제 `.node`를 불러 엔진으로 프로젝트를 만들고 보정을 계산하는 시험을 돌린 뒤 아티팩트를 올립니다. 게시 잡은 네 아티팩트를 받아 `napi create-npm-dirs`, `napi artifacts`, `napi prepublish --no-gh-release`로 플랫폼 패키지를 올리고 마지막에 메인을 `npm publish`합니다. GitHub 릴리스는 지금 절차가 만들므로 napi의 릴리스 생성은 끕니다.
- 게시는 npm trusted publishing(GitHub OIDC)입니다. npm CLI 11.5.1 이상, Node 22.14 이상, GitHub-hosted 러너, `id-token: write`가 필요하고, npmjs.com에서 패키지마다(메인과 플랫폼 패키지 다섯) trusted publisher(`115dkk`, `Sound-Equalization-and-Correction-System-2R`, `release.yml`, 환경 `npm`)를 등록해야 합니다. 새 등록은 2일 안에 게시에 성공해야 유지되고, 처음에는 stage 권한만 받으므로 직접 게시를 허용해야 합니다. 공개 저장소의 공개 패키지라 provenance는 자동으로 붙습니다. 장기 토큰은 두지 않습니다.
- 첫 게시는 패키지가 아직 없어 trusted publisher를 등록할 자리가 없습니다. 유지보수자가 npm 계정으로 조직 `@secs2r`를 만들고 다섯 패키지를 `npm stage` 또는 첫 수동 게시로 만든 뒤 publisher를 등록하는 순서를 구현 작업서에서 확정합니다.

### 확인

- 격리 크레이트의 unsafe 예산 0과 `secs-policy` 게이트 통과.
- 네 대상에서 실제 `.node`를 불러 프로젝트 생성, 측정 가져오기, 보정 계산, 결과 조회가 되는 CI 시험. 이벤트 구독 여럿, `break`와 `close()`, 구독 종료 뒤 프로세스 종료.
- `npm pack --dry-run --ignore-scripts`로 다섯 패키지의 내용(LICENSE, 타입 선언, `.node`)을 확인.

## 결과

- Node.js(Electron 포함)에서 엔진과 로컬 API를 쓸 수 있습니다. 장치 측정과 GPU는 Rust API처럼 빠집니다.
- 격리 크레이트가 하나 늘어납니다(`secs-node-sys`, 예산 0). 손으로 쓴 unsafe는 없고 매크로가 만든 것만 있으므로, 경계 문서는 `docs/UNSAFE_BOUNDARIES.md`에 그렇게 적습니다.
- `release.yml`이 길어지고 npm 쪽 설정(조직, 다섯 publisher)이 유지보수자의 손을 거칩니다.
