# P33 (ASTRA): wire the screens to the service

Worktree: `/home/user/wt/p33` (branch `m5/p33`). Do not commit; Claude
reviews the diff and commits.

The screens are drawn (P29–P32, P38) and the service answers everything
they need (P26–P28, P36, P37), but nothing connects them: `App.tsx` still
shows a placeholder. This packet makes the app run end to end on the real
service and on the mock, without changing how anything looks.

Read first: `CLAUDE.md`, `docs/M5.md`, `docs/packets/M5-screens-common.md`
(the view/actions boundary), `docs/DESIGN.md` §11–13, `docs/adr/0003-stack-and-ipc.md`,
every `apps/secs2r/ui/src/screens/*/types.ts` (the `View` and `Actions` you
implement; each action's JSDoc names its IPC method), `src/ipc/{client,events,context,mock,fixtures}.ts`,
`src/app/{AppFrame,model,prefs,shortcuts}.tsx`, `src/app/jobs/useJobs.ts`,
`apps/secs2r/src/main.rs` (the `secs_call` command takes an optional event
channel), and the generated bindings.

## What you build

1. **The Tauri bridge** (`src/ipc/tauri.ts`): one long-lived
   `Channel<JobEvent>` (from `@tauri-apps/api/core`) whose messages go to the
   event bus, passed as `events` with every `secs_call`; `createTauriIpc():
   Ipc`. Check how `Channel` is passed and reused in `@tauri-apps/api` 2.12
   before you rely on it.
2. **State** (`src/state/`, new; React `useSyncExternalStore`, no library):
   small stores for what the service owns (project info, configuration,
   layout and estimate, DSP, measurement methods, settings, records, curves
   and levels, calibration settings, mode spec, target, results, result
   curves and lines, export folder and runs, app settings). Each store loads
   from its IPC method and reloads on an explicit table of triggers:
   mutations it depends on, and `Finished` events of the job kinds that
   change it (measure, sample_load, mdat_import → records, curves, project;
   calibrate → results, result curves and lines; mimo → results; export →
   runs; …). Write the table as data in one file so it can be read and
   tested.
3. **One hook per screen** (`src/screens/<step>/use<Step>Screen.ts`):
   builds the screen's `View` from the stores plus the host state the types
   name (selected tab, channel, graph hidden, plot range, pending mdat scan,
   …) and returns its `Actions`, each calling the IPC method its JSDoc
   names. Destructive actions resolve to the undo token. Replace P31b's
   `settingsEdit.ts`: `changeSetting(edit)` calls `calibration_settings_edit`;
   delete the file and its test.
4. **The frame model** (`src/app/useFrameModel.ts`): the rail's step states
   from each `<step>StepState(view)`, the project summary from the stores,
   the current step, navigation.
5. **The app** (`src/App.tsx`): providers (IPC, prefs from `app_settings`,
   toasts, layers), `AppFrame` with the current step's screen, its header
   from `<step>Header(view, actions, t)`, the job tray (`useJobs`), the help
   panel in the frame's `aside` slot (F1 toggles), the project dialogs
   behind the project menu and before closing, the mic meter panel
   floating on Ctrl+M, and `TutorialLayer` with P32's sample quests
   (P35 replaces them).
6. **Lifecycles, as upstream** (`window.py:746-783`): start the status watch
   when the app starts; start the level monitor while 연결·점검 is shown with
   a ready path and stop it when leaving; refresh REW facts (`rew_info`)
   when that step opens; clear the measure screen's selection after a
   measurement.
7. **Closing and the tutorial.** Closing the window with unsaved changes
   asks first (`getCurrentWindow().onCloseRequested`, prevent, then close
   after the answer); add only the window permission this needs to
   `apps/secs2r/capabilities/default.json`. Starting the tutorial creates
   `project_new { sample: true, tutorial: true }` and switches the accent;
   `tutorial_blocked` errors show as the screens' notes.
8. **The mock answers everything.** `src/ipc/mock.ts` answers every
   method the hooks call, from typed fixtures, and emits job events, so the
   whole app runs in vitest and in the gallery without Tauri. Add one
   gallery specimen `app.live` that runs `App` on the mock.
9. **The flow test** (M5 §6): a vitest that renders `App` on the mock and
   walks the sample project: new sample project → 연결·점검 with the mock
   method → 측정·가져오기 with the demo data → 보정 계산 → 내보내기
   CamillaDSP, asserting the IPC calls in order and what each screen shows
   at the end. Add tests for the trigger table and for each hook's actions
   (the IPC method and params each one sends).

## Rules

- No markup or style changes: do not edit the screens' `.tsx` components,
  their CSS, the kit or the frame components. If a screen cannot be wired
  without one, stop at that point and list it in the report (Claude routes
  it to Opus). You may add fields to a screen's `types.ts` only where a
  P36/P37 type changed (and update its fixtures).
- No new npm dependencies. Every string the wiring itself shows (rare) is a
  catalog key in both `ko.json` and `en.json`.
- Errors follow ADR 0003: show the code's cause and next step (the job
  tray's `errorText`), never the raw detail outside 자세히.

## Files you may change

`apps/secs2r/ui/src/ipc/**` (not `bindings/`), `src/state/**` (new),
`src/screens/*/use*Screen.ts` (new) and their tests, `src/screens/*/types.ts`
and `fixtures.ts` (only as above), `src/screens/calibrate/settingsEdit*.ts`
(delete), `src/app/useFrameModel.ts` (new), `src/App.tsx`, `src/main.tsx`,
`src/gallery/` (the `app.live` specimen), `src/i18n/{ko,en}.json`,
`apps/secs2r/capabilities/default.json`, `features.toml` (the `ui.screens`
note).

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p33/apps/secs2r/ui
export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH
npm ci --no-audit --no-fund
npm run typecheck
npm test
npm run build
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/gallery-shots.mjs --out /tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/shots/p33
cd /home/user/wt/p33 && CARGO_TARGET_DIR=/home/user/target-p33 CARGO_PROFILE_DEV_DEBUG=0 cargo build --locked -p secs2r
```

Delete the shots folder after you have looked at `app.live`; disk is tight.

## Report

The stores and their trigger table, each hook's actions with their IPC
calls, what the app does on start, on step change and on close, the flow
test, anything you could not wire without a markup change, and the checks
with their results.
