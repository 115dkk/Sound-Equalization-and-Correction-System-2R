# P122 (ASTRA): text and the last wiring for 적용 (ADR 0018)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\apply-levels`
(branch `claude/apply-levels`; P119 built the backend, P121 the screen).
Do not commit, do not push. Work only in this worktree. Do not change the
markup, styles or layout P121 drew.

Read first: `CLAUDE.md`, `docs/UI-TERMS.md`, `docs/packets/P121-opus-apply-screen.md`,
`apps/secs2r/ui/src/screens/export/apply.tsx`, `types.ts` (`eqapoDesign`,
`currentDesignId`), `apps/secs2r/ui/src/ipc/bindings/ApplyStatus.ts`,
`apps/secs2r/ui/src/ipc/fixtures.ts`, `screens/export/fixtures.ts`,
`ipc/mock.ts`, and the `export.apply.*`, `export.status.*`,
`export.kind.camilla_dsp.run`, `jobs.kind.apply`, `error.unsupported*`,
`error.permission_denied*`, `error.apply_failed*` keys in
`apps/secs2r/ui/src/i18n/ko.json` and `en.json`.

## Tasks

1. **Catalog.** Rewrite every key listed above in both languages. Korean
   sentences are 합니다체; an instruction to the user ends in -십시오 (the
   first versions use -세요, which this screen does not use); labels and
   states are noun phrases; no em-dash; plain and short. Keep one term per
   thing across keys (적용, 적용 방식, 설정 폴더, config.txt 줄). English
   matches in meaning and length. Placeholders identical across languages.
2. **Design match for Equalizer APO.** `ApplyStatus` now carries
   `current_design_id: string | null` (the current result's design ID). Make
   the Equalizer APO status compare `eqapo.applied_design_id` with it, so
   the row shows 이 설계 / 다른 설계 / 없음 without needing an apply in this
   session; keep the session-local fallback only when `current_design_id` is
   null. Add the field to `ipc/fixtures.ts`, `screens/export/fixtures.ts`
   and `ipc/mock.ts` (the mock returns `MOCK_DESIGN_ID` once a result
   exists), and update the specimens that show the Equalizer APO design so
   they exercise this design, another design and unknown through the field.
   Adjust the tests that cover the comparison.
3. **Records.** Add the new `data-ui` ids and specimen ids from P121
   (`export.apply.*`, `export.run.camilla_dsp` at 적용, `export.status.problem`,
   the 21 `screen.export.apply.*` specimens) to `docs/DESIGN.md` §11 and §12
   and `docs/M5-reach.md` in their existing formats. Add one `CHANGELOG.md`
   sentence under the next version for the screen (the backend entry from
   P119 exists; merge them into one item if that reads better).

## Files

You may edit: `apps/secs2r/ui/src/i18n/ko.json`, `en.json`,
`apps/secs2r/ui/src/screens/export/types.ts` (the comparison helper only),
`screens/export/fixtures.ts`, `screens/export/specimens.tsx` (fixture
choice only), `screens/export/*.test.ts(x)`, `ipc/fixtures.ts`,
`ipc/mock.ts`, `ipc/mockWiring.test.ts`, `docs/DESIGN.md`,
`docs/M5-reach.md`, `CHANGELOG.md`.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
```

## Report

Each key's final ko and en text, the comparison change, files changed, and
the commands' last lines.
