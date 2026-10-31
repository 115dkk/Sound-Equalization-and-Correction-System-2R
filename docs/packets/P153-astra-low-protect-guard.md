# P153 (ASTRA): low_protect blocks boosts without killing the bass (ADR 0022 amendment)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`,
branch `claude/low-protect-fix` (cut from `claude/lf-align`). Do not commit,
do not push. You are the only worker in this worktree.

Read first: `CLAUDE.md`, `docs/adr/0022-low-end-protection.md` including the
new section '보완 (2026-10-31)' (the decision; this packet fixes the details),
`crates/secs-engine/src/low_protect.rs`, `crates/secs-engine/src/rc.rs`
(`prepare_correction`, `finish_correction`, `channel_with_cuts`),
`crates/secs-engine/src/ba.rs` (`target_peq`, `bus_fit`, the bundle fit near
`Note::LowProtect`), `crates/secs-engine/src/design.rs` (RC input
construction, the sub high-pass block that pushes `Note::LowProtect`),
`crates/secs-engine/src/tests/low_protect.rs`, and the probe the coordinator
used: `C:\Users\32170336\AppData\Local\Temp\claude\E--Sound-Equalization-and-Correction-System-2R\a241f0ba-0f2d-4875-a552-204969fa6951\scratchpad\zz_lp_probe.rs`
and `zz_lp_probe2.rs` (temporary tests that ran designs of the `room` case with
`low_protect` on and off; copy what you need into real tests).

## What the probe showed (room case, Linear, two subs)

- Bus PEQ with low_protect off: `22/1.70/+5.2 31/2.54/+4.5 60/8.14/+6.0 ...`;
  on: `22/+0.5 31/+0.5 61/+0.6 58/+0.5 ...`. The bus rolloff is about 21 Hz,
  yet the 58–61 Hz boosts were cut to +0.5 dB because `limit` scales every
  boost band centred below 4 × rolloff by one factor. The aligned sub sum
  loses 4–6 dB from 20 to 60 Hz.
- No `Note::LowProtect` appears when `limit` succeeds, so the loss is silent.
- With `rc_lo = 20` the main RC filters refill the loss (main L input filter
  +1.8 dB at 25 Hz off, +7.5 dB on) and that boost reaches the subs through
  bass management; with `rc_lo >= 30` nothing refills it and the output at
  20–35 Hz drops 5–6 dB ('저역이 죽는다').
- A main whose FIR leaks above +0.5 dB below its rolloff makes `limit` return
  false and the original filter is kept ('막지 못해 원래 필터를 그대로 썼습니다').

## What to build (fixed)

All of it only when `config.low_protect`; with it off every output stays bit
identical (the golden and oracle tests are the gate; do not touch golden data).

1. **Targeted reduction** (replace the uniform `k` in `low_protect::limit`).
   New signature: `pub(crate) fn limit(peq: &mut Vec<Peq>, fixed: &[C], rolloff: f64) -> LimitOutcome`
   with `LimitOutcome { reduced: usize, cuts: usize }` (counts of boost bands
   whose gain was lowered and of protective cuts appended). Algorithm, on the
   design grid below `rolloff`, total = `peq_response(peq) · fixed` in dB:
   - Repeat while some bin exceeds +0.5 dB (at most `4 * peq.len() + 8`
     rounds): take the worst bin `f_w`; among boost bands (gain > 0) pick the
     one with the largest dB response at `f_w`; if that contribution is at
     most 0.05 dB, stop this phase; otherwise lower that band's gain by
     bisection (40 steps, deterministic) to the largest gain in `[0, gain]`
     that brings the total at `f_w` to at most +0.5 dB (0 if none does).
   - If bins still exceed +0.5 dB, append a protective cut
     `(f_w, 1.4, -(excess + 0.1))` and re-measure; at most 4 cuts. If the
     fourth cut still leaves an excess, deepen that last cut by the
     remaining excess at its bin and re-measure until no bin exceeds (bounded
     by 8 rounds; if it still fails, return the outcome anyway: no panic).
   - Never restore the original fit.
2. **Callers.** `rc::channel_with_cuts` (FIR or PEQ-only main), `ba::target_peq`
   (bus with unity `fixed`), the bundle fit: use the outcome; push
   `Note::LowProtect` whenever `reduced + cuts > 0` (and keep today's note for
   the sub high-pass). Extend `Note::LowProtect` with `reduced: usize` and
   `cuts: usize`; `limited` stays and is now `true` whenever the bound is met
   (it is false only if the bounded loop above ran out). Fix every match on the
   note (engine tests, `secs-service/src/calibration.rs` notice mapping with
   params `reduced` and `cuts` added, `secs-camilla`/`secs-export` if they read
   it).
3. **Mains see the bus rolloff.** BA already computes the bus rolloff in
   `target_peq`; return it from the alignment (e.g. `bus_rolloff_hz:
   Option<f64>` next to the bus PEQ) and give `RcInput` a field
   `bus_rolloff_hz: Option<f64>` (set for mains with a nonzero crossover when
   `low_protect` is on, `None` otherwise). Where `rc.rs` takes the main's
   rolloff for low_protect (the `nb` mask in `prepare_correction`,
   `clamp_centres`, and `limit` in `channel_with_cuts`), use
   `max(own rolloff, bus_rolloff_hz)` (either one alone if the other is
   `None`). Do not change `lf_align`'s `f_lin` (ADR 0025).
4. **Less FIR leakage.** In `finish_correction`'s residual passes, when
   low_protect is on and a rolloff is in force, clamp `broad[i]` to `<= 0`
   for bins below that rolloff.
5. **Screen text.** In `apps/secs2r/ui/src/screens/calibrate/text.ts`, the
   `low_protect` notice line also says how many bands were lowered and how many
   protective cuts were added when those params are nonzero; add the keys to
   `ko.json` and `en.json` (합니다체; no em-dash; labels as noun phrases). Keep
   the existing `limited === "false"` warning path; it should no longer occur
   but must still render. Do not change markup or styles.

## Tests

Update `crates/secs-engine/src/tests/low_protect.rs`:
- `low_protect_realized_guard_scales_leaking_boosts_and_reports_infeasible`
  becomes a test that the formerly infeasible case now ends at or below
  +0.5 dB with protective cuts and `limited = true`.
- New `low_protect_keeps_boosts_that_do_not_leak`: a PEQ with a leaking 22 Hz
  boost and a non-leaking 60 Hz boost (Q high enough that it adds < 0.05 dB
  below the rolloff): only the 22 Hz band is lowered; the 60 Hz band is bit
  identical.
- New `low_protect_room_bass_survives` on the `room` case (Linear, `rc_lo` 20
  and 40): with low_protect on, (a) every main filter (`main.response`) and
  the bus PEQ response stay at or below +0.5 dB below their rolloff; (b) the
  aligned sub sum and the mains' `after` responses at 50, 60 and 80 Hz stay
  within 1.5 dB of the off design (report the numbers); (c) a
  `Note::LowProtect` exists for every channel the guard changed.
- New `low_protect_mains_use_the_bus_rolloff`: a main with a crossover whose
  own response has no rolloff but whose bus does: the main filter stays at or
  below +0.5 dB below the bus rolloff.
Register the new tests under the existing low_protect entries in
`features.toml`.

## You may edit

`crates/secs-engine/**`, `crates/secs-service/src/calibration.rs` (notice
mapping and its tests), `crates/secs-camilla/**` and `crates/secs-export/**`
only where they match on `Note::LowProtect`,
`apps/secs2r/ui/src/screens/calibrate/{text.ts,text.test.ts}`,
`apps/secs2r/ui/src/i18n/{ko,en}.json`, `features.toml`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-engine --locked
cargo test -p secs-service -p secs-export -p secs-camilla --locked
(cd apps/secs2r/ui && npx vitest run src/screens/calibrate src/i18n)
```

Never revert files you did not change.

## Report

The final `limit` and its outcome type, every caller change, the bus-rolloff
plumbing, the numbers each new test checks (on and off), test totals, and the
commands' last lines.
