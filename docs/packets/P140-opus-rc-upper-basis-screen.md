# P140 (Opus 5.5): 범위 카드에서 RC 상한 기준을 고릅니다

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-upper`
(branch `claude/rc-upper`). Do not commit, do not push. Work only under
`apps/secs2r/ui/`.

Read first: `CLAUDE.md` (screen rules), `docs/DESIGN.md`, load the
`make-interfaces-feel-better` skill, **`docs/adr/0021-rc-upper-limit-basis.md`**,
then `apps/secs2r/ui/src/screens/calibrate/Settings.tsx` (the range card:
`calibrate.range.upper_auto`, the room volume field, `shownHi`),
`text.ts` (`rc_limit` notice keys), `fixtures.ts`, `useCalibrateScreen*`
(how `changeSetting` turns into a `SettingEdit`), the bindings
`CalibrationSettings.ts`, `RcHiBasis.ts`, `SettingEdit.ts`, and
`apps/secs2r/ui/src/ipc/mock.ts` (how the mock applies calibration edits and
notes).

## What changes on screen

The range card's upper-limit control. Today it is a "상한 자동" switch, a
number field when off and the room volume when on. It becomes:

1. **상한 기준** (a select or segmented control, your call by width;
   `data-ui="calibrate.range.basis"`, options `.strict`, `.lenient`,
   `.agreement`, `.manual`): 엄격한 슈뢰더(기본), 너그러운 슈뢰더, 측정 위치
   일치도, 직접 입력. A help tip beside the label explains each in one
   sentence by its effect, and that raising the limit spreads the
   minimum-delay mode's PEQ bands and depends on an accurate mic calibration
   file (ADR 0021 결과).
2. Shown only for the two Schroeder bases: the room volume (as today) and
   **전이 구간** ×1 / ×2 / ×4 (`data-ui="calibrate.range.transition"`,
   options `.1`, `.2`, `.4`, default ×1).
3. **측정 위치 일치도** is disabled with a visible reason when the project has
   fewer than three measured positions (read the count from the data the
   screen already has; say where), and keeps working when chosen.
4. **직접 입력** shows today's number field (100 Hz to 20 kHz).
5. The RC card's `rc_limit` line states the evidence from the new note params
   (`basis`, `factor`, `band_hz`, `band_t60_s`, `positions`, plus the old
   `hz`, `auto`, `schroeder_hz`):
   'RC 상한 308 Hz(엄격한 슈뢰더 154 Hz × 2)', 'RC 상한 200 Hz(너그러운
   슈뢰더, 200 Hz 대역 T60 0.50초)', 'RC 상한 287 Hz(위치 9곳 일치도)', and
   today's lines for manual and for a missing volume (with the hint to enter
   the volume). Old notes without the new params keep today's text.

Edits go through the same path as today: basis → `SettingEdit` `rc_hi_basis`,
factor → `rc_hi_transition`, manual → `rc_hi` with a number, back to a basis
→ `rc_hi` null plus the basis. Follow the neighbouring controls for layout,
tokens and states; keep the card's height steady when options appear.

## Strings, specimens, tests

- New keys in `ko.json` and `en.json` (합니다체, noun-phrase states, effect
  labels, no em-dash, no particle right after a placeholder). List them;
  ASTRA reviews the catalogs afterwards.
- Gallery specimens: the card in each basis, ×2 and ×4, agreement disabled
  with two positions, manual; RC lines for each evidence kind.
- Tests: each basis sends the right edit; the transition shows only for
  Schroeder bases; agreement disabled below three positions with the reason;
  the evidence lines; old notes keep today's text; the ids above exist.
- Make the mock apply the new edits and return notes with the new params so
  specimens and tests use real flows.

## Files

You may edit: `apps/secs2r/ui/src/screens/calibrate/**`,
`apps/secs2r/ui/src/ipc/mock.ts` (calibration edits and notes only),
`apps/secs2r/ui/src/i18n/ko.json`, `en.json`, `apps/secs2r/ui/src/gallery/**`
and `src/screens/specimens.ts` for registration.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
```

## Report

Files changed, the control layout and why, keys with ko and en, specimens,
tests, and the commands' last lines.
