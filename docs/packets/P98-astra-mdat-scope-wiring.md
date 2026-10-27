# P98 (ASTRA): wire the REW mdat's contents choice, and its text

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\dirac-liveproject-support-2e310d`
(branch `claude/mdat-export-scope`). Work only there. Do not commit, do not
push, do not spawn sub-agents. Claude reviews the diff and commits. In the
same tree at the same time Daybreak edits Rust crates (do not touch `crates/`
or `apps/secs2r-cli/`), and Claude draws the export screen (do not touch
`ExportScreen.tsx`, `export.css`, `screens/export/fixtures.ts`,
`screens/export/specimens.tsx`, `screens/export/ExportScreen.test.tsx`,
`screens/export/types.ts`, `screens/export/types.test.ts`).

Read first: `CLAUDE.md`, `apps/secs2r/ui/src/screens/export/types.ts`
(Claude's contract: `MdatScope`, `MDAT_SCOPES`, `ExportView.mdatScope`,
`ExportActions.setMdatScope`, `mdatScopeFor`, `mdatFileName`, `canExport`),
`apps/secs2r/ui/src/ipc/bindings/ExportParams.ts` and `MdatScope.ts`
(generated; `mdat_scope?: MdatScope`), `docs/packets/P97-daybreak-mdat-export-scope.md`
(what the service does: the rules table, folders, errors).

## What changes for the user

The export screen's REW mdat row gets "담을 내용": 모두 (default), 원 측정만,
보정 결과만. 원 측정만 needs no correction result, so before any correction
the row writes the original measurements of every position. File names say
what the file holds (`SECS 원 측정.mdat`, `SECS 보정 결과 <모드>.mdat`,
`SECS 원 측정+보정 결과 <모드>.mdat`).

## Wiring

1. `state/session.ts`: host state gains `mdatScope: MdatScope`, initial
   `"all"`. `state/effects.ts`: the project-open reset (the `host.set({...
   runs: {} ...})` on open) also sets `mdatScope: "all"`. Keep the choice when
   a calibration finishes (`export.invalidate` clears `runs` only).
2. `screens/export/useExportScreen.ts`: `view.mdatScope = host.mdatScope`;
   `actions.setMdatScope(scope)` sets it in the host; `exportFiles(kind)` sends
   `{ kind, folder: "" }` as now, and for `before_after_mdat` adds
   `mdat_scope: mdatScopeFor(<current results>.calibration, <current host>.mdatScope)`
   read from the stores at call time (not a stale closure). Other kinds never
   send `mdat_scope`.
3. `ipc/mock.ts` `export`: follow the service.
   - `mdat_scope` with a kind other than `before_after_mdat`:
     `invalid_request`, params `{ reason: "mdat_scope" }`.
   - `before_after_mdat` + `measurements`: no calibration needed and no
     `stale_cache` block; with no measured position (`project.positions` empty)
     `engine_input`, params `{ reason: "no_measurements" }`; folder (when
     `params.folder` is empty) `<base>/SECS_mdat_measurements_<stamp>`; files
     `[<folder>/SECS 원 측정.mdat]`; `stale: false`. Tutorial still refused first.
   - `before_after_mdat` + `all`/`results`/absent: today's checks; file
     `<folder>/` + `mdatFileName(scope, <the result's mode and ultra_ms>)`
     (import it from `screens/export/types`, as the mock already imports
     `filterModeName`). Absent means `all`.
   - Leave `staged_mdat` as it is.
4. Tests: `screens/export/useExportScreen.test.tsx` (the scope reaches the
   request; no result → `measurements` is sent whatever was chosen; other
   kinds send no `mdat_scope`; `setMdatScope` updates the view), the mock's
   tests (`ipc/mockWiring.test.ts` or the file that covers `export` now:
   measurements without a result, names per scope, refusal for other kinds),
   and `state/*.test.ts` where the open reset is checked.

## Text (ko and en together; placeholders identical in both)

Korean is fixed below unless it breaks a rule in `CLAUDE.md` (합니다체 for
sentences, noun phrases for labels, no em dash). Write the English to match
the existing English catalogue's register. Keys:

| key | ko |
| --- | --- |
| `export.kind.before_after_mdat` (change) | `REW mdat` |
| `export.kind.before_after_mdat.body` (change; HelpTip) | `REW에서 열 mdat 파일을 저장합니다. 원 측정은 모든 위치의 측정을 받은 그대로 담고, 측정에 붙은 마이크 보정 곡선은 REW가 빼고 그립니다. 보정 결과는 주 청취 위치의 보정 후 계산 응답이고, MIMO를 포함하면 적용 응답이 더해집니다. 파일 이름에 원 측정과 보정 결과 중 담은 것이 들어갑니다.` |
| `export.kind.before_after_mdat.where.measurements` (new) | `SECS_mdat_measurements_<저장 시각> 폴더` |
| `export.mdat.scope` (new; the choice's label) | `담을 내용` |
| `export.mdat.scope.all` (new) | `모두` |
| `export.mdat.scope.measurements` (new) | `원 측정만` |
| `export.mdat.scope.results` (new) | `보정 결과만` |
| `export.mdat.scope.no_result` (new; hint under the choice) | `보정 결과가 없어 원 측정만 저장할 수 있습니다.` |
| `export.mdat.scope.no_measurements` (new; hint) | `저장할 측정이 없습니다.` |

Also bring these existing help texts in line (ko and en), keeping their
style and length:

- `help.export.steps.3.see`: the default (모두) now saves
  `SECS 원 측정+보정 결과 <모드>.mdat`; per channel `_1 보정 전`, `_2 보정 후`,
  `_3 + MIMO` as before, subs at the end as measured, then the other
  positions' measurements named `<채널>_P<위치>`; 원 측정만 saves
  `SECS 원 측정.mdat` (every position), 보정 결과만 saves
  `SECS 보정 결과 <모드>.mdat`.
- `help.export.steps.4.see`: the 원 측정 mdat goes to
  `SECS_mdat_measurements_<시각>`.
- `help.export.checks.1` and `help.export.trouble.1.fix`: without a result
  or while computing nothing exports, except the REW mdat's 원 측정만, which
  needs measurements only.

Run the catalogue checks in `i18n/i18n.test.ts` (keys in both catalogues,
identical placeholders, any font-subset gate the repo has).

## Verify (foreground; do not end your turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
```

`npm run typecheck` may fail only in Claude's files while Claude is still
drawing (`ExportScreen.tsx` not yet using `setMdatScope`, or fixtures missing
`mdatScope`); report such errors, do not fix them.

## Report

Changed files, the tail of each command, the English strings you wrote, and
anything you could not do.
