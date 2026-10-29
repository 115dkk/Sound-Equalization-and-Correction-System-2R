# P111 (ASTRA): text and mock data for the MIMO scorecard

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\mimo-scorecard`
(branch `claude/mimo-scorecard`; P109 built the backend, P110 the screen).
Do not commit, do not push. Work only in this worktree. Do not change the
markup, styles or layout P110 drew.

Read first: `CLAUDE.md`, `docs/packets/P110-opus-mimo-scorecard-screen.md`,
`apps/secs2r/ui/src/screens/calibrate/scorecard.ts`, `Results.tsx`
(`Scorecard`, `ScoreChange`, `LateByPosition`), `apps/secs2r/ui/src/i18n/ko.json`
and `en.json` around `calibrate.score.*` and `calibrate.change.*`,
`docs/UI-TERMS.md`, `apps/secs2r/ui/src/ipc/mock.ts`, `ipc/fixtures.ts`,
`docs/M5-reach.md`, `docs/DESIGN.md` §12.

## Tasks

1. **Catalog.** Rewrite the first versions P110 wrote, in both languages:
   `calibrate.change.aria`, `calibrate.change.aria.mimo`,
   `calibrate.legend.before`, `calibrate.legend.after`,
   `calibrate.score.caption`, `calibrate.score.caption.mimo`,
   `calibrate.score.late`, `calibrate.score.late.band`,
   `calibrate.score.late.note`, `calibrate.score.mimo`,
   `calibrate.score.positions`, `calibrate.score.positions.caption`,
   `calibrate.score.position`, `calibrate.score.position.main`.
   Korean is 합니다체 for sentences and noun phrases for labels, plain and
   short, no em-dash, no dressed-up phrasing; follow the user's Korean
   rules in `CLAUDE.md` and `docs/UI-TERMS.md`. Use one term for the
   measure everywhere (늦은 저음 / late bass unless `UI-TERMS.md` says
   otherwise) and keep placeholders identical across languages. Run the
   catalog's own checks (font subset gate and key parity tests in
   `npm test`).
2. **Mock data.** Give `ipc/fixtures.ts` (and `mock.ts` where it builds
   results) realistic `calibration.late_energy` for every main role at every
   measured position of the mock project, and a MIMO `scorecard` and
   `late_energy` that match the MIMO result the mock returns. Use the same
   reference values P110 used in `screens/calibrate/fixtures.ts` so the
   mock app and the gallery agree. The mock app must then show the late bass
   column and, after a MIMO run, the third values.
3. **Records.** Add the four new `data-ui` ids
   (`calibrate.scorecard.key`, `calibrate.scorecard.late`,
   `calibrate.scorecard.positions`, `calibrate.scorecard.positions.table`)
   and the new specimen ids to `docs/M5-reach.md` and `docs/DESIGN.md` §12
   in those files' existing format. Add a `CHANGELOG.md` entry under the
   next version's section: the scorecard shows after-MIMO figures and late
   bass by position (one or two sentences, 합니다체).

## Files

You may edit: `apps/secs2r/ui/src/i18n/ko.json`, `en.json`,
`apps/secs2r/ui/src/ipc/fixtures.ts`, `ipc/mock.ts`, their tests if values
they assert change, `docs/M5-reach.md`, `docs/DESIGN.md`, `CHANGELOG.md`.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
```

## Report

Each key's final ko and en text, the mock values, files changed, and the
commands' last lines.
