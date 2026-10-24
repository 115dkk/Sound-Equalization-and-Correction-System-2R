# P68 (ASTRA): `secs-dirac`, the Dirac Live `.liveproject` reader

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\dirac-liveproject-support-2e310d`
(branch `claude/dirac-liveproject-support-2e310d`). Work only there. Do not
commit, do not push, do not spawn sub-agents. Claude reviews the diff and
commits. Another worker edits `apps/secs2r/ui/src/i18n/` at the same time;
do not touch it.

Read first: `docs/DIRAC_LIVEPROJECT.md` (the format, reverse-engineered by
Claude; it is the specification), `docs/adr/0013-dirac-liveproject-import.md`,
`docs/adr/0002-*` (unsafe policy), `CLAUDE.md`, and `crates/secs-mdat` as the
house style for a parser of untrusted files (crate layout, `Limits`, error
enum, tests). The sample `.liveproject` files hold personal data and are not
in the repository; build every test input with the synthetic writer below.

## Goal

A new crate `crates/secs-dirac` that parses `.liveproject` bytes into the
measured impulse responses (field 12 of the measurement session), the output
channel list (field 30) and a few header strings, without panicking on any
input and with bounded allocation. A fuzz target and a committed seed file.
P69 (Daybreak) puts it behind the mdat import flow of `secs-service`.

## Design (fixed)

`crates/secs-dirac/Cargo.toml`: workspace package fields as `secs-mdat`,
`[lints] workspace = true`, `[features] test-support = []` (the name
`secs-engine` uses), dependency
`thiserror.workspace = true` only. Add
`secs-dirac = { path = "crates/secs-dirac" }` to `[workspace.dependencies]`
in the root `Cargo.toml` and let Cargo update `Cargo.lock`.

`src/lib.rs` starts with `#![forbid(unsafe_code)]` and
`#![deny(unsafe_op_in_unsafe_fn)]`, then a module doc that names
`docs/DIRAC_LIVEPROJECT.md`. Split into modules as you see fit (`qt.rs` for
the QDataStream cursor, `proto.rs` for the protobuf reader, `test_support.rs`),
but the public API is exactly this:

```rust
pub const MU: f64 = 255.0;
pub const CODE_FULL_SCALE: f64 = 32767.0;
/// Dirac places a position's first arrival about 250 ms into each stored IR.
pub const NOMINAL_ARRIVAL_S: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_file_bytes: usize,    // default 256 MiB
    pub max_string_bytes: usize,  // default 1 MiB, per QString or protobuf string
    pub max_channels: usize,      // default 64
    pub max_positions: usize,     // default 64
    pub max_ir_samples: usize,    // default 1 << 20, per IR
    pub max_total_samples: usize, // default 1 << 24, all IRs together
}
impl Default for Limits { /* the defaults above */ }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind { Main, Subwoofer, Other(u64) } // field 30.2: 1, 4, anything else

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel { pub name: String, pub kind: ChannelKind }

#[derive(Debug, Clone, PartialEq)]
pub struct LiveProject {
    pub format_version: u32,   // header u32 #1
    pub name: String,          // header QString #2
    pub application: String,   // header QString #3
    pub microphone: String,    // header QString #6
    pub sample_rate: u32,      // field 12.1
    pub channels: Vec<Channel>,
    pub positions: usize,      // field 12.3
    responses: Vec<Option<Vec<f64>>>, // position-major, len = positions * channels.len()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured<'a> { pub position: usize, pub channel: usize, pub samples: &'a [f64] }

impl LiveProject {
    /// None when out of range or not measured.
    pub fn response(&self, position: usize, channel: usize) -> Option<&[f64]>;
    /// Measured IRs in file order: position by position, channels in list order.
    pub fn measured(&self) -> impl Iterator<Item = Measured<'_>>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiracError {
    #[error("truncated {0}")]
    Truncated(&'static str),
    #[error("{0}")]
    Invalid(String),
    #[error("limit exceeded: {0}")]
    Limit(String),
    #[error("the project holds no measured impulse responses")]
    NoMeasurements,
}

pub fn parse(bytes: &[u8], limits: &Limits) -> Result<LiveProject, DiracError>;

/// One stored code to a linear sample:
/// scale * 32767 * sign(q) * ((1 + MU)^(|q| / 32767) - 1) / MU.
/// Use `code.unsigned_abs()`; -32768 decodes slightly past full scale.
pub fn decode_code(code: i16, scale: f32) -> f64;
```

Parsing rules (the format doc has the layout):

- `bytes.len() > max_file_bytes` is `Limit`. Every read is bounds-checked on
  a cursor; running out is `Truncated("<what>")`. No `unwrap`, no indexing
  that can panic, no arithmetic that can overflow (`checked_*`).
- Header: u32 BE format version (any value), six QStrings, u32 (ignored).
  QString: u32 BE byte length, `0xFFFFFFFF` is an empty string, odd length is
  `Invalid`, length over `max_string_bytes` is `Limit`, text decoded with
  `String::from_utf16_lossy`. The user id, the second UUID and the
  calibration file path are read past and not kept.
- Then a bool byte: 0 is `NoMeasurements`, 1 continues, anything else is
  `Invalid`. Then u32 BE length: `0xFFFFFFFF` is `NoMeasurements`; the body
  must fit. Nothing after the body is read (the opaque blob and processor
  state are ignored, so trailing bytes are fine).
- Protobuf reader: wire types 0 (varint), 1 (8 bytes), 2 (length-delimited),
  5 (4 bytes) are skipped or read; 3, 4, 6, 7 are `Invalid`; field number 0 is
  `Invalid`; a varint longer than 10 bytes or overflowing u64 is `Invalid`; a
  length past the end of its parent is `Truncated`. Unknown fields are
  skipped at every level. Read only what is needed; field 50 (tens of MB of
  recordings) must be skipped without copying.
- Session: field 12 exactly once (none is `NoMeasurements`, more is
  `Invalid`); field 30 repeated, in order. Field 30 entry: 1 string (UTF-8,
  lossy, `max_string_bytes`), 2 varint kind (missing is `Other(0)`).
- IR set (field 12): 1 rate, 2 channels, 3 positions must all be present and
  nonzero (`Invalid` otherwise); rate at most 1,000,000; channels and
  positions within the limits (`Limit`). Field 4 must occur exactly
  `channels * positions` times (`Invalid` otherwise).
- IR entry (field 12.4): 1 rate (if present it must equal the set rate),
  5 fixed32 scale as `f32::from_le_bytes`, 6 bytes. Field 6 absent or empty:
  not measured (`None`). Field 6 with odd length, or present without field 5,
  or a scale that is negative or not finite: `Invalid`. Codes are big-endian
  i16. More than `max_ir_samples` codes, or more than `max_total_samples`
  decoded in total: `Limit`.
- Channel list: if the number of field 30 entries equals the IR set's channel
  count, use it; otherwise use `Channel N` (1-based) with `Other(0)` for every
  channel. An empty name becomes `Channel N` too. Duplicate names stay as
  they are (P69 numbers them).
- All slots unmeasured: `NoMeasurements`.
- Decoding is a pure function of the code and scale; the same bytes give the
  same `f64` on every run.

`test_support` module, compiled under `#[cfg(any(test, feature = "test-support"))]`:

```rust
pub struct SyntheticChannel { pub name: String, pub kind: u64 }
pub struct SyntheticProject {
    pub format_version: u32,
    pub name: String,
    pub microphone: String,
    pub sample_rate: u32,
    pub channels: Vec<SyntheticChannel>,
    pub positions: usize,
    pub responses: Vec<Option<Vec<f64>>>, // position-major; None writes field 6 absent
}
impl SyntheticProject {
    /// The committed fuzz seed: two positions, channels "Left" (1) and
    /// "Subwoofer" (4), 64-sample responses, position 2's subwoofer unmeasured.
    pub fn minimal() -> Self;
}
/// Inverse of decode_code for a sample x of a response whose peak is `peak`
/// (> 0): round(32767 * sign(x) * ln(1 + MU*|x|/peak) / ln(1 + MU)), clamped.
pub fn encode_code(x: f64, peak: f64) -> i16;
/// A complete file: header (format_version, name, "Live Desktop",
/// "00000000-0000-0000-0000-000000000000" twice, microphone, ""), u32 0,
/// bool 1 + the session, bool 1 + 32 bytes of 0xA5, QString "3.14.3".
/// The session also writes fields the reader must skip: 1 (string), 3 (rate),
/// 14 and 15 (float bytes), 29 (one entry), 50 (a recording with 4 and a small
/// 13 starting with b"OggS"), 51 (one fixed32 field), 60 (fixed32 and varint
/// fields), and puts field 50 before field 12. Each response is written with
/// scale = (peak / 32767) as f32 and encode_code; an all-zero response writes
/// scale 0 and zero codes.
pub fn write(project: &SyntheticProject) -> Vec<u8>;
```

Committed seed: `crates/secs-dirac/tests/data/minimal.liveproject`, the bytes
of `write(&SyntheticProject::minimal())`.

Example `crates/secs-dirac/examples/dump.rs`: `cargo run -p secs-dirac
--example dump -- <file>` prints name, application, microphone, rate,
positions, each channel (index, name, kind), then one line per measured IR:
position, channel, length, index of the largest |x|, that x, and the sum of
x² (`{:.17e}`). Claude compares this with a reference reader.

Fuzz: `fuzz/fuzz_targets/liveproject_parse.rs` in the style of
`mdat_load.rs` (`let _ = secs_dirac::parse(data, &secs_dirac::Limits::default());`),
a `[[bin]] name = "liveproject_parse"` entry and
`secs-dirac = { path = "../crates/secs-dirac" }` in `fuzz/Cargo.toml`, and the
updated `fuzz/Cargo.lock`. Claude edits `.github/workflows/ci.yml`.

## Tests (names fixed; `features.toml` will cite them)

In `crates/secs-dirac` (unit tests or `tests/`):

1. `decodes_mu_law_codes_symmetrically_and_monotonically`: 0 is 0; 32767 is
   `scale * 32767`; `decode(-q) == -decode(q)` for q in 1..=32767; strictly
   increasing over all codes; -32768 finite.
2. `synthetic_project_round_trips_within_code_precision`: a project with
   three channels (one `Other(7)`), three positions, one unmeasured slot,
   responses with a decaying sine and an all-zero response; every header
   string, kind, rate, `positions`, `response(..)` of each slot and the
   `measured()` order match; each sample within
   `1e-4 * |x| + 4e-7 * peak` of the original.
3. `committed_seed_matches_the_writer`.
4. `every_truncation_of_the_seed_is_an_error`: every prefix shorter than the
   end of the session body fails; the full seed parses. Keep it fast enough
   for Miri (the seed is a few hundred bytes).
5. `malformed_protobuf_is_rejected`: field 0, wire types 3, 4, 6, 7, an
   11-byte varint, a length past the end, a nested length past its parent.
6. `ir_set_shape_is_validated`: zero or missing rate, channels, positions;
   wrong number of entries; entry rate mismatch; odd code bytes; codes
   without a scale; scale NaN, infinite, negative; a second field 12.
7. `limits_are_enforced`: each `Limits` field, with a small custom limit.
8. `no_measurements_is_its_own_error`: bool 0; null byte array; no field 12;
   every slot unmeasured.
9. `channel_names_fall_back_when_the_list_does_not_match`: count mismatch and
   an empty name.
10. `parses_dirac_live_sample_projects`, `#[ignore]`: reads every
    `*.liveproject` under the directory in `SECS_DIRAC_SAMPLES` (recursively;
    skip with a message when the variable is unset); each parses, rate 48000,
    every IR has 32769 finite samples, the largest |x| equals `scale * 32767`
    of that IR within 1 ulp-scale tolerance (check through `decode_code`), and
    channel 0's peak index lies in 10_000..13_000 for every measured position.
    Claude runs it against the samples in `<samples>`, a local folder outside
    the repository whose path Claude gives when handing out the packet; you
    may run it too (read only; never copy those files anywhere).

## Gates (run in the foreground, all must pass, then report)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-dirac --all-targets --all-features --locked -- -D warnings
cargo clippy -p secs-dirac --all-targets --locked -- -D warnings
cargo test -p secs-dirac --all-features --locked
cargo test -p secs-policy --locked
cargo +nightly-2026-10-01 miri test --locked -p secs-dirac
SECS_DIRAC_SAMPLES=<samples> cargo test -p secs-dirac --locked -- --ignored --nocapture
cargo run -p secs-dirac --example dump -- "<samples>/1.liveproject"
```

Try `cargo +nightly-2026-10-01 check --manifest-path fuzz/Cargo.toml --bins`;
if libFuzzer cannot build on this Windows machine, say so (CI builds it on
Linux). If an API you use is uncertain, check it instead of guessing.

## Allowed files

`crates/secs-dirac/**` (new), root `Cargo.toml` (one line in
`[workspace.dependencies]`), `Cargo.lock`, `fuzz/Cargo.toml`,
`fuzz/Cargo.lock`, `fuzz/fuzz_targets/liveproject_parse.rs` (new). Nothing
else.

## Report

Files touched; the public API as built if it differs from the above in any
way and why; each gate command with its result (pass/fail and the last lines
on failure); the `dump` output for `1.liveproject`; anything in the format
doc that the code had to interpret.
