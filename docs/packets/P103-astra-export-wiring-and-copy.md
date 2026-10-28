# P103 (ASTRA): wire the export screen and write its text

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\export-bundle`
(branch `claude/export-convolution`; P100, P101 and P102 are in it). Work
only there. Do not commit, do not push. Claude reviews the diff and commits.

Read first: `CLAUDE.md`, `docs/packets/P102-opus-export-screen.md` (what the
screen now holds) and its result in `apps/secs2r/ui/src/screens/export/`,
`apps/secs2r/ui/src/screens/export/useExportScreen.ts` and its test,
`apps/secs2r/ui/src/state/` (how resources and the session host work),
`apps/secs2r/ui/src/ipc/bindings/ExportParams.ts`, `CamillaDevices.ts`,
`MeasureMethodsResult.ts`, `ExportBlock.ts`, the `export.*` and error-code
keys in `apps/secs2r/ui/src/i18n/ko.json` and `en.json`,
`crates/secs-service/src/design_export.rs` (`validate_design_name` and the
reason strings it returns), `docs/DEVIATIONS.md` F20 and F21.

## Rules (fixed)

1. **Wiring** in `useExportScreen.ts` (and the state files it needs):
   - `exportFiles(kind)` sends `design_name` for the filter kinds
     (`camilla_dsp`, `filter_text`, `equalizer_apo`, `convolution`) when the
     field holds a name without a problem (trimmed), and nothing otherwise;
     it sends `camilla_devices` with `camilla_dsp` only when either side is
     not `file`; the REW mdat keeps its `mdat_scope` as today.
   - `nativeDevices` comes from `measure_methods` (`native_devices`), read
     once when the screen first needs it and refreshed when the user opens
     the devices disclosure; `null` while unavailable or on error.
   - The design name and the devices live in the session host state
     (`state/session.ts` already declares `designName` and
     `camillaDevices`). Opening or creating a project resets `designName` to
     `""`, where `state/effects.ts` resets `mdatScope`. `camillaDevices` is
     not reset: it describes the user's CamillaDSP machine, not a project.
   - When the service rejects a design name (`invalid_request` with one of
     the reason strings `validate_design_name` returns), the row shows the
     same message the field shows for that problem.
   Extend `useExportScreen.test.tsx` for each of these.
2. **Text.** Rewrite every key P102 added (its report lists them; diff the
   catalogs against `HEAD~` to be sure) in Korean and English. Korean:
   합니다체 sentences, noun-phrase labels and states, labels that say the
   effect, no dressed-up or sentimental phrasing, no em-dash, and the user's
   own words for the parts (설계 이름, 컨볼루션 IR, 파일 자리표시, 독점 모드,
   루프백). The design-name warning says plainly that exporting again under
   the same name replaces that folder and deletes other files in it. The
   ASIO note says CamillaDSP must be a build with ASIO support and that the
   released Windows binaries have none.
3. **Filter text refused for subs or MIMO** (`ExportBlock` `subs`, `mimo`):
   keep the reason, and add what to use instead: Equalizer APO, CamillaDSP
   or 컨볼루션 IR carry subs and MIMO.
4. Keep `ko.json` and `en.json` key sets and placeholders identical and run
   the i18n tests and any font-subset gate the UI has.
5. **The screen's IPC mock and fixtures** (the typecheck fails today):
   `src/ipc/fixtures.ts` builds an `UpstreamBase` without the `repository`
   field P101 added; `src/ipc/mock.ts` has no `convolution` entry in
   `EXPORT_FOLDER_TAG`, and its export branch writes a `SECS_undefined_…`
   folder and a `convolution.mdat` for the new kind. Make the mock export
   for `convolution` behave like the real service: a
   `SECS_convolution_<what>_<mode>_<time>` folder (or `SECS_convolution_<name>`
   with a design name) holding one `<input>_to_<output>.wav` and `.csv` per
   pair of the mock project's chain, `routing.json`,
   `컨볼루션 적용 방법.txt` and `secs-design.json`. Give the other filter kinds'
   mock exports the same design-name folder rule and a `secs-design.json`, and
   the CamillaDSP mock export its `<name>_bypass.yml`. Read
   `crates/secs-service/src/export.rs` and `design_export.rs` for the rules.
   Extend the mock tests (`mock.test.ts`, `mockWiring.test.ts`).

## Files

You may edit: `apps/secs2r/ui/src/screens/export/useExportScreen.ts` and its
test, `apps/secs2r/ui/src/state/**` only where the export host state lives,
`apps/secs2r/ui/src/ipc/mock.ts`, `apps/secs2r/ui/src/ipc/fixtures.ts` and
their tests, `apps/secs2r/ui/src/i18n/ko.json`, `apps/secs2r/ui/src/i18n/en.json`,
and the export screen's tests if a text change breaks an assertion.

Do not touch: markup and styles (`ExportScreen.tsx`, `export.css`,
`specimens.tsx`) except to fix a key name, Rust code, bindings, other
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

Changed files; for every key, the final Korean and English; the tail of
each verification command; anything left undone.
