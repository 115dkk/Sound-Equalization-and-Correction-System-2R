# P128 (ASTRA): the macOS app check plays and records for real (#120)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\macos-audio`
(branch `claude/macos-audio`, from main at v0.35.0). Do not commit, do not
push. Another worker (P127) fixes `crates/secs-sys/src/cpal.rs` in the same
worktree at the same time; do not touch Rust outside `apps/secs2r/src/self_test.*`.
Never install drivers, change TCC or run the app on this machine: all of
that runs on GitHub's macOS runners, which are disposable.

## Why

Issue #120: on a Mac the level check plays nothing and the microphone reads
"응답 없음". The macOS app check (`.github/scripts/macos_app_check.py`, run by
`ci.yml`'s bundle job and `release.yml` before publishing) launches the
shipped app with `SECS2R_SELF_TEST`, but its self-test measures with
`method: "mock"`, so no sound is ever played or recorded through CoreAudio.
The cause is known (P127: CoreAudio reports only a device's full channel
count and the app asked for fewer channels). The maintainer requires the
check to play and record for real (2026-10-29), and approved installing
BlackHole 2ch (https://github.com/ExistentialAudio/BlackHole, GPL-3.0) on
the CI runners only.

Read first: `CLAUDE.md`, `.github/scripts/macos_app_check.py` and its test,
`apps/secs2r/src/self_test.rs` and `self_test.js`, the macOS parts of
`.github/workflows/ci.yml` (bundle job) and `release.yml` (installers job),
`.github/eqapo-engines.toml` and `.github/scripts/eqapo_engine.py` (how a
pinned download is fetched and verified), `crates/secs-service/src/monitor.rs`
(level check: `level_monitor_start`, `level_play`, `levels`),
`crates/secs-service/src/measure/**` and the IPC types for
`measure_configure` / `measure_methods` / `measure_position` in
`crates/secs-types/src/ipc.rs`. External, fetch rather than guess:
BlackHole's README and release assets for the pinned version; how macOS
stores microphone consent (TCC) and what GitHub's macOS runner images allow
(https://github.com/actions/runner-images, the macOS image readme and any
TCC notes); how established projects grant microphone access on GitHub
macOS runners.

## Design (fixed)

1. **Pinned BlackHole.** `.github/blackhole.toml` with version, URL of the
   official 2ch installer package from the GitHub release, SHA-256 and
   format, like `eqapo-engines.toml`. A helper `.github/scripts/blackhole.py`
   (stdlib, pure parts unit tested in `test_blackhole.py`) fetches and
   verifies it, installs it (`sudo installer -pkg ... -target /`), restarts
   CoreAudio the way BlackHole documents, and waits until a "BlackHole 2ch"
   device appears (`system_profiler SPAudioDataType -json`), failing with a
   clear sentence otherwise. Never upload the package or the driver.
2. **Microphone consent for the app.** Before the audio self-test, grant
   the shipped app (bundle id `io.github.115dkk.secs2r`, or what
   `tauri.conf.json` says) microphone access on the runner in the way that
   actually works on GitHub's macOS images (research it; say what you used
   and why, and check the TCC table's columns at run time rather than
   assuming a schema). If the runner refuses, fail with that reason; do not
   skip the audio step.
3. **Audio self-test.** `self_test.rs` gains an optional second variable
   `SECS2R_SELF_TEST_AUDIO` naming a device (for example `BlackHole 2ch`);
   only when it is set, `self_test.js` also runs, after the existing mock
   steps, a native audio step in a new stereo project in the work folder
   (choose the project preset that has exactly two speakers, L and R; read
   the presets):
   - `measure_configure` with the native method, that device as output and
     input, L on output 1, R on output 2, input channel 1;
   - the level check for L and then R through the screen's own requests
     (`level_monitor_start`, `level_play`, waiting for the play to end,
     `levels`), requiring each stored level to be within ±6 dB of the
     played level (BlackHole loops output back to input unchanged; say the
     exact rule you implemented and why, from the code's level math);
   - one `measure_position` with the native method, then a check that the
     stored measurement of L and of R is not silent and has its impulse
     where the loopback puts it (pick a check the existing IPC exposes,
     for example `measurement_curves` or `measurement_records`, and say
     which);
   - every failure goes into the report with the request that failed.
   The mock steps stay exactly as they are and still run without the
   variable.
4. **macos_app_check.py.** A new option `--audio-device NAME`; when given,
   the native-architecture run of the self-test sets
   `SECS2R_SELF_TEST_AUDIO` and the report must show the audio step
   passed (`report_problems` gains the audio facts). The Rosetta run stays
   mock-only. Unit tests for the new report checks.
5. **Workflows.** In `ci.yml`'s bundle job and `release.yml`'s installers
   job, macOS only, before "The macOS app opens and calibrates": install
   BlackHole through the helper and grant consent; then pass
   `--audio-device "BlackHole 2ch"` to the check. Upload, as now, only the
   app-check folder (reports, logs), never the driver.
6. **CHANGELOG.md**: nothing (P127 writes the user-facing line); CI-only.

## Correction (2026-10-29, after the first attempt)

BlackHole loops output N to input N and never sums channels, so input 1
cannot hear R on output 2. The audio step therefore runs in three
configurations of the same 2.0 project and device: (1) level check L with
`output_channels: [1, 2]`, `input_channel: 1`; (2) level check R with the
same outputs and `input_channel: 2`; (3) one sweep with
`output_channels: [1, 1]`, `input_channel: 1`, requiring non-silent L and R
measurements with the loopback impulse. `measure_configure` allows two
roles on one output. Each configuration is read back into the report.

## Files

You may edit: `.github/blackhole.toml` (new), `.github/scripts/blackhole.py`
(new), `.github/scripts/test_blackhole.py` (new),
`.github/scripts/macos_app_check.py`, `.github/scripts/test_macos_app_check.py`,
`.github/workflows/ci.yml` and `release.yml` (the macOS steps above only),
`apps/secs2r/src/self_test.rs`, `apps/secs2r/src/self_test.js`,
`features.toml` only if the self-test is registered there.

## Verification (foreground; do not end the turn before they finish)

```sh
python3 -m unittest discover -s .github/scripts -p "test_*.py"
python3 -I .github/scripts/blackhole.py --help
python3 -I .github/scripts/macos_app_check.py --help
cargo fmt --all -- --check
cargo clippy -p secs2r --all-targets --locked -- -D warnings
cargo test -p secs2r --locked
```

## Report

The BlackHole pin (version, URL, SHA-256) and install steps, how consent is
granted and what that was based on, the audio self-test steps and their
pass rules, the workflow changes, files changed, and the commands' last
lines.
