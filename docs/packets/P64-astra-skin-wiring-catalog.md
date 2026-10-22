# P64 (ASTRA): wire the skin setting and settle its catalog strings

Checkout: `/home/user/Sound-Equalization-and-Correction-System-2R` (branch
`ccr-aa2605d0-38ypeu`). Do not commit; Claude reviews the diff and commits.
Use absolute paths. Node 24 is at `/opt/node24/bin`; prepend it to `PATH`
for every npm/npx command. `npm ci` has already been run.

SECS 2R now has skin sets (ADR 0010, `docs/skins/`). Daybreak added
`AppSettings.skin: AppSkin` (`default` · `rack` · `matrix`, bindings in
`apps/secs2r/ui/src/ipc/bindings/AppSkin.ts`), Opus 5.5 drew the two skins
and put a skin choice in the settings popover (`src/app/menus.tsx`,
`frame.settings.skin*` keys), and `PrefsScope` (`src/app/prefs.tsx`)
applies `data-skin`. What is still missing is the link between the
popover and the service, and a final look at the four strings.

Read first: `CLAUDE.md` (screen rules, the catalogs' register),
`docs/DESIGN.md` §2.5, §7, §10, `docs/UI-TERMS.md`, `docs/skins/README.md`
(what the three skins are called), `src/app/prefs.tsx`, `src/App.tsx`
(`Preferences`: the initial prefs come from the service's `appSettings`
resource and `onChange` writes back through `app_settings_set`),
`src/state/flow.test.tsx` (the test that already pins the logo and theme
round trip), `src/ipc/fixtures.ts` (`appSettings`), `src/ipc/mock.ts`.

## What you do

1. **Wire the setting** in `src/App.tsx` only:
   - the initial `Prefs` take `skin: settings.skin` (the binding's
     `AppSkin` and `SkinChoice` have the same values; make `SkinChoice` an
     alias of the binding type in `src/app/prefs.tsx` if that reads better,
     keeping the exported name);
   - `onChange` persists `skin` the way it persists `theme` and `icon`
     (compare `current.skin !== (prefs.skin ?? "default")` and include
     `skin` in the `app_settings_set` params).
   Touch no markup and no CSS. The popover's `Segmented` already calls
   `prefs.set({ skin })`.
2. **Pin it** in `src/state/flow.test.tsx` (next to the logo/theme test):
   choosing `rack` sends `app_settings_set` with `skin: "rack"`, and an
   app started from settings with `skin: "matrix"` applies
   `data-skin="matrix"` on `<html>` (or the scope) before any user action.
   Reuse the file's existing helpers and mock wiring.
3. **Settle the four strings** in `src/i18n/ko.json` and `en.json`:
   `frame.settings.skin` (the row label), `.default`, `.rack`, `.matrix`.
   The register is DESIGN.md §10 (labels are noun phrases; no 해요체, no
   exclamation marks). The skin names are proper names in `docs/skins/`
   ("Hardware Rack", "Signal Matrix"); decide whether the Korean screen
   shows them in English, in Korean transliteration, or with a short
   Korean gloss, and keep the choice consistent with how the settings row
   already names the logo choices ("전문가 모드" / "킹반인 모드"). The
   default's label must say it is the default. Write the reason for your
   choice in the report. Do not add other keys.
4. **Doc note.** If `docs/DESIGN.md` §7 or §11 names the setting
   differently from what you settled, fix only that wording.

## Rules

- Files you may change: `src/App.tsx`, `src/app/prefs.tsx` (type alias
  only), `src/state/flow.test.tsx`, `src/i18n/ko.json`, `src/i18n/en.json`,
  and the one wording in `docs/DESIGN.md`. Nothing else: no CSS, no
  components, no `menus.tsx`, no bindings, no Rust.
- Do not reformat the catalogs; add or edit only the four keys in place.

## Verification (foreground; finish every command before you report)

```sh
cd /home/user/Sound-Equalization-and-Correction-System-2R/apps/secs2r/ui
export PATH=/opt/node24/bin:$PATH
npm run typecheck
npx vitest run src/i18n src/state/flow.test.tsx src/app
npm test
```

## Report

The files you changed, the four strings in both languages with the reason
for the naming choice, the new test's name and what it asserts, and the
tail of each verification command.
