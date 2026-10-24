# P72 (Daybreak Blue): audit #92 — Save As, REW level monitor, cached exports, CLI (S1b)

Repository: `/home/user/Sound-Equalization-and-Correction-System-2R`. Work in
the main tree: `git switch -c wip/s1b` from the current `HEAD` (it already
contains P71's commits). Do not push, do not switch branches again, do not
spawn sub-agents. Commit once per finding (subjects below); Claude reviews,
re-runs the gates and re-authors the commits.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0003-stack-and-ipc.md`,
`docs/adr/0009-compact-result-ipc-and-job-retention.md`, `docs/DEVIATIONS.md`,
`TECH_DEBT_AUDIT.md` (rows TD-03 to TD-09, TD-12, TD-13),
`crates/secs-service/src/{session.rs,monitor.rs,export.rs,measurement.rs,calibration.rs,cache.rs,jobs.rs}`,
`crates/secs-types/src/ipc.rs` (`CalibrationSummary`, `ExportBlock`,
`filter_export_block`), `apps/secs2r-cli/src/main.rs`, and the tests in
`crates/secs-service/tests/{service.rs,monitor.rs,measure_ipc.rs}` and
`apps/secs2r-cli/tests/cli.rs`.

Method: test first for every finding. Red at the named seam (confirm it fails
for the finding's reason), minimum green, then the crate tests. Expected
values come from the scenario, not from re-running the code under test.

## Findings, in order

### 1. TD-03 — Save As keeps the cached results

`session.rs:143` sets `open.cached = None` after copying the `.irs` folder
(which carries `secs_run_cache.json`). A reopened project whose results come
only from the cache loses them on Save As; export then fails.

Fix: after the new path is in place, `open.cached = crate::cache::load(&open.path)`
(the copied file), falling back to keeping the previous value if the load
returns `None`.

Seam: `Service::call`: calibrate, close, reopen (cached only), `ProjectSave`
to a new path → `Results.calibration` is `Some` and an export succeeds.

Commit: `fix(audit): A-5 다른 이름으로 저장한 뒤에도 캐시 결과를 유지합니다 (#92 TD-03)`

### 2. TD-04 — Save As stages, then swaps

Three failure modes in `Session::save` (`session.rs:126-150`, `copy_tree`
`:227-239`):
1. the session switches path and store before the `.secs` write succeeds;
2. a destination `.irs` inside the source `.irs` recurses forever while the
   session lock is held;
3. saving over an existing project overwrites its IRs file by file.

Design (fixed): for a path change,
- canonicalize the source `.irs` and the destination's parent; reject with a
  `ServiceError`/`IpcError` (`InvalidRequest`, message "cannot save a project
  inside its own measurement folder") when the destination `.irs` is the
  source or lies inside it;
- copy the source `.irs` into a sibling temporary directory of the
  destination `.irs` (`.{name}.irs.tmp-{pid}-{n}`), then write the new
  `.secs` with a new `MeasurementStore` (rewritten IR paths) — the open
  session is not touched yet;
- on success: if a destination `.irs` exists, rename it aside
  (`.{name}.irs.old-…`), rename the temporary directory into place, remove
  the aside directory; then update `open.path`, `open.store` and (finding 1)
  `open.cached`;
- on any failure: remove the temporary directory, leave the open session and
  any existing destination untouched, return the error.
Keep `write_side_mdat` after the swap as today. Same-path saves are
unchanged.

Seam: `Service::call` `ProjectSave` with (a) a destination whose `.secs`
cannot be written (make the destination a directory named `x.secs`, or a
read-only parent on unix) → error, `ProjectInfo.path` is still the old path;
(b) destination `old.irs/inner.secs` → `InvalidRequest`, returns quickly, no
new directories; (c) an existing destination project with different IR bytes
and a forced failure → its `.secs` and IR bytes unchanged.

Commit: `fix(audit): A-6 다른 이름으로 저장을 준비한 뒤 바꿉니다 (#92 TD-04)`

### 3. TD-05 — REW level play ends by itself

`monitor.rs:341-349`: while a play request is active its generation is not
`handled_generation`, so every 100 ms loop calls `play_pink` again and resets
the start instant; the 2.5 s auto-stop never fires.

Fix: start a request only when it is not the active one
(`active.as_ref().is_none_or(|(play, ..)| play.generation != requested.generation)`).

Seam: `tests/monitor.rs` with the recorded REW transport: one level play →
within ~3 s a `level_play_end` "done" event, exactly one Play request and one
generator stop in the recorded exchanges.

Commit: `fix(audit): A-7 REW 레벨 재생이 스스로 끝나게 합니다 (#92 TD-05)`

### 4. TD-06 — REW monitor cleans up on errors

`?` inside the REW loop (`monitor.rs:345` and nearby) skips
`stop_generator`/`stop` at the end (`:376-380`); `RewClient` does not send
Stop on drop.

Fix: run the loop body in a closure/inner function and always perform the
cleanup afterwards, returning the body's result.

Seam: recorded transport that fails the request after Play → the job
finishes with the error and the recorded exchanges include the generator
stop and the unsubscribe/stop.

Commit: `fix(audit): A-8 REW 모니터가 오류에도 신호 발생기를 끕니다 (#92 TD-06)`

### 5. TD-07 — the level monitor writes only into the project it started with

`store_level` (`monitor.rs:384-397`) writes into whatever project is open.

Fix: capture the session identity when the monitor starts and pass it to
`store_level`; if it differs, skip the write (no error, the monitor keeps
running for the device) — the old project's level must not land in the new
one.

Seam: `Service::call`: start the level monitor (mock or native-sim method as
existing monitor tests do), open another project, finish a level burst →
the new project's `levels`/`spl` unchanged and not dirty.

Commit: `fix(audit): A-9 레벨 모니터가 시작한 프로젝트에만 씁니다 (#92 TD-07)`

### 6. TD-08 — cached mdat export uses the project it was requested for

`export.rs:266-313`: the export job re-locks the session and designs from the
project open at that moment, while labels, SPL offset, sources and folder come
from the request time.

Fix: capture the project identity when the export is requested; in the job,
before the redesign, `require_identity`-style check → `StaleInputs` with
"project changed while exporting" if it changed.

Seam: pausing event sink (as in
`project_replacement_rejects_old_compute_commit_and_preserves_completion`):
request a cached mdat export, open another project while paused → the job
fails with `StaleInputs`; no files in the export folder.

Commit: `fix(audit): A-10 캐시 결과의 mdat 내보내기가 요청한 프로젝트로만 설계합니다 (#92 TD-08)`

### 7. TD-09 — reopened projects: mdat carries cached MIMO; stale cache blocks mdat

Maintainer decision (2026-10-24): when the calibration comes from the cache
(no live run in this session):
- cache **fresh** → redesign as today (deterministic, same result) and apply
  the cached MIMO paths to the mdat responses, so staged/before-after mdat
  carries the same MIMO as CamillaDSP;
- cache **stale** → mdat export is blocked; the screen shows "구 버전에서
  측정한 값입니다. mdat으로 내보내려면 다시 보정해야 합니다." with a
  [재보정] button (packet U1 draws it). Other formats keep today's behaviour
  (allowed, `stale` flag).

Design (fixed, IPC types change — regenerate bindings with
`cargo test -p secs-types` and commit them in this commit):
- `CalibrationSummary` gets `#[serde(default)] pub cached: bool` — "Whether
  this result was restored from the project cache instead of computed in this
  session; staged and before/after mdat need a live design." `summaries()`
  sets it for the cached branch.
- `ExportBlock` gets a variant `StaleCache` (doc: "The result was restored
  from the cache and is stale; mdat needs the measurements' spectra, so it
  must be recalibrated first").
- `filter_export_block(kind, has_subs, has_mimo)` stays for the filter
  formats. Add `pub fn export_block(kind: ExportKind, has_subs: bool, has_mimo: bool, stale_cache: bool) -> Option<ExportBlock>`
  that returns `Some(StaleCache)` for `StagedMdat`/`BeforeAfterMdat` when
  `stale_cache`, otherwise `filter_export_block(..)`. The service's
  `start_export` and the CLI use `export_block`.
- Service: in `start_export`, when there is no live calibration, the cached
  one is stale and the kind is mdat → return `IpcError` with code
  `StaleInputs` and message "cached result is stale; recalibrate before mdat
  export" (defence in depth; the screen blocks first). When fresh, build the
  `MimoResponses` from the cached MIMO paths after the redesign (the same
  eligibility as the live path, including the P71 run key).
- Look for the exhaustive `ExportBlock` match in the UI
  (`apps/secs2r/ui/src/screens/export/ExportScreen.tsx`, `types.ts`) — the
  TypeScript build must still compile: add the variant to the TS mirror of
  the rule (`exportBlock` in `screens/export/types.ts`) returning
  `"stale_cache"` for mdat kinds when `view.calibration.cached && stale`, and
  map it to a placeholder text key `export.block.stale_cache` in `ko.json`
  and `en.json` with the exact Korean text above and the English "These
  values were measured with an earlier version. Recalibrate to export mdat."
  Do not add the button or change markup — packet U1 does that.

Seam: `Service::call`: (a) calibrate + MIMO, close, reopen (fresh cache) →
`StagedMdat` export contains the MIMO stage files/curves exactly as the
pre-close export did (compare to the existing
`exports_after_reopening_match_pre_close_bytes` pattern); (b) reopen, change
a setting so the cache is stale → `StagedMdat` returns `StaleInputs`, and
`Results.calibration.cached == true`; CamillaDSP still exports with
`stale: true`. Unit test for `export_block`.

Commit: `fix(audit): A-11 다시 연 프로젝트의 mdat에 캐시 MIMO를 넣고 낡은 캐시는 막습니다 (#92 TD-09)`

### 8. TD-12 — CLI `mimo` works in a fresh process

`apps/secs2r-cli/src/main.rs:446`: `mimo <project>` opens the project and asks
for MIMO; a fresh process has no live calibration, so it always fails.

Fix: the `mimo` action runs `runner.calibrate(None)` first (as `calibrate`
does) and then MIMO. Update the CLI help text for the command to say it
calibrates first.

Seam: `apps/secs2r-cli/tests/cli.rs`: run `calibrate`, then `mimo` in a new
process on the same project → success JSON with a MIMO summary.

Commit: `fix(audit): A-12 CLI mimo가 새 프로세스에서도 동작합니다 (#92 TD-12)`

### 9. TD-13 — `sample_load.positions` has an upper bound

`measurement.rs:526`: only a lower bound. Add an upper bound of 17 (the
largest layout, 패밀리) and reject larger values with `InvalidRequest` before
allocating. Update the `SampleLoadParams` doc in `secs-types` to state the
range and regenerate bindings in this commit.

Seam: `Service::call` `SampleLoad` with 17 (ok), 18 and `u32::MAX`
(`InvalidRequest`, returns immediately).

Commit: `fix(audit): A-13 sample_load 위치 수에 상한을 둡니다 (#92 TD-13)`

## Allowed files

- `crates/secs-service/**`, `crates/secs-types/src/ipc.rs` and regenerated
  `apps/secs2r/ui/src/ipc/bindings/**`
- `apps/secs2r-cli/**`
- `apps/secs2r/ui/src/screens/export/types.ts`, `ExportScreen.tsx` (only the
  exhaustive-match line needed to compile), `apps/secs2r/ui/src/i18n/{ko,en}.json`
  (one key), and the UI tests that mirror `exportBlock`
- `docs/DEVIATIONS.md` (if an entry describes Save As, cached exports or the
  CLI), `features.toml` (point `ipc.export`/CLI entries at new tests if they
  name the behaviour)
- `Cargo.lock` as cargo changes it

## Gates (foreground; do not end the turn before they finish)

After each finding:

```sh
cargo fmt --all -- --check
cargo clippy -p secs-service -p secs-types -p secs2r-cli --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-types -p secs2r-cli --locked
```

After findings 7 and 9 also:

```sh
cd apps/secs2r/ui && PATH=/opt/node24/bin:$PATH npm run typecheck && PATH=/opt/node24/bin:$PATH npm test
```

At the end: `python3 .github/scripts/check_feature_tests.py`.

## Report (final answer)

Per finding: commit hash, files, test names, the red failure message, green
result. Then the last gate output tail, anything left undone, and any test you
changed because it asserted the old behaviour (with the reason).
