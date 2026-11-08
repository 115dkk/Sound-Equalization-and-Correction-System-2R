# P187 (Daybreak): audit #186, Category A, the Windows managed service

Tree: `E:\wt\audit-host` (git worktree of the repository, branch `audit/186-host`).
Set `CARGO_TARGET_DIR=C:\cargo-target\audit-host` for every cargo command.
Work only in that tree. This machine is Windows 11, so every test here runs
for real; nothing is "static reading" for you.

Allowed files: `apps/secs2r-host/**`, `crates/secs-sys/src/host.rs`,
`crates/secs-sys/src/lib.rs`, `crates/secs-policy/tests/gates.rs` (the
`OS_PACKAGES` array only), `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`
(one sentence under '관리 전환' or '등록과 권한' if the text no longer matches
the code after an item), `features.toml`. Forbidden: everything else,
including `crates/secs-service/**`, `apps/secs2r/**`, `apps/secs2r-host-shell/**`,
`crates/secs-types/**`.

Do not push. Do not spawn sub-agents. Commit on `audit/186-host` only.

Budget rules you must not break (CLAUDE.md, ADR 0002, `docs/UNSAFE_BOUNDARIES.md`):
`secs-sys` is the only crate that may hold OS dependencies, and its `unsafe`
count (3, counted by `crates/secs-policy/tests/gates.rs`) must not grow. If
an item needs a Windows API that win-custody or the `windows` crate does
not wrap safely, stop that item and report what is missing.

Read first: `CLAUDE.md`, `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`
('관리 전환', '등록과 권한', '보존'), `apps/secs2r-host/src/main.rs`
(`service_loop`, `configure_watcher`, `managing_session`, `record`,
`handle_request`, `takeover_port`, `repair_eqapo`), `apps/secs2r-host/src/transitions.rs`
(`Input`, `Effect`, `decide`), `apps/secs2r-host/src/rules.rs`,
`crates/secs-sys/src/host.rs` (`pipe_server`, `pipe_call_at`, `PIPE_SDDL`,
`PIPE_TIMEOUT`, `PIPE_RESPONSE_TIMEOUT`, the `windows_service` glue near the
end: `status`, `run_inner`, the session-change handler), the win-custody
crate's docs for `PipeServer`, `PipeClient`, `SecurityDescriptor`,
`process_session_id` and whatever it offers to read a process's owner
(`cargo doc -p win-custody --no-deps --open`, or the source under
`%USERPROFILE%\.cargo\registry\src`). The audit was written against main
`97a27818`; line numbers may have drifted.

## Why

The audit read the managed service (`secs2r-host`, SYSTEM) and found that a
start-up failure ends its loop while reporting exit code 0 (so the Service
Control Manager never recovers it), that the named pipe is re-created every
100 ms so any local process can take its name in between, that a slow
handler blocks the single service thread for up to 30 s, and that fast
user switching never reaches the transition rules. Each item changes what
the service does and is done test-first; the pipe items also need one real
run on this machine.

## Rules for every item

- One finding, one commit. Subject `fix(audit): A-n <한국어 설명> (#186 TD-xxx)`,
  Korean body saying what the Red test proves and what changed, last line
  `Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>`.
- Red expected values from literals and the spec (ADR 0018, the finding,
  Windows documentation you cite), never from the code under test.
- Gate per item, foreground: `cargo fmt --all -- --check`,
  `cargo clippy -p secs-sys -p secs2r-host --all-targets --locked -- -D warnings`,
  `cargo clippy --workspace --exclude secs2r --exclude secs2r-host-shell --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings`
  (the CI's Windows lint), `cargo test -p secs-sys -p secs2r-host --locked`,
  and `cargo test -p secs-policy --locked` (the unsafe and OS-package gates).
- At the end: `cargo test --workspace --locked`.
- Register each new test in `features.toml` beside `apply.managed_host`
  (and `apply.supervise` for process rules).
- Tidy touches only lines this cycle wrote.

## Items, in order

### A-21 (TD-188): start-up failures do not end the service silently

Finding: "시작 때 `configure_watcher(..)?`는 `validate_eqapo_path`가 실패하면
(Equalizer APO를 지웠는데 `host.json`에 eqapo 대상이 남은 경우) `service_loop`를
끝내고, `managing_session(..)?`·`record(..)?`·`running.wait(..)?`도 일시적인
토큰이나 로그 추가 실패로 끝냅니다. 그러면 파이프를 열지 않아 앱이
`StopManaging`을 보낼 수 없고, 재시작마다 같은 상태에 걸리며, SCM에는 종료 코드
0을 보고해(`host.rs:1068`) 복구 동작이 돌지 않습니다."

Seams: `transitions::decide` (a new `Input::WatcherFailed { detail }` or the
existing config-changed path) and the service exit code in
`crates/secs-sys/src/host.rs::status`/`run_inner`.
Red 1 (`apps/secs2r-host`, pure): a state with a saved eqapo target and a
watcher failure (`validate_eqapo_path` error) → `decide` yields an effect
that records `eqapo_error` and sets the include state `Damaged`, and the
next state still accepts `StopManaging`; today the loop would have
returned. Write it the way the existing `transitions` tests are written
(`grep -n "fn " apps/secs2r-host/src/transitions.rs` for the test module).
Red 2 (`secs-sys`): `ServiceExitCode` for a worker that returns `Err` is
`Win32(<non-zero>)`: find the smallest seam (`fn exit_code_for(result: &Result<..>) -> ServiceExitCode`,
pure) and test it with `Ok(())` → `Win32(0)` and `Err(..)` → `ServiceSpecific(1)`
or `Win32(ERROR_SERVICE_SPECIFIC_ERROR)`; cite the Windows docs for which
code triggers SCM recovery actions ("recovery actions run when the service
stops with an error" needs a non-zero exit code; `sc failureflag` is off by
default, say so in the commit body).
Green: in `service_loop`, `configure_watcher` failure → the Red 1 path
instead of `?`; `managing_session` failure → one log line and `None`;
`record`/`append_log` failure → non-fatal (log once per minute at most);
`running.wait` error → treat as exited with an unknown code. The worker's
`Err` reaches `status(.., Stopped)` with the non-zero code.

### A-22 (TD-190): the pipe name cannot be taken over, and the client checks who answers

Finding: "서비스가 100 ms마다 `\\.\pipe\secs2r-host` 인스턴스를 새로 만들고
시한이 지나면 버리므로, 그 틈에 아무 로컬 프로세스가 이름을 차지할 수 있습니다.
그러면 `FILE_FLAG_FIRST_PIPE_INSTANCE` 때문에 서비스의 생성이 모두 실패해 `host.log`에
초당 약 5줄이 쌓여 회전하고, 클라이언트는 `pipe_call_at`이 `server_process_id()`를
버려 서버가 SYSTEM인지 확인하지 않으므로 가로챈 쪽과 대화합니다."

Seams: `secs_sys::host::pipe_server` (keeps an instance across accept
polls) and `pipe_call_at` (verifies the server).
Red 1 (`secs-sys`, real pipes, `cfg(windows)` test next to the existing
pipe round-trip tests near line 1181): start the server on a test pipe name
in a thread that loops `pipe_server` with a 100 ms accept timeout for one
second; meanwhile, in the test thread, try every 10 ms to create a
`PipeServer` with the same name and `FIRST_PIPE_INSTANCE` semantics
(win-custody's `PipeServer::create`); assert that the attempt never
succeeds (today it succeeds in the gaps). Then a client call still round-trips.
Red 2 (`secs-sys`): `pipe_call_at` against a pipe served by this test
process (not SYSTEM) returns `Err` whose detail names the server's process
id and "not the managed service"; the round-trip tests that legitimately
serve from the test process get an explicit `pipe_call_at_unchecked` (test
only, `#[cfg(test)]`) or a check hook they can turn off; production
`pipe_call_at` always checks. The check: the server PID's session is 0 and
its owner is `SYSTEM` (win-custody `process_session_id` gives the session;
for the owner look for a win-custody helper; if none exists, use the image
path of the PID being `<Program Files>\SECS 2R\secs2r-host.exe` as ADR 0018
'등록과 권한' describes, through an existing safe helper, and say in the
commit body which rule you implemented).
Green: a `PipeServerSlot` kept in `service_loop` across iterations (create
the next instance before the served one is dropped, so the name is never
free), creation failures backed off (log once, retry after 1 s), the SDDL
descriptor parsed once per process, and the client check. Real run: after
the gate, `cargo build -p secs2r-host --locked`, then from the test (not
from an installed service) prove with the Red 1 test that the gap is closed
on this machine; quote the test output in the report.

### A-23 (TD-191): a long handler does not block the service

Finding: "서비스 스레드 하나가 파이프 핸들러 안에서 막힙니다. `takeover_port`는
최대 5 s + 5 s + 접속 3 s, `repair_eqapo`는 도우미를 30 s 기다리는데 클라이언트의
응답 대기도 30 s이고 SCM 정지와 로그온 이벤트가 그 뒤에 줄을 섭니다. [...] 응답
쓰기는 핸들러 전에 만든 절대 5 s `PipeWait`를 쓰므로 핸들러가 5 s를 넘기면 보류된
쓰기가 TimedOut이 됩니다."

Seams: `handle_request` for `SetManaged`/repair (answers `HostResponse::Accepted`
and schedules the work) and `pipe_server`'s response write deadline.
Red 1 (`secs-sys`): a handler that sleeps 6 s still gets its response
written (today the write times out at 5 s): a real-pipe test with a client
that waits `PIPE_RESPONSE_TIMEOUT` and asserts the response arrives.
Red 2 (`apps/secs2r-host`, pure): `decide`/the request path for a port
takeover answers `Accepted` immediately with the work recorded as pending
in the state (`transition_state.pending_takeover` or whatever name the
transitions module already uses for scheduled work), and the next loop
ticks drive it in `drive_transition` with the 100 ms cadence instead of
blocking. Keep the existing `HostResponse` variants: if `Accepted` does not
exist in `secs-types` (forbidden), use the variant the shell already reads
for "in progress" (grep `apps/secs2r-host-shell` for how it polls status) and
say so.
Green: the write `PipeWait` is created after the handler returns, sized
`PIPE_RESPONSE_TIMEOUT`; `takeover_port`'s `5 s` wait and `repair_eqapo`'s
`30 s` helper wait become state-machine steps polled from the loop (or run on
a worker thread that reports back through the existing `config_sender`-style
channel), with the pipe answering at once. Logon/stop events must be handled
while the work is pending: add that to the Red 2 test (a `SessionLogoff`
during a pending takeover is not lost).

### A-48 (TD-240): console connect and disconnect reach the rules; `windows-registry` is gated

Finding: "`SessionLogon`/`SessionLogoff`만 루프에 닿고 `ConsoleConnect`/`ConsoleDisconnect`
(빠른 사용자 전환)는 버려지며 콘솔 세션을 폴링하지도 않아, ADR 0018의 '다른
사용자가 콘솔을 쓰면 CamillaDSP를 멈추고 `other_user`'는 다음 요청이나 로그온
이벤트에서야 일어납니다. 정책 게이트의 `OS_PACKAGES`에 ADR 0018이 `secs-sys` 전용으로
적은 `windows-registry`가 빠져 다른 크레이트가 더해도 모릅니다."

Seams: `ServiceEvent` (new `ConsoleConnect(session)`/`ConsoleDisconnect(session)`
from `SessionChangeReason::ConsoleConnect`/`ConsoleDisconnect` in the
`windows_service` handler) → `transitions::Input::Session { event: SessionEvent::ConsoleConnect, .. }`;
and `OS_PACKAGES` in the policy gate.
Red 1 (`apps/secs2r-host`, pure): with an owner managing CamillaDSP, a
`ConsoleConnect` for another session → `decide` yields the stop effect and
the `other_user` state ADR 0018 describes; a `ConsoleConnect` for the owner's
own session after `other_user` → resume. Mirror the existing
`SessionLogon`/`SessionLogoff` tests.
Red 2 (`secs-policy`): `OS_PACKAGES` contains `"windows-registry"` (the test
that lists the array: assert it; and the gate itself must still pass on the
workspace, since only `secs-sys` depends on it).
Green: the two events wired from the service control handler through
`ServiceEvent` to `Input::Session`; `"windows-registry"` added to the array
(16 → 17; keep the array sorted the way it is). A real check: with the
service built, `cargo test -p secs2r-host` passes and, if you can run the
service in a test session, say what you observed; otherwise say that the
event mapping is unit-tested only.

## Report (in your final message, nothing else)

For each item, in order: the commit hash on `audit/186-host`, the Red test
names and what they assert, the Green summary in one line, the gate output
summary (fmt, both clippy runs, the test counts for secs-sys, secs2r-host
and secs-policy, the workspace run), the unsafe count (must still be 3), and
for A-22 the quoted output of the real-pipe test. Then anything blocked or
reclassified with why (a missing safe Windows API, a `secs-types` variant
the main session must add), and `git log --oneline main..audit/186-host`.
