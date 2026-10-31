# P148 (Opus 5.5): extra correction bands on the calibrate screen (ADR 0024)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-bands`
(branch `claude/rc-bands`). Do not commit, do not push. Two workers edit Rust
at the same time (P146 engine, P147 service); work only under
`apps/secs2r/ui/` and `docs/M5-reach.md`.

Read first: `CLAUDE.md` (screen rules), `docs/DESIGN.md`, load the
`make-interfaces-feel-better` skill, `docs/adr/0024-rc-extra-bands.md`, the
bindings `src/ipc/bindings/{RcBand,CalibrationSettings,SettingEdit,CalibrationLimits,ResultCurvesResult,rules}.ts`
(already regenerated), then `src/screens/calibrate/Settings.tsx`
(`RangeSection`), `settingsView.ts`, `text.ts`, `ResultGraph.tsx`,
`src/ui/plot/LogPlot.tsx`, `src/ipc/mock.ts`, `src/screens/calibrate/fixtures.ts`.

## Interface (fixed)

- Edit: `{ kind: "rc_bands", bands: RcBand[] }` replaces the whole list;
  `[]` clears it. Limits: `CALIBRATION_LIMITS.rc_band` (20–20000 Hz each
  edge), `CALIBRATION_LIMITS.rc_bands_max[1]` (4 bands). The primary lower
  edge now accepts `CALIBRATION_LIMITS.rc_lo` = 20–200.
- The service rejects with `invalid_request` and params (mirror them in the
  mock):
  - `field=rc_bands, reason=rc_band_count, max`
  - `field=rc_bands, reason=rc_band_range, index (1-based), lo_hz, hi_hz`
  - `field=rc_lo, reason=rc_lo_range, min, max`
  - `field=rc_lo, reason=rc_lo_above_hi, hi_hz`
  - `field=rc_hi, reason=rc_hi_range, min, max`
  - `field=rc_hi, reason=rc_hi_below_lo, lo_hz`
  Valid lists come back sorted by `lo_hz`.
- Graph: `ResultCurvesResult.rc_bands_hz: [lo, hi][]` (extra bands); shade
  outside the union of `rc_band_hz` and these, with dashed edges per band.
- Notice `rc_bands` with param `bands` = `"lo-hi,lo-hi"` (the merged band
  set after calculation), shown where `rc_limit` evidence reaches the screen
  (`text.ts`), only when present.

## What to build

1. **Range card** (`RangeSection`): below the primary lower and upper
   controls, an action '보정 구간 추가' (`data-ui="calibrate.range.extra.add"`).
   With no extra bands only this action shows (disclosure). Each added band
   is a row `calibrate.range.extra.<n>` (n = 1..4) with lower
   `calibrate.range.extra.<n>.lower`, upper `calibrate.range.extra.<n>.upper`
   (NumberField, Hz, step sensible for the band: 5 below 1 kHz, 50 above),
   and remove `calibrate.range.extra.<n>.remove`. A new row starts at a
   default that is valid and outside the primary band (for example
   15000–20000 when free, otherwise the next free span), so adding never
   produces an immediate error. Hide the add action at 4 bands. A rejected
   value shows its reason next to that input (from the error params) and the
   stored value stays. Keep 14 px minimum text, effect labels, noun-phrase
   states, 하십시오체 sentences.
2. **Primary lower edge**: its NumberField now uses min 20 (from
   `CALIBRATION_LIMITS.rc_lo`); show the `rc_lo_range`, `rc_lo_above_hi` and
   `rc_hi_below_lo` reasons on the matching input.
3. **Graph**: shading and labels follow the band union (`LogPlot`,
   `ResultGraph`); with no extra bands it looks exactly as today.
4. **Notice text** for `rc_bands` in the calculation notes (for example
   'RC 추가 구간 15000~20000 Hz' listing every band of the merged set after
   the primary).
5. **Mock** (`src/ipc/mock.ts`): apply the edit with the same validation
   and params; `rc_bands_hz` from the settings; a plausible `rc_bands`
   notice after a calibration with bands. Fix `CalibrationSettings` objects
   in fixtures and tests (`rc_bands: []`).
6. Strings: new keys in `ko.json` and `en.json` (list them in the report;
   ASTRA reviews the catalogues afterwards).
7. Gallery specimens: range card with no extra band, with two bands, a
   rejected band (reason shown), four bands (add hidden); result graph with
   an extra band shaded; the `rc_bands` notice.
8. `docs/M5-reach.md`: rows for the new `calibrate.range.extra*` ids, like
   the existing `calibrate.range.*` rows.

## Tests

- Adding a band sends one `rc_bands` edit with a valid default; removing
  sends the list without it; editing an edge sends the whole list.
- Each rejection reason renders on the right input; the stored value stays.
- Add is hidden at 4; the card shows only the add action with none.
- Primary lower below 20 is rejected with its reason.
- Graph shading includes the extra band and is unchanged with none.
- The `rc_bands` notice text in ko and en.
- New ids exist.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
```

## Report

Files changed, where each piece sits and why, the keys with ko and en, the
specimens, the tests, and the commands' last lines.
