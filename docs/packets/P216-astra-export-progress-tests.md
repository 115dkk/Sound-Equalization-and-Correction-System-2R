# P216 (ASTRA): tests and the mock backend for the export's real progress

Worktree `E:\wt\export-progress`, branch `feat/export-progress`, folder
`E:\wt\export-progress\apps\secs2r\ui`. `node_modules` is installed (Node 24).
No Rust build is needed and none may be started: another worker is changing
the Rust crates in this worktree at the same time, so do not touch anything
outside `apps\secs2r\ui`, and inside it only the files listed below. Do not
commit, do not push, do not run `git add`, `git stash`, `git checkout` or
`git reset`. Run every command in the foreground and end your turn only
after the report.

## What changed (read these first; they are the specification)

The maintainer asked (2026-11-11) whether the bar shown during an export is
the real progress of the save. It was not: the service reported nothing
while files were computed and written, and the screen animated an
indeterminate bar. The service is being changed (packet
`docs\packets\P215-daybreak-export-progress.md`, section 'The contract') so
that an `export` or `apply` job reports, in order:

1. the contents, counted in items: stage `export.prepare` (FIR coefficient
   files, or the measurements of a REW mdat), or for the convolution export
   stage `filters.convolution` (pairs rendered and encoded, `rates * 2 * P`);
   a design with no FIR reports no such stage;
2. the files, counted in bytes: stage `export.write`.

Each stage starts at `(0, total)`, ends at `(total, total)`, never goes back,
and `total` is never 0. Stage `export.camilla.prepare` no longer exists.

The main session already changed the screen for it:

- `src\app\jobs\jobText.ts`: `progressText(t, stage, done, total)` and
  `sizeProgressText(done, total)`. Stage `export.write` is shown as sizes
  (`50.0 / 125.5 MB`; KB without decimals, GB with two, plain `B` under
  1 KB; the unit follows the total); every other stage as `jobs.progress`
  (`41/117`).
- `src\app\jobs\JobTray.tsx` (`JobRow`): the bar and the count are drawn
  only when the job has a total (`job.total > 0`); a job with no report has
  an empty `job-row__progress` cell and no `progressbar` at all. The
  indeterminate bar is gone from the tray.
- `src\screens\export\types.ts`: `ExportView.progress: ExportProgress | null`.
- `src\screens\export\useExportScreen.ts`: `progress` is the running
  `export` job's last report, or the running `apply` job's; null when the
  total is 0 or nothing was reported.
- `src\screens\export\ExportScreen.tsx`: `RunProgress` in `ExportItem`,
  `data-ui="export.kind.<kind>.progress"`, shown while that kind exports or
  while its apply target applies, above the outcomes. With a report: the
  stage's words (`stageText`, else `jobs.status.running`), a `progressbar`
  named by those words with `aria-valuenow`/`aria-valuemax` and the count as
  `aria-valuetext`, and the count as text. Without one: only
  `export.progress.starting`, no `progressbar`.
- Catalog keys added in `ko.json` and `en.json`: `jobs.stage.export.prepare`,
  `jobs.stage.export.write`, `jobs.stage.filters.convolution`,
  `export.progress.starting`; `jobs.stage.export.camilla.prepare` removed.
- Fixtures in `src\screens\export\fixtures.ts`: `exportExportingPrepare`,
  `exportExportingWrite`, `exportConvolutionRendering`.

## Your work

### 1. The mock backend plays the same stages

`src\ipc\mock.ts` plays an `export` or `apply` job with
`stages: [{ stage: "design", total: 1 }]` (near `case "export": case "apply":`).
Make it play what the service's contract says for the kind being exported,
with fixed, plausible totals (no randomness, so tests stay deterministic):

- `camilla_dsp`, `equalizer_apo`, `filter_text`: `export.prepare` with the
  number of FIR files a design of the mock's result would have (pick one
  constant, at least 2), then `export.write` in bytes;
- `convolution`: `filters.convolution`, then `export.write`;
- `before_after_mdat`, `staged_mdat`: `export.prepare` in measurements, then
  `export.write`;
- `apply_camilla`, `apply_eqapo`: the stages of the kind they export.

Read how the mock advances a job through its stages before you change it;
if the job script cannot carry a per-kind choice or byte-sized totals
without a larger change, make the smallest change that can, and say what
you changed. Update every test that depended on the old `design` stage of a
mock export so it proves the same thing with the new stages.

### 2. Tests of what the main session changed

- `src\app\jobs\jobText.test.ts`: `sizeProgressText` at the unit borders
  (under 1 KB, exactly 1 KB, under and at 1 MB, under and at 1 GB, done
  equal to total), and `progressText` choosing sizes only for
  `export.write`, in both languages.
- `src\app\jobs\JobTray.test.tsx`: a running job with no report has no
  `progressbar` and no count but keeps its name, spinner, elapsed time and
  cancel; a job with a count stage shows the bar with the right
  `aria-valuenow`, `aria-valuemax`, `aria-valuetext` and the count text; an
  `export.write` report shows sizes in both places; no element carries the
  class `ui-progress--indeterminate`.
- `src\screens\export\useExportScreen.test.tsx`: `view.progress` follows the
  running export job's reports through `export.prepare` and `export.write`,
  is null before the first report and after the job ends, and follows an
  apply job the same way.
- `src\screens\export\ExportScreen.test.tsx`: `RunProgress` appears only in
  the item whose kind is exporting (and for an apply, in the item of that
  target), not in the other items of the tab; the starting line without a
  `progressbar`; a count stage and a byte stage with their words, values
  and `aria-valuetext` in both languages; a stage id without a catalog key
  falls back to `jobs.status.running`; the line is gone when nothing runs.

### 3. The four new catalog sentences

Read the Korean and English values of the four new keys in their context
(tray row and panel line). They are labels: noun phrases, no sentence
ending. If one is unnatural or inconsistent with its neighbours in the
catalog, change its value in both catalogs as needed and say why in the
report. Do not rename keys.

## Files you may edit

- `src\ipc\mock.ts` (only the job script of `export` and `apply`), and its
  tests under `src\ipc\`
- `src\app\jobs\jobText.test.ts`, `src\app\jobs\JobTray.test.tsx`
- `src\screens\export\ExportScreen.test.tsx`,
  `src\screens\export\useExportScreen.test.tsx`
- any other existing test file that fails only because the mock's export
  stages changed (list each in the report)
- `src\i18n\ko.json`, `src\i18n\en.json`: the values of the four keys only

Do not edit the screen or its styles (`ExportScreen.tsx`, `JobTray.tsx`,
`jobText.ts`, `types.ts`, `useExportScreen.ts`, `fixtures.ts`,
`specimens.tsx`, any `.css`). If a test can only pass by changing one of
them, leave the test failing and report what you saw: a defect found there
is the result I want, not something to work around.

## Checks to run (foreground) and report

```
cd E:\wt\export-progress\apps\secs2r\ui
npm run typecheck
npm test
npm run build
```

## Report

- Each file changed, with one line on what changed.
- The stages the mock now plays for each export kind, with their totals.
- Any defect or doubt about the screen code, with the test that shows it.
- The output tail of each check.
