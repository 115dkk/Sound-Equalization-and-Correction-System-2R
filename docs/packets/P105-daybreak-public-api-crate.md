# P105 (Daybreak): the public crate `sound-equalization-and-correction-system-rust-api`

Worktree: `E:\Sound-Equalization-and-Correction-System-2R` (branch
`claude/public-api`, which already holds P99 and P104). Work only there. Do
not commit, do not push, do not spawn sub-agents, never run `cargo publish`
without `--dry-run`. Claude reviews the diff and commits.

Read first: `CLAUDE.md`, `docs/adr/0016-public-rust-api-and-local-api.md`,
`crates/secs-service/src/lib.rs` (`Deps`, `Service`, `EventSink`, `VERSION`,
`upstream()`), `crates/secs-types/src/ipc.rs` (`Request`, `Response`,
`Envelope`, `IpcError`, `JobEvent`), `crates/secs-types/src/local_api.rs`,
`crates/secs-local-api/` (the server you talk to), `crates/secs-rew/src/transport.rs`
(`HttpTransport`), `apps/secs2r-cli/src/main.rs` (how the CLI drives a
calibration and export end to end, which the examples should mirror),
`crates/secs-policy/tests/publishing.rs`.

## Why

ADR 0016: one supported crate on crates.io. A Rust program either embeds the
engine (no app, files only) or drives a running SECS 2R through the local
API, with the same request types either way.

## Rules (fixed)

1. **Crate.** `crates/secs-api`, package name
   `sound-equalization-and-correction-system-rust-api`, library name left to
   cargo (`sound_equalization_and_correction_system_rust_api`), `version =
   "0.1.0"`, publish metadata, `README.md` and `LICENSE` copy as P99's rule
   1 and 2 (the README is the crate's front page, not the "internal crate"
   text), `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`,
   `[lints] workspace = true`. Add it to the published list in the P99 gate.
   Dependencies on published crates are exact `=0.1.0` through
   `[workspace.dependencies]`.
2. **Features.** `default = ["engine"]`; `engine` pulls `secs-service` and
   what it needs; `client` pulls `ureq` (workspace) and `tungstenite` (sync,
   no TLS; check the current version and licence against `deny.toml`). With
   `--no-default-features --features client` the crate builds without
   `secs-service`. Both features together also build.
3. **Surface.**
   - `pub use secs_types as types;` and, at the root, `Request`, `Response`,
     `JobEvent`, `IpcError`, `ErrorCode`.
   - `pub enum Error { Api(IpcError), Transport(String), Protocol(String) }`
     implementing `std::error::Error`, and `pub type Result<T>`.
   - `pub trait Secs { fn call(&self, request: Request) -> Result<Response>; }`
     where an error envelope becomes `Error::Api`.
   - `engine` feature: `Engine` and `EngineBuilder`. The builder sets an
     optional app-settings path, an optional compute preference, an optional
     audio backend factory (`secs_audio::AudioBackend`), the REW transport
     (default `secs_rew::HttpTransport::default()`), and an optional observer;
     `build()` makes a `secs_service::Service` whose `app_version` is this
     crate's version. `Engine::new()` is `EngineBuilder::default().build()`.
     `Engine` implements `Secs`, has `call_json(&self, serde_json::Value) ->
     Envelope` for callers that hold raw JSON, `subscribe(&self) ->
     std::sync::mpsc::Receiver<JobEvent>` fed by an internal observer (every
     event of every call), `wait_job(&self, job_id, timeout) -> Result<JobEvent>`
     returning the job's final event (finished, failed or cancelled; read
     `JobEvent` to know which variants end a job), and `service(&self) ->
     &secs_service::Service` for anything not wrapped.
   - `client` feature: `Client` with `Client::new(port, token)`,
     `Client::from_token_file(port, path)`, `Client::default_token_path()`
     returning the path the app uses on this OS (the Tauri app config dir for
     the app's identifier; read `apps/secs2r/tauri.conf.json` and say how you
     derived it), `call` (implements `Secs`), `call_json`, `status() ->
     Result<types::local_api::Status>`, and `events() -> Result<Events>`
     where `Events` is an iterator of `Result<types::local_api::ServerEvent>`
     over the WebSocket. Every request sends `Authorization: Bearer` and
     `Host: 127.0.0.1:<port>`; the client never sends `Origin`. HTTP errors
     map to `Error::Transport` with the status, envelope errors to
     `Error::Api`.
   - Crate docs: what the crate is, the two features, a table of what is and
     is not in the public API (no native audio measurement, no GPU), the
     licence (GPL-3.0-only) and its consequence for programs that link it, a
     `cargo add sound-equalization-and-correction-system-rust-api --rename secs`
     line, and examples: open a project, run `calibrate`, wait for the job,
     read `results`, `export` CamillaDSP (engine); read `status` and follow
     `events` (client). Examples that need files or a running app are
     `no_run`; everything else runs as a doc test.
4. **README.md** (English first, then a short Korean section with the same
   install line and one example): what SECS 2R is (room correction designed
   from measurements; a Rust and Tauri port of SECS 2), the two features,
   install, the two examples, the version policy (this crate follows its own
   0.x line; internal `secs-*` crates are pinned exactly and carry no
   compatibility promise), the licence and the repository link.

## Tests (new, all must pass)

- `engine`: `Engine::new()` answers `bootstrap` with this crate's version as
  `version`; an unknown method through `call_json` is an `invalid_request`
  envelope; with the oracle project the CLI tests use, `calibrate` then
  `wait_job` then `results` succeeds and `subscribe` saw the job's events in
  order (reuse the test fixtures the CLI or `secs-service` tests already
  use; tests are not packaged, so paths outside the crate are fine there).
- `client` (dev-dependency on `secs-local-api`, path only): against a server
  on port 0 with a temporary token, `status()` parses, `call(Bootstrap)`
  matches the engine's answer, a wrong token is `Error::Transport` with 401,
  and `events()` yields a `changed` event after a mutating call.
- `cargo build -p sound-equalization-and-correction-system-rust-api
  --no-default-features --features client` succeeds and its dependency tree
  (`cargo tree -e normal`) holds no `secs-service`.

## Files

You may edit: a new `crates/secs-api/**`, the workspace `Cargo.toml`
(members are a glob; dependencies only), `Cargo.lock`,
`crates/secs-policy/tests/publishing.rs` (add the crate to the list),
`features.toml` (a new entry `api.public_crate`, kind of your choice among
the existing kinds, status `implemented`, your tests).

Do not touch: other crates' sources (if you need a change there, stop and
report it), the screen, the upstream tree, `.github/`, `docs/`,
`CHANGELOG.md`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p sound-equalization-and-correction-system-rust-api --no-default-features --features client --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p sound-equalization-and-correction-system-rust-api --all-features --locked
cargo deny --all-features check
cargo publish --workspace --dry-run --locked --allow-dirty
python .github/scripts/check_feature_tests.py
```

and `cargo doc --no-deps -p sound-equalization-and-correction-system-rust-api --all-features`
with `RUSTDOCFLAGS=-D warnings`.

## Report

Added files; the public items (one line each); how `default_token_path` is
derived; the packaged file list and size of the crate; the tail of each
verification command; anything left undone.
