# 0059. 로컬 API의 수명을 `secs-local-api`가 소유합니다

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A18. 기술 부채 감사의 TD-214(API를 끄거나 토큰을 바꿀 때 긴 요청이 끝날 때까지 막힘)와 TD-208의 일부(토큰 폴더 실패가 `Internal`로 보임)를 닫습니다.

## 배경

ADR 0016은 로컬 API 서버를 새 크레이트 `secs-local-api`에 두고 앱이 서비스를 전달해 띄운다고 정했습니다. 서버 자체(`start(ServerConfig) -> ServerHandle`, 인증, 본문 상한, 이벤트 허브)는 그렇게 되어 있지만, 서버의 수명과 토큰과 상태 조립은 Tauri 앱에 있습니다. 아래 줄 번호는 main `885f0535` 기준입니다.

- `apps/secs2r/src/local_api.rs:17-201`의 `Manager`가 `lifecycle(old, new)`(켜기·끄기·포트 변경 판정), `startup`, `apply_settings`, `apply_revision`, `status`, `regenerate_token`, `ensure_token`, `start_locked`, `stop_locked`, `status_locked`를 가집니다. 150줄 가운데 Tauri 고유 줄은 `:144`의 `app.emit("secs://external-change")`, `:145-155`의 설정 재적용 스레드, `:102`의 `crate::icon::show`뿐입니다.
- `crates/secs-local-api/src/lib.rs:76-89`의 `ServerConfig`는 이미 `on_change` 클로저를 받고, `token.rs:17-41`은 토큰 파일을 만들고 읽습니다. 크레이트는 수명을 모르는 채 재료만 줍니다.
- `secs-service`의 `Host` trait(`lib.rs:159-166`)에서 `local_api_status`와 `local_api_regenerate_token`을 구현한 adapter는 `TauriHost`(`apps/secs2r/src/host.rs:174-182`)뿐입니다. `NullHost`를 쓰는 CLI와 공개 API의 `Engine`은 로컬 API를 띄울 수 없습니다.
- '앱 설정이 바뀌면 판을 적용한다'는 결정을 두 입구가 따로 합니다. 화면 요청은 `apps/secs2r/src/main.rs:74-80`이 `AppSettingsSet` 응답을 보고 `apply_revision`을 부르고, HTTP 요청은 `local_api.rs:143-156`의 `on_change`가 같은 일을 새 스레드에서 합니다.
- `ServerHandle::stop`(`lib.rs:140-150`)은 런타임 스레드를 `join`하고, 스레드 안의 `Runtime`이 드롭될 때 진행 중인 `spawn_blocking` 서비스 호출을 기다립니다. 설정 화면에서 API를 끄거나 토큰을 다시 만들면 긴 요청(`mdat_scan`, `project_open`)이 끝날 때까지 `Manager`의 뮤텍스가 막힙니다(TD-214).
- 토큰 파일을 만들지 못하면 `local_error`(`local_api.rs:203-205`)가 `ErrorCode::Internal`로 전달해, 권한이 없는 설정 폴더가 결함처럼 보입니다(TD-208).

deletion test로 보면 `Manager`를 지웠을 때 두 번째 호스트(`secs2r-cli serve`, Python `Engine.serve()`)마다 같은 150줄이 다시 나타납니다. 깊이가 크레이트가 아니라 앱에 있습니다.

## 결정

### `secs_local_api::Runtime`

새 module `crates/secs-local-api/src/runtime.rs`입니다.

```rust
pub struct Runtime {
    config_dir: PathBuf,
    service: Arc<Service>,
    hub: EventHub,
    on_change: Arc<dyn Fn(&str) + Send + Sync>,     // 바깥 변경을 호스트에 알림
    log: Arc<dyn Fn(LogLevel, &str) + Send + Sync>, // 서버 오류를 호스트 로그에(TD-151의 seam)
    inner: Mutex<Inner>,                            // settings, server, token, error
}

impl Runtime {
    pub fn new(config_dir, service, hub, on_change, log) -> Self;
    pub fn startup(&self, settings: LocalApiSettings);
    pub fn apply_settings(&self, old: LocalApiSettings, new: LocalApiSettings);
    pub fn status(&self) -> Result<LocalApiStatus, LocalApiError>;
    pub fn regenerate_token(&self) -> Result<LocalApiStatus, LocalApiError>;
    pub fn stop(&self);
}
```

`lifecycle`, `startup`, `apply_settings`, `status`, `regenerate_token`, `ensure_token`, `start_locked`, `stop_locked`, `status_locked`가 그대로 옮겨 갑니다. 상태 조립(`LocalApiStatus`의 `url`, `token_path`, `running`, `error`)도 크레이트의 몫입니다.

`LocalApiError`에 `Token { path, source: io::Error }`를 추가하고, 앱은 그것을 `io::ErrorKind::PermissionDenied`면 `ErrorCode::PermissionDenied`, 아니면 `ProjectIo`로 바꿉니다(TD-208). `Internal`은 결함에만 남깁니다.

### 멈춤에 상한을 둡니다

`ServerHandle::stop`은 스레드에서 `Runtime`을 암묵적으로 드롭하는 대신 `tokio::runtime::Runtime::shutdown_timeout(Duration::from_secs(2))`를 부릅니다. 2초 안에 끝나지 않은 서비스 호출은 그 스레드에 버려지고, 서버 소켓은 바로 닫힙니다. `Runtime::stop`과 `apply_settings`의 끄기·재시작도 같은 상한을 씁니다. 상한은 rustdoc과 ADR 0016의 결과 절에 적습니다(TD-214).

### 호스트에 남는 것

Tauri 앱의 `local_api.rs`는 `Runtime`을 만들고 `on_change`에 `app.emit("secs://external-change", method)` 한 줄, `log`에 `AppLog::line`을 전달하는 일만 남습니다. `TauriHost::local_api_status`와 `local_api_regenerate_token`은 `Runtime`에 위임합니다. `Host` trait의 두 메서드는 그대로 둡니다. `secs-service`는 `secs-local-api`에 의존할 수 없으므로(의존 방향이 반대) 서비스가 런타임을 직접 쥘 수는 없고, 호스트가 `Runtime`을 만들어 `Host`로 잇는 모양이 맞습니다. CLI의 `serve` 하위 명령과 공개 API의 `Engine::serve()`는 이 ADR의 범위 밖이지만, 둘 다 `Runtime::new(.., NullHost 같은 on_change, eprintln 로그)` 한 줄로 가능해집니다.

### 앱 설정 변경은 한 곳에서 관찰합니다

`Deps`에 `on_app_settings: Arc<dyn Fn(RevisedAppSettings) + Send + Sync>`(기본은 아무 일도 하지 않음)를 추가하고, `project_edit.rs`의 `app_settings_set`이 저장에 성공한 뒤 `revised_app_settings()`로 그것을 부릅니다. 앱은 이 관찰자 하나에서 `AppEffects::accept`로 판을 걸러 `icon::show`와 `Runtime::apply_settings`를 부릅니다. `main.rs:74-80`의 응답 검사와 `local_api.rs:145-155`의 재적용 스레드는 지웁니다. A10(ADR 0051, 앱 설정 저장소 하나)과 같은 자리이므로 둘 가운데 먼저 들어오는 쪽이 이 관찰자를 만들고 나머지는 그것을 씁니다.

이 ADR이 먼저 들어와 `Deps.on_app_settings`를 만들었고, 그다음 A10의 `AppSettingsStore`가 들어오면서 관찰자는 ADR 0051의 `Service::register_settings_effect`로 합쳐졌습니다. 앱은 `SettingsEffect` 구현 둘(아이콘, 로컬 API)을 등록하고, `Deps.on_app_settings`, `RevisedAppSettings`, `AppEffects`는 없습니다. 아래 `secs-service` 시험은 그 등록 위에서 같은 규칙을 확인합니다.

## 시험

깊어진 interface인 `Runtime`에서 먼저 씁니다. 모두 `crates/secs-local-api/tests/local_api.rs`에 들어가고 실제 루프백 소켓을 씁니다.

- `startup(enabled, port 0)` → `status().running`이 참이고 `url`이 실제 포트를 담습니다. `startup(disabled)` → 서버 없음, `token`은 그래도 채워집니다(화면이 복사할 수 있어야 함).
- `apply_settings`로 포트를 바꾸면 옛 포트가 닫히고 새 포트가 열리며, 끄면 닫히고, 같은 값이면 서버 핸들이 그대로입니다(`lifecycle` 시험은 이 네 경우를 크레이트에서 다시 가집니다).
- `regenerate_token` → 토큰 파일이 새 값이고 옛 토큰의 요청은 401, 새 토큰은 200입니다.
- TD-214: 10초 자는 서비스 호출(`FakeHost`로 막는 요청)이 진행 중일 때 `apply_settings`로 끄면 `stop`이 3초 안에 돌아오고 포트가 닫혀 있습니다.
- 토큰 폴더를 읽기 전용으로 만들면 `status()`가 `LocalApiError::Token`이고, 앱의 변환이 `PermissionDenied`입니다(`secs-service`의 `Host` 경로 시험).
- `secs-service`: `app_settings_set`이 성공한 뒤에만 등록된 `SettingsEffect`가 한 번 불리고, 거절된 설정에서는 불리지 않습니다(`settings_effect_runs_once_after_a_successful_set_and_never_after_a_refusal`).

## 이행

1. `runtime.rs`와 시험(앱은 아직 `Manager`를 씀).
2. `ServerHandle::stop`의 `shutdown_timeout`과 TD-214 시험(A 항목).
3. 앱의 `Manager`를 `Runtime` 호출로 바꾸고 `local_api.rs`를 30줄 안팎으로 줄입니다. `LocalApiError::Token`과 오류 코드 변환(TD-208 부분).
4. `Deps.on_app_settings`와 두 입구의 특례 삭제.
5. ADR 0016의 결과 절에 멈춤 상한과 `Runtime`을 적습니다.

## 결과

- 토큰, 포트, 상태, 멈춤 규칙이 크레이트 한 곳에 있고 실제 소켓으로 시험됩니다. 두 번째 호스트는 `Runtime` 하나를 만들면 됩니다.
- API를 끄거나 토큰을 바꾸는 일이 긴 요청에 묶이지 않습니다. 그 요청은 버려지므로, 2초를 넘기는 요청은 클라이언트에게 연결 끊김으로 보입니다. ADR 0016에 적습니다.
- 바뀌지 않는 것: HTTP 입구 셋, 인증 규칙, 토큰 파일 이름과 위치, `LocalApiStatus`의 wire 모양.
- 위험: `shutdown_timeout`이 버린 `spawn_blocking` 호출은 서비스 안에서 계속 돌다가 끝납니다. 서비스 잠금을 쥔 채 버려지는 것이 아니라 호출이 끝날 때 정상으로 풀리므로 상태가 깨지지는 않습니다.
- 편차 항목은 없습니다. 업스트림에는 로컬 API가 없습니다.
- 다음 감사는 `Host` trait에 로컬 API 메서드 둘이 남아 있는 것을 다시 지적하지 않습니다. 의존 방향 때문에 호스트가 잇는 것이 맞습니다.
