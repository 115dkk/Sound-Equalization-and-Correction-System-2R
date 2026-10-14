# P24 (Daybreak): OS audio adapters, OS probes and the hardware probe in `secs-sys`

Worktree: `/home/user/wt/p24` (branch `m4/p24`, based on P21's `secs-audio`).
Work only there. Do not commit; Claude reviews the diff and commits. Do not
spawn sub-agents. Build with
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p24`
(yours alone).

Read first: `docs/M4.md`, `docs/adr/0002-*` (the isolation crate, budget 0:
safe APIs only, the leaf contract in `crates/secs-sys/src/lib.rs`),
`docs/adr/0005-native-measurement.md`, `CLAUDE.md` (third-party code: source
and licence at the top of each file, a row in `docs/THIRD_PARTY.md`),
`crates/secs-audio` (the `AudioBackend`, `OutputSession`, `InputSession`
traits and `SharePreference` you implement, ported from Impulcifer in P21),
`crates/secs-rew/src/launch.rs` (the `ProcessProbe` trait), and
`crates/secs-compute` (the `MemoryProbe` seam).

Then the Impulcifer 3.x code you port (MIT; `/home/user/Impulcifer-pip313`,
commit `cac2217`): `crates/impulcifer-sys-win` (WASAPI through wasapi-rs's
safe API, exclusive first and shared with auto-conversion as fallback, thread
priority, `examples/hardware_probe.rs`) and
`crates/impulcifer-audio-io/src/cpal_backend.rs` (macOS and Linux). Their
`CLAUDE.md` rules on wasapi apply here too: wasapi ≥ 0.25, never
`WaveFormat::parse` or `Device::from_raw` (both `unsafe fn`).

## Goal

The real devices behind `secs-audio`'s seam, the OS queries the other seams
need, and a probe the maintainer runs once on real hardware.

## Interface

- `secs_sys::audio::default_backend() -> Box<dyn secs_audio::AudioBackend>`:
  WASAPI on Windows (no ASIO, DirectSound or MME), cpal elsewhere.
  `share_mode` behaviour as ADR 0005: the default tries exclusive and falls
  back to shared with auto-conversion; a pinned mode opens without fallback
  and reports the refusal as such.
- `secs_sys::process::OsProcessProbe: secs_rew::ProcessProbe` (whether REW is
  running) and `secs_sys::memory::OsMemoryProbe: secs_compute::MemoryProbe`,
  through a cross-platform safe crate (for example `sysinfo`; check its
  licence passes `cargo deny`). Keep the dependency set small.
- `crates/secs-sys/examples/hardware_probe.rs`: ported from Impulcifer's, plus
  one SECS measurement: list endpoints, probe formats in both share modes,
  then (with `--output`, `--input`, `--channel` arguments) run
  `secs_audio::measure_position` for one role and a loopback if given, and
  print a Markdown report (devices, formats, mode opened, latency, SNR,
  dropouts) the maintainer pastes into `docs/HARDWARE.md`. Without arguments
  it only lists and probes.
- `docs/HARDWARE.md`: a template with the report sections and a short how-to
  (in Korean, 합니다체, like the other docs).

Dependencies of `secs-sys` stay OS-specific behind `cfg(target_os)` as in
Impulcifer. `secs-sys` may depend on `secs-audio`, `secs-rew`,
`secs-compute` (for the traits). Nothing but `apps/*` depends on `secs-sys`
(the policy gate checks it).

## Tests

- Ported Impulcifer tests that run without a device (format negotiation,
  policy, error mapping, conversions).
- Without a device, `default_backend().enumerate()` returns `Ok` (possibly
  empty) on Linux CI; no test opens a stream.
- The probes return plausible values (total memory above zero; a process list
  that contains the test process itself).
- `cargo clippy --target x86_64-pc-windows-msvc` passes for `secs-sys`
  (the WASAPI path must compile on Windows; the CI runs Windows tests).

`features.toml`: `measure.native` stays `partial` (Claude sets it when the
probe has run on real hardware); add `sys.audio_backends` and
`sys.os_probes` (M4) as `implemented` with your tests, and
`sys.hardware_probe` as `partial` (the example builds; the real run is the
maintainer's).

## Files you may change

- `crates/secs-sys/**`, `docs/HARDWARE.md` (new), `docs/THIRD_PARTY.md` (rows)
- `Cargo.toml` (workspace dependencies), `Cargo.lock` only as cargo rewrites it,
  `deny.toml` only if a licence needs an exception (say why)
- `features.toml` (the entries above)

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p24
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p24
cargo fmt --all -- --check
cargo clippy -p secs-sys --all-targets --locked -- -D warnings
cargo clippy -p secs-sys --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs-sys --locked 2>&1 | grep -E "test result|FAILED|panicked"
cargo build -p secs-sys --example hardware_probe --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-sys
cargo deny --all-features check 2>&1 | tail -3
cargo test -p secs-policy --locked 2>&1 | grep -E "test result|FAILED"
python3 .github/scripts/upstream.py check-tree
```

Do not run `check_feature_tests.py` (Claude runs it in the gate).

## Report

What you ported (files, commit) and wrote, the adapters' behaviour per OS,
the probe's output on this machine (no devices: list and probe only), each
test, and anything you could not do.
