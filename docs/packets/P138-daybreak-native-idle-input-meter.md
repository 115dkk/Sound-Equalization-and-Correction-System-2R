# P138 (Daybreak): the native level monitor shows the input while idle

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\native-meter`
(branch `claude/native-meter`, from main at 0.35.1). Do not commit, do not
push. Never open real audio devices or run the app on this machine; tests
use fake backends.

## What happened

A tester on a Mac (0.35.1) reports: the sweep plays, but on 2. 연결·점검 the
input meter never moves, so the microphone looked dead; 3. 측정 then records
normally. The cause is a design gap, on every OS: with the native method the
level monitor (`crates/secs-service/src/monitor.rs`, `sampled_loop`) reads
input only during a level play (`level_check_streaming`), and even then
reports the windows only after the recording ends. While idle it sleeps, so
the meter is still. With REW, `rew_loop` forwards REW's input levels every
0.25 s, and the meter moves (upstream behaviour). `apps/secs2r/ui/src/state/session.ts`
documents the gap ("the native and mock monitors read only during a play").

Read first: `CLAUDE.md`, `crates/secs-service/src/monitor.rs`,
`crates/secs-service/src/measure/native.rs` (`level_io`, `to_audio_plan`),
`crates/secs-audio/src/level.rs` (`level_check_streaming`, `analyse_level`,
the 0.1 s windows), `crates/secs-audio/src/session.rs` (how an input is
opened with the share preference, read and stopped), `crates/secs-audio/src/audio.rs`
(`AudioBackend`, `CaptureRead`), the tests of these modules,
`apps/secs2r/src/self_test.js` and `.github/scripts/macos_app_check.py`
(the macOS app check with BlackHole), `apps/secs2r/ui/src/ipc/tauri.ts`
(how the screen passes a Tauri `Channel` as `events` to `secs_call`).

## Design (fixed)

1. **`secs_audio::level::input_meter`** (public, re-exported like
   `level_check`): opens the input endpoint of a `LevelInput` with the given
   sample rate and share preference (the same opener the session uses),
   reads the chosen channel, and calls `on_level(analyse_level(window))` for
   every 0.1 s window, until `cancel` is cancelled or `stop_when()` returns
   true (checked at least every window). It stops the stream on every exit.
   A capture discontinuity does not fail it (a meter may skip); backend
   errors return `Err`. Non-finite samples skip that window. The callback
   must not run on the audio thread.
2. **`NativeMeasurer::input_meter(plan, cancel, stop_when, on_level)`**
   builds the input, channel, sample rate and share from the plan exactly as
   `level_io` does (share one helper; no role is needed).
3. **`sampled_loop`**: for the native method, while no new play is pending,
   run the input meter and forward each report with `send_meter`; when a
   play arrives (or the job is cancelled), the meter stops, closing the
   input before the level check opens its streams, and resumes after the
   play. A meter error fails the monitor job through `level_error` as a
   play error does. The mock method keeps its current behaviour.
4. **Tests**: `input_meter` with a fake backend (windows reported, the
   chosen channel, stop on `stop_when` and on cancel, stream stopped on
   every exit, a discontinuity tolerated, an error returned); the native
   monitor sends `Meter` events while idle and stops metering for a play
   (fake backend, no devices).
5. **macOS app check**: `self_test.js` passes an events channel to
   `secs_call` for `level_monitor_start` the way the screen does (build it
   from `window.__TAURI_INTERNALS__.transformCallback`; the screen's
   `Channel` serialises as `__CHANNEL__:<id>`, and its callback receives
   `{ index, message }`; accept a bare message too), and before the first
   `level_play` waits up to 5 s for at least five `meter` events of that
   job. It records `audio.idle_meter = { events, first_ms }` in the report.
   `macos_app_check.py`'s `audio_problems` requires `events >= 5`; add a
   unit test. (BlackHole is silent while idle, so values may be `null`;
   only arrival is checked.)
6. **CHANGELOG.md**: new top section `## 0.35.2 - 2026-10-30`, `### 고친 것`,
   one line (합니다체, no em-dash): with the built-in measurement the input
   meter on 연결·점검 stayed still while nothing played, so a working
   microphone looked dead; it now shows the input continuously, as with REW.

## Files

You may edit: `crates/secs-audio/src/level.rs`, `crates/secs-audio/src/lib.rs`
(re-export), `crates/secs-audio/tests/**` if tests live there,
`crates/secs-service/src/monitor.rs`, `crates/secs-service/src/measure/native.rs`,
`crates/secs-service/tests/**` for the monitor test, `apps/secs2r/src/self_test.js`,
`.github/scripts/macos_app_check.py`, `.github/scripts/test_macos_app_check.py`,
`CHANGELOG.md`, `features.toml` if a feature row names these tests.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-audio -p secs-service --locked
cargo test -p secs2r --locked
python -m unittest discover -s .github/scripts -p "test_*.py"
python .github/scripts/check_feature_tests.py
node --check apps/secs2r/src/self_test.js
```

## Report

The function and its rules as built, how the monitor switches between
metering and a play, the self-test change, tests, files changed, and the
commands' last lines.
