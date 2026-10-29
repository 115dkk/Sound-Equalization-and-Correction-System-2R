# P106 (Opus 5.5): the local API in the settings popover

Worktree: `E:\Sound-Equalization-and-Correction-System-2R` (branch
`claude/public-api`; P104 added the backend). Work only under
`apps/secs2r/ui/`. A Daybreak worker edits Rust in the same checkout at the
same time (`crates/secs-api/`, manifests): do not touch Rust, manifests or
`Cargo.lock`, and do not run cargo. Do not commit, do not push. ASTRA wires
the calls and writes the final catalog text afterwards (P107).

Read first: `CLAUDE.md` (the screen rules), `docs/DESIGN.md`,
`docs/adr/0016-public-rust-api-and-local-api.md`,
`apps/secs2r/ui/src/app/menus.tsx` (`SettingsButton` and its popover),
`apps/secs2r/ui/src/app/prefs.tsx`, `apps/secs2r/ui/src/ui/` (the
components the app already has: `Popover`, dialogs, `Segmented`, switches,
fields, buttons, toasts; reuse them), an existing dialog for patterns (search
for `Dialog` under `src/screens/`), the gallery specimens of the frame
(`src/gallery/`), `apps/secs2r/ui/src/ipc/bindings/LocalApiStatus.ts`,
`LocalApiSettings.ts`, `AppSettings.ts`, and the `frame.settings.*` keys in
`src/i18n/ko.json` and `en.json`. Load the `make-interfaces-feel-better`
skill and follow it.

## What it is for

The local API lets another program on the same PC (a monitoring tray app,
a script) read what the app is doing and drive it: open a project, import
measurements, compute, export, cancel. It is off by default. When it is on,
the app listens on `http://127.0.0.1:<port>` (default 4712) and every request
needs the token stored in the app's settings folder. A program with the token
can do what the user can do from the screen except open windows, including
writing files and playing measurement sweeps through the speakers. The user
needs to: switch it on or off, see whether it is listening (or why not, for
example the port is taken), see and copy the address and the token, change
the port, and make a new token when the old one leaked.

## Rules (fixed)

1. **Popover row.** In the settings popover, after 로고, a row 로컬 API with
   a switch and one line of state: `꺼짐`, `127.0.0.1:4712에서 받는 중`, or the
   error (`포트 4712를 다른 프로그램이 쓰고 있습니다` style). A text button
   on the row opens the dialog. Turning it on does not need the dialog.
2. **Dialog** 로컬 API: one or two sentences on what it does and the risk
   above (in plain words, no alarm styling); then fields:
   - 상태 (the same state line), 주소 (URL, copy button);
   - 포트 (number field, 1024 to 65535, applied with its own button, disabled
     while unchanged or invalid; invalid shows why under the field);
   - 토큰 (masked by default, a show/hide toggle, copy button, and
     `새 토큰 만들기`, which first asks in one confirm sentence that programs
     using the old token must be given the new one); 토큰 파일 (path, copy).
   - A short reference of the three endpoints (`POST /v1/call`,
     `GET /v1/status`, `GET /v1/events`) and the header a client sends
     (`Authorization: Bearer <토큰>`), in a monospace block with a copy
     button, so a user can hand it to whoever writes the client.
   The dialog's one filled action is 닫기 or the port apply, whichever the
   house dialogs do; keep one filled action.
3. **View model.** A new module (for example `src/app/localApi.ts` and
   `src/app/LocalApiDialog.tsx`): `LocalApiView = { status: LocalApiStatus |
   null; pending: "toggle" | "port" | "token" | null; error: IpcError | null }`
   and `LocalApiActions = { setEnabled(on: boolean); setPort(port: number);
   regenerateToken(); copy(text: string) }`. Write a stub hook
   (`useLocalApi`) that returns a fixed off status and no-op actions so the
   app runs; ASTRA replaces its body in P107. Export a pure
   `portProblem(text: string): PortProblem | null` and unit-test it.
4. **Copy.** 합니다체 sentences, noun-phrase labels and states, labels that
   say the effect, UI text at least 14 px, no em-dash. Every new string is a
   key in both catalogs; write a first Korean and English version yourself
   and list every key in the report (ASTRA rewrites them).
5. **Specimens and tests.** Gallery specimens: off; on and listening; port
   busy; token shown; token-regenerate confirm; invalid port. Tests for the
   popover row states, the masked token, the confirm before a new token, and
   `portProblem`.

## Files

You may edit: `apps/secs2r/ui/src/app/**`, `apps/secs2r/ui/src/gallery/**`
(specimens), `apps/secs2r/ui/src/i18n/ko.json`, `en.json`.

Do not touch: Rust, manifests, bindings, other screens, `.github/`, `docs/`.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm ci
npm run typecheck
npm test
npm run build
```

## Report

Changed files; the view model and actions; every new i18n key with your
draft Korean and English; the tail of each verification command; anything
left undone.
