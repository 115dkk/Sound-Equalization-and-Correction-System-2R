# P125 (ASTRA): 관리 proven on a Windows runner by driving the real GUI (ADR 0018)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\apply-manage`
(branch `claude/apply-manage`). P120 built the service `secs2r-host`, P123
the screens (export screen 관리, `host.html`), P126 the tray shell app. Do
not commit, do not push. Never run the service, the installer, the shell,
`tauri-driver` or the driver script against this machine: everything here
runs on a GitHub Windows runner, which is a disposable machine. Locally you
only run the unit tests and the linters below.

The maintainer requires (2026-10-29) that 관리 be proven in CI by operating
the real app's GUI, with the real programs, before the feature counts as
done: "구현하고 나서 그것이 동작하는지는 CI에서 GUI를 직접 조작하며 테스트해야
합니다." Mocks and static checks may be extra layers, never the proof.

Read first: `CLAUDE.md`, `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`
(all, including "관리 단계의 세부"), packets P120, P123, P126 in
`docs/packets/`, `.github/workflows/ci.yml` (jobs `bundle`,
`eqapo-playback`, `camilla-playback`), `.github/camilladsp-engines.toml`,
`.github/eqapo-engines.toml`, `.github/scripts/{camilla_engine.py,eqapo_engine.py,macos_app_check.py,collect_bundles.py}`
and their tests (follow their style: stdlib only, pure functions unit
tested, `argparse` entry), `apps/secs2r/src/self_test.{rs,js}` (how the
self-test drives `secs_call` for a mock-room measurement and a calibration),
`apps/secs2r-host/src/**` (log line format, `status` output, env overrides),
`crates/secs-sys/src/host.rs`, `crates/secs-sys/src/lib.rs` (Equalizer APO
detection reads `HKLM\SOFTWARE\EqualizerAPO` `ConfigPath` and
`InstallPath`), and the screens' `data-ui` attributes in
`apps/secs2r/ui/src/screens/export/**` and `apps/secs2r/ui/src/host/**`.
External, fetch rather than guess: https://v2.tauri.app/develop/tests/webdriver/
(and its CI example), the W3C WebDriver spec sections you use
(https://www.w3.org/TR/webdriver2/), CamillaDSP 4.1.3's README for device
types and the websocket commands
(https://github.com/HEnquist/camilladsp/blob/v4.1.3/README.md and
`websocket.md` at the same tag), and the EqualizerAPO-XT repository
(https://github.com/115dkk/EqualizerAPO-XT) for how its installer writes the
registry and whether it installs silently.

## Design (fixed)

### The job

`ci.yml` gets a job `manage-e2e`, name "관리 end to end (Windows GUI)":

```yaml
manage-e2e:
  name: Manage end to end (Windows GUI)
  needs: bundle
  runs-on: windows-latest
  timeout-minutes: 40
```

Steps, in order:

1. `actions/checkout` (pinned like the others), `actions/setup-python`
   3.13, the pinned toolchain and `Swatinem/rust-cache` (shared key
   `manage-e2e`) only for `cargo install`.
2. `actions/download-artifact` of `installers-windows` (pin the action by
   SHA as the repository does for `upload-artifact`; look up the current
   v-tag's SHA and say which).
3. **Environment report** (`manage_e2e.py env`): the session id of the job's
   process, the active console session id, `query user`, whether the
   process is elevated, `EnableLUA`, the WebView2 runtime version. Written
   to the step summary. If there is no user in the active console session,
   the script fails here with that sentence; no later step can pass
   without one.
4. **Install SECS 2R** from the downloaded `*-setup.exe` silently (`/S`),
   then find the installed folder (per-user install: under
   `%LOCALAPPDATA%\Programs`; read the uninstall registry entry rather than
   guessing) and check that `secs2r-host.exe` and `secs2r-host-shell.exe`
   sit next to the app exe.
5. **CamillaDSP**: `camilla_engine.py fetch --engine windows` (pinned in
   `camilladsp-engines.toml`).
6. **Equalizer APO XT**: install it the way its own installer does if that
   can run silently on the runner; otherwise extract it with
   `eqapo_engine.py fetch --engine xt` and write the same
   `HKLM\SOFTWARE\EqualizerAPO` values (`ConfigPath`, `InstallPath`, and
   anything else the app's detection reads) that XT's installer writes,
   pointing at a runner folder with a `config.txt`. Say which one you did and
   why. `eqapo_engine.py` gains whatever option this needs, with tests.
7. **WebDriver**: `cargo install tauri-driver --locked --version <x>` (the
   newest 2.x release that is at least two weeks old; say which), and
   msedgedriver for the runner's WebView2 version from Microsoft's official
   download, refused unless `Get-AuthenticodeSignature` reports a valid
   Microsoft signature. No third-party driver managers.
8. **The run** (`manage_e2e.py run`), described below.
9. `actions/upload-artifact` `manage-e2e` with `if: always()`: screenshots,
   the driver's step log, the `tauri-driver` log, `host.log`, `host.json`,
   the CamillaDSP configs the app wrote, and `config.txt` before and after.
   Never upload any executable (ours or anyone's): exclude `*.exe`, `*.dll`
   and archives explicitly.
10. Report: the script's markdown report appended to the step summary.

The new job is not in any `needs` of the release (ci.yml only). Add it to
the job list comment at the top of `ci.yml`.

### The driver script `.github/scripts/manage_e2e.py`

Stdlib only. A minimal W3C WebDriver client over `urllib` (new session,
delete session, find element(s) by CSS, click, clear, send keys, element
text and attribute, execute script, screenshot), talking to `tauri-driver`
on `127.0.0.1:4444` with `tauri:options` `{ application, args, webviewOptions }`
as the Tauri guide shows. Every wait polls with a deadline and fails with
the last observed state. Elements are found by `data-ui` attributes (and
`data-state` where the screens expose state), never by visible text, so
catalog changes cannot break the run. If an element the run needs has no
stable `data-ui`, do not edit the screens: list it in the report, and use
the most stable selector available meanwhile.

The run, as one scenario; each numbered check is a named step in the log
and the report, with a screenshot after it:

1. **Base CamillaDSP.** Start the pinned `camilladsp.exe -p 1234 -a 127.0.0.1 base.yml`
   as the runner user, where `base.yml` is a config that runs without any
   audio hardware (CamillaDSP 4 has non-hardware capture and playback
   types; choose from its README, and say what you chose and why). Wait
   until its websocket answers `GetState`. Record its pid.
2. **App and design.** Start a WebDriver session on the installed app.
   Create a project in a runner folder, run the mock-room measurement and a
   calibration through the screen's own command
   (`window.__TAURI_INTERNALS__.invoke("secs_call", ...)` as the self-test
   does). This setup is not the claim; everything after it is done by
   clicking and typing in the GUI.
3. **Settings through the GUI.** Open the export screen. In the CamillaDSP
   row: set the address to `ws://127.0.0.1:1234` (or what the field
   expects), choose 적용 방식 관리, type the CamillaDSP exe path into the
   path field. Check the row's action becomes enabled.
4. **Service install through the GUI.** Click 서비스 설치. Wait until
   `sc query SECS2RHost` reports RUNNING and the service block's state is
   running. Check the registered binary path is under `%ProgramFiles%`
   (P120 copies it there) and not in the per-user install folder.
5. **관리 시작 (takeover).** Click CamillaDSP 관리 시작. Wait for the job to
   finish in the GUI. Check: the base CamillaDSP's pid has exited; `host.log`
   has a `takeover` line and a `launch` line; `secs2r-host.exe status`
   reports camilla running with pid P1 different from the base pid; the
   process P1 runs in the active console session as the runner user; the
   websocket at port 1234 answers and `GetConfigFilePath` is the applied
   config the app wrote; the GUI's service block shows running and P1.
6. **Restart.** End P1 (`taskkill /PID P1 /F`). Wait until status shows a
   new pid P2, `restarts_in_window` at least 1, `host.log` has `exit` and
   `launch` for it, the websocket answers again, and the GUI shows P2.
7. **Equalizer APO 관리 시작.** In the Equalizer APO row choose 관리 and
   click Equalizer APO 관리 시작. Check `config.txt` has SECS 2R's lines.
   Then remove those lines from `config.txt` (as another editor would:
   rewrite the file without them). Wait until the lines are back, `host.log`
   has a `repair` line, status has `last_repair`, and the GUI shows the
   repair. Check the rest of `config.txt` is byte for byte what it was.
8. **Shell.** End the app's session. Start a WebDriver session on the
   installed `secs2r-host-shell.exe` (`host.html`). Check it shows the
   service running, both targets managed, CamillaDSP running with P2, and
   log lines. Click 관리 끄기 and confirm. Check P2 has exited, nothing
   relaunches within 10 s, and status reports nothing managed.
9. **관리 시작 with nothing running.** Start the app again (new session),
   click CamillaDSP 관리 시작 with no CamillaDSP running. Check the job's
   outcome is handed over, and the service launches CamillaDSP with the
   applied config (pid, websocket, `GetConfigFilePath`).
10. **Service removal through the shell.** In the shell click 서비스 제거
    and confirm. Check `sc query SECS2RHost` reports the service does not
    exist, the CamillaDSP it launched has exited, the copy under
    `%ProgramFiles%` is gone, and the logon start value
    `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\SECS2RHostShell`
    is gone.

`manage_e2e.py` subcommands: `env` (step 3) and `run` (step 8), each with
the paths as arguments (installed folder, CamillaDSP exe, Equalizer APO
config folder, tauri-driver and msedgedriver paths, artifact folder,
report file). The state and log paths are the service's defaults
(`%ProgramData%\SECS2R`), because the real installed service runs; read
them, do not override them.

### Unit tests `.github/scripts/test_manage_e2e.py`

The WebDriver client against an in-process fake HTTP server (request
shapes, error handling, deadlines), parsing `secs2r-host status` JSON and
`host.log` lines, the `config.txt` line removal and the byte-for-byte
comparison, the `base.yml` text, choosing the msedgedriver download for a
WebView2 version, the uninstall-registry lookup with a fake reader, and the
report rendering.

### Documentation

ADR 0018, 결과: replace the sentence that says CI does not register the
real service with one saying CI installs the app on a Windows runner,
registers the real service through the GUI, and checks takeover, restart,
repair, switching off and removal with the pinned CamillaDSP and Equalizer
APO XT (Korean, 합니다체, no em-dash). `docs/ARCHITECTURE.md` or wherever
the CI jobs are listed, if they are listed: add the job.

## Files

You may edit: `.github/workflows/ci.yml` (the new job and the list comment
only), `.github/scripts/manage_e2e.py` (new), `test_manage_e2e.py` (new),
`.github/scripts/eqapo_engine.py` and its test (only the option step 6
needs), `.github/scripts/camilla_engine.py` and its test (only if step 5
needs something), `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`
(the one sentence), `docs/ARCHITECTURE.md` (the job list, if any).

Do not touch: Rust, screens, catalogs, `release.yml`, other jobs.

## Verification (foreground; do not end the turn before they finish)

```sh
python3 -m unittest discover -s .github/scripts -p "test_*.py"
python3 -I .github/scripts/manage_e2e.py --help
python3 -I .github/scripts/manage_e2e.py run --help
```

and, if `actionlint` is on PATH, `actionlint .github/workflows/ci.yml`
(say if it is not).

## Report

The job as written (step by step), the CamillaDSP `base.yml` and why, how
Equalizer APO XT is installed or registered, the tauri-driver and
msedgedriver versions and how they are checked, the selectors used for
each GUI step (and any missing `data-ui`), what the script checks at each
of the ten steps, the artifact list, files changed, and the commands' last
lines.
