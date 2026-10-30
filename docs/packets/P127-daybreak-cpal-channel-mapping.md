# P127 (Daybreak): open CoreAudio devices at their own channel count (#120)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\macos-audio`
(branch `claude/macos-audio`, from main at v0.35.0). Do not commit, do not
push. Another worker (P128) edits `.github/**` and `apps/secs2r/src/self_test.*`
in the same worktree at the same time; do not touch those.

## The bug

Issue #120: on macOS the level check plays nothing and the microphone shows
"응답 없음". `secs_audio::level::level_check_streaming` plays channel `n` as
an `n`-channel stream (L = 1 channel, R = 2 channels, only the last one
carries the signal) and records input channel `m` as an `m`-channel stream;
`secs_audio::measurement` does the same with the plan's highest channel.
`crates/secs-sys/src/cpal.rs` `select_config` accepts a device range only
when `r.channels() == spec.channels`. cpal 0.18.2's CoreAudio host reports a
device's total channel count as its only channel count
(`cpal-0.18.2/src/host/coreaudio/macos/device.rs`, `supported_configs`), so
a 1-channel stream on a 2-channel MacBook output, or input 1 of a 2-input
interface, is refused with `UnsupportedFormat`, the level monitor job ends
and the screen is left showing "재생 중". Windows uses WASAPI and is not
affected; ALSA usually reports wider ranges, but the fix applies there too.

## Design (fixed)

1. **Range choice.** A pure function over the device's ranges picks the
   stream config: sample rate inside the range, PCM format, and
   `channels >= spec.channels`. Prefer the exact channel count, then the
   fewest channels above it, then F32 (as now). Keep it testable without a
   device (build `cpal::SupportedStreamConfigRange` values in tests). The
   chosen device channel count travels with the session.
2. **Output.** The stream runs at the device channel count. `render` maps
   each source frame (`spec.channels` samples) into the first
   `spec.channels` channels of a device frame and fills the other channels
   with `T::EQUILIBRIUM`. Cursor, completion and final-period accounting
   stay correct in source frames (the final period is computed from the
   device buffer's frame count). Allocation-free in the callback, as now.
3. **Input.** The stream runs at the device channel count. `capture` keeps
   the first `spec.channels` samples of each device frame and drops the rest
   before packetising; packets stay whole frames of `spec.channels`; the
   overflow and alignment rules stay as now. `PACKET_SAMPLES` bounds the
   kept channels; a device frame wider than the callback buffer can carry is
   not a reason to fail as long as the kept channels fit.
4. **Probe.** `probe` reports `supported` when the range choice succeeds,
   and its `detail` says when the device is wider (for example
   "device has 2 channels; using the first 1").
5. **Tests** (in `cpal.rs`'s test module, no device): range choice (exact
   preferred over wider; wider chosen when no exact; none when every range
   is narrower or the rate is outside; F32 preferred among equals); render
   1→2, 2→8 and 3→3 with zero fill and correct completion; capture 2→1,
   8→3 with alignment kept across packet boundaries and overflow dropping
   whole frames. Keep the existing tests passing.
6. **CHANGELOG.md**: a new top section `## 0.35.1 - 2026-10-29` with one line
   under `### 고친 것` (Korean, 합니다체, no em-dash): on macOS, playing or
   recording fewer channels than the device has (for example only L of
   stereo speakers, or input 1 of a two-input interface) failed to open the
   stream, so the level check and measurements stopped without sound;
   SECS 2R now opens the device at its own channel count and uses the first
   channels (#120). Follow the style of the sections below it.

## Files

You may edit: `crates/secs-sys/src/cpal.rs`, `CHANGELOG.md`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs-sys --all-targets --locked
cargo test -p secs-audio -p secs-service --all-targets --locked
```

`cpal.rs` is compiled only off Windows (`#[cfg(not(target_os = "windows"))]`
in `lib.rs`), so on this Windows machine its tests do not run. Also run
`cargo check -p secs-sys --target x86_64-unknown-linux-gnu --tests` if the
target is installed (say if it is not), and say plainly in the report that
the new tests were not executed locally; CI runs them on Linux and macOS.

## Report

The range rule and the mapping as built, the tests added, files changed,
and the commands' last lines.
