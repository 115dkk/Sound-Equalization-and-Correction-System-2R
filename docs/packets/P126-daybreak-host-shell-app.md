# P126 (Daybreak): the host shell's Tauri app (ADR 0018)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\apply-manage`
(branch `claude/apply-manage`). P120 built the service `secs2r-host` and the
`secs_sys::host` API; P123 drew the shell's page `apps/secs2r/ui/host.html`
with `apps/secs2r/ui/src/host/api.ts` (`HostShellApi` and its Tauri
implementation). Do not commit, do not push. Never install, start or stop a
real Windows service on this machine, never write to `%ProgramData%`,
`%ProgramFiles%` or `HKLM`, never run `secs2r-host.exe install` or the
elevated paths, and never write the real `HKCU\...\Run` key: tests use fakes
and temp folders. Do not touch the screens (`apps/secs2r/ui/src/**` except
reading `src/host/api.ts`), and do not draw anything.

Read first: `CLAUDE.md`, `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`
(all, including "관리 단계의 세부"), `docs/packets/P120-daybreak-host-service.md`,
`docs/packets/P123-opus-manage-screen-and-shell.md`, `crates/secs-sys/src/host.rs`,
`apps/secs2r-host/src/**`, `apps/secs2r/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,src/main.rs,src/apply.rs}`,
`apps/secs2r/ui/src/host/api.ts`, `apps/secs2r/ui/vite.config.*`,
`.github/workflows/{ci.yml,release.yml}` (the bundle and installers jobs),
`.github/scripts/collect_bundles.py` and its test,
`crates/secs-policy/tests/gates.rs` (subprocess and OS-dependency gates),
`features.toml`. Check Tauri 2 APIs you are unsure of on docs.rs
(`tauri` 2.12: `tray::TrayIconBuilder`, `menu::{Menu, MenuItem}`,
`WebviewWindowBuilder`, `WindowEvent::CloseRequested`), the single-instance
plugin (https://v2.tauri.app/plugin/single-instance/) and the NSIS hooks
(https://v2.tauri.app/distribute/windows-installer/#build-hooks); fetch them
rather than guessing.

## What it is for

A small tray app that shows what the 관리 service is doing even when SECS 2R
is closed, and installs, removes and switches off the service. It starts at
logon (hidden, tray only) once the service is installed.

## Design (fixed)

1. **Crate `apps/secs2r-host-shell`** (Tauri 2 app, package and binary name
   `secs2r-host-shell`, `#![forbid(unsafe_code)]`, `[lints] workspace = true`,
   `windows_subsystem = "windows"` in release like `apps/secs2r`). It builds
   on every OS so the workspace lints and tests it everywhere; off Windows the
   host calls answer `ServiceState::NotInstalled` / unsupported through the
   existing `secs_sys::host` stubs. Dependencies: `tauri` (with `tray-icon`),
   `tauri-build`, `secs-sys`, `secs-types`, `serde`, `serde_json`, and
   `tauri-plugin-single-instance` pinned with `=` like the other plugins.
   Add it to the workspace members.
2. **`tauri.conf.json`**: `productName` "SECS 2R Host", `mainBinaryName`
   `secs2r-host-shell`, identifier `io.github.115dkk.secs2r.hostshell`,
   `build.frontendDist` `../secs2r/ui/dist`, `devUrl` the same dev server as
   the main app, `beforeDevCommand`/`beforeBuildCommand` running the main
   UI's `npm run dev` / `npm run build` with `cwd` `../secs2r/ui`, the main
   app's CSP unchanged, `bundle.active` false (the shell ships inside the
   main installer), icons reused from `../secs2r/icons`. No windows in the
   config: the window is built in code.
3. **Window.** One window, label `main`, URL `host.html`, title "SECS 2R
   Host", 420×560, `min_inner_size` 420×560, centred, created hidden. It is
   shown and focused on tray left click, on the tray menu's first item, on a
   second launch (single-instance callback), and at start unless the process
   was started with `--hidden`. A close request hides the window and keeps
   the process (prevent close); only the tray menu's quit item exits.
   `capabilities/default.json`: window `main`, `core:default` only, plus the
   app commands below (follow how `apps/secs2r` allows `secs_call`).
4. **Tray.** Icon from the app icon, tooltip "SECS 2R Host". Menu with three
   items, ids `open`, `open_app`, `quit`; English defaults "Open",
   "Open SECS 2R", "Quit" until the page sets them. Left click shows the
   window.
5. **Commands** (names and payloads are what `src/host/api.ts` calls; read it
   and match it exactly; if it differs from this list, report it and follow
   the file):
   - `host_status` → `{ service: ServiceState, host: HostStatus | null }`:
     `service_state()`; when `Running`, `pipe_call(HostRequest::Status)`;
     a pipe failure while the SCM says running reports
     `ServiceState::Unreachable` and `host: null`.
   - `host_log { lines: u32 }` → `Vec<HostLogLine>` via `HostRequest::Log`
     (clamp `lines` to 1..=200).
   - `host_install` → `install_service_elevated(<dir of this exe>/secs2r-host.exe)`,
     then turn on the logon start (item 6).
   - `host_uninstall` → `uninstall_service_elevated(...)`, then turn the
     logon start off.
   - `host_stop_managing` → `HostRequest::StopManaging`.
   - `open_app` → start the main app detached: the first of `SECS 2R.exe`,
     `secs2r.exe` that exists next to this exe (the portable zip names it
     `SECS 2R.exe`; check what the NSIS installer names it and say).
   - `tray_labels { open, open_app, quit }` → sets the three menu texts (the
     page calls it after its catalog loads; P124 adds that call).
   Errors are `IpcError` serialised as the command's rejection.
6. **Logon start** (new in `secs_sys::host`, Windows only, stubs elsewhere):
   `set_shell_autostart(exe: Option<&Path>) -> Result<(), IpcError>` writes
   or deletes the value `SECS2RHostShell` under
   `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` as
   `"<exe>" --hidden` (quote the path), with `windows-registry` as the EqAPO
   code does. Put the value string building in a pure function with a test.
   `spawn_detached(exe: &Path, args: &[&str]) -> Result<(), IpcError>` for
   `open_app` and item 7 (this file is already on `SUBPROCESS_ALLOWLIST`;
   the shell's own `src/` must not spawn processes).
7. **Main app wiring** (`apps/secs2r/src/apply.rs`, `NativeManagedHost`):
   after a successful `install`, turn on the logon start for
   `<app dir>/secs2r-host-shell.exe` and start it with `--hidden` if it
   exists; after `uninstall`, turn it off. A missing shell exe is not an
   error (a local `tauri dev` has none).
8. **Pure parts with unit tests** in the shell crate: composing the status
   from (SCM state, pipe result), the `lines` clamp, picking the main app
   exe from a folder listing, the `--hidden` argument check. Put the host
   calls behind a small trait with the `secs_sys` implementation and a fake
   for tests; the Tauri glue stays thin.
9. **Build and ship (Windows only).** The shell embeds the UI, so it is
   built with the Tauri CLI after the UI is built:
   `node ../secs2r/ui/node_modules/@tauri-apps/cli/tauri.js build --ci --no-bundle`
   from `apps/secs2r-host-shell` (output `target/release/secs2r-host-shell.exe`).
   - `release.yml` installers job and `ci.yml` bundle job: a step "Build the
     host shell" after the UI dependencies are installed and before
     `tauri build`, Windows only; add
     `"../../target/release/secs2r-host-shell.exe":"secs2r-host-shell.exe"`
     to the Windows `bundle.resources` override next to the CLI and the
     service.
   - Replace the inline `python -c` zip edits P120 added in both workflows
     with `collect_bundles.py`: on Windows it also packs
     `secs2r-host.exe` and `secs2r-host-shell.exe` into the portable zip
     under `SECS 2R/`, checks both are GUI-subsystem or console as
     appropriate (`secs2r-host.exe` is a service and CLI: console;
     the shell: GUI), and fails when either is missing. Update
     `test_collect_bundles.py`.
   - `ci.yml` lint job: the Windows clippy line also excludes
     `secs2r-host-shell` (its build script compiles Windows resources);
     change the same command in `CLAUDE.md`.
10. **NSIS hooks** (`apps/secs2r/windows/hooks.nsh`, wired through
    `bundle.windows.nsis.installerHooks` in `apps/secs2r/tauri.conf.json`):
    - `NSIS_HOOK_PREINSTALL` and `NSIS_HOOK_PREUNINSTALL`: end a running
      `secs2r-host-shell.exe` (use what Tauri's NSIS template already uses to
      end the app, and say what).
    - `NSIS_HOOK_POSTINSTALL`: if the `SECS2RHostShell` Run value exists,
      start the shell with `--hidden` again.
    - `NSIS_HOOK_PREUNINSTALL`: if `HKLM\SYSTEM\CurrentControlSet\Services\SECS2RHost`
      exists (readable without elevation), run
      `"$INSTDIR\secs2r-host.exe" uninstall` elevated (`ExecShellWait` with
      `runas`), then delete the Run value. Without the service, no UAC
      prompt appears.
    The installer is per-user; nothing here may need elevation except that
    one uninstall.
11. **features.toml**: register the shell (`host.shell` or the naming the
    file uses for apps) pointing at its tests.
12. **CHANGELOG.md**: one line in the top section under the ADR 0018 items.

## Files

You may edit: `apps/secs2r-host-shell/**` (new), `apps/secs2r/src/apply.rs`,
`apps/secs2r/tauri.conf.json` (installer hooks only),
`apps/secs2r/windows/hooks.nsh` (new), `crates/secs-sys/**` (the two new
functions), workspace `Cargo.toml`, `Cargo.lock`, `fuzz/Cargo.lock` if
touched, `.github/workflows/ci.yml` and `release.yml` (the steps above
only), `.github/scripts/collect_bundles.py`, `test_collect_bundles.py`,
`CLAUDE.md` (the Windows clippy line only), `features.toml`, `CHANGELOG.md`.

Do not touch: `apps/secs2r/ui/**`, `apps/secs2r-host/**` (P120's worker owns
it), bindings, other crates.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --exclude secs2r-host-shell --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --workspace --all-targets --locked
cargo deny --all-features check
python3 -m unittest discover -s .github/scripts -p "test_*.py"
python3 .github/scripts/check_feature_tests.py   # Windows lists one known secs-sys gap; report anything else
(cd apps/secs2r/ui && npm run build)
(cd apps/secs2r-host-shell && node ../secs2r/ui/node_modules/@tauri-apps/cli/tauri.js build --ci --no-bundle)
```

The last command must leave `target/release/secs2r-host-shell.exe`; do not
run it beyond `secs2r-host-shell.exe --version` if you add that flag (do
not add a tray to this machine's session for long; if you start it to look,
quit it from the tray and say so).

## Report

The command list as built (names, payloads, results), the tray and window
behaviour, the new `secs_sys::host` functions, how the installer and the
portable zip ship the shell, the NSIS hooks and what ends the shell,
dependency changes, files changed, and the commands' last lines.
