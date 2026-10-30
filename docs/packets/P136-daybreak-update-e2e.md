# P136 (Daybreak): CI installs an older build and watches it update itself (ADR 0020)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\auto-update`
(branch `claude/auto-update`). Do not commit, do not push. Claude writes the
`ci.yml` job; you write the script it runs.

**Safety on this machine.** The `run` command works only on GitHub's
Windows runners (reuse `manage_e2e.runner_only()`); never run it here,
never install the app or a service here. Unit tests cover the pure parts.

Read first: `CLAUDE.md`, `docs/adr/0020-automatic-updates.md`,
`.github/scripts/manage_e2e.py` (its `WebDriver`, `wait_for`, `ui`,
`uninstall_entries`, `find_installed_app`, `prepare_edge`, `command`,
`powershell`, `render_report`, `runner_only`, the screenshot on failure),
`.github/scripts/updater_manifest.py`, `apps/secs2r/windows/hooks.nsh`,
`apps/secs2r-host/src/main.rs` (`status` subcommand, log path, the
`update_*` log events), and the interface ids in
`docs/packets/P134-opus-update-screens.md`. Import from `manage_e2e`
instead of copying; move a helper into a shared module only if importing
is impossible, and say so.

## The two builds (made by ci.yml)

- **New (B)**: the normal Windows installer of the run, version = the
  workspace version, in the `installers-windows` artifact as
  `SECS2R-0.0.0-ci.<run>-windows-x64-setup.exe` with its `.sig`.
- **Old (A)**: the same sources built as version `0.0.1` (app, shell and
  service), allowed to read an `http://` endpoint, signed with the same
  per-run key, in the artifact `update-old-windows` as
  `SECS2R-0.0.1-windows-x64-setup.exe`.

## `update_e2e.py` (fixed)

Subcommands:

1. `set-version --version V` (pure, tested): in `apps/secs2r/Cargo.toml`,
   `apps/secs2r-host/Cargo.toml` and `apps/secs2r-host-shell/Cargo.toml`
   replace `version.workspace = true` with `version = "V"`; fail when the
   line is not there. ci.yml restores the files afterwards.
2. `run --old DIR --new DIR --version V --asset-version A --report PATH
   --artifacts DIR` (runner only). Steps, each recorded in the report with
   its evidence, a desktop screenshot on failure, and every process it
   starts stopped at the end:
   1. Serve: write `latest.json` for B with `updater_manifest.py latest
      --only windows --version V --asset-version A --previous 0.0.1
      --notes-file <a fixed test note> --base-url http://127.0.0.1:8765/`,
      copy B's installer and `.sig` beside it, and serve that folder with
      `python -m http.server 8765 --bind 127.0.0.1`.
   2. Environment: every process this script starts (installers,
      tauri-driver, the shell) gets `SECS2R_UPDATE_ENDPOINT=http://127.0.0.1:8765/latest.json`,
      so nothing launched by them (the installer's shell launch, the app's
      exit install) reaches the real release server.
   3. Install A silently; require the uninstall entry's `DisplayVersion`
      `0.0.1`. Register the service with the installed
      `secs2r-host.exe install` (the runner is an administrator); require
      `secs2r-host.exe status` to report version `0.0.1`.
   4. App A through tauri-driver (the same driver setup as
      `manage_e2e.py`): wait up to 180 s for `update.badge` with
      `data-state="ready"` and `data-version` = V.
   5. Close the window through WebDriver. Require, within 240 s, that the
      uninstall entry reports V, that no installer process is left, and
      that `%APPDATA%\io.github.115dkk.secs2r\update.json` says `from`
      0.0.1 and `to` V.
   6. Service: within 180 s, `secs2r-host.exe status` reports V, and the
      service log holds `update_staged` and `update_done` for V (the
      relaunched shell B asks for it).
   7. App B through tauri-driver: `update.notice` is shown and names V;
      clicking `update.notice.dismiss` hides it and removes `update.json`.
      Close the app.
   8. Shell path: uninstall B silently (its uninstall string with `/S`),
      install A again, do not start the app; within 300 s the shell alone
      installs B (the uninstall entry reports V) and writes `update.json`.
   9. Clean up: uninstall, stop the server and the driver.
   Write the report (Markdown, like `manage_e2e.py`'s) to `--report` and
   to the job summary when `GITHUB_STEP_SUMMARY` is set.
3. Tests in `test_update_e2e.py`: `set-version` on sample manifests
   (replaced, missing line), the environment builder, the uninstall-entry
   version reader, the `update.json` check, the report rendering, and the
   step order (a fake runner records the steps).

## Files

You may edit: `.github/scripts/update_e2e.py` (new),
`.github/scripts/test_update_e2e.py` (new), and
`.github/scripts/manage_e2e.py` only to make a helper importable (no
behaviour change; its tests must still pass).

## Verification (foreground; do not end the turn before they finish)

```sh
python -m unittest discover -s .github/scripts -p "test_*.py"
python -I .github/scripts/update_e2e.py --help
python -I .github/scripts/update_e2e.py run --help
```

## Report

The steps as built with their waits and evidence, what you import from
`manage_e2e.py`, files changed, and the commands' last lines.
