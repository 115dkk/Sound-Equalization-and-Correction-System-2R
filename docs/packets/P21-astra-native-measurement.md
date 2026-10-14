# P21 (ASTRA): native measurement core in `secs-audio`

Worktree: `/home/user/wt/p21` (branch `m4/p21`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p21`
(yours alone).

Read first: `docs/M4.md`, `docs/adr/0005-native-measurement.md`,
`docs/adr/0002-*` (unsafe isolation: this crate has `#![forbid(unsafe_code)]`
and no OS dependency), `docs/adr/0007-*`, `CLAUDE.md` (third-party code:
source and licence at the top of each file, and a row in
`docs/THIRD_PARTY.md`), `crates/secs-audio` (a stub), `crates/secs-dsp`
(FFT, filters, what already exists).

Then the Impulcifer 3.x code you will port (MIT; repository checked out at
`/home/user/Impulcifer-pip313`, commit `cac2217`):
`crates/impulcifer-types/src/audio.rs` (the `AudioBackend`, `OutputSession`,
`InputSession` traits, `Endpoint`, `StreamSpec`, `ShareMode`, `AudioError`),
`crates/impulcifer-audio-io/src/{session.rs, policy.rs, null_backend.rs,
cached_backend.rs}` (the play-and-record session and the share-mode policy),
`crates/impulcifer-dsp/src/estimator.rs` (the exponential sweep, its inverse
filter, the sweep sequence, the estimate), and their tests. Do **not** port
`cpal_backend.rs` or `impulcifer-sys-win`: those are OS adapters and go to
`secs-sys` in P24.

Upstream SECS 2 has no audio code (REW does it); read
`src/secs/rewapi/interfaces.py` (`MeasurementPlan`, `SWEEP_LENGTHS`,
`LOW_SNR_DB`), `src/secs/wizard/step3_level.py` and `src/secs/wizard/mic_meter.py`
for what the level check reports, and `src/secs/rewapi/fake.py` for what the
mock measurement simulates.

## Goal

Measure a position with any `AudioBackend`, return per-role IRs with a
consistent time origin, check levels, read a microphone calibration file,
and simulate a room so the mock measurement runs the same code.

## Design (fixed)

- **One session per position.** All roles of a position are measured in one
  play-and-record session: a sweep sequence plays each role's sweep on its
  output channel in plan order, with a silent gap after each sweep long
  enough for the room to decay (a parameter, default 1.5 s), while the
  microphone channel records throughout (Impulcifer's `sweep_sequence`). The
  roles then share one clock, so their relative arrival times are exact. This
  is the native counterpart of REW's timing reference.
- **IR and t0.** Each role's segment is deconvolved with the sweep's inverse
  filter. The IR is cropped to a window that starts a fixed pre-roll before
  the direct sound (parameters with defaults you justify; the engine needs at
  least 2 s after the onset for the long window) and its `t0` is the time of
  its first sample on the session clock, measured from the start of that
  role's sweep. Equal latency for every role then cancels in the engine's
  relative arrivals (`t0 + onset/fs`, `secs-engine/src/measurement.rs`).
- **Optional loopback.** If the plan names a loopback input channel (an output
  wired back to an input), it is recorded too and its deconvolved peak gives
  the absolute latency, which is subtracted from every `t0`.
- **Sweep.** Exponential sine sweep from the plan's start to end frequency,
  length from `SWEEP_LENGTHS` in samples (`64k` = 65,536), level from the
  plan's dBFS, with fades. Port Impulcifer's sweep and inverse filter; adapt
  only what the length vocabulary and the level need.
- **SNR.** Report each IR's SNR (direct-sound energy against the noise floor
  before the sweep) so the service can warn below `LOW_SNR_DB`.
- **Level check.** `level_check(backend, output, input, level_dbfs, seconds)`
  plays a deterministic pink-noise burst (a sine for subs, as upstream asks
  REW) on one output channel and reports `input_dbfs` (RMS), `peak_dbfs`,
  `clip` (peak above −1 dBFS) and, with a calibration, `spl_db`.
  `recommend_sweep_level(measured, played, target_window)` returns the sweep
  level that lands the recording in upstream's −30 to −15 dBFS window
  (deviation U1: the level check sets the sweep level).
- **Microphone calibration.** Parse the common calibration file formats
  (REW/miniDSP style: optional `"Sens Factor = x dB"` header, then `freq dB
  [phase]` lines; comments and blank lines). How a calibration enters SPL and
  whether and how it corrects the IR must follow REW's documented behaviour,
  so the native path gives what a REW-measured project gives: fetch REW's help
  pages on calibration files and SPL calibration (WebFetch), cite the URLs in
  the code, and implement that. If REW's behaviour cannot be established,
  parse and expose the curve, do not apply it to the IR, and say so.
- **Simulated room.** `SimRoom: AudioBackend` holds, per output channel, an IR
  to each input channel, a fixed latency in samples, and a seeded noise
  level. Playing a buffer and recording yields the convolution plus noise,
  delayed. The service's mock measurement uses it with SECS 2's sample IRs.
- **Cancellation.** A cancel token stops the session between sweeps (and, if
  the ported session supports it, within one); a cancelled position returns
  no partial roles.

## Tests

- Ported Impulcifer tests for the session, the policy and the estimator.
- Round trip in `SimRoom`: known IRs (synthetic, and the `dolby` engine
  case's IRs read from `oracle/goldens`) measured through the sweep sequence
  come back with relative error you state and justify (aim below −60 dB in the
  band 20 Hz–20 kHz for noiseless runs) and relative arrivals exact to one
  sample, for several latencies, sample rates 44.1/48/96 kHz and lengths.
- Loopback: absolute latency recovered to one sample.
- SNR with known added noise within 1 dB.
- Level check on known signals; recommended level lands in the window.
- Calibration files: the sample files you add under `crates/secs-audio/tests/data/`
  (write small ones; do not copy vendor files), malformed input rejected
  without panics.
- Cancellation: a cancel during a position returns `Cancelled` and no roles.

`features.toml`: set `measure.native` to `partial` with your tests (it becomes
`implemented` when P24 adds the OS adapters and the probe), and add
`measure.level_check`, `measure.mic_calibration` (M4) as `implemented`.

## Files you may change

- `crates/secs-audio/**` (keep `#![forbid(unsafe_code)]`, `[lints] workspace = true`;
  no OS dependency)
- `docs/THIRD_PARTY.md` (the Impulcifer rows), `docs/DEVIATIONS.md` (U1 row only)
- `features.toml` (the three entries), `Cargo.toml` (workspace dependencies you
  need, if any), `Cargo.lock` only as cargo rewrites it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p21
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p21
cargo fmt --all -- --check
cargo clippy -p secs-audio --all-targets --locked -- -D warnings
cargo test -p secs-audio --locked 2>&1 | grep -E "test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-audio
cargo deny --all-features check 2>&1 | tail -3
python3 .github/scripts/upstream.py check-tree
```

Do not run `check_feature_tests.py` (Claude runs it in the gate).

## Report

What you ported (files, commit) and what you wrote, the API as built, the
REW calibration behaviour you found and its sources, each test with its
numbers, and anything you could not do.
