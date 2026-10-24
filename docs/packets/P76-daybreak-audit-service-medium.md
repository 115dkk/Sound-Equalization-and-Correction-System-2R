# P76 (Daybreak Blue): audit #92 — service Medium findings (S2)

Repository: `/home/user/Sound-Equalization-and-Correction-System-2R`. Work in
the main tree: `git switch -c wip/s2` from the current `HEAD`. Do not push,
do not switch branches again, do not spawn sub-agents. Commit once per
finding (subjects below); Claude reviews, re-runs the gates and re-authors
the commits.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0009-compact-result-ipc-and-job-retention.md`,
`TECH_DEBT_AUDIT.md` (rows TD-32 to TD-41), `crates/secs-service/src/**`,
`crates/secs-service/tests/**`, `apps/secs2r-cli/src/main.rs` and
`tests/cli.rs`, `crates/secs-engine/src/mimo/run.rs` (`cap_text`),
`src/secs/calibration.py:996-1002, 1135-1140, 1355-1362` (upstream cap text).

Method: test first for A items (red at the named seam, confirm the reason,
minimum green, crate tests). Expected values come from the scenario or
upstream, not from the code under test. For TD-40 (tests that check the
wrong thing) the red step is: break the behaviour on purpose (locally, not
committed), show the old test still passes and the new one fails, restore.

## Findings, in order

1. **TD-32** — `export.rs:135-139` formats the MIMO cap as `"max N"` from the
   raw setting. Use `secs_engine::mimo::cap_text(run.key.max_supports)` (the
   clamped value; check the field name on `MimoRun`). Seam: live staged mdat
   export with `mimo_max_supports = 4` → the note text contains
   `" · 입력당 서포트 ≤ 4"` exactly as upstream (`calibration.py:1000-1002`)
   and no `"max"`. Commit:
   `fix(audit): A-32 MIMO 상한 문구를 업스트림과 같게 씁니다 (#92 TD-32)`
2. **TD-33** — `curves.rs::fresh_mimo` ignores the compute key that
   `summaries()` and export check. Pass the current `ComputeIdentity` and
   compare `result.compute_key`. Seam: calibrate + MIMO → `AppSettingsSet`
   changing the compute preference → `ResultCurves` has no MIMO curve and
   `ResultLines` no support counts, like `Results.mimo.stale == true`.
   Commit: `fix(audit): A-33 결과 그래프도 계산 환경이 바뀐 MIMO를 낡음으로 봅니다 (#92 TD-33)`
3. **TD-34** — `monitor.rs:49` maps every native monitor error to
   `DeviceUnavailable`. Use `measurement::measure_error` with the real share
   setting, as `level_check` does. Seam: native/mock monitor with an unknown
   role → the same error code and retryable flag as `LevelCheck` for that
   role. Commit: `fix(audit): A-34 레벨 모니터 오류를 레벨 확인과 같게 분류합니다 (#92 TD-34)`
4. **TD-35** — `measure/rew.rs:103` sets `peak_dbfs` from `input_dbfs`. Use
   the REW level's peak field. Seam: recorded REW level response with RMS
   −30 dBFS and peak −1 dBFS → `LevelCheck` reports −1 as peak. Commit:
   `fix(audit): A-35 REW 레벨의 피크 값을 그대로 전합니다 (#92 TD-35)`
5. **TD-36** — CLI `import-mdat --position N`: scan rows carry a suggested
   `position` that overrides `params.position`. For rows whose title has no
   position tag (`position_tagged == false` in the scan result), send
   `position: None` so the CLI's `--position` applies. Seam:
   `tests/cli.rs` with an mdat whose titles have no position tag and
   `--position 3` → the project records position 3. Commit:
   `fix(audit): A-36 CLI import-mdat의 --position이 적용됩니다 (#92 TD-36)`
6. **TD-37** — `calibration.rs:279` (`prepare`) loads every WAV under the
   session lock before the job is registered (no job id, not cancellable,
   other requests wait, `JobBusy` only after loading). mdat/Dirac import
   re-parses under the lock after `begin_commit` (`measurement.rs:1118`).
   Design (fixed): (a) check the job group is free first (expose the existing
   busy check from `jobs.rs` without starting, or start the job and do the
   loading inside its body); (b) under the lock take a snapshot of what the
   load needs (project clone, IR paths), release the lock, load inside the
   job body (cancellable); (c) re-check the project identity when
   committing (already done). For import: parse and classify inside the job
   body before `begin_commit`, and lock only for the record loop. Seam:
   while a calibration is running, a second `Calibrate` returns `JobBusy`
   without reading any WAV (count reads with a test measurement store or
   time it against a large project); `ProjectInfo` answers while a
   calibration's loading is in progress (pausing hook). Commit:
   `fix(audit): A-37 무거운 파일 읽기를 세션 락 밖, 취소할 수 있는 잡 안으로 옮깁니다 (#92 TD-37)`
7. **TD-38** — `calibration.rs:135`: a cache save failure after the results
   were committed in memory fails the job. Make it a warning: the job
   finishes successfully and the summary carries a `Notice` with a new code
   `cache_save_failed` (params: the error text). Add the key to
   `apps/secs2r/ui/src/i18n/{ko,en}.json` in the existing notice family
   (ko: "결과를 프로젝트 폴더에 저장하지 못했습니다. 다시 열면 다시 계산해야 합니다.",
   en: "The results could not be saved in the project folder. They will need to be recalculated after reopening.").
   Same for MIMO. Seam: read-only `.irs` (unix) → `Finished` ok, results
   present, notice present. Commit:
   `fix(audit): A-38 캐시 저장 실패를 계산 실패가 아닌 경고로 알립니다 (#92 TD-38)`
8. **TD-39** — `project_edit.rs:799`: app-settings file write and in-memory
   update are not one critical section (Tauri runs requests in parallel), and
   saves are unbounded while reads are capped at 1 MiB (`:831`). Hold one
   mutex across write + memory update; reject a serialized size over the
   read cap with `InvalidRequest`. Seam: two threads issuing
   `AppSettingsSet` with different values many times → after both finish,
   `AppSettingsGet` equals what a fresh `Service` reads from disk; an
   oversized `recent_projects` entry → `InvalidRequest`. Commit:
   `fix(audit): A-39 앱 설정 저장을 직렬화하고 크기를 제한합니다 (#92 TD-39)`
9. **TD-40** — three tests do not check what their names say:
   `tests/measure_ipc.rs:1170` (`session_free` only sees an HTTP request),
   `tests/service.rs:1322` (`continue`s on success),
   `crates/secs-project/tests/record.rs:446` (F1 test fails a different
   target). Rewrite each to assert its name: block the transport while
   another thread completes `ProjectInfo`; per request the exact expected
   code or success shape; inject the failure on the same `project.secs` and
   check the old bytes and no temp file. Commit:
   `fix(audit): A-40 이름대로 확인하지 않던 테스트 셋을 고칩니다 (#92 TD-40)`
10. **TD-41** — `features.toml:1030-1108`: `ipc.project_new`,
    `project_open`, `project_info`, `calibrate`, `recommend_xo`, `mimo`,
    `results`, `job_cancel`, `jobs`, `rew_launch` cite only the method-name
    round trip and the malformed-request test. Point each at existing
    behaviour tests (grep `tests/*.rs` for each `Request::…`); add a small
    success test only where none exists. Gate:
    `python3 .github/scripts/check_feature_tests.py`. Commit:
    `chore(audit): C-1 기능 등록부의 IPC 근거를 동작 테스트로 잇습니다 (#92 TD-41)`
11. **TD-90** — a tutorial project is created with `path: ""` (the screen
    sends it, `apps/secs2r/ui/src/App.tsx:196`), so measured IR WAVs and
    `secs_run_cache.json` land in the app's current directory
    (`cache.rs:258-260`, `secs-project/src/irs.rs:8-11`). In
    `session.rs::new`, when `tutorial` is set, ignore the given path and use
    a private directory `std::env::temp_dir()/secs2r-tutorial-<pid>-<n>/`
    (`n` from a process-wide counter) with `tutorial.secs` inside; hold it in
    `OpenProject` through a small guard type whose `Drop` removes the
    directory (best effort, error ignored), so closing, replacing or opening
    another project cleans it up. Fix the `ProjectNewParams.tutorial` doc in
    `secs-types/src/ipc.rs:622` to the real contract (the session cannot be
    saved or exported; measurements and results live in a temporary folder
    removed when the session ends) and regenerate bindings. Test
    (`tests/`): a tutorial project that loads samples and calibrates writes
    nothing to a fresh current directory (set a temp CWD in a dedicated
    test binary or assert on the paths the session reports), its folder
    exists while open and is gone after `project_close`. Commit:
    `fix(audit): A-81 튜토리얼이 작업 폴더가 아니라 임시 폴더에 씁니다 (#92 TD-90)`
12. **TD-91** — `measurement.rs::rew_driver_set` (`:902`) and the REW
    device switch inside `measure_configure` (`:380-400`) change REW's
    driver and devices even while a measurement-group job sweeps through
    REW. Reject both with `ErrorCode::JobBusy` (same shape as `jobs.rs:185`)
    while `jobs.running_in_device_group()` is true for a job other than the
    level monitor (the screen restarts the monitor itself; add a small
    `JobRegistry` query that excludes `JobKind::LevelMonitor` rather than
    changing the existing one). Test: start a blocked fake measurement, call
    each method → `JobBusy`; with only a level monitor running → accepted.
    Commit:
    `fix(audit): A-82 측정 중에는 REW 드라이버와 장치를 바꾸지 않습니다 (#92 TD-91)`

## Allowed files

`crates/secs-service/**`, `apps/secs2r-cli/**`, `crates/secs-project/tests/record.rs`
(TD-40 only), `crates/secs-types/src/ipc.rs` (only if a notice/doc change is
needed; then regenerate bindings), `apps/secs2r/ui/src/i18n/{ko,en}.json`
(TD-38 key), `features.toml`, `docs/DEVIATIONS.md`, `Cargo.lock`.

## Gates (foreground; do not end the turn before they finish)

After each finding:

```sh
cargo fmt --all -- --check
cargo clippy -p secs-service -p secs2r-cli --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs2r-cli --locked
```

At the end: `cargo test -p secs-types -p secs-project --locked`,
`python3 .github/scripts/check_feature_tests.py`, and if you touched the
catalogs `cd apps/secs2r/ui && PATH=/opt/node24/bin:$PATH npm test -- i18n`.

## Report (final answer)

Per finding: commit hash, files, test names, red failure message, green
result. Then the last gate output tail and anything left undone.
