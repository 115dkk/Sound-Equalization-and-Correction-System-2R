# P152 (Opus 5.5): LF arrival and room-ringing switches on the calibrate screen (ADR 0025)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`
(branch `claude/lf-align`). Do not commit, do not push. Other workers change
Rust crates at the same time; touch only `apps/secs2r/ui/**`.

Read first: `CLAUDE.md` (screen rules: 합니다체 sentences, noun-phrase
labels and states, labels that say the effect, 14 px minimum, new text as
keys in both `ko` and `en`), `docs/adr/0025-lf-arrival-and-mode-decay.md`
(section 화면과 저장), `apps/secs2r/ui/src/screens/calibrate/Settings.tsx`
(`Settings`, `RangeSection` with the `low_protect` toggle, `MimoSection`),
`text.ts` (the `rc_bands` and `low_protect` notice cases), `Results.tsx`,
`specimens.tsx`, `fixtures.ts`, `src/ipc/mock.ts` and its tests.

The bindings are regenerated already: `CalibrationSettings.lf_align`,
`CalibrationSettings.mode_decay`, and `SettingEdit` kinds `lf_align` and
`mode_decay` (`{ kind, on }`).

## Design (fixed)

The maintainer decided these are official, easy alternatives on the main
screen, never under 고급 or 실험 (2026-10-31). Both default off.

1. **Card.** A new `SettingsCard id="lf"` placed directly before
   `MimoSection` in `Settings`, title key `calibrate.lf.title`, help key
   `calibrate.lf.description`. It is a normal card like `mimo`, not an
   `Advanced` disclosure.
2. **Toggle `calibrate.lf_align`** (label key `calibrate.lf_align`, help key
   `calibrate.lf_align.help`), `changeSetting({ kind: "lf_align", on })`.
   - Zero mode (`s.mode === "zero"`): disabled and shown unchecked, hint key
     `calibrate.lf_align.unavailable_zero` (Zero mode does not change
     phase). The stored value is not changed by switching modes.
   - Mixed mode with the toggle on: a normal (not warning) hint
     `calibrate.lf_align.mixed` (the short delay of Mixed mode aligns only
     part of the way).
3. **Toggle `calibrate.mode_decay`** (label `calibrate.mode_decay`, help
   `calibrate.mode_decay.help`), `changeSetting({ kind: "mode_decay", on })`,
   available in every mode.
4. **Notices** in `text.ts`, next to `rc_bands`:
   - `lf_align` (params `role`, `strength`, `limited`, optional
     `arrival_ms`): key `notice.lf_align` with `{role}`, `{strength}` (as a
     percentage of the 0–1 value), and `{arrival}` (ms, one decimal) when
     present; `notice.lf_align.no_arrival` when absent; when
     `limited === "true"` append `notice.lf_align.limited`.
   - `mode_decay` (params `role`, `hz`, `t60_s`, `target_s`, `depth_db`):
     key `notice.mode_decay`.
   - `mode_decay_none` (params `role`, `target_s`): key
     `notice.mode_decay_none`.
   Format numbers with the existing helpers the other notices use.
5. **Mock and fixtures.** `mock.ts` applies the two new edits like
   `low_protect`; fixtures and the gallery specimens include the new card in
   its off state and one specimen with both on (Linear) and one in Zero mode
   showing the disabled hint. Add result notices for the three codes to the
   specimen that shows notices.
6. **Catalog.** Add every key above to `src/i18n/ko.json` and `en.json` with
   a first draft. Korean labels: `저음 도착 맞추기`, `방 울림 줄이기`; card
   title a short noun phrase about bass timing and ringing. Help text says
   in one or two 합니다체 sentences what changes and what it costs
   (lf_align: all mains' bass arrives with the treble at the same time, at
   the price of some pre-ringing, bounded; mode_decay: long room modes ring
   shorter through narrow cuts, at most 12 dB each). ASTRA rewrites the
   catalog text after you; keep the keys and placeholders stable.

## Do not

- Do not touch Rust, bindings, `ipc/bindings/**`, or keys you did not add.
- Do not put either toggle in an `Advanced` block or call them experimental.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui && npm ci && npm run typecheck && npm test && npm run build
```

Use Node 24 (npm 11). If an i18n or font-subset test fails because of new
glyphs, report it with the message; do not edit the font files.

## Report

Files changed, where the card sits, the keys added with their draft ko/en
text, the specimens added, and the last lines of each command.
