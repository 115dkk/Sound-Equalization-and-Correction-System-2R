# P165 (ASTRA): audit #155, group W5: screen state, writers and the mock service

Follow the **common rules** in `docs/packets/P161-daybreak-audit155-deploy.md`.

Worktree `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\audit-155-w5`,
branch `claude/audit-155-w5`. Files: `apps/secs2r/ui/src/state/**`,
`apps/secs2r/ui/src/screens/**` hooks and their tests (`use*.ts`, `*.test.ts(x)`,
`types.ts`), `apps/secs2r/ui/src/ipc/mock.ts` and its tests,
`apps/secs2r/ui/src/gallery/tour/scenes.ts` (C-11 text only). Do not change
markup or styles beyond a `disabled` flag an item needs; do not change
`data-ui` ids or catalog keys. Read ADR 0010, 0014, 0028, 0029, then
`state/session.ts`, `state/store.ts`, `state/effects.ts`, `state/writers.ts`
and `screens/connect/useConnectScreen.ts` (its mic-file choice already guards
a follow-up against a project switch: the pattern to reuse).

Gates (Node 24, npm 11), from `apps/secs2r/ui`: `npm ci` once, then
`npm run typecheck`, `npm test`, `npm run build`. Not cargo.

## Design (fixed): Session owns project-scoped follow-up work (A5)

Session already drops a stale response's central effects after a project
switch, but callers decide on their own whether a dialog's follow-up requests
still belong to the project that started them, and what to wait for before
saving. Add to `Session` (no new IPC):
- a project guard taken when a user action starts (`session.projectGuard()`
  or similar) whose `alive()` turns false once the project changes; hooks run
  follow-up reads and writes only while it is alive;
- a wait for one project's queued edits, `session.projectEditsSettled()`,
  resolving with whether they all succeeded; it does not wait for unrelated
  writers or for the caller's own request.
Hooks use these instead of ad hoc checks.

## Items (in this order)

| Item | Finding | Cat | Seam and what is fixed |
|---|---|---|---|
| A-16 | TD-120 | A | `useProjectScreen` with a delayed mock response for the first of two queued project edits: "save then close" and "save then open" send `project_save` only after both edits finished, and stop when one failed. Introduces `projectEditsSettled`. |
| A-22 | TD-122 | A | `useMeasureScreen`: an mdat chosen in A whose scan answers after an external switch to B does not set B's `host.mdat`; same guard for the target-file and export-folder dialogs (`useCalibrateScreen`, `useExportScreen`). Introduces the project guard. |
| A-15 | TD-119 | A | Session/store with the mock: opening B succeeds but B's `dsp_get` (and the calibration settings fetch) fails; nothing of A's is shown for B and an edit does not send A's values to B. Clear project-owned resources when the project identity changes; same-project refetch failures keep today's policy. |
| A-14 | TD-118 | A | `useExportScreen` with a writer that fails: changing the CamillaDSP address or executable and pressing apply sends no `apply_camilla` (and no Equalizer APO apply when its level save failed). |
| A-20 | TD-68 | A | Session writers: the app writer and the apply writer go through one serialisation unit, so an apply-address save and a whole-app save that read the same old value cannot revert each other. Test with interleaved mock responses. |
| A-21 | TD-121 | A | `useCalibrateScreen` / extra RC bands: two edits to different edges of one band while the first response is delayed both end up saved; add and remove apply to the latest array. |
| A-23 | TD-124 | A | Calibrate screen: a partial room-dimensions draft typed in A is cleared when the project identity changes, even when both projects have no dimensions. |
| A-39 | Q5 | A | `useExportScreen`: the mdat export's re-calibration uses the shared full calibration (`state/calibration.ts`), so it follows XO include and the MIMO choice like the main calibrate button. Test the `calibrate` request arguments. |
| A-34 | TD-123 | A | `ipc/mock.ts`: job exclusivity mirrors the service's job groups (`crates/secs-service/src/jobs.rs` near lines 27 to 34: compute, device, export). Test: starting two different kinds of one group is refused with `job_busy`. |
| A-35 | TD-125 | A | `ipc/mock.ts`: standalone `mimo` checks the service's preconditions (`crates/secs-service/src/calibration.rs` near lines 214 to 228 and 284 to 298). Fix the success fixture in `mockWiring.test.ts` and add one refusal per condition. |
| C-7 | TD-85 | C | Remove or correct the resolved service-gap comments in `screens/measure/types.ts` (near lines 24 to 38) and `screens/connect/types.ts` (near lines 66, 74, 89). |
| C-11 | TD-146 | C | `gallery/tour/scenes.ts` near lines 698 to 703: the `export.mimo-blocked` scene text in both languages matches what is actually blocked (F12: Equalizer APO export is allowed with MIMO). Keep the scene id. |
