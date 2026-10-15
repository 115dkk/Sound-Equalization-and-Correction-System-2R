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

1. **The Tauri bridge** (`src/ipc/tauri.ts`): a **new**
   `Channel<JobEvent>` (from `@tauri-apps/api/core`) for every `secs_call`,
   passed as `events`, whose messages go to the one event bus;
   `createTauriIpc(): Ipc`. One long-lived channel does not work: the Rust
   side builds a channel per request and ends it when the request's last
   clone drops (`src/ipc/channelReuse.test.ts` shows it). A job keeps its
   channel until it finishes, so its events arrive on it.
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
   names. Destructive actions resolve to the undo token.
   `changeSetting(edit)` calls `calibration_settings_edit` (the edit rules
   left the screen; `settingsView.ts` keeps only the read-side helpers), and
   the view's `subWarnings` comes from `sub_warnings`.
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
   `apps/secs2r/capabilities/default.json`. The app stops the service's
   jobs when the window is destroyed (`apps/secs2r/src/main.rs`), not on the
   close request, so a cancelled close keeps them running. Starting the tutorial creates
   `project_new { sample: true, tutorial: true }` and switches the accent;
   `tutorial_blocked` errors show as the screens' notes.
8. **The mock answers everything.** `src/ipc/mock.ts` answers every
   method the hooks call, from typed fixtures, and emits job events, so the
   whole app runs in vitest and in the gallery without Tauri. Add one
   gallery specimen `app.live` that runs `App` on the mock.
9. **The flow test** (M5 §6: "샘플 프로젝트로 화면에서 측정(모의)부터
   내보내기까지"): a vitest that renders `App` on the mock and walks: new
   project from the sample room (not a tutorial) → 연결·점검: choose the
   mock method (모의 측정) → 측정·가져오기: press 이 위치 측정 for the
   positions the mock room has (`measure_position` jobs on the mock) → 보정:
   보정 계산 → 내보내기: CamillaDSP 설정 내보내기 (a real `export` call; the
   project is not a tutorial). Assert the IPC calls in order and what each
   screen shows at the end. The demo-data button belongs to the tutorial
   and is not part of this flow. Add tests for the trigger table and for each hook's actions
   (the IPC method and params each one sends).

## Already done before this packet ran

A first run stopped on three blockers; Claude fixed them on this branch: a
channel per call (item 1), the edit rules removed from the screen
(`settingsEdit.ts` → `settingsView.ts`, `subWarnings` in the view), and the
job shutdown moved to window destruction. Keep
`src/ipc/channelReuse.test.ts`.

## How to handle gaps

Do not stop the packet for a mismatch between this packet, a screen's
contract and the service. Resolve what you can inside the stores and hooks
(the screen contract wins over this packet's wording), keep going with
everything else, and list each mismatch and what you did about it in the
report. Stop only for something that makes the whole app impossible to
wire, and say exactly what it is.

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
