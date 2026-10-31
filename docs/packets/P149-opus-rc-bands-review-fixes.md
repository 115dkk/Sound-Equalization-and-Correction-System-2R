# P149 (Opus 5.5): fixes from the review of the extra correction bands (ADR 0024)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-bands`
(branch `claude/rc-bands`, commits d1c932e4 and 66991c53 hold P146–P148). Do
not commit, do not push.

Read first: `CLAUDE.md`, `docs/adr/0024-rc-extra-bands.md`, then the files
named below. An independent review found these defects; fix each one and add
a test that fails before the fix.

## 1. CamillaDSP and Equalizer APO headers (high)

`crates/secs-camilla/src/note.rs` (~line 125) appends `"\nRC extra bands
15000–20000 Hz"` to the note. Both writers print the note as one comment
line `# {note}` after dropping non-ASCII (`crates/secs-camilla/src/lib.rs`
~1300-1308, `crates/secs-filters/src/lib.rs` ~404-408). The second line
leaves the comment (CamillaDSP YAML no longer parses; the EQAPO file gets a
non-comment line) and the en dash is dropped ("1500020000").

Fix: keep the note on one line and ASCII only, for example
`...; no MIMO; RC extra bands 15000-20000 Hz, 30-60 Hz`. Update the tests
that lock the old text (`crates/secs-service/src/export_run_tests.rs` ~100
and any in secs-camilla), and add tests that (a) the CamillaDSP export with
an extra band parses as YAML with the comment as the only header line
(serde_yaml or the crate's own YAML reader), and (b) every header line of the
EQAPO filter file with an extra band starts with `#`.

## 2. An extra high band changes the low Zero-mode fit (medium)

`crates/secs-engine/src/rc.rs` (~299) passes the band set to every Zero-mode
fit; `fit.rs` (`in_range` ~33, centre bounds ~129-138) then narrows the low
fit (15–400 Hz) to the primary band: with `rc_lo=20`, auto `rc_hi=300` and
one extra band 15000–20000, 15–20 Hz and 300–400 Hz drop to weight 0.15 and
low-fit centres are held to [20, 300] instead of [15, 2000]. Adding a band at
15 kHz must not change the 20–300 Hz result.

Fix: for each Zero-mode fit, pass `bands: None` (legacy) unless an extra
band overlaps that fit's own interval. When one does, pass the union of the
primary coverage `[fit_lo, rc_hi·√2]` and each extra band's coverage
`[lo/√2, hi·√2]`, so the primary part keeps its legacy weights. Test: Zero
mode, primary 20–auto/300, with and without an extra 15000–20000 band, the
low-fit PEQs (frequency, Q, gain) are identical; an extra band that overlaps
the low fit (for example 150–250 Hz with a manual primary 20–100) still
excludes the gap 141–150 Hz from seeds.

## 3. Phase-strength error range drops 20 Hz–rc_lo (low)

`rc.rs` ~687-698: with extra bands, the primary interval of the realizer
acceptance band starts at `rc_lo` instead of `max(20, lo_fir)`. Use
`max(20, lo_fir)` for the primary interval like the legacy branch. Test with
`rc_lo=40` and an extra band.

## 4. GD cuts with an inverted primary band (low)

`fit.rs` ~549-552: `band_set` keeps an inverted primary (for example
`rc_lo=150` with an automatic upper limit of 126), `band_grid` builds a
falling grid and `.expect("GD peak belongs to a correction band")` or
`.expect("valid GD fit")` panics (reachable through the engine API with
`Tuning.gd_cuts=true`). Make grids skip intervals with `lo >= hi`, skip a
peak that belongs to no band, and skip a fit whose bounds are empty, instead
of panicking. Keep `band_set`'s output unchanged when there are no extra
bands (bit-identical legacy). Test: no panic and no cut outside the bands.

## 5. Graph labels with an inverted primary band (low)

`apps/secs2r/ui/src/ui/plot/bands.ts` ~32: with lo=150, hi=126 `findIndex`
returns -1 and both RC labels vanish (they showed before). Fall back to the
legacy labels from `rc_band_hz`. Test in `bands.test.ts`.

## 6. Unchecked settings and docs (low)

- `CalibrateParams.settings` (the request that carries settings) reaches the
  engine as given. In the engine's settings-to-config conversion
  (`crates/secs-engine/src/calibration.rs` ~225), drop extra bands that break
  `20 <= lo < hi <= 20000` (or are not finite), sort them, keep the first 4,
  the same rule the project reader applies. Test it.
- `docs/DEVIATIONS.md` D48: also state that out-of-range `rc_hi` edits and
  crossed manual edges are now refused where upstream clamps or accepts
  them. `docs/M5-reach.md`: the `calibrate.range.upper` row points at D48
  instead of '편차 없음'.

## You may edit

`crates/secs-engine/**`, `crates/secs-camilla/**`, `crates/secs-filters/**`,
`crates/secs-service/**`, `apps/secs2r/ui/src/ui/plot/**`,
`docs/DEVIATIONS.md`, `docs/M5-reach.md`. Not secs-types, not the ADR.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-engine -p secs-camilla -p secs-filters -p secs-service --locked
cd apps/secs2r/ui && npm run typecheck && npm test
```

## Report

Each fix with file:line, the test that covers it (and that it failed before),
and the commands' last lines.
