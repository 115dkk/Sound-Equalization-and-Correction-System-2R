# P102 (Opus 5.5): the export screen for convolution IR, design names and CamillaDSP devices

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\export-bundle`
(branch `claude/export-convolution`; P100 and P101 are committed). Work only
there, under `apps/secs2r/ui/`. Do not commit, do not push. Claude reviews
the diff and commits. ASTRA wires the hook and writes the final catalog text
afterwards (P103), so leave `useExportScreen.ts` to ASTRA except for what
rule 5 says.

Read first: `CLAUDE.md` (the screen rules), `docs/DESIGN.md`,
`apps/secs2r/ui/src/screens/export/` (all files: `types.ts`, `ExportScreen.tsx`,
`export.css`, `specimens.tsx`, `fixtures.ts`, the tests),
`apps/secs2r/ui/src/ipc/bindings/ExportKind.ts`, `ExportParams.ts`,
`CamillaDevices.ts`, `CamillaCaptureDevice.ts`, `CamillaPlaybackDevice.ts`,
`CamillaWasapiFormat.ts`, `MeasureDevice.ts`, `MeasureMethodsResult.ts`,
`apps/secs2r/ui/src/i18n/ko.json` and `en.json` (the `export.*` keys), and
`docs/DEVIATIONS.md` F20 and F21. Load the `make-interfaces-feel-better`
skill and follow it.

## What the screen is for

The export screen writes the designed filters for the user's DSP or player.
Three things are new, from a user who is replacing Dirac Live Processor with
CamillaDSP and from a forum request for convolution files:

1. **컨볼루션 IR** (`ExportKind` `convolution`): one WAV per input→output
   pair holding the whole correction (FIR, PEQ, crossover, all-pass, gain,
   polarity, delay), plus a CSV of each pair's response, `routing.json` and a
   guide. For Equalizer APO's `Convolution:`, Roon, JRiver, HQPlayer,
   foobar2000. It carries subs, LFE and MIMO, so it is never blocked by
   `export_block`.
2. **설계 이름** (`ExportParams.design_name`): optional, for the filter
   exports (CamillaDSP, 필터 값 텍스트, Equalizer APO, 컨볼루션 IR). Without a
   name, files go to a timestamped folder as today and the record calls the
   design by its mode name. With a name, the folder is `SECS_<kind>_<name>`
   and exporting again with the same name **replaces that folder, files the
   user added there included**. The screen must say so before the user
   exports, next to the field, in one sentence.
3. **CamillaDSP 장치** (`ExportParams.camilla_devices`): capture and
   playback, each `file` (today's placeholders, the default), `wasapi`
   (device by name, format `S16`/`S24`/`S32`/`F32`, exclusive, and for
   capture loopback) or `asio` (driver name; CamillaDSP must be built with
   `asio-backend`, its released Windows binaries have no ASIO, which the
   screen says when ASIO is chosen). The WASAPI device list comes from
   `measure_methods().native_devices` (name, input and output channel
   counts); capture lists devices with inputs (or every output device when
   loopback is on), playback devices with outputs. When the list is
   unavailable (not Windows, or enumeration failed), the user can still type
   a name.

## Rules (fixed)

1. **Order** of the kinds: `camilla_dsp`, `filter_text`, `equalizer_apo`,
   `convolution`, then the REW group (`before_after_mdat`, `staged_mdat`
   under 고급) as today. Update `EXPORT_KINDS` and everything keyed by it.
2. **View model** (`types.ts`): add to `ExportView`
   `designName: string` (the field's text, `""` when unset),
   `camillaDevices: CamillaDevices` (default both `{ type: "file" }`),
   `nativeDevices: MeasureDevice[] | null` (`null` = not available); add to
   `ExportActions` `setDesignName(name: string)` and
   `setCamillaDevices(devices: CamillaDevices)`. Add a pure
   `designNameProblem(name: string): DesignNameProblem | null` mirroring the
   service's rules (trimmed length 1 to 64; no control characters; none of
   `\ / : * ? " < > |`; not ending in `.` or space; not `CON`, `PRN`, `AUX`,
   `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9` before the first dot, any case) with
   one problem value per rule, and unit-test it. A problem blocks the filter
   exports' buttons and shows its message under the field; an empty field is
   not a problem.
3. **Layout.** The design name field sits once, above the DSP filter group,
   not inside each row. The CamillaDSP devices sit inside the CamillaDSP row
   behind a disclosure that is closed by default and whose summary reads the
   current choice (for example `파일 자리표시` or `WASAPI · Speakers (Tesira)`).
   The convolution row reads like its neighbours (what it writes, where it
   goes, its outcome). Keep one filled primary action per screen as today.
4. **Copy.** Sentences in 합니다체, labels and states as noun phrases, labels
   that say the effect, UI text at least 14 px. Every new string is a key in
   both `ko.json` and `en.json`. Write a first version of each Korean and
   English text yourself, and list every new key in your report: ASTRA
   rewrites the text in P103.
5. **Hook.** Only so the screen runs: in `useExportScreen.ts` return the new
   view fields from simple sources (`designName` and `camillaDevices` from
   the session host state with the defaults above, `nativeDevices: null`) and
   make the two new actions set the host state. Do not change what
   `exportFiles` sends; ASTRA does that.
6. **Specimens and tests.** Add specimens for: the convolution row (idle,
   done, failed), the design name field (empty, named, each problem), the
   devices disclosure (closed, file, WASAPI with a list, WASAPI typed name,
   ASIO with its note). Extend `ExportScreen.test.tsx` and
   `types.test.ts` for the order, the name problems blocking the buttons, and
   the disclosure summary text.

## Files

You may edit: `apps/secs2r/ui/src/screens/export/**`,
`apps/secs2r/ui/src/i18n/ko.json`, `apps/secs2r/ui/src/i18n/en.json`, and the
session host state type that holds `mdatScope` if the two new host fields
must be declared there (find it from `useExportScreen.ts`).

Do not touch: Rust code, `apps/secs2r/ui/src/ipc/bindings/**`, other
screens, `.github/`, `docs/`.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm ci
npm run typecheck
npm test
npm run build
```

## Report

Changed files; the new view fields and actions; every new i18n key with
your draft Korean and English; screenshots are not needed; the tail of each
verification command; anything left undone.
