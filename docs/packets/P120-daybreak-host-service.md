# P120 (Daybreak): the 관리 service `secs2r-host` (ADR 0018)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\apply-manage`
(branch `claude/apply-manage`, on top of `claude/apply-levels`: P113, P119,
P121, P122 are in). Do not commit, do not push. Work only in this worktree.
Never install, start or stop a real Windows service on this machine, never
write to `%ProgramData%` or the real Equalizer APO folder, and never run
`secs2r-host.exe install`: tests use temp folders, injected paths and the
service logic without the Service Control Manager. The tray shell and the
screen come later (P123 Opus, P124 ASTRA); do not draw anything.

Read first: `CLAUDE.md`, `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`
(all of it, including "관리 단계의 세부"), `docs/adr/0002-unsafe-isolation-and-policy-gates.md`,
`crates/secs-apply/src/**` (`camilla`, `eqapo`, `supervise`),
`crates/secs-sys/src/lib.rs` and its Windows modules,
`crates/secs-service/src/apply.rs` and `lib.rs` (`ApplyAdapter`, `Deps`),
`crates/secs-types/src/ipc.rs` (apply types), `crates/secs-policy/tests/gates.rs`
(`SUBPROCESS_ALLOWLIST`, OS-dependency gate), `apps/secs2r/src/main.rs`,
`apps/secs2r-cli/src/main.rs`, the release and CI workflows (how Windows
binaries are built and bundled: `.github/workflows/release.yml`,
`ci.yml` bundle job, `apps/secs2r/tauri.conf.json`), and the win-custody
README and API (https://github.com/115dkk/Win-Custody.rs, crate
`win-custody` 0.1.1 on docs.rs).

## Design (fixed)

1. **Types** (`secs-types`, regenerate bindings):
   - `AppSettings.apply.camilla` gains `#[serde(default)] exe_path: Option<String>`.
   - `HostRequest { Status, SetManaged(HostManaged), StopManaging, Log { lines: u32 } }`
     and `HostResponse { Status(HostStatus), Ok, Log(Vec<HostLogLine>), Error(IpcError) }`.
   - `HostManaged { camilla: Option<ManagedCamilla>, eqapo: Option<ManagedEqApo> }`,
     `ManagedCamilla { exe_path, config_path, port: u16, design_id }`,
     `ManagedEqApo { config_path, design_id }`.
   - `HostStatus { version, managed: HostManaged, camilla: ManagedProcess (state: running/backing_off{until}/given_up{reason}/stopped/no_session, pid, restarts_in_window), eqapo_include: IncludeState, last_repair: Option<String> }`.
   - `ServiceState { NotInstalled, Stopped, Running, Unreachable }` for the app's view.
2. **Wire format.** One request and one response per connection: a 4-byte
   little-endian length, then UTF-8 JSON; at most 1 MiB; 5 s deadline on
   each side (win-custody `PipeWait`).
3. **`crates/secs-sys/src/host.rs`** (Windows only; non-Windows stubs return
   `ServiceState::NotInstalled` / unsupported):
   - `pipe_server(...)` with `PipeServer` and SDDL `D:P(A;;GA;;;SY)(A;;GRGW;;;IU)`;
     `pipe_call(request) -> Result<HostResponse, IpcError>` client.
   - `launch_camilla(...)`: `PrimaryToken::for_session_user` for the active
     console session, `Launch::as_user`, `spawn_in_job` with
     `kill_on_close`; returns a handle the supervisor can wait on.
   - `service_state() -> ServiceState` (query only, via `windows-service`).
   - `install_service_elevated(exe: &Path)` / `uninstall_service_elevated`
     through the `runas` crate (`secs2r-host.exe install|uninstall`, GUI
     elevation prompt).
   - `watch_config(path, on_change)` with `notify`.
   Add `crates/secs-sys/src/host.rs` to `SUBPROCESS_ALLOWLIST` in
   `crates/secs-policy/tests/gates.rs` with a comment citing ADR 0018.
   `windows-service`, `win-custody`, `notify`, `runas` are `secs-sys`-only
   dependencies (`cargo deny` must pass; say if a licence needs allowing).
4. **`apps/secs2r-host`** (new app crate, `#![forbid(unsafe_code)]`,
   `[lints] workspace = true`, Windows-only binary; on other targets `main`
   prints that it is Windows-only and exits 2):
   - `secs2r-host.exe install` registers the service `SECS2RHost`
     (display "SECS 2R Host", automatic start, LocalSystem) pointing at
     itself with `run`; `uninstall` stops and deletes it; `run` is the
     service entry (`windows-service` dispatcher); `status` prints the pipe
     status for debugging.
   - Service loop: load `%ProgramData%\SECS2R\host.json` (path injectable
     for tests), serve the pipe, supervise CamillaDSP with
     `secs_apply::supervise` (launch when a user session exists; stop on
     logoff, relaunch on logon via session-change notifications), watch the
     managed `config.txt` and repair SECS 2R's two lines with
     `secs_apply::eqapo::with_include` (atomic write, log each repair), keep
     a size-capped `host.log` (for example 256 KiB, then rotate once).
   - Put every rule that does not need Windows (state file round trip,
     request handling, what to do on exit / logon / logoff / file change) in
     a pure module of the app with unit tests; the Windows glue stays thin.
5. **App wiring** (`secs-service` + the two apps, through `Deps` like
   `ApplyAdapter`): when a target's level is `Manage`, `apply_camilla` and
   `apply_eqapo` first do what 적용 does, then send `SetManaged` with the
   result (CamillaDSP: the applied config path, the port from the address,
   `exe_path`, which must be set or the call fails with a typed error).
   `apply_status` gains `host: ServiceState` and, when running, the
   `HostStatus`. New IPC methods `host_install` and `host_uninstall`
   (elevated through secs-sys; refused off Windows) and `host_stop_managing`.
   Register each IPC method in `features.toml`.
6. **CLI.** `secs2r-cli host status|install|uninstall|stop-managing`.
7. **Release.** Build `secs2r-host.exe` for Windows in the release
   installers job and ship it next to `SECS 2R.exe` in the setup and the
   portable zip (follow how `secs2r-cli.exe` is shipped; say exactly what
   you changed). The tray shell comes in P123.

## Tests

- Pure host rules: state file round trip and corrupt-file handling;
  supervise decisions on exit, logon, logoff; repair on file change only
  when the lines are missing or damaged; log rotation.
- Pipe protocol: framing, size limit, deadline (with an in-process fake
  transport; one Windows-only test may open a real pipe under a unique
  name in the test process and must clean up).
- Service wiring: `Manage` level sends `SetManaged` through a fake host
  client; missing `exe_path` fails; `apply_status` reports `host`.
- CLI parsing.

## Files

You may edit: `apps/secs2r-host/**` (new), `crates/secs-sys/**`,
`crates/secs-types/**`, `crates/secs-service/**`, `crates/secs-apply/**`
(only if a rule belongs there; say what), `crates/secs-policy/tests/gates.rs`
(the allowlist line), `apps/secs2r/src/**`, `apps/secs2r-cli/**`,
`apps/secs2r/ui/src/ipc/bindings/**` (generated; add defaults to
`ipc/mock.ts`, `ipc/fixtures.ts` and test fixtures only if typecheck needs
them), workspace `Cargo.toml`, `Cargo.lock`, `fuzz/Cargo.lock` if touched,
`deny.toml` (only if a licence must be allowed), `.github/workflows/release.yml`
and `ci.yml` (bundle/release steps for the new binary), `features.toml`,
`CHANGELOG.md`, `docs/UNSAFE_BOUNDARIES.md` if it lists external unsafe
crates.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --workspace --all-targets --locked
cargo deny --all-features check
python3 -m unittest discover -s .github/scripts -p "test_*.py"
python3 .github/scripts/check_feature_tests.py   # Windows lists one known secs-sys gap; report anything else
(cd apps/secs2r/ui && npm run typecheck && npm test)
```

## Report

The host protocol as built, the secs-sys host API, how the release ships
the binary, the dependency and licence changes, files changed, and the
commands' last lines.

## Addendum: end-to-end check comes next

The maintainer approved this service on 2026-10-29 and requires that it be
proven in CI by driving the real app's GUI on a Windows runner (P125, after
the shell and screen): the runner installs the app and `secs2r-host`,
chooses 관리 in the screen, lets the service launch the pinned CamillaDSP and
checks it, kills CamillaDSP and checks the restart, and removes SECS 2R's
line from an installed Equalizer APO XT's `config.txt` and checks the
repair. Build for that now: every path the service uses (state file, log,
pipe name) must be overridable by an environment variable or flag read only
by `secs2r-host.exe` itself, the service must log each launch, exit,
restart and repair as one machine-readable line, and `secs2r-host status`
must print the status JSON so CI can assert on it.
