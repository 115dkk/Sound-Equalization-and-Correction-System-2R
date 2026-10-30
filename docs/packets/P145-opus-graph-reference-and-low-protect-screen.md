# P145 (Opus 5.5): 측정 그래프의 0 dB 기준과 극저역 보호 스위치

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-upper`
(branch `claude/rc-upper`). Do not commit, do not push. Work only under
`apps/secs2r/ui/`.

Read first: `CLAUDE.md` (screen rules), `docs/DESIGN.md`, load the
`make-interfaces-feel-better` skill, `docs/adr/0022-low-end-protection.md`,
`docs/DEVIATIONS.md` rows U17 and D47, then the screens named below.

## 1. Measurement graph reference (#127, U17)

The service now subtracts one reference from every measurement curve and
returns it: `MeasurementCurvesResult.reference: CurveReference | null`
(`role`, `position`, `band: "mid" | "sub"`; bindings in
`src/ipc/bindings/`). Before, each curve was shifted to its own
200–2000 Hz mean, which lifted subwoofers by tens of dB.

- In the measure screen's graph (`src/screens/measure/MeasureScreen.tsx`,
  around the curves passed to the plot), show what 0 dB means when
  `reference` is not null: for `mid`, '0 dB = L 위치 1의 200~2000 Hz 평균';
  for `sub`, '0 dB = SW1 위치 1의 40~120 Hz 평균' (role and position from the
  field). Place it where the graph's axis or legend text already sits; it
  stays when the reference curve is hidden by a filter. Hide it when null.
- Make the mock (`src/ipc/mock.ts`) return a `reference` consistent with its
  curves (and stop shifting each mock curve to its own midrange if it does).
- If the y range used by the graph cuts off typical curves now (subs no
  longer sit far above mains), adjust only what the screen controls and say
  why.

## 2. 극저역 보호 switch (ADR 0022, D47)

- Setting `CalibrationSettings.low_protect` (default false); edit
  `calibration_settings_edit` with `{ kind: "low_protect", on: boolean }`.
- A switch in the calibrate settings range card
  (`src/screens/calibrate/Settings.tsx` `RangeSection`), next to the boost
  limit, `data-ui="calibrate.range.low_protect"`. Label by effect (for
  example '롤오프 아래 부스트 막기'; your call with ASTRA reviewing), and a
  help tip in one or two sentences: finds each channel's rolloff (10 dB below
  its own level), stops boosting below it, and cuts subs half an octave below
  with a 24 dB/oct high-pass unless the user set one; quieter deepest bass in
  exchange for protecting the drivers. Default off, same results as before.
- Evidence: the engine emits notice `low_protect` per protected channel with
  string params `role`, `rolloff_hz`, `limited` (`"true"` | `"false"`),
  optional `hp_hz`. Show them where the result card already lists notices
  for the correction (follow how `rc_limit` and other notices reach the
  screen in `text.ts`), e.g. 'SW1: 22 Hz 아래 부스트 안 함, 16 Hz 하이패스',
  'L: 30 Hz 아래 부스트 안 함'. `limited="false"` means the boost could not
  be held to +0.5 dB even with every eligible boost removed and the original
  fit was kept: show it as a warning with what to do (lower the boost limit
  or raise the RC lower limit), never as success. A group note may carry
  several roles joined by ' + '.
- The mock applies the edit and returns plausible `low_protect` notices
  after a calibration with it on, so specimens and tests use real flows.

## Strings, specimens, tests

- New keys in `ko.json` and `en.json` (합니다체, effect labels, noun-phrase
  states, no em-dash, no particle right after a placeholder). List them;
  ASTRA reviews the catalogs afterwards.
- Gallery: the measure graph with a sub and mains showing the reference
  line text (mid) and a sub-only project (sub); the range card with the
  switch off and on; result notices for a protected sub with a high-pass, a
  main, and a `limited="false"` warning.
- Tests: the reference text for mid, sub and null; it does not change when a
  curve is filtered; the switch sends the edit and reflects the setting; the
  notice lines for each case including the warning; ids exist.

## Files

You may edit: `apps/secs2r/ui/src/screens/measure/**`,
`apps/secs2r/ui/src/screens/calibrate/**`, `apps/secs2r/ui/src/ipc/mock.ts`,
`apps/secs2r/ui/src/ipc/mock.test.ts`, `apps/secs2r/ui/src/i18n/ko.json`,
`en.json`, `apps/secs2r/ui/src/gallery/**`, `src/screens/specimens.ts` for
registration. Not bindings, not Rust.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
```

## Report

Files changed, where each piece sits and why, keys with ko and en,
specimens, tests, and the commands' last lines.
