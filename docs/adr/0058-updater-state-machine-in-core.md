# 0058. 업데이터의 상태 기계를 서비스 크레이트에 둡니다

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A17. 기술 부채 감사의 TD-164(업데이트 실패의 원인이 버려짐)와 TD-204(설치본 전체를 상한 없이 메모리에 받음)를 해결합니다.

## 배경

ADR 0020의 정책 규칙(6시간마다 확인, `auto`는 뒤에서 받고 닫을 때 설치, `notify`는 확인만, `off`는 묻지 않음, 포터블은 알림만)은 `apps/secs2r/src/update.rs`의 `TauriUpdates` 안에 있습니다. 아래 줄 번호는 main `885f0535` 기준입니다.

- `check_locked`(`update.rs:225-295`), `download_work`(`:297-366`), `download_locked`(`:368-382`), `on_exit`(`:406-427`), `set_policy`(`:490-517`)가 정책 × 단계 × 사건 → 다음 단계 규칙을 `tauri::async_runtime::spawn`과 `app.updater_builder()` 호출 사이에 끼워 두었습니다. 상태는 `Inner { status, update: Option<tauri_plugin_updater::Update>, bytes, task, generation, stopped }`(`:18-25`)이고 Tauri 타입이 상태의 일부입니다.
- 시험(`:524-632`)은 도우미 여섯(`initial_phase`, `found_phase`, `endpoint_override`, `notice_from_pending`, `require_download`, `require_restart`)만 닿습니다. 전환 자체는 CI의 `update-e2e`(설치본 A에서 B로 올리는 GUI 시험)만 봅니다.
- 화면의 모의 서비스에도 같은 전환이 따로 구현되어 있습니다(`apps/secs2r/ui/src/ipc/mock.ts:1263-1293`의 `update_*`와 `app_settings_set`의 일곱 갈래). deletion test로 보면 `set_policy`를 지워도 `mock.ts:1285-1293`에 같은 갈래가 남으므로 한 module이 두 번 삽니다(ADR 0014가 금한 규칙 사본).
- `secs-service`의 `UpdateAdapter` trait(`lib.rs:243-256`)는 `status`, `check`, `download`, `restart`, `dismiss_notice`, `set_policy`를 받고 adapter는 `TauriUpdates`와 `NoUpdates` 둘입니다. 정책은 `project_edit.rs:830`이 앱 설정 저장 뒤 `set_policy`로 전달합니다.
- 확인·내려받기 실패는 `map_err(|_| "Could not check for updates")`, `Err(_) =>`로 원인을 버립니다(`:252-258, 355-359`). 앱 로그(ADR 0027)에는 잡과 요청 실패만 남으므로 '업데이트가 안 된다'는 보고를 진단할 수 없습니다(TD-164).
- 내려받기는 설치본 전체를 `inner.bytes: Vec<u8>`(`:342`)로 받고 크기 상한이 없습니다. `SECS2R_UPDATE_ENDPOINT`로 끝점을 바꾸면 `http://`도 받으므로, 서명 검사에서 거절될 파일이라도 메모리는 다 쓰게 할 수 있습니다(TD-204).

## 결정

### `secs-service`에 순수 `UpdateMachine`을 둡니다

새 module `crates/secs-service/src/update_machine.rs`입니다. 타입은 `secs-types`의 `UpdateStatus`, `UpdatePhase`, `UpdatePolicy`, `UpdateInstall`, `PendingUpdate`, `UpdateNotice`를 그대로 씁니다.

```rust
pub struct UpdateMachine {
    status: UpdateStatus,
    portable: bool,
    install: UpdateInstall,          // 패키지 종류가 정함(deb는 OnRequest)
    found: Option<FoundRelease>,     // 마지막 확인이 찾은 판
    generation: u64,                 // 취소된 작업의 늦은 결과를 거르는 번호
    stopped: bool,
}

pub struct FoundRelease { pub version: String, pub notes: String, pub previous: Option<String> }

pub enum UpdateEvent {
    Start { policy: UpdatePolicy, notice: Option<UpdateNotice> },
    Tick,                                             // 6시간 주기와 시작 15초 뒤
    Check,                                            // update_check
    CheckResult { generation: u64, result: Result<Option<FoundRelease>, UpdateFailure> },
    Download,                                         // update_download
    DownloadProgress { generation: u64, received: u64, total: Option<u64> },
    DownloadResult { generation: u64, result: Result<(), UpdateFailure> },
    Restart,                                          // update_restart
    DismissNotice,
    SetPolicy(UpdatePolicy),
    Exit,
}

pub struct UpdateFailure { pub reason: &'static str, pub cause: String }

pub enum UpdateCommand {
    Check { generation: u64 },
    Download { generation: u64, release: FoundRelease },
    Cancel,                                           // 돌고 있는 확인·내려받기를 버림
    SavePending(PendingUpdate),
    RemovePending,
    Install { restart: bool },
    Log { level: LogLevel, line: String },            // 원인을 앱 로그에 남김(TD-164)
}

impl UpdateMachine {
    pub fn new(current: &str, portable: bool, install: UpdateInstall) -> Self;
    pub fn status(&self) -> &UpdateStatus;
    pub fn step(&mut self, event: UpdateEvent) -> Result<Vec<UpdateCommand>, IpcError>;
}
```

규칙은 모두 `step` 안에 있고 ADR 0020의 문장 하나가 `match` 가지 하나입니다. 지금 adapter에 흩어진 판정을 그대로 옮깁니다.

- `Start`는 정책이 `off`면 `Off`, 아니면 `Idle`로 시작하고 알림을 싣습니다. `Tick`은 정책이 `off`가 아니고 단계가 `Checking`, `Downloading`, `Ready`, `Installing`이 아닐 때만 `Check`를 냅니다.
- `CheckResult`는 `generation`이 다르거나 `stopped`면 버립니다. 찾았으면 포터블은 `Portable { version, url }`, 아니면 `Available`이고 정책이 `auto`면 곧바로 `Downloading`과 `Download` 명령입니다. 못 찾았으면 `UpToDate`, 실패면 `Failed { reason }`와 `Log`(원인 포함)입니다.
- `DownloadProgress`는 `received`가 `total`을 넘거나 상한 `MAX_DOWNLOAD_BYTES`(512 MiB)를 넘으면 `Cancel`과 `Failed { reason: "The update is larger than allowed" }`로 끝냅니다(TD-204). 상한은 Windows 설치본(약 30 MiB)과 AppImage(약 120 MiB)의 열 배보다 큽니다.
- `DownloadResult`의 성공은 `SavePending`(판, 바뀐 점, 건너뛴 판)과 `Ready { install }`이고 실패는 `Failed`와 `Log`입니다. `SavePending`이 실패했다고 adapter가 `DownloadResult`를 다시 보내면 `Failed { reason: "Could not save update details" }`입니다.
- `SetPolicy`는 지금 `set_policy`의 네 갈래 그대로입니다. `off`는 `Cancel`, `RemovePending`, `Off`. `notify`로 바꾸는데 받는 중이면 `Cancel`과 `Available`. `auto`로 바꾸는데 `Available`이면 `Download`. `Off`에서 켜면 `Check`. `Installing`과 `stopped`에서는 정책만 바꿉니다.
- `Restart`는 `Ready`에서만 받아 `Installing`과 `Install { restart: true }`를 냅니다. `Exit`는 `stopped = true`, `Cancel`을 내고, `Ready { install: OnExit }`면 `Install { restart: false }`를 냅니다.
- `DismissNotice`는 알림을 지우고, 받은 판이 없고 설치 중이 아니면 `RemovePending`을 냅니다. 지금 `dismiss_notice`(`:473-488`)의 조건과 같습니다.

### `TauriUpdates`는 명령만 실행합니다

`apps/secs2r/src/update.rs`에 남는 것은 `Mutex<UpdateMachine>`을 쥐고 `step`을 부르는 일과 명령 실행입니다. `Check { generation }`은 `updater_builder().check()`를 띄워 결과를 `CheckResult`로 되돌리고, `Download`는 `Update::download`의 진행 콜백을 `DownloadProgress`로, 끝을 `DownloadResult`로 되돌립니다. `Install`은 `update.install(bytes)`와 `app.restart()`, `SavePending`/`RemovePending`은 `update.json` 파일, `Log`는 `AppLog::line`입니다. `tauri_plugin_updater::Update`와 받은 바이트는 adapter가 들고 있고 기계는 모릅니다. `Cancel`은 `generation`을 올리고 `JoinHandle::abort`를 부르는 지금의 `cancel_locked`입니다. 6시간 주기의 조건 변수 루프(`start`, `:179-216`)는 그대로 두되 `Tick`을 보낼 뿐 단계를 보지 않습니다.

`secs2r-host-shell`의 업데이터(ADR 0020 '관리 서비스와 셸')는 이 ADR의 범위 밖입니다. 셸은 앱이 꺼진 동안 조용히 설치하는 다른 정책이고 화면 상태가 없습니다.

### 모의 서비스

`mock.ts`의 일곱 갈래 사본은 A11(ADR 0052)이 서비스 기록으로 바꿀 때 지웁니다. 그때까지는 그 자리에 이 ADR과 `update_machine.rs`를 가리키는 주석을 두고, 갈래의 수를 늘리지 않습니다.

## 시험

`crates/secs-service/src/update_machine.rs`의 단위 시험이 ADR 0020의 규칙을 문장마다 하나씩 고정합니다. 기대값은 코드가 아니라 ADR의 문장에서 옵니다.

- `auto`에서 찾으면 `Downloading`과 `Download` 명령, `notify`에서 찾으면 `Available`과 명령 없음, 포터블에서 찾으면 `Portable`과 릴리스 URL.
- `Tick`은 `Checking`, `Downloading`, `Ready`, `Installing`에서 아무것도 내지 않습니다.
- 늦게 온 `CheckResult`(옛 `generation`)는 단계를 바꾸지 않습니다.
- `SetPolicy(Off)`는 `Cancel`, `RemovePending`, `Off`. 받는 중에 `Notify`는 `Available`로 돌아가고, `Available`에서 `Auto`는 `Download`.
- `Exit`는 `Ready { OnExit }`에서만 `Install { restart: false }`, `Ready { OnRequest }`(deb)에서는 설치하지 않습니다.
- 실패는 사용자 문장(`reason`)을 그대로 두고 `Log`에 원인 문자열이 들어 있습니다(TD-164).
- `DownloadProgress`가 상한을 넘으면 `Cancel`과 `Failed`이고, 그 뒤의 `DownloadResult`는 버립니다(TD-204).
- `secs-service`의 `app_settings_set` 시험이 정책 변경을 `set_policy`로 전달하는 것은 지금 그대로입니다.

앱 크레이트의 시험 여섯은 도우미와 함께 기계 module로 옮기거나 지웁니다(`require_download`/`require_restart`는 `step`의 거절이 대신합니다). `update-e2e`는 그대로 돕니다.

## 이행

1. `update_machine.rs`와 시험. 동작은 바뀌지 않습니다(adapter가 아직 쓰지 않음).
2. `TauriUpdates`를 기계의 명령 실행기로 다시 만듭니다. `UpdateAdapter` trait와 wire는 그대로입니다.
3. `Log` 명령을 `AppLog`에 잇습니다(TD-164, A 항목: 끝점이 답하지 않을 때 앱 로그에 원인이 남는 시험).
4. `DownloadProgress` 상한(TD-204, A 항목).
5. `mock.ts` 주석. A11이 들어오면 사본 삭제.

## 결과

- ADR 0020의 정책 규칙이 한 타입에 있고 Tauri 없이 시험됩니다. 새 정책(예: 특정 판 건너뛰기)은 `step`의 가지 하나와 시험 하나입니다.
- 실패 원인이 앱 로그에 남고, 설치본 크기가 상한을 넘으면 메모리를 다 쓰기 전에 멈춥니다.
- 바뀌지 않는 것: `UpdateStatus`/`UpdatePhase`의 wire 모양, `update.json`의 형식, 화면과 셸.
- 위험: 기계와 adapter 사이의 `generation` 규약이 어긋나면 늦은 결과가 새 상태를 덮습니다. 시험이 늦은 결과를 고정하고, adapter는 모든 결과에 자기 `generation`을 실어 보냅니다.
- 편차 항목은 없습니다. 업스트림에는 업데이터가 없습니다.
- 다음 감사는 `TauriUpdates`가 Tauri 타입(`Update`, `JoinHandle`)을 쥐고 있는 것을 다시 지적하지 않습니다. 그것은 adapter의 몫이고 규칙은 기계에 있습니다.
