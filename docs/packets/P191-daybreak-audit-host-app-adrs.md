# P191 (Daybreak): audit #186, the managed-service and updater ADRs

Tree: `E:\wt\audit-host`, a git worktree that Claude re-points to branch
`audit/186-host-adr`, created from the then-current `fix/audit-186` (after the
Phase 1 packets P183..P187 and the engine/service ADR packets P188/P189 have
been rebased into it). Check `git log -1` and `git status` before the first
edit; if the branch is not `audit/186-host-adr` or the tree is dirty, stop and
report. Every cargo command runs with `CARGO_TARGET_DIR=C:\cargo-target\audit-host`.
This machine is Windows 11: the Windows-only crates build and their tests run
for real here. Commit on that branch, one commit per step below, in order.
Do not push, do not rebase, do not spawn sub-agents, do not touch `audit-state.md`.

This packet builds on commits that are already on the base: P187's A-21
(TD-188: watcher failure handled, non-zero exit code), A-22 (TD-190 pipe
instance and SYSTEM check), A-23 (TD-191 `Accepted` and loop work), A-48
(TD-240 console events) and P183's A-12 (TD-164 update failure logging through
the `AppLog`). Read them first (`git log --oneline fix/audit-186 -- apps/secs2r-host
crates/secs-sys apps/secs2r/src/update.rs`). ADR 0057 and 0058 generalise what
those commits did; keep their tests passing or replace each with the ADR's
test that covers it (say which).

Allowed files:

- `apps/secs2r-host/**` (`main.rs`, `rules.rs`, `transitions.rs`, a new
  `files.rs`, tests);
- `crates/secs-sys/src/host.rs`, `crates/secs-sys/src/lib.rs` (the `unsafe`
  count of `secs-sys` must not grow: 3 is the budget, CLAUDE.md);
- `crates/secs-policy/tests/gates.rs` only if an `OS_PACKAGES` line is still
  missing `windows-registry` after P187;
- `crates/secs-service/src/update_machine.rs` (new) and the `pub mod
  update_machine;` line in `crates/secs-service/src/lib.rs`;
- `apps/secs2r/src/update.rs` and its tests, `apps/secs2r/src/main.rs` only
  for the `TauriUpdates::new` call if its signature changes;
- `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md` (the '관리 전환'
  paragraph) and `docs/adr/0020-automatic-updates.md` (one result line);
- `features.toml`: register every new test in the matching `[[feature]]`
  entry (`host.*`, `update.*`) or add one in the same style;
  `python3 .github/scripts/check_feature_tests.py` must accept it.

Forbidden: `crates/secs-types/**` (if a new variant is unavoidable, stop and
report), `apps/secs2r/ui/**`, `apps/secs2r-host-shell/**`, `CHANGELOG.md`.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0057-managed-service-transitions-in-decide.md`,
`docs/adr/0058-updater-state-machine-in-core.md`, ADR 0018 ('관리 전환'), ADR
0020, `apps/secs2r-host/src/transitions.rs` (`Input` 17, `Effect` 42, `State`
59, `Platform` 98, `decide` 132, `drive` 286, the 15 pure tests at 421-883),
`apps/secs2r-host/src/main.rs` (`service_loop` 389-690: child exit 587-637,
`RestartTick` 639-657, logon/logoff 472-541, the direct assignments at 483,
511, 538-539, 590-592, 607-630; `handle_request` 796-867; `launch` 1133-1257),
`apps/secs2r-host/src/rules.rs` (`Core` 484, `Core::handle` 550, the I/O
functions 231-700, `ProcessEvent`/`ProcessAction`/`ProcessRules` 742-808,
the copy's test at 1158), `crates/secs-sys/src/host.rs` (`ServiceEvent` 1021,
the session-change arm 1086-1093, `service_main` 1055-1068),
`apps/secs2r/src/update.rs` (`Inner` 18, `check_locked` 225, `download_work`
297, `set_policy` 490, the six tests at 524-632), `crates/secs-service/src/lib.rs:243-256`
(`UpdateAdapter`), the update types in `crates/secs-types/src/ipc.rs:2663-2780`.

## Why

The maintainer decided (2026-11-06) to implement every architecture candidate
of audit #186 behind an ADR. These two candidates deepen the managed service's
transition module and move the updater's policy rules into a pure state
machine. Each step is a Category A "deepening" item of
`/115dkk-skills:resolve-audit`: tests at the deepened interface first (pure
`decide`/`step` tests that run on every OS), the behaviour moves behind it,
and the shallow tests the new ones cover are deleted in the same commit.
Expected values come from the ADR sentences and literals, never from the code
under test.

## The protocol for every step

As in P183/P184: announce; Red (the named tests fail on their assertions, not
on compilation; stub first); Green (move the behaviour); Tidy (only this
step's lines); gate (`cargo fmt --all -- --check`,
`cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`,
`cargo test -p <touched crates> --locked`; for the host also
`cargo clippy --workspace --exclude secs2r --exclude secs2r-host-shell --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings`
once at the end; every fifth commit `cargo test --workspace --locked` in the
foreground); commit with `git add <files>` and

```
refactor(audit): X-n <한국어 한 문장> (#186 A<n>, ADR 00NN)

<깊어진 interface에서 어떤 시험을 먼저 썼고 무엇이 그 뒤로 옮겨 갔는지 한두 문장.>

Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>
```

(`fix(audit): A-34 …` for the TD-204 step; 하십시오체, no em-dash, no
program-as-subject verbs, "추가하다" not "더하다"). Rule 7 of P183 applies to
blocked steps.

## Steps

### X-17 · ADR 0058 · the updater state machine in the service crate

Step 1 · A-34 · TD-204 (a Category A finding not yet done anywhere): the
download size cap. Finding: "업데이터가 설치본 전체를 서명 검사 전에
`Vec<u8>`(`inner.bytes`)로 받으며 크기 상한이 없습니다. 어긋난 끝점
(`SECS2R_UPDATE_ENDPOINT` 덮어쓰기는 `http://`도 받음)은 설치는 못 해도
메모리를 다 쓰게 할 수 있습니다." Do it inside the machine of step 2 if you
prefer one commit less: the ADR's `DownloadProgress { received, total }` rule
with `MAX_DOWNLOAD_BYTES = 512 * 1024 * 1024`. Seam: `UpdateMachine::step`.
Red test `download_past_the_cap_cancels_and_fails_before_buffering_td204` in
`crates/secs-service/src/update_machine.rs` tests: `Downloading` state,
`DownloadProgress { received: 512 MiB + 1, total: None }` → commands
`[Cancel]` and status `Failed { reason: "The update is larger than allowed" }`
(literal from the ADR); a later `DownloadResult` with the old generation is
ignored. If you do it before the machine exists, put the cap into
`download_work`'s progress callback with a unit test on a pure
`exceeds_cap(received, total) -> bool` and move it in step 2.

Commit: `fix(audit): A-34 업데이트 내려받기에 크기 상한을 둡니다 (#186 TD-204)`.

Step 2: `crates/secs-service/src/update_machine.rs` exactly as ADR 0058
writes it (`UpdateMachine::{new, status, step}`, `FoundRelease`, `UpdateEvent`,
`UpdateFailure`, `UpdateCommand`, `LogLevel` from P183's log seam). Red tests,
one per ADR sentence, in the module's tests (expected values from the ADR, not
from `update.rs`): `auto_policy_downloads_a_found_release_at_once`,
`notify_policy_only_announces_a_found_release`,
`portable_build_reports_the_release_url`, `tick_is_ignored_while_checking_downloading_ready_or_installing`,
`late_check_result_with_an_old_generation_is_ignored`,
`set_policy_off_cancels_removes_pending_and_goes_off`,
`notify_while_downloading_cancels_back_to_available`,
`auto_while_available_starts_the_download`,
`exit_installs_only_a_ready_on_exit_release` (OnExit → `Install { restart: false }`;
OnRequest → nothing), `failures_keep_the_user_sentence_and_log_the_cause_td164`
(`Failed { reason }` unchanged and a `Log { level: Error, line }` containing
the cause string), plus step 1's test. The adapter does not use the machine
yet; behaviour is unchanged.

Commit: `refactor(audit): X-17 업데이터의 정책 규칙을 순수 UpdateMachine에 둡니다 (#186 A17, ADR 0058)`.

Step 3: `TauriUpdates` becomes the command executor: `Mutex<UpdateMachine>`,
`step` on every `UpdateAdapter` call and every async result, commands
executed as ADR 0058's table (`Check` → `updater_builder().check()` with the
generation; `Download` → `Update::download` with `DownloadProgress`/`DownloadResult`;
`Install`; `SavePending`/`RemovePending` → the `update.json` helpers
`atomic_pending`/`remove_pending`; `Log` → `AppLog::line`; `Cancel` → today's
`cancel_locked`). The 6-hour loop sends `Tick` only. Delete `check_locked`'s
and `download_work`'s policy branches, `set_policy`'s four branches,
`require_download`/`require_restart` and the six app tests they fed
(`update_policy_and_portable_decisions`, `update_actions_reject_wrong_phases`,
`update_notice_matches_running_version_and_tracks_skipped_releases` move to
the machine's tests with the same literals; `updater_config_requires_signed_version`
and `update_endpoint_override_preserves_nonempty_address` stay). The app
crate's remaining test records `update_transitions.json` for ADR 0052 (the
policy × phase → next-phase table) into `apps/secs2r/ui/src/ipc/recorded/`
(allowed for this file only) with the same `SECS_RECORD=1` convention as
`crates/secs-service/tests/recorded.rs`. Keep P183's A-12 logging test green
through the `Log` command. `mock.ts` gets no edit here (P192); do not widen
its branches.

Commit: `refactor(audit): X-17 TauriUpdates는 기계의 명령만 실행합니다 (#186 A17, ADR 0058)`.

### X-16 · ADR 0057 · one `decide` for the managed service

Step 4: `State` absorbs `rules::Supervisor` as `restart: Supervisor` and gets
`restart_due: Option<Instant>` (replacing `restart_scheduled` and `main.rs`'s
`restart_at`); `running_child_session` becomes a read-only accessor; `Input`
gains `ChildExited { pid, exit_code, now }` and `PortBusy { reason, detail, now }`;
`Effect` gains `SetCamillaState(ManagedProcessState)` and `CountRestart`
with `Platform::set_camilla_state` and `count_restart`. Red tests in
`transitions.rs` (pure; literals from the ADR):
`one_child_exit_schedules_the_first_backoff_and_counts_it` (`restart_due` =
now + the policy's first delay; effects contain `SetCamillaState(BackingOff { until })`,
`CountRestart`, an `exit` log), `five_exits_within_five_minutes_give_up`
(`SetCamillaState(GivenUp { reason: "five exits within five minutes" })`,
`given_up` log, `restart_due == None`), `exit_without_a_session_schedules_nothing`,
`restart_tick_after_restart_due_launches`, `owner_logoff_and_stop_managing_clear_restart_due_and_stop`,
`port_busy_backs_off_and_gives_up_on_the_fifth`. `main.rs` still runs the old
code in this step (both compile).

Commit: `refactor(audit): X-16 State가 재시작 정책과 자식 종료 입력을 가집니다 (#186 A16, ADR 0057)`.

Step 5: `service_loop` routes child exit, restart and port backoff through
`decide` (`Input::ChildExited`, `Input::PortBusy`, `state.restart_due` read
for the timer); `ProcessRules`, `ProcessEvent`, `ProcessAction` deleted;
`launch` returns the port-busy fact instead of backing off itself. Gate: the
pure tests of step 4 plus `cargo test -p secs2r-host` and a manual service
run on this machine where feasible (build `secs2r-host`, run its pipe
round-trip tests; quote the output).

Commit: `refactor(audit): X-16 자식 종료와 재시작을 decide가 정합니다 (#186 A16, ADR 0057)`.

Step 6: logon/logoff/stop-managing stop assigning `transition_state` and
`core.camilla` directly; `Core.camilla` becomes a projection that only
`drive` updates through `SetCamillaState`/`ClearCamillaStatus`. Red:
`fake_platform_records_no_session_after_owner_logoff` (the `FakePlatform` in
`transitions.rs` tests records `set_camilla_state(NoSession)` and the test
asserts it instead of `main.rs` state).

Commit: `refactor(audit): X-16 Core.camilla는 drive만 갱신하는 투영이 됩니다 (#186 A16, ADR 0057)`.

Step 7: `Input::WatcherFailed { detail }` and the `drive` rule that a
`ReplaceWatcher` failure fills `state.eqapo_error`, logs `watcher_failed` and
continues; `secs-sys`'s `exit_code_for(result)` already exists from P187's
A-21: keep it and route the loop's `Err` through it. Red:
`watcher_failure_keeps_the_loop_and_later_effects` (`replace_watcher` returns
`Err` → `eqapo_error` set, `watcher_failed` log, a following `LaunchChild`
still executed). Replace P187's A-21 test only if this one covers it; say
which.

Commit: `refactor(audit): X-16 감시자 실패는 상태로 남고 루프는 계속됩니다 (#186 A16, ADR 0057)`.

Step 8: `Input::Console { event: ConsoleEvent, session_id, owner_matches }`
from `secs-sys`'s `ServiceEvent::{ConsoleConnect, ConsoleDisconnect}` (P187's
A-48 added the events; here they go through `decide`). Red:
`owner_console_disconnect_stops_the_child_and_marks_no_session`,
`owner_console_connect_launches_and_repairs`, `other_users_console_connect_marks_other_user`.
Manual check on this PC: `tsdiscon` from the owner session and reconnect, the
service log shows the two transitions; quote the log lines in the report (if
`tsdiscon` is not available on this edition, say so).

Commit: `refactor(audit): X-16 콘솔 세션 전환도 decide가 다룹니다 (#186 A16, ADR 0057)`.

Step 9: `rules.rs` keeps only pure rules; `load_state`, `save_state`,
`load_logs`, `append_log`, `read_bounded`, `write_staged`,
`replace_host_executable` move to `files.rs` with the same names; `Core::handle`'s
`SetManaged`/`StopManaging` arms answer `ErrorCode::Unsupported` like
`UpdateSelf`, and `request_handling_merges_targets_and_stop_clears_them` is
deleted (the merge rule is `transitions::managed_after_update` with its two
tests). ADR 0018's '관리 전환' paragraph now points at ADR 0057 and names child
exit, backoff, give-up and console switches as `decide`'s; ADR 0020 gets one
result line pointing at ADR 0058.

Commit: `refactor(audit): X-16 rules.rs는 순수 규칙만 두고 파일 I/O는 files.rs로 갑니다 (#186 A16, ADR 0057)`.

## Verification (foreground; do not end the turn before they finish)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --exclude secs2r-host-shell --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --workspace --locked
cargo test -p secs-sys -p secs2r-host --locked
python3 .github/scripts/check_feature_tests.py
grep -c "unsafe" crates/secs-sys/src/*.rs      # the policy gate counts tokens; it must not exceed the budget
git log --oneline fix/audit-186..audit/186-host-adr
```

`check_feature_tests.py` on Windows reports two names it cannot list here
(`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`); anything else is yours.

## Report

Per step: the commit hash, the Red tests and the assertion each failed on,
what moved behind the interface, which shallow tests were deleted or replaced
(and which P187/P183 test each replacement covers), gate tails, the manual
run outputs (pipe tests, console switch). Then: steps blocked, seams or
signatures you changed from the ADR and why, and the final `git log --oneline`
of the branch.
