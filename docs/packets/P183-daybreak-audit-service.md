# P183 (Daybreak): audit #186, Category A findings in the service crate

Tree: `E:\wt\audit-svc`, a git worktree on branch `audit/186-svc` (created by
Claude from `fix/audit-186`). Work only there. Every cargo command runs with
`CARGO_TARGET_DIR=C:\cargo-target\audit-svc` set in the environment (the main
checkout's `target/` is not yours). Commit on that branch, one commit per
finding, in the order below. Do not push, do not rebase, do not spawn
sub-agents, do not touch `audit-state.md`.

Allowed files:

- `crates/secs-service/**` (source and tests);
- `crates/secs-project/src/record.rs` and its tests, for A-20 only;
- `crates/secs-local-api/src/lib.rs`, for the log callback of A-3 only;
- `apps/secs2r/src/main.rs`, `apps/secs2r/src/applog.rs`, `apps/secs2r/src/update.rs`;
- `crates/secs-rew/src/client.rs` and its tests, for the subscribe failure of A-41 only;
- `features.toml`: add every new test name to the `tests = [...]` list of the
  `[[feature]]` entry that already covers that behaviour (search the file for
  the IPC method or the crate), or add an entry in the same style when none
  fits. `python3 .github/scripts/check_feature_tests.py` must accept it.

Forbidden: `crates/secs-types/**` (another worker regenerates bindings
there), `crates/secs-engine/**`, `crates/secs-graph/**`, `crates/secs-filters/**`,
`apps/secs2r/ui/**`, `CHANGELOG.md`, `docs/**` (Claude writes those).

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/ARCHITECTURE.md` (sections on
the service and jobs), `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`,
`docs/adr/0015-measurement-sources.md` if present, `crates/secs-service/src/lib.rs`
(`Deps`, `Service::call`, `lock()`), `crates/secs-service/src/jobs.rs`,
`crates/secs-service/tests/service.rs` lines 1-120 (the helpers `service()`,
`deterministic_service()`, `Events`, `temp_root`), `crates/secs-service/tests/apply.rs`
lines 1-120, and the finding texts quoted below.

## Why

The monthly audit (issue #186) found behaviour defects in the service crate.
Each one is a Category A item of `/115dkk-skills:resolve-audit`: the change is
observable, so it is made test-first at an agreed seam. The seam for every
item is fixed below; a test at another seam is not accepted. Expected values
in a test come from literals or the finding's text, never from calling the
code under test to compute them.

## The protocol for every item

1. Announce the item in the report (`── A-n: … ──`).
2. **Red.** Write the test named below in the file named below. Run it.
   It must fail, and fail for the reason the finding describes (quote the
   assertion in the report). A test that fails to compile does not count as
   Red: stub the new function or variant first so the test compiles and then
   fails on the assertion.
3. **Green.** The smallest change that makes the test pass.
4. **Tidy.** Only the lines this item wrote (names, leftovers). Do not clean
   up neighbouring code; that is Phase 2's business.
5. Gate: `cargo fmt --all -- --check`,
   `cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`,
   `cargo test -p <touched crates> --locked`. Every fifth commit, and at the
   end: `cargo test --workspace --locked` (long; run it in the foreground and
   wait).
6. Commit with `git add <the files you touched>` (never `git add -A`; the
   tree has generated files whose line endings differ) and this message
   shape, Korean, subject under 72 characters where possible:

   ```
   fix(audit): A-n <한국어 한 문장> (#186 TD-xxx)

   <Red 시험이 무엇을 증명하는지 한두 문장. 바뀐 동작 한 문장.>

   Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>
   ```

   Korean sentences follow the repository's writing rules: 하십시오체
   (-ㅂ니다), no em-dash, no program-as-subject verbs such as 묻다/적다/넘기다
   (use 표시합니다/기록합니다/전달합니다), "더하다" is "추가하다".
7. If an item turns out to need a change that another worker owns (a
   `secs-types` variant, an engine error), stop that item, leave the Red test
   in place but `#[ignore]`d with a comment naming the dependency, commit
   nothing for it, and report it as blocked.

## Shared interface for A-3 (fixed; A-12, A-13 and A-30 build on it)

```rust
// crates/secs-service/src/lib.rs
/// Severity of a line the service writes to the embedder's log (TD-151).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel { Warn, Error }

/// Receives one line: level, a short static scope ("apply", "export", "session", "cache", "local_api"), the text.
pub type LogSink = Arc<dyn Fn(LogLevel, &'static str, &str) + Send + Sync>;

pub struct Deps {
    // ...existing fields...
    /// Where failures that do not fail the request are reported. The default writes
    /// `[warn|error] scope: line` to stderr; `apps/secs2r` routes it to the app log.
    pub log: LogSink,
}
```

`Service` gets `pub(crate) fn log(&self, level: LogLevel, scope: &'static str, line: &str)`.
`Deps::default()` keeps the stderr behaviour. `secs_local_api::ServerConfig`
gets `pub log: secs_service::LogSink` and uses it at `lib.rs:241`. In
`apps/secs2r/src/main.rs` the sink maps `LogLevel::Warn` to `applog::Level`'s
warning level and `Error` to its error level and calls `AppLog::line(level, scope, text)`.
Tests build a sink that pushes `(level, scope, line)` into a `Mutex<Vec<_>>`.

## Items

### A-3 · TD-151 · service failures only reach `eprintln!`

Finding: "보존 기록, 표식, 세대 정리, 색인, 곁 mdat, 로컬 API 서버의 실패를
`eprintln!`로만 적습니다. 출하 앱은 `windows_subsystem = "windows"`라 stderr가
없고, 서비스에는 로그 seam이 없어 … ADR 0018의 '이 실패는 세대 이름과 함께
오류로 기록합니다'가 Windows 릴리스 빌드에서는 아무것도 남기지 않습니다.
`let _ = remove_stage(&stage)`(138, 280)의 staging 폴더 누수도 같은 이유로
보이지 않습니다." Recommendation: a log callback in `Deps`, default stderr,
the twenty sites use it, `apps/secs2r` connects it to `AppLog`, the local
API `ServerConfig` takes the same callback.

Sites (current line numbers on `fix/audit-186`): `apply.rs:192, 569, 630,
649, 664, 681, 688`; `design_export.rs:342, 358, 372, 383, 418, 439, 451, 588`
(the three `eprintln!` inside `#[cfg(test)]` skip messages at 811, 876, 939
stay); `session.rs:198`; `cache.rs:583`; `secs-local-api/src/lib.rs:241`.
Also the two `let _ = remove_stage(&stage)` at `apply.rs:138, 280`: log the
error with level `Warn` and scope `"apply"` when removal fails.

Seam: `Deps.log`. Red test `retire_failure_reaches_the_log_sink_td151` in
`crates/secs-service/tests/apply.rs`: apply once so a generation exists, make
the retire step fail (make the previous generation's folder read-only or
replace it with a file, whichever the existing tests use to provoke
`could not retire old applied generations`), apply again, assert the sink
received exactly one `(LogLevel::Error, "apply", line)` whose `line`
contains the generation name and the word `retire` (literal expectations),
and that the apply result is still success. A second test
`side_mdat_failure_reaches_the_log_sink_td151` in `tests/service.rs`: make
the side mdat write fail (a file in the way of the mdat path), save, assert
a `(Warn, "session", …)` line and that `project_save` still succeeds (A-2
later turns this into a job; keep this test and let A-2 update its
expectation to a job error).

Green: the field, the method, the sites. Tidy: nothing else.

Commit subject: `fix(audit): A-3 서비스의 실패 기록이 로그 seam을 거칩니다 (#186 TD-151)`.

### A-2 · TD-149 · `project_save` builds the side mdat under the session lock

Finding: "`project_save`마다 `write_side_mdat`가 세션 잠금 안에서 모든 WAV를
다시 읽어 f64로 늘리고(`original_measurements`, 메모리 예산 없음) mdat 전체를
메모리에 만들어 씁니다. 그동안 다른 요청(로컬 API `status`, 잡 커밋, 그래프
읽기)이 모두 멈추고, 실패는 `eprintln!`(198)뿐이며 저장은 성공으로 끝납니다.
TD-130이 내보내기에 적용한 방식이 저장에는 적용되지 않았습니다."
Recommendation: take an `OriginalSnapshot` under the lock, build the mdat in
an `Export`-kind job (same shape as TD-130) with the job's budget and cancel
token, report failure as a job error, then delete `original_measurements`
(`session.rs:726-769`, a copy of `OriginalSnapshot::load`).

Decision (open question 1): upstream writes the side mdat on every save
(`src/secs/wizard/window.py:907`), so keep writing it; only move it off the
lock. Use `JobKind::Export` (no new wire types; `secs-types` is not yours).
`ProjectSave` still answers `ProjectInfo` at once; the job's `Finished`
event carries success or the error.

Seam: `Service::call(Request::ProjectSave)` plus the job events. Red test
`project_save_returns_before_the_side_mdat_exists_and_a_job_writes_it_td149`
in `tests/service.rs`: a project with sample measurements; call save; assert
the side mdat path does not exist in the response's moment (check right
after `call` returns, before waiting on events), then wait for a `Finished`
event of an `Export` job and assert the file exists and parses with
`secs_mdat::load` (the loader the existing export tests use). Second test
`side_mdat_failure_is_a_job_error_not_a_silent_save_td149`: put a directory
in the mdat's path, save, assert save succeeds and the job finishes with an
error whose detail names the path. Update A-3's
`side_mdat_failure_reaches_the_log_sink_td151` to the new expectation
(the job error replaces the log line) in the same commit.

Green: the job; delete `original_measurements` only after
`OriginalSnapshot::load` covers its callers. Tidy: within the moved code.

Commit subject: `fix(audit): A-2 곁 mdat을 세션 잠금 밖의 잡에서 만들어 저장합니다 (#186 TD-149)`.

### A-5 · TD-153 · apply jobs rebuild `designs.json` in foreign config folders

Finding: "이름 없는 명시적 내보내기는 고른 폴더의 부모에 설계 색인을 다시
만드는데, 적용은 staging 폴더를 그 폴더로 넘깁니다. 그래서 CamillaDSP 적용마다
`<앱 데이터>\applied\designs.json`이, Equalizer APO 적용마다 Equalizer APO 설정
루트(`ConfigPath\`)에 `designs.json`이 생기고, 둘 다 잠시 뒤 `deploy::install`이
이름을 바꾸는 `.SECS2R.stage-<pid>-<n>` 폴더를 가리킵니다." Recommendation:
pass a flag into `start_export_with` so `job_kind == JobKind::Apply` skips
`rebuild_index*`; the test at `tests/apply.rs:1768` checks that
`config_root` holds only `config.txt`, the backup and `SECS2R`.

Seam: the apply job's file effects. Red test
`eqapo_apply_leaves_no_design_index_in_the_config_root_td153` in
`tests/apply.rs` (next to the test at 1768, reusing its setup): after an
Equalizer APO apply, list `config_root` and assert the sorted names equal
the literal list `["SECS2R", "config.txt", "<backup name the existing test
asserts>"]` and that no `designs.json` exists under `config_root` or under
`<app data>/applied`. A CamillaDSP twin
`camilla_apply_leaves_no_design_index_in_applied_td153` if the suite has a
CamillaDSP apply path with a fake transport.

Commit subject: `fix(audit): A-5 적용 잡은 설계 색인을 만들지 않습니다 (#186 TD-153)`.

### A-6 · TD-154 · `io_error` classifies by English message text

Finding: "`io_error`가 `PermissionDenied`와 `ApplyFailed`를 영어 메시지의 부분
문자열("permission denied", "access is denied")로 가릅니다. … 한국어 Windows에서는
"액세스가 거부되었습니다"라서 … 일반 `ApplyFailed`로 보고되고, 화면의 권한
안내를 건너뜁니다." Recommendation: take `&io::Error`, check
`kind() == PermissionDenied` first, keep the string check as a fallback;
test with a non-English `PermissionDenied` error.

Seam: `apply::io_error`. Red test `permission_denied_is_classified_by_kind_not_by_english_text_td154`
in `crates/secs-service/src/apply.rs` `#[cfg(test)]` (unit test):
`io::Error::new(io::ErrorKind::PermissionDenied, "액세스가 거부되었습니다.")`
must map to `ErrorCode::PermissionDenied`; an `Other` error with the text
"permission denied" still maps to `PermissionDenied` (fallback kept); an
`Other` error with "disk full" maps to `ErrorCode::ApplyFailed`.

Commit subject: `fix(audit): A-6 권한 거부를 io::ErrorKind로 판정합니다 (#186 TD-154)`.

### A-12 · TD-164 · update failures drop their cause

Finding: "업데이트 확인·내려받기 실패가 고정 문구("Could not check for updates")로
줄고 원인은 버려집니다(`map_err(|_| …)`, `Err(_)`). 앱 로그에도 닿지 않아
'업데이트가 안 된다'는 보고를 진단할 수 없습니다." Recommendation: keep
`UpdatePhase::Failed.reason` for the screen and log the original error at
the three sites with `AppLog::line(Error, "update", …)`.

Seam: the app log. `apps/secs2r/src/update.rs` is a Tauri-app module; find
how its existing tests (around lines 500-540, the `requireSignedVersion`
test) construct the updater and reuse that. Red test
`check_failure_logs_its_cause_td164`: an endpoint that returns an HTTP 500 or
an unreachable port; after the check fails, the log (the `AppLog` test
double or a captured sink) holds one error line with area `"update"` whose
text contains the status code or the connection error text. If the module
has no injectable log, add a small `log: Arc<dyn Fn(Level, &str, &str)>`
field to `TauriUpdates::new` and wire it from `main.rs` to `AppLog::line`.

Commit subject: `fix(audit): A-12 업데이트 실패의 원인을 앱 로그에 남깁니다 (#186 TD-164)`.

### A-13 · TD-165 · app log omits `params`; cancel level inconsistent

Finding: "잡 실패와 요청 오류를 `error_code`와 `detail`만으로 적고 원인을 담은
`params`(`reason=rc_agreement_positions`, `stage=mimo`, `field=…`)는
빼놓습니다. 또 본문이 `Err(Cancelled)`를 돌려준 취소 잡은 ERROR로, 취소 뒤
`Ok`로 끝난 잡은 WARN으로 적힙니다." Recommendation: reuse the
`JobEvent::Log` format (`applog.rs:181-185`) to append `params` as
`key=value`, choose the level from `status` rather than `error.is_some()`,
and give `Cancelled` one `error` shape in `jobs.rs` `finish`.

Seam: `applog` formatting and `jobs::finish`. Red tests in
`apps/secs2r/src/applog.rs` `#[cfg(test)]`:
`failed_job_line_carries_params_as_key_value_td165` (an `IpcError` with
`params {"reason":"rc_agreement_positions","stage":"mimo"}` → the line
contains `reason=rc_agreement_positions stage=mimo`, keys sorted) and
`cancelled_job_is_logged_as_warning_whatever_its_error_shape_td165` (a
`Finished` with `status: Cancelled` and an `error: Some(Cancelled)` → WARN).
And in `crates/secs-service` `jobs.rs` unit tests:
`a_body_that_returns_cancelled_finishes_like_an_external_cancel_td165`
(both paths produce the same `JobStatus::Cancelled` + the same `error`
shape, compared with literals).

Commit subject: `fix(audit): A-13 앱 로그가 params를 기록하고 취소 수준을 통일합니다 (#186 TD-165)`.

### A-14 · TD-169 · `load_app_settings` silently resets damaged or newer files

Finding: "`load_app_settings`가 손상됐거나 상한을 넘었거나 판이 더 새로운 파일을
아무 알림 없이 기본값으로 바꾸고, 다음 `app_settings_set`이 그 파일을 덮어써
최근 프로젝트와 정책을 조용히 잃습니다." Recommendation: return
`(AppSettings, Option<Notice>)`, carry it in `BootstrapInfo` or the app log,
and never overwrite a file whose `version` is newer.

`BootstrapInfo` lives in `secs-types` (not yours), so the notice goes
through the log seam of A-3 with level `Warn`, scope `"settings"`, and the
newer-version rule is enforced in `app_settings_set`.

Seam: `Service::call(AppSettingsGet / AppSettingsSet)` with a damaged file.
Red tests in `tests/service.rs` (near `app_settings_are_versioned_bounded_and_capped`):
`damaged_settings_file_is_reported_and_not_overwritten_until_set_td169`
(write `{not json` to the settings path; `app_settings_get` answers the
defaults and the sink holds a `Warn`/`"settings"` line naming the path) and
`newer_settings_file_is_never_overwritten_td169` (write a valid file with
`version` = current + 1 and a recognisable value; `app_settings_set` answers
an `IpcError` with code `Unsupported`, or whichever code the settings module
already uses for version problems, with `params.reason = "newer_version"`;
the file bytes are unchanged).

Commit subject: `fix(audit): A-14 손상되거나 새 판인 설정 파일을 알리고 덮어쓰지 않습니다 (#186 TD-169)`.

### A-15 · TD-170 · REW cal file read without a bound, once per role

Finding: "REW가 `/audio/input-cal`로 알려 준 경로를 `fs::read(&path).ok()`로
상한도 해석도 없이 읽고, 역할마다 다시 읽습니다 … 다른 두 읽기는 4 MiB와
256 MiB 상한을 둡니다." Recommendation: read once per position with a
`MAX_CALIBRATION_BYTES` bound and a `Notice` on failure; one
`read_bounded(path, limit, what)` for the three idioms
(`measurement_file.rs:66-76`, `mic_calibration.rs:223-253`,
`secs-audio/src/calibration.rs:100`; the last file is not yours, leave it).

Seam: the REW measurer path with `FakeRew`. Red test
`oversized_input_cal_file_is_refused_with_a_notice_and_read_once_td170` in
`tests/measure_ipc.rs`: the fake announces a cal path; the file is
`MAX_CALIBRATION_BYTES + 1` bytes; a two-role position records both roles,
the result carries one `Notice` with code `measurement_calibration_too_large`
(literal) and `params.path`, and a read counter (wrap the path in a test
file whose reads you can count, or assert through the fake's call log if
it exposes file reads) shows one read.

Commit subject: `fix(audit): A-15 REW 보정 파일을 상한 아래에서 위치마다 한 번 읽습니다 (#186 TD-170)`.

### A-16 · TD-171 · in-role progress never reaches the screen

Finding: "`start_measure`는 `measure_recording`만 부르고 그것은 `progress = None`을
넘기므로 화면은 역할이 끝날 때의 `ctx.role_progress`만 받습니다. …
사용자는 역할 안의 진행(REW 퍼센트, 자체 측정의 OutputStarted/Drained)을
보지 못합니다." Recommendation: `measure_recording` passes a sink into
`measure_position` and maps stages to `ctx.report_progress`, or the
parameter is removed from the service trait; a service test asserts stage
events.

Seam: the measure job's `Progress` events. Red test
`mock_measure_reports_in_role_stages_td171` in `tests/measure_ipc.rs`: with
the mock measurer, record one position and assert the `Progress` events
include at least one event between the role's start and its end whose stage
text is one of the literal stage names the native measurer emits
(`OutputStarted`, `Drained`, or the mapped names you define in the packet
report), and that the final progress is `done == total`.

Commit subject: `fix(audit): A-16 측정 역할 안의 진행이 화면에 전달됩니다 (#186 TD-171)`.

### A-17 · TD-172 · REW `configure` repeated per role

Finding: "역할마다 따로 녹음하는 방식(F2)이 역할마다 `configure(plan)`을 다시
돌립니다. … 업스트림은 위치마다 한 번 설정합니다(`src/secs/rewapi/rew.py:813`).
`measure_ipc.rs:738-748`이 이 반복을 고정합니다." Recommendation: configure
once per position before the loop; per role only `set_output_channel`,
name and notes; refresh the recorded dialogue.

Seam: the `FakeRew` call log. Red test: change the expectation at
`tests/measure_ipc.rs:738-748` to one `configure` per position (the literal
count 1 for a two-role position) and name it
`rew_configures_once_per_position_td172`; keep the per-role
`set_output_channel` expectations. If a recorded transcript fixture pins the
old sequence, regenerate it with the repository's recorder and say so.

Commit subject: `fix(audit): A-17 REW를 위치마다 한 번만 설정합니다 (#186 TD-172)`.

### A-20 · TD-183 · skipped `.src.json` sidecars are silent

Finding: "너무 크거나 못 읽거나 예산이 바닥난 `.src.json` 사이드카를 조용히
버립니다. `load_source`가 모든 오류를 `None`으로 펴고,
`load_project_sources_within`의 `exhausted` 깃발은 함수 밖으로 나가지 않으며,
`MeasurementStore::open`은 보정 지문을 잃어 엔진이 그 IR을 보정 없는 것으로
다룹니다." Recommendation: `MigrationWarning`s `measurement.source.skipped`
and `measurement.source.budget` from `open`, a small result struct from
`load_project_sources_within`, and `session.rs:655/733` surface them.

Seam: `MeasurementStore::open` warnings and `ProjectInfo.migration_warnings`.
Red test `unreadable_sidecar_is_reported_as_a_migration_warning_td183` in
`crates/secs-project/src/record.rs` tests (next to the TD-112 tests at
1536/1578): a project whose `.src.json` is 17 MiB or malformed → `open`
returns warnings containing one with code `measurement.source.skipped`
(literal) and `params.path`; and `budget_exhaustion_is_reported_td183` with
the project budget forced small → `measurement.source.budget`. Then
`project_open_surfaces_sidecar_warnings_td183` in `tests/service.rs`
asserting the same codes in `ProjectInfo.migration_warnings`.

Commit subject: `fix(audit): A-20 건너뛴 사이드카를 이주 경고로 알립니다 (#186 TD-183)`.

### A-27 · TD-262 · invalid sidecar curve dropped but still counted as attached

Finding: "사이드카의 곡선이 `validate_point`에 걸리면 … `mic_curve`가
`from_points(points).ok()`로 조용히 `None`을 돌려줘 보정에서 빠지는데,
`attached_calibration`은 여전히 그 측정을 '붙음'으로 셉니다." Decision (open
question 16): emit a `Notice` naming role and position and exclude the entry
from `attached_calibration` so status and signature match what is applied.

Seam: `mic_calibration_status` (or the status request that reports attached
calibrations) and the calibration notices. Red test
`invalid_sidecar_curve_is_excluded_and_noticed_td262` in `tests/service.rs`
(or the mic calibration test file): attach a calibration, then rewrite the
sidecar's curve with an unsorted frequency pair; the status reports the
measurement as not attached and the calibration run's notices contain
`calibration_curve_invalid` (literal) with `params.role` and `params.position`.

Commit subject: `fix(audit): A-27 잘못된 사이드카 곡선을 알리고 붙음에서 뺍니다 (#186 TD-262)`.

### A-28 · TD-263 · imported IR samples unchecked

Finding: "가져오기와 REW 녹음이 IR 샘플을 검사하지 않습니다. Dirac IR의 scale이
`f32::MAX`×32767까지라 `:269`의 `as f32`에서 `inf`가 되고, `b64_be_f32`는 빈
배열이나 NaN을 받으며, `MeasurementStore::record`는 받은 그대로 씁니다."
Recommendation: check `samples` (non-empty, all finite) once in
`measurement_file::import` / `run_position` before `store.record` and return
`MeasureError::Input` naming the row.

Seam: `measurement_file::import` and the REW recording path. Red tests
`import_rejects_a_row_with_non_finite_samples_naming_it_td263` and
`import_rejects_an_empty_row_td263` in the mdat/dirac import tests
(`crates/secs-service/tests/measure_ipc.rs` or the import test file): an
mdat built with a NaN sample (use `secs_mdat::build` on a row with a NaN) →
the import answers an `IpcError` whose `params.row` names the row and whose
code is the existing input code; nothing is recorded.

Commit subject: `fix(audit): A-28 가져온 IR 샘플의 유한성과 길이를 검사합니다 (#186 TD-263)`.

### A-29 · TD-196 · `begin_commit` before writer resampling

Finding: "`begin_commit`이 `secs_camilla::export_with_options`·
`export_filter_text_with_options`·`export_eqapo_with_options`보다 앞에 있어,
작성기 안에서 모든 FIR을 출력 레이트로 리샘플하는 동안 취소할 수 없습니다.
컨볼루션(555)만 `prepare_…` 뒤에 커밋합니다." Recommendation: compute the
resampled taps first, `begin_commit` after, starting with CamillaDSP.

Seam: an export job cancelled during the writer's preparation. Red test
`camilla_export_cancelled_before_commit_writes_nothing_td196` in
`tests/service.rs`: a project with a 96 kHz output rate (so resampling
happens); cancel the export job as soon as its first `Progress` event
arrives; assert the job finishes `Cancelled` and the destination folder has
no files. The existing cancel tests show how to race the cancel.

Commit subject: `fix(audit): A-29 내보내기 커밋을 리샘플 뒤로 옮겨 취소할 수 있게 합니다 (#186 TD-196)`.

### A-30 · TD-198 · rollback failure path silent; backup folder left

Finding: "`replace_folder`에서 롤백 실패가 `let _`라 이전 묶음이
`.<name>.backup-<pid>-<n>` 아래 남는데 오류에 그 경로가 없고,
`remove_dir_all(backup)` 실패는 새 묶음을 이미 설치한 뒤에 내보내기를 실패로
만듭니다." Recommendation: name the backup path in the rollback error;
a backup-delete failure is a warning through the log seam, not a job
failure. (`secs-graph/src/files.rs` is not yours; only `design_export.rs`.)

Seam: the export job's error detail and the log sink. Red tests in
`crates/secs-service/src/design_export.rs` tests:
`rollback_failure_names_the_backup_folder_td198` (provoke a rollback whose
restore fails; the error detail contains the literal backup folder name)
and `backup_removal_failure_is_a_warning_not_a_job_error_td198` (make the
backup undeletable; the replace succeeds; the sink has one `Warn`/`"export"`
line with the path).

Commit subject: `fix(audit): A-30 롤백 실패 오류에 백업 경로를 표시하고 백업 삭제 실패는 경고로 둡니다 (#186 TD-198)`.

### A-31 · TD-199 · `config.txt` read as UTF-8 only

Finding: "`config.txt`를 `read_to_string`으로 읽어 UTF-8이 아닌 파일(ANSI의
비ASCII 주석, 일부 편집기의 UTF-16)이면 상태가 `IncludeState::Absent`로 보이고
적용·되돌리기가 "stream did not contain valid UTF-8"로 실패합니다."
Recommendation: detect a UTF-16 BOM and invalid UTF-8 and answer
`ApplyFailed` with `reason = config_encoding`; test with Latin-1 bytes.

Seam: `eqapo_status` / apply with a non-UTF-8 `config.txt`. Red tests in
`tests/apply.rs`: `latin1_config_txt_is_reported_as_an_encoding_problem_td199`
(bytes `b"# caf\xe9\r\nInclude: x\r\n"` → the status answers an `IpcError`
with `params.reason == "config_encoding"`, not `Absent`) and
`utf16_config_txt_is_reported_as_an_encoding_problem_td199` (a BOM `FF FE`
file). `crates/secs-apply/src/eqapo.rs` is not yours; keep the detection in
`apply.rs`.

Commit subject: `fix(audit): A-31 UTF-8이 아닌 config.txt를 인코딩 문제로 알립니다 (#186 TD-199)`.

### A-32 · TD-200 · registry paths accepted relative

Finding: "레지스트리의 `ConfigPath`·`InstallPath`를 `is_absolute()`나 디렉터리
여부 없이 받아, 상대 `ConfigPath`면 `deploy::install`이 프로세스 CWD 아래에
`SECS2R`을 만듭니다." Recommendation: check in `start_apply_eqapo` /
`eqapo_status` and answer `Unsupported` with `reason = eqapo_config_path`.

Seam: `eqapo_status` with a relative install. Red test
`relative_eqapo_config_path_is_unsupported_td200` in `tests/apply.rs`: the
`eqapo_install` dependency answers `config_path = "relative\\dir"`; the
status answers `ErrorCode::Unsupported` with `params.reason ==
"eqapo_config_path"` and no folder is created under the current directory.

Commit subject: `fix(audit): A-32 상대 경로인 Equalizer APO 설정 경로를 거부합니다 (#186 TD-200)`.

### A-38 · TD-210 · tutorial folder predictable in `/tmp`

Finding: "튜토리얼 폴더가 공유 `/tmp`의 예측 가능한 이름 `secs2r-tutorial-<pid>-<n>`이고
`remove_dir_all` 뒤 `create_dir_all`이라, 다른 로컬 사용자가 그 사이에 경로를
먼저 만들어 튜토리얼의 측정 파일을 받을 수 있습니다." Recommendation: create
it under `Deps::app_data_path` (per user) or with a random suffix and
`create_dir` (fails when present).

Seam: `project_tutorial` (the request that opens the tutorial). Red test
`tutorial_folder_lives_under_app_data_and_refuses_an_existing_path_td210`
in `tests/service.rs`: with `app_data_path` set to a temp root, open the
tutorial and assert its folder is under that root (`starts_with`), is not
under `std::env::temp_dir()`, and that a second tutorial in the same service
gets a different folder (no reuse of a pre-created path).

Commit subject: `fix(audit): A-38 튜토리얼 폴더를 사용자별 앱 데이터 아래에 만듭니다 (#186 TD-210)`.

### A-39 · TD-211 · relative export folder resolves against the CWD

Finding: "상대 `ExportParams::folder`가 프로세스 작업 디렉터리 기준으로
풀립니다. Tauri 앱은 운영체제가 띄운 곳이 CWD라 로컬 API와 바인딩이
뜻하지 않은 곳에 쓸 수 있습니다." Recommendation: reject with
`InvalidRequest` (`reason = relative_folder`) or resolve against the
project folder. Choose rejection.

Seam: `Request::Export`. Red test `relative_export_folder_is_rejected_td211`
in `tests/service.rs`: `folder = "out"` → `ErrorCode::InvalidRequest` with
`params.reason == "relative_folder"` and `params.field == "folder"`, before
any job starts (no `Started` event).

Commit subject: `fix(audit): A-39 상대 경로인 내보내기 폴더를 거부합니다 (#186 TD-211)`.

### A-41 · TD-215 · REW subscribe failure and sidecar-less recording silent

Finding: "`/measure/subscribe` 실패가 `own=false`로 꺼지고 신호가 없으며,
`from_rew_measurement(...).ok().flatten()`은 역할을 `.src.json` 없이 조용히
기록해 뒤에 곡선을 빼지 않습니다(ADR 0015)." Recommendation: both emit a
`Notice` like `measurement_low_snr`.

Seam: the measure job's notices with `FakeRew`. Red tests in
`tests/measure_ipc.rs`: `subscribe_failure_is_noticed_td215` (the fake
refuses `/measure/subscribe`; the position still records; the result
notices contain `measurement_subscribe_failed`, literal) and
`sidecar_build_failure_is_noticed_td215` (the fake answers a measurement
object `from_rew_measurement` cannot convert; the role is recorded and the
notice `measurement_source_missing` names the role). The only
`crates/secs-rew/src/client.rs` change allowed is making the subscribe
failure observable to the service (an error value instead of a silent
`own=false`), with its unit test.

Commit subject: `fix(audit): A-41 REW 구독 실패와 사이드카 없는 기록을 알립니다 (#186 TD-215)`.

### A-50 · TD-265 · a failed `play_pink` ends the level monitor

Finding: "REW 레벨 모니터에서 `play_pink`가 한 번 실패하면(`?`, 373) 재생 하나를
실패로 알리는 대신 모니터 잡 전체가 끝납니다. 자체 측정 루프는 재생 오류를
요청마다 처리합니다." Recommendation: send `level_play_end` with the error
and continue; the job ends only on cancel.

Seam: the level monitor job events. Red test
`rew_level_monitor_survives_a_failed_play_td265` in `tests/monitor.rs`:
the fake fails the first `play_pink`; assert a `level_play_end` event with
an error, a second play request succeeds afterwards, and the job is still
running until cancelled.

Commit subject: `fix(audit): A-50 핑크 재생 실패가 레벨 모니터를 끝내지 않습니다 (#186 TD-265)`.

### A-53 · TD-268 · an unparsable `mic_calibration` blocks every measure call

Finding: "저장된 `devices.mic_calibration` 글자가 더는 파싱되지 않으면
`MeasurementSettings::from_project`가 실패해 `measure_config_get`, 모든 측정
시작, 레벨 모니터가 보정을 이름 짓지 않는 "settings" 오류로 막힙니다.
`mic_calibration_get`은 바르게 알립니다." Recommendation: map the parse
error to a `mic_calibration` reason parameter (as `mic_calibration.rs:62-80`
does) or degrade to no calibration with a notice. Choose the reason
parameter (the user can then fix the setting).

Seam: `measure_config_get`. Red test
`unparsable_mic_calibration_setting_names_itself_td268` in
`tests/measure_ipc.rs`: a project whose saved `devices.mic_calibration` is
`"not a calibration"`; `measure_config_get` answers an `IpcError` with
`params.field == "mic_calibration"` (literal) instead of a bare settings
error, and `mic_calibration_get` still answers.

Commit subject: `fix(audit): A-53 깨진 마이크 보정 설정 오류가 필드 이름을 표시합니다 (#186 TD-268)`.

### A-37b · TD-208 (service part) · `ServiceError::Project(String)` flattens its cause

Finding (the part of TD-208 that lives in this crate): "`ServiceError::Project(String)`은
`ProjectError`와 `io::Error`를 글자로 펴서 `project_open`이 없는 파일·권한·상한
초과를 구별하지 못하고 `path` 매개변수도 없습니다." Recommendation: keep the
cause in the variant, map `io::ErrorKind::PermissionDenied` to
`ErrorCode::PermissionDenied`, a missing file and other I/O to the existing
`ProjectIo` code, and attach `params.path` and `params.reason`. The
`IpcError::io(code, path, error)` constructor in `secs-types` belongs to
another worker (P184); build the `IpcError` with the existing constructors
here and leave a `// TD-208: IpcError::io once P184 lands` comment.

Seam: `Request::ProjectOpen`. Red tests in `tests/service.rs`:
`opening_a_missing_project_names_the_path_td208` (a path that does not exist
→ `ErrorCode::ProjectIo`, `params.path` equals the literal path given,
`params.reason == "not_found"`) and, on Windows only (`#[cfg(windows)]`),
`opening_an_unreadable_project_is_a_permission_error_td208` (a `.secs` whose
ACL denies read to the current user, set with `icacls` in the test and
restored after → `ErrorCode::PermissionDenied` with `params.path`).

Commit subject: `fix(audit): A-37b 프로젝트 열기 오류가 원인과 경로를 표시합니다 (#186 TD-208)`.

## Verification (foreground; do not end the turn before they finish)

After the last commit:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-svc
```

`check_feature_tests.py` on Windows reports two tests Cargo does not list
here (`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`); that is a known local blind
spot, any other name it reports is yours to fix.

## Report

Per item: the commit hash, the Red test name and the assertion it failed
on, the Green summary, the gate outputs (tail). Then: items blocked or
reclassified and why, any seam you changed and why, anything the packet
left open. Give the final `git log --oneline` of the branch.
