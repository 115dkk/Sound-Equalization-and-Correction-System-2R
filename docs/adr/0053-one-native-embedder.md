# 0053. 운영체제 adapter와 `Deps` 조립은 `secs-sys` 한 곳에 둔다

상태: 제안, 2026-11-06

근거: 감사 이슈 #186, 아키텍처 리뷰 A12. 해결하는 기술 부채 발견은 TD-152(CLI의 `Deps`에 `clock_offset_seconds`가 빠져 내보낸 시각이 한국에서 9시간 어긋남)입니다.

## 배경

서비스는 운영체제 경계를 `Deps`(`crates/secs-service/src/lib.rs:377-427`, 21 필드)와 두 trait `ApplyAdapter`·`ManagedHostAdapter`(`:184-236`)로 받습니다. 그 adapter와 조립이 앱마다 한 벌씩 있습니다. 2026-11-06의 코드에서 다시 센 결과입니다.

- `apps/secs2r/src/apply.rs:13-49`와 `apps/secs2r-cli/src/apply.rs:13-49`의 `NativeApply`는 `secs_sys::*`로 그대로 넘기는 여섯 메서드가 글자까지 같습니다. `NativeManagedHost`(`54-61`)도 같고, `install`만 Tauri 앱이 `set_shell_autostart`와 `spawn_detached`를 추가로 부릅니다(`apps/secs2r/src/apply.rs:63-73` 대 CLI `63-69`). `host_executable`도 두 벌입니다.
- `apps/secs2r-host-shell/src/main.rs:42-50`의 `HostPlatform` → `NativeHost`는 같은 모양의 세 번째 변형으로, `service_state`·`pipe_call`·`install_service`를 같은 `secs_sys::host` 함수로 보냅니다.
- `Deps`를 손으로 조립하는 곳이 셋입니다. `apps/secs2r/src/main.rs:180-204`(14 필드), `apps/secs2r-cli/src/main.rs:587-627`(11 필드), `crates/secs-api/src/lib.rs:335-356`(5 필드). CLI는 `clock`에 운영체제의 UTC 오프셋을 넣으면서 `clock_offset_seconds`를 빠뜨려 기본값 0이 됩니다. `crates/secs-service/src/export.rs:277, 731`이 이 둘로 완료 시각을 UTC로 되돌리므로 CLI가 `secs-design.json`과 `last_export`에 남기는 시각이 한국에서는 9시간 어긋납니다(TD-152).

차이는 의도가 아니라 누락에서 생깁니다. deletion test로 보면 CLI의 `deps()`와 `apply.rs`를 지웠을 때 Tauri 앱의 사본에서 그대로 다시 나타나므로, 하나로 두어도 잃는 것이 없습니다.

## 결정

### `secs-sys`가 adapter와 기본 조립을 가진다

- `crates/secs-sys/src/embed.rs`에 `OsApply`와 `OsManagedHost`를 두고 `secs_service::ApplyAdapter`·`ManagedHostAdapter`를 구현합니다. 두 앱의 `apply.rs`는 지웁니다.
- `OsManagedHost::new(host_executable: PathBuf, shell: Option<ShellAutostart>)`. `shell`이 있으면 `install`이 서비스 등록 뒤 셸의 자동 시작을 켜고 셸을 띄우며, `uninstall`이 자동 시작을 끕니다. Tauri 앱은 `Some`, CLI는 `None`을 줍니다. 두 앱의 차이가 사본 둘이 아니라 생성자 인자 하나가 됩니다. `host_executable`·`shell_executable`·`application_executable`을 찾는 함수도 이 module로 옮깁니다.
- `secs_sys::embed::native_deps(NativeEmbedding { app_version, app_data_path, shell }) -> secs_service::Deps`가 운영체제가 정하는 필드에 값을 모두 넣어 돌려줍니다. `clock`과 `clock_offset_seconds`는 같은 `secs_sys::clock::utc_offset_seconds`에서, `unix_ms`는 시스템 시계에서, `audio_backend`는 `secs_sys::audio::default_backend`, `rew_transport`는 `secs_rew::HttpTransport`, `process`·`memory`는 `OsProcessProbe`·`OsMemoryProbe`, `eqapo_install`은 `secs_sys::eqapo_install`, `apply`·`managed_host`는 위의 두 adapter입니다. 앱이 정하는 필드(`acceleration`, `compute_override`, `host`, `observer`, `updates`, `app_settings_path`, `log_dir`, `uuid`)는 `..Deps::default()`로 두고 호출자가 얹습니다.
- 그래서 `secs-sys`가 `secs-service`를 의존하게 됩니다. ADR 0002와 `CONTEXT.md`의 불변식은 누가 `secs-sys`를 의존하는가(앱만)를 제한하고, `secs-sys`가 다른 seam을 채우는 것(`AudioBackend`, `ProcessProbe`, `MemoryProbe`)은 이미 허용합니다. `secs-service`는 `secs-sys`를 의존하지 않으므로 순환은 없고, `secs-engine` 이하가 오디오 장치 없이 빌드된다는 불변식도 그대로입니다. 리뷰가 생각한 다른 길(두 trait를 `secs-types`로 옮기고 `secs-sys`는 `secs-service`를 모르게 두는 것)은 `Deps` 조립이 `secs-service`의 타입을 돌려주어야 하므로 성립하지 않습니다. 조립을 앱에 남기면 누락이 다시 생기므로, 의존 간선 하나를 받아들입니다.

### 세 소비자

- `apps/secs2r`: `native_deps` 위에 `acceleration: secs_gpu::select`, `host: TauriHost`, `observer`, `updates`, `app_settings_path`, `log_dir`를 얹습니다.
- `apps/secs2r-cli`: `native_deps` 위에 `acceleration`과 `compute_override`를 얹고, `--deterministic-uuids`일 때 지금처럼 `uuid`·`clock`·`unix_ms`를 고정값으로 바꿉니다. 고정 시계에서는 `clock_offset_seconds`도 0으로 함께 바꿉니다. 둘은 같은 순간을 뜻해야 하므로 한 함수(`deterministic_clock(deps)`)가 셋을 함께 바꿉니다.
- `apps/secs2r-host-shell`: `HostPlatform`은 `OsManagedHost`가 하는 네 메서드(`service_state`, `pipe_call`, `install_service`, `uninstall_service`)를 지우고 `set_shell_autostart`, `spawn_detached`, `main_app_running` 셋만 남깁니다. 서비스 쪽 일은 `OsManagedHost`를 통해 합니다.
- `crates/secs-api`의 `EngineBuilder`는 게시 크레이트라 `secs-sys`를 쓰지 못합니다. 지금처럼 자기 조립을 가지되, ADR 0054가 추가하는 setter로 CLI가 그것을 쓰게 되면 CLI의 조립은 `native_deps`의 결과를 builder에 넣는 한 줄이 됩니다.

## 시험

- `secs-sys`: `native_deps_keeps_the_clock_and_its_offset_consistent`. `clock()`이 돌려준 지역 시각에서 `clock_offset_seconds()`를 빼면 `unix_ms()`의 UTC 시각과 1초 안에서 같습니다. 운영체제 오프셋이 0인 러너에서도 뜻이 있도록 UTC 시각과의 차이를 단언합니다.
- `secs-sys`: `managed_host_install_touches_the_shell_only_when_asked`. `OsManagedHost`의 상승 설치 호출을 시험용 seam(`HostCommands` trait, 기본 adapter는 `secs_sys::host`)으로 두고, `shell: None`이면 자동 시작과 셸 실행이 불리지 않는지, `Some`이면 서비스 등록 뒤에 불리는지 기록 adapter로 확인합니다.
- `apps/secs2r-cli`: 고정 시계로 내보낸 `secs-design.json`의 `created_utc`가 UTC인지 확인하는 시험(TD-152의 seam)은 그대로 두고, 운영체제 시계로 돌릴 때 `clock_offset_seconds`가 `clock`과 같은 출처인지는 위의 `secs-sys` 시험이 보증합니다.
- 두 앱의 `apply.rs`가 사라졌으므로 `manage-e2e`(Windows GUI 종단 검사)가 설치와 해제 과정을 그대로 검사합니다.

## 이행

1. `secs-sys`에 `embed` module과 `secs-service` 의존을 추가하고 시험을 둡니다. `secs-policy`의 의존 게이트가 `secs-sys`의 새 의존을 허용 목록에 두고, `secs-service → secs-sys`가 생기지 않는지는 지금처럼 검사합니다.
2. CLI를 `native_deps`로 옮깁니다. 이 커밋으로 TD-152가 해결됩니다.
3. Tauri 앱과 호스트 셸을 옮기고 두 `apply.rs`를 지웁니다.

## 결과

- 운영체제 adapter 한 벌, `Deps`의 기본 조립 한 곳입니다. 시계 오프셋이나 자동 시작처럼 누락으로 생기던 차이가 한 자리의 결정이 됩니다.
- `secs-sys`의 빌드가 `secs-service`를 끌어옵니다. 앱은 어차피 둘을 모두 빌드하므로 앱 빌드 시간은 늘지 않습니다.
- `docs/ARCHITECTURE.md`의 의존 그림에 `secs-sys → secs-service` 화살표를 추가하고, ADR 0002의 '앱은 `secs-sys`의 어댑터를 seam에 주입합니다'를 '앱은 `secs-sys`가 조립한 `Deps`에 자기 몫만 얹습니다'로 고칩니다.
- 계산과 내보낸 파일은 바뀌지 않습니다. CLI가 기록하는 `created_utc`만 바르게 UTC가 됩니다. 편차 항목은 없습니다.
