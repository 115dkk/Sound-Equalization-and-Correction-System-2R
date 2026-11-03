# P167 (Daybreak): the C ABI crate and the Secs2R C# package

Worktree `E:\wt\nuget-package`, branch `claude/nuget-package`. Do not commit, push or
switch branches; Claude reviews the diff, reruns the gates and commits.

Read first: `E:\wt\nuget-package\docs\adr\0032-nuget-package.md` (the decision this
packet implements), `E:\wt\nuget-package\docs\adr\0002*.md` (isolation crates),
`E:\wt\nuget-package\bindings\python\src\lib.rs` (the Python binding over the same API;
copy its request, error and event semantics), `E:\wt\nuget-package\crates\secs-api\src\lib.rs`
(the Rust API you wrap: `Engine`, `EngineBuilder`, `DroppedEventCount`, `Client`, `Events`,
`Error`), `E:\wt\nuget-package\crates\secs-policy\tests\gates.rs` lines 440-610 (the unsafe
gates your crate must pass) and `E:\wt\nuget-package\bindings\python\tests\test_secs2r.py`.

## Files

You may create or edit only:

- `bindings/dotnet/native/**` (the Rust crate `secs-dotnet-sys`)
- `bindings/dotnet/Secs2R/**` (the C# library)
- `bindings/dotnet/tests/Secs2R.Tests/**` (the C# test project)
- `bindings/dotnet/README.md`, `bindings/dotnet/LICENSE`, `bindings/dotnet/.gitignore`
  (and a `nuget.config` under `bindings/dotnet/tests/` if the test project needs one)
- root `Cargo.toml`: only add `"bindings/dotnet/native"` to `[workspace] members`
- `Cargo.lock` (only as Cargo rewrites it)
- `unsafe-budget.toml`: add the one entry below

Claude owns and edits in parallel: `.github/**`, `crates/secs-policy/**`, `docs/**` except
this packet, `CHANGELOG.md`, `features.toml`, `CLAUDE.md`. Do not touch them.

## 1. Crate `secs-dotnet-sys` at `bindings/dotnet/native`

`Cargo.toml`:

- `name = "secs-dotnet-sys"`, `description`, `version = "0.15.3"` (must equal
  `crates/secs-api/Cargo.toml`'s version; the release gate bumps it later),
  `edition.workspace = true`, `rust-version.workspace = true`, `license = "AGPL-3.0-only"`,
  `repository.workspace = true`, `publish = false`.
- `[lib] name = "secs2r_native"`, `crate-type = ["cdylib", "rlib"]` (the rlib lets
  `cargo test` link). The produced files are `secs2r_native.dll`, `libsecs2r_native.so`,
  `libsecs2r_native.dylib`.
- Dependencies: `secs = { package = "sound-equalization-and-correction-system-rust-api",
  path = "../../../crates/secs-api", version = "=0.15.3", default-features = false,
  features = ["engine", "client"] }`, `serde_json.workspace = true`. Nothing OS-facing,
  no GPU. Build dependency: `csbindgen` (latest release; check crates.io).
- No `[lints] workspace = true`. Its own table, exactly these five at `"deny"` plus
  the warn: `[lints.rust] unsafe_code = "deny"`, `unsafe_op_in_unsafe_fn = "deny"`,
  `missing_debug_implementations = "warn"`; `[lints.clippy] undocumented_unsafe_blocks`,
  `multiple_unsafe_ops_per_block`, `missing_safety_doc` all `"deny"` (and `dbg_macro =
  "deny"` like the workspace). Copy the shape of `crates/secs-sys/Cargo.toml`.

`unsafe-budget.toml`, under `[budget]`, after `secs-cuda-sys`:

```toml
# ADR 0032: the C ABI of the Secs2R NuGet package: the exported functions and
# the borrows of the buffers C# owns.
secs-dotnet-sys = 32
```

The gate counts every `unsafe` token in every `.rs` file under `bindings/dotnet/native`
(comments and string literals excluded), tests and `build.rs` included. Expected total:
15 `#[unsafe(no_mangle)]` + 5 `unsafe extern "C" fn` + 6 borrow blocks = 26. Report the
number the gate prints (`cargo test -p secs-policy --locked -- --nocapture no_unsafe_outside_budget`).

### Module layout

- `src/lib.rs`: `#![deny(unsafe_code)]` as a crate attribute (not forbid: the gate
  requires deny for a budgeted root), crate docs pointing at ADR 0032, `mod core;`,
  `#[allow(unsafe_code)] pub mod ffi;`. Nothing else.
- `src/ffi.rs`: the 15 functions and nothing else. Each is a thin shell: validate the
  pointer/length pair with a safe core function, borrow the bytes in one `unsafe` block
  with a `// SAFETY:` comment, then call the safe core. No logic here.
- `src/core.rs` (safe, no `unsafe` token anywhere): the handle tables, the envelopes,
  every behaviour. Unit tests live here in `#[cfg(test)] #[forbid(unsafe_code)] mod tests`
  (or `src/core/tests.rs`) and exercise the core functions directly; they never call
  the pointer-taking extern functions. The FFI path itself is proven by `dotnet test`.
- `build.rs`: `#![deny(unsafe_code)]` (deny, not forbid; it is a budgeted root),
  runs csbindgen (below) and nothing else.
- No `tests/` directory unless you need one; if you add one, its files carry
  `#![forbid(unsafe_code)]`.

### Handles

- One global `AtomicU64` counter starting at 1 hands out every handle; 0 means none.
- Four tables, each `OnceLock<Mutex<HashMap<u64, Arc<T>>>>` (or one struct of them):
  engines (`Arc<secs::Engine>`), clients (`Arc<secs::Client>`), subscriptions, results
  (`Vec<u8>`, the UTF-8 JSON envelope).
- Never hold a table lock while calling into the engine, the client or a blocking
  receive: clone the `Arc` out, drop the guard, then work. Recover poisoned mutexes with
  `unwrap_or_else(|p| p.into_inner())`, as the Python crate does.
- A subscription is `Mutex<Subscription>` where both kinds sit behind an
  `mpsc::Receiver`, so `recv_timeout` serves both:
  - engine: `Engine::subscribe_with_drop_count()` gives `Receiver<JobEvent>` and a
    `DroppedEventCount`;
  - client: `secs::Events` is a blocking WebSocket iterator with no timeout, so
    `secs_client_events` spawns one pump thread that forwards each
    `Result<ServerEvent>` into an `mpsc::channel` and ends when `send` fails or the
    iterator ends. Document at the site that a freed subscription's thread lingers until
    the next event or the socket closes.
- Free functions on an unknown or already freed handle do nothing.

### Envelopes (the only thing a result holds)

Every result is UTF-8 JSON, one of:

```json
{"ok":true,"data":<value>}
{"ok":false,"error":{"code":"...","message":"...","detail":"...","params":{},"retryable":false}}
```

- An API error (`secs::Error::Api(IpcError)`): `code` is the `ErrorCode`'s serde string
  (`project_not_open`, ...), `params`, `retryable`, `detail` from the `IpcError`,
  `message` = `Error::to_string()`. Build it from the engine's `Envelope::Err` as well,
  so engine and client errors look the same.
- `Error::Transport(d)`: `code = "transport_error"`, `Error::Protocol(d)`:
  `code = "protocol_error"`; `detail = d`, `message = to_string()`, `params = {}`,
  `retryable = false`.
- Binding errors: `invalid_handle` (unknown or freed handle, detail names the kind and
  number), `invalid_argument` (a null pointer with a nonzero length, a length above
  `isize::MAX`, a path or token that is not UTF-8, token and token path both given,
  timeout out of range), `invalid_request` (request bytes that are not JSON),
  `internal` (a caught panic; `detail` carries the panic message when it is a string).
- Every extern function body runs inside `std::panic::catch_unwind(AssertUnwindSafe(..))`
  (in core: one `guard` helper). The release profile unwinds (no `panic = "abort"`),
  so this works; a panic must never cross the C ABI.

### The 15 functions

Exact names and C types (`u64` handles, `usize` lengths). "Result" below means: the
function returns a new result handle (never 0) holding an envelope.

| Function | Parameters | Returns | `data` on success |
|---|---|---|---|
| `secs_engine_new` | `app_settings_path: *const u8, app_settings_path_len: usize` | result | `{"handle": N}` |
| `secs_engine_free` | `engine: u64` | nothing | |
| `secs_engine_call` | `engine: u64, request: *const u8, request_len: usize` | result | the `Response` JSON |
| `secs_engine_wait_job` | `engine: u64, job_id: u64, timeout_ms: u64` | result | the `JobEvent` JSON |
| `secs_engine_subscribe` | `engine: u64` | result | `{"handle": N}` |
| `secs_client_new` | `port: u16, token: *const u8, token_len: usize, token_path: *const u8, token_path_len: usize` | result | `{"handle": N}` |
| `secs_client_free` | `client: u64` | nothing | |
| `secs_client_call` | `client: u64, request: *const u8, request_len: usize` | result | the `Response` JSON |
| `secs_client_status` | `client: u64` | result | the `Status` JSON |
| `secs_client_events` | `client: u64` | result | `{"handle": N}` |
| `secs_events_next` | `subscription: u64, timeout_ms: u64` | result | `{"event": <JSON or null>, "closed": bool, "dropped": N}` |
| `secs_events_free` | `subscription: u64` | nothing | |
| `secs_result_len` | `result: u64` | `usize` (0 = unknown result; an envelope is never empty) | |
| `secs_result_take` | `result: u64, buffer: *mut u8, buffer_len: usize` | `i32` status | |
| `secs_result_free` | `result: u64` | nothing | |

Rules:

- Empty path (`len == 0`) in `secs_engine_new` means the default builder; otherwise
  `EngineBuilder::default().app_settings_path(path).build()`.
- `secs_client_new` follows the Python constructor: a token (`token_len > 0`) and a token
  path both given is `invalid_argument`; a token gives `Client::new`; otherwise the path
  (or `Client::default_token_path()` when empty) gives `Client::from_token_file`.
- `secs_events_next`: an event gives `event` = its JSON, `closed = false`; a timeout gives
  `event = null, closed = false`; a disconnected channel gives `event = null,
  closed = true`; a client error item gives an error envelope (and the stream is then
  closed). `dropped` is the engine subscription's `DroppedEventCount::get()`, 0 for a
  client subscription. Convert `timeout_ms` with `Duration::from_millis`.
- `secs_engine_wait_job` maps errors like the Python crate (`secs_error`).
- `secs_result_take` statuses: `0` copied all bytes and removed the result; `1` unknown
  result; `2` `buffer_len` smaller than the result (result kept); `3` invalid buffer (null
  with a nonzero length, or a length above `isize::MAX`; result kept). Expose them as
  `pub const` in `ffi.rs` so csbindgen emits them, or document them in the C# wrapper.

Pointer rules (ADR 0032): a null pointer is accepted only with length 0 (it borrows `&[]`
without touching `from_raw_parts`); a non-null pointer with length 0 also gives `&[]`; a
length above `isize::MAX` is rejected before any borrow. The check is a safe core
function that never dereferences; test it. Pointer-taking functions are
`pub unsafe extern "C" fn` with a `/// # Safety` section stating the one remaining
trust: the pointer and length describe that much readable (for `buffer`, writable)
memory for the duration of the call. Functions without pointers are plain
`pub extern "C" fn`. `secs_client_new` has two pointers, so two borrow blocks (clippy's
`multiple_unsafe_ops_per_block` forbids one block with two borrows).

### csbindgen

`build.rs` generates `bindings/dotnet/Secs2R/NativeMethods.g.cs` from `src/ffi.rs`:
DLL name `secs2r_native`, namespace `Secs2R`, class `NativeMethods`, `internal`
accessibility. Add `cargo:rerun-if-changed` for `src/ffi.rs` and `build.rs`. The file is
committed; regenerating it on an unchanged `ffi.rs` must leave it byte-identical on
every OS (check line endings).

Verify against csbindgen's source or changelog (crates.io / GitHub Cysharp/csbindgen)
that it recognises the edition-2024 `#[unsafe(no_mangle)]` attribute and
`pub unsafe extern "C" fn`. If the release you pick does not, say so in your report and
choose the smallest workaround that keeps the 2024 syntax in `ffi.rs` (for example a
newer csbindgen, or its option that takes every `extern "C"` function regardless of
attributes). After generating, confirm the file declares all 15 functions; report the
count.

### Rust tests (core)

At least: unknown handle to call/free is `invalid_handle` or a no-op as specified; the
pointer check (null + 0 ok, null + 5 rejected, `isize::MAX + 1` rejected); invalid JSON
bytes give `invalid_request`; `{"method":"bootstrap"}` on an engine with a temp
`settings.json` path succeeds and `data.version` equals `env!("CARGO_PKG_VERSION")`;
`{"method":"results"}` on a fresh engine gives `project_not_open` with a non-empty
`message` and `detail`; a result taken with a short buffer gives status 2 and stays;
`events_next` on a fresh engine subscription times out with `event = null, closed =
false, dropped = 0`; a client on a closed port gives `transport_error` within 5 s; a
panic inside `guard` becomes `internal`. Every test that builds an engine passes a path
inside a temp directory; never let a test use the default settings path (it writes the
user's real config directory).

## 2. C# library `bindings/dotnet/Secs2R`

`Secs2R.csproj`:

- `TargetFrameworks` `netstandard2.1;net8.0`, `Nullable` enable, `LangVersion` latest,
  `AllowUnsafeBlocks` true (the generated file and `fixed` need it),
  `GenerateDocumentationFile` true, `TreatWarningsAsErrors` true.
- `netstandard2.1` only: `PackageReference` to `System.Text.Json` and
  `Microsoft.Bcl.AsyncInterfaces` (latest stable 10.0.x; check nuget.org).
- Version: read from `../native/Cargo.toml` with an MSBuild property function (the first
  `^version = "..."` line), so the crate's version is the single source and the release
  gate's bump reaches the package. A `-p:Version=` on the command line may still override.
  Verify `dotnet pack` names the file `Secs2R.0.15.3.nupkg`.
- Package metadata: `PackageId` exactly `Secs2R` (nuget.org's trusted-publishing policy
  matches that name), `Authors` `115dkk`, `PackageLicenseExpression` `AGPL-3.0-only`,
  `PackageReadmeFile` `README.md` (from `bindings/dotnet/README.md`), `LICENSE` packed at
  the package root as a plain file, `RepositoryUrl`
  `https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R`,
  `PackageProjectUrl` the same, `PackageTags` (room correction, audio, dsp, secs),
  `Description` whose first sentence is: "Programs that use this package must be
  distributed under terms compatible with the GNU AGPL-3.0." followed by one sentence on
  what the package is.
- Native assets: a property `Secs2RNativeDir` (default `$(MSBuildThisFileDirectory)../artifacts/native`)
  holding `<rid>/<file>` for `win-x64/secs2r_native.dll`, `linux-x64/libsecs2r_native.so`,
  `osx-x64/libsecs2r_native.dylib`, `osx-arm64/libsecs2r_native.dylib`. Each existing file
  packs to `runtimes/<rid>/native/`. With `-p:Secs2RRequireAllNatives=true` the pack fails
  with a clear error when any of the four is missing (the release uses this). Without
  it, whatever exists is packed (CI packs one OS at a time).
- Also pack a `buildTransitive` or rely on the `runtimes/` convention only; .NET Core
  resolves `runtimes/<rid>/native` itself. Do not add Unity-specific files.

Sources (namespace `Secs2R`, XML docs on every public member):

- `NativeMethods.g.cs`: generated, never hand-edited.
- `Native.cs` (internal static): the only code that touches pointers. Helpers that send
  UTF-8 bytes with `fixed` (an empty array gives a null pointer with length 0, which the
  crate accepts), read a result (`secs_result_len`; 0 throws `InvalidOperationException`;
  a length above `int.MaxValue` throws; `new byte[len]`; `secs_result_take`; a nonzero
  status frees the result and throws), parse the envelope with `JsonDocument`, return
  `data.Clone()` or throw `SecsException`.
- `SecsException : Exception` with `Code`, `Detail`, `Retryable` (and `Message` from the
  base), built from the envelope's `error`.
- `Engine : IDisposable`: `Engine(string? appSettingsPath = null)`,
  `JsonElement Call(JsonElement request)`, `string Call(string requestJson)` (returns the
  `data` JSON text), `JsonElement WaitJob(ulong jobId, TimeSpan timeout)`,
  `IAsyncEnumerable<JsonElement> Events(CancellationToken cancellationToken = default)`,
  `Dispose()`.
- `Client : IDisposable`: `const int DefaultPort = 4712`,
  `Client(int port = DefaultPort, string? token = null, string? tokenPath = null)`
  (port outside 0..65535 throws `ArgumentOutOfRangeException`), `Call` (both overloads),
  `JsonElement Status()`, `Events(CancellationToken)`, `Dispose()`.
- Events: subscribe once, then loop `secs_events_next` with a short timeout (200 ms) on
  a thread-pool thread (`Task.Run`), yield each event, stop on `closed`, honour the
  token between polls (`[EnumeratorCancellation]`), free the subscription in `finally`.
- Dispose: swap the handle to 0 with `Interlocked.Exchange` and free it; a call after
  dispose throws `ObjectDisposedException`. A finalizer that frees the handle is fine
  (an unknown or freed handle is harmless); call `GC.KeepAlive(this)` after native calls.
- `Engine` and `Client` may be used from several threads at once (the crate is
  thread-safe); say so in the docs.

## 3. Test project `bindings/dotnet/tests/Secs2R.Tests`

- `net8.0`, xUnit (pin current versions). It consumes the **packed** `Secs2R` package from
  the local folder `bindings/dotnet/artifacts/packages` (a `nuget.config` with that
  folder plus nuget.org, and `PackageReference Include="Secs2R" Version="..."` with the
  version read from `../../native/Cargo.toml` the same way). It never references the
  library project directly: the point is to test what users install.
- Tests (port of `test_secs2r.py`, plus a real calculation):
  - the package's assembly version / bootstrap `data.version` equals the crate version;
  - `bootstrap` on an engine with a temp settings path succeeds; `Events` can be
    started and cancelled;
  - `results` on a fresh engine throws `SecsException` with `Code == "project_not_open"`
    and non-empty `Message` and `Detail`;
  - a client on a closed port throws `SecsException` `transport_error` within 5 s;
  - **a real calculation**: create a project in a temp folder, fill it with the
    `mock` measurement method, calibrate, and check `results` has a calibration. Find
    the request sequence the CLI uses for `--fill mock` in
    `E:\wt\nuget-package\apps\secs2r-cli\src\main.rs` (around `"mock"`) and the service
    side in `crates/secs-service/src/measurement.rs`; drive the same requests through
    `Engine.Call` and `WaitJob`. Export (CamillaDSP or Equalizer APO) into the temp folder
    and assert a file appeared. If the mock fill needs something the IPC cannot reach,
    stop and report exactly what is missing rather than faking it.
  - a disposed engine throws `ObjectDisposedException`.
- Every test uses a temp directory for settings and projects.

## 4. README, LICENSE, ignore

- `bindings/dotnet/README.md`: the package README for nuget.org, modelled on
  `bindings/python/README.md` (install, embedded engine, running application, events,
  errors, what is not included), English, and the AGPL sentence first.
- `bindings/dotnet/LICENSE`: a byte-identical copy of the root `LICENSE-AGPL`.
- `bindings/dotnet/.gitignore`: `artifacts/`, `bin/`, `obj/`.

## Verify (run each in the foreground; report the output tail of each)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-dotnet-sys --all-targets --locked -- -D warnings
cargo clippy -p secs-dotnet-sys --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs-dotnet-sys --locked
cargo test -p secs-policy --locked -- --nocapture
RUSTDOCFLAGS="-D warnings" cargo doc -p secs-dotnet-sys --no-deps --locked
cargo deny --all-features check
cargo build -p secs-dotnet-sys --release --locked
# copy target/release/secs2r_native.dll to bindings/dotnet/artifacts/native/win-x64/
dotnet pack bindings/dotnet/Secs2R -c Release -o bindings/dotnet/artifacts/packages
dotnet test bindings/dotnet/tests/Secs2R.Tests -c Release
git status --short; git diff --stat
```

This machine has only the .NET 10 SDK; run the net8.0 tests with
`DOTNET_ROLL_FORWARD=Major` set in the environment (do not put roll-forward in the test
project). `cargo deny` must stay clean; if csbindgen pulls a license or duplicate that
`deny.toml` refuses, report it instead of editing `deny.toml`. After the release build,
`git diff --exit-code bindings/dotnet/Secs2R/NativeMethods.g.cs` must pass. Do not run
the desktop app, install anything system-wide, or touch real settings, ProgramData or
the registry.

## Report

1. Files created or changed.
2. The unsafe token count the gate printed, and where each token sits.
3. csbindgen version, what you verified about `#[unsafe(no_mangle)]`, and the number of
   functions in `NativeMethods.g.cs`.
4. The Rust test names (Claude registers them in `features.toml`).
5. The mock-fill request sequence the C# test drives, or exactly what blocked it.
6. Output tails of every verify command, with any failure quoted.
7. Anything in this packet you could not follow, and why.
