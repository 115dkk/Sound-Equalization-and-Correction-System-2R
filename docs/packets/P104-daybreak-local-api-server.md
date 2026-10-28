# P104 (Daybreak): the local HTTP and WebSocket API

Worktree: `E:\Sound-Equalization-and-Correction-System-2R` (branch
`claude/public-api`, which already holds P99). Work only there. Do not
commit, do not push, do not spawn sub-agents. Claude reviews the diff and
commits.

Read first: `CLAUDE.md`, `docs/adr/0016-public-rust-api-and-local-api.md`
(the decision this packet implements), `docs/adr/0003-stack-and-ipc.md`,
`crates/secs-service/src/lib.rs` (`Deps`, `Host`, `EventSink`, `Service`),
`crates/secs-service/src/ipc.rs` (dispatch), `crates/secs-service/src/jobs.rs`,
`crates/secs-service/src/session.rs`, `crates/secs-service/src/export.rs`,
`crates/secs-types/src/ipc.rs` (`Request`, `Response`, `AppSettings`,
`ErrorCode`, `JobEvent`, the method tests), `apps/secs2r/src/main.rs`,
`apps/secs2r/src/host.rs`, `deny.toml`.

## Why

An external monitoring program wants to read and drive the running app the
way it reads Dirac Live Processor's local REST API: which project is open,
which jobs run, the latest scorecard, the last export; and to import, compute,
export and cancel. ADR 0016 fixes the shape: an opt-in server inside the app,
bound to 127.0.0.1, guarded by a token, taking the same requests as the
screen.

## Rules (fixed)

1. **Types in `secs-types`** (they are published; P105's client reads them).
   - `AppSettings.local_api: LocalApiSettings { enabled: bool, port: u16 }`,
     `#[serde(default)]`, default `{ enabled: false, port: 4712 }`, so
     settings files written before this change still load.
   - New module `secs_types::local_api` with `API_VERSION: u32 = 1`,
     `DEFAULT_PORT: u16 = 4712`, `TOKEN_FILE: &str = "local-api-token"`,
     `CALL_PATH = "/v1/call"`, `STATUS_PATH = "/v1/status"`,
     `EVENTS_PATH = "/v1/events"`; `Status` (below); `ServerEvent`, a tagged
     enum `{"kind": "job", "event": JobEvent}` or
     `{"kind": "changed", "method": "<wire name>"}`.
   - `Status`: `api` (`API_VERSION`), `app_name`, `app_version` (the app
     version from `Deps`), `library_version` (`secs_service::VERSION`),
     `project` (none, or path, name and unsaved-changes flag), `jobs` (what
     the `jobs` request returns), `results` (none, or what the `results`
     request returns, which already carries the scorecard, the MIMO summary
     and the stale flags), `last_export` (none, or kind, folder, MIMO flag,
     mode name and the UTC time as RFC 3339). Reuse existing response types
     rather than copying their fields.
   - A method access table: `pub fn access(method: &str) -> Option<MethodAccess>`
     with `MethodAccess { ui_only: bool, mutates: bool }`, one entry per wire
     name. `ui_only` is true for every request that opens a window or a
     program on the user's screen: the six `dialog_*`, `open_folder`,
     `open_gpu_help`, `rew_launch`, and the two new requests in rule 4.
     `mutates` is true when the handler changes the session, the project,
     the app settings, files on disk, a device, or starts a job; read each
     handler to decide. A test checks that every name in `Request::METHODS`
     has exactly one entry and no entry names an unknown method.
   - `ErrorCode::UiOnly` (wire `ui_only`): the method opens a window and is
     refused by the local API.
   Regenerate the TypeScript bindings (`cargo test -p secs-types`).
2. **Service** (`secs-service`):
   - `Deps.observer: Arc<dyn EventSink>` (default `NoEvents`). `Service::call`
     sends every event of every call to both the caller's sink and the
     observer, in the same order.
   - The session remembers the last successful export (kind, folder, MIMO
     flag, mode name, time from `Deps::clock` converted to UTC).
   - `pub fn Service::status(&self) -> secs_types::local_api::Status`, built
     from the same handlers the `project_info`, `jobs` and `results`
     requests use (a missing project or result is `None`, not an error).
   - `Host` gains `local_api_status(&self) -> Result<LocalApiStatus, IpcError>`
     and `local_api_regenerate_token(&self) -> Result<LocalApiStatus, IpcError>`,
     both defaulting to `NotImplemented` like the other host methods.
3. **Server crate** `crates/secs-local-api` (`publish.workspace = true`,
   `#![forbid(unsafe_code)]`, `[lints] workspace = true`, a crate doc that
   points at ADR 0016). Use `axum` with its WebSocket support on a tokio
   runtime the crate owns (its own thread, two worker threads); check the
   current versions and their licences against `deny.toml` before adding
   them, and add them to `[workspace.dependencies]`.
   - `EventHub`: an `EventSink` the app passes to `Deps.observer`; the
     server's WebSocket clients subscribe to it (a bounded broadcast; a slow
     client loses events and is told so by a close frame, it never blocks
     the service).
   - `pub fn start(config: ServerConfig) -> Result<ServerHandle, LocalApiError>`
     with `ServerConfig { port: u16, token: String, service: Arc<Service>,
     hub: EventHub, on_change: Arc<dyn Fn(&str) + Send + Sync> }`. Binds
     `127.0.0.1:port` only (port 0 picks a free port, for tests);
     `ServerHandle::port()`, `ServerHandle::stop()` (also on drop) which
     waits for the listener to close. A busy port is a typed error the
     screen can show.
   - Every request: `Host` must be `127.0.0.1:<port>` or `localhost:<port>`
     (else 403); any `Origin` header is 403; `Authorization: Bearer <token>`
     must match in constant time (else 401, with `WWW-Authenticate: Bearer`);
     bodies over 1 MiB are 413. Error bodies are the usual envelope
     `{"ok": false, "error": {...}}` so a client parses one shape.
   - `POST /v1/call`: the body is the same JSON the screen sends to
     `secs_call`. Parse it like `Service::call_value`; a `ui_only` method is
     an envelope error `ui_only` (HTTP 200, like every envelope). Run the
     call on a blocking thread. When the call succeeds and the method
     `mutates`, call `on_change(method)` and publish
     `ServerEvent::Changed`.
   - `GET /v1/status`: `Service::status()` as JSON.
   - `GET /v1/events`: WebSocket upgrade (same checks); sends each
     `ServerEvent` as one JSON text frame; ignores client frames except
     close and ping.
   - Token helpers: `token::load_or_create(dir: &Path) -> io::Result<String>`
     (64 lowercase hex characters from 32 random bytes, written atomically
     to `dir/local-api-token`; a file with anything else is replaced) and
     `token::regenerate(dir)`.
4. **App** (`apps/secs2r`): the service is managed as `Arc<Service>` so the
   server can share it. A new module owns the server's lifecycle:
   - at startup, after loading app settings, start the server when
     `local_api.enabled`; after every successful `app_settings_set`, start,
     stop or restart it when `enabled` or `port` changed (the screen keeps
     its own copy of settings, so compare the old and new values);
   - `on_change` emits the Tauri event `secs://external-change` with the
     method name as payload;
   - `TauriHost` implements the two host methods. `LocalApiStatus` (a
     `secs-types` type) holds `enabled`, `port`, `running`, `error` (none or
     a short message, for example the port is busy), `url`
     (`http://127.0.0.1:<port>`), `token` and `token_path`.
   - Two new requests: `local_api_status` and `local_api_token_regenerate`
     (both `ui_only`), registered as `ipc.local_api_status` and
     `ipc.local_api_token_regenerate` in `features.toml`, plus a feature
     entry `app.local_api` (status `implemented`, your tests) with a note
     pointing at ADR 0016.
5. **CLI** keeps working; it does not start a server.

## Tests (new, all must pass)

In `crates/secs-local-api/tests/`, against a server on port 0 with
`Service::new(Deps::default())` and a temporary token:
- missing or wrong token is 401; a `Host` of `example.com:<port>` is 403; a
  request with `Origin: https://example.com` is 403; a 2 MiB body is 413;
- `bootstrap` through `/v1/call` returns the same envelope as
  `Service::call`; `dialog_open_project` returns the `ui_only` error and
  never reaches the host;
- `/v1/status` parses into `Status` with `api == 1`, no project, no export;
- a WebSocket client receives `changed` after a mutating call (for example
  `project_new` in a temporary folder) and the job events of a short job
  (for example a measurement on the mock measurer, or whichever job is
  quickest in `Deps::default()`), in order;
- `stop()` frees the port (a second `start` on the same port succeeds).
In `secs-types`: the access table covers `Request::METHODS` exactly; the old
settings JSON without `local_api` loads with the default.
In `secs-service`: the observer sees every event the caller's sink sees; the
last export is reported after an export of the oracle project.
In `apps/secs2r`: the lifecycle decision (start, stop, restart, nothing) for
each change of `enabled` and `port` is a pure function with a unit test.

## Files

You may edit: `crates/secs-types/**` and the regenerated
`apps/secs2r/ui/src/ipc/bindings/**`, `crates/secs-service/**`, a new
`crates/secs-local-api/**`, `apps/secs2r/src/**`, `apps/secs2r/Cargo.toml`,
the workspace `Cargo.toml` (dependencies only), `Cargo.lock`, `features.toml`,
`crates/secs-policy/tests/publishing.rs` only if the new unpublished crate
must be listed there.

Do not touch: the screen (`apps/secs2r/ui/src/` other than bindings), the
upstream tree, `.github/`, `docs/`, `CHANGELOG.md`, the published crates'
versions and publish metadata from P99.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny --all-features check
cargo publish --workspace --dry-run --locked --allow-dirty
python .github/scripts/check_feature_tests.py
python .github/scripts/upstream.py check-tree
```

and `cargo doc --no-deps -p secs-local-api -p secs-types -p secs-service`
with `RUSTDOCFLAGS=-D warnings`. Check any API you are unsure of (axum,
tokio, WebSocket upgrade, Tauri state and events) in its documentation before
using it.

## Report

Changed and added files; the access table (method, `ui_only`, `mutates`) as a
list; the crates and versions you added with their licences; the tail of each
verification command; anything left undone.
