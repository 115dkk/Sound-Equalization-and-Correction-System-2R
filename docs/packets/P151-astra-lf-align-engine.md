# P151 (ASTRA): common LF arrival and pole-replacing mode cuts in the RC engine (ADR 0025)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`
(branch `claude/lf-align`). Do not commit, do not push. Daybreak works on
the service, project, export and CLI crates at the same time (P150) and Opus
on `apps/secs2r/ui/**` (P152); touch only the files listed below.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0025-lf-arrival-and-mode-decay.md`
(the decision; this packet fixes the details), `docs/adr/0022-*.md`
(rolloff), `crates/secs-engine/src/rc.rs` whole, `decay.rs`
(`mode_t60`, `late_peaks`, `median`), `low_protect.rs` (`rolloff`,
`clamp_centres`, `limit`), `notes.rs`, `setup.rs`, and
`crates/secs-engine/src/tests/rc_bands.rs` for the test harness.

Already in place (do not redo): `EngineConfig.lf_align/mode_decay`,
`Setup.lf_align/mode_decay` (copied from the config; the agreement probe
clears both in `design.rs`), and the note shapes
`Note::LfAlign { role, strength, arrival_ms: Option<f64>, limited }` and
`Note::ModeDecay { role, target_t60_s, modes: Vec<(hz, t60_s, depth_db)> }`.
Read the switches from `input.setup`, never from `input.config`.

## Rule zero: off is bit-identical

With both switches off, every RC output must be bit-identical to today. Use
`if setup.lf_align { new path } else { today's expression }` branches, not
`β = 0` arithmetic (0 × NaN is NaN). Keep `e.clamp(-0.005, self.lift * frac)`
literally on the off path. The golden and oracle tests
(`cargo test -p secs-engine`) are the gate for this; do not touch golden
data.

## Part A: `lf_align` (FIR modes only)

Only when `setup.lf_align && config.taps > 0`. In Zero mode (`taps == 0`)
nothing changes and no note is written.

1. **Total phase vector.** In `prepare_correction`, in the same loop and with
   the same weight `w` and the same `PhasePositions::Main` skip as `u`,
   accumulate `ut[i] += w · reference[i] / (|reference[i]| + 1e-30)` where
   `reference = h·p·m` after the peak shift (the value the loop already
   builds). Divide by `total` like `u`. Store it in `FirPreparation` only
   when lf_align is on (`Option`).
2. **Rolloff and band.** `f_lin = max(1.5 · rolloff, rc_lo)` where
   `rolloff = low_protect::rolloff(input.comb, input.main_position, None)`
   (call it regardless of `config.low_protect`); `f_lin = rc_lo` when no
   rolloff is found. `f_x = 200 Hz`. `β(f)` is 1 on `[f_lin, f_x]`, a raised
   cosine over half an octave below `f_lin` (0 at `f_lin/√2`) and above `f_x`
   (0 at `f_x·√2`), 0 elsewhere. If `f_lin >= f_x`, `β ≡ 0` and the note says
   `limited = true`.
   Masked bins: `β_eff = smooth(β · (1 − mask), 6.0)` with
   `mask = nb[i] || lm[i]` (the no-boost and long-mode masks of this
   channel; keep `lm` in `FirPreparation` when on) and `smooth` the
   module's 1/6-octave power smoother. Clamp `β_eff` to [0, 1].
3. **Target `τ_T`.** Predicted corrected magnitude in dB:
   `sm[i] + corr[i]` (the averaged smoothed measurement and the masked design
   correction, before the PEQ split; take the values `prepare_correction`
   already has). `τ_mp` = group delay of `minimum_phase(10^(dB/20))`,
   through `unwrap` → `gradient` → `smooth(·, 3.0)`. Then
   `τ_T(f) = max(0, τ_mp(f) − τ_mp(f_lin))` for `f < f_lin`, and 0 for
   `f >= f_lin`. Store `τ_T`, `β_eff` and `f_lin` in the preparation.
4. **Correction.** In `finish_correction`, on the lf_align path:
   `τ_tot = smooth(gradient(unwrap(smooth_complex(f, ut, Scalar(phase_frac)))), 3.0)`,
   `ref_tot` = median of `τ_tot` on 3–6 kHz (as `reference` for the excess),
   and per bin
   `e = fade · [(1 − b)·(τ_ex − ref) + b·(τ_tot − ref_tot − τ_T)]`
   with `b = β_eff[i]`, `fade` = today's `phase_fade`, `τ_ex`/`ref` today's
   values. Any non-finite term in the new part makes that bin fall back to
   today's `(τ_ex − ref)·fade`.
5. **Pull limit.** The `Realizer` gets an optional per-bin `β_eff`. On the
   lf_align path the upper clamp is
   `(1 − b)·lift·frac + b·min(lift·frac, 1.5 / f)` (`f` the bin frequency,
   `f > 0`; bin 0 uses `lift·frac`). Lower clamp stays −5 ms. The ladder,
   its acceptance test and the residual passes are unchanged.
6. **Outcome.** Add to `RcOutput` a field `lf_align: Option<LfAlignOutcome>`
   (`None` when off or Zero mode; every other constructor sets `None`):
   `strength` = the chosen ladder fraction; `arrival_ms` = 1000 × median,
   over bins with `max(f_lin, 40) <= f <= 150` and `b >= 0.5`, of
   `(τ_tot − ref_tot − τ_T) − e_applied` where `e_applied` is the clamped
   value at the chosen strength (`None` if there are no such bins);
   `limited` = some bin with `b >= 0.5` has `e − e_applied > 0.001`
   (or `f_lin >= f_x`, or strength 0).
   In `channel_with_cuts`, after the phase candidate is chosen, push one
   `Note::LfAlign { role, .. }` from `out.lf_align`. Do not push it in
   `finish_correction`: that function runs twice per channel and its notes
   are merged.

## Part B: `mode_decay` (all modes, including Zero)

Only when `setup.mode_decay`.

1. **Modes.** Split `long_mode_mask` so the `(f0, T60)` list it builds
   before its own `1.3 × median` filter comes from a helper
   `long_modes(input) -> Vec<(f64, f64)>`; `long_mode_mask` must stay
   bit-identical. Use that list here; do not run `late_peaks` again.
2. **Target decay.** `T60_t = max(median(T), 0.25)` where `T` collects
   `decay::mode_t60(&ir, SR_F, fc, 0.115 · fc)` for
   `fc ∈ {200, 250, 315, 400, 500}` on the 48 kHz main-position impulse
   response `ir = fft::irfft(main_response, N_FFT)`, keeping `Some` values;
   `T60_t = 0.25` if none. (The 600 Hz low-rate signal cannot measure above
   ~285 Hz.)
3. **Selection and depth.** Candidates: `T60_m > 1.5 · T60_t`. Height
   `h = sm(f0) − (target(f0) + level_db)` from the averaged smoothed
   measurement before any cut (interpolate on the design grid). Depth
   `D = min(12, 20·log10(T60_m / T60_t), max(0, h) + tuning.long_boost_db)`;
   skip if `D < 1`. Keep at most 4, longest `T60_m` first.
4. **Biquad.** `A = 10^(−D/40)`, `Q_m = π · f0 · T60_m / 6.91`,
   `Q = A · Q_m`; the cut is the PEQ `(f0, Q, −D)`. With the engine's RBJ
   peaking form this places the zeros on the measured poles and the poles at
   `Q_t = A² · Q_m`. Put this in a pure function
   `decay_cut(f0, t60_m, t60_t, height_db, long_boost_db) -> Option<Peq>`.
5. **Where it acts.** Let `cuts` be the selected list and
   `dr = peq_response(f, &cuts)`. In `prepare_correction`:
   - the averaged `power` (and therefore `sm`, `corr`) and every per-position
     `reference = h·p·m` use `h·dr` instead of `h`;
   - the no-boost and long-mode masks keep using the raw measurement;
   - bins within `f0·2^(±1/24)` of each cut get the long-mode boost cap
     (`corr = corr.min(long_boost_db)`) like `lm`.
   The fitted PEQ stays separate from `cuts` everywhere it is fitted, refitted
   (`finish_correction` refits `peq` from scratch), clamped by
   `low_protect::clamp_centres` or used inside `design_error`. Only the
   outputs combine them: `RcOutput.peq = cuts ++ fitted`,
   `RcOutput.response = (FIR response) · p_fitted · dr` (Zero path:
   `peq_response(f, &RcOutput.peq)`), `RcOutput.design_db = design_db + dB(dr)`.
   Check that `low_protect::limit` in `channel_with_cuts` leaves the cuts as
   they are (it scales only boosts) and keep it that way.
6. **Outcome.** `RcOutput.mode_decay: Option<ModeDecayOutcome>` with
   `target_t60_s` and `(hz, t60_s, depth_db)` per cut; push one
   `Note::ModeDecay { role, .. }` in `channel_with_cuts` (empty `modes` when
   none qualified).

## Tests

New files `crates/secs-engine/src/tests/lf_align.rs` and
`crates/secs-engine/src/tests/mode_decay.rs` (register them in
`tests/mod.rs`), using the `rc_bands.rs` harness style. At least:

- `lf_align_aligns_lf_total_gd_to_hf`: one main with a 2nd-order HP near
  35 Hz and an LF all-pass (e.g. `allpass2(f, 80, 1)`), Linear mode. With
  lf_align on, the median total GD of `main·response` at 60–100 Hz is within
  1 ms of the 3–6 kHz median; with it off the gap is larger (state the two
  numbers in the report).
- `lf_align_common_arrival_across_mains`: two mains with different LF
  all-passes; with lf_align on, their 60–150 Hz total GD differ by less than
  1 ms.
- `lf_align_pull_limit_holds`: for every bin with `b >= 0.5` the applied
  pull is at most `1.5 / f` (check through a helper that exposes the
  realized clamp, or through the FIR energy before its peak in 100–125 Hz).
- `lf_align_zero_mode_and_off_are_unchanged`: Zero mode with lf_align on
  equals off bit for bit, and Linear with lf_align off equals today.
- `lf_align_mixed_mode_reports_limited`: Mixed (lift 8 ms) on the first
  case gives a note with `limited = true`.
- `mode_decay_cut_matches_pole_replacement`: `decay_cut` gives
  `A = sqrt(T60_t/T60_m)` and `Q = A·Q_m` without the depth cap, the cap at
  12 dB and at `h + long_boost_db`, and `None` below 1 dB.
- `mode_decay_shortens_a_long_mode`: a synthetic main with a 50 Hz mode of
  T60 ≈ 1.2 s on a ~0.3 s background; with mode_decay on, `mode_t60` of the
  corrected main at 50 Hz is at most `1.5 · T60_t` (or at most half of the
  uncorrected value; state both numbers), and the cut is in `out.peq`.
- `mode_decay_survives_a_misplaced_estimate`: apply `decay_cut` built for
  `f0` to a mode at `f0 ± 1 Hz`; the decay still shortens and the magnitude
  dip is at most `D`.
- `mode_decay_zero_mode_carries_the_cut` and `mode_decay_off_is_unchanged`.

Register in `features.toml`: `engine.lf_align` and `engine.mode_decay`
(status `implemented`, `upstream = "none (ADR 0025)"`, tests = the names
above as `secs-engine::<name>`), in the style of `engine.rc_bands`.

## You may edit

`crates/secs-engine/src/{rc.rs,decay.rs,low_protect.rs,design.rs}`,
`crates/secs-engine/src/tests/{mod.rs,lf_align.rs,mode_decay.rs}`, and the
`engine.*` entries of `features.toml`. Anything else in secs-engine only if
the new `RcOutput` fields force it (constructors); say which.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-engine --locked
```

If the workspace outside secs-engine does not compile because P150 is
mid-way, that is expected; run the commands above only. Never revert files
you did not change.

## Report

The design points you had to decide beyond this packet, every new
function's signature, the numbers each test checks (before and after),
`cargo test -p secs-engine` totals, and the commands' last lines.
