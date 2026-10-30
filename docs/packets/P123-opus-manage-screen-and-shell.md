# P123 (Opus 5.5): 관리 on the export screen, and the host shell's window (ADR 0018)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\apply-manage`
(branch `claude/apply-manage`; P120 built the service and its IPC, and is
being hardened in the same worktree right now: do not touch Rust, manifests,
`apps/secs2r-host/**` or `crates/**`). Work only under `apps/secs2r/ui/`. Do
not commit, do not push. ASTRA writes the final catalog text afterwards
(P124). A Daybreak packet (P126) builds the shell's Tauri app around the page
you draw here.

Read first: `CLAUDE.md` (screen rules), `docs/DESIGN.md`,
`docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md` (all, including
"관리 단계의 세부"), `docs/packets/P121-opus-apply-screen.md` (what the 적용
screen already does), `apps/secs2r/ui/src/screens/export/apply.tsx`,
`types.ts`, `useExportScreen.ts`, `state/session.ts`, `state/effects.ts`,
the bindings `ApplyStatus.ts`, `ServiceState.ts`, `HostStatus.ts`,
`ManagedProcess.ts`, `HostManaged.ts`, `CamillaApply.ts` (`exe_path`),
`Request.ts` (`host_install`, `host_uninstall`, `host_stop_managing`),
`apps/secs2r/ui/vite.config.*`, `index.html`, `src/main.tsx`,
`src/gallery/**`, and `src/ui/`. Load the `make-interfaces-feel-better`
skill and follow it.

## What it is for

적용 pushes the design once, while the app is open. 관리 hands it to a
Windows service (`SECS2RHost`) that keeps it applied when the app is closed:
it launches CamillaDSP as the logged-on user and relaunches it when it stops,
and puts SECS 2R's line back into Equalizer APO's `config.txt` when another
editor removes it. The service must be registered once with administrator
rights (a UAC prompt); after that the app and the shell talk to it without
elevation. A small tray app (the host shell) shows what the service is doing
even when SECS 2R itself is closed.

## Rules (fixed)

1. **Export screen.** On Windows, the 적용 방식 choice of both rows gains
   관리. Choosing it:
   - CamillaDSP needs an executable: a path field with a file picker
     (`camilladsp.exe`), stored in `ApplySettings.camilla.exe_path` through
     the existing writer. Without it the row's action is disabled with the
     reason.
   - A service block in each row: `apply_status.host` not installed → one
     line why it is needed and a `서비스 설치` button (`host_install`; say
     that Windows asks for administrator approval once); stopped or
     unreachable → the state and what to do; running → the managed state
     from `host_status`: CamillaDSP running (pid), restarting (restarts in
     the window, next try), given up (reason), waiting for a user session,
     or another user at the console; Equalizer APO line state and the last
     repair time.
   - The row's primary action becomes `CamillaDSP 관리 시작` /
     `Equalizer APO 관리 시작` (it applies, then hands over); a quiet
     `관리 끄기` (`host_stop_managing`) asks in one confirm sentence first.
   - Off Windows, 관리 is disabled with the reason.
2. **Host shell page.** A second page in this UI project, `host.html`, for
   the shell's single window (about 420×560, not resizable below that):
   service state, what is managed (design name or ID, CamillaDSP path,
   Equalizer APO folder), the CamillaDSP process state, the Equalizer APO
   line state, and the latest log lines (newest first, at most 50, each with
   time and a plain sentence; raw JSON only in a collapsed detail). Actions:
   `서비스 설치`, `서비스 제거` (confirm), `관리 끄기` (confirm), `SECS 2R 열기`.
   The page talks only to an injected interface you define in
   `src/host/api.ts`:

   ```ts
   export type HostShellApi = {
     status(): Promise<{ service: ServiceState; host: HostStatus | null }>;
     log(lines: number): Promise<HostLogLine[]>;
     install(): Promise<void>;
     uninstall(): Promise<void>;
     stopManaging(): Promise<void>;
     openApp(): Promise<void>;
   };
   ```

   with a mock implementation for tests and the gallery, and a Tauri
   implementation that calls `invoke("host_status")`, `invoke("host_log",
   { lines })`, `invoke("host_install")`, `invoke("host_uninstall")`,
   `invoke("host_stop_managing")`, `invoke("open_app")` (P126 implements
   these commands). Poll status every 2 s while the window is visible.
   Add `host.html` as a second Vite entry so `npm run build` emits it.
3. **Copy.** 합니다체 sentences, noun-phrase labels and states, labels that
   say the effect, UI text at least 14 px, no em-dash; every new string a
   key in both catalogs with a first ko/en version (list them).
4. **Wiring.** Extend the export view types and hook for the host state,
   the exe path and the three host actions; reuse the job tray for the
   apply part. Mocks: `ipc/mock.ts` answers `host_install` (service becomes
   running), `host_uninstall`, `host_stop_managing`, and `apply_status` with
   a host state the specimens can set.
5. **Specimens and tests.** Export: 관리 without exe path; service not
   installed; installing; running with CamillaDSP running; restarting;
   given up; another user at the console; Equalizer APO repaired 2 minutes
   ago; 관리 끄기 confirm; non-Windows. Shell page: not installed; running
   and managing both; given up with log lines; uninstall confirm. Tests for
   the level switch to 관리 writing `ApplySettings`, the exe path rule, the
   service block states, the confirm dialogs, and the shell page with the
   mock API.

## Files

You may edit: `apps/secs2r/ui/src/screens/export/**`, `src/host/**` (new),
`host.html` (new) and the Vite config (second entry), `state/**`,
`ipc/mock.ts`, `ipc/mockWiring.test.ts`, `ipc/fixtures.ts`,
`src/i18n/ko.json`, `en.json`, `src/gallery/**` (registration).

Do not touch: Rust, manifests, bindings, `apps/secs2r-host/**`, other
screens, `.github/`, `docs/`.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm ci
npm run typecheck
npm test
npm run build
```

Node 24 (npm 11); skip `npm ci` if `node_modules` is current. Confirm
`dist/host.html` exists after the build.

## Report

Files changed, the `HostShellApi` as built, keys with first ko/en versions,
specimens, and the commands' last lines.
