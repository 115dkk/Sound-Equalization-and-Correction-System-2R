# P146 (ASTRA): extra correction bands in the engine (ADR 0024)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-bands`
(branch `claude/rc-bands`). Do not commit, do not push.

Read first: `CLAUDE.md`, `docs/adr/0024-rc-extra-bands.md` (the decision you
implement), `docs/adr/0021-rc-upper-limit-basis.md`,
`crates/secs-types/src/calibration.rs` (`RcBand`, `CalibrationSettings.rc_bands`,
already defined; do not change secs-types), then `crates/secs-engine/src/`
`calibration.rs`, `input.rs`, `setup.rs`, `design.rs`, `rc.rs`, `fit.rs`,
`notes.rs`, `levels.rs`.

## What to build (fixed)

1. **Input.** Carry `settings.rc_bands` into the engine setup next to
   `rc_range` (`calibration.rs` around line 224, `input.rs`, `setup.rs`). Add
   `rc_bands: Vec<RcBand>` (or `Vec<(f64, f64)>`) to the setup type.
2. **Band set.** One function, used everywhere below:
   `fn band_set(lo: f64, hi: f64, extra: &[(f64, f64)]) -> Vec<(f64, f64)>`
   puts the primary band `(rc_lo, resolved rc_hi)` and the extra bands
   together, sorts by low edge, and merges bands that overlap or touch. With
   no extra bands it returns exactly `[(rc_lo, rc_hi)]`. Unit-test it.
3. **Uses of the range in `rc.rs`, switched to the band set:**
   - Correction-target mask (now `rc.rs` ~431-441): one half-octave ramp per
     band (same formula as today: 0 at `lo/√2`, 1 at `lo`, 1 at `hi`, 0 at
     `hi·√2` capped at 0.99×Nyquist), combined with `max`. Between bands, the
     mask is 0 where no ramp reaches.
   - No-boost mask grid (~164-181): over the band set (keep `max(lo, 20)`).
   - Long-mode peaks (~187): keep the 160 Hz cap; keep a peak only if it lies
     in the band set.
   - Zero-mode PEQ fits (~446-480) and `fit.rs` weights (~82, ~140): the
     seed zeroes and the refine weights 0.15 outside the **band set** instead
     of outside `[lo, hi]`; the high fit runs from 400 Hz to the band set's
     highest upper edge (≤ 20000) instead of `min(rc_hi, 20000)`. The low fit
     stays `max(15, rc_lo/1.4)`..400.
   - FIR modes: the PEQ pre-stage (20–500 Hz, `fit_request(... Some(s.rc_hi))`
     ~661-670) does not change.
   - Excess-phase fade (~612-624): per band, `max`-combined. The primary band
     keeps today's fade exactly (ramp below 15 Hz, full up to `rc_hi`, 0 at
     `rc_hi·√2`, no lower edge). Each extra band ramps in from `lo/√2` to `lo`
     and out from `hi` to `hi·√2` (log2-linear, like the magnitude mask). The
     phase reference (median GD over 3–6 kHz, ~605-611) does not change.
   - Realizer acceptance band (~625-628): `f >= max(20, lo_fir)` and inside
     the union of `[band_lo, band_hi·√2]` over the band set (each upper edge
     ×√2, capped below Nyquist). Today that is `[max(20,lo_fir), rc_hi·√2]`.
   - Second-stage FIR trim mask (~679-682): the band set (keep `max(lo, 20)`).
   - Phase-candidate score grid (~748-759) and GD cuts (~817-822, `fit.rs`
     ~446): over the band set.
4. **Agreement basis.** The probe pass with `rc_hi = 2000` (`design.rs`
   ~427-458) runs **without** extra bands, so they never change the
   agreement limit. The final pass uses them.
5. **Notes.** Add `Note::RcBands { bands: Vec<(f64, f64)> }` holding the
   merged band set, emitted only when there are extra bands. Expose the same
   merged set on the design result (for example `Design.rc_bands`), empty
   when there are none.
6. **Default unchanged.** With `rc_bands` empty, every mask, range, weight
   and output must be bit-identical to today. Goldens and oracle tests are
   the check; do not regenerate any golden.
7. Fix the `CalibrationSettings { .. }` literals in
   `crates/secs-engine/src/test_support.rs` and
   `crates/secs-engine-testkit/src/lib.rs` (`rc_bands: Vec::new()`).

## Tests (new, synthetic; upstream has no multi-band oracle)

Put them in `crates/secs-engine/src/tests/rc_bands.rs` (wire into the tests
module like `rc_upper.rs`).

- `band_set` sorts, merges overlapping and touching bands, and returns the
  primary band alone when there are no extras.
- A synthetic main with a +6 dB bump at 16 kHz and a dip at 100 Hz:
  with only the primary band (`rc_hi` manual 300), the 16 kHz bump is not
  corrected; with an extra band 15000–20000 it is (within the existing
  16 kHz boost cap for boosts), and a bump at 5 kHz between the bands stays
  uncorrected.
- The extra band also gets excess-phase correction: the corrected GD inside
  15–20 kHz moves toward the reference compared with no extra band, in a
  FIR mode (Mixed or Linear).
- Zero mode: PEQ bands appear inside the extra band; none are centred in
  the gap between bands.
- Agreement basis: the resolved upper limit is the same with and without an
  extra band.
- With `rc_bands` empty, the design equals the design before this change
  (compare against a design computed with the same input in the same test,
  or rely on the existing goldens; say which).

## You may edit

`crates/secs-engine/**`, `crates/secs-engine-testkit/src/lib.rs`. Not
secs-types, not the service, not the screen.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-engine -p secs-engine-testkit --all-targets --locked -- -D warnings
cargo test -p secs-engine --locked
```

## Report

Files changed; for each use in step 3 what changed and the line; the
`band_set` signature and the note/design fields you added (the service
packet reads them); every test with what it shows; the commands' last lines.
