# P134 (Opus 5.5): the update shows up quietly and says once what changed (ADR 0020)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\auto-update`
(branch `claude/auto-update`). Do not commit, do not push. Work only under
`apps/secs2r/ui/`.

Read first: `CLAUDE.md` (screen rules), `docs/DESIGN.md`, load the
`make-interfaces-feel-better` skill, `docs/adr/0020-automatic-updates.md`
("흐름"), then the wiring P132 left: `apps/secs2r/ui/src/state/update.ts`
(`useUpdate()`: status, `check`, `download`, `restart`, `dismiss`,
`setPolicy`), the mock setters in `src/ipc/mock.ts`, and the bindings
`UpdateStatus.ts`, `UpdatePhase.ts`, `UpdateInstall.ts`, `UpdateNotice.ts`,
`UpdatePolicy.ts`. Neighbours to follow: `src/app/AppFrame.tsx` and
`app.css` (the frame, its header and status bar), `src/app/menus.tsx` and
`prefs.tsx` (the settings menu), `src/app/LocalApiDialog.tsx` (a settings
row that opens a dialog), `src/ui/Dialog.tsx`, `src/ui/StatusPill.tsx`,
`src/ui/Toast.tsx`. Use the tokens the frame already uses; add none
unless DESIGN.md lacks one, and then say which.

## What the screens are for

Updating is a background chore. Nothing may block work, steal focus or
open by itself, except the one notice after an update. A user who never
looks at it still gets the new version the next time they close the app.

## States and ids (fixed)

1. **Frame indicator** `data-ui="update.badge"`, `data-state` = the phase
   `kind`, `data-version` when the phase has one. It is drawn only for
   `downloading` (small progress, no percentage animation beyond a plain
   bar), `ready` and `portable`, and `failed` only after a check the user
   started. `ready` with `on_exit`: the label says the version is applied
   on the next start, with a button `data-ui="update.restart"` that
   restarts now. `ready` with `on_request` (deb): the label says the
   version is ready, with `data-ui="update.restart"` labelled as installing
   now (it asks for the password). `portable`: the label says a new
   version is out, with `data-ui="update.portable"` opening the release
   page through the app's existing external-link path. Place it in the
   frame where it reads as status, not as a call to action, and keep the
   frame's layout stable when it appears.
2. **Notice** `data-ui="update.notice"` (a dialog or panel, your call),
   shown once when `status.notice` is set: the title says the app was
   updated to `to`, the body renders `notes` (the CHANGELOG section, which
   is Markdown with `###` headings and `-` bullets; render that subset
   without a new dependency; follow any existing Markdown helper first),
   and when `skipped` is true one sentence with a link
   `data-ui="update.notice.release"` to `release_url` for the releases in
   between. One button `data-ui="update.notice.dismiss"` closes it and
   calls `dismiss`. It must not appear above a running measurement's
   confirm dialogs; if one is open, it waits until that closes.
3. **Settings** in the settings menu, next to the local API row:
   `data-ui="settings.updates"` with the three policies as
   `data-ui="settings.updates.auto|notify|off"` (the effect in each label:
   download and apply on close, tell me only, never check), the current
   version and the last check time, `data-ui="update.check"` (check now;
   shows `checking` and then up to date, available, ready or failed with
   the reason in a collapsed detail), and `data-ui="update.download"` only
   in `available` (download and install). Portable copies show that the
   portable copy is replaced by hand, with the same release link.
4. **Strings.** New keys in `ko.json` and `en.json` with first versions
   (합니다체, noun-phrase states, effect labels, no em-dash); list them.
   ASTRA reviews the catalogs afterwards.
5. **Specimens and tests.** Gallery specimens for every badge state, the
   notice with and without `skipped`, and the settings section in each
   policy and phase, through the mock setters. Tests: the badge appears
   only in the listed phases, restart calls `restart`, the notice shows
   once and dismiss calls `dismiss`, the notice waits for an open confirm
   dialog, settings store the policy through `setPolicy`, the ids above
   exist.

## Files

You may edit: `apps/secs2r/ui/src/app/**`, `apps/secs2r/ui/src/ui/**` only
for a shared fix the new parts need (say which), a new
`apps/secs2r/ui/src/app/update/**`, `apps/secs2r/ui/src/i18n/ko.json`,
`en.json`, `apps/secs2r/ui/src/gallery/**` and `src/screens/specimens.ts`
for registration. Do not edit `src/state/update.ts` or `src/ipc/**`; if the
hook lacks something, say what in the report.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
```

## Report

Files changed, where each part sits and why, the keys with ko and en, the
specimens, the tests, and the commands' last lines.
