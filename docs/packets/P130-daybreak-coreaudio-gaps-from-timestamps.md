# P130 (Daybreak): CoreAudio gaps come from the data, not from overload notices

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\macos-audio`
(branch `claude/macos-xrun`, from main at v0.35.1). Do not commit, do not push.

## What happened

The first real macOS run (PR #122's `installers (macos)` job: BlackHole 2ch
installed, microphone consent granted, P127's channel mapping opened both
streams) failed the level check L with
`recording_failed / gap: capture reported a discontinuity`.

`crates/secs-sys/src/cpal.rs` `CallbackState::stream_error` turns every
`cpal::ErrorKind::Xrun` into `discontinuity = true` (capture) and into
`underruns` (playback). On macOS, cpal 0.18.2 emits `Xrun` from a listener on
`kAudioDeviceProcessorOverload` with **global scope**
(`cpal-0.18.2/src/host/coreaudio/macos/mod.rs`, `spawn_overload_listener`
and the listener near line 183): a device-wide notice that some IO cycle on
that device overloaded, delivered to every stream on the device, carrying no
information on whether this stream's samples were lost. `play_and_record`
(`crates/secs-audio/src/session.rs`) starts the input first and adds the
output on the same device afterwards, as a user with one audio interface
does, so a notice around that moment fails the recording even when the
captured data is contiguous. On Linux, cpal's ALSA `Xrun` comes from a real
xrun (EPIPE) and stays as it is. Windows does not use this backend.

## Design (fixed)

1. **macOS: overload notices are diagnostics.** Under
   `cfg(target_os = "macos")`, `ErrorKind::Xrun` increments an overload
   counter and sets neither `discontinuity` nor the playback underrun count.
   Other non-Windows hosts keep today's behaviour.
2. **macOS: capture gaps from timestamps.** `capture` reads
   `InputCallbackInfo::timestamp().capture` (check cpal 0.18.2's
   `InputCallbackInfo`, `InputStreamTimestamp`, `StreamInstant` API in the
   registry source). The first callback sets the baseline. Each later
   callback compares its capture instant with the previous instant plus the
   previous callback's device frames divided by the sample rate; when the
   difference in either direction exceeds `max(2 ms, half the previous
   callback's duration)`, it sets `discontinuity` and counts a gap. Put the
   decision in a pure function over (previous instant as nanoseconds,
   previous frames, sample rate, current instant) with unit tests; the
   callback stays allocation-free (atomics only).
3. **macOS: playback gaps from timestamps.** `render` does the same with
   `OutputCallbackInfo::timestamp().playback` and counts a late or early
   jump as an underrun, so a real output glitch still fails a measurement.
4. **Diagnostics.** When a session stops (input `stop`, output after
   `play_to_completion`), if any overload notice or gap was counted, write
   one line to stderr, for example
   `secs-sys cpal: input stream: 3 overload notices, 0 timestamp gaps`.
   (The CI's macOS check captures the app's stderr into its log; this is how
   the next failure, if any, says which it was.) Nothing in the callbacks
   prints.
5. **Tests** in `cpal.rs`'s test module, device-free: the gap rule (exact
   continuation, small jitter accepted, late jump and early jump rejected,
   tolerance floor 2 ms, first callback only sets the baseline); on macOS
   builds, `stream_error(Xrun)` counts without setting discontinuity, and on
   other non-Windows builds it still sets it (gate the two tests by cfg).
6. **CHANGELOG.md**: a new top section `## 0.35.2 - 2026-10-30` with one
   line under `### 고친 것` (Korean, 합니다체, no em-dash): on macOS a
   recording failed as "끊김" whenever the device reported a processing
   overload anywhere, even when no samples were lost (for example while the
   output starts on the same interface); SECS 2R now decides a gap from the
   capture and playback timestamps (#120).

## Files

You may edit: `crates/secs-sys/src/cpal.rs`, `CHANGELOG.md`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p secs-sys --all-targets --locked --target x86_64-apple-darwin -- -D warnings
cargo check -p secs-sys --tests --locked --target x86_64-apple-darwin
cargo test -p secs-sys --all-targets --locked
```

`cpal.rs` is compiled only off Windows; its tests run in CI on Linux and
macOS, not here. Say so plainly in the report.

## Report

The rules as built (with the cpal API you used), the tests, files changed,
and the commands' last lines.
