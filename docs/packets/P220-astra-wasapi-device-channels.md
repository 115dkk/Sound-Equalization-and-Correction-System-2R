# P220 (ASTRA): shared-mode WASAPI streams carry the device's own channels

Worktree `E:\wt\forum-584453`, branch `fix/forum-584453`. Set
`CARGO_TARGET_DIR=C:\cargo-target\forum-584453` for every cargo command (the
worktree must not get a `target` folder of its own). Do not commit, do not
push. Run every command in the foreground and end your turn only after the
report. Allowed files: `E:\wt\forum-584453\crates\secs-sys\src\wasapi.rs`
only. Do not touch `crates\secs-audio`, `crates\secs-sys\examples`, the
upstream Python tree, docs or the changelog. Another worker edits
`crates\secs-audio\src\level.rs` and `session.rs` at the same time; leave them
alone.

## Why

Forum report (2026-11-12): with the app's own measurement the level check
reads much lower than REW at the same microphone gain, and the levels of the
channels are inconsistent. Measured on this machine with VB-CABLE and the
scratch example `crates\secs-sys\examples\channel_probe.rs` (uncommitted, you
may run it, do not edit it):

- A 1-channel shared capture stream on a 2-channel capture endpoint returns
  `(L + R) / 2`: a −20 dBFS tone on the left channel reads −26.02, and a tone on
  the right channel also arrives in "channel 1". A 2-channel stream on the same
  endpoint reads −20.00 on the right channel and nothing on the other.
- A 7-channel shared playback stream on an 8-channel endpoint is remixed by
  Windows: channels 1–3, 5 and 6 arrive 4.6 dB low and channel 7 arrives on
  channels 1 and 3 at −27.7. Streams of 1–6 and 8 channels arrive unchanged on
  this endpoint.

The cause is ours: the callers ask for a stream of `spec.channels` channels
where `spec.channels` is the highest channel number they use, and
`requested_format` gives it `WaveFormat::new(.., None)`, whose mask is
`(1 << channels) - 1`. With `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM` the audio
engine then converts between that speaker layout and the endpoint's.

## What to do

In `ShareMode::SharedAutoConvert` the audio client is initialised with the
endpoint's own channel count and channel mask (taken from `GetMixFormat`) and
the requested sample rate, as 32-bit float. The session keeps presenting
`spec.channels` channels to its caller:

- Output: every frame the caller writes (`spec.channels` samples) is widened to
  the device channel count, the caller's channel `i` going to device channel
  `i` and the rest zero.
- Input: every captured frame (device channel count) is narrowed to its first
  `spec.channels` samples before it reaches the caller.

`ShareMode::Exclusive` keeps today's behaviour exactly (the format the device
accepts verbatim, no widening). `validate_shared_channels` already refuses a
request wider than the mix format; keep it.

Details that are yours to get right, check them against the code and the
documentation instead of assuming:

- `carries_spec_as_float32` compares the accepted format's channel count with
  `spec.channels`; in shared mode the comparison is with the device channel
  count.
- Buffer sizes, `scratch`, `CaptureQueue`, `append_packet`, the stall watch,
  frame counts in `CaptureRead`, `PlaybackReport`, the trace records and
  `validate_frame_aligned` count either caller frames or device samples. Go
  through every use of `spec.channels` in the file (there is a list from
  `grep -n "spec.channels\|get_nchannels\|\.channels" crates/secs-sys/src/wasapi.rs`)
  and decide for each which count it needs. Frame counts reported to the
  caller stay in frames, which do not change.
- The realtime paths must not allocate per packet: widen and narrow into
  buffers allocated at open.
- The probe (`probe`, around line 1577) reports whether a spec is usable; in
  shared mode a spec is usable when `spec.channels <= mix channels`, and the
  format it describes is the device-wide one.
- No new `unsafe`. The crate's budget is fixed; if something cannot be done
  without it, stop and report.
- When the mix format is not `WAVE_FORMAT_EXTENSIBLE` (plain mono or stereo),
  there is no mask to copy; build the format the way the endpoint reports it.

## Tests

Unit tests in `wasapi.rs` beside the existing ones, on the pure functions you
extract for widening and narrowing (frame order, zero fill, partial trailing
frame rejected, `spec.channels == device channels` is a plain copy), and for
the format the shared open asks for given a mix format (channel count and mask
of the mix format, rate of the spec).

Then run the scratch example on the real endpoints (it only uses the virtual
cables; nothing is audible):

```sh
cargo build -p secs-sys --example channel_probe --locked
C:/cargo-target/forum-584453/debug/examples/channel_probe.exe "CABLE-B Input" "CABLE-B Output"
C:/cargo-target/forum-584453/debug/examples/channel_probe.exe "CABLE Input" "CABLE Output"
```

The example captures each case twice, with a 1-channel stream and with the
endpoint's full channel count. Expected after the change: on CABLE-B,
`play ch 1 of 1` and `play ch 1 of 2` read −20.00 in the first captured
channel (also with `capture 1 ch`, which reads −26.02 today) and nothing in
the second; `play ch 2 of 2` reads −20.00 only in the second and nothing with
`capture 1 ch`. On CABLE (8 channels out), `play ch K of 7` arrives only on
channel K at −20.00 for every K. A residue around −40 dB in another channel
right after a tone is the cable's buffer from the previous case, not a
finding. Paste both outputs.

## Checks (foreground, paste the tail of each)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-sys --all-targets --locked -- -D warnings
cargo test -p secs-sys --locked --no-fail-fast
python .github/scripts/check_unsafe.py 2>/dev/null || true
```

(`secs-sys::callback_native_sample_conversion` fails on this machine before
your change too; say so if it still does and nothing else fails.) If the
repository has a script that counts `unsafe` for the policy gate, find it with
`git grep -n "unsafe" -- .github/scripts crates/secs-policy | head` and run it.

## Report

- What the shared open asks for now, and where widening and narrowing happen.
- Every `spec.channels` use you changed and every one you left, with the
  reason, as a short table.
- The two example outputs.
- The tail of every check, including failures you could not resolve.
