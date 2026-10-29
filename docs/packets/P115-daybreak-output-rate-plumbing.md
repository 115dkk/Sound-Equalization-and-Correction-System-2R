# P115 (Daybreak): the output rate through project, service and exporters (ADR 0017)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\output-rates`
(branch `claude/output-rates`). Do not commit, do not push. Work only in
this worktree. P114 (already in this tree) added `secs_dsp::rate`
(`OUTPUT_RATES`, `is_output_rate`, `taps_at`, `delay_samples`,
`fir_to_rate`, `lowrate_to_rate`, `fir_response`) and
`secs_filters::render_paths_at`; use them, do not rewrite them. The screen
comes later (P116, P117); do not touch `apps/secs2r/ui/src` except the
generated bindings.

Read first: `CLAUDE.md`, `docs/adr/0017-output-sample-rate.md` (the
decisions are fixed there), `docs/DEVIATIONS.md`,
`crates/secs-project/src/dsp_profile.rs`, `crates/secs-project/src/project_edit.rs`
(`dsp_fields`, `dsp_result`, `dsp_set`), `crates/secs-types/src/ipc.rs`
(`DspFields`, `DspResult`, `DspSetParams`, `ExportParams`, `ExportResult`),
`crates/secs-service/src/export.rs`, `design_export.rs`,
`crates/secs-camilla/src/lib.rs` (`Options`, `make_config`, `delay`,
`fir_wav`, the MIMO path FIRs), `crates/secs-filters/src/lib.rs` (Equalizer
APO config and guide text, filter text, `fir_files`, `export_convolution`),
`crates/secs-audio/src/audio.rs` (`Endpoint`, `AudioBackend`), and
`crates/secs-service/src/lib.rs` (`Deps.audio_backend`).

## Fixed design

1. **Type.** In `secs-types`:

   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
   #[serde(rename_all = "snake_case")]
   #[ts(export)]
   pub enum OutputRateChoice { Auto, Fixed(u32) }
   ```

   JSON is `"auto"` or `{"fixed": 96000}`. `DspFields` gains
   `output_rate: OutputRateChoice` and `output_rate_editable: bool` (true for
   `camilla` and `custom`). `DspResult.window_ms` uses the fixed rate, or
   48 kHz when `Auto`. `ExportParams` gains
   `#[serde(default)] extra_rates: Vec<u32>` (convolution only; other kinds
   refuse a non-empty list like they refuse `mdat_scope`). `ExportResult`
   gains `output_rate: u32` (the rate the files were written at; 48000 for
   REW mdat) and `#[serde(default)] notes: Vec<Notice>` with a code
   `output_rate_fallback` when `Auto` could not read a supported rate.
   Regenerate the bindings with `cargo test -p secs-types`.
2. **Project.** `DspProfile.output_rate: OutputRateChoice`; presets
   `camilla` 48000, `tesira` 48000, `qsys` 48000, `minidsp` 96000, `custom`
   48000. Project JSON key `output_rate`: an integer or the string `"auto"`.
   A project without the key takes its preset's value; an unknown preset
   takes 48000. A value outside `OUTPUT_RATES` is a load error with a clear
   message. `dsp_set` accepts a new rate only for `camilla` and `custom`
   (`Auto` too); for the others it returns a typed invalid-params error;
   choosing a preset sets its rate. Saving writes the key; the upstream JSON
   shape otherwise stays, and `keep_unknown` must not duplicate it.
3. **Not in the design.** The output rate is not part of any calibration
   signature, cache key or result. Changing it never makes a result stale.
4. **Resolving at export.** `secs-service` resolves the rate when an export
   starts: `Fixed(r)` → `r`; `Auto` → the default output endpoint's
   `default_samplerate` from `Deps.audio_backend` (`enumerate()`, the
   endpoint with `is_default_output`); if that fails, has no default, or is
   not in `OUTPUT_RATES`, use 48000 and add the `output_rate_fallback` note
   (params: the rate read, or none). Put the resolver in one function with a
   unit test using a fake backend.
5. **Exporters.** Every exporter takes the resolved rate explicitly (a field
   in its options struct, not a global). At 48000 every byte stays as today;
   the existing golden and byte-identity tests must pass untouched.
   - CamillaDSP: `samplerate`, delays through `delay_samples`, FIR WAVs
     through `fir_to_rate` and `wav_bytes(.., rate)`, MIMO low-rate paths
     through `lowrate_to_rate`, the how-to comment's rate, and the bypass
     pair. PEQ, all-pass and crossover stay as parameters (CamillaDSP builds
     them at its own rate).
   - Equalizer APO: the "Device format" line and the guide text name the
     rate; FIR WAVs at the rate. Delays stay in ms.
   - Filter text: taps `(N 탭, R kHz)` at the output rate, delay samples at
     the rate, the info line's rate.
   - Convolution: render with `render_paths_at` at the rate; `routing.json`
     `sample_rate` is the rate; for each extra rate write the same bundle
     (WAV, CSV, `routing.json`) into a subfolder named `<rate>Hz/` and list
     the subfolders in the guide. Extra rates must be in `OUTPUT_RATES`,
     distinct, and not the main rate.
   - REW mdat stays at 48 kHz.
   - `secs-design.json`: `sample_rate` is the output rate; add
     `design_sample_rate: 48000` and `output_rate_auto: bool`. Keep old
     readers working (new fields only).
6. **CLI.** If `secs2r-cli` exposes DSP fields, add the output rate there;
   add `--extra-rates 44100,96000` to `export --kind convolution`. Say what
   you changed.
7. **Records.** `docs/DEVIATIONS.md`: the miniDSP preset exports 96 kHz
   coefficients where upstream exports 48 kHz ones. `features.toml`:
   `export.output_rate` (implemented, real tests) and update the DSP and
   export IPC entries' notes. `CHANGELOG.md`: an entry under the next
   version (출력 레이트, the miniDSP change, extra convolution rates).

## Tests

- Project: old JSON without the key per preset; round trip of `"auto"` and
  every fixed rate; invalid rate rejected; `dsp_set` rules.
- Resolver: fixed, auto with 44100/96000/32000 devices, no default device,
  backend error.
- Exporters at 96000 and 44100: CamillaDSP `samplerate` line, a delay in
  samples, FIR WAV header rate and length `taps_at(len, rate)`, MIMO FIR
  lengths; Equalizer APO device-format line; filter text taps and delay
  samples; convolution root and an extra-rate subfolder, `routing.json`
  rates; `secs-design.json` fields. Every existing 48 kHz byte test
  unchanged.
- A service-level test: change the output rate of a calibrated oracle
  project and export without recalculating; the result is not stale.

## Files

You may edit: `crates/secs-types/**`, `crates/secs-project/**`,
`crates/secs-service/**`, `crates/secs-camilla/**`,
`crates/secs-filters/src/lib.rs` and its tests (not `render.rs` or
`secs-dsp`, unless a P114 function is wrong: then stop and report),
`crates/secs-export/**` only if mdat needs the explicit 48 kHz,
`apps/secs2r-cli/**`, `apps/secs2r/ui/src/ipc/bindings/**` (generated),
`features.toml`, `docs/DEVIATIONS.md`, `CHANGELOG.md`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
python3 .github/scripts/check_feature_tests.py
(cd apps/secs2r/ui && npm run typecheck)
```

If `npm run typecheck` fails only because the screen does not yet pass the
new fields (fixtures, mocks), add the fields to `apps/secs2r/ui/src/ipc/mock.ts`,
`ipc/fixtures.ts` and screen fixtures with the 48 kHz values and nothing
else, and list the files.

## Report

The resolved-rate function, every exporter change with its test, files
changed, the CLI change, and the commands' last lines.
