# P116 (Opus 5.5): output rate on the screen (ADR 0017)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\output-rates`
(branch `claude/output-rates`; P114 and P115 built the conversion and the
IPC). Work only under `apps/secs2r/ui/`. Do not touch Rust, manifests or
bindings, and do not run cargo. Do not commit, do not push. ASTRA writes the
final catalog text and finishes the wiring afterwards (P117).

Read first: `CLAUDE.md` (screen rules), `docs/DESIGN.md`,
`docs/adr/0017-output-sample-rate.md`, the bindings `OutputRateChoice.ts`,
`DspFields.ts`, `DspResult.ts`, `ExportParams.ts`, `ExportResult.ts`,
`Notice.ts` under `apps/secs2r/ui/src/ipc/bindings/`;
`screens/system/SystemScreen.tsx` (`DspCard`, `FIR_TAP_CHOICES`,
`tapLabel`), `screens/system/{types.ts,useSystemScreen.ts,fixtures.ts,text.ts,specimens.tsx}`,
`state/session.ts` (the DSP writer), `screens/calibrate/Settings.tsx` (the
MIMO FIR budget line) and `useCalibrateScreen.ts` (`firBudget`),
`screens/export/ExportScreen.tsx` (`ExportItem`, the convolution row, how
the CamillaDSP device fields sit under their row), `screens/export/{types.ts,useExportScreen.ts,fixtures.ts,specimens.tsx}`,
and `src/ui/` (reuse `Select`, `Segmented`, checkboxes, `Advanced`). Load
the `make-interfaces-feel-better` skill and follow it.

## What it is for

Design stays at 48 kHz; the DSP's output rate decides the rate of the
exported filters. Most users never change it (default 48 kHz). miniDSP runs
at 96 kHz; a PC convolver may run at 44.1 kHz; `자동` reads the default
playback device's rate when exporting. Changing the rate never makes the
result stale; exporting again is enough.

## Rules (fixed)

1. **DSP card.** Under 장치, a row 출력 레이트:
   - Editable (`output_rate_editable`): a select with 자동 and 44.1, 48,
     88.2, 96, 176.4, 192 kHz. `자동` has a one-line help: it reads the
     default playback device's rate when exporting, and uses 48 kHz if it
     cannot. Writes through the existing DSP writer with
     `output_rate: "auto" | { fixed: n }`.
   - Fixed by the device: the value as text (`96 kHz`) with a short reason
     (장치가 정한 값), no control.
   - The tap select's labels (`tapLabel`) and the note's window use the
     output rate (48 kHz for 자동) instead of the literal 48000.
2. **Budget lines.** Where the screen compares FIR taps with the DSP
   (`calibrate` MIMO budget line), convert the design's 48 kHz taps to the
   output rate first (`round(taps × rate / 48000)`; 48 kHz for 자동) and
   name the rate in the line when it is not 48 kHz. If the channel FIR at
   the output rate exceeds `fir_taps`, show one warning line in the same
   place style (the screen already knows the mode's taps from `modeSpec`;
   check). Put the conversion in one pure function with a unit test.
3. **Export screen.**
   - Each filter export row (CamillaDSP, Equalizer APO, 필터 값 텍스트,
     컨볼루션 IR) names the rate it will write when it is not 48 kHz
     (`96 kHz로 씁니다` style, from the DSP fields; 자동 says it reads the
     device). REW mdat says nothing (it stays 48 kHz).
   - The convolution row gains 추가 레이트: checkboxes for the other five
     rates, under the row like the CamillaDSP device fields, collapsed by
     default; checked rates go into `ExportParams.extra_rates`. Keep the
     choice in the export screen's state for the session only.
   - A finished export with an `output_rate_fallback` note shows it as a
     warning line on that row's outcome (cause and what to do: set the rate
     in the DSP card).
4. **Copy.** 합니다체 sentences, noun-phrase labels and states, labels that
   say the effect, UI text at least 14 px, no em-dash. Every new string is a
   key in both catalogs; write first ko and en versions and list the keys.
5. **Wiring.** Extend the view types and hooks so the screens read the new
   fields (`useSystemScreen`, `useExportScreen`, `session.ts`); keep calls
   through the existing IPC wrappers. If something needs a new IPC call,
   stop and report instead of adding one.
6. **Fixtures, specimens, tests.** Gallery specimens: DSP card editable at
   48 kHz, 자동 selected, miniDSP fixed at 96 kHz; export screen with a
   96 kHz rate on the rows, the convolution extra rates open with two
   checked, and a fallback warning after export; the calibrate MIMO budget
   line at 96 kHz over budget. Tests: the select writes the right value,
   fixed rates render as text, the tap and budget conversion function, the
   extra rates reach `ExportParams`, the fallback warning.

## Files

You may edit: `apps/secs2r/ui/src/screens/system/**`,
`screens/export/**`, `screens/calibrate/**` (budget line only),
`state/session.ts`, `src/i18n/ko.json`, `en.json`, gallery registration if
needed, `ipc/mock.ts` and `ipc/fixtures.ts` (fixture values only).

Do not touch: Rust, manifests, bindings, other screens, `.github/`, `docs/`.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm ci
npm run typecheck
npm test
npm run build
```

Use Node 24 (npm 11). Skip `npm ci` if `node_modules` is already current.

## Report

Files changed, keys with their first ko/en versions, specimens, and the
commands' last lines.
