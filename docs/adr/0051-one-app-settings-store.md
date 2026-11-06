# 0051. 앱 설정 저장소 하나

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A10. 함께 해결하는 기술 부채: TD-169(손상되거나 더 새로운 설정 파일을 조용히 기본값으로 바꾸는 것)의 근본 수정. TD-115(#155)의 `AppEffects`가 이 ADR의 저장소 안으로 들어갑니다.

## 배경

앱 설정(`AppSettings`, `crates/secs-types/src/ipc.rs:2602-2633`)은 두 파일에서 쓰고 네 파일에서 읽으며 효과 타입은 다섯 번째 파일에 있습니다.

- 읽기와 쓰기: `project_edit.rs:769-879`(`app_settings_get`, `revised_app_settings`, `app_settings_set`, `load_app_settings`). `load_app_settings`는 파일이 없거나, 1 MiB를 넘거나, JSON이 깨졌거나, 판이 다르면 모두 `AppSettings::default()`를 돌려주고 아무 신호도 남기지 않습니다. 다음 `app_settings_set`이 그 파일을 덮어쓰므로 최근 프로젝트와 정책이 사라집니다(TD-169).
- 날것 읽기: `apply.rs:19, 72, 240`, `compute.rs:64`, `project_edit.rs:852`가 `lock(&self.app_settings)`로 필드를 꺼냅니다. `apply_settings_set`(`apply.rs:355-364`)은 전체를 복사해 한 필드를 바꾸고 다시 저장합니다.
- 효과: 서비스의 `AppEffects`(`lib.rs:577-610`)는 판 번호로 낡은 효과를 거르고, `app_settings_set`이 업데이트 정책 효과(`deps.updates.set_policy`)만 직접 보냅니다. Tauri 앱은 두 번째 `Mutex<AppEffects>`를 들고(`apps/secs2r/src/main.rs:236-246`) 아이콘과 로컬 API 효과를 `local_api.rs:96-106`의 `apply_revision`에서 보냅니다. 그 호출자는 `main.rs:75-79`(앱의 IPC 명령 뒤)와 `local_api.rs:145-154`(로컬 API 서버의 `on_change` 스레드)입니다. 같은 판 번호를 두 인스턴스가 따로 셉니다.

`tests/config.rs:62`의 `ReorderedUpdates`는 서비스 쪽 순서만 확인합니다. 앱 쪽 인스턴스는 시험이 없습니다. 2026-11-06 현재 코드도 리뷰가 적은 그대로입니다.

## 결정

### `app_settings.rs` module 하나가 설정을 소유합니다

`crates/secs-service/src/app_settings.rs`에 저장소를 둡니다.

```rust
pub struct AppSettingsStore { /* path, Mutex<(revision, AppSettings)>, Mutex<AppEffects>, Vec<Arc<dyn SettingsEffect>> */ }

pub struct Opened {
    pub store: AppSettingsStore,
    /// 파일을 읽지 못해 기본값으로 시작했을 때의 알림(코드 `app_settings_unreadable`, `app_settings_newer`).
    pub notice: Option<Notice>,
}

pub trait SettingsEffect: Send + Sync + Debug {
    /// 저장이 끝난 뒤 판 순서대로 한 번씩 불립니다.
    fn changed(&self, previous: &AppSettings, next: &AppSettings);
}

impl AppSettingsStore {
    pub fn open(path: Option<&Path>) -> Opened;
    pub fn register(&mut self, effect: Arc<dyn SettingsEffect>);
    pub fn get(&self) -> AppSettings;
    pub fn revised(&self) -> RevisedAppSettings;
    pub fn set(&self, settings: AppSettings) -> Result<RevisedAppSettings, IpcError>;
    pub fn apply(&self) -> ApplySettings;
    pub fn compute(&self) -> ComputePreference;
    pub fn updates(&self) -> UpdatePolicy;
    pub fn local_api(&self) -> LocalApiSettings;
    pub fn icon(&self) -> AppIcon;
}
```

- `set`은 지금 `app_settings_set`의 검증(최근 프로젝트 10개, 창 크기 양수, 1 MiB 상한), 원자 쓰기, 판 증가를 한 임계 구역에서 하고(TD-39), 잠금을 놓은 뒤 `AppEffects::accept`로 낡은 판을 거른 다음 등록된 효과를 판 순서대로 부릅니다. 효과 전달이 저장소 안에 있으므로 두 번째 `AppEffects`는 필요 없습니다.
- 날것 읽기 다섯 자리는 typed reader(`apply()`, `compute()` 등)로 바뀝니다. `apply_settings_set`은 `set`을 부르는 한 줄입니다.
- 파일 형식 `AppSettingsFile { version, settings }`와 `APP_SETTINGS_VERSION = 1`은 그대로입니다.

### 읽지 못한 파일은 알리고, 더 새로운 파일은 덮어쓰지 않습니다

- `open`은 파일이 깨졌거나 상한을 넘으면 기본값으로 시작하되 `Opened.notice`에 `app_settings_unreadable`(매개변수 `path`, `error`)을 돌려줍니다. 판이 더 새로우면 `app_settings_newer`(매개변수 `version`)를 돌려주고 저장소를 읽기 전용으로 둡니다. 그 상태의 `set`은 `ErrorCode::Unsupported`, `reason = app_settings_newer`로 거절합니다. 옛 판으로 내려간 사용자가 새 판의 설정을 잃지 않게 하기 위해서입니다. 판이 더 오래됐을 때는 지금처럼 기본값입니다(아직 이주할 옛 판이 없습니다).
- 알림은 `BootstrapInfo`에 `settings_notice: Option<Notice>`(선택 필드, ts-rs `optional`)로 실어 보냅니다. 화면은 다른 알림과 같은 자리에 보여 줍니다. `Deps.log`(TD-151, A-3)가 생기면 같은 알림을 앱 로그에도 남깁니다.

### 앱은 효과 adapter만 등록합니다

`apps/secs2r/src/main.rs`는 서비스를 만들 때 효과 셋을 등록합니다. 업데이터(`deps.updates.set_policy`, 지금은 서비스 안), 아이콘(`icon::show`), 로컬 API(`Manager::apply_settings`). `Mutex<AppEffects>`, `apply_revision`, `local_api.rs:145-154`의 `on_change` 스레드는 사라집니다. 로컬 API 서버가 `app_settings_set`을 처리하면 효과는 저장소가 이미 보냈으므로 `secs://external-change` 이벤트만 남습니다.

`AppEffects`와 `RevisedAppSettings`는 `secs-service`의 공개 항목이지만 `secs-api`가 다시 내보내지 않으므로 ADR 0016의 호환 약속 밖입니다. `AppEffects`는 저장소의 private 부품이 됩니다.

### 바뀌지 않는 것

`app_settings_get`·`app_settings_set`의 wire, 설정 파일 형식과 경로, 효과의 순서 규칙(낡은 판의 효과는 버림), 업데이트 정책 효과가 저장 뒤에 나가는 시점.

## 시험

깊어진 interface에서 먼저 씁니다. `crates/secs-service/src/app_settings.rs`의 단위 시험과 `tests/config.rs`입니다.

- `open`: 없는 파일은 기본값과 알림 없음. 깨진 JSON과 1 MiB 초과는 기본값과 `app_settings_unreadable`. `version: 2`는 저장된 값을 읽지 않고 기본값, `app_settings_newer`, 그리고 `set`이 `Unsupported`로 거절하며 파일 바이트가 그대로인지.
- `set`: 판이 1씩 오르고, 파일이 원자적으로 바뀌며, 거절된 설정(창 크기 0)은 판과 파일을 바꾸지 않는지.
- 효과: 두 스레드가 `set`을 섞어 불러도 각 효과가 판 순서대로 한 번씩 `(previous, next)`를 받는지. `tests/config.rs:62`의 `ReorderedUpdates`는 이 시험으로 대체됩니다. 아이콘·로컬 API 효과는 가짜 `SettingsEffect`로 같은 시험에 넣습니다.
- typed reader: `apply()`가 저장된 `apply`를 돌려주고 `apply_settings_set`이 다른 필드를 건드리지 않는지.
- `BootstrapInfo.settings_notice`가 `open`의 알림을 그대로 싣는지(서비스 시험).
- 얕은 시험 삭제: `lib.rs:613-637`의 `effects_compare_with_the_last_applied_settings_td115`는 저장소의 효과 시험이 덮으므로 지웁니다.

## 이행

1. `app_settings.rs`와 단위 시험, `Service`가 저장소를 소유하도록 교체, 날것 읽기 다섯 자리와 `apply_settings_set` 교체, 업데이트 정책 효과를 `SettingsEffect`로. 한 커밋(Daybreak).
2. `open`의 알림과 읽기 전용 규칙, `BootstrapInfo.settings_notice`, 바인딩 재생성. 한 커밋(A-14, TD-169의 커밋).
3. 앱의 `AppEffects` 삭제와 효과 adapter 등록(`main.rs`, `local_api.rs`). 한 커밋.
4. 화면의 알림 표시와 카탈로그 두 줄(`notice.app_settings_unreadable`, `notice.app_settings_newer`). 한 커밋(Opus, 문구는 ASTRA).

각 단계 뒤 `cargo test -p secs-service --locked`, `cargo test -p secs-types`(바인딩), 화면의 `npm run typecheck && npm test`가 통과해야 합니다. 리뷰에서는 `lock(&self.app_settings)`가 서비스에 남지 않았는지, 앱에 `AppEffects`가 남지 않았는지, 효과가 잠금 밖에서 불리는지(효과 adapter가 서비스를 다시 부를 수 있으므로 데드락이 없어야 합니다)를 확인해야 합니다.

## 결과

- 판 순서 규칙이 인스턴스 하나에만 있고, 설정을 읽는 자리는 typed reader 다섯뿐입니다.
- 설정 파일을 잃는 두 경우(손상, 더 새로운 판)가 알림과 거절로 바뀝니다. 사용자가 보는 동작이 바뀌므로 A 항목입니다. 편차 항목은 없습니다. 업스트림에는 앱 설정 파일이 없습니다.
- `BootstrapInfo`에 선택 필드가 하나 늘어납니다. 0.x의 wire 추가이고 메서드 id는 그대로입니다(ADR 0003).
- 위험: 효과 adapter가 저장소를 다시 부르면(예: 로컬 API 효과가 설정을 읽음) 잠금 순서가 문제가 됩니다. 효과는 잠금 밖에서 `(previous, next)` 사본으로 불리므로 adapter는 저장소를 읽지 않아도 됩니다. 리뷰 체크리스트에 둡니다.
- 다음 리뷰가 다시 제안하지 않을 것: `AppEffects`가 저장소 안에 private으로 남는 것(순서 규칙의 자리), 업데이트 정책 효과가 서비스에 등록되는 것(`Deps.updates`가 서비스의 의존이기 때문).
