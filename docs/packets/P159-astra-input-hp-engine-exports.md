# P159 (ASTRA): the common input high-pass in the engine, the export plan and every writer (ADR 0031)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`
(branch `claude/input-hp`). Do not commit, do not push. Daybreak works on
`crates/secs-service/**`, `crates/secs-project/**` and `apps/secs2r-cli/**` at
the same time (P160) and Claude on `apps/secs2r/ui/**`; do not touch them.

Read first: `CLAUDE.md`, `docs/adr/0031-input-high-pass.md` (the design; follow
it exactly), `docs/adr/0022-low-end-protection.md` (the sub high-pass that is
also added to the chain after the design), then:

- `crates/secs-types/src/chain.rs` (`SignalChain`, `BandType`)
- `crates/secs-graph/src/plan.rs`: `Op`, `FilterPlan` (~139), `make`
  (~278–496), `plan_responses_prepared` (~600–680; the `pre` branch and the
  MIMO loop), `headroom_db` (~679–716)
- `crates/secs-filters/src/render.rs` (route collector ~304–379),
  `crates/secs-filters/src/lib.rs` (Equalizer APO: raw MIMO copies ~410–428,
  input correction ~429–437, `eqapo_config_at` ~812–829; `filter_text_at`
  ~1052–1170; convolution ~1690–1790), `crates/secs-filters/src/playback.rs`
  (`MimoFirPlayback` ~243–300 and the base playback)
- `crates/secs-camilla/src/lib.rs` (pipeline ~733–1265: `route_in` 768,
  `mimo_fork` 771–792, LFE branch 842–861)
- `crates/secs-engine/src/export_view.rs`, `crates/secs-engine/src/design.rs`
  (chain built ~1035; ADR 0022 sub HP set after RC/BA ~925–948),
  `crates/secs-engine/src/calibration.rs` `engine_inputs` (~111–240),
  `input.rs` `EngineConfig`, `setup.rs` `Setup`, `replay.rs`/`replay_mimo.rs`
  (cached export view and MIMO replay), `mimo/run.rs` `applied_response`
  (~393–437)

Already done by Claude (do not change): `CalibrationSettings.input_hp_hz`,
`SettingEdit::InputHp`, `CalibrationLimits.input_hp` (10–50 Hz), the
`|input_hp=` engine-key suffix, `SignalChain.input_hp_hz` (skipped when
`None`), `BandType::Bw48` with `BAND_TYPES`/`SECTION_Q`/CamillaDSP order 8/
playback order 8, `Note::InputHp { hz }` and its notice `input_hp`. Every
struct literal already carries `input_hp_hz: None`; set the real value where
this packet says.

## What to build (fixed)

The filter is `secs_dsp::response::band_filter(f, hz, "HP", "BW48")` in every
simulated response, and `Op::Hp { frequency: hz, kind: BandType::Bw48, fixed:
false }` in the plan. Use nothing else, so the simulation and every writer
agree.

1. **Engine plumbing.** `EngineConfig.input_hp_hz: Option<f64>` from
   `settings.input_hp_hz` in `engine_inputs`; `Setup.input_hp_hz`. The XO probe
   pass in `design.rs` leaves it as is (it does not change RC/BA). The design
   sets `chain.input_hp_hz` from it and pushes `Note::InputHp { hz }` once when
   it is `Some`. Fix the literals in `test_support.rs` and
   `crates/secs-engine-testkit/src/lib.rs`.
2. **RC and BA unchanged.** Nothing before the chain is built reads the value:
   `prepared`, `used`, RC, BA, bus and bundle PEQ, the verdict scores and
   `MainDesign.after` stay bit-identical to off.
3. **Export view.** `export_view_prepared` (and the cached replay path that
   returns the same view) multiplies every speaker's 2 s spectrum by the
   high-pass when `design.chain.input_hp_hz` is `Some`. Every input carries the
   same cut, so this equals cutting each input. Check whether `has_sub_filters`
   (or any "view equals design spectra" shortcut) must also turn true; mdat,
   `late_energy`, MIMO `support_ranges`, the MIMO design and
   `applied_response` then include the cut. Make sure no consumer applies it
   twice.
4. **Plan.** `FilterPlan.pre_ops: Vec<Op>`, the ops every input (LFE
   included) passes first; empty when off. `make` fills it from the chain.
   `plan_responses_prepared` multiplies it into every main, sub, LFE and MIMO
   response (`headroom_db` follows). The render collector puts it first on
   every route, MIMO and LFE routes included.
5. **Writers.**
   - CamillaDSP: one `BiquadCombo` `ButterworthHighpass` (order 8) filter,
     named `input_hp`, applied to every input channel `0..n_in` (LFE
     included) in a pipeline step between `route_in` and `mimo_fork`. WebFetch
     the CamillaDSP README on GitHub
     (https://github.com/HEnquist/camilladsp, the `BiquadCombo` part of the
     filters section) and confirm `ButterworthHighpass` accepts `order: 8`
     before you write it; quote the line in the report. The existing
     `band_name`/`combo` code already writes orders 2–8 for crossovers.
   - Equalizer APO: before the `Copy` lines that make the raw MIMO copies (and
     before input correction), every input channel gets the four high-pass
     sections with the `SECTION_Q` values of `BW48`. Check how the existing
     writer prints an `Op::Hp` with a Butterworth kind and reuse it.
   - Filter text: one line before the input correction lines that names the
     common input high-pass, its frequency and 48 dB/oct. Mirror the existing
     line format; Korean text, 합니다체 if it is a sentence.
   - Convolution (`routing.json`, REAPER): covered by the render collector;
     confirm every route's IR includes the cut.
   - Playback (`MimoFirPlayback` and the base playback): the cut runs first on
     each input.
6. **Off is bit-identical.** With `None`, every plan, writer output, export
   view, MIMO design and golden stays byte-identical.

## Tests

New `crates/secs-graph/tests/input_hp.rs` (or next to the existing graph
tests) and engine tests in `crates/secs-engine/src/tests/input_hp.rs`
(register it in `tests/mod.rs`):

- A chain with mains, LFE, two subs and MIMO FIR paths (reuse an existing
  fixture): with `input_hp_hz = Some(27.0)`, for every route the render
  collector returns, the response equals the off response times
  `band_filter(f, 27, "HP", "BW48")` within 0.01 dB and 0.1° on 10 Hz–20 kHz.
  Do the same for `plan_responses_prepared`.
- CamillaDSP: the YAML has exactly one `input_hp` filter (`BiquadCombo`,
  `ButterworthHighpass`, `freq: 27`, `order: 8`) and one pipeline step using it
  on every input channel, placed after `route_in` and before `mimo_fork`.
- Equalizer APO: the config has the four BW48 sections on every input channel
  before the MIMO `Copy` lines; the existing EAPO playback test harness (if
  one simulates the config) agrees with the plan within 0.05 dB.
- Off: every existing golden passes unchanged (run them).
- Engine: with the setting on, RC/BA outputs (`MainDesign.response`,
  `MainDesign.after`, the chain's mains and subs) are bit-identical to off;
  `chain.input_hp_hz == Some(27.0)`; one `Note::InputHp`; the export view
  spectra equal off × high-pass within 1e-9 relative.
- `features.toml`: add one entry for the engine and export side with your test
  names (status implemented, upstream "new", note citing ADR 0031). Only your
  entry; Daybreak adds its own.

## Allowed files

`crates/secs-engine/**`, `crates/secs-engine-testkit/**`, `crates/secs-graph/**`,
`crates/secs-filters/**`, `crates/secs-camilla/**`, `crates/secs-dsp/**` (only
if a helper is needed), `features.toml` (your entry only). Not secs-types (ask
in the report if a type must change), not the service, project, CLI or screen.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-engine -p secs-graph -p secs-filters -p secs-camilla -p secs-dsp --locked
```

If Daybreak's crates are mid-way and do not compile, say so and run what you
can with `-p`. Never revert files you did not change. Before a large build,
check that drive E: has at least 20 GB free; if not, delete only
`target/debug/incremental` in this worktree.

## Report

Files changed; how the cut enters the export view, the plan and each writer
(a line each); the CamillaDSP doc line you confirmed; every number the new
tests print (largest dB and degree error per check); the commands' last lines;
anything in ADR 0031 that turned out wrong.
