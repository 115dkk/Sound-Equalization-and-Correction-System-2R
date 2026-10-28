# P107 (ASTRA): wire the local API settings, follow external changes, write the text

Worktree: `E:\Sound-Equalization-and-Correction-System-2R` (branch
`claude/public-api`; P104, P105 and P106 are in it). Work only under
`apps/secs2r/ui/`. Do not commit, do not push. Claude reviews the diff and
commits.

Read first: `CLAUDE.md`, `docs/adr/0016-public-rust-api-and-local-api.md`,
`docs/packets/P106-opus-local-api-settings.md` and what it produced
(`src/app/localApi.ts`, `src/app/LocalApiDialog.tsx`, `src/app/menus.tsx`),
`src/state/` (session, resources, effects, how `app_settings_get` and
`app_settings_set` are called today and how a resource is refreshed),
`src/ipc/` (how `secs_call` is invoked and how Tauri events are listened to,
if anywhere), the bindings `LocalApiStatus.ts`, `LocalApiSettings.ts`,
`AppSettings.ts`, `AppSettingsParams.ts`, `ErrorCode.ts`, and
`apps/secs2r/src/local_api.rs` (what the shell emits and when).

## Rules (fixed)

1. **Settings wiring** (replace the stub body of `useLocalApi`):
   - `status` comes from `local_api_status`, read when the popover or the
     dialog opens and after each change below.
   - `setEnabled(on)` and `setPort(port)` write the whole app settings with
     `app_settings_set`, starting from the current `app_settings_get` result
     and changing only `local_api`, through the same save path and failure
     policy the other app settings use (the change shows once the service
     accepts it, the old value stays when the save fails). Then read the
     status again: the shell starts, stops or restarts the server after the
     save.
   - `regenerateToken()` calls `local_api_token_regenerate` and shows the
     returned status.
   - `copy(text)` writes to the clipboard the way the app already does it
     (find an existing copy action; if none, `navigator.clipboard.writeText`)
     and shows the house toast.
   - `pending` and `error` follow each call.
2. **External changes.** The shell emits the Tauri event
   `secs://external-change` with the wire name of a method a local-API client
   just changed state with. Listen for it once for the app's lifetime and
   refresh what that method can have changed: at least the project, results,
   settings, export folder and jobs resources for `project_*`, `calibrate`,
   `mimo`, `export`, `mdat_import`, `measure_*`, `config_*`, `dsp_set`,
   `layout_set`, `target_*`, `calibration_settings_*`, `app_settings_set`,
   `export_folder_set`; when in doubt refresh everything, and coalesce a
   burst into one refresh. Unit-test the method-to-refresh mapping and the
   coalescing.
3. **Text.** Rewrite every key P106 added in Korean and English (P106's
   report lists them; diff the catalogs against `HEAD~` to be sure):
   합니다체 sentences, noun-phrase labels and states, labels that say the
   effect, no dressed-up phrasing, no em-dash. The explanation says in plain
   words what a program with the token can do (read state, open projects,
   compute, write files, play measurement sweeps). Add a message for the new
   error code `ui_only` wherever the catalogs keep one message per error
   code. Keep both catalogs' key sets and placeholders identical and run the
   i18n tests and any font-subset gate.

4. **The typecheck fails today** on five places that predate P106 and come
   from P104's new types: `src/app/jobs/jobText.ts` (no text for the
   `ui_only` error code), `src/ipc/fixtures.ts` (an `AppSettings` without
   `local_api`), `src/ipc/mock.ts` (`local_api_status` and
   `local_api_token_regenerate` fall through as `never`: make the mock answer
   them with an off status on port 4712 and a stable fake token, and turn the
   server "on" in its status when the mock's app settings enable it),
   `src/ipc/mockWiring.test.ts`, `src/state/effects.test.ts`. Fix them.
5. P106 notes that matter here: `useLocalApi(open)` is called by
   `SettingsButton` in the gallery and in tests without a `SessionProvider`,
   so the hook must work without a session (fixed off status, no calls);
   read the status when `open` turns true. A busy port is recognised today
   from the text of `LocalApiStatus.error` (`localApiState` in
   `localApi.ts`); keep that working.

## Files

You may edit: `apps/secs2r/ui/src/app/localApi.ts` (the hook body and its
tests), `apps/secs2r/ui/src/app/jobs/jobText.ts`, `apps/secs2r/ui/src/state/**`, `apps/secs2r/ui/src/ipc/**` (not the
generated `bindings/`), `apps/secs2r/ui/src/i18n/ko.json`, `en.json`, tests
that a text change breaks.

Do not touch: markup and styles (`LocalApiDialog.tsx`, `menus.tsx`, CSS)
except to fix a key name, Rust, bindings, `.github/`, `docs/`.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm ci
npm run typecheck
npm test
npm run build
```

## Report

Changed files; for every key, the final Korean and English; the refresh
mapping; the tail of each verification command; anything left undone.
