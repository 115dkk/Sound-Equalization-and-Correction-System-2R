# P110 (Opus 5.5): MIMO 후 성적표와 위치별 늦은 저음

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\mimo-scorecard`
(branch `claude/mimo-scorecard`; P109 added the backend). Work only under
`apps/secs2r/ui/`. Do not touch Rust, manifests, `Cargo.lock`, bindings,
`.github/` or `docs/`, and do not run cargo. Do not commit, do not push.
ASTRA writes the final catalog text and the IPC mock values afterwards (P111).

Read first: `CLAUDE.md` (the screen rules), `docs/DESIGN.md`,
`apps/secs2r/ui/src/screens/calibrate/Results.tsx` (`Verdict`, `Scorecard`,
`Change`), `calibrate.css`, `fixtures.ts`, `specimens.tsx`,
`CalibrateScreen.test.tsx`, `apps/secs2r/ui/src/ui/` (reuse `DataTable`,
`Card`, `ui-details`), and the bindings `CalibrationSummary.ts`,
`MimoSummary.ts`, `LateEnergy.ts`, `MimoChannelScore.ts`,
`MimoLateEnergy.ts`, `MetricsSummary.ts`, `ChannelResultLine.ts` under
`apps/secs2r/ui/src/ipc/bindings/`. Load the `make-interfaces-feel-better`
skill and follow it.

## What it is for

MIMO adds support speakers to each main channel to shorten the bass that
keeps ringing after the direct sound. Until now the screen showed only the
correction's before and after, so a user could not see what MIMO changed.
The backend now sends, in `view.results`:

- `calibration.late_energy`: for every main role at every measured position,
  the late 20–150 Hz energy (150–400 ms against the early part) before and
  after correction, in dB. Lower is better (less ringing).
- `mimo.scorecard`: for every main role, the upstream metrics after MIMO at
  the main position (`after_mimo.in1_20` is the same measure as the
  scorecard's ±1 dB column, `after_mimo.gd_20` the same as its GD column).
- `mimo.late_energy`: the late energy after MIMO at every position.

The main position is the smallest measured position number (the engine picks
it that way). A user should see, per channel, what correction did and then
what MIMO did, and whether the ringing went down at the other seats too.
MIMO often trades a little flatness for less ringing (in the reference case
±1 dB share drops 64 → 54 % while late energy drops −16.3 → −23.1 dB), so
the three numbers have to sit side by side, not on separate screens.

## Rules (fixed)

1. **Scorecard column.** `Scorecard` gains a third column 늦은 저음, band
   label `20–150 Hz`, the main position's before → after in dB with one
   decimal (minus signs as the number formatter gives them). Rows without a
   late-energy entry show an em-dash-free empty mark the table already uses
   for missing values, or nothing.
2. **After MIMO.** When `results.mimo` is present, not stale, and has a
   `scorecard` row (and for the late column a `late_energy` row at the main
   position) for the role, each of the three cells shows a third value:
   before → after → MIMO. The third value must be told apart at a glance
   (for example the colour the MIMO curve uses in the graph, or a small
   `MIMO` tag; find what the result graph uses for MIMO and reuse it) and
   the reader must learn what it is without a tooltip: a legend line or a
   column sub-label that appears only while the third value is shown. The
   screen reader hears 보정 전 …, 보정 후 …, MIMO 후 …. When MIMO is absent
   or stale, cells stay two-valued and nothing mentions MIMO. Keep the
   `낮은 순` sort as it is.
3. **Positions.** When `late_energy` covers more than one position, the
   scorecard card ends with a collapsed `details` (the `ui-details` the
   verdict's notes use) 위치별 늦은 저음: a table whose rows are the main
   roles in `main_roles` order and whose columns are the positions in
   ascending order, the main position's header marked as the reference.
   Cells: before → after, plus → MIMO under the same rule as 2. The table
   scrolls sideways inside the card when it does not fit; the page never
   scrolls sideways. With one position, no details.
4. **Copy.** 합니다체 sentences, noun-phrase labels, UI text at least 14 px,
   no em-dash. Explain 늦은 저음 once in plain words where a user meets it
   (caption or legend): what it measures and that lower means shorter
   ringing. Every new string is a key in both `ko.json` and `en.json`; write
   a first version of each yourself and list every key in the report.
5. **No new IPC.** Everything comes from `view.results`; do not change
   `types.ts`'s view shape unless a derived helper belongs there. Put the
   pure derivation (rows, which values to show, main position) in a small
   function in `screens/calibrate/` and unit-test it.
6. **Fixtures and specimens.** In `screens/calibrate/fixtures.ts` give the
   result fixtures realistic `late_energy` and MIMO `scorecard`/`late_energy`
   (reference values: L in1_20 64→54 %, gd_20 47.5→41.1 ms, late −23.9 /
   −16.3 / −23.1 dB; R 65→56 %, 47.3→45.2 ms, −24.0 / −17.0 / −21.6 dB;
   C 62→55 %, 47.7→45.1 ms, −17.0 / −16.0 / −18.7 dB; other positions a few
   dB apart). Gallery specimens: scorecard with fresh MIMO (three values);
   positions details open with three positions; MIMO stale (two values);
   one position only.
7. **Tests.** Three values with fresh MIMO; two values when stale or absent;
   the late column; positions details only with more than one position,
   main position marked; the derivation function.

## Files

You may edit: `apps/secs2r/ui/src/screens/calibrate/**`,
`apps/secs2r/ui/src/i18n/ko.json`, `en.json`, and gallery registration if
specimens need it.

Do not touch: Rust, manifests, bindings, `ipc/mock.ts`, `ipc/fixtures.ts`,
other screens, `.github/`, `docs/`.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm ci
npm run typecheck
npm test
npm run build
```

Use Node 24 (npm 11). If `npm ci` was already run in this worktree, skip it.

## Report

Files changed, the catalog keys with their ko/en first versions, the
specimens added, how the MIMO value is told apart, and the four commands'
last lines.
