# P157 (ASTRA): unwind the RC FIR's constant phase above the band (ADR 0030)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`
(branch `claude/hf-phase-unwind`). Do not commit, do not push. Daybreak works
on the service, project, export and CLI crates at the same time (P158) and
Claude on `apps/secs2r/ui/**`; do not touch them.

Read first: `CLAUDE.md`, `docs/adr/0030-hf-phase.md` (the design; follow it
exactly), then in `crates/secs-engine/src/rc.rs` the `Realizer` struct and
`realize`, `finish_correction` (where `excess`, `band` and the strength ladder
are built), `phase_fade`, `band_set`, `channel_with_cuts` (where
`Note::ModeDecay` is pushed after the phase-candidate choice), and
`crates/secs-engine/src/tests/lf_align.rs` (helpers `config`, `correct`,
`arrival`, `assert_bits`, `synthetic`).

Already done by Claude (do not change): `CalibrationSettings.hf_phase`,
`SettingEdit::HfPhase { on }`, the `|hf_phase` engine-key suffix (secs-types),
and `Note::HfPhase { role, rotation_deg: Option<f64>, limited }` in
`crates/secs-engine/src/notes.rs`.

## What to build (fixed)

1. **Plumbing.** `EngineConfig.hf_phase: bool` next to `mode_decay`
   (`input.rs`), set from `settings.hf_phase` in `calibration.rs`
   `engine_inputs`; `Setup.hf_phase` (`setup.rs`); the XO probe pass in
   `design.rs` clears it with `lf_align` and `mode_decay`. Fix the struct
   literals in `test_support.rs` and `crates/secs-engine-testkit/src/lib.rs`.
2. **Off is bit-identical.** When `setup.hf_phase` is false (or `lift_ms <= 0`,
   which never reaches the realizer), every float operation stays exactly as
   now, in the same order. Keep the existing `realize` loop for the off case.
3. **Unwind in `realize(frac, m)`** when on:
   - First pass: compute `applied_i` exactly as now (the `lf_pull` branch when
     `beta` is set, else `e.clamp(-0.005, lift*frac)`) and `S = Σ applied_i`
     in index order. Do not store a second spectrum; recomputing `applied_i`
     in the second pass is fine (deterministic). A BINS-long `f64` weight
     vector built once per `Realizer` is fine too; if you store one, add it to
     the RC memory accounting in `design.rs` (`ChannelBytes`) the way
     `RC_ADR0025_SPECTRA` is added.
   - `df = design_freqs()[1]`; `rho = 2π·df·S`; `r` = `rho` wrapped to
     `(−π, π]`.
   - Region: `f1 = max(rc_hi·√2, max over setup.rc_bands of hi·√2)` (the same
     edges `phase_fade` uses; `debug_assert!` that every `excess_i` with
     `f_i >= f1` is 0). `f2 = min(4·f1, 0.45·SR_F)`. If `f2 < 2·f1` there is
     no unwind: the realizer behaves as off and reports `rotation_deg: None`.
   - Weights on bins with `f1 <= f <= f2`: `x = ln(f/f1)/ln(f2/f1)`,
     `w = 0.5·(1 − cos(2π·x))`; `W = Σ w`.
   - Second pass: `applied'_i = (applied_i − r/(2π·df)·w_i/W).clamp(-0.005, lift)`
     for region bins (others unchanged), then the same `sum += lift − applied'`
     phase build as now. `limited` = the clamp changed any region value.
   - Return `r` in degrees and `limited` beside the existing tuple (change the
     return type as you see fit).
4. **Outcome and note.** `RcOutput.hf_phase: Option<HfPhaseOutcome>` with
   `HfPhaseOutcome { rotation_deg: Option<f64>, limited: bool }`, taken from
   the realize call whose result the ladder kept (including fraction 0).
   `None` when off or when `lift_ms <= 0`. In `channel_with_cuts`, after the
   phase-candidate choice and next to `Note::ModeDecay`, push
   `Note::HfPhase { role, rotation_deg, limited }` when the outcome is `Some`.
   Do not change `phase_error_band` or the ladder thresholds (ADR 0030 says
   why).
5. **Tests**, new file `crates/secs-engine/src/tests/hf_phase.rs` (register it
   in `tests/mod.rs`; make `lf_align.rs` helpers `pub(super)` if needed). Use
   rooms `hp2(35)·allpass2(80, 1.0)`, `hp2(35)·allpass2(60, 2.0)` and
   `hp2(35)·allpass2(45, 3.0)·allpass2(120, 1.0)`, in standard (Linear) mode
   with `rc_hi` 1000 Hz and with `rc_hi` 250 Hz, plus one Mixed-mode case.
   Measure the HF phase as `arg(response_i · e^{j2π f_i·lift})` in degrees.
   For each case, off vs on:
   - on: `|phase|` ≤ 1° on every bin in `[1.06·f2, min(16 kHz, 0.45·SR)]`;
     off: the third room exceeds 10° there (so the test measures something).
   - `arrival(h, out, 60, 100)` differs by ≤ 0.1 ms; `phase_strength` equal.
   - On bins in `[f1, f2]`, `|dB(on.response) − dB(off.response)|` ≤ 0.1 dB.
   - Energy of `fir[..peak − 48]` (peak = argmax |fir|, 1 ms before it) on
     ≤ off × 1.02 + 1e-12.
   - The note: one `Note::HfPhase` for the channel, `rotation_deg` within 1°
     of the off HF phase measured above (the rotation that was unwound; the
     off phase relative to the lift delay is `+rho`).
   - Extra band case: `setup.rc_bands` with one band whose `hi` is 3000 Hz (use
     the type `Setup.rc_bands` has): `f1 = 4243 Hz`; on and off phase agree
     within 0.5° on `[2000, 4000]` Hz and the on HF check above holds on
     `[1.06·f2, 21 kHz]`.
   - `rc_hi` 8000 Hz (`f2 < 2·f1`): `rotation_deg: None` and `assert_bits`
     equal to off. Zero mode with `hf_phase` on: no note, `assert_bits` equal
     to off.
   Print the measured numbers with `eprintln!` so the report can quote them.
   All existing engine tests (goldens, oracles) must pass unchanged: they are
   what proves off stays bit-identical.
6. `features.toml`: `calibration.hf_phase` as `implemented`, pointing at the
   new tests. Do not add the service or CLI entries (P158 does).

## You may edit

`crates/secs-engine/**` except `src/notes.rs`, `crates/secs-engine-testkit/**`,
`features.toml` (only your entry).

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-engine -p secs-engine-testkit --all-targets --locked -- -D warnings
cargo test -p secs-engine --locked
cargo test -p secs-engine --locked hf_phase -- --nocapture
```

The rest of the workspace (secs-service and others) may not compile until
P158 lands; say so and run what you can. Never revert files you did not change.

## Report

Files changed, the realize change in a few lines, every number the new tests
print (per room and case: off and on HF phase, arrival difference, magnitude
difference in the region, pre-peak energy ratio, rotation in the note), the
commands' last lines, and anything in the ADR that turned out wrong.

## Revision 1 (after the first run)

The first run showed two errors in the design; ADR 0030 now has the fix.
Change the realizer and tests as follows; everything else stays.

- **Rotation.** The phase build counts bin 0, so the constant above the band
  is `rho = 2π·df·(S − lift)`, not `2π·df·S`. Use that.
- **Delay only.** Unwind `u = rho` wrapped to `[0, 2π)` (never the other
  way): `applied'_i = (applied_i − u/(2π·df)·w_i/W).clamp(-0.005, lift)`.
  Unwinding towards less delay moved region energy ahead of the first arrival
  (pre-peak ratio 1.22 in room 3, `rc_hi` 250).
- **Region.** `f2 = min(8·f1, 0.45·SR_F)` (three octaves); still no unwind
  when `f2 < 2·f1`.
- **Note.** `rotation_deg` = `rho` wrapped to `(−180°, 180°]` (the rotation
  found, which is what the off FIR shows above the band).
- **Tests.** The HF check range becomes `[1.06·f2, 0.45·SR]`. The extra-band
  case uses `rc_hi` 250 Hz with one extra band 900–1200 Hz (`f1` = 1697 Hz,
  `f2` = 13576 Hz); on and off agree within 0.5° on `[1000, 1600]` Hz. Keep
  every other threshold, including pre-peak ratio ≤ 1.02 and region
  magnitude ≤ 0.1 dB. Also print the largest extra delay in the region
  (`max_i (applied_i − applied'_i)` in ms) per case.
- `crates/secs-types/src/calibration.rs:839` formatting is Claude's; leave it.

## Revision 2 (test metrics only)

The algorithm of revision 1 stays. Two test metrics measured the wrong thing:

- **LF arrival.** `arrival()` references 3–6 kHz, which lies inside the
  region revision 1 delays on purpose when `rc_hi` is 1000 Hz. Replace that
  check with the absolute LF delay: median of `response::gd_curve` of the
  corrected response (`h · out.response`) over 60–100 Hz (same 101-point grid
  and 1/24 smoothing `arrival()` uses), on vs off ≤ 0.1 ms. Keep printing the
  old `arrival` delta as information.
- **Pre-ringing.** Anchor the window at the designed arrival, not at
  `argmax |fir|`: `pre = Σ fir[i]²` for `i < round(lift·SR) − 48`. Pass when
  `10·log10(pre_on / pre_off) ≤ 0.25` dB. Print this ratio in dB and keep
  printing the old argmax-anchored ratio as information.

Every other threshold stays. Same rules as before; report the same way.
