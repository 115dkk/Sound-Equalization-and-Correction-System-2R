# P124 (ASTRA): 관리 copy and the last wiring (ADR 0018)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\apply-manage`
(branch `claude/apply-manage`). P123 (Opus) drew the export screen's 관리
and the shell page `host.html` with first ko/en versions of 93 new keys.
P120 (Daybreak) is finishing the service in the same worktree right now: do
not touch Rust, manifests, bindings, `apps/secs2r-host/**`, `crates/**`,
`.github/**` or docs. Do not commit, do not push.

Read first: `CLAUDE.md` (screen rules: 합니다체 sentences, noun-phrase labels
and states, labels that say the effect, no em-dash), the maintainer's
Korean style rules in `C:\Users\32170336\.claude\CLAUDE.md` (번역투 제거),
`docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`,
`docs/packets/P123-opus-manage-screen-and-shell.md`,
`docs/packets/P126-daybreak-host-shell-app.md` (the shell's commands,
including `tray_labels`), `apps/secs2r/ui/src/host/**`,
`apps/secs2r/ui/src/screens/export/{types.ts,apply.tsx,useExportScreen.ts}`,
`apps/secs2r/ui/src/state/effects.ts`, the bindings
`ManagedProcessState.ts`, `ApplyOutcome.ts`, `OpenFileKind.ts`,
`HostLogLine.ts`, and `apps/secs2r-host/src/rules.rs` (read only: the log
line format and event names).

## Work

1. **Catalog.** Review every key P123 added or changed under `export.apply.*`,
   `export.manage.*`, `export.status.host.*`, `export.status.managing.*`,
   `export.status.exe_path`, `export.status.windows_only` and `host.*` in
   `src/i18n/ko.json` and `en.json`. Keep the meaning; fix register,
   번역투, labels that do not say their effect, and any ko/en mismatch.
   Keep placeholder sets identical between the two catalogs. Say what you
   changed and why, key by key.
2. **Log events the service now writes.** The service logs `launch`, `exit`,
   `restart`, `repair`, `takeover`, `port_busy`, `given_up`, `other_user`,
   `session_logon`, `session_logoff`, `state_corrupt`, `service_error`
   (check `rules.rs` for the exact names and fields). `src/host/model.ts`
   maps only some; add `takeover` (the service ended another CamillaDSP on
   the port to run its own), `port_busy` (the port stayed busy), `given_up`
   (with its reason kept for the raw detail only, as P123 does) and
   `other_user`, with keys in both catalogs and tests.
3. **Types that now exist.** `ManagedProcessState` has `other_user`: drop
   the widened `ProcessState`/`HostState` in `src/host/model.ts` in favour of
   the bindings, keeping behaviour. `OpenFileKind` has `"camilla_exe"`:
   replace the double cast in `screens/export/types.ts` with the literal.
   `ApplyOutcome` has `handed_over`: check `state/effects.ts` handles it as
   P123 wrote and that the outcome text key exists in both catalogs.
4. **Tray labels.** After the shell page's catalog is ready, call
   `invoke("tray_labels", { open, open_app, quit })` once with the current
   locale's texts (new keys `host.tray.open` "창 열기" / "Open window",
   `host.tray.open_app` "SECS 2R 열기" / "Open SECS 2R", `host.tray.quit`
   "끝내기" / "Quit"; you may improve the wording). Put the call in the
   `HostShellApi` Tauri implementation as a seventh method
   `trayLabels(labels)` with a mock that records it, and call it from the
   shell's startup code, not from markup. Test it with the mock.
5. **Font subset.** If the repository has a font-subset gate for the
   catalogs, run it and fix what it reports.

You change calls, state and text only; markup and styles stay as P123 drew
them. If a sentence cannot fit without a layout change, report it.

## Files

You may edit: `apps/secs2r/ui/src/i18n/ko.json`, `en.json`,
`apps/secs2r/ui/src/host/{api.ts,mockApi.ts,model.ts,useHostShell.ts,main.tsx,HostShell.test.tsx,fixtures.ts}`,
`apps/secs2r/ui/src/screens/export/{types.ts,types.test.ts}`,
`apps/secs2r/ui/src/state/effects.ts`, tests next to the files you change,
and the font-subset outputs if the gate regenerates them.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
```

(Node 24; `node_modules` is current.) Run the font-subset gate if there is
one and say which command.

## Report

Key changes (old → new, ko and en), the new keys, the log mapping as
built, the `trayLabels` addition, files changed, and the commands' last
lines.
