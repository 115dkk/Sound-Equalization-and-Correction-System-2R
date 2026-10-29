# P108 (Daybreak): play the CamillaDSP exports through CamillaDSP itself

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\camilla-playback`
(branch `claude/camilla-playback`, based on the export branch with the
CamillaDSP labels, headroom, bypass steps and device options). Work only
there. Do not commit, do not push, do not spawn sub-agents. Claude reviews
the diff, commits and writes the CI job.

Read first: `CLAUDE.md`, `docs/DEVIATIONS.md` F12, F13, F20,
`docs/EQUALIZER_APO_INTEROP.md`, `docs/adr/0012-resource-optimizations.md`
(the CamillaDSP 4.1.3 impulse check made by hand), the whole
`crates/secs-filters/tests/eqapo_playback.rs` (the pattern to follow: cases,
excitation, response estimate, `REL`/`ABS` bounds and why, report),
`.github/scripts/eqapo_engine.py` and its test, `.github/eqapo-engines.toml`,
`crates/secs-camilla/src/lib.rs` (`CamillaOptions`, devices, `write_config`),
`crates/secs-filters/src/render.rs`, and the CamillaDSP v4 documentation for
the `RawFile` and `File` devices (`extra_samples`, formats, how the process
ends when the capture file ends).

## Why

GitHub issue #107: SECS 2R's Equalizer APO export is already played through
the official Equalizer APO 1.4.2 and EqualizerAPO-XT engines in CI and must
reproduce the designed responses. Users ask whether CamillaDSP gives the same
result. XT is C++ and CamillaDSP is Rust, so float details differ; whether
they agree is a measurable claim. If CamillaDSP deviates, we report it.

## Rules (fixed)

1. **Engine pin.** `.github/camilladsp-engines.toml` with one entry `[linux]`:
   version `4.1.3`, url
   `https://github.com/HEnquist/camilladsp/releases/download/v4.1.3/camilladsp-linux-amd64.tar.gz`,
   sha256 `55f5ec2ed80fcc79a543672f9f89ace4557d290d80584ef31ee0442111bd0b11`,
   format `tar.gz`. A comment says why bumping is deliberate.
2. **Fetcher.** `.github/scripts/camilla_engine.py fetch --engine linux
   --dest <dir> [--github-env <file>]`: download, check the sha256, extract
   the `camilladsp` binary, make it executable, print its path and, with
   `--github-env`, append `SECS_CAMILLADSP=<path>` and
   `SECS_CAMILLADSP_VERSION=<version>`. Reuse what `eqapo_engine.py` already
   does (manifest loading and checks, retrying download, digest) by
   importing it or moving shared parts into a small module; do not copy
   blocks. Unit tests in `.github/scripts/test_camilla_engine.py` with no
   network (manifest checks, digest mismatch, tar member that escapes the
   destination is refused).
3. **Playback test** `crates/secs-camilla/tests/camilla_playback.rs`,
   `#[ignore]`d (CI runs it with `--ignored`), skipping with a clear message
   when `SECS_CAMILLADSP` is unset:
   - Cases: the oracle chains the EqAPO playback test plays (RC only, subs
     with LFE, MIMO IIR, MIMO FIR including the low-rate FIR case). For each,
     write the config the app writes today (all the new options on: design
     identity, headroom, labels, correction and MIMO steps) and its
     `_bypass.yml`, with the capture set to a `RawFile` excitation and the
     playback to a `File` output, both 32-bit float, and enough
     `extra_samples` for the longest tail.
   - Excitation: one impulse per input, far enough apart that the responses
     do not overlap, or the multisine method of `eqapo_playback.rs`; choose
     and justify. Run the real binary (`camilladsp <config>`), with a
     timeout, and read the output file.
   - Reference: for the main config, `plan_responses` of the same plan (the
     analog reference the EqAPO test uses) scaled by the headroom gain
     `10^(-H/20)`; for the bypass config, the plan of the chain without RC
     and without MIMO. Compare every input-to-output pair, 10 Hz to 20 kHz,
     with the `REL`/`ABS` bounds of `eqapo_playback.rs` (do not widen them;
     if CamillaDSP fails them, the test fails and the report says where).
     Also compare against the convolution renderer's impulse responses of the
     same plan (`secs_filters::render::render_paths`), which use the same
     digital filters CamillaDSP should run, and report that error separately:
     it is the closest measure of CamillaDSP's own arithmetic.
   - Report: when `SECS_CAMILLA_REPORT` names a file, write a Markdown table
     like `eqapo_playback.rs` writes (case, config, pairs, largest error over
     bound against the reference, largest absolute error against the
     renderer, engine version); when `SECS_CAMILLA_ARTIFACTS` names a folder,
     copy each case's configs, excitation, output and CamillaDSP log there.
4. Run it locally on Windows with the Windows build of the same release
   (`camilladsp-windows-amd64.zip`, sha256
   `d949de5ac5fc13282dd47155e0ac67087c09dd84baf7c7d568616bfc6e43d190`)
   fetched into the scratch folder
   `C:\Users\32170336\AppData\Local\Temp\claude\E--Sound-Equalization-and-Correction-System-2R\a241f0ba-0f2d-4875-a552-204969fa6951\scratchpad\camilladsp\`
   (add a `[windows]` entry to the manifest with that file and `zip` format,
   and support it in the fetcher), and report the table. Do not install
   anything system-wide.

## Files

You may edit: `crates/secs-camilla/tests/camilla_playback.rs` (new),
`crates/secs-camilla/Cargo.toml` (dev-dependencies only),
`.github/camilladsp-engines.toml` (new), `.github/scripts/camilla_engine.py`
(new), `.github/scripts/test_camilla_engine.py` (new),
`.github/scripts/eqapo_engine.py` and its test only to share code,
`Cargo.lock` as cargo changes it, `features.toml` (point the
`export.camilladsp` entry's tests at the new test too, or add an entry
`export.camilladsp_playback` with status `implemented`).

Do not touch: `.github/workflows/` (Claude writes the CI job), library
sources, the screen, `docs/`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-camilla --locked
python -m unittest discover -s .github/scripts -p "test_*.py"
```

and the playback test itself with the Windows engine:
`$env:SECS_CAMILLADSP='<path>'; cargo test -p secs-camilla --test camilla_playback --locked -- --ignored --nocapture`.

## Report

Changed files; how the excitation and the run work; the result table for
every case and config (both comparisons); anything CamillaDSP got wrong,
with the pair, frequency and size; the tail of each verification command;
anything left undone.
