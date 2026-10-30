# P113 (Daybreak): the `secs-apply` crate (ADR 0018, pure part)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\apply-levels`
(branch `claude/apply-levels`). Do not commit, do not push. Work only in this
worktree.

Read first: `CLAUDE.md`, `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`,
`docs/adr/0002-unsafe-isolation-and-policy-gates.md`, an existing small crate
for layout (`crates/secs-jos/Cargo.toml` and `src/lib.rs`), the workspace
`Cargo.toml`, `crates/secs-policy/` (the gates a new crate must pass),
`deny.toml`, and the CamillaDSP websocket reference for the version CI uses:
https://github.com/HEnquist/camilladsp/blob/v4.1.3/websocket.md (and the
v3.0.0 copy, to see which commands 3.x lacks).

## What it is for

ADR 0018 lets the app push a design into a running CamillaDSP over its
websocket, and add or remove SECS 2R's include line in Equalizer APO's
`config.txt`; a Windows service later keeps CamillaDSP running. This packet
builds the part that needs no operating system: protocol, text editing and
the restart rule. Later packets wire it into `secs-service` and the service.

## Crate (fixed)

New workspace member `crates/secs-apply`, `publish = false`,
`#![forbid(unsafe_code)]`, `[lints] workspace = true`, no OS dependencies.
Dependencies: `serde`, `serde_json`, `thiserror` (workspace versions), and
`tungstenite` with `default-features = false` (no TLS; plain `ws://`). Add it
to the workspace `[workspace.dependencies]` and members; `cargo deny` must
pass.

```rust
pub mod camilla;
pub mod eqapo;
pub mod supervise;

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("connect {addr}: {message}")] Connect { addr: String, message: String },
    #[error("websocket: {0}")] Transport(String),
    #[error("{command}: {message}")] Rejected { command: String, message: String },
    #[error("{command}: unexpected reply {reply}")] Reply { command: String, reply: String },
    #[error("{command}: not supported by this CamillaDSP")] Unsupported { command: String },
}
```

### `camilla`

```rust
pub enum Command {
    GetVersion, GetState, GetStopReason, GetConfigTitle, GetConfigFilePath,
    GetCaptureRate, GetClippedSamples, GetProcessingLoad, GetSignalLevels,
    ValidateConfig(String), SetConfigFilePath(String), Reload,
}
impl Command {
    pub fn name(&self) -> &'static str;
    /// The text frame CamillaDSP expects: `"GetState"` for commands without
    /// an argument, `{"SetConfigFilePath": "..."}` with one.
    pub fn to_message(&self) -> String;
}
/// The `value` of `{"<name>": {"result": "Ok", "value": ...}}`; `Rejected`
/// for `"result": "Error"` (with the message CamillaDSP gave), `Unsupported`
/// for CamillaDSP's reply to an unknown command, `Reply` otherwise.
pub fn parse_reply(command: &Command, text: &str) -> Result<serde_json::Value, ApplyError>;

pub trait Transport {
    /// Send one text frame and return the next text frame (CamillaDSP answers
    /// every command with exactly one).
    fn request(&mut self, message: &str) -> Result<String, ApplyError>;
}
pub struct WsTransport { /* tungstenite::WebSocket<std::net::TcpStream> */ }
impl WsTransport {
    /// `addr` is `host:port`; connect, read and write all time out after `timeout`.
    pub fn connect(addr: &str, timeout: std::time::Duration) -> Result<Self, ApplyError>;
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CamillaStatus {
    pub version: String,
    pub state: String,
    pub stop_reason: Option<String>,
    pub title: Option<String>,
    pub config_path: Option<String>,
    pub capture_rate: Option<u32>,
    pub clipped_samples: Option<u64>,
    pub processing_load_pct: Option<f64>,
    pub playback_peak_db: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ApplyOutcome {
    Applied { previous_path: Option<String> },
    /// Reload failed; the previous path was set back and reloaded.
    RolledBack { error: String, previous_path: Option<String> },
}

pub struct Camilla<T: Transport> { transport: T }
impl<T: Transport> Camilla<T> {
    pub fn new(transport: T) -> Self;
    /// Every field CamillaDSP 3.0+ answers; a command the running version
    /// does not know leaves its field `None` instead of failing.
    pub fn status(&mut self) -> Result<CamillaStatus, ApplyError>;
    /// ValidateConfig(config_text) → GetConfigFilePath → SetConfigFilePath(path)
    /// → Reload. A rejected validation returns `Rejected` and changes nothing.
    /// A failed Reload sets the previous path back and reloads it (when there
    /// was one) and returns `RolledBack`.
    pub fn apply(&mut self, config_path: &str, config_text: &str) -> Result<ApplyOutcome, ApplyError>;
}
```

Check the exact reply shapes (including `GetSignalLevels`' fields and what
an unknown command returns) in `websocket.md` and CamillaDSP's source
(`src/socketserver.rs`) for 4.1.3 and 3.0.0; cite the lines in the report.

### `eqapo`

```rust
pub const MARKER: &str = "# SECS2R: managed by SECS 2R (this line and the next)";
pub const INCLUDE: &str = r"Include: SECS2R\SECS.txt";
pub const FOLDER: &str = "SECS2R";
pub const BACKUP: &str = "config.txt.secs2r-backup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeState { Absent, Present, Damaged }

pub fn include_state(config_txt: &str) -> IncludeState;
/// New text with the two lines appended (after a line break when the file
/// does not end with one), or `None` when they are already present and
/// intact. `Damaged` (marker without the include right after it, or several
/// markers) is repaired: every marker line and the include line right after
/// each marker are removed, then the pair is appended once.
pub fn with_include(config_txt: &str) -> Option<String>;
/// Text without SECS 2R's lines, or `None` when there are none.
pub fn without_include(config_txt: &str) -> Option<String>;
```

Rules: keep every other byte (line endings as the file uses them, CRLF or
LF; a UTF-8 BOM if present; trailing whitespace of other lines). An include
line written by hand without the marker is the user's and stays. Matching is
exact on the marker line after trimming trailing whitespace.

### `supervise`

```rust
pub struct RestartPolicy {
    pub delays: Vec<std::time::Duration>,   // 1, 2, 4, 8 s; the last repeats
    pub window: std::time::Duration,        // 300 s
    pub max_failures: usize,                // 5
}
impl Default for RestartPolicy;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision { RestartAfter(std::time::Duration), GiveUp }
pub struct Supervisor { /* failure instants inside the window */ }
impl Supervisor {
    pub fn new(policy: RestartPolicy) -> Self;
    /// Called when the child exits unexpectedly at `now`.
    pub fn on_exit(&mut self, now: std::time::Instant) -> Decision;
    /// Called after the child ran `window` without exiting: forget failures.
    pub fn on_stable(&mut self);
}
```

## Tests

- `camilla`: messages for every command; replies Ok, Error, unknown
  command, malformed; `status` against a fake transport with 4.1.3 replies
  and with 3.0 replies where newer commands are unknown; `apply` success,
  validation rejected (no further command sent), reload failure with
  rollback, reload failure without a previous path. One test runs
  `WsTransport` against an in-process `tungstenite` server on `127.0.0.1:0`
  that answers like CamillaDSP, and one checks the timeout against a server
  that never answers.
- `eqapo`: absent → appended (LF, CRLF, BOM, no final newline); present →
  `None`; damaged variants repaired; removal; a hand-written include without
  marker untouched; round trip `without_include(with_include(x)) == x` for
  files that ended with a newline.
- `supervise`: the delay sequence, give-up at the fifth failure inside 300 s,
  failures older than the window forgotten, `on_stable` resets.
- Register `apply.camilla_ws`, `apply.eqapo_config` and `apply.supervise` in
  `features.toml` (look at how other library features are kinded and named),
  implemented, pointing at the real tests.

## Files

You may edit: `crates/secs-apply/**` (new), workspace `Cargo.toml`,
`Cargo.lock`, `fuzz/Cargo.lock` if the workspace lock change reaches it,
`features.toml`, `deny.toml` only if a new licence must be allowed (say so),
and whatever `crates/secs-policy` requires a new crate to be listed in (say
what). Nothing else.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-apply --all-targets --locked -- -D warnings
cargo test -p secs-apply --locked
cargo test -p secs-policy --locked
cargo deny --all-features check
python3 .github/scripts/check_feature_tests.py
```

## Report

Files changed, the reply shapes you found (with source lines), anything in
`secs-policy` or `deny.toml` you had to change, and the commands' last lines.
