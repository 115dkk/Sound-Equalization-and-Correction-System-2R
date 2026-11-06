# 0057. 관리 서비스의 전환을 `decide` 하나에 모읍니다

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A16. 기술 부채 감사의 TD-189(전환 결정이 세 곳에 있음), TD-188의 일부(시작 실패가 서비스 루프를 끝내고 종료 코드 0을 보고함), TD-240(콘솔 연결·해제 이벤트를 버림)을 해결합니다.

## 배경

ADR 0018의 '관리 전환' 절은 서비스가 상태와 요청과 세션 이벤트를 받아 할 일을 정하는 규칙이 `apps/secs2r-host/src/transitions.rs` 한 곳에 있고 실행은 그 결과를 따르기만 한다고 정했습니다. 지금 코드는 그 문장과 어긋납니다. 아래 줄 번호는 main `885f0535` 기준입니다.

- `transitions::decide(state, input) -> (State, Vec<Effect>)`(`transitions.rs:132-268`)는 순수 함수이고 `drive`(`:286-332`)가 `Platform` trait(`:98-107`)으로 효과를 실행합니다. 여기까지는 ADR 0018대로입니다.
- 자식 종료는 `decide`를 거치지 않습니다. `main.rs:587-637`의 `service_loop`가 `child.wait`로 종료를 보면 `transition_state.running_child_session = None`을 직접 대입하고, 두 번째 규칙 엔진인 `rules::ProcessRules`(`rules.rs:757-808`, 안에 `Supervisor`)에 `ProcessEvent::Exit`를 전달해 `RestartAfter`/`GiveUp`을 받은 뒤 `restart_at`, `transition_state.restart_scheduled`, `core.camilla.state`(`BackingOff`, `GivenUp`), `core.camilla.restarts_in_window`를 직접 갱신합니다. 재시작 시각이 되면 `RestartTick`(`:640-657`)만 `decide`로 갑니다.
- 로그온 처리(`main.rs:472-508`)는 `ProcessRules`가 `Launch`를 돌려줄 때 `transition_state.running_child_session = None`으로 `decide`를 '찔러' `LaunchChild` 효과가 나오게 하고, 로그오프 처리(`:509-541`)는 `drive` 뒤에 `core.camilla.state = NoSession`과 `pid = None`을 직접 대입합니다.
- 관리 중지(`main.rs:848-867`)는 `decide` 다음에 `core.camilla = Default::default()`를 대입하고, 곧이어 `drive`의 `ClearCamillaStatus`가 같은 일을 다시 합니다.
- `State`(`transitions.rs:58-66`)의 `running_child_session`과 `restart_scheduled`를 `main.rs:483, 592`, `:611, 628`이 바깥에서 갱신합니다. `ProcessRules`는 같은 사실을 `has_session`, `running` 두 불리언으로 한 번 더 가집니다.
- `rules.rs:550-585`의 `Core::handle`에는 소유자 SID 규칙 없이 `SetManaged`를 합치는 TD-111 이전 사본이 남아 있고, 생산 코드는 `main.rs:810-867`이 가로채므로 닿지 않습니다. 시험 `request_handling_merges_targets_and_stop_clears_them`(`rules.rs:1158-1177`)이 그 사본을 고정합니다.
- `rules.rs`는 이름과 달리 상태 파일과 로그 I/O(`load_state`, `save_state`, `load_logs`, `append_log`, `:598-700`)도 맡습니다.
- 시작 때 `configure_watcher(..)?`(`main.rs:410`)가 실패하면(Equalizer APO를 지웠는데 `host.json`에 eqapo 대상이 남은 경우) `service_loop`가 끝나 파이프를 열지 않고, `secs-sys/src/host.rs:1068`이 SCM에 종료 코드 0을 보고해 복구 동작이 돌지 않습니다(TD-188).
- `secs-sys`의 `ServiceEvent`(`host.rs:1021-1025`)는 `SessionLogon`과 `SessionLogoff`만 전달하고 빠른 사용자 전환의 `ConsoleConnect`/`ConsoleDisconnect`는 버립니다(`:1085-1093`, TD-240).

Windows에서만 도는 코드라 Linux CI가 닿는 것은 `decide`의 순수 시험(`transitions.rs:338-883`, 15개)뿐인데, 가장 자주 바뀌는 동작(자식 종료, 백오프, 포기)이 그 밖에 있습니다. deletion test로 보면 `ProcessRules`를 지웠을 때 그 규칙이 `State`에 이미 있으므로 pass-through이고, `main.rs`의 직접 갱신 네 자리는 `decide`가 알아야 할 사실을 바깥에서 되풀이한 것입니다.

## 결정

### `transitions.rs`가 전환의 유일한 소유자입니다

`Input`에 다음을 추가합니다.

| 입력 | 뜻 | 지금의 자리 |
|---|---|---|
| `ChildExited { pid: Option<u32>, exit_code: i32, now: Instant }` | 관리하는 CamillaDSP가 끝났음 | `main.rs:587-637` |
| `PortBusy { reason: &'static str, detail: String, now: Instant }` | 띄우려는 포트를 남이 쥐고 있어 띄우지 못했음 | `main.rs:1177-1216`(`launch` 안의 백오프) |
| `Console { event: ConsoleEvent, session_id: u32, owner_matches: bool }` | 빠른 사용자 전환으로 콘솔 세션이 바뀜(`ConsoleEvent::Connect`, `Disconnect`) | 없음(TD-240) |
| `WatcherFailed { detail: String }` | 시작이나 대상 변경에서 `config.txt` 감시자를 만들지 못함 | `main.rs:410`의 `?` |

`State`가 재시작 정책을 가집니다. `rules::Supervisor`(5분 안에 다섯 번 끝나면 포기, 그 전에는 백오프 지연)를 `State.restart: Supervisor`로 옮기고, `restart_scheduled: bool`과 `main.rs`의 `restart_at: Option<Instant>`를 `State.restart_due: Option<Instant>` 하나로 바꿉니다. `running_child_session`은 `pub` 필드가 아니라 `State::running_child_session() -> Option<u32>` 읽기 전용 접근자가 되고, 바꾸는 곳은 `decide`와 `drive`뿐입니다. `ProcessRules`와 `ProcessEvent`, `ProcessAction`은 지웁니다.

`Effect`에 다음을 추가합니다.

| 효과 | `Platform` 메서드 | 뜻 |
|---|---|---|
| `SetCamillaState(ManagedProcessState)` | `set_camilla_state` | `Core.camilla.state`(`BackingOff { until }`, `GivenUp { reason }`, `NoSession`, `Stopped`)를 바꿈. `Core.camilla`는 `drive`만 갱신하는 투영이 됩니다 |
| `CountRestart` | `count_restart` | `restarts_in_window`를 올림 |

재시작 시각은 효과가 아니라 상태입니다. `service_loop`는 `state.restart_due`를 읽어 시각이 지나면 `Input::RestartTick`을 전달합니다. 지금 `Platform::stop_child`와 `launch_child`가 `restart_at`을 지우는 일은 `decide`가 `restart_due = None`으로 합니다.

`drive`에서 `ReplaceWatcher`의 실패는 `RepairEqapo`의 실패와 같은 등급입니다. `?`로 루프를 끝내는 대신 `state.eqapo_error`에 담고 `repair_failed`와 같은 모양의 `watcher_failed` 줄을 기록한 뒤 다음 효과를 이어 갑니다. 그래서 Equalizer APO가 사라진 채 남은 대상은 `IncludeState::Damaged`와 오류 문장으로 보이고, CamillaDSP와 파이프는 그대로 돕니다(TD-188 가운데 루프가 끝나는 문제). `service_loop`가 `Err`로 끝나면 `secs-sys`의 서비스 진입점이 0이 아닌 `ServiceExitCode`를 보고해 SCM 복구가 돕니다(TD-188 가운데 종료 코드 문제, `secs-sys/src/host.rs:1056-1068`).

`secs-sys`의 `ServiceEvent`에 `ConsoleConnect(u32)`와 `ConsoleDisconnect(u32)`를 추가합니다. `service_loop`는 둘을 `Input::Console`로 전달하고, `decide`는 소유자 세션의 `Disconnect`를 소유자 로그오프와 같이(자식을 멈추고 재시작을 취소), 소유자 세션의 `Connect`를 소유자 로그온과 같이(자식을 띄우고 복구) 다룹니다. 다른 사용자의 `Connect`는 `OtherUser` 표시입니다. 이렇게 해야 ADR 0018의 '다른 사용자가 콘솔을 쓰면 CamillaDSP를 멈추고 `other_user`'가 다음 요청이 아니라 그 순간에 일어납니다.

### `main.rs`에 남는 것

운영체제에 묻는 일(`active_console_session`, `session_user_identity`), 이벤트 펌프, 파이프 서버, 시각(`Instant::now()`)을 `Input`에 실어 `decide`를 부르는 일, `drive`에 `RealTransitionPlatform`을 전달하는 일, `state.restart_due`와 수리 재시도 시각으로 타이머를 돌리는 일입니다. `transition_state`의 필드를 직접 대입하는 줄과 `core.camilla`를 직접 대입하는 줄은 모두 없어집니다. `launch`(`main.rs:1133-1257`)는 포트 인수와 프로세스 생성만 하고, 포트가 막혔을 때의 백오프는 `Input::PortBusy`로 `decide`에 돌려줍니다.

### `rules.rs`와 `Core`

`rules.rs`는 순수 규칙만 둡니다. `load_state`, `save_state`, `load_logs`, `append_log`, `read_bounded`, `write_staged`, `replace_host_executable`처럼 파일을 읽고 저장하는 함수는 `files.rs`로 옮깁니다(이름은 그대로, 파일 위치만 바뀜). `Core::handle`의 `SetManaged`와 `StopManaging` 가지는 `UpdateSelf`처럼 `ErrorCode::Unsupported`를 돌려주고, 사본을 고정하던 시험 `request_handling_merges_targets_and_stop_clears_them`은 지웁니다. 합치는 규칙은 `transitions::managed_after_update` 하나뿐이고 그 시험(`changing_owner_discards_the_previous_users_targets`, `same_owner_keeps_targets_omitted_from_an_update`)이 이미 있습니다.

### ADR 0018

'관리 전환' 절의 첫 문장을 이 ADR을 가리키도록 고치고, 자식 종료·백오프·포기·콘솔 전환도 같은 `decide`가 정한다고 밝힙니다. 이 ADR이 채택되기 전까지는 그 절에 '자식 종료와 백오프는 `service_loop`가 정합니다'라는 예외를 남깁니다.

## 시험

깊어진 interface에서 먼저 씁니다. 모두 `transitions.rs`의 순수 시험이라 세 운영체제의 CI가 돌립니다.

- 자식 종료 한 번 → `restart_due`가 정책의 첫 지연만큼 뒤이고 `SetCamillaState(BackingOff { until })`와 `CountRestart`, `exit` 로그가 나옵니다. 5분 안에 다섯 번 → `SetCamillaState(GivenUp { reason: "five exits within five minutes" })`와 `given_up` 로그가 나오고 `restart_due`는 비어 있습니다. 세션이 없을 때의 종료는 재시작을 예약하지 않습니다.
- `restart_due`가 지난 `RestartTick` → `LaunchChild`, 소유자 로그오프나 관리 중지 → `restart_due`가 비워지고 `StopChild`.
- `PortBusy` → 백오프와 `port_busy` 로그, 다섯 번째에는 포기.
- 소유자의 `Console { Disconnect }` → `StopChild`와 `NoSession`, 소유자의 `Console { Connect }` → `LaunchChild`와 `RepairEqapo`, 다른 사용자의 `Connect` → `SetCamillaState(OtherUser)`.
- `drive`에서 `replace_watcher`가 `Err` → `eqapo_error`가 차고 `watcher_failed` 로그가 남으며 뒤의 효과(`LaunchChild`)가 그대로 실행됩니다.
- `FakePlatform`이 `set_camilla_state`를 기록해, 로그오프 뒤 `core.camilla`가 `NoSession`인 것을 `main.rs`가 아니라 효과로 확인합니다.
- `secs-sys`: 서비스 진입점이 작업자의 `Err`를 0이 아닌 종료 코드로 바꾸는 순수 함수(`exit_code_for`)의 단위 시험.

실제 서비스 동작은 지금처럼 CI의 `manage-e2e`(Windows 러너에 설치하고 GUI에서 관리를 켬)가 확인하고, 콘솔 전환은 이 Windows PC에서 `tsdiscon`/재접속으로 손으로 확인해 결과를 PR에 남깁니다. 유지보수자의 Windows PC가 이 저장소의 작업 환경이므로 `cargo test -p secs2r-host -p secs-sys`도 거기서 돕니다.

## 이행

커밋 단위로 나눕니다. 각 단계 뒤에 `cargo test -p secs2r-host`와 Linux에서의 `cargo check --target x86_64-pc-windows-msvc`가 통과해야 합니다.

1. `Supervisor`를 `State`로 옮기고 `Input::ChildExited`, `PortBusy`, `Effect::SetCamillaState`, `CountRestart`, `restart_due`를 시험과 함께 추가합니다. `main.rs`는 아직 옛 방식대로 처리합니다.
2. `service_loop`의 자식 종료·재시작·포트 백오프 처리를 `decide`로 바꾸고 `ProcessRules`를 지웁니다.
3. 로그온·로그오프·관리 중지의 직접 갱신을 지우고 `Core.camilla`를 투영으로 만듭니다.
4. `WatcherFailed`와 종료 코드(TD-188).
5. `ServiceEvent::ConsoleConnect/Disconnect`와 `Input::Console`(TD-240). `secs-policy`의 `OS_PACKAGES`에 `windows-registry`를 추가하는 일도 TD-240의 같은 커밋입니다.
6. `Core::handle` 사본과 그 시험 삭제, `files.rs` 분리.
7. ADR 0018 문장 수정.

## 결과

- ADR 0018의 '전환 규칙은 한 곳'이 참이 됩니다. 자식 종료와 백오프가 순수 시험으로 들어오고, Windows 러너 없이도 전환 규칙의 회귀를 잡습니다.
- 서비스 루프가 시작 실패에서 끝나지 않고, 끝날 때는 SCM이 알 수 있습니다. 빠른 사용자 전환도 로그온·로그오프와 같은 규칙으로 처리됩니다.
- 바뀌지 않는 것: 파이프 프로토콜, `HostRequest`/`HostResponse`, 상태 파일 `host.json`과 로그 형식, 앱과 셸이 보는 `HostStatus`.
- 위험: 콘솔 전환 알림은 Windows가 주는 이벤트라 이 PC의 실측으로만 확인합니다. `ServiceEvent`가 넓어지므로 `secs-sys`의 unsafe 예산(3)은 그대로이고 새 unsafe는 없습니다.
- 편차 항목은 없습니다. 업스트림에는 관리 서비스가 없습니다.
- 다음 감사는 `State`와 `Core.camilla`의 분리를 다시 제안하지 않습니다. `Core.camilla`는 화면에 보이는 투영이고 `State`가 정본입니다.
