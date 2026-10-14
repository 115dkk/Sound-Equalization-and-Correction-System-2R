# P28 (Daybreak): the host, the level monitor, the status watch, export folders

Worktree: `/home/user/wt/p28` (branch `m5/p28`, from `m5/plan`). Work only
there. Do not commit; Claude reviews the diff and commits. Do not spawn
sub-agents. Build with
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p28`
(yours alone). P26 (config and settings methods) and P27
(`crates/secs-service/src/curves*`) work in parallel in `secs-service`:
put your service code in new files `crates/secs-service/src/{host,monitor,
watch,export_folder}*.rs` and do not edit theirs. Add your request
variants and job event variants to `secs_types::ipc` in one contiguous
block at the end of each enum so the three branches merge cleanly.

Read first: `docs/M5.md` (§2 app frame and job tray, §3 rows 레벨 모니터,
호스트, 측정 경로 감시), `docs/M5-inventory.md` §1 steps 1, 3 and export,
§2 (window, save and open, close, REW watch, busy indicators),
`docs/adr/0003-*` (one IPC command, pushed events), `docs/adr/0002-*` and
`crates/secs-policy` (no shell subprocesses; OS code only in `secs-sys`),
`apps/secs2r/src/main.rs`, `crates/secs-service/src/{lib.rs,jobs.rs,
measurement.rs,export.rs,session.rs}` (`Deps`, `Host`, `EventSink`, job
groups), `crates/secs-sys` (backends and probes), `crates/secs-audio/src/level.rs`,
`crates/secs-rew` (level callbacks, status, `launch.rs`), and upstream
`src/secs/wizard/{step1_devices.py,step3_level.py,mic_meter.py,rew_cache.py,
step8_mdat.py,window.py}`, `src/secs/proc.py`.

## Goal

The desktop app runs the service with real dependencies; the screen can
open the dialogs, folders and live meters upstream has, all through the
one `secs_call` command (ADR 0003): the service forwards dialog requests to
the injected `Host`, and pushes meter and status events on the event sink.

## Interface

- **Host methods** (service forwards to `Host`, the app implements them
  with `tauri-plugin-dialog` and `tauri-plugin-opener`, pinned to exact
  versions that pass `cargo deny`):
  `dialog_open_project`, `dialog_save_project { suggested }`,
  `dialog_pick_mdat`, `dialog_pick_folder { start? }`,
  `dialog_save_file { kind: patch_sheet | target, suggested }`,
  `dialog_open_file { kind: target }` (each returns the chosen path or
  `null` when cancelled), and `open_folder { path }`. `NullHost` keeps
  returning `not_implemented`; a recording test host returns scripted
  answers.
- **REW launch.** `rew_launch` (P23c) calls `Host::launch_rew`. Implement
  it in `secs-sys` by starting REW's executable with the API flag without a
  shell (`std::process::Command` with explicit arguments; find the
  executable as upstream `find_rew_exe` does). If the policy gate
  (`no_shell_subprocesses`) needs an allowlist entry for it, add the
  smallest one and say so in the report; Claude decides.
- **Level monitor** (device job group): `level_monitor_start { role? }`
  starts reading the input through the configured method (native: the
  `secs-audio` level path on the injected backend; REW: its level
  callbacks; mock: the simulated room) and pushes
  `JobEvent::Meter { job, rms_dbfs, peak_dbfs, spl_db?, clipping }` about
  ten times a second; `level_play { role, master_dbfs }` plays pink noise
  (subs: 50 Hz sine) on that role's output for 2.5 s as upstream;
  `level_monitor_stop` (or `job_cancel`) ends it. The readings that
  upstream stores (`extra['levels'][role]`, `extra['spl'][role]`) are
  written when a played role's reading settles.
- **Status watch** (its own group, not the device group):
  `status_watch_start` pushes `JobEvent::MeasureStatus { job, status }`
  (the `measure_methods` result) when it changes, checking every 3 s and
  pausing while a device job runs, as upstream's `RewCache.watch_tick`;
  `status_watch_stop`.
- **Export folders.** `export_folder_get` / `export_folder_set` for
  `extra['export']['folder']` with upstream's default
  (`<project dir>/SECS_export/<YYYY-MM-DD> <stem>/`, or
  `~/Documents/SECS_export/<date>` without a project; the home folder
  comes from the host). The before/after mdat export writes into
  `<folder>/SECS_mdat_<RC_BA|RC_BA_MIMO>_<mode>_<YYYYmmdd-HHMMSS>/` and the
  CamillaDSP export into `<folder>/camilladsp`, as upstream; the clock
  comes from `Deps` so tests are deterministic.
- **App wiring.** `apps/secs2r/src/main.rs` builds `Deps` from `secs-sys`
  (`default_backend`, `OsProcessProbe`, `OsMemoryProbe`), the HTTP REW
  transport, the app settings path from Tauri's app config dir (P26 adds
  the `Deps` field; if it is not there yet, add the field and P26's merge
  will keep one), and a `TauriHost`. Closing the window cancels running
  jobs and waits up to 3 s for the device job, as upstream's close.

## Tests

- Host methods with a recording host: each request reaches the host with
  its parameters, a cancelled dialog returns `null`, `NullHost` returns
  `not_implemented`.
- Level monitor with the mock method: `Meter` events arrive, `level_play`
  on a role writes `extra['levels'][role]`, stop and cancel end the job,
  a measurement cannot start while the monitor runs (`job_busy`).
- Status watch with `RecordedTransport`/`FakeRew`: an event when REW
  appears and when it goes away, none while nothing changes, paused
  during a device job.
- Export folders: the default folder with and without a project, the
  before/after and CamillaDSP subfolders with a fixed clock, stored and
  restored through `extra['export']['folder']`.
- `cargo clippy --target x86_64-pc-windows-msvc` for `secs-sys` and the
  app's host code (the REW launch path must compile on Windows).

`features.toml`: `ipc.<method>` for every method, `service.host`,
`service.level_monitor`, `service.status_watch`, `service.export_folders`
(M5) as `implemented` with their tests.

## Files you may change

- `crates/secs-service/src/{host,monitor,watch,export_folder}*.rs`, the
  lines in `lib.rs`/`ipc.rs`/`export.rs` that declare, dispatch and use
  them, `crates/secs-service/tests/{host,monitor,watch,export_folder}*.rs`
- `crates/secs-sys/**` (REW launch), `apps/secs2r/src/**`,
  `apps/secs2r/Cargo.toml`, `apps/secs2r/tauri.conf.json` and
  `apps/secs2r/capabilities/**` (the plugins' permissions, least needed)
- `crates/secs-types/src/ipc.rs` (your block), `apps/secs2r/ui/src/ipc/bindings/**`
  (regenerated)
- `Cargo.toml` (workspace dependencies), `Cargo.lock` as cargo writes it,
  `features.toml`, `docs/THIRD_PARTY.md` if code is ported

Not the screen beyond the generated bindings.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p28
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p28
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs-service --test host --test monitor --test watch --test export_folder --locked 2>&1 | grep -E "test result|FAILED|panicked"
cargo test -p secs-sys -p secs-types -p secs-policy --locked 2>&1 | grep -E "test result|FAILED|panicked"
cargo build -p secs2r --locked
cargo deny --all-features check 2>&1 | tail -3
(cd apps/secs2r/ui && npm ci --no-audit --no-fund && npm run typecheck && npm test)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-service -p secs-sys -p secs-types
python3 .github/scripts/upstream.py check-tree
```

Do not run `check_feature_tests.py`.

## Report

The methods and events as built, the app wiring, the plugin versions and
permissions, the REW launch and any policy allowlist entry, each test, and
anything you could not do.
