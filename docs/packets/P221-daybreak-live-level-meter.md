# P221 (Daybreak): the level meter moves while the check sound plays

Worktree `E:\wt\forum-584453`, branch `fix/forum-584453`. Set
`CARGO_TARGET_DIR=C:\cargo-target\forum-584453` for every cargo command (the
worktree must not get a `target` folder of its own). Do not commit, do not
push. Run every command in the foreground and end your turn only after the
report. Allowed files: `E:\wt\forum-584453\crates\secs-audio\src\session.rs`,
`E:\wt\forum-584453\crates\secs-audio\src\level.rs`, the tests of
`secs-audio`, and callers that stop compiling because of a signature you
changed (name them in the report). Do not touch `crates\secs-sys` (another
worker is editing `wasapi.rs` at the same time), the UI, docs or the
changelog.

## Why

Forum report (2026-11-12): during the level check the microphone bar shows
nothing while the sound plays and appears only after it ends. The cause is in
`secs_audio::level::level_check_streaming`: it calls `play_and_record`, which
returns after the whole recording, and only then calls `on_level` for the
0.1-second windows of the burst. Between bursts the service runs
`input_meter`, which does report live, so the bar is alive exactly when no
sound plays.

## What to do

1. `session.rs`: `SessionRequest` gets `pub meter_channel: Option<u16>`
   (default `None` in `SessionRequest::new`; validated: `Some(c)` needs
   `1 <= c <= input_channels`). `SessionEvent` gets

   ```rust
   /// One 0.1-second window of the metered input channel, in capture order.
   /// Windows that contain a discontinuity are not sent.
   InputWindow { samples: Vec<f32> },
   ```

   The capture worker, while it reads, copies the metered channel into a
   window of `sample_rate / 10` frames (at least 1) and sends each full window
   to the thread that runs `on_event`, the same way the other events reach it
   (read how `Message` is delivered and keep the ordering guarantees the
   existing events have). With `meter_channel == None` the capture loop does
   exactly what it does today: no allocation, no message. The window buffers
   are allocated before the stream starts; sending a window may allocate (it
   is not the audio callback, the worker already allocates nothing else, so
   reuse a spare buffer if the channel type makes that simple, otherwise a
   `Vec` per window is acceptable at ten per second; say which you did).
   A window whose read reported `discontinuity`, or that contains a non-finite
   sample, is dropped, as `input_meter` does.

2. `level.rs`, `level_check_streaming`: set `request.meter_channel =
   Some(input.channel)` and turn each `SessionEvent::InputWindow` into
   `on_level(analyse_level(..)?)` with the input's calibration and SPL
   reference, while the sound plays. An `analyse_level` error inside the event
   callback must fail the check the way it does today (the callback cannot
   return a `Result`; carry the first error out and return it after
   `play_and_record`, or cancel through the token; describe what you chose).
   Remove the loop that replays the windows of the settled burst through
   `on_level` after the recording: the live windows replace it. The returned
   `LevelReport` (the loudest burst-sized window) does not change by a bit.

3. Tests (in `secs-audio`'s existing test modules, with `NullBackend` or
   `SimRoom`, whichever the neighbouring tests use):
   - `on_level` is called before `level_check_streaming` returns and at least
     `floor(seconds * 10)` times for a burst of `seconds`;
   - the returned report equals today's for the same input (compare with a
     value computed in the test from the recording, or pin the existing
     golden if one exists);
   - `meter_channel == None` sends no `InputWindow`;
   - a `meter_channel` outside `1..=input_channels` is rejected before any
     stream opens.

## Checks (foreground, paste the tail of each)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-audio -p secs-service --all-targets --locked -- -D warnings
cargo test -p secs-audio --locked --no-fail-fast
cargo test -p secs-service --locked --no-fail-fast -- level monitor
```

## Report

- Files changed.
- How windows travel from the capture worker to `on_event`, and what happens
  to ordering relative to `Progress` and `InputStopped`.
- How an `analyse_level` error leaves the check.
- The tail of every check, including failures you could not resolve.
