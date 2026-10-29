# P100 (Daybreak): convolution IR and CSV export

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\export-bundle`
(branch `claude/export-convolution`). Work only there. Do not commit, do not
push, do not spawn sub-agents. Claude reviews the diff and commits. Nobody
else edits this worktree while you work.

Read first: `CLAUDE.md`, `CONTEXT.md`, `crates/secs-graph/src/lib.rs`,
`crates/secs-graph/src/plan.rs` (`Op`, `FilterPlan`, `SubFeed`, `MimoFeed`,
`plan_responses`, `headroom_db`, `response_chain`), `crates/secs-graph/src/files.rs`,
`crates/secs-filters/src/lib.rs` (how `export_eqapo` builds its plan, its
`LfeChoice`, `SECTION_Q`, the guide text and `write_bundle_observed`),
`crates/secs-filters/tests/eqapo_playback.rs` (the `REL`/`ABS` bounds and
why), `crates/secs-types/src/ipc.rs` (`ExportKind`, `filter_export_block`,
`export_block`), `crates/secs-service/src/export.rs`,
`crates/secs-service/src/export_folder.rs`, `apps/secs2r-cli/src/main.rs`
(export arguments), `features.toml` (`export.*`, `ipc.export`).

## Why

Users of PC convolvers (Equalizer APO's `Convolution:`, Roon, JRiver,
HQPlayer, foobar2000) asked to take SECS 2R's correction as convolution
files, and as CSV. Today the EqAPO, CamillaDSP and filter-text bundles hold
FIR WAVs, but those carry only the FIR stage: PEQ, crossovers, all-passes,
gains, polarity and delays live in the config files. A convolver given those
WAVs applies part of the correction. This packet adds a new export that folds
every stage of every signal path into one impulse response per path.

## Rules (fixed)

1. **Renderer.** Add `crates/secs-filters/src/render.rs`: a time-domain
   renderer of a `FilterPlan` in f64 at 48 kHz that uses the digital filters
   the EqAPO and CamillaDSP writers ask their engines for:
   - `Peq` is an RBJ peaking biquad; a high-pass or low-pass of family `F` is
     the cascade of RBJ high-pass or low-pass sections with the Q values of
     `SECTION_Q[F]` at the op's frequency; the analog all-pass is the RBJ
     all-pass biquad with the op's frequency and Q;
   - `Fir` convolves its taps; `FirLowRate` convolves its 48 kHz
     materialization (`FirView::materialize`), as the writers do;
   - `Gain` multiplies by `10^(dB/20)`, `Polarity` by `-1`, `Delay` shifts by
     `round_ties_even_i64(ms * 48000 / 1000)` samples;
   - topology exactly as `plan_responses_prepared`: an input's `input_ops`,
     then its `main_ops` to the output of the same role; `input_ops`, then
     `send_ops`, then each sub feed that lists the input (feed polarity and
     gain, then the feed's `ops`) to that sub; the LFE input's `lfe_ops` to
     every sub feed that lists `LFE`; each MIMO feed's `ops` from its raw
     input to its support output. `skip_fixed` is false.
   Public surface: `pub fn render_paths(plan: &FilterPlan, max_len: usize) ->
   Result<RenderedPaths, FiltersError>` returning, for every reachable
   `(input, output)` pair in plan order (inputs, then outputs), one impulse
   response; all responses have the same length. Length rule: render until
   the last sample whose magnitude exceeds `1e-6` times the largest
   magnitude over all paths (-120 dB), round that up to a power of two, at
   least 4096; if it exceeds `max_len` (the export uses `1 << 20`), return
   an error naming the path. A response that is identically zero is not a
   path. Keep the floating-point accumulation order fixed (ADR 0007).
2. **Export kind.** Add `ExportKind::Convolution` (wire name `convolution`)
   in `secs-types`, documented like its neighbours. It carries subs, LFE and
   MIMO: `filter_export_block` returns `None` for it, and the existing test
   that walks every kind is extended. Regenerate the TypeScript bindings with
   `cargo test -p secs-types` and keep them in the diff.
3. **Writer.** Add `export_convolution(...)` in `secs-filters` next to
   `export_eqapo`, built from the same plan (same chain, same MIMO paths,
   same `LfeChoice` the EqAPO export uses), validated before the first file
   is written, and written with the same atomic bundle helper. Files:
   - one WAV per path, `<input>_to_<output>.wav`, 48 kHz, 32-bit float, mono
     (`secs_graph::files::wav_bytes`), every file the same length;
   - every response is multiplied by `10^(-H/20)` where `H` is
     `(headroom_db(plan) * 2).ceil() / 2`, the value the EqAPO export writes
     as its preamp, so the files do not clip;
   - one CSV per path, `<input>_to_<output>.csv`, UTF-8, comma separated,
     header `frequency_hz,magnitude_db,phase_deg`, rows on a logarithmic grid
     of 48 points per octave from 10 Hz up to 24 kHz inclusive, evaluated as
     the exact DTFT of the written (attenuated, f32-rounded) IR, magnitude
     with six decimals (`-200.000000` for an exact zero), phase wrapped to
     `(-180, 180]` with three decimals, `.` as the decimal point;
   - `routing.json` (pretty, UTF-8): `{"schema": 1, "sample_rate": 48000,
     "length": N, "format": "f32", "headroom_db": H, "inputs": [...],
     "outputs": [...], "paths": [{"input", "output", "wav", "csv",
     "peak_gain_db", "delay_ms"}]}` where `peak_gain_db` is the path's largest
     gain between 20 Hz and 20 kHz before the headroom attenuation and
     `delay_ms` the sum of its `Delay` ops;
   - `컨볼루션 적용 방법.txt`, UTF-8 with CRLF line ends like the EqAPO guide,
     with exactly this text (fill the braces; the table has one row per path
     in `routing.json` order; numbers as shown):

     ```text
     SECS 2R 컨볼루션 IR

     이 폴더의 WAV는 보정 필터 전체(FIR, PEQ, 크로스오버, 올패스, 게인, 극성, 딜레이)를 입력과 출력 쌍마다 하나로 합친 임펄스 응답입니다.
     형식: 48000 Hz, 32비트 float, 모노. 모든 파일의 길이는 {N} 샘플로 같습니다.
     헤드룸: 클리핑을 막으려고 모든 파일에 {H:.1} dB 감쇠를 넣었습니다. 컨볼버에서 따로 줄이지 않아도 됩니다.

     거는 법
     - 아래 표의 한 줄은 입력 채널 하나를 그 WAV로 컨볼루션해 출력 채널 하나로 보내는 연결입니다.
     - 같은 출력으로 가는 줄이 여럿이면 결과를 더하십시오(서브 합산, LFE, MIMO 서포트).
     - 입력과 출력의 이름이 같고 그 출력으로 가는 줄이 하나뿐이면 그 WAV 하나만 거십시오.
     - 재생 장치와 컨볼버의 샘플레이트를 48000 Hz로 맞추십시오.

     입력 → 출력 | 파일 | 최대 이득 | 지연
     {input} → {output} | {wav} | {peak:+.1} dB | {delay:.2} ms

     같은 이름의 .csv에는 줄마다 주파수 응답(Hz, dB, 위상)이 있습니다. 기계가 읽는 표는 routing.json입니다.
     ```
4. **Service, folder and CLI.** `secs-service` exports the new kind like
   the EqAPO bundle (same result source, same staleness flag, cached designs
   after reopening). Folder: `timestamped_folder(base, "convolution", ...)`
   in `export_folder.rs`, like its neighbours. The CLI's export command
   accepts the new kind. The REW mdat and other kinds are unchanged.
5. **Registry.** Add `export.convolution` to `features.toml` (kind
   `export`, status `implemented`, the tests below, a `note`; there is no
   upstream source, so say it is new) and extend `ipc.export`'s note.

## Tests (new, all must pass)

- `secs-filters`: for the oracle plans the EqAPO golden tests already load
  (one RC only, one with subs and LFE, one with MIMO support paths), every
  rendered path's spectrum on the 48 kHz real-FFT grid matches
  `plan_responses` for the same `(input, output)` within the `REL`/`ABS`
  bounds of `eqapo_playback.rs`, from 10 Hz to 20 kHz; and the set of paths
  equals the set of non-zero `plan_responses` entries.
- `secs-filters`: a hand-built plan with one of every `Op` gives the
  analytically expected IR (delay position, polarity sign, gain, RBJ
  section count).
- `secs-filters`: the written bundle has the files above, WAVs of equal
  length that read back (with the reader in `secs-project`, if reachable, or
  a header check) at 48 kHz float mono, the attenuation `H`, a CSV whose
  first row is 10 Hz and last row 24000 Hz, and a `routing.json` that parses
  to the paths; an invalid chain writes nothing.
- `secs-types`: `filter_export_block(Convolution, subs, mimo)` is `None` for
  all four combinations; the wire name is `convolution`.
- `secs-service`: exporting the oracle project with subs and MIMO as
  `Convolution` writes the bundle in a `SECS_convolution_...` folder; after
  reopening it exports the same bytes.

## Files

You may edit: `crates/secs-filters/**`, `crates/secs-graph/**` (only if a
helper must move there), `crates/secs-types/src/ipc.rs` and the regenerated
`apps/secs2r/ui/src/ipc/bindings/**`, `crates/secs-service/src/export.rs`,
`crates/secs-service/src/export_folder.rs`, `crates/secs-service/tests/**`,
`apps/secs2r-cli/src/**`, `features.toml`, `Cargo.lock` as cargo changes it.

Do not touch: anything else under `apps/secs2r/ui/` (screens come later), the
upstream tree, `.github/`, `docs/`, `CHANGELOG.md`, crate manifests other than
adding a dependency you need.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python .github/scripts/check_feature_tests.py
python .github/scripts/upstream.py check-tree
```

and `cargo doc --no-deps -p secs-filters -p secs-types` with
`RUSTDOCFLAGS=-D warnings` (PowerShell: `$env:RUSTDOCFLAGS='-D warnings'`).

## Report

Changed files; the renderer's public items; the tests added and their
largest observed error against the bounds; the length the oracle cases
produced; the tail of each verification command; anything left undone.
