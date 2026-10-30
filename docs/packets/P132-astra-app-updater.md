# P132 (ASTRA): the app downloads in the background and installs on close (ADR 0020)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\auto-update`
(branch `claude/auto-update`). Do not commit, do not push. Another worker
(P133) edits `apps/secs2r-host/**` and `apps/secs2r-host-shell/**` in the
same worktree at the same time, and P131 edits `.github/scripts/**`; do not
touch those.

Read first: `CLAUDE.md`, `docs/adr/0020-automatic-updates.md` (the whole
design), `apps/secs2r/src/main.rs`, `apps/secs2r/tauri.conf.json`,
`crates/secs-service/src/lib.rs` (`UpdateAdapter`, `NoUpdates`, `Deps`),
`crates/secs-service/src/ipc.rs` (the `Update*` arms),
`crates/secs-types/src/ipc.rs` (`UpdatePolicy`, `UpdatePhase`,
`UpdateInstall`, `UpdateNotice`, `UpdateStatus`, `PendingUpdate`). These
types and the service seam are fixed; do not change them. If one is wrong
for the design, stop and say so in the report.

The plugin is `tauri-plugin-updater` 2.14.0 with `default-features = false,
features = ["rustls-tls", "system-proxy"]` (already in `Cargo.toml` and the
lock file; do not add or change dependencies). Read its source in the cargo
registry (`~/.cargo/registry/src/*/tauri-plugin-updater-2.14.0/`) before
using any API: `UpdaterExt`, `UpdaterBuilder` (`endpoints`, `pubkey`,
`installer_arg`/`installer_args`, `on_before_exit`), `Update` (`version`,
`body`, `raw_json`, `download`, `install`, `download_and_install`), the
Windows install path (which NSIS arguments quiet and passive mode pass,
whether `/R` relaunches, whether it calls `std::process::exit`), the macOS
and AppImage install paths, and how it reads `plugins.updater` from
`tauri.conf.json`. With `rustls-no-provider`, check whether the plugin or
reqwest installs a rustls crypto provider; if neither does, the first check
panics, so install one the way the plugin's docs say (no new dependency;
if that is impossible, stop and report).

## Design (fixed)

1. **Config.** `tauri.conf.json`: `bundle.createUpdaterArtifacts: true`;
   `plugins.updater` with `pubkey` =
   `dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IENCRThGOTI4NTc2RUYyRUUKUldUdThtNVhLUG5veTJ0eHM0dzZ4V1ArNlNRWjRJbzJrVE96NGpESXZIY1VtVHg2SWp1Qk85QloK`,
   `endpoints` = the two URLs in the ADR in that order, and
   `windows.installMode: "quiet"`. Do not grant any `updater:*` permission
   to the webview: only Rust calls the updater.
2. **`apps/secs2r/src/update.rs` (new)**, `TauriUpdates` implementing
   `secs_service::UpdateAdapter`, built in `setup` with the stored
   `AppSettings::updates` and put into `Deps::updates`.
   - State behind one mutex: `phase`, `checked_at`, `notice`, the
     downloaded `Update` with its bytes, the policy.
   - Scheduler on `tauri::async_runtime`: the first check 15 s after start,
     then every 6 h. `off`: no request at all, phase `Off`. `notify`: check
     only, a newer release gives `Available`. `auto`: check, then download,
     then `Ready`. `set_policy` takes effect at once (`off` cancels a
     running download and drops the bytes; a change to `auto` while
     `Available` starts the download).
   - Portable: a file named `portable` next to the running executable means
     a portable copy. A newer release then gives `Portable { version, url }`
     with `url` = `https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R/releases/tag/v<version>`;
     nothing is downloaded.
   - `SECS2R_UPDATE_ENDPOINT`, when set and non-empty, replaces both
     endpoints with that one URL (signature checks unchanged).
   - Downloading: `Downloading { received, total }` from the chunk
     callback. When the download finishes, write `PendingUpdate` (`from` =
     running version, `to`, `notes` = the update's body, `previous` =
     `raw_json["previous"]` when it is a string) to `update.json` in the
     app config dir (atomic write), then set `Ready`. `install` is
     `OnRequest` when the running bundle is a deb (`tauri::utils::platform::bundle_type()`
     or what the plugin uses), otherwise `OnExit`.
   - `check()` runs a check now (also under `off` and `notify`) and returns
     `Checking`; `download()` is valid only from `Available` (otherwise
     `invalid_request`); `restart()` is valid only from `Ready`: set
     `Installing`, install, then start the new version (Windows: the
     installer relaunches it, through the argument the plugin source shows
     for that; macOS and AppImage: `app.restart()` after the files are
     replaced; deb: install, then `app.restart()`).
   - Errors become `Failed { reason }` with a short English reason; the
     next scheduled check retries.
3. **Install on close.** Change `main.rs` from `.run(context)` to
   `.build(context)?.run(|app, event| ...)` (keep every existing builder
   call, the `Destroyed` shutdown and the start-up error message). On
   `RunEvent::Exit`, when the phase is `Ready` with `OnExit`, install
   quietly without relaunching. Nothing is installed while the window is
   still open, so the "save first?" flow and the job shutdown always finish
   first.
4. **Next start.** At setup, read `update.json`. If `to` equals the running
   version, keep it as `notice` (`skipped` = `previous` is set and differs
   from `from`; `release_url` = the tag page of `to`). If `to` differs, the
   install did not happen: delete the file. `dismiss_notice` deletes the
   file and clears `notice`.
5. **Screen wiring** (no markup, no styles):
   `apps/secs2r/ui/src/ipc/mock.ts` serves the five `update_*` methods from
   a scripted state with setters that specimens and tests can call (up to
   date, downloading 40 %, ready on exit, ready on request, available,
   portable, failed, a notice with and without `skipped`);
   `src/ipc/fixtures.ts` and `src/state/effects.test.ts` get
   `updates: "auto"`; `src/ipc/mockWiring.test.ts` covers the new methods;
   a new `src/state/update.ts` exports a `useUpdate()` hook returning the
   status and `check`, `download`, `restart`, `dismiss`, polling
   `update_status` every 60 s and every 1 s while `checking` or
   `downloading`, plus a `setPolicy(policy)` that stores the policy through
   the existing app-settings path. Follow how neighbouring state modules
   call IPC and handle errors.
6. **features.toml**: register `ipc.update_status`, `ipc.update_check`,
   `ipc.update_download`, `ipc.update_restart`, `ipc.update_notice_dismiss`
   and a feature for the updater pointing at the real tests you add.
7. **Tests**: pure decisions in `update.rs` (policy to phase, portable,
   endpoint override, notice from `update.json` including `skipped` and a
   mismatched `to`, `download`/`restart` from the wrong phase) without a
   network or a Tauri runtime; split the pure part from the plugin calls
   for this. UI tests for the hook's polling and actions with the mock.
8. **CHANGELOG.md**: nothing (P134 writes the user-facing lines).

## Files

You may edit: `apps/secs2r/src/update.rs` (new), `apps/secs2r/src/main.rs`,
`apps/secs2r/tauri.conf.json`, `apps/secs2r/capabilities/**` only if a
build error proves it necessary, `apps/secs2r/ui/src/ipc/mock.ts`,
`apps/secs2r/ui/src/ipc/fixtures.ts`, `apps/secs2r/ui/src/ipc/mockWiring.test.ts`,
`apps/secs2r/ui/src/state/update.ts` (new) and its test,
`apps/secs2r/ui/src/state/effects.test.ts`, `features.toml`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy -p secs2r --all-targets --locked -- -D warnings
cargo clippy -p secs2r --all-targets --locked --target x86_64-apple-darwin -- -D warnings
cargo test -p secs2r --locked
python .github/scripts/check_feature_tests.py
cd apps/secs2r/ui && npm run typecheck && npm test && npm run build
```

Never install anything, never run the built app on this machine, and never
contact the real release URLs from tests.

## Report

The plugin facts you read (with file and line), the state rules as built,
the install arguments per platform, the crypto provider answer, files
changed, and the commands' last lines.
