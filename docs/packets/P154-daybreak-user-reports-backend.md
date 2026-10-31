# P154 (Daybreak): app log, stale XO recommendation, 'XO 추천 포함' and background refresh (ADR 0027, 0028, 0029)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`,
branch `claude/user-reports` (cut from `origin/main`). Do not commit, do not push.
Opus works on `apps/secs2r/ui/**` at the same time (P155); do not touch it, except the regenerated bindings under `apps/secs2r/ui/src/ipc/bindings/`.

Read first: `CLAUDE.md`, `docs/adr/0027-app-log.md` (the decision),
`apps/secs2r/src/main.rs` (the `secs_call` command, `Deps` construction,
`observer`), `crates/secs-service/src/lib.rs` (`Deps`, `EventSink`, where the
observer receives every job event), `crates/secs-types/src/ipc.rs`
(`BootstrapInfo`, `ResultsSummary`, `JobEvent`), `crates/secs-service/src/calibration.rs`
(`start_xo`, `signature_key`, `target_key_string`, `config_key`),
`crates/secs-service/src/results.rs` (how `ResultsSummary` and staleness are
built), `crates/secs-project/src/calibration.rs` (how `xo_rec` is read and
written), and `apps/secs2r-host/src/rules.rs` (`load_logs`, an existing log
reader in this repository, for style).

## Part A: app log (ADR 0027)

1. New module `apps/secs2r/src/applog.rs`, std only (no new dependencies):
   - `AppLog::open(dir: &Path) -> AppLog` creates `dir` if needed and appends
     to `dir/secs2r.log`; when the file passes 2 MiB, rotate
     `secs2r.3.log → secs2r.4.log`, …, `secs2r.log → secs2r.1.log` (drop the
     oldest beyond 4). Every write is one line, flushed; any I/O error is
     swallowed (the app never stops because of the log).
   - `fn line(&self, level: Level, area: &str, text: &str)` with `Level::{Info,
     Warn, Error}`; line format `<local time with offset, ms> <LEVEL> <area>
     <text>`. Use the service clock helpers the app already has
     (`secs_sys::clock::utc_offset_seconds`) for the offset.
   - `fn redact(text: &str) -> String` replaces the user's home directory
     (from `std::env::home_dir()` or the `HOME`/`USERPROFILE` variable) with
     `~`, case-insensitively on Windows. Apply it to every line.
   - Thread-safe (`Mutex<File>`), cheap to clone (`Arc`).
2. `EventSink` for the log: logs `JobEvent::Started` (job id, kind),
   `JobEvent::Log` (code and params as `key=value`), `JobEvent::Finished`
   (status, elapsed since its `Started`, error code and message when present);
   ignores `Progress`, `Meter` and anything else. Compose it with the existing
   `hub` so both receive every event (a small fan-out sink).
3. In `main.rs`: open the log in `setup` at `app_config_dir()/logs`; log the
   start (app version, OS and architecture from `std::env::consts`, and the
   compute selection the service reports through `ComputeStatus` or
   `acceleration`, whichever is cheap without starting work); install a panic
   hook that logs message, location and a forced backtrace, then calls the
   previous hook; in `secs_call`, when the envelope is `Err`, log the method
   name (the `method` field of the request JSON) with the error code and
   message, never the params.
4. Calibration and MIMO starts log the settings' `engine_key()`: do this in
   the service where those jobs start, by emitting a `JobEvent::Log` with code
   `calibration_settings` and param `engine_key` (so the log sink records it and
   nothing else changes). Make sure the screen ignores unknown log codes (check
   `apps/secs2r/ui/src/app/jobs/jobText.ts`; report what it does, do not edit UI
   files).
5. `BootstrapInfo` gains `log_dir: Option<String>` (`#[serde(default,
   skip_serializing_if = "Option::is_none")]`, `#[ts(optional)]`); the service
   fills it from `Deps::app_data_path.join("logs")` when the app supplies a data
   path, `None` in tests and the CLI. Regenerate bindings with `cargo test -p
   secs-types`.
6. Unit tests in `applog.rs`: rotation keeps at most five files and the newest
   line is in `secs2r.log`; redaction replaces the home directory; the sink
   writes Started/Log/Finished and skips Progress.

## Part B: stale crossover recommendation

1. `CalibrationSettings` gains `xo_rec_key: String` (`#[serde(default,
   skip_serializing_if = "String::is_empty")]`, `#[ts(optional)]`), not part of
   `engine_key`. Project file: read and write `xo_rec_key` next to `xo_rec`,
   only when non-empty (an upstream SECS 2 project ignores it).
2. Recommendation key: `pub(crate) fn xo_rec_key(project: &Project, settings:
   &CalibrationSettings) -> String` = `signature_key(project)`, `target_key_string(project)`,
   `config_key(project)` and the `engine_key()` of a copy of `settings` with
   `xo` cleared, joined with `|`. `start_xo` computes it from the snapshot it
   ran on and stores it with `xo_rec` in the same commit.
3. `ResultsSummary` gains `xo_rec_stale: bool` (`#[serde(default,
   skip_serializing_if = "std::ops::Not::not")]`, `#[ts(optional)]`): true when the
   stored `xo_rec` is non-empty and `xo_rec_key` differs from the key computed
   now (an empty stored key with a non-empty `xo_rec`, from older projects, also
   counts as stale).
4. Tests: a recommendation then a settings edit (e.g. `boost_max_db`) makes
   `xo_rec_stale` true; editing only `xo` keeps it false; a re-recommendation
   clears it; project round trip keeps `xo_rec_key`.
5. `docs/DEVIATIONS.md`: add D50 in the format of D49 for the extra project
   key `xo_rec_key` (written only with a recommendation; upstream ignores it).

## Part C: 'XO 추천 포함' (ADR 0028)

1. `CalibrationSettings` gains `xo_rec_include: bool`, default **true**
   (`#[serde(default = "...true", skip_serializing_if = "...is_true")]`,
   `#[ts(optional)]`, so a missing value means on and the recorded responses do
   not change). Not part of `engine_key`. Project file: written only when
   false, read with true as the default.
2. `SettingEdit::XoRecInclude { on: bool }` stores it (does not mark results
   stale). An edit that sets a group's crossover value (find the existing XO
   edit variant) also sets `xo_rec_include = false`, so a manual value is not
   overwritten by the next calibration; the edit's response must let the screen
   see the new value (it already returns the settings).
3. The calibrate flow itself is screen-driven (P155: when on and subs exist,
   '보정 계산' starts `recommend_xo`, and the existing `xo.recalibrate` effect
   starts the calibration). Nothing else changes in the service.
4. Tests: default true, missing key reads as true, false round-trips, the XO
   edit turns it off, `engine_key` unchanged by it.
5. `docs/DEVIATIONS.md`: D51 for ADR 0028 (XO is re-recommended before every
   calibration by default; upstream only on the button; `xo_rec_include: false`
   written only when off).

## Part D: background refresh (ADR 0029)

1. `AppSettings` gains `auto_refresh: bool` (default false), stored with the
   other app settings (follow how `updates` or `icon` is versioned and
   migrated in the service's app-settings file), with an `AppSettingsSet`
   path like the others.
2. `CalibrateParams` gains `background: bool` (`#[serde(default)]`,
   `#[ts(optional)]`). The XO recommendation request gains the same flag
   without breaking today's `{"method":"recommend_xo"}` (absent params mean
   `background: false`); check how the request is declared and keep the local
   API and CLI callers working.
3. `secs-compute`: `CancelToken` gains optional pacing,
   `pub fn paced(&self, slice: Duration) -> CancelToken` (shares the cancel
   flag with `self`; clones share the pacing state). On a paced token,
   `is_cancelled()` and `check()` sleep for `slice` whenever at least `slice`
   of time has passed since the last rest, then record the rest. Put the clock
   behind a small trait or function pointer so a unit test can drive it without
   sleeping; add a test that 1 s of simulated work with 5 ms checks rests
   about 0.5 s. Document it as a public API addition.
4. `compute_context(job, memory, background: bool)`: background jobs use
   `max(1, n / 2)` threads where `n = available_parallelism()`; when `n == 1`,
   the context uses `job.cancel().paced(50 ms)` instead. Foreground jobs are
   unchanged. Thread the flag from `start_calibration` (including its chained
   MIMO) and `start_xo`.
5. Tests: thread selection for n = 1, 2, 3, 8; a calibration of a small case
   gives bit-identical summaries with `background` true and false.
6. Log line (Part A): background jobs log `background=true` on their start
   line.

## You may edit

`apps/secs2r/src/**`, `apps/secs2r/Cargo.toml` (no new dependencies),
`crates/secs-compute/**` (Part D only),
`crates/secs-types/**` (only the fields above) and the regenerated
`apps/secs2r/ui/src/ipc/bindings/**`, `crates/secs-service/**`,
`crates/secs-project/**`, `features.toml` (register `app.log` and
`calibration.xo_rec_stale` with the real tests), `docs/DEVIATIONS.md`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-service -p secs-project -p secs2r --locked
python .github/scripts/check_feature_tests.py
```

If the recorded service responses under `apps/secs2r/ui/src/ipc/recorded/`
no longer match, report which file and why; do not run the recorder and do
not edit those files.

## Report

Files changed, the log line format with three real example lines, the
fan-out sink, the panic hook, the bindings diff, the recommendation key and
staleness rule, tests added, and the commands' last lines.
