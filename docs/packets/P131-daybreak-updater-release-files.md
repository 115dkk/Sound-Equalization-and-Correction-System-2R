# P131 (Daybreak): the release carries signed updater files and `latest.json` (ADR 0020)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\auto-update`
(branch `claude/auto-update`). Do not commit, do not push. Other workers
edit `apps/**` in the same worktree at the same time; work only in
`.github/scripts/`. Claude writes the workflow changes; you write the
scripts they call.

Read first: `CLAUDE.md`, `docs/adr/0020-automatic-updates.md`,
`.github/scripts/collect_bundles.py` and `test_collect_bundles.py`,
`.github/scripts/release_notes.py` (how the CHANGELOG section is taken),
`.github/workflows/release.yml` (installers and publish jobs, read only).
Read `tauri-plugin-updater` 2.14.0's source in the cargo registry
(`~/.cargo/registry/src/*/tauri-plugin-updater-2.14.0/src/updater.rs`) for
the exact `latest.json` schema it accepts: field names, the `pub_date`
format, how it picks the platform key (`{os}-{arch}` and
`{os}-{arch}-{installer}`), and what the `signature` field holds (the
`.sig` file's content as text). With `bundle.createUpdaterArtifacts: true`
Tauri writes next to each updater bundle a `.sig` file: the NSIS
`*-setup.exe.sig`, the macOS `macos/<product>.app.tar.gz` and `.sig`, the
`appimage/*.AppImage.sig` and `deb/*.deb.sig`. Check those paths in the
`tauri-bundler` source if it is in the registry, otherwise from
https://v2.tauri.app/plugin/updater/ (WebFetch), and say which.

## Design (fixed)

1. **`collect_bundles.py`** keeps every existing asset and adds, under the
   same stable naming:
   - `SECS2R-<v>-windows-x64-setup.exe.sig`
   - `SECS2R-<v>-macos-universal.app.tar.gz` and `.app.tar.gz.sig`
   - `SECS2R-<v>-linux-x86_64.AppImage.sig`, `SECS2R-<v>-linux-amd64.deb.sig`
   A missing `.sig` is an error (CI always builds with a key). The
   portable zip also holds `SECS 2R/portable` (an empty marker file the app
   reads, ADR 0020) and, when `target/release/secs2r-host.json` and
   `secs2r-host.json.sig` exist, both next to `secs2r-host.exe`; when they
   do not exist that is an error on Windows. Update the module docstring's
   asset list and the portable README sentence about updates (it already
   says the portable copy does not update itself; add that it shows when a
   new release is out).
2. **`updater_manifest.py` (new, stdlib only)** with two subcommands:
   - `latest --version V --previous P --notes-file F --assets DIR
     --base-url URL --pub-date RFC3339 --out FILE` writes `latest.json`:
     `version`, `notes` (the file's text), `pub_date`, `previous` (omitted
     when `P` is empty; P132 reads it), and `platforms` with
     `windows-x86_64` and `windows-x86_64-nsis` (setup.exe),
     `darwin-aarch64`, `darwin-x86_64`, `darwin-aarch64-app`,
     `darwin-x86_64-app` (the universal `.app.tar.gz`), `linux-x86_64` and
     `linux-x86_64-appimage` (AppImage), `linux-x86_64-deb` (deb); adjust
     the key list to exactly what the plugin source accepts and say so.
     Each entry is `{"signature": <.sig text>, "url": URL + asset name}`.
     Every asset and signature must exist in `DIR`; the version in each
     asset name must equal `V`.
   - `host --exe PATH --version V --out FILE` writes `secs2r-host.json`:
     `{"version": V, "sha256": <lowercase hex of PATH>, "file":
     "secs2r-host.exe"}` with sorted keys and a trailing newline.
3. **Tests** in `test_updater_manifest.py` and `test_collect_bundles.py`:
   the platform map, a missing asset or signature, a version mismatch,
   `previous` omitted and present, the host manifest bytes, the new assets
   and the portable marker and manifest files.

## Files

You may edit: `.github/scripts/collect_bundles.py`,
`.github/scripts/test_collect_bundles.py`,
`.github/scripts/updater_manifest.py` (new),
`.github/scripts/test_updater_manifest.py` (new).

## Verification (foreground; do not end the turn before they finish)

```sh
python -m unittest discover -s .github/scripts -p "test_*.py"
python -I .github/scripts/updater_manifest.py --help
python -I .github/scripts/updater_manifest.py latest --help
python -I .github/scripts/updater_manifest.py host --help
```

## Report

The plugin's `latest.json` rules and the bundler's `.sig` paths (with the
source), the platform keys written, files changed, and the commands' last
lines.
