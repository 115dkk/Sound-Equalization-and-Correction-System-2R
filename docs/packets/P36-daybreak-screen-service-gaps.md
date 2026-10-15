# P36 (Daybreak): what the screens need from the service

Worktree: `/home/user/wt/p36` (branch `m5/p36`). Do not commit; Claude
reviews the diff and commits. Another worker runs P37 in parallel
(`/home/user/wt/p37`): it owns the settings edit rules, sub warnings, REW
facts and the mdat position tag, so do not touch those.

The step screens (P30–P32, `apps/secs2r/ui/src/screens/`) are drawn against
the generated IPC types. Their reports list facts the service does not give
yet. This packet adds them, each in the service where upstream keeps the
rule, so the screens only draw. Read `CLAUDE.md`, `docs/M5.md` §3,
`docs/ARCHITECTURE.md` (service), `docs/adr/0003-stack-and-ipc.md`
(codes, not sentences), `docs/DEVIATIONS.md`, and the screen types that will
consume each item (`src/screens/*/types.ts`). Upstream is `src/secs/` in the
same tree (read-only).

## Items

### Measurement

1. **Position records.** `measurement_records` returns, per arranged
   position, `{position, done, roles, sources}`: the roles recorded, whether
   every planned role is recorded, and the source file per role (upstream
   `measurements[*].source[role]`, "measured" for sweeps). Screen:
   `src/screens/measure/types.ts` (`PositionRecord`).
2. **Repetitions.** `MeasureConfigResult.repetitions` and
   `MeasureConfigureParams.repetitions` (1, 2, 4, 8), stored where upstream
   stores it (`step5_measure.py:932-934`, `_store_measure_option`), forced to
   1 with an acoustic timing reference (upstream note "음향 타이밍
   레퍼런스는 반복 1"). The measurement session already repeats; check it
   reads this value.
3. **Sweep lengths and rate.** `MeasureConfigResult.sample_rate` and
   `sweep_lengths` (the 64k…4M list upstream offers,
   `step1_devices.py:64-78`), so the screen shows "256k · 5.5 s" without
   knowing the list.
4. **Start delay countdown.** The measure job runs the start delay
   (`extra['start_delay']`) itself: before the first sweep it emits
   `Progress { stage: "countdown", done: seconds_passed, total: delay }` once
   a second; cancelling during the countdown ends the job before any sweep.
5. **No method chosen, and mdat only.** `measure_configure` accepts
   `method: "none"` and `"mdat"`; `MeasureConfigResult.method` reports them;
   a new project starts at `"none"`; measuring with `none` or `mdat` fails
   with `measurement_input` and `field: method`. Keep reading old projects
   that have no method as today.
6. **Timing reference role.** `layout_get` resolves an unset
   `timing.role` to upstream `default_ref_role` (R, else the first main)
   instead of `""`.
7. **Levels.** `levels_get` returns the stored `extra['levels']` and
   `extra['spl']` per role; `levels_fill_defaults` fills only the missing
   roles with upstream's −22 − (len(role)·7 mod 6) dBFS
   (`step3_level.py:410-425`) and marks the project dirty; `level_stop`
   ends a 2.5 s burst early (the monitor keeps running).
8. **REW launch errors.** `rew_launch` separates its failures with a
   `reason` param: `exe_not_found`, `cannot_launch`, `launch_failed`. It
   returns right after the process starts; the status watch (P28) reports
   when the API answers, and the screen decides "timed out" after 40 s.

### Export

9. **Destination.** `ExportResult.folder`: the folder the job wrote.
10. **Folder names as upstream.** CamillaDSP goes to
    `<base>/SECS_camilla_<RC_BA|RC_BA_MIMO>_<mode>_<YYYYmmdd-HHMMSS>/camilladsp`
    and before/after mdat to `<base>/SECS_mdat_…` (`step8_mdat.py:200-230`).
    Check the staged mdat card (`step_calibration.py:2404-2473`) and use its
    naming for `staged_mdat`.
11. **MIMO inclusion.** Include MIMO only when `mimo_on`, a current MIMO
    result exists and the positions allow it (upstream `show_mimo`,
    `step_calibration.py:2126-2129`, `cal.mimo_availability`).
12. **Stale results export as upstream.** Upstream exports a stale result
    and shows a note (`step8_mdat.py:128-135` checks only "computing" and
    "no result"). Stop refusing with `stale_inputs`; add
    `ExportResult.stale`. This removes an undocumented difference from
    upstream.
13. **Open folder.** `open_folder` creates a missing folder first, as
    upstream's `mkdir` before opening.
14. **Tutorial projects.** `project_new { tutorial: true }` (with the sample
    room) marks the session as a tutorial (`ProjectInfo.tutorial`); in it
    `project_save` and `export` fail with a new `ErrorCode::TutorialBlocked`
    and write nothing (upstream `trial`, `step8_mdat.py:203-205`).

### Results

15. **Mode of the run.** `CalibrationSummary.mode` and `ultra_ms`: the mode
    the result was computed with (also for a restored cache).
16. **Why stale.** `CalibrationSummary.stale_reason`: `measurements`,
    `settings` or `target`, from the parts of the signature that differ.
17. **MIMO FIR total.** `MimoSummary.fir_taps_total` for the budget line.
18. **Group ids.** `ConfigGroup` gains `id` (`front`, `center`, `sub`,
    `surround`, `back`, `wide`, `top`, `height`, `other`); `key` stays the
    upstream group name the project and the `xo` maps use, and its doc says
    so.
19. **Notes as codes.** Engine notes whose params are JSON
    (`phase_strength`, `phase_band`, `sub_filters`, `ba_cost`,
    `ba_bus_pass`) carry flat scalar params (`<name>` or `<name>.<i>`), and
    the MIMO note's upstream free text becomes a code with params. Document
    each code's params in the type docs.

## Rules

- IPC types only in `secs-types`; regenerate the bindings
  (`cargo test -p secs-types` from the repository root) and keep the UI
  typecheck green: update `src/ipc/mock.ts` and `src/ipc/fixtures.ts` for new
  required fields, nothing else under `apps/secs2r/ui/src/`.
- Register every new method as `ipc.<method>` in `features.toml` with a
  real test; new behaviour gets a test in `crates/secs-service/tests/`.
- Items 10–12 change export behaviour toward upstream; item 14 and the
  `none`/`mdat` methods are new (record them in `docs/DEVIATIONS.md` as U
  rows only if upstream behaves differently, with the reason).
- Byte comparisons that exist today (export goldens, integrity) must still
  pass; the integrity flow passes an explicit folder.

## Files you may change

`crates/secs-types/src/**`, `crates/secs-service/**`,
`crates/secs-project/**` (only if a rule lives there), `apps/secs2r-cli/**`
(if a changed type needs it), `apps/secs2r/ui/src/ipc/bindings/**`
(generated), `apps/secs2r/ui/src/ipc/{mock,fixtures}.ts`, `features.toml`,
`docs/DEVIATIONS.md`.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p36
export CARGO_TARGET_DIR=/home/user/target-p36 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --locked -p secs-types -p secs-service -p secs-project -p secs2r-cli -p secs-policy
python3 .github/scripts/upstream.py check-tree
(cd apps/secs2r/ui && PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH npm ci && npm run typecheck && npm test)
```

Run cargo from `/home/user/wt/p36` (the bindings land relative to the
directory cargo runs in).

## Report

Per item: what changed (types, methods, files), the test that covers it,
and anything you could not do or decided differently, with the reason.
