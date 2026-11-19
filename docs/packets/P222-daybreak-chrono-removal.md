# P222 · Daybreak · Remove chrono (RUSTSEC-2026-0543)

Worktree: `E:\wt\chrono` (branch `wip/chrono`, from `origin/main`). Work only
there. Do not commit, push or touch any other worktree.

## Why

chrono was declared unmaintained (RUSTSEC-2026-0543, issued 2026-11-17, no
patched version; the maintainer points at jiff). `deny.toml` carries an
`ignore` entry for it as a stopgap. This packet removes chrono from the
workspace and deletes that entry.

chrono does four things here:

| Use | Where | Needs the OS |
|---|---|---|
| now, UTC, as RFC 3339 | `apps/secs2r-host/src/{main,rules,transitions}.rs` (7), `crates/secs-service/src/update_machine.rs:182` | no |
| now + delay, as RFC 3339 | `apps/secs2r-host/src/transitions.rs:261` | no |
| unix ms as RFC 3339 (millis, `Z`) | `crates/secs-service/src/design_export.rs:169` | no |
| local UTC offset at an instant | `crates/secs-sys/src/clock.rs:10` | yes |

## Design (fixed)

### 1. UTC strings: no dependency, in `crates/secs-service/src/clock.rs`

The module already has `civil_from_days` / `days_from_civil`. Add, and
re-export from `lib.rs` next to `LocalTime` (`pub use clock::{LocalTime, utc_rfc3339};`):

```rust
/// `time` in UTC as chrono's `DateTime::<Utc>::to_rfc3339()` wrote it:
/// `2026-11-19T01:02:03.123456700+00:00`. The fraction is omitted when the
/// nanoseconds are zero, and otherwise has 3, 6 or 9 digits, the fewest that
/// lose nothing.
pub fn utc_rfc3339(time: SystemTime) -> String;

/// `unix_ms` in UTC as `2026-11-19T01:02:03.123Z`; `None` outside years
/// 0000..=9999, which RFC 3339 cannot write.
pub(crate) fn utc_rfc3339_millis(unix_ms: i64) -> Option<String>;
```

- The output of `utc_rfc3339` must be byte-identical to what chrono 0.4.45
  produced for the same instant: the consumers of these strings (host shell,
  screen, log readers) must see no change. Instants before the epoch use floor
  division like `LocalTime::from_system` does. No panics for any `SystemTime`.
- `created_utc` in `design_export.rs` becomes
  `clock::utc_rfc3339_millis(unix_ms).ok_or(DesignExportError::Timestamp)`.
- `update_machine.rs:182` becomes `utc_rfc3339(SystemTime::now())`.
- Host: every `chrono::Utc::now().to_rfc3339()` becomes
  `secs_service::utc_rfc3339(SystemTime::now())`; the back-off `until` becomes
  `utc_rfc3339(SystemTime::now().checked_add(delay).unwrap_or_else(SystemTime::now))`.
  If `secs2r-host` has no direct dependency on `secs-service` in a table where
  it had chrono, add `secs-service.workspace = true` (or the path form the
  neighbours use) there.
- Unit tests in `clock.rs` (write them before removing chrono and confirm
  each expected string against chrono once, then keep them as literals):
  zero nanos, exact millis, exact micros, 100 ns (Windows resolution), 1 ns,
  an instant before the epoch with a fraction, a leap day, year boundary;
  `utc_rfc3339_millis` for 0, a negative value, the last ms of 9999 (`Some`)
  and the first ms of 10000 and of year -1 (`None`).

### 2. Local offset: jiff, only in `crates/secs-sys`

`utc_offset_seconds(time: SystemTime) -> i32` keeps its signature and
meaning (seconds east of UTC at that instant, DST included). Implement with
jiff's system time zone (`jiff::tz::TimeZone::system()` and its offset for a
`jiff::Timestamp`). When `time` is outside jiff's range, return the offset at
the nearest representable instant; never panic.

- Add `jiff` to `[workspace.dependencies]` in the root `Cargo.toml` and
  `jiff.workspace = true` in `crates/secs-sys/Cargo.toml` only. No other crate
  may depend on it.
- Version: the newest stable on crates.io. Check
  <https://crates.io/api/v1/crates/jiff> and <https://docs.rs/jiff> for the
  version, the API names and the feature list. Do not rely on memory.
- `default-features = false` and the smallest feature set that gives: `std`,
  system time zone detection, and time zone data on every OS (Windows has no
  zoneinfo, so it needs the bundled database; Linux and macOS read the
  system's). No serde, no logging.
- The toolchain is pinned in `rust-toolchain.toml`; if the newest jiff does
  not build on it, stop and report instead of changing the toolchain.

### 3. Remove chrono

- Delete `chrono` from the root `Cargo.toml` and from the three crates'
  manifests; `Cargo.lock` must no longer contain `name = "chrono"`.
- Delete the `RUSTSEC-2026-0543` entry and its comment from `deny.toml`
  (leave `ignore = []` out entirely if it becomes empty).
- Add a line under the top (next version) section of `CHANGELOG.md`, in
  Korean, 하십시오체 statement form, matching the neighbouring entries:
  chrono를 걷어내고 현지 시간대 오프셋은 jiff로 읽는다는 내용. No em dash.

## Allowed files

`Cargo.toml`, `Cargo.lock`, `deny.toml`, `CHANGELOG.md`,
`crates/secs-service/{Cargo.toml,src/clock.rs,src/lib.rs,src/design_export.rs,src/update_machine.rs}`,
`crates/secs-sys/{Cargo.toml,src/clock.rs}`,
`apps/secs2r-host/{Cargo.toml,src/main.rs,src/rules.rs,src/transitions.rs}`.
If a policy test (`crates/secs-policy`) or another file must change, stop and
say which and why instead of editing it.

No `unsafe`. No new crate. Do not touch `src/`, `tests/`, `tools/` (upstream
Python tree).

## Verification (run in the foreground, from `E:\wt\chrono`; set
`CARGO_TARGET_DIR=C:\ct\chrono`)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --locked -p secs-service -p secs-sys -p secs2r-host -p secs-policy --no-fail-fast
cargo deny --all-features check
cargo tree --locked --target all -i chrono   # must fail: no such package
cargo tree --locked --target all -i jiff     # only secs-sys above it (plus whatever already pulled another jiff version)
```

Also print, on this machine (Korea, UTC+9), the value of
`secs_sys::clock::local_offset_seconds()` from a throwaway test or example
that you delete afterwards; it must be 32400.

## Report

End your turn only after the commands above have finished. Report: the jiff
version and features chosen and why each feature is needed; the full list of
files changed; each command's result (paste failures verbatim); the offset
printed; anything in this packet that turned out wrong.
