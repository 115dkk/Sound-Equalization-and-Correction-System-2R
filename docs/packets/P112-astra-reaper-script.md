# P112 (ASTRA): a REAPER script in the convolution bundle (#105)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\reaper-export`
(branch `claude/reaper-export`, from `main` at v0.31.0). Do not commit, do
not push. Work only in this worktree.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/ARCHITECTURE.md`,
`crates/secs-filters/src/lib.rs` from `struct ConvolutionRoute` to the end
of `export_convolution` (about lines 1270–1460) and its tests (search
`convolution_bundle_contains_complete_paths_and_is_atomic`),
`features.toml` entry `export.convolution`, `CHANGELOG.md` (top section).

## What it is for

Issue #105 (https://m.dcinside.com/board/speakers/582404, comment by
Ecosystem): users who route Windows audio into REAPER (System Audio Bridge)
and run monitoring FX there want to drop SECS 2R's correction in with one
click instead of wiring every IR by hand. The convolution bundle already
writes one WAV per reachable input → output pair, a CSV per WAV, a
`routing.json` and a guide. Add a REAPER ReaScript to the bundle that builds
the routing in the open project.

## Behaviour (fixed)

1. The bundle always contains `secs2r_reaper.lua` beside `routing.json`.
   The guide text gains one paragraph saying what it is and how to run it
   (REAPER: Actions → Show action list → New action → Load ReaScript, then
   run it with the source track selected).
2. Running it in REAPER:
   - The selected track is the source; its channel `k` (1-based) carries the
     input with index `k` in the routing's input order. With no selected
     track the script shows one message box explaining that and stops.
   - It creates a folder track named `SECS 2R <design name or 보정>` right
     after the source, holding one track per routing path and one output bus
     track. A path track receives only its input channel from the source
     (a send, source channel k → path track channel 1, mono), runs the
     path's IR fully wet with no dry signal and no extra gain, and sends to
     the output bus on the output's channel. The output bus has as many
     channels as there are outputs (rounded up to an even count, REAPER's
     rule), and its master send is left on so the user routes it as usual.
     The source track's master send is turned off (the corrected signal
     replaces it); the script says so in its final message.
   - Running it again replaces the folder it made before (find it by a
     project ext state or track ext state key, not by name), so it never
     duplicates.
   - IR paths are resolved relative to the script file's own location, so
     the bundle folder can be moved or copied to another PC.
   - Everything happens in one undo block (`Undo_BeginBlock`/`EndBlock`)
     with UI refresh suppressed while it builds.
3. The convolution plug-in: use REAPER's built-in ReaVerb if its IR can be
   set from a script, with zero-latency mode on, normalisation and any
   automatic gain off, wet 0 dB, dry off, no trimming or stretching. Find out
   how to set ReaVerb's file from ReaScript (its state chunk, a named config
   parameter, or another documented way); cite your sources in the report.
   If no reliable way exists, say so in the report, choose the next best
   built-in option, and explain the choice. Do not depend on third-party
   plug-ins or extensions (SWS, js_ReaScriptAPI).
4. The Lua is generated in Rust from the same data as `routing.json`
   (a pure function beside `convolution_guide`), with every string escaped
   for Lua long-string or quoted-string rules (file names are ASCII today,
   design names may not be). No timestamps, so the bundle stays
   byte-identical for the same design.

## Tests

- Rust unit tests: the script names every routing path's WAV once, with the
  right input channel and output channel; it escapes a design name holding
  quotes, backslashes, newlines and Hangul; bundles stay byte-identical.
- Syntax: if a Lua compiler is available (`luac -p` or `luac5.4 -p`, found
  through an environment variable such as `SECS_LUAC`), a test compiles the
  generated script; without it the test returns early and says so. Add a CI
  step to the existing Linux test job that installs Lua 5.4 from apt and
  sets the variable, so CI always compiles it.
- REAPER itself: look for a local REAPER install (`reaper.exe` under
  `C:\Program Files*`, other drives, or the path in
  `%APPDATA%\REAPER\REAPER.ini`). If you find one, run the script once
  against a scratch project in your scratchpad, from a harness script that
  dumps the resulting tracks, sends, channel counts and FX state to a text
  file and then quits REAPER without saving; put the harness in
  `crates/secs-filters/tests/reaper/` with a README on how to run it. Do not
  touch the user's REAPER settings, projects or other files. If REAPER is
  not installed, write the harness anyway and say it was not run.
- Register `export.convolution_reaper` in `features.toml` (implemented,
  pointing at the real tests) and add a `CHANGELOG.md` entry under the next
  version's section.

## Files

You may edit: `crates/secs-filters/src/**`, `crates/secs-filters/tests/**`,
`.github/workflows/ci.yml` (one step), `features.toml`, `CHANGELOG.md`,
`docs/` (a short `docs/REAPER.md` if useful). Do not touch the screen,
`secs-types`, the upstream Python tree, or other crates' public APIs.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-filters --all-targets --locked -- -D warnings
cargo test -p secs-filters --locked
cargo test -p secs-service --locked convolution
python3 .github/scripts/check_feature_tests.py
```

## Report

How ReaVerb's IR is set (with sources), the generated script for a two-path
example, files changed, whether REAPER ran locally and what the dump showed,
and the commands' last lines.

## Addendum (2026-10-29): run REAPER itself in CI

The maintainer wants CI to install the real REAPER on a runner and prove the
script works there, as the Equalizer APO and CamillaDSP playback jobs do with
their engines. The REAPER binary must never be committed or uploaded as an
artifact; the job downloads it from reaper.fm at run time.

1. **Pin.** `.github/reaper.toml` with the Linux x86_64 build you inspected
   (`version`, `url` on reaper.fm, `sha256`, `format = "tar.xz"`).
   `.github/scripts/reaper_engine.py fetch --dest <dir> --github-env <file>`
   downloads, checks the SHA-256, extracts into `<dir>` without running any
   installer that writes outside it, and exports `SECS_REAPER=<path to the
   reaper executable>`. Unit-test it like `test_eqapo_engine.py` (manifest
   validation, digest mismatch, extraction stays inside `<dir>`).
2. **Run.** An ignored Rust test `crates/secs-filters/tests/reaper_playback.rs`
   (`#[ignore = "REAPER is required; CI runs this test with --ignored"]`):
   - Export convolution bundles from at least two oracle plans: one with
     subs (`computed_dolby`) and one with MIMO supports (`mimo_iir`).
   - Write a multichannel 32-bit float source WAV with one unit impulse per
     input channel at non-overlapping times, and a scratch project (in a temp
     dir) whose one track holds it with the right channel count.
   - Start REAPER headless with an isolated configuration (its own ini or
     resource path in the temp dir; never the runner user's default), under
     `xvfb-run` on Linux, running a harness script that: selects the source
     track, runs the bundle's `secs2r_reaper.lua`, dumps tracks, sends,
     channel counts, ReaVerb state, then renders the output bus (all its
     channels, 32-bit float WAV, the project's 48 kHz) to the temp dir, runs
     the script a second time and dumps again, and quits without saving.
     Fail with REAPER's own output if it does not quit within a timeout.
   - Assert: the second run replaced the folder (same track count, no
     duplicates); every output channel's rendered response to each input
     impulse equals that path's WAV (unity gain, zero added latency) within
     the same tolerance the CamillaDSP playback test uses for float engines,
     and outputs with no path from an input stay silent for that impulse.
3. **CI job** `reaper-playback` in `ci.yml` (ubuntu-latest, `needs: lint`,
   actions pinned by SHA like the others): install the libraries REAPER's
   Linux build needs plus `xvfb`, fetch, run the test with `--ignored
   --nocapture`, and upload only the dumps, rendered WAVs and logs as
   artifacts (`if: always()`), never the REAPER folder. Add
   `secs-filters::<test name>` to `IGNORED_CI_TESTS` in
   `.github/scripts/check_feature_tests.py` as `"reaper-playback"` and point
   `export.convolution_reaper` in `features.toml` at it.
4. **Docs.** Update `crates/secs-filters/tests/reaper/README.md` and
   `docs/REAPER.md` (if you made it) with how CI runs it.

You cannot run CI yourself. Make every part you can check locally pass
(Rust unit parts, Python tests, `cargo test` of the non-ignored tests,
`luac -p` if you can get a Lua compiler without running REAPER); the caller
pushes and runs the CI job, and sends you its log if it fails. Do not run the
REAPER binary on this machine.
