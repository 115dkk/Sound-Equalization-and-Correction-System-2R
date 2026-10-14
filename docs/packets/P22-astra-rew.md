# P22 (ASTRA): the REW API client in `secs-rew`

Worktree: `/home/user/wt/p22` (branch `m4/p22`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p22`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

Read first: `docs/M4.md`, `docs/DEVIATIONS.md` (R1–R4), `docs/adr/0002-*`
(no OS dependency here: `#![forbid(unsafe_code)]`, no `windows*`/`libc`),
`crates/secs-rew` (a stub), and upstream `src/secs/rewapi/rew.py`
(`RealREW`, every endpoint it calls and in what order, `_b64_be_f32`,
`parse_progress`, `_level_payload`, the circuit breaker, `RewProRequired`,
`find_rew_exe`, `rew_process_running`, `channel_name_for`, `launch`),
`src/secs/rewapi/callback.py` (`CallbackServer`), `src/secs/rewapi/fake.py`
(`FakeREW`, `fake_plan`), `src/secs/rewapi/interfaces.py` (the protocols
`RewStatus`, `DeviceLister`, `LevelMonitor`, `Measurer`, `IRProvider`, and
`MeasurementResult`, `DeviceInfo`, `progress`, `PHASES`), `src/secs/proc.py`,
and the tests `tests/test_rewapi_real.py` (its `StubSession` holds replies
recorded from a real REW), `test_rewapi_callback.py`, `test_rewapi_fake.py`,
`test_rew_down_fast_fail.py`, `test_rew_progress_snr.py`,
`test_rew_deferred_connect.py`, `test_proc_launch.py`.

## Goal

Everything SECS 2 does with REW, as a library with no screen: status and
version, device and driver lists, the measurement configuration in REW's
required order, running a position, progress and level callbacks, reading
IRs with `t0`, SNR and SPL, cancel, and the fake REW for the tutorial and
tests. P23 (the service) puts it behind the measurement seam.

## Design (fixed)

- **`RewClient<T: Transport>`** with upstream's methods (names in snake_case
  as upstream). `Transport` is a trait with two implementations:
  `HttpTransport` (`ureq` 3, `default-features = false`, no TLS; base
  `http://127.0.0.1:4735`, connect timeout 0.6 s, read 5 s, measurement 180 s,
  as upstream) and `RecordedTransport` (a script of expected requests and
  their replies; a test fails on any request that is not the next expected
  one, method, path and JSON body included).
- **Order of configuration** exactly as upstream (REW rejects some orders;
  `tests/test_rewapi_real.py` records them).
- **IR decoding**: base64 big-endian float32 to `f32`, `sampleRate`, and
  `startTime` as `t0`.
- **Callback server**: a small HTTP/1.1 server on `127.0.0.1` with the
  standard library only (no new server crate), one random path token per
  server (R3), bodies read to the end and capped (bodies over the cap are
  drained and rejected, R3), reply before dispatch as upstream.
- **Errors**: `RewError` variants with codes the screen can translate
  (connection, HTTP status with path, schema, Pro required, cancelled). R1:
  Pro required is decided by status code and path only, never by matching
  'pro' in a body. R2: the circuit breaker applies to status probes, not to
  a running measurement session.
- **`FakeRew`**: upstream's behaviour (timing, progress phases including
  `running`, cancel, SNR) but its IRs come from an `IrSource` the caller
  provides; the crate has no test code and no bundled data (R4).
- **Launch and process checks**: `find_rew_exe` (search the standard install
  locations; filesystem only) and `launch` (spawn with `-api`; `open -a` on
  macOS) here. Whether REW is running needs the OS process list, which lives
  in `secs-sys`: define a `ProcessProbe` trait here and let P24 implement it.
- **Pro status** cannot be read from the API; keep upstream's user hint.

## Tests

- Every behaviour of the upstream tests listed above that does not need Qt,
  as Rust tests named after them, driven by `RecordedTransport` with the
  replies `tests/test_rewapi_real.py` recorded (copy those replies into
  `crates/secs-rew/tests/data/` as JSON; they are recorded protocol data).
- The callback server: random token, wrong path 404, oversized body drained
  and rejected, progress strings parsed as upstream (`"NN% …"`), level
  payloads as upstream (`-180` means none, clip above −1 dBFS).
- R1–R4 each with a test; write them into `docs/DEVIATIONS.md` as **적용**.
- `FakeRew` runs a two-position plan with a supplied `IrSource` and reports
  upstream's phase sequence.

`features.toml`: set `measure.rew_api` to `implemented` with your tests.

## Files you may change

- `crates/secs-rew/**` (keep `#![forbid(unsafe_code)]`, `[lints] workspace = true`)
- `Cargo.toml` (`ureq` and `base64` as workspace dependencies if needed),
  `Cargo.lock` only as cargo rewrites it, `deny.toml` only if a licence of
  a new dependency needs an exception (say why)
- `docs/DEVIATIONS.md` (R1–R4 rows only), `features.toml` (the one entry)

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p22
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p22
cargo fmt --all -- --check
cargo clippy -p secs-rew --all-targets --locked -- -D warnings
cargo test -p secs-rew --locked 2>&1 | grep -E "test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-rew
cargo deny --all-features check 2>&1 | tail -3
python3 .github/scripts/upstream.py check-tree
```

Do not run `check_feature_tests.py` (Claude runs it in the gate).

## Report

The client API as built, the endpoint list with the upstream line each came
from, each test and the upstream test it stands for, the deviations, and
anything you could not do.
